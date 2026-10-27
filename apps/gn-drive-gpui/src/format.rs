//! Small formatters shared by the workspace, edge cards, history and logs.

use gpui::Rgba;

use crate::engine::{FileTransfer, SyncProgress};

/// Byte count in the shape rclone prints: 940 B, 12.4 KiB, 3.1 MiB.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

pub fn human_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec <= 0.0 {
        "—".into()
    } else {
        format!("{}/s", human_bytes(bytes_per_sec.max(0.0) as u64))
    }
}

/// Seconds in the shape a run log uses: 8s, 4m 12s, 1h 03m.
pub fn human_duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "—".into();
    }
    let total = seconds.round() as u64;
    if total < 60 {
        format!("{total}s")
    } else if total < 3600 {
        format!("{}m {:02}s", total / 60, total % 60)
    } else {
        format!("{}h {:02}m", total / 3600, (total % 3600) / 60)
    }
}

/// The speed part of an edge label. Zero reads as a quiet dash.
pub fn progress_speed(progress: &SyncProgress) -> String {
    if progress.state == "running" {
        human_speed(progress.bytes_per_sec)
    } else {
        String::new()
    }
}

/// The share of a transfer that is done, 0..=1.
pub fn fraction(done: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0) as f32
    }
}

/// Semantic status colour. Green is running or completed, amber is waiting
/// or stopped, red is failed. The accent is not a status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusTone {
    Ok,
    Warn,
    Err,
}

pub fn flow_status_tone(status: &str) -> StatusTone {
    match status {
        "running" | "completed" => StatusTone::Ok,
        "failed" => StatusTone::Err,
        _ => StatusTone::Warn,
    }
}

/// The status color a flow dot or badge takes.
pub fn status_color(status: &str) -> Rgba {
    let theme = crate::theme();
    match flow_status_tone(status) {
        StatusTone::Ok => theme.ok,
        StatusTone::Warn => theme.warn,
        StatusTone::Err => theme.err,
    }
}

pub fn status_label(status: &str) -> &'static str {
    match status {
        "running" => "Running",
        "cancelling" => "Stopping",
        "completed" => "Completed",
        "failed" => "Failed",
        // A user stop is not a failure. The stored status stays `cancelled`.
        "cancelled" => "Stopped",
        _ => "Idle",
    }
}

pub fn running(status: &str) -> bool {
    matches!(status, "running" | "cancelling")
}

/// A cron expression in words. The raw five fields stay in the Advanced
/// field; everywhere else a person reads this sentence.
pub fn cron_label(cron: &str) -> String {
    let cron = cron.trim();
    if cron.is_empty() {
        return if crate::prefs::is_vi() {
            "Thủ công".into()
        } else {
            "Manual".into()
        };
    }
    let parts: Vec<&str> = cron.split_whitespace().collect();
    if parts.len() != 5 {
        return if crate::prefs::is_vi() {
            "Lịch tùy chỉnh".into()
        } else {
            "Custom schedule".into()
        };
    }
    let (min, hour, dom, month, dow) = (parts[0], parts[1], parts[2], parts[3], parts[4]);
    if dom == "*" && month == "*" {
        if min == "0" && hour == "*" && dow == "*" {
            return phrase("Every hour", "Mỗi giờ");
        }
        if min == "0"
            && let Some(step) = hour.strip_prefix("*/")
            && let Ok(step) = step.parse::<u32>()
            && step > 0
            && dow == "*"
        {
            return if crate::prefs::is_vi() {
                format!("Mỗi {step} giờ")
            } else {
                format!("Every {step} hours")
            };
        }
        if let (Ok(minute), Ok(hour_n)) = (min.parse::<u32>(), hour.parse::<u32>())
            && minute < 60
            && hour_n < 24
        {
            let clock = format!("{hour_n:02}:{minute:02}");
            if dow == "*" {
                return if crate::prefs::is_vi() {
                    format!("Mỗi ngày lúc {clock}")
                } else {
                    format!("Every day at {clock}")
                };
            }
            if dow == "1-5" {
                return if crate::prefs::is_vi() {
                    format!("Ngày trong tuần lúc {clock}")
                } else {
                    format!("Weekdays at {clock}")
                };
            }
            if let Some(day) = weekday_name(dow) {
                return if crate::prefs::is_vi() {
                    format!("Mỗi {day} lúc {clock}")
                } else {
                    format!("Every {day} at {clock}")
                };
            }
        }
    }
    phrase("Custom schedule", "Lịch tùy chỉnh")
}

