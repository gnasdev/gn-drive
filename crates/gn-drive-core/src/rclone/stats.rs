//! rclone log-line stats parsing (port of `StatsParser.swift` / `internal/rclone/stats.go`).
//!
//! Pure functions over canned rclone lines. JSON decoding is deliberately
//! tolerant (missing keys / nulls fall back to defaults), as in the Go parser.

use super::types::{FileEntry, FileTransfer, SyncStats};
use serde_json::Value;
use std::collections::HashMap;

/// Maximum number of per-file rows kept by [`FileTransferTracker`].
pub const MAX_TRACKED_FILES: usize = 150;

/// Current unix time in seconds.
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Truncate to `n` characters, appending `...(truncated)` when cut.
pub fn truncate_string(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let cut: String = s.chars().take(n).collect();
        format!("{cut}...(truncated)")
    }
}

// ---- JSON helpers --------------------------------------------------------

fn get_i64(v: &Value, key: &str) -> i64 {
    match v.get(key) {
        Some(x) => x
            .as_i64()
            .or_else(|| x.as_f64().map(|f| f as i64))
            .unwrap_or(0),
        None => 0,
    }
}

fn get_f64(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn get_str(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Decoded `--use-json-log` line (only the fields we use).
struct JsonLogLine {
    level: String,
    msg: String,
    object: String,
    size: i64,
    stats: Option<Value>,
}

fn decode_log_line(line: &str) -> Option<JsonLogLine> {
    let t = line.trim();
    if !t.starts_with('{') {
        return None;
    }
    let v: Value = serde_json::from_str(t).ok()?;
    if !v.is_object() {
        return None;
    }
    let stats = v.get("stats").filter(|s| s.is_object()).cloned();
    Some(JsonLogLine {
        level: get_str(&v, "level"),
        msg: get_str(&v, "msg"),
        object: get_str(&v, "object"),
        size: get_i64(&v, "size"),
        stats,
    })
}

// ---- File transfer tracker ----------------------------------------------

/// Accumulates per-file status from CLI JSON logs.
#[derive(Debug, Default)]
pub struct FileTransferTracker {
    by_name: HashMap<String, FileTransfer>,
    order: Vec<String>,
}

impl FileTransferTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, name: &str) -> Option<&FileTransfer> {
        self.by_name.get(name)
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn upsert(&mut self, ft: FileTransfer) {
        if ft.name.is_empty() {
            return;
        }
        if let Some(prev) = self.by_name.get(&ft.name) {
            // Don't demote completed/failed/checked back to transferring.
            if matches!(prev.status.as_str(), "completed" | "failed" | "checked")
                && ft.status == "transferring"
            {
                return;
            }
            self.by_name.insert(ft.name.clone(), ft);
            return;
        }
        if self.order.len() >= MAX_TRACKED_FILES && ft.status != "failed" {
            self.evict_replaceable();
            if self.order.len() >= MAX_TRACKED_FILES {
                return;
            }
        }
        self.order.push(ft.name.clone());
        self.by_name.insert(ft.name.clone(), ft);
    }

    fn evict_replaceable(&mut self) {
        let status = |n: &String| self.by_name.get(n).map(|f| f.status.as_str());
        if let Some(i) = self.order.iter().position(|n| status(n) == Some("pending")) {
            let name = self.order.remove(i);
            self.by_name.remove(&name);
            return;
        }
        let pos = self
            .order
            .iter()
            .position(|n| matches!(status(n), Some("completed" | "checked")));
        if let Some(i) = pos {
            let name = self.order.remove(i);
            self.by_name.remove(&name);
        }
    }

    /// Insert listed files as pending without demoting known live rows.
    pub fn seed_pending(&mut self, entries: &[FileEntry]) {
        for e in entries {
            if e.is_dir {
                continue;
            }
            let path = e.path.trim();
            let name = if !path.is_empty() {
                path
            } else {
                e.name.trim()
            };
            if name.is_empty() {
                continue;
            }
            if let Some(prev) = self.by_name.get_mut(name) {
                if prev.size == 0 && e.size > 0 {
                    prev.size = e.size;
                }
                continue;
            }
            if self.order.len() >= MAX_TRACKED_FILES {
                continue;
            }
            self.by_name.insert(
                name.to_string(),
                FileTransfer {
                    name: name.to_string(),
                    size: e.size,
                    status: "pending".into(),
                    ..Default::default()
                },
            );
            self.order.push(name.to_string());
        }
    }

    /// Rows in insertion order, plus a synthetic `(N pending)` row when
    /// `total_files` exceeds the known rows.
    pub fn snapshot(&self, total_files: i64) -> Vec<FileTransfer> {
        let mut out = Vec::with_capacity(self.order.len() + 1);
        let (mut active, mut completed, mut failed, mut pending_named) = (0i64, 0i64, 0i64, 0i64);
        for name in &self.order {
            let Some(ft) = self.by_name.get(name) else {
                continue;
            };
            out.push(ft.clone());
            match ft.status.as_str() {
                "transferring" | "checking" => active += 1,
                "completed" | "checked" => completed += 1,
                "failed" => failed += 1,
                "pending" => pending_named += 1,
                _ => {}
            }
        }
        if total_files > 0 {
            let known = completed + failed + active + pending_named;
            let pend = total_files - known;
            if pend > 0 {
                out.push(FileTransfer::new(format!("({pend} pending)"), "pending"));
            }
        }
        out
    }
}

// ---- Line parsers --------------------------------------------------------

/// Parse a JSON log line's `stats` object into `s`. Returns false when the
/// line is not JSON-with-stats (caller falls back to the text parser).
pub fn parse_json_stats_line(line: &str, s: &mut SyncStats) -> bool {
    let Some(entry) = decode_log_line(line) else {
        return false;
    };
    let Some(st) = entry.stats else { return false };
    s.bytes = get_i64(&st, "bytes");
    s.bytes_total = get_i64(&st, "totalBytes");
    s.files = get_i64(&st, "transfers");
    s.files_total = get_i64(&st, "totalTransfers");
    s.transfers = s.files;
    s.checks = get_i64(&st, "checks");
    s.checks_total = get_i64(&st, "totalChecks");
    s.deletes = get_i64(&st, "deletes");
    s.renames = get_i64(&st, "renames");
    s.errors = get_i64(&st, "errors");
    s.speed = get_f64(&st, "speed");
    if let Some(eta) = st.get("eta").and_then(Value::as_f64) {
        s.eta = eta as i64;
    }
    if !entry.object.is_empty() {
        s.current_file = entry.object;
    }
    true
}

/// Update aggregate stats + per-file tracker from one log line.
pub fn ingest_json_log_line(line: &str, s: &mut SyncStats, track: &mut FileTransferTracker) {
    let Some(entry) = decode_log_line(line) else {
        return;
    };

    if let Some(arr) = entry
        .stats
        .as_ref()
        .and_then(|st| st.get("transferring"))
        .and_then(Value::as_array)
    {
        for tr in arr {
            let name = get_str(tr, "name");
            if name.is_empty() {
                continue;
            }
            s.current_file = name.clone();
            track.upsert(FileTransfer {
                name,
                size: get_i64(tr, "size"),
                bytes: get_i64(tr, "bytes"),
                progress: get_f64(tr, "percentage"),
                status: "transferring".into(),
                speed: get_f64(tr, "speed"),
                error: String::new(),
            });
        }
    }

    if entry.object.is_empty() {
        return;
    }
    s.current_file = entry.object.clone();
    let msg = entry.msg.to_lowercase();
    let level = entry.level.to_lowercase();

    let mut ft = FileTransfer {
        name: entry.object.clone(),
        size: entry.size,
        bytes: entry.size,
        ..Default::default()
    };
    if level == "error" || msg.contains("error") || msg.contains("failed") {
        ft.status = "failed".into();
        ft.error = entry.msg.clone();
        ft.progress = 0.0;
    } else if msg.contains("check") {
        if msg.contains("ok") || msg.contains("identical") {
            ft.status = "checked".into();
            ft.progress = 100.0;
        } else {
            ft.status = "checking".into();
        }
    } else if msg.contains("copied")
        || msg.contains("moved")
        || msg.contains("updated")
        || msg.contains("multi-thread")
    {
        ft.status = "completed".into();
        ft.progress = 100.0;
        if entry.size > 0 {
            ft.bytes = entry.size;
        }
    } else if level == "info" || level == "notice" {
        ft.status = "completed".into();
        ft.progress = 100.0;
    } else {
        return;
    }
    track.upsert(ft);
}

/// Reduce rclone messages into stable UI lifecycle markers.
pub fn update_stage(line: &str, s: &mut SyncStats) {
    let Some(entry) = decode_log_line(line) else {
        return;
    };
    let msg = entry.msg.to_lowercase();
    let transferring_nonempty = entry
        .stats
        .as_ref()
        .and_then(|st| st.get("transferring"))
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty());
    let stage = if transferring_nonempty
        || msg.contains("copied")
        || msg.contains("transferred")
        || msg.contains("moved")
    {
        "transferring"
    } else if msg.contains("checking") || msg.contains("check") {
        "checking"
    } else if msg.contains("listing") || msg.contains("list ") {
        "listing"
    } else if msg.contains("retry") || msg.contains("rate limit") || msg.contains("throttle") {
        "retrying"
    } else if msg.contains("starting") || msg.contains("config file") || msg.contains("using ") {
        "connecting"
    } else {
        return;
    };
    s.stage = stage.into();
    s.stage_detail = entry.object;
}