fn phrase(en: &str, vi: &str) -> String {
    if crate::prefs::is_vi() {
        vi.to_string()
    } else {
        en.to_string()
    }
}

fn weekday_name(dow: &str) -> Option<&'static str> {
    if crate::prefs::is_vi() {
        return match dow {
            "0" | "7" => Some("Chủ nhật"),
            "1" => Some("thứ Hai"),
            "2" => Some("thứ Ba"),
            "3" => Some("thứ Tư"),
            "4" => Some("thứ Năm"),
            "5" => Some("thứ Sáu"),
            "6" => Some("thứ Bảy"),
            _ => None,
        };
    }
    match dow {
        "0" | "7" => Some("Sunday"),
        "1" => Some("Monday"),
        "2" => Some("Tuesday"),
        "3" => Some("Wednesday"),
        "4" => Some("Thursday"),
        "5" => Some("Friday"),
        "6" => Some("Saturday"),
        _ => None,
    }
}

/// Five cron fields, or empty (manual). Used to explain a bad Advanced value.
pub fn cron_error(cron: &str) -> Option<&'static str> {
    let cron = cron.trim();
    if cron.is_empty() {
        return None;
    }
    let parts: Vec<&str> = cron.split_whitespace().collect();
    if parts.len() == 5 && parts.iter().all(|part| !part.is_empty()) {
        None
    } else if crate::prefs::is_vi() {
        Some("Lịch cần đúng 5 phần: phút giờ ngày tháng thứ.")
    } else {
        Some("A schedule needs 5 fields: minute hour day month weekday.")
    }
}

/// Keep the start and the end of a path, so `/Users/me/…/Photos` stays readable.
pub fn middle_ellipsis(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if max_chars < 5 || chars.len() <= max_chars {
        return text.to_string();
    }
    let tail = text
        .rfind('/')
        .filter(|at| *at > 0 && *at + 1 < text.len())
        .map(|at| text[at..].chars().count())
        .unwrap_or(0);
    let (head, tail) = if tail > 0 && tail + 4 < max_chars {
        (max_chars - 1 - tail, tail)
    } else {
        let keep = max_chars - 1;
        (keep / 2, keep - keep / 2)
    };
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(chars[chars.len() - tail..].iter());
    out
}

pub fn action_label(action: &str) -> &'static str {
    match action {
        "bi" => "Two-way",
        "bi-resync" => "Re-sync",
        _ => "Push",
    }
}

/// Glyph for an edge chip: Push arrow, two-way sync, or re-sync restart.
pub fn action_glyph(action: &str) -> &'static str {
    match action {
        "bi" => "sync",
        "bi-resync" => "restart",
        _ => "arrow-down-to-line",
    }
}

/// One line for the edge chip. The label stays whole; the chip never substitutes "…".
pub fn chip_tip(action: &str) -> &'static str {
    match action {
        "bi" => "Two-way — keep both folders the same",
        "bi-resync" => "Re-sync — rebuild the comparison, then keep both folders the same",
        _ => "Push — copy changes left → right",
    }
}

/// One sentence naming the two places. The connection inspector leads with this.
pub fn connection_summary(action: &str, from: &str, to: &str) -> String {
    let from = named_place(from);
    let to = named_place(to);
    match action {
        "bi" => format!("Two-way: keep {from} and {to} the same. Deletions are mirrored."),
        "bi-resync" => {
            format!("Re-sync: rebuild the comparison, then keep {from} and {to} the same.")
        }
        _ => format!("Push: copy changes from {from} to {to}. Deletions are mirrored."),
    }
}

fn named_place(place: &str) -> &str {
    let place = place.trim();
    if place.is_empty() {
        "the folder"
    } else {
        place
    }
}

/// Toolbar line under the flow name: how many connections, and when it runs.
pub fn flow_summary(connections: usize, when: &str) -> String {
    let noun = if connections == 1 {
        "connection"
    } else {
        "connections"
    };
    let when = when.trim();
    let when = if when.is_empty() { "Manual" } else { when };
    format!("{connections} {noun} · {when}")
}

/// One sentence for the tooltip and the inspector. Matches the engine:
/// push copies left to right; two-way mirrors both ways.
pub fn action_help(action: &str) -> &'static str {
    match action {
        "bi" => {
            "Two-way: keep both folders the same. New and changed files copy either way, and deletions are mirrored."
        }
        "bi-resync" => {
            "Re-sync: rebuild the two-way comparison from scratch, then keep both folders the same."
        }
        _ => {
            "Push: copy changes from the left folder to the right. Deletions on the left are mirrored on the right."
        }
    }
}

/// Where a live file sits on an edge, 0..=1, and which color it takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FileDot {
    pub t: f32,
    pub kind: &'static str,
}

fn transfer_rank(status: &str) -> Option<u8> {
    match status {
        "transferring" => Some(0),
        "failed" => Some(1),
        "checking" => Some(2),
        "checked" => Some(3),
        "completed" => Some(4),
        _ => None,
    }
}

fn unit_progress(progress: f64) -> f32 {
    let value = if progress > 1.0 {
        progress / 100.0
    } else {
        progress
    };
    value.clamp(0.0, 1.0) as f32
}

/// At most 24 dots. Pending files are left out. Order is status rank, then name.
pub fn file_dots(action: &str, transfers: &[FileTransfer]) -> Vec<FileDot> {
    let mut ranked: Vec<&FileTransfer> = transfers
        .iter()
        .filter(|transfer| transfer_rank(&transfer.status).is_some())
        .collect();
    ranked.sort_by(|left, right| {
        transfer_rank(&left.status)
            .cmp(&transfer_rank(&right.status))
            .then_with(|| left.name.cmp(&right.name))
    });
    ranked.truncate(24);
    let mirror = matches!(action, "bi" | "bi-resync");
    ranked
        .into_iter()
        .map(|transfer| {
            let kind = match transfer.status.as_str() {
                "failed" => "failed",
                "checking" | "checked" => "checking",
                "completed" => "completed",
                _ => "transferring",
            };
            let t = match transfer.status.as_str() {
                "failed" => 0.50,
                "completed" | "checked" => 0.98,
                _ => 0.18 + 0.52 * unit_progress(transfer.progress),
            };
            let t = if mirror { 1.0 - t } else { t };
            FileDot { t, kind }
        })
        .collect()
}

/// Card order: transferring, checking, failed, pending, checked, completed.
/// Pending stays in the list. Dots use [`file_dots`], which leaves pending out.
pub(crate) fn card_rank(status: &str) -> u8 {
    match status {
        "transferring" => 0,
        "checking" => 1,
        "failed" => 2,
        "pending" => 3,
        "checked" => 4,
        "completed" => 5,
        _ => 6,
    }
}

/// Transfers for the file card. Pending rows stay. Ties break on the file name.
pub fn ranked_transfers(transfers: &[FileTransfer]) -> Vec<&FileTransfer> {
    let mut ranked: Vec<&FileTransfer> = transfers.iter().collect();
    ranked.sort_by(|left, right| {
        card_rank(&left.status)
            .cmp(&card_rank(&right.status))
            .then_with(|| left.name.cmp(&right.name))
    });
    ranked
}

pub fn percent_text(progress: f64) -> String {
    let value = unit_progress(progress);
    format!("{:.0}%", value * 100.0)
}

/// Strip rclone noise, hide a user stop, and keep the sentence short.
pub fn humanize_error(message: &str) -> String {
    let raw = message.trim();
    if raw.is_empty() || is_cancellation(raw) {
        return String::new();
    }
    let mut text = raw.to_string();
    if let Some(rest) = text.strip_prefix("rclone:") {
        text = rest.trim().to_string();
    }
    if let Some(at) = text.find("(stderr:") {
        text.truncate(at);
        text = text.trim().trim_end_matches(',').to_string();
    }
    let lower = text.to_lowercase();
    if lower.contains('{') && (lower.contains("stats") || text.matches('{').count() > 1) {
        text = "Sync failed. Check the folders and the cloud account.".into();
    }
    if text.chars().count() > 160 {
        let truncated: String = text.chars().take(157).collect();
        text = format!("{truncated}…");
    }
    text
}