/// Legacy text `--stats` parser (`TRANSFER: x/y` etc.).
pub fn parse_stats_line(line: &str, s: &mut SyncStats) {
    if !line.contains("INFO") {
        return;
    }
    let after = |label: &str| line.find(label).map(|i| &line[i + label.len()..]);
    if let Some((a, b)) = after("TRANSFER: ").and_then(parse_fraction) {
        s.bytes = a;
        s.bytes_total = b;
    }
    if let Some((a, b)) = after("CHECK: ").and_then(parse_fraction) {
        s.checks = a;
        s.checks_total = b;
    }
    if let Some(n) = after("ERRORS: ").and_then(parse_leading_int) {
        s.errors = n;
    }
    if let Some(n) = after("DELETED: ").and_then(parse_leading_int) {
        s.deletes = n;
    }
}

/// Parse `"X/Y more"` (as found after a `TRANSFER: ` label, or a full
/// `"LABEL: X/Y"` token): the Swift/Go parser skips up to the first space.
/// Here the caller already stripped the label, so the first token is `X/Y`;
/// a leading label (input with a space before the slash) is also handled.
pub fn parse_fraction(s: &str) -> Option<(i64, i64)> {
    // Mirror Swift: skip to after the first space, then split on "/".
    // Callers strip the label, so when `s` begins with `X/Y` use it directly.
    let rest = match (s.find(' '), s.find('/')) {
        (Some(sp), Some(sl)) if sp < sl => &s[sp + 1..],
        (_, Some(_)) => s,
        _ => return None,
    };
    let slash = rest.find('/')?;
    let left = &rest[..slash];
    let right_and_more = &rest[slash + 1..];
    let right = right_and_more.split(' ').next().unwrap_or(right_and_more);
    Some((parse_size(left), parse_size(right)))
}