/// What the user reads, and the engine code kept for a details line.
pub struct Explained {
    pub text: String,
    pub code: String,
}

/// One sentence for an engine failure. The code is not part of the sentence.
pub fn explain(code: &str, message: &str) -> Explained {
    let code = code.trim().to_string();
    let text = match code.as_str() {
        "flow_busy" => "Stop the flow before changing it.".into(),
        "flow_empty" | "empty_flow" => "This flow has no connections to run.".into(),
        "already_running" => "This flow is already running.".into(),
        "not_running" => "This flow is not running.".into(),
        "flow_not_found" => "That flow is no longer there.".into(),
        "invalid_password" => "That password is not right.".into(),
        "not_setup" => "Set a password before locking GN Drive.".into(),
        "locked" => "GN Drive is locked.".into(),
        "locked_out" | "lockout" => "Too many wrong attempts.".into(),
        "invalid_params" | "missing_name" => "Some of the details are missing.".into(),
        "profile_not_found" | "remote_not_found" => "That item is no longer there.".into(),
        "rclone_missing" => "GN Drive could not find rclone.".into(),
        "instance_busy" => "Another GN Drive is already using this setup.".into(),
        "binary_missing" | "spawn_failed" => "GN Drive could not start its engine.".into(),
        "oauth_session_not_found" => "That sign-in session is no longer there.".into(),
        "sync_failed" => sentence(&humanize_error(message), "The sync failed."),
        _ => sentence(&humanize_error(message), "Something went wrong."),
    };
    Explained { text, code }
}

impl Explained {
    /// The sentence, then `Details: code` when the engine sent a code.
    pub fn display(&self) -> String {
        if self.code.is_empty() {
            self.text.clone()
        } else if self.text.is_empty() {
            format!("Details: {}", self.code)
        } else {
            format!("{}\nDetails: {}", self.text, self.code)
        }
    }
}

/// A readable sentence. A lowercase engine line gains a capital and a period.
fn sentence(cleaned: &str, fallback: &str) -> String {
    let text = cleaned
        .trim()
        .trim_start_matches("auth:")
        .trim()
        .trim_start_matches("flow:")
        .trim();
    if text.is_empty() {
        return fallback.to_string();
    }
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return fallback.to_string();
    };
    let mut out = String::new();
    out.extend(first.to_uppercase());
    out.push_str(chars.as_str());
    if !matches!(out.chars().last(), Some('.' | '…' | '!' | '?')) {
        out.push('.');
    }
    out
}

pub fn is_cancellation(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "signal: killed",
        "signal: interrupt",
        "context canceled",
        "context cancelled",
        "errtaskcancelled",
        "task cancelled",
        "task canceled",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// A civil minute, used to phrase a history time without a timestamp crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Civil {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
}

/// History time in the same short style as the other formatters.
///
/// Today and yesterday use the local day. Older runs read `21 Oct 2026, 18:20`.
/// An unreadable value is an em dash, never a raw `T…Z` string.
pub fn friendly_time(iso: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|dur| i64::try_from(dur.as_secs()).ok())
        .unwrap_or(0);
    friendly_time_in(iso, now, local_offset_secs(now))
}

/// [`friendly_time`] with the clock and the UTC offset supplied, so tests
/// do not depend on the machine's zone.
pub fn friendly_time_in(iso: &str, now_unix: i64, offset_secs: i32) -> String {
    let Some(unix) = parse_iso(iso) else {
        return "—".into();
    };
    let shift = i64::from(offset_secs);
    friendly_when(
        civil_from_unix(unix.saturating_add(shift)),
        civil_from_unix(now_unix.saturating_add(shift)),
    )
}

fn friendly_when(at: Civil, today: Civil) -> String {
    let clock = format!("{:02}:{:02}", at.hour, at.minute);
    let at_day = days_from_civil(at.year, at.month, at.day);
    let today_day = days_from_civil(today.year, today.month, today.day);
    match today_day.checked_sub(at_day) {
        Some(0) => format!("Today {clock}"),
        Some(1) => format!("Yesterday {clock}"),
        _ => format!("{} {} {}, {clock}", at.day, month_name(at.month), at.year),
    }
}