/// Leading unsigned integer of `s` (after optional leading spaces).
pub fn parse_leading_int(s: &str) -> Option<i64> {
    let digits: String = s
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Parse rclone size suffixes: `1.024k`, `2M`, `1G`, `1024` → bytes.
pub fn parse_size(s: &str) -> i64 {
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let (num, suffix) = s.split_at(end);
    if num.is_empty() {
        return 0;
    }
    let Ok(n) = num.parse::<f64>() else { return 0 };
    let mult: f64 = match suffix.to_lowercase().as_str() {
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    (n * mult) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, size: i64) -> FileEntry {
        FileEntry {
            name: path.into(),
            path: path.into(),
            size,
            ..Default::default()
        }
    }

    #[test]
    fn json_stats_line_parses_all_fields() {
        let line = r#"{"time":"2026-06-28T23:49:00+07:00","level":"notice","msg":"...","stats":{"bytes":512000,"checks":3,"deletes":1,"errors":2,"eta":null,"speed":1234.5,"totalBytes":1024000,"totalChecks":6,"totalTransfers":4,"transfers":2}}"#;
        let mut s = SyncStats::default();
        assert!(parse_json_stats_line(line, &mut s));
        assert_eq!((s.bytes, s.bytes_total), (512000, 1024000));
        assert_eq!((s.files, s.files_total, s.transfers), (2, 4, 2));
        assert_eq!((s.checks, s.checks_total), (3, 6));
        assert_eq!((s.deletes, s.errors), (1, 2));
        assert_eq!(s.speed, 1234.5);
        assert_eq!(s.eta, 0);
    }

    #[test]
    fn json_stats_line_eta_and_object() {
        let line = r#"{"level":"info","msg":"stats","object":"a.txt","stats":{"bytes":9000,"totalBytes":10000,"transfers":1,"totalTransfers":2,"checks":1,"totalChecks":2,"speed":123.0,"eta":42.9,"renames":3}}"#;
        let mut s = SyncStats::default();
        assert!(parse_json_stats_line(line, &mut s));
        assert_eq!(s.eta, 42);
        assert_eq!(s.current_file, "a.txt");
        assert_eq!(s.renames, 3);
        assert_eq!(s.speed, 123.0);
    }

    #[test]
    fn json_stats_line_rejections() {
        let mut s = SyncStats::default();
        assert!(!parse_json_stats_line(
            "2025/01/15 10:00:00 INFO  : TRANSFER: 1k/2k",
            &mut s
        ));
        assert!(!parse_json_stats_line(
            r#"{"level":"info","msg":"hi"}"#,
            &mut s
        ));
        assert!(!parse_json_stats_line("{not json", &mut s));
        assert!(!parse_json_stats_line("", &mut s));
        assert!(!parse_json_stats_line("[1,2]", &mut s));
        assert_eq!(s, SyncStats::default());
    }

    #[test]
    fn json_stats_line_tolerates_missing_keys_and_whitespace() {
        let mut s = SyncStats::default();
        assert!(parse_json_stats_line(
            "  {\"stats\":{\"bytes\":5}}  ",
            &mut s
        ));
        assert_eq!(s.bytes, 5);
        assert_eq!(s.bytes_total, 0);
    }

    #[test]
    fn update_stage_cases() {
        let mut s = SyncStats::default();
        update_stage(r#"{"level":"info","msg":"Listing objects"}"#, &mut s);
        assert_eq!((s.stage.as_str(), s.stage_detail.as_str()), ("listing", ""));
        update_stage(
            r#"{"level":"info","msg":"Copied (new)","object":"folder/photo.jpg"}"#,
            &mut s,
        );
        assert_eq!(
            (s.stage.as_str(), s.stage_detail.as_str()),
            ("transferring", "folder/photo.jpg")
        );
        update_stage(r#"{"level":"notice","msg":"Retry 1/3"}"#, &mut s);
        assert_eq!(s.stage, "retrying");
        update_stage(
            r#"{"level":"info","msg":"Starting bandwidth limiter"}"#,
            &mut s,
        );
        assert_eq!(s.stage, "connecting");
        update_stage(r#"{"level":"info","msg":"Checking"}"#, &mut s);
        assert_eq!(s.stage, "checking");
        // Unrelated message leaves stage untouched.
        update_stage(r#"{"level":"info","msg":"hello"}"#, &mut s);
        assert_eq!(s.stage, "checking");
        // Non-JSON ignored.
        update_stage("plain copied text", &mut s);
        assert_eq!(s.stage, "checking");
        // Non-empty transferring array => transferring.
        update_stage(
            r#"{"msg":"x","stats":{"transferring":[{"name":"a"}]}}"#,
            &mut s,
        );
        assert_eq!(s.stage, "transferring");
    }

    #[test]
    fn parse_size_variants() {
        let cases: [(&str, i64); 15] = [
            ("", 0),
            ("0", 0),
            ("abc", 0),
            (".", 0),
            ("1024", 1024),
            ("100", 100),
            ("1k", 1024),
            ("1K", 1024),
            ("1kb", 1024),
            ("1m", 1024 * 1024),
            ("1MB", 1024 * 1024),
            ("1g", 1024 * 1024 * 1024),
            ("1t", 1024i64 * 1024 * 1024 * 1024),
            ("2.5k", 2560),
            ("1.5g", (1.5 * 1024.0 * 1024.0 * 1024.0) as i64),
        ];
        for (s, want) in cases {
            assert_eq!(parse_size(s), want, "{s:?}");
        }
    }

    #[test]
    fn parse_fraction_cases() {
        assert_eq!(parse_fraction("nospaces"), None);
        assert_eq!(parse_fraction("TRANSFER: 1k"), None);
        assert_eq!(parse_fraction("TRANSFER: 100/200"), Some((100, 200)));
        assert_eq!(
            parse_fraction("1.024k/2.048k BYTES 10/20"),
            Some((1048, 2097))
        );
        assert_eq!(parse_fraction("3/7"), Some((3, 7)));
    }

    #[test]
    fn parse_leading_int_cases() {
        assert_eq!(parse_leading_int("4"), Some(4));
        assert_eq!(parse_leading_int("12 files"), Some(12));
        assert_eq!(parse_leading_int("x"), None);
        assert_eq!(parse_leading_int(""), None);
    }

    #[test]
    fn text_stats_lines() {
        let mut s = SyncStats::default();
        parse_stats_line("some non-info line", &mut s);
        assert_eq!(s, SyncStats::default());

        parse_stats_line("TRANSFER: 1k/2M", &mut s); // no INFO
        assert_eq!(s.bytes, 0);

        parse_stats_line("2025/01/15 10:00:00 INFO  : TRANSFER: 1k/2M BYTES", &mut s);
        assert_eq!((s.bytes, s.bytes_total), (1024, 2 * 1024 * 1024));

        parse_stats_line("2025/01/15 10:00:00 INFO  : CHECK: 3/7", &mut s);
        assert_eq!((s.checks, s.checks_total), (3, 7));

        parse_stats_line("2025/01/15 10:00:00 INFO  : ERRORS: 4", &mut s);
        assert_eq!(s.errors, 4);
        parse_stats_line("2025/01/15 10:00:00 INFO  : ERRORS: 5 retried", &mut s);
        assert_eq!(s.errors, 5);

        parse_stats_line("2025/01/15 10:00:00 INFO  : DELETED: 12", &mut s);
        assert_eq!(s.deletes, 12);

        let mut s2 = SyncStats::default();
        parse_stats_line(
            "2025/01/15 10:00:00 INFO  : TRANSFER: 1.024k/2.048k BYTES 10/20 ERRORS 0",
            &mut s2,
        );
        assert_eq!(s2.bytes, 1048);
    }

    #[test]
    fn tracker_seed_pending_and_promote() {
        let mut tr = FileTransferTracker::new();
        let mut dir = entry("dir", 0);
        dir.is_dir = true;
        tr.seed_pending(&[entry("a.txt", 10), entry("b.txt", 20), dir]);
        let snap = tr.snapshot(0);
        assert_eq!(snap.len(), 2);
        assert!(snap.iter().all(|f| f.status == "pending"));

        tr.upsert(FileTransfer {
            name: "a.txt".into(),
            size: 10,
            bytes: 5,
            progress: 50.0,
            status: "transferring".into(),
            ..Default::default()
        });
        assert_eq!(tr.get("a.txt").unwrap().status, "transferring");
        assert_eq!(tr.get("b.txt").unwrap().status, "pending");

        tr.seed_pending(&[entry("a.txt", 10)]);
        assert_eq!(tr.get("a.txt").unwrap().status, "transferring");
    }

    #[test]
    fn tracker_seed_fills_size_and_uses_name_fallback() {
        let mut tr = FileTransferTracker::new();
        tr.upsert(FileTransfer::new("x", "transferring"));
        tr.seed_pending(&[entry("x", 77)]);
        assert_eq!(tr.get("x").unwrap().size, 77);
        let e = FileEntry {
            name: " n.bin ".into(),
            path: "  ".into(),
            size: 3,
            ..Default::default()
        };
        tr.seed_pending(&[e, FileEntry::default()]);
        assert!(tr.get("n.bin").is_some());
        assert_eq!(tr.len(), 2);
    }

    #[test]
    fn tracker_snapshot_synthetic_pending() {
        let mut tr = FileTransferTracker::new();
        tr.upsert(FileTransfer {
            name: "done.txt".into(),
            status: "completed".into(),
            progress: 100.0,
            ..Default::default()
        });
        let snap = tr.snapshot(3);
        let synth = snap
            .iter()
            .find(|f| f.status == "pending")
            .expect("synthetic row");
        assert_eq!(synth.name, "(2 pending)");

        tr.seed_pending(&[entry("x.bin", 1), entry("y.bin", 1)]);
        let snap = tr.snapshot(3);
        assert!(snap.iter().all(|f| !f.name.starts_with('(')), "{snap:?}");
        assert_eq!(snap.len(), 3);
    }

    #[test]
    fn tracker_seed_never_evicts_live_progress() {
        let mut tr = FileTransferTracker::new();
        tr.upsert(FileTransfer {
            name: "done.txt".into(),
            status: "completed".into(),
            progress: 100.0,
            ..Default::default()
        });
        tr.upsert(FileTransfer {
            name: "active.txt".into(),
            status: "transferring".into(),
            progress: 40.0,
            ..Default::default()
        });
        let entries: Vec<FileEntry> = (0..MAX_TRACKED_FILES + 10)
            .map(|i| entry(&format!("seed-{i}.bin"), 1))
            .collect();
        tr.seed_pending(&entries);
        assert_eq!(tr.get("done.txt").unwrap().status, "completed");
        assert_eq!(tr.get("active.txt").unwrap().status, "transferring");
        assert_eq!(tr.len(), MAX_TRACKED_FILES);
    }

    #[test]
    fn tracker_upsert_does_not_demote_and_evicts() {
        let mut tr = FileTransferTracker::new();
        tr.upsert(FileTransfer::new("a", "completed"));
        tr.upsert(FileTransfer::new("a", "transferring"));
        assert_eq!(tr.get("a").unwrap().status, "completed");
        tr.upsert(FileTransfer::new("", "completed")); // ignored
        assert_eq!(tr.len(), 1);

        // Fill with pending; a live row evicts the first pending.
        let mut tr = FileTransferTracker::new();
        let entries: Vec<FileEntry> = (0..MAX_TRACKED_FILES)
            .map(|i| entry(&format!("p{i}"), 1))
            .collect();
        tr.seed_pending(&entries);
        tr.upsert(FileTransfer::new("live", "transferring"));
        assert_eq!(tr.len(), MAX_TRACKED_FILES);
        assert!(tr.get("p0").is_none());
        assert!(tr.get("live").is_some());

        // Fill with transferring rows: non-failed new rows rejected, failed accepted.
        let mut tr = FileTransferTracker::new();
        for i in 0..MAX_TRACKED_FILES {
            tr.upsert(FileTransfer::new(format!("t{i}"), "transferring"));
        }
        tr.upsert(FileTransfer::new("new", "transferring"));
        assert!(tr.get("new").is_none());
        tr.upsert(FileTransfer::new("bad", "failed"));
        assert!(tr.get("bad").is_some());
        assert_eq!(tr.len(), MAX_TRACKED_FILES + 1);

        // Completed rows are evictable.
        let mut tr = FileTransferTracker::new();
        tr.upsert(FileTransfer::new("c", "completed"));
        for i in 1..MAX_TRACKED_FILES {
            tr.upsert(FileTransfer::new(format!("t{i}"), "transferring"));
        }
        tr.upsert(FileTransfer::new("n", "transferring"));
        assert!(tr.get("c").is_none());
        assert!(tr.get("n").is_some());
    }

    #[test]
    fn ingest_transferring_then_copied() {
        let mut tr = FileTransferTracker::new();
        let mut s = SyncStats::default();
        tr.seed_pending(&[entry("photo.jpg", 9000)]);
        ingest_json_log_line(
            r#"{"level":"info","msg":"stats","stats":{"bytes":100,"totalBytes":9000,"transfers":0,"totalTransfers":1,"transferring":[{"name":"photo.jpg","size":9000,"bytes":100,"percentage":1,"speed":50}]}}"#,
            &mut s,
            &mut tr,
        );
        let ft = tr.get("photo.jpg").unwrap();
        assert_eq!(ft.status, "transferring");
        assert_eq!(
            (ft.size, ft.bytes, ft.progress, ft.speed),
            (9000, 100, 1.0, 50.0)
        );
        assert_eq!(s.current_file, "photo.jpg");
        ingest_json_log_line(
            r#"{"level":"info","msg":"Copied (new)","object":"photo.jpg","size":9000}"#,
            &mut s,
            &mut tr,
        );
        assert_eq!(tr.get("photo.jpg").unwrap().status, "completed");
        let snap = tr.snapshot(1);
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].status, "completed");
        assert_eq!(snap[0].progress, 100.0);
    }

    #[test]
    fn ingest_status_classification() {
        let mut tr = FileTransferTracker::new();
        let mut s = SyncStats::default();
        let cases = [
            (
                r#"{"level":"error","msg":"Failed to copy: boom","object":"e.txt"}"#,
                "e.txt",
                "failed",
            ),
            (
                r#"{"level":"info","msg":"Size and modification time the same (differ by 0s)","object":"i.txt"}"#,
                "i.txt",
                "completed",
            ),
            (
                r#"{"level":"info","msg":"checking","object":"c2.txt"}"#,
                "c2.txt",
                "checking",
            ),
            (
                r#"{"level":"info","msg":"Check: OK","object":"ok.txt"}"#,
                "ok.txt",
                "checked",
            ),
            (
                r#"{"level":"info","msg":"Updated modification time","object":"u.txt"}"#,
                "u.txt",
                "completed",
            ),
            (
                r#"{"level":"notice","msg":"something","object":"n.txt"}"#,
                "n.txt",
                "completed",
            ),
        ];
        for (line, name, want) in cases {
            ingest_json_log_line(line, &mut s, &mut tr);
            assert_eq!(
                tr.get(name).map(|f| f.status.as_str()),
                Some(want),
                "{name}"
            );
        }
        assert_eq!(tr.get("e.txt").unwrap().error, "Failed to copy: boom");
        // Debug-level line with unknown message: ignored (no row), but current_file set.
        ingest_json_log_line(
            r#"{"level":"debug","msg":"zzz","object":"d.txt"}"#,
            &mut s,
            &mut tr,
        );
        assert!(tr.get("d.txt").is_none());
        assert_eq!(s.current_file, "d.txt");
        // Garbage ignored.
        ingest_json_log_line("not json", &mut s, &mut tr);
        ingest_json_log_line(r#"{"level":"info","msg":"no object"}"#, &mut s, &mut tr);
        assert_eq!(s.current_file, "d.txt");
    }

    #[test]
    fn truncate_string_cases() {
        assert_eq!(truncate_string("abc", 3), "abc");
        assert_eq!(truncate_string("abcd", 3), "abc...(truncated)");
        assert_eq!(truncate_string("ééé", 2), "éé...(truncated)");
        assert_eq!(truncate_string("", 0), "");
    }

    #[test]
    fn unix_now_is_recent() {
        assert!(unix_now() > 1_700_000_000);
    }
}