/// The flow's name when `profile_name` is that flow, or `flowId:opId`.
/// Anything else stays as written, and an empty name falls back to the run id.
pub fn history_title(profile_name: &str, entry_id: &str, flows: &[(&str, &str)]) -> String {
    let raw = profile_name.trim();
    let token = if raw.is_empty() { entry_id.trim() } else { raw };
    if token.is_empty() {
        return "—".into();
    }
    let head = token.split(':').next().unwrap_or(token);
    for (id, name) in flows {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if *id == token || *id == head || name == token {
            return name.to_string();
        }
    }
    token.to_string()
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        _ => "Dec",
    }
}

fn is_leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn month_len(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since the Unix epoch. Howard Hinnant's civil-from-days algorithm.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = (y - era * 400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp as u64 + 2) / 5 + u64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (
        i32::try_from(year).unwrap_or(0),
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

fn civil_from_unix(unix: i64) -> Civil {
    let days = unix.div_euclid(86_400);
    let secs = u32::try_from(unix.rem_euclid(86_400)).unwrap_or(0);
    let (year, month, day) = civil_from_days(days);
    Civil {
        year,
        month,
        day,
        hour: secs / 3600,
        minute: (secs % 3600) / 60,
    }
}

/// `2026-10-21T18:20:00Z`, with optional fraction and a numeric offset.
/// Seconds since the Unix epoch, or nothing when the text is not a timestamp.
fn parse_iso(text: &str) -> Option<i64> {
    let text = text.trim();
    let (date, rest) = text.split_once(['T', 't', ' '])?;
    let mut date_parts = date.split('-');
    let year: i32 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() || month_len(year, month) < day {
        return None;
    }
    let (time, offset) = split_zone(rest)?;
    let mut time_parts = time.split(':');
    let hour: u32 = time_parts.next()?.parse().ok()?;
    let minute: u32 = time_parts.next()?.parse().ok()?;
    let sec_raw = time_parts.next().unwrap_or("0");
    if time_parts.next().is_some() {
        return None;
    }
    let sec: u32 = sec_raw.split('.').next()?.parse().ok()?;
    if hour > 23 || minute > 59 || sec > 60 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(
        days.saturating_mul(86_400)
            .saturating_add(i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(sec))
            .saturating_sub(offset),
    )
}

fn split_zone(rest: &str) -> Option<(&str, i64)> {
    let rest = rest.trim();
    if let Some(time) = rest.strip_suffix(['Z', 'z']) {
        return Some((time, 0));
    }
    let Some(idx) = rest.rfind(['+', '-']) else {
        return Some((rest, 0));
    };
    if idx == 0 {
        return None;
    }
    let time = &rest[..idx];
    let zone = &rest[idx..];
    let sign = if zone.starts_with('+') { 1 } else { -1 };
    let zone = &zone[1..];
    let (hour, minute) = if let Some((hour, minute)) = zone.split_once(':') {
        (hour.parse::<i64>().ok()?, minute.parse::<i64>().ok()?)
    } else if zone.len() == 4 {
        (zone[..2].parse().ok()?, zone[2..].parse().ok()?)
    } else if zone.len() == 2 {
        (zone.parse().ok()?, 0)
    } else {
        return None;
    };
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }
    Some((time, sign * (hour * 3600 + minute * 60)))
}

/// Seconds east of UTC for `unix`, from the process zone. `0` if that fails.
fn local_offset_secs(unix: i64) -> i32 {
    #[repr(C)]
    struct LibcTm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const std::ffi::c_char,
    }
    unsafe extern "C" {
        fn localtime_r(clock: *const i64, result: *mut LibcTm) -> *mut LibcTm;
    }
    // SAFETY: `out` is a zeroed `repr(C)` `tm`. `localtime_r` writes it before
    // we read `tm_gmtoff`, and we never dereference `tm_zone`. A null result
    // means the conversion failed, so the offset stays 0 (UTC).
    unsafe {
        let mut out = std::mem::zeroed::<LibcTm>();
        if localtime_r(&unix, &mut out).is_null() {
            return 0;
        }
        i32::try_from(out.tm_gmtoff).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_use_rclone_units() {
        assert_eq!(human_bytes(940), "940 B");
        assert_eq!(human_bytes(1536), "1.50 KiB");
        assert_eq!(human_bytes(12 * 1024 * 1024), "12.0 MiB");
    }

    #[test]
    fn cron_presets_have_words() {
        assert_eq!(cron_label("0 0 * * *"), "Every day at 00:00");
        assert_eq!(cron_label("0 3 * * *"), "Every day at 03:00");
        assert_eq!(cron_label("0 9 * * 1-5"), "Weekdays at 09:00");
        assert_eq!(cron_label("0 0 * * 0"), "Every Sunday at 00:00");
        assert_eq!(cron_label("0 */6 * * *"), "Every 6 hours");
        assert_eq!(cron_label(""), "Manual");
        assert_eq!(cron_label("not cron"), "Custom schedule");
        assert!(cron_error("0 3 *").is_some());
        assert!(cron_error("0 3 * * *").is_none());
    }

    #[test]
    fn chip_tips_name_the_action_on_one_line() {
        assert_eq!(chip_tip("push"), "Push — copy changes left → right");
        assert_eq!(chip_tip("pull"), "Push — copy changes left → right");
        assert_eq!(action_glyph("push"), "arrow-down-to-line");
        assert_eq!(action_glyph("bi"), "sync");
        assert_eq!(action_glyph("bi-resync"), "restart");
        for action in ["push", "pull", "bi", "bi-resync", ""] {
            let tip = chip_tip(action);
            let label = action_label(action);
            assert!(tip.starts_with(label), "{tip}");
            assert!(tip.contains('—'), "{tip}");
            assert!(!tip.contains('\n'), "{tip}");
            assert_ne!(tip, "…");
            assert_ne!(tip, "...");
            assert!(!action_glyph(action).is_empty());
        }
    }

    #[test]
    fn paths_keep_both_ends() {
        assert_eq!(
            middle_ellipsis("/Users/me/Pictures/Vacation/Photos", 18),
            "/Users/me/…/Photos"
        );
        assert_eq!(
            middle_ellipsis("/Users/me/Pictures/Vacation", 20),
            "/Users/me/…/Vacation"
        );
        assert_eq!(middle_ellipsis("short", 18), "short");
        assert_eq!(middle_ellipsis("/Users/me/Photos", 4), "/Users/me/Photos");
    }

    #[test]
    fn connection_summary_names_both_places() {
        assert_eq!(
            connection_summary("push", "This Mac", "Google Drive"),
            "Push: copy changes from This Mac to Google Drive. Deletions are mirrored."
        );
        assert_eq!(
            connection_summary("bi", "This Mac", "Google Drive"),
            "Two-way: keep This Mac and Google Drive the same. Deletions are mirrored."
        );
        assert!(connection_summary("bi-resync", "This Mac", "NAS").starts_with("Re-sync:"));
        assert_eq!(
            connection_summary("pull", "", "Google Drive"),
            "Push: copy changes from the folder to Google Drive. Deletions are mirrored."
        );
    }

    #[test]
    fn flow_summary_is_one_line() {
        assert_eq!(
            flow_summary(3, "Every day at 03:00"),
            "3 connections · Every day at 03:00"
        );
        assert_eq!(flow_summary(1, "Manual"), "1 connection · Manual");
        assert_eq!(flow_summary(0, ""), "0 connections · Manual");
        assert!(!flow_summary(2, "Manual").contains('\n'));
    }

    #[test]
    fn status_colour_is_semantic() {
        assert_eq!(flow_status_tone("running"), StatusTone::Ok);
        assert_eq!(flow_status_tone("completed"), StatusTone::Ok);
        assert_eq!(flow_status_tone("failed"), StatusTone::Err);
        assert_eq!(flow_status_tone("cancelled"), StatusTone::Warn);
        assert_eq!(flow_status_tone("cancelling"), StatusTone::Warn);
        assert_eq!(flow_status_tone("idle"), StatusTone::Warn);
        assert_eq!(flow_status_tone(""), StatusTone::Warn);
    }

    #[test]
    fn errors_drop_rclone_noise_and_cancellation() {
        assert_eq!(
            humanize_error("rclone: path missing (stderr: boom)"),
            "path missing"
        );
        assert!(humanize_error("signal: killed").is_empty());
        assert!(humanize_error(&"x".repeat(200)).chars().count() <= 160);
        let long = format!("failed {}", "{".repeat(3));
        assert!(humanize_error(&format!("{long} stats")).contains("Sync failed"));
    }

    #[test]
    fn errors_are_one_sentence_and_keep_the_code() {
        let busy = explain("flow_busy", "flow is running");
        assert_eq!(busy.text, "Stop the flow before changing it.");
        assert_eq!(busy.code, "flow_busy");
        assert_eq!(
            busy.display(),
            "Stop the flow before changing it.\nDetails: flow_busy"
        );
        assert_eq!(
            explain("invalid_password", "auth: invalid password").text,
            "That password is not right."
        );
        assert_eq!(
            explain("", "rclone: path missing (stderr: boom)").text,
            "Path missing."
        );
        assert!(explain("signal", "signal: killed").text == "Something went wrong.");
    }

    #[test]
    fn dots_skip_pending_and_cap_at_24() {
        let transfers = vec![
            FileTransfer {
                name: "waiting".into(),
                status: "pending".into(),
                progress: 0.2,
                ..Default::default()
            },
            FileTransfer {
                name: "done".into(),
                status: "completed".into(),
                progress: 1.0,
                ..Default::default()
            },
            FileTransfer {
                name: "now".into(),
                status: "transferring".into(),
                progress: 0.0,
                ..Default::default()
            },
        ];
        let dots = file_dots("push", &transfers);
        assert_eq!(dots.len(), 2);
        assert_eq!(dots[0].kind, "transferring");
        assert!((dots[0].t - 0.18).abs() < 0.001);
        assert_eq!(dots[1].kind, "completed");
        let mirrored = file_dots("bi", &transfers);
        assert!((mirrored[0].t - (1.0 - 0.18)).abs() < 0.001);
        let rows = ranked_transfers(&transfers);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].status, "transferring");
        assert_eq!(rows[1].status, "pending");
        assert_eq!(rows[2].status, "completed");
    }

    #[test]
    fn duration_reads_like_a_log() {
        assert_eq!(human_duration(8.0), "8s");
        assert_eq!(human_duration(252.0), "4m 12s");
        assert_eq!(human_duration(3780.0), "1h 03m");
    }

    #[test]
    fn history_times_are_today_yesterday_or_a_date() {
        let today = parse_iso("2026-10-22T15:00:00Z").unwrap();
        assert_eq!(
            friendly_time_in("2026-10-22T03:00:00Z", today, 0),
            "Today 03:00"
        );
        assert_eq!(
            friendly_time_in("2026-10-22T03:02:14.9Z", today, 0),
            "Today 03:02"
        );
        assert_eq!(
            friendly_time_in("2026-10-21T03:02:00+00:00", today, 0),
            "Yesterday 03:02"
        );
        let later = parse_iso("2026-10-23T12:00:00Z").unwrap();
        let dated = friendly_time_in("2026-10-21T18:20:00Z", later, 0);
        assert_eq!(dated, "21 Oct 2026, 18:20");
        assert!(!dated.contains('T'));
        assert!(!dated.ends_with('Z'));
        let march = parse_iso("2026-03-01T10:00:00Z").unwrap();
        assert_eq!(
            friendly_time_in("2026-02-28T09:05:00Z", march, 0),
            "Yesterday 09:05"
        );
        let evening = parse_iso("2026-10-22T18:00:00Z").unwrap();
        assert_eq!(
            friendly_time_in("2026-10-21T18:20:00Z", evening, 7 * 3600),
            "Yesterday 01:20"
        );
        assert_eq!(friendly_time_in("not a time", today, 0), "—");
        assert_eq!(friendly_time_in("", today, 0), "—");
    }

    #[test]
    fn history_titles_use_the_flow_name() {
        let flows = [
            ("F1", "Nightly Photos"),
            ("F3", "Archive of the family photo library"),
        ];
        assert_eq!(history_title("F1:O1", "T101", &flows), "Nightly Photos");
        assert_eq!(history_title("F1", "T1", &flows), "Nightly Photos");
        assert_eq!(
            history_title("Archive of the family photo library", "T99", &flows),
            "Archive of the family photo library"
        );
        assert_eq!(history_title("photos", "T97", &flows), "photos");
        assert_eq!(history_title("", "T97", &flows), "T97");
        assert_eq!(history_title("", "", &flows), "—");
    }
}
