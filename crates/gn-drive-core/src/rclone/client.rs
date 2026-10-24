//! rclone binary wrapper (port of `RcloneClient.swift`; Go `internal/rclone`).
//!
//! Every process spawn goes through an [`RcloneRunner`]; [`ProcessRunner`] is
//! the real one and tests inject [`super::testing::FakeRunner`].

use super::stats::{
    FileTransferTracker, MAX_TRACKED_FILES, ingest_json_log_line, parse_json_stats_line,
    parse_stats_line, truncate_string, unix_now, update_stage,
};
use super::{
    CancellationToken, FileEntry, ProcessOutput, ProfileFlags, ProgressFn, QuotaInfo, RcloneAction,
    RcloneError, RcloneRunner, Remote, StreamKind, SyncClient, SyncConfig, SyncFailedError,
    SyncResult, SyncStats,
};
use crate::logger::Logger;
use parking_lot::Mutex;
use serde_json::{Map, Value};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Output of [`RcloneClient::run_raw`] (Swift `ProcessResult`).
pub type ProcessResult = ProcessOutput;

// ---- Real process runner ---------------------------------------------------

/// Spawns the real rclone binary. stdin is always `/dev/null`; the
/// environment is inherited unchanged.
#[derive(Debug, Clone)]
pub struct ProcessRunner {
    binary: String,
}

impl ProcessRunner {
    pub fn new(binary: &str) -> ProcessRunner {
        ProcessRunner {
            binary: binary.to_string(),
        }
    }
}

fn exit_code_of(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    // Like Foundation's terminationStatus: signal number when signalled.
    status.code().or_else(|| status.signal()).unwrap_or(-1)
}

/// Read `r` splitting on `\n`, delivering each line (and a trailing partial
/// line) to `f`. Invalid UTF-8 is replaced, not dropped.
fn pump_lines(r: impl Read, mut f: impl FnMut(&str)) {
    let mut br = BufReader::new(r);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match br.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if buf.last() == Some(&b'\n') {
                    buf.pop();
                }
                f(&String::from_utf8_lossy(&buf));
            }
        }
    }
}

impl RcloneRunner for ProcessRunner {
    fn binary(&self) -> String {
        self.binary.clone()
    }

    fn run(&self, args: &[String]) -> std::io::Result<ProcessOutput> {
        let out = Command::new(&self.binary)
            .args(args)
            .stdin(Stdio::null())
            .output()?;
        Ok(ProcessOutput {
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            exit_code: exit_code_of(out.status),
        })
    }

    fn run_streaming(
        &self,
        args: &[String],
        token: Option<&CancellationToken>,
        on_line: &(dyn Fn(StreamKind, &str) + Sync),
    ) -> std::io::Result<i32> {
        let mut child = Command::new(&self.binary)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // pid is cleared (under the lock) as soon as the child is reaped so a
        // late cancel can never signal a recycled pid.
        let live_pid: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(Some(child.id() as i32)));
        if let Some(t) = token {
            let lp = live_pid.clone();
            t.on_terminate(move || {
                if let Some(pid) = *lp.lock() {
                    // SAFETY: plain kill(2) on a child we have not yet reaped.
                    unsafe { libc::kill(pid, libc::SIGTERM) };
                }
            });
        }

        std::thread::scope(|sc| {
            if let Some(o) = stdout {
                sc.spawn(move || pump_lines(o, |l| on_line(StreamKind::Stdout, l)));
            }
            if let Some(e) = stderr {
                sc.spawn(move || pump_lines(e, |l| on_line(StreamKind::Stderr, l)));
            }
        });

        // Reap without ever holding the pid slot across a blocking wait.
        let status = loop {
            let mut slot = live_pid.lock();
            match child.try_wait() {
                Ok(Some(st)) => {
                    *slot = None;
                    break Ok(st);
                }
                Ok(None) => {}
                Err(e) => {
                    *slot = None;
                    break Err(e);
                }
            }
            drop(slot);
            std::thread::sleep(Duration::from_millis(2));
        };
        if let Some(t) = token {
            t.clear_terminate();
        }
        status.map(exit_code_of)
    }
}

// ---- Binary resolution -----------------------------------------------------

fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Search `name` in the `:`-separated `path_env` (empty components skipped).
pub fn look_path_in(name: &str, path_env: &str) -> Option<String> {
    path_env
        .split(':')
        .filter(|d| !d.is_empty())
        .map(|d| format!("{d}/{name}"))
        .find(|p| is_executable_file(Path::new(p)))
}

/// rclone shipped next to `exe`: for a `.app` bundle
/// (`<App>.app/Contents/MacOS/<exe>`) `Contents/Resources/rclone` then
/// `Contents/MacOS/rclone`, otherwise `<exe dir>/rclone`.
pub fn bundled_binary_for(exe: &Path) -> Option<String> {
    let dir = exe.parent()?;
    let mut candidates: Vec<PathBuf> = Vec::new();
    if dir.file_name().is_some_and(|n| n == "MacOS")
        && let Some(contents) = dir.parent()
        && contents.file_name().is_some_and(|n| n == "Contents")
    {
        candidates.push(contents.join("Resources").join("rclone"));
    }
    candidates.push(dir.join("rclone"));
    candidates
        .into_iter()
        .find(|p| is_executable_file(p))
        .map(|p| p.to_string_lossy().into_owned())
}

/// Pure binary resolution (explicit `path_env` / `exe` for tests).
pub fn resolve_binary_with(
    path: Option<&str>,
    path_env: &str,
    exe: Option<&Path>,
) -> Result<String, RcloneError> {
    match path {
        None | Some("") => {
            // No override: bundled rclone first, PATH as dev fallback.
            if let Some(p) = exe.and_then(bundled_binary_for) {
                return Ok(p);
            }
            look_path_in("rclone", path_env)
                .ok_or_else(|| RcloneError::BinaryNotFound("rclone".into()))
        }
        Some(p) => {
            if let Some(found) = look_path_in(p, path_env) {
                return Ok(found);
            }
            if is_executable_file(Path::new(p)) {
                return Ok(p.to_string());
            }
            Err(RcloneError::BinaryNotFound(p.to_string()))
        }
    }
}

// ---- Client ----------------------------------------------------------------

fn sv(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn run_failed(command: &str, stderr: &str) -> RcloneError {
    RcloneError::RunFailed {
        command: command.to_string(),
        stderr: stderr.to_string(),
    }
}

fn json_i64(v: &Value, key: &str) -> i64 {
    v.get(key)
        .and_then(|x| x.as_i64().or_else(|| x.as_f64().map(|f| f as i64)))
        .unwrap_or(0)
}

fn json_str(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Tolerant `lsjson` decode: missing/null keys default; non-array → empty.
fn parse_file_entries(s: &str) -> Vec<FileEntry> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(s) else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|v| v.is_object())
        .map(|v| FileEntry {
            name: json_str(v, "Name"),
            size: json_i64(v, "Size"),
            mime_type: json_str(v, "MimeType"),
            is_dir: v.get("IsDir").and_then(Value::as_bool).unwrap_or(false),
            mod_time: json_str(v, "ModTime"),
            path: json_str(v, "Path"),
            id: json_str(v, "ID"),
        })
        .collect()
}

fn run_checked(runner: &dyn RcloneRunner, args: &[String]) -> Result<String, RcloneError> {
    let joined = args.join(" ");
    let out = runner
        .run(args)
        .map_err(|e| run_failed(&joined, &e.to_string()))?;
    if out.exit_code != 0 {
        let (e, o) = (out.stderr.trim(), out.stdout.trim());
        let msg = if e.is_empty() { o } else { e };
        return Err(run_failed(&joined, &truncate_string(msg, 500)));
    }
    Ok(out.stdout)
}

fn list_pending_with(
    runner: &dyn RcloneRunner,
    config_path: &str,
    remote_path: &str,
    limit: usize,
) -> Result<Vec<FileEntry>, RcloneError> {
    let p = remote_path.trim();
    if p.is_empty() || !(p.contains(':') || p.starts_with('/')) {
        return Ok(Vec::new());
    }
    let out = run_checked(
        runner,
        &sv(&[
            "--log-level",
            "ERROR",
            "lsjson",
            p,
            "--config",
            config_path,
            "--recursive",
            "--files-only",
        ]),
    )?;
    let mut entries = parse_file_entries(out.trim());
    entries.truncate(limit);
    Ok(entries)
}

pub struct RcloneClient {
    runner: Arc<dyn RcloneRunner>,
    pub binary: String,
    pub config_path: String,
    log: Logger,
}

struct SyncState {
    stats: SyncStats,
    tracker: FileTransferTracker,
    stderr_buf: String,
}

impl RcloneClient {
    /// Resolve the binary (see [`RcloneClient::resolve_binary`]) and wrap a [`ProcessRunner`].
    pub fn new(
        binary_path: Option<&str>,
        config_path: &str,
        log: &Logger,
    ) -> Result<RcloneClient, RcloneError> {
        let bin = Self::resolve_binary(binary_path)?;
        Ok(Self::with_runner(
            Arc::new(ProcessRunner::new(&bin)),
            config_path,
            log,
        ))
    }

    pub fn with_runner(
        runner: Arc<dyn RcloneRunner>,
        config_path: &str,
        log: &Logger,
    ) -> RcloneClient {
        RcloneClient {
            binary: runner.binary(),
            runner,
            config_path: config_path.to_string(),
            log: log.clone(),
        }
    }

    /// rclone shipped with the running executable (bundled build wins over PATH).
    pub fn bundled_binary() -> Option<String> {
        std::env::current_exe()
            .ok()
            .and_then(|e| bundled_binary_for(&e))
    }

    /// Bundled rclone first, then `PATH` (no explicit path); an explicit path is
    /// looked up on `PATH` by name, else must itself be an executable file.
    pub fn resolve_binary(path: Option<&str>) -> Result<String, RcloneError> {
        let path_env = std::env::var("PATH").unwrap_or_default();
        let exe = std::env::current_exe().ok();
        resolve_binary_with(path, &path_env, exe.as_deref())
    }

    fn run(&self, args: &[String]) -> Result<String, RcloneError> {
        run_checked(self.runner.as_ref(), args)
    }

    pub fn version(&self) -> Result<String, RcloneError> {
        let out = self.run(&sv(&["version"]))?;
        Ok(out.split('\n').next().unwrap_or("").trim().to_string())
    }

    /// Run rclone and capture both streams without failing on a non-zero exit.
    pub fn run_raw(&self, args: &[String]) -> Result<ProcessResult, RcloneError> {
        self.runner
            .run(args)
            .map_err(|e| run_failed(&args.join(" "), &e.to_string()))
    }

    /// `rclone config providers` — the backend schema array (PascalCase).
    pub fn providers(&self) -> Result<Vec<Map<String, Value>>, RcloneError> {
        let res = self.run_raw(&sv(&["config", "providers", "--config", &self.config_path]))?;
        if res.exit_code != 0 {
            let msg = res.stderr.trim();
            let msg = if msg.is_empty() {
                res.stdout.as_str()
            } else {
                msg
            };
            return Err(run_failed("config providers", &truncate_string(msg, 500)));
        }
        match serde_json::from_str::<Value>(&res.stdout) {
            Ok(Value::Array(a)) if a.iter().all(Value::is_object) => Ok(a
                .into_iter()
                .filter_map(|v| {
                    if let Value::Object(m) = v {
                        Some(m)
                    } else {
                        None
                    }
                })
                .collect()),
            _ => Err(run_failed("config providers", "invalid JSON output")),
        }
    }

    /// `rclone config create … --non-interactive --obscure`; returns the raw
    /// question JSON (stdout) so the caller can detect OAuth/required options.
    pub fn create_remote_non_interactive(
        &self,
        name: &str,
        r#type: &str,
        config_kvs: &[String],
    ) -> Result<ProcessResult, RcloneError> {
        let mut args = sv(&["config", "create", name, r#type]);
        args.extend(config_kvs.iter().cloned());
        args.extend(sv(&[
            "--non-interactive",
            "--obscure",
            "--config",
            &self.config_path,
        ]));
        self.run_raw(&args)
    }

    /// Continue the config wizard from a prior `State` with one answer.
    pub fn continue_remote_create(
        &self,
        name: &str,
        r#type: &str,
        state: &str,
        result: &str,
        config_kvs: &[String],
    ) -> Result<ProcessResult, RcloneError> {
        let mut args = sv(&["config", "create", name, r#type]);
        args.extend(config_kvs.iter().cloned());
        args.extend(sv(&[
            "--continue",
            "--state",
            state,
            "--result",
            result,
            "--non-interactive",
            "--obscure",
            "--config",
            &self.config_path,
        ]));
        self.run_raw(&args)
    }

    // ---- File ops ----

    /// One-level listing (`remote:path` or absolute local path).
    pub fn list_files(&self, remote_path: &str) -> Result<Vec<FileEntry>, RcloneError> {
        let p = remote_path.trim();
        if p.is_empty() {
            return Err(run_failed("lsjson", "path is required"));
        }
        if !(p.contains(':') || p.starts_with('/')) {
            return Err(run_failed("lsjson", "path must be absolute or remote:path"));
        }
        let out = self.run(&sv(&[
            "--log-level",
            "ERROR",
            "lsjson",
            p,
            "--config",
            &self.config_path,
            "--max-depth",
            "1",
        ]))?;
        Ok(parse_file_entries(out.trim()))
    }

    /// Recursive files-only listing (≤ `limit`) for the Pending tab seed.
    /// Pass [`MAX_TRACKED_FILES`] for the Swift default.
    pub fn list_files_for_pending(
        &self,
        remote_path: &str,
        limit: usize,
    ) -> Result<Vec<FileEntry>, RcloneError> {
        list_pending_with(self.runner.as_ref(), &self.config_path, remote_path, limit)
    }

    pub fn mkdir(&self, remote_path: &str) -> Result<(), RcloneError> {
        self.run(&sv(&["mkdir", remote_path, "--config", &self.config_path]))
            .map(drop)
    }

    pub fn purge(&self, remote_path: &str) -> Result<(), RcloneError> {
        self.run(&sv(&["purge", remote_path, "--config", &self.config_path]))
            .map(drop)
    }

    pub fn delete_file(&self, remote_path: &str) -> Result<(), RcloneError> {
        self.run(&sv(&[
            "deletefile",
            remote_path,
            "--config",
            &self.config_path,
        ]))
        .map(drop)
    }

    pub fn about(&self, remote_name: &str) -> Result<QuotaInfo, RcloneError> {
        let out = self.run(&sv(&[
            "about",
            &format!("{remote_name}:"),
            "--config",
            &self.config_path,
            "--json",
        ]))?;
        match serde_json::from_str::<Value>(&out) {
            Ok(v @ Value::Object(_)) => Ok(QuotaInfo {
                used: json_i64(&v, "used"),
                total: json_i64(&v, "total"),
                free: json_i64(&v, "free"),
            }),
            _ => Err(run_failed("about", "parse error")),
        }
    }

    // ---- Remotes CRUD ----

    pub fn list_remotes(&self) -> Result<Vec<Remote>, RcloneError> {
        let out = match self.run(&sv(&["listremotes", "--config", &self.config_path])) {
            Ok(o) => o,
            Err(RcloneError::RunFailed { command, stderr }) => {
                // Empty/missing config exits non-zero with usage text.
                if stderr.contains("Usage:") || stderr.contains("Available commands:") {
                    return Ok(Vec::new());
                }
                return Err(RcloneError::RunFailed { command, stderr });
            }
            Err(e) => return Err(e),
        };
        let types = self.remote_types_from_dump();
        Ok(out
            .split('\n')
            .filter_map(|line| {
                let name = line.trim();
                let n = name.strip_suffix(':').unwrap_or(name);
                (!n.is_empty()).then(|| Remote {
                    name: n.to_string(),
                    r#type: types.get(n).cloned().unwrap_or_default(),
                    description: String::new(),
                })
            })
            .collect())
    }

    fn remote_types_from_dump(&self) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        let Ok(out) = self.run(&sv(&["config", "dump", "--config", &self.config_path])) else {
            return map;
        };
        let Ok(Value::Object(dump)) = serde_json::from_str::<Value>(&out) else {
            return map;
        };
        for (name, section) in dump {
            if let Some(t) = section.get("type").and_then(Value::as_str) {
                map.insert(name, t.to_string());
            }
        }
        map
    }

    /// Create a remote non-interactively. `config_kvs` are `key=value` pairs.
    pub fn create_remote(
        &self,
        name: &str,
        r#type: &str,
        config_kvs: &[String],
    ) -> Result<(), RcloneError> {
        let mut args = sv(&["config", "create", name, r#type]);
        args.extend(config_kvs.iter().cloned());
        // Never drop into rclone's interactive wizard. OAuth backends would
        // otherwise open a browser and bind :53682; callers that need the
        // wizard use `create_remote_non_interactive` and surface questions.
        args.extend(sv(&[
            "--non-interactive",
            "--obscure",
            "--config",
            &self.config_path,
        ]));
        self.run(&args).map(drop)
    }

    /// Create then probe (`lsd`); deletes the remote if the probe fails.
    pub fn create_remote_verified(
        &self,
        name: &str,
        r#type: &str,
        config_kvs: &[String],
    ) -> Result<(), RcloneError> {
        self.create_remote(name, r#type, config_kvs)?;
        if let Err(e) = self.test_remote(name) {
            let _ = self.delete_remote(name);
            return Err(e);
        }
        Ok(())
    }

    pub fn delete_remote(&self, name: &str) -> Result<(), RcloneError> {
        self.run(&sv(&[
            "config",
            "delete",
            name,
            "--config",
            &self.config_path,
        ]))
        .map(drop)
    }

    pub fn test_remote(&self, name: &str) -> Result<(), RcloneError> {
        self.run(&sv(&[
            "lsd",
            &format!("{name}:"),
            "--config",
            &self.config_path,
            "--max-depth",
            "1",
        ]))
        .map(drop)
    }

    // ---- Sync ----

    /// Run the configured action, streaming progress to `on_progress`.
    #[allow(clippy::result_large_err)]
    pub fn sync(
        &self,
        cfg: &SyncConfig,
        on_progress: Option<ProgressFn<'_>>,
        token: Option<&CancellationToken>,
    ) -> Result<SyncResult, SyncFailedError> {
        let fail = |e: RcloneError| SyncFailedError {
            result: SyncResult::default(),
            message: e.to_string(),
            cancelled: false,
        };
        let args = self.build_args(cfg).map_err(fail)?;
        self.log
            .debug("rclone sync", &[("action", cfg.action.as_str())]);
        let seed_path = pending_seed_path(cfg);
        self.execute(&args, on_progress, &seed_path, token)
    }

    /// Full rclone argument list for `cfg` (without the binary).
    pub fn build_args(&self, cfg: &SyncConfig) -> Result<Vec<String>, RcloneError> {
        let (src, dst) = resolve_endpoints(cfg)?;
        let interval = if cfg.stats_interval.is_empty() {
            "1s"
        } else {
            cfg.stats_interval.as_str()
        };
        let base = sv(&[
            "--config",
            &self.config_path,
            "--stats",
            interval,
            "--use-json-log",
            "-v",
        ]);
        let mut args = match cfg.action {
            RcloneAction::Pull => sv(&["sync", &dst, &src, "--update"]),
            RcloneAction::Push => sv(&["sync", &src, &dst, "--update"]),
            RcloneAction::Bi => sv(&["bisync", &src, &dst]),
            RcloneAction::BiResync => sv(&["bisync", &src, &dst, "--resync", "--force"]),
            RcloneAction::Copy => sv(&["copy", &src, &dst]),
            RcloneAction::Move => sv(&["move", &src, &dst]),
            RcloneAction::Check => sv(&["check", &src, &dst]),
            RcloneAction::DryRun => sv(&["sync", &src, &dst, "--dry-run", "--update"]),
        };
        args.extend(base);
        if let Some(p) = &cfg.profile {
            args.extend(Self::profile_to_flags(p));
        }
        Ok(args)
    }

    pub fn profile_to_flags(p: &ProfileFlags) -> Vec<String> {
        let mut f: Vec<String> = Vec::new();
        if !p.bandwidth.is_empty() {
            kv(&mut f, "--bwlimit", &p.bandwidth);
        }
        if p.transfers > 0 {
            kv(&mut f, "--transfers", &p.transfers.to_string());
        }
        if p.checkers > 0 {
            kv(&mut f, "--checkers", &p.checkers.to_string());
        }
        if p.tps_limit > 0.0 {
            // Debug formatting matches Swift's `String(Double)` ("4.0").
            kv(&mut f, "--tpslimit", &format!("{:?}", p.tps_limit));
        }
        if !p.min_age.is_empty() {
            kv(&mut f, "--min-age", &p.min_age);
        }
        if !p.max_age.is_empty() {
            kv(&mut f, "--max-age", &p.max_age);
        }
        if !p.min_size.is_empty() {
            kv(&mut f, "--min-size", &p.min_size);
        }
        if !p.max_size.is_empty() {
            kv(&mut f, "--max-size", &p.max_size);
        }
        if !p.exclude_if_present.is_empty() {
            kv(&mut f, "--exclude-if-present", &p.exclude_if_present);
        }
        if p.max_delete > 0 {
            kv(&mut f, "--max-delete", &p.max_delete.to_string());
        }
        if p.dry_run {
            f.push("--dry-run".to_string());
        }
        if p.no_unicode_normalize {
            f.push("--no-unicode-normalization".to_string());
        }
        for inc in p
            .includes
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            kv(&mut f, "--include", inc);
        }
        for exc in p
            .excludes
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            kv(&mut f, "--exclude", exc);
        }
        if p.multi_thread_streams > 0 {
            kv(
                &mut f,
                "--multi-thread-streams",
                &p.multi_thread_streams.to_string(),
            );
        }
        if !p.buffer_size.is_empty() {
            kv(&mut f, "--buffer-size", &p.buffer_size);
        }
        if p.retries > 0 {
            kv(&mut f, "--retries", &p.retries.to_string());
        }
        if p.low_level_retries > 0 {
            kv(
                &mut f,
                "--low-level-retries",
                &p.low_level_retries.to_string(),
            );
        }
        if !p.max_duration.is_empty() {
            kv(&mut f, "--max-duration", &p.max_duration);
        }
        if !p.retries_sleep.is_empty() {
            kv(&mut f, "--retries-sleep", &p.retries_sleep);
        }
        if !p.conn_timeout.is_empty() {
            kv(&mut f, "--contimeout", &p.conn_timeout);
        }
        if !p.io_timeout.is_empty() {
            kv(&mut f, "--timeout", &p.io_timeout);
        }
        if !p.order_by.is_empty() {
            kv(&mut f, "--order-by", &p.order_by);
        }
        if p.check_first {
            f.push("--check-first".to_string());
        }
        if p.immutable {
            f.push("--immutable".to_string());
        }
        if !p.max_transfer.is_empty() {
            kv(&mut f, "--max-transfer", &p.max_transfer);
        }
        if !p.max_delete_size.is_empty() {
            kv(&mut f, "--max-delete-size", &p.max_delete_size);
        }
        if !p.suffix.is_empty() {
            kv(&mut f, "--suffix", &p.suffix);
        }
        if p.suffix_keep_extension {
            f.push("--suffix-keep-extension".to_string());
        }
        if !p.backup_dir.is_empty() {
            kv(&mut f, "--backup-dir", &p.backup_dir);
        }
        if p.size_only {
            f.push("--size-only".to_string());
        }
        if p.update_mode {
            f.push("--update".to_string());
        }
        if p.ignore_existing {
            f.push("--ignore-existing".to_string());
        }
        if p.delete_excluded {
            f.push("--delete-excluded".to_string());
        }
        if p.max_depth > 0 {
            kv(&mut f, "--max-depth", &p.max_depth.to_string());
        }
        match p.delete_timing.trim().to_lowercase().as_str() {
            "before" => f.push("--delete-before".to_string()),
            "after" => f.push("--delete-after".to_string()),
            "during" => f.push("--delete-during".to_string()),
            _ => {}
        }
        if !p.conflict_resolve.is_empty() {
            kv(&mut f, "--conflict-resolve", &p.conflict_resolve);
        }
        if !p.conflict_loser.is_empty() {
            kv(&mut f, "--conflict-loser", &p.conflict_loser);
        }
        if !p.conflict_suffix.is_empty() {
            kv(&mut f, "--conflict-suffix", &p.conflict_suffix);
        }
        if p.resilient {
            f.push("--resilient".to_string());
        }
        if !p.max_lock.is_empty() {
            kv(&mut f, "--max-lock", &p.max_lock);
        }
        if p.check_access {
            f.push("--check-access".to_string());
        }
        f
    }

    /// Run rclone with `args`; parse `--use-json-log` lines (and legacy text
    /// stats) from either stream, emit progress, track per-file status.
    #[allow(clippy::result_large_err)]
    fn execute(
        &self,
        args: &[String],
        on_progress: Option<ProgressFn<'_>>,
        seed_path: &str,
        token: Option<&CancellationToken>,
    ) -> Result<SyncResult, SyncFailedError> {
        let mut result = SyncResult {
            started_at: unix_now(),
            exit_code: -1,
            ..Default::default()
        };
        let state = Mutex::new(SyncState {
            stats: SyncStats {
                stage: "starting".into(),
                ..Default::default()
            },
            tracker: FileTransferTracker::new(),
            stderr_buf: String::new(),
        });
        let emit = || {
            let snap = {
                let mut st = state.lock();
                st.stats.file_transfers = st.tracker.snapshot(st.stats.files_total);
                let mut snap = st.stats.clone();
                snap.last_update = unix_now();
                snap
            };
            if let Some(cb) = on_progress {
                cb(snap);
            }
        };
        // Immediate lifecycle marker for the pre-stats gap.
        emit();

        // Concurrent pending seed (bounded to 20s, discarded once the run ends).
        let seed_cancel = Arc::new(AtomicBool::new(false));
        let mut seed_rx = crossbeam_channel::never::<Vec<FileEntry>>();
        if !seed_path.is_empty() {
            let (tx, rx) = crossbeam_channel::bounded::<Vec<FileEntry>>(1);
            seed_rx = rx;
            let runner = self.runner.clone();
            let config = self.config_path.clone();
            let path = seed_path.to_string();
            let cancel = seed_cancel.clone();
            std::thread::spawn(move || {
                let started = Instant::now();
                if let Ok(entries) =
                    list_pending_with(runner.as_ref(), &config, &path, MAX_TRACKED_FILES)
                    && !entries.is_empty()
                    && started.elapsed() < Duration::from_secs(20)
                    && !cancel.load(Ordering::SeqCst)
                {
                    let _ = tx.send(entries);
                }
            });
        }

        let on_line = |kind: StreamKind, line: &str| {
            {
                let mut guard = state.lock();
                let st = &mut *guard;
                if kind == StreamKind::Stderr {
                    st.stderr_buf.push_str(line);
                    st.stderr_buf.push('\n');
                }
                if !parse_json_stats_line(line, &mut st.stats) {
                    parse_stats_line(line, &mut st.stats);
                }
                update_stage(line, &mut st.stats);
                ingest_json_log_line(line, &mut st.stats, &mut st.tracker);
            }
            emit();
        };

        let (done_tx, done_rx) = crossbeam_channel::bounded::<std::io::Result<i32>>(1);
        let apply_seed = |entries: Vec<FileEntry>| {
            state.lock().tracker.seed_pending(&entries);
            emit();
        };
        let io_result = std::thread::scope(|sc| {
            let runner = self.runner.as_ref();
            sc.spawn(|| {
                let _ = done_tx.send(runner.run_streaming(args, token, &on_line));
            });
            loop {
                crossbeam_channel::select! {
                    recv(seed_rx) -> msg => match msg {
                        Ok(entries) => apply_seed(entries),
                        Err(_) => seed_rx = crossbeam_channel::never(),
                    },
                    recv(done_rx) -> r => break r.unwrap_or_else(|_| Err(std::io::Error::other("rclone runner thread died"))),
                }
            }
        });
        seed_cancel.store(true, Ordering::SeqCst);
        if let Ok(entries) = seed_rx.try_recv() {
            state.lock().tracker.seed_pending(&entries);
        }

        result.ended_at = unix_now();
        {
            let mut st = state.lock();
            result.stderr = st.stderr_buf.clone();
            st.stats.file_transfers = st.tracker.snapshot(st.stats.files_total);
            result.stats = st.stats.clone();
        }
        let code = match io_result {
            Ok(c) => c,
            Err(e) => {
                let message = format!("rclone {}: {}", args.join(" "), e);
                return Err(SyncFailedError {
                    result,
                    message,
                    cancelled: false,
                });
            }
        };
        result.exit_code = code;

        if token.is_some_and(|t| t.is_cancelled()) {
            return Err(SyncFailedError {
                result,
                message: "cancelled".into(),
                cancelled: true,
            });
        }
        if code != 0 {
            let message = format!(
                "rclone {}: exit {} (stderr: {})",
                args.join(" "),
                code,
                truncate_string(&result.stderr, 500)
            );
            return Err(SyncFailedError {
                result,
                message,
                cancelled: false,
            });
        }
        Ok(result)
    }
}

fn kv(f: &mut Vec<String>, flag: &str, val: &str) {
    f.push(flag.to_string());
    f.push(val.to_string());
}

impl SyncClient for RcloneClient {
    #[allow(clippy::result_large_err)]
    fn sync(
        &self,
        cfg: &SyncConfig,
        on_progress: Option<ProgressFn<'_>>,
        token: Option<&CancellationToken>,
    ) -> Result<SyncResult, SyncFailedError> {
        RcloneClient::sync(self, cfg, on_progress, token)
    }
}

/// Path to list for the Pending-tab seed: dest for pull, otherwise source.
pub fn pending_seed_path(cfg: &SyncConfig) -> String {
    let (mut src, mut dst) = (cfg.source.clone(), cfg.dest.clone());
    if src.is_empty() || dst.is_empty() {
        if !cfg.source_remote.is_empty() && !cfg.source_path.is_empty() {
            src = format!("{}:{}", cfg.source_remote, cfg.source_path);
        }
        if !cfg.dest_remote.is_empty() && !cfg.dest_path.is_empty() {
            dst = format!("{}:{}", cfg.dest_remote, cfg.dest_path);
        }
    }
    if cfg.action == RcloneAction::Pull {
        dst
    } else {
        src
    }
}

fn resolve_endpoints(cfg: &SyncConfig) -> Result<(String, String), RcloneError> {
    if !cfg.source.is_empty() && !cfg.dest.is_empty() {
        return Ok((cfg.source.clone(), cfg.dest.clone()));
    }
    if cfg.source_remote.is_empty()
        || cfg.source_path.is_empty()
        || cfg.dest_remote.is_empty()
        || cfg.dest_path.is_empty()
    {
        return Err(run_failed("sync", "requires Source+Dest or remotes+paths"));
    }
    Ok((
        format!("{}:{}", cfg.source_remote, cfg.source_path),
        format!("{}:{}", cfg.dest_remote, cfg.dest_path),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rclone::testing::{FakeRunner, Response};
    use std::os::unix::fs::PermissionsExt;

    fn client(fake: &Arc<FakeRunner>) -> RcloneClient {
        RcloneClient::with_runner(fake.clone(), "/cfg/rclone.conf", &Logger::quiet())
    }

    fn cfg(action: RcloneAction) -> SyncConfig {
        SyncConfig {
            action,
            source: "/src".into(),
            dest: "r:dst".into(),
            ..Default::default()
        }
    }

    fn write_exe(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    // ---- build_args ----

    #[test]
    fn build_args_per_action() {
        let fake = Arc::new(FakeRunner::new());
        let c = client(&fake);
        let tail = [
            "--config",
            "/cfg/rclone.conf",
            "--stats",
            "1s",
            "--use-json-log",
            "-v",
        ];
        let cases: [(RcloneAction, Vec<&str>); 8] = [
            (
                RcloneAction::Push,
                vec!["sync", "/src", "r:dst", "--update"],
            ),
            (
                RcloneAction::Pull,
                vec!["sync", "r:dst", "/src", "--update"],
            ),
            (RcloneAction::Bi, vec!["bisync", "/src", "r:dst"]),
            (
                RcloneAction::BiResync,
                vec!["bisync", "/src", "r:dst", "--resync", "--force"],
            ),
            (RcloneAction::Copy, vec!["copy", "/src", "r:dst"]),
            (RcloneAction::Move, vec!["move", "/src", "r:dst"]),
            (RcloneAction::Check, vec!["check", "/src", "r:dst"]),
            (
                RcloneAction::DryRun,
                vec!["sync", "/src", "r:dst", "--dry-run", "--update"],
            ),
        ];
        for (action, head) in cases {
            let want: Vec<String> = head
                .iter()
                .chain(tail.iter())
                .map(|s| s.to_string())
                .collect();
            assert_eq!(c.build_args(&cfg(action)).unwrap(), want, "{action:?}");
        }
    }

    #[test]
    fn build_args_remote_paths_interval_and_errors() {
        let fake = Arc::new(FakeRunner::new());
        let c = client(&fake);
        let mut cf = SyncConfig {
            action: RcloneAction::Copy,
            source_remote: "a".into(),
            source_path: "x/y".into(),
            dest_remote: "b".into(),
            dest_path: "z".into(),
            stats_interval: "5s".into(),
            ..Default::default()
        };
        let args = c.build_args(&cf).unwrap();
        assert_eq!(&args[..3], ["copy", "a:x/y", "b:z"]);
        assert_eq!(args[6], "5s");
        cf.stats_interval.clear();
        assert_eq!(c.build_args(&cf).unwrap()[6], "1s");
        cf.dest_path.clear();
        let err = c.build_args(&cf).unwrap_err();
        assert_eq!(
            err.to_string(),
            "rclone sync: requires Source+Dest or remotes+paths"
        );
        // Explicit source+dest wins over remotes.
        let both = SyncConfig {
            source: "s".into(),
            dest: "d".into(),
            ..cf
        };
        assert_eq!(&c.build_args(&both).unwrap()[..3], ["copy", "s", "d"]);
    }

    #[test]
    fn build_args_appends_profile_flags_last() {
        let fake = Arc::new(FakeRunner::new());
        let c = client(&fake);
        let mut cf = cfg(RcloneAction::Push);
        cf.profile = Some(ProfileFlags {
            bandwidth: "1M".into(),
            transfers: 3,
            ..Default::default()
        });
        let args = c.build_args(&cf).unwrap();
        let n = args.len();
        assert_eq!(&args[n - 4..], ["--bwlimit", "1M", "--transfers", "3"]);
        assert_eq!(args[n - 5], "-v");
    }

    #[test]
    fn profile_to_flags_empty_and_order() {
        assert!(RcloneClient::profile_to_flags(&ProfileFlags::default()).is_empty());
        let p = ProfileFlags {
            use_list_r: true,
            bandwidth: "10M".into(),
            transfers: 4,
            checkers: 8,
            tps_limit: 4.0,
            min_age: "1d".into(),
            max_age: "2d".into(),
            min_size: "1k".into(),
            max_size: "1G".into(),
            exclude_if_present: ".nosync".into(),
            max_delete: 5,
            dry_run: true,
            no_unicode_normalize: true,
            includes: vec![" *.txt ".into(), "  ".into()],
            excludes: vec!["tmp/**".into()],
            multi_thread_streams: 2,
            buffer_size: "16M".into(),
            retries: 3,
            low_level_retries: 10,
            max_duration: "1h".into(),
            retries_sleep: "2s".into(),
            conn_timeout: "30s".into(),
            io_timeout: "5m".into(),
            order_by: "size".into(),
            check_first: true,
            immutable: true,
            max_transfer: "5G".into(),
            max_delete_size: "1G".into(),
            suffix: ".bak".into(),
            suffix_keep_extension: true,
            backup_dir: "bk".into(),
            size_only: true,
            update_mode: true,
            ignore_existing: true,
            delete_excluded: true,
            max_depth: 3,
            delete_timing: " After ".into(),
            conflict_resolve: "newer".into(),
            conflict_loser: "delete".into(),
            conflict_suffix: "conflict".into(),
            resilient: true,
            max_lock: "2m".into(),
            check_access: true,
        };
        let want = [
            "--bwlimit",
            "10M",
            "--transfers",
            "4",
            "--checkers",
            "8",
            "--tpslimit",
            "4.0",
            "--min-age",
            "1d",
            "--max-age",
            "2d",
            "--min-size",
            "1k",
            "--max-size",
            "1G",
            "--exclude-if-present",
            ".nosync",
            "--max-delete",
            "5",
            "--dry-run",
            "--no-unicode-normalization",
            "--include",
            "*.txt",
            "--exclude",
            "tmp/**",
            "--multi-thread-streams",
            "2",
            "--buffer-size",
            "16M",
            "--retries",
            "3",
            "--low-level-retries",
            "10",
            "--max-duration",
            "1h",
            "--retries-sleep",
            "2s",
            "--contimeout",
            "30s",
            "--timeout",
            "5m",
            "--order-by",
            "size",
            "--check-first",
            "--immutable",
            "--max-transfer",
            "5G",
            "--max-delete-size",
            "1G",
            "--suffix",
            ".bak",
            "--suffix-keep-extension",
            "--backup-dir",
            "bk",
            "--size-only",
            "--update",
            "--ignore-existing",
            "--delete-excluded",
            "--max-depth",
            "3",
            "--delete-after",
            "--conflict-resolve",
            "newer",
            "--conflict-loser",
            "delete",
            "--conflict-suffix",
            "conflict",
            "--resilient",
            "--max-lock",
            "2m",
            "--check-access",
        ];
        assert_eq!(RcloneClient::profile_to_flags(&p), want);
        for (t, flag) in [("before", "--delete-before"), ("DURING", "--delete-during")] {
            let p = ProfileFlags {
                delete_timing: t.into(),
                ..Default::default()
            };
            assert_eq!(RcloneClient::profile_to_flags(&p), [flag]);
        }
        let p = ProfileFlags {
            delete_timing: "bogus".into(),
            tps_limit: 0.5,
            ..Default::default()
        };
        assert_eq!(RcloneClient::profile_to_flags(&p), ["--tpslimit", "0.5"]);
    }

    #[test]
    fn pending_seed_path_by_action() {
        assert_eq!(pending_seed_path(&cfg(RcloneAction::Push)), "/src");
        assert_eq!(pending_seed_path(&cfg(RcloneAction::Pull)), "r:dst");
        let c = SyncConfig {
            source_remote: "a".into(),
            source_path: "p".into(),
            dest_remote: "b".into(),
            dest_path: "q".into(),
            ..Default::default()
        };
        assert_eq!(pending_seed_path(&c), "a:p");
        let c = SyncConfig {
            action: RcloneAction::Pull,
            ..c
        };
        assert_eq!(pending_seed_path(&c), "b:q");
    }

    // ---- binary resolution ----

    #[test]
    fn resolve_binary_explicit_and_path_lookup() {
        let d = tempfile::tempdir().unwrap();
        let bin = write_exe(d.path(), "my-rclone");
        let dir = d.path().to_str().unwrap();
        // Name found via the explicit PATH string.
        assert_eq!(
            resolve_binary_with(Some("my-rclone"), dir, None).unwrap(),
            format!("{dir}/my-rclone")
        );
        // Absolute path: not a PATH hit, but an executable file.
        assert_eq!(
            resolve_binary_with(Some(bin.to_str().unwrap()), "", None).unwrap(),
            bin.to_str().unwrap()
        );
        // Missing / not executable.
        let nonexec = d.path().join("plain");
        std::fs::write(&nonexec, "x").unwrap();
        for bad in ["nope", nonexec.to_str().unwrap()] {
            match resolve_binary_with(Some(bad), dir, None) {
                Err(RcloneError::BinaryNotFound(p)) => assert_eq!(p, bad),
                other => panic!("{other:?}"),
            }
        }
        // Directories are not executables.
        assert!(resolve_binary_with(Some(dir), "", None).is_err());
    }

    #[test]
    fn resolve_binary_default_prefers_bundled_then_path() {
        let d = tempfile::tempdir().unwrap();
        let path_dir = d.path().join("pathdir");
        let app_dir = d.path().join("appdir");
        std::fs::create_dir_all(&path_dir).unwrap();
        std::fs::create_dir_all(&app_dir).unwrap();
        let path_env = format!("/nonexistent:{}", path_dir.display());
        // Nothing anywhere.
        let exe = app_dir.join("gn-drive");
        match resolve_binary_with(None, &path_env, Some(&exe)) {
            Err(RcloneError::BinaryNotFound(p)) => assert_eq!(p, "rclone"),
            other => panic!("{other:?}"),
        }
        // PATH fallback (empty string explicit path behaves like None).
        write_exe(&path_dir, "rclone");
        assert_eq!(
            resolve_binary_with(Some(""), &path_env, Some(&exe)).unwrap(),
            format!("{}/rclone", path_dir.display())
        );
        // Bundled next to the executable wins.
        let bundled = write_exe(&app_dir, "rclone");
        assert_eq!(
            resolve_binary_with(None, &path_env, Some(&exe)).unwrap(),
            bundled.to_str().unwrap()
        );
    }

    #[test]
    fn bundled_binary_for_app_bundle_layout() {
        let d = tempfile::tempdir().unwrap();
        let macos = d.path().join("GN Drive.app/Contents/MacOS");
        let res = d.path().join("GN Drive.app/Contents/Resources");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::create_dir_all(&res).unwrap();
        let exe = macos.join("GNDrive");
        assert_eq!(bundled_binary_for(&exe), None);
        let in_macos = write_exe(&macos, "rclone");
        assert_eq!(
            bundled_binary_for(&exe).unwrap(),
            in_macos.to_str().unwrap()
        );
        // Resources has priority.
        let in_res = write_exe(&res, "rclone");
        assert_eq!(bundled_binary_for(&exe).unwrap(), in_res.to_str().unwrap());
        // A plain directory named MacOS outside Contents doesn't consult Resources.
        let plain = d.path().join("x/MacOS");
        std::fs::create_dir_all(d.path().join("x/Resources")).unwrap();
        std::fs::create_dir_all(&plain).unwrap();
        write_exe(&d.path().join("x/Resources"), "rclone");
        assert_eq!(bundled_binary_for(&plain.join("app")), None);
    }

    #[test]
    fn new_with_explicit_binary_and_not_found() {
        let d = tempfile::tempdir().unwrap();
        let bin = write_exe(d.path(), "fake-rclone");
        let c =
            RcloneClient::new(Some(bin.to_str().unwrap()), "/c.conf", &Logger::quiet()).unwrap();
        assert_eq!(c.binary, bin.to_str().unwrap());
        assert_eq!(c.config_path, "/c.conf");
        let e = RcloneClient::new(Some("/definitely/not/here"), "/c", &Logger::quiet())
            .err()
            .unwrap();
        assert_eq!(
            e.to_string(),
            "rclone: binary not found: /definitely/not/here"
        );
    }

    // ---- simple ops via FakeRunner ----

    #[test]
    fn version_takes_first_line() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(
            &["version"],
            Response::new()
                .stdout("rclone v1.70.0")
                .stdout("- os/version: darwin"),
        );
        assert_eq!(client(&fake).version().unwrap(), "rclone v1.70.0");
        let fail = Arc::new(FakeRunner::new());
        fail.on(&["version"], Response::new().exit(1).stderr("boom"));
        assert_eq!(
            client(&fail).version().unwrap_err().to_string(),
            "rclone version: boom"
        );
    }

    #[test]
    fn run_failure_prefers_stderr_else_stdout_and_truncates() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(&["mkdir"], Response::new().exit(1).stdout("from stdout"));
        let e = client(&fake).mkdir("r:x").unwrap_err();
        assert_eq!(
            e.to_string(),
            "rclone mkdir r:x --config /cfg/rclone.conf: from stdout"
        );
        let long = "e".repeat(600);
        fake.on(&["purge"], Response::new().exit(1).stderr(long));
        let msg = client(&fake).purge("r:x").unwrap_err().to_string();
        assert!(msg.ends_with(&format!("{}...(truncated)", "e".repeat(500))));
    }

    #[test]
    fn file_ops_args() {
        let fake = Arc::new(FakeRunner::new());
        let c = client(&fake);
        c.mkdir("r:a").unwrap();
        c.purge("r:b").unwrap();
        c.delete_file("r:c/f").unwrap();
        c.delete_remote("old").unwrap();
        c.test_remote("t").unwrap();
        c.create_remote("n", "local", &["a=1".into(), "b=2".into()])
            .unwrap();
        let calls = fake.calls();
        let cfgp = ["--config", "/cfg/rclone.conf"];
        assert_eq!(
            calls[0],
            ["mkdir", "r:a"]
                .iter()
                .chain(&cfgp)
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(calls[1][..2], ["purge", "r:b"]);
        assert_eq!(calls[2][..2], ["deletefile", "r:c/f"]);
        assert_eq!(calls[3][..3], ["config", "delete", "old"]);
        assert_eq!(
            calls[4],
            [
                "lsd",
                "t:",
                "--config",
                "/cfg/rclone.conf",
                "--max-depth",
                "1"
            ]
        );
        assert_eq!(
            calls[5],
            [
                "config",
                "create",
                "n",
                "local",
                "a=1",
                "b=2",
                "--non-interactive",
                "--obscure",
                "--config",
                "/cfg/rclone.conf"
            ]
        );
    }

    #[test]
    fn list_files_validation_and_parsing() {
        let fake = Arc::new(FakeRunner::new());
        let c = client(&fake);
        assert_eq!(
            c.list_files("  ").unwrap_err().to_string(),
            "rclone lsjson: path is required"
        );
        assert_eq!(
            c.list_files("relative/dir").unwrap_err().to_string(),
            "rclone lsjson: path must be absolute or remote:path"
        );
        assert_eq!(fake.call_count(), 0);

        fake.on(
            &["--log-level", "ERROR", "lsjson"],
            Response::new().stdout(
                r#"[{"Path":"a.txt","Name":"a.txt","Size":12,"MimeType":"text/plain","ModTime":"2025-01-01T00:00:00Z","IsDir":false,"ID":"x1"},{"Name":"sub","IsDir":true},{"Name":"nosize","Size":null}]"#,
            ),
        );
        let l = c.list_files(" r:dir ").unwrap();
        assert_eq!(l.len(), 3);
        assert_eq!(
            (l[0].name.as_str(), l[0].size, l[0].id.as_str()),
            ("a.txt", 12, "x1")
        );
        assert!(l[1].is_dir && l[1].size == 0 && l[1].path.is_empty());
        assert_eq!(l[2].size, 0);
        assert_eq!(
            fake.last_call().unwrap(),
            [
                "--log-level",
                "ERROR",
                "lsjson",
                "r:dir",
                "--config",
                "/cfg/rclone.conf",
                "--max-depth",
                "1"
            ]
        );
    }

    #[test]
    fn list_files_tolerates_garbage_and_stderr_notices() {
        let fake = Arc::new(FakeRunner::new());
        let c = client(&fake);
        fake.on(&["--log-level"], Response::new().stdout("not json"));
        assert!(c.list_files("/x").unwrap().is_empty());
        let fake2 = Arc::new(FakeRunner::new());
        fake2.on(
            &["--log-level"],
            Response::new().stderr("NOTICE: something").stdout("[]"),
        );
        assert!(client(&fake2).list_files("/x").unwrap().is_empty());
        let fake3 = Arc::new(FakeRunner::new());
        fake3.on(
            &["--log-level"],
            Response::new().exit(3).stderr("directory not found"),
        );
        assert!(
            client(&fake3)
                .list_files("/x")
                .unwrap_err()
                .to_string()
                .contains("directory not found")
        );
    }

    #[test]
    fn list_files_for_pending_limit_and_guards() {
        let fake = Arc::new(FakeRunner::new());
        let items: Vec<String> = (0..10)
            .map(|i| format!(r#"{{"Path":"f{i}","Name":"f{i}","Size":1}}"#))
            .collect();
        fake.on(
            &["--log-level", "ERROR", "lsjson"],
            Response::new().stdout(format!("[{}]", items.join(","))),
        );
        let c = client(&fake);
        assert_eq!(c.list_files_for_pending("/d", 4).unwrap().len(), 4);
        assert_eq!(
            c.list_files_for_pending("/d", MAX_TRACKED_FILES)
                .unwrap()
                .len(),
            10
        );
        assert!(
            fake.last_call()
                .unwrap()
                .ends_with(&sv(&["--recursive", "--files-only"]))
        );
        let before = fake.call_count();
        assert!(c.list_files_for_pending("relative", 5).unwrap().is_empty());
        assert!(c.list_files_for_pending(" ", 5).unwrap().is_empty());
        assert_eq!(fake.call_count(), before);
    }

    #[test]
    fn about_parses_and_errors() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(
            &["about"],
            Response::new().stdout(r#"{"total":100,"used":40}"#),
        );
        let q = client(&fake).about("gd").unwrap();
        assert_eq!(
            q,
            QuotaInfo {
                used: 40,
                total: 100,
                free: 0
            }
        );
        assert_eq!(
            fake.last_call().unwrap(),
            ["about", "gd:", "--config", "/cfg/rclone.conf", "--json"]
        );
        let bad = Arc::new(FakeRunner::new());
        bad.on(&["about"], Response::new().stdout("nope"));
        assert_eq!(
            client(&bad).about("gd").unwrap_err().to_string(),
            "rclone about: parse error"
        );
        let failing = Arc::new(FakeRunner::new());
        failing.on(&["about"], Response::new().exit(1).stderr("no quota"));
        assert!(client(&failing).about("gd").is_err());
    }

    // ---- list_remotes ----

    #[test]
    fn list_remotes_with_types() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(
            &["listremotes"],
            Response::new()
                .stdout("gdrive:")
                .stdout("  local:  ")
                .stdout("")
                .stdout(":")
                .stdout("bare"),
        );
        fake.on(
            &["config", "dump"],
            Response::new().stdout(r#"{"gdrive":{"type":"drive","scope":"drive"},"local":{"type":"alias"},"x":{"nottype":1}}"#),
        );
        let r = client(&fake).list_remotes().unwrap();
        let got: Vec<(&str, &str)> = r
            .iter()
            .map(|r| (r.name.as_str(), r.r#type.as_str()))
            .collect();
        assert_eq!(got, [("gdrive", "drive"), ("local", "alias"), ("bare", "")]);
        assert_eq!(
            fake.calls()[0],
            ["listremotes", "--config", "/cfg/rclone.conf"]
        );
    }

    #[test]
    fn list_remotes_dump_failure_gives_empty_types() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(&["listremotes"], Response::new().stdout("a:"));
        fake.on(&["config", "dump"], Response::new().exit(1).stderr("bad"));
        let r = client(&fake).list_remotes().unwrap();
        assert_eq!(
            r,
            vec![Remote {
                name: "a".into(),
                r#type: String::new(),
                description: String::new()
            }]
        );
    }

    #[test]
    fn list_remotes_usage_error_is_empty_other_errors_propagate() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(
            &["listremotes"],
            Response::new()
                .exit(1)
                .stderr("Usage:\n  rclone listremotes"),
        );
        assert!(client(&fake).list_remotes().unwrap().is_empty());
        let fake = Arc::new(FakeRunner::new());
        fake.on(
            &["listremotes"],
            Response::new().exit(1).stderr("Available commands: x"),
        );
        assert!(client(&fake).list_remotes().unwrap().is_empty());
        let fake = Arc::new(FakeRunner::new());
        fake.on(
            &["listremotes"],
            Response::new().exit(1).stderr("permission denied"),
        );
        assert!(
            client(&fake)
                .list_remotes()
                .unwrap_err()
                .to_string()
                .contains("permission denied")
        );
    }

    #[test]
    fn create_remote_verified_rolls_back_on_probe_failure() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(&["lsd"], Response::new().exit(1).stderr("auth failed"));
        let e = client(&fake)
            .create_remote_verified("r", "drive", &[])
            .unwrap_err();
        assert!(e.to_string().contains("auth failed"));
        let cmds: Vec<String> = fake.calls().iter().map(|c| c[..2].join(" ")).collect();
        assert_eq!(cmds, ["config create", "lsd r:", "config delete"]);

        let ok = Arc::new(FakeRunner::new());
        client(&ok)
            .create_remote_verified("r", "local", &[])
            .unwrap();
        assert_eq!(ok.call_count(), 2);

        let create_fails = Arc::new(FakeRunner::new());
        create_fails.on(
            &["config", "create"],
            Response::new().exit(1).stderr("exists"),
        );
        assert!(
            client(&create_fails)
                .create_remote_verified("r", "local", &[])
                .is_err()
        );
        assert_eq!(create_fails.call_count(), 1);
    }

    // ---- wizard helpers / providers ----

    #[test]
    fn wizard_helpers_build_args_and_return_raw() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(
            &["config", "create"],
            Response::new().stdout(r#"{"State":"s1"}"#).exit(0),
        );
        let c = client(&fake);
        let r = c
            .create_remote_non_interactive("n", "drive", &["a=b".into()])
            .unwrap();
        assert_eq!(r.stdout, "{\"State\":\"s1\"}\n");
        assert_eq!(
            fake.last_call().unwrap(),
            [
                "config",
                "create",
                "n",
                "drive",
                "a=b",
                "--non-interactive",
                "--obscure",
                "--config",
                "/cfg/rclone.conf"
            ]
        );
        c.continue_remote_create("n", "drive", "st", "true", &[])
            .unwrap();
        assert_eq!(
            fake.last_call().unwrap(),
            [
                "config",
                "create",
                "n",
                "drive",
                "--continue",
                "--state",
                "st",
                "--result",
                "true",
                "--non-interactive",
                "--obscure",
                "--config",
                "/cfg/rclone.conf"
            ]
        );
        // Non-zero exit is reported, not raised.
        let f2 = Arc::new(FakeRunner::new());
        f2.on(&["config"], Response::new().exit(2).stderr("q"));
        let r = client(&f2).run_raw(&sv(&["config", "x"])).unwrap();
        assert_eq!((r.exit_code, r.stderr.as_str()), (2, "q\n"));
    }

    #[test]
    fn providers_parsing_and_errors() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(
            &["config", "providers"],
            Response::new().stdout(r#"[{"Name":"drive","Options":[]},{"Name":"s3"}]"#),
        );
        let p = client(&fake).providers().unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0]["Name"], "drive");
        let bad = Arc::new(FakeRunner::new());
        bad.on(&["config", "providers"], Response::new().stdout("{}"));
        assert_eq!(
            client(&bad).providers().unwrap_err().to_string(),
            "rclone config providers: invalid JSON output"
        );
        let fail = Arc::new(FakeRunner::new());
        fail.on(
            &["config", "providers"],
            Response::new().exit(1).stdout("only stdout"),
        );
        assert_eq!(
            client(&fail).providers().unwrap_err().to_string(),
            "rclone config providers: only stdout\n"
        );
    }

    // ---- sync via FakeRunner ----

    fn collect_progress() -> (Arc<Mutex<Vec<SyncStats>>>, impl Fn(SyncStats) + Send + Sync) {
        let store = Arc::new(Mutex::new(Vec::new()));
        let s2 = store.clone();
        (store, move |s| s2.lock().push(s))
    }

    #[test]
    fn sync_progress_from_json_stats() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(
            &["sync"],
            Response::new()
                .stderr(r#"{"level":"info","msg":"Copied (new)","object":"photo.jpg","size":9000}"#)
                .stderr(r#"{"level":"info","msg":"stats","stats":{"bytes":4500,"totalBytes":9000,"transfers":1,"totalTransfers":2,"checks":1,"totalChecks":2,"speed":123.0,"eta":1,"transferring":[{"name":"b.bin","size":5000,"bytes":100,"percentage":2,"speed":50}]}}"#)
                .stdout("2025/01/15 10:00:00 INFO  : ERRORS: 2")
                .stderr("plain stderr line"),
        );
        let (store, cb) = collect_progress();
        let res = client(&fake)
            .sync(&cfg(RcloneAction::Push), Some(&cb), None)
            .unwrap();
        assert_eq!(res.exit_code, 0);
        assert!(res.ended_at >= res.started_at && res.started_at > 0);
        assert_eq!((res.stats.bytes, res.stats.bytes_total), (4500, 9000));
        assert_eq!((res.stats.files, res.stats.files_total), (1, 2));
        assert_eq!(res.stats.speed, 123.0);
        assert_eq!(res.stats.errors, 2);
        assert!(res.stderr.contains("plain stderr line\n"));
        assert!(
            !res.stderr.contains("ERRORS: 2"),
            "stdout isn't captured in stderr"
        );
        let names: Vec<(&str, &str)> = res
            .stats
            .file_transfers
            .iter()
            .map(|f| (f.name.as_str(), f.status.as_str()))
            .collect();
        assert_eq!(
            names,
            [("photo.jpg", "completed"), ("b.bin", "transferring")]
        );

        let snaps = store.lock();
        assert_eq!(snaps[0].stage, "starting");
        assert!(snaps.len() >= 4);
        assert!(snaps.iter().any(
            |s| s.bytes == 4500 && s.file_transfers.iter().any(|f| f.status == "transferring")
        ));
        assert!(snaps.iter().all(|s| s.last_update > 0));
        // Args are the push command.
        let call = fake.calls().into_iter().find(|c| c[0] == "sync").unwrap();
        assert_eq!(call, c_args(&client(&fake), &cfg(RcloneAction::Push)));
    }

    fn c_args(c: &RcloneClient, cf: &SyncConfig) -> Vec<String> {
        c.build_args(cf).unwrap()
    }

    #[test]
    fn sync_seeds_pending_rows_from_listing() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(
            &["--log-level", "ERROR", "lsjson"],
            Response::new().stdout(r#"[{"Path":"p1.bin","Name":"p1.bin","Size":10},{"Path":"p2.bin","Name":"p2.bin","Size":20}]"#),
        );
        fake.on(
            &["sync"],
            Response::new()
                .stderr(r#"{"level":"info","msg":"stats","stats":{"totalTransfers":3}}"#),
        );
        // Make the sync wait until the seeder thread has delivered.
        let gate = Arc::new(AtomicBool::new(false));
        let g = gate.clone();
        let f2 = fake.clone();
        fake.set_line_hook(move |_, _, _| {
            let t = Instant::now();
            while f2.calls_for("lsjson").is_empty() && t.elapsed() < Duration::from_secs(2) {
                std::thread::sleep(Duration::from_millis(1));
            }
            std::thread::sleep(Duration::from_millis(30));
            g.store(true, Ordering::SeqCst);
        });
        let res = client(&fake)
            .sync(&cfg(RcloneAction::Push), None, None)
            .unwrap();
        assert!(gate.load(Ordering::SeqCst));
        // lsjson targeted the source ("/src") for push.
        let ls = &fake.calls_for("lsjson")[0];
        assert_eq!(ls[3], "/src");
        // Seeded rows may or may not land before the run ends, but if present they are pending.
        for f in &res.stats.file_transfers {
            assert_eq!(f.status, "pending", "{f:?}");
        }
    }

    #[test]
    fn sync_pull_seeds_from_dest() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(&["sync"], Response::new());
        let _ = client(&fake)
            .sync(&cfg(RcloneAction::Pull), None, None)
            .unwrap();
        // The seeder is detached; give it a moment only if it hasn't run yet.
        let t = Instant::now();
        while fake.calls_for("lsjson").is_empty() && t.elapsed() < Duration::from_millis(100) {
            std::thread::sleep(Duration::from_millis(1));
        }
        if let Some(ls) = fake.calls_for("lsjson").first() {
            assert_eq!(ls[3], "r:dst");
        }
    }

    #[test]
    fn sync_nonzero_exit_message() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(
            &["sync"],
            Response::new().stderr("Failed to sync: boom").exit(7),
        );
        let c = client(&fake);
        let e = c.sync(&cfg(RcloneAction::Push), None, None).unwrap_err();
        assert!(!e.cancelled);
        assert_eq!(e.result.exit_code, 7);
        let expect = format!(
            "rclone {}: exit 7 (stderr: Failed to sync: boom\n)",
            c_args(&c, &cfg(RcloneAction::Push)).join(" ")
        );
        assert_eq!(e.message, expect);
        assert_eq!(e.to_string(), expect);
        assert!(e.result.stderr.contains("Failed to sync: boom"));
    }

    #[test]
    fn sync_failure_truncates_stderr_to_500() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(&["copy"], Response::new().stderr("z".repeat(800)).exit(1));
        let e = client(&fake)
            .sync(&cfg(RcloneAction::Copy), None, None)
            .unwrap_err();
        assert!(
            e.message
                .contains(&format!("{}...(truncated))", "z".repeat(500))),
            "{}",
            e.message
        );
    }

    #[test]
    fn sync_requires_endpoints() {
        let fake = Arc::new(FakeRunner::new());
        let e = client(&fake)
            .sync(&SyncConfig::default(), None, None)
            .unwrap_err();
        assert_eq!(
            e.message,
            "rclone sync: requires Source+Dest or remotes+paths"
        );
        assert_eq!(fake.call_count(), 0);
    }

    #[test]
    fn sync_spawn_failure_is_reported() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(&["sync"], Response::new().spawn_error("no such binary"));
        let e = client(&fake)
            .sync(&cfg(RcloneAction::Push), None, None)
            .unwrap_err();
        assert!(e.message.contains("no such binary"), "{}", e.message);
        assert!(!e.cancelled);
    }

    #[test]
    fn sync_cancellation_kills_and_reports_cancelled() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(
            &["sync"],
            Response::new()
                .stderr("{\"level\":\"info\",\"msg\":\"Listing\"}")
                .hang_until_cancelled(),
        );
        let token = CancellationToken::new();
        let t2 = token.clone();
        let (store, cb) = collect_progress();
        let c = client(&fake);
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            t2.cancel();
        });
        let e = c
            .sync(&cfg(RcloneAction::Push), Some(&cb), Some(&token))
            .unwrap_err();
        canceller.join().unwrap();
        assert!(e.cancelled);
        assert_eq!(e.message, "cancelled");
        assert!(store.lock().iter().any(|s| s.stage == "listing"));
    }

    #[test]
    fn sync_pre_cancelled_token() {
        let fake = Arc::new(FakeRunner::new());
        fake.on(&["sync"], Response::new().stderr("x"));
        let token = CancellationToken::new();
        token.cancel();
        let e = client(&fake)
            .sync(&cfg(RcloneAction::Push), None, Some(&token))
            .unwrap_err();
        assert!(e.cancelled);
    }

    #[test]
    fn sync_client_trait_object() {
        let fake = Arc::new(FakeRunner::new());
        let c: Arc<dyn SyncClient> = Arc::new(client(&fake));
        assert!(c.sync(&cfg(RcloneAction::Check), None, None).is_ok());
    }

    // ---- ProcessRunner with scripts (no rclone needed) ----

    fn script(dir: &Path, name: &str, body: &str) -> String {
        let p = dir.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn process_runner_run_and_streaming_split_lines() {
        let d = tempfile::tempdir().unwrap();
        let bin = script(
            d.path(),
            "r",
            "echo out1; echo err1 1>&2; printf 'out-partial'; echo 'a b' \"$1\" 1>&2; read x; echo \"stdin:[$x]\"; exit 3",
        );
        let r = ProcessRunner::new(&bin);
        assert_eq!(r.binary(), bin);
        let out = r.run(&sv(&["arg"])).unwrap();
        assert_eq!(out.exit_code, 3);
        assert_eq!(out.stdout, "out1\nout-partialstdin:[]\n");
        assert_eq!(out.stderr, "err1\na b arg\n");

        let lines = Mutex::new(Vec::new());
        let code = r
            .run_streaming(&sv(&["arg"]), None, &|k, l| {
                lines.lock().push((k, l.to_string()))
            })
            .unwrap();
        assert_eq!(code, 3);
        let lines = lines.into_inner();
        let so: Vec<&str> = lines
            .iter()
            .filter(|(k, _)| *k == StreamKind::Stdout)
            .map(|(_, l)| l.as_str())
            .collect();
        let se: Vec<&str> = lines
            .iter()
            .filter(|(k, _)| *k == StreamKind::Stderr)
            .map(|(_, l)| l.as_str())
            .collect();
        assert_eq!(so, ["out1", "out-partialstdin:[]"]);
        assert_eq!(se, ["err1", "a b arg"]);
    }

    #[test]
    fn process_runner_spawn_error() {
        let r = ProcessRunner::new("/definitely/missing/rclone");
        assert!(r.run(&[]).is_err());
        assert!(r.run_streaming(&[], None, &|_, _| {}).is_err());
    }

    #[test]
    fn process_runner_cancel_terminates_child() {
        let d = tempfile::tempdir().unwrap();
        let bin = script(d.path(), "slow", "echo started 1>&2; exec sleep 30");
        let r = ProcessRunner::new(&bin);
        let token = CancellationToken::new();
        let t2 = token.clone();
        let started = Instant::now();
        let code = r
            .run_streaming(&[], Some(&token), &|_, l| {
                if l == "started" {
                    t2.cancel();
                }
            })
            .unwrap();
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(code, libc::SIGTERM);
        assert!(token.is_cancelled());
    }

    #[test]
    fn process_runner_pre_cancelled_token_terminates() {
        let d = tempfile::tempdir().unwrap();
        let bin = script(d.path(), "slow", "exec sleep 30");
        let token = CancellationToken::new();
        token.cancel();
        let t = Instant::now();
        let code = ProcessRunner::new(&bin)
            .run_streaming(&[], Some(&token), &|_, _| {})
            .unwrap();
        assert!(t.elapsed() < Duration::from_secs(10));
        assert_eq!(code, libc::SIGTERM);
    }

    #[test]
    fn sync_with_scripted_binary_end_to_end() {
        let d = tempfile::tempdir().unwrap();
        let bin = script(
            d.path(),
            "rclone-json",
            r#"echo '{"level":"info","msg":"x","stats":{"bytes":2048,"totalBytes":4096,"transfers":3,"totalTransfers":6,"errors":0}}' 1>&2"#,
        );
        let c = RcloneClient::new(Some(&bin), "/c.conf", &Logger::quiet()).unwrap();
        let (store, cb) = collect_progress();
        let res = c
            .sync(
                &SyncConfig {
                    action: RcloneAction::Push,
                    source: "r:src".into(),
                    dest: "r:dst".into(),
                    ..Default::default()
                },
                Some(&cb),
                None,
            )
            .unwrap();
        assert!(!store.lock().is_empty());
        assert_eq!(
            (res.stats.bytes, res.stats.bytes_total, res.stats.files),
            (2048, 4096, 3)
        );
    }

    // ---- real rclone, local paths only ----

    fn system_rclone() -> Option<String> {
        RcloneClient::resolve_binary(None)
            .ok()
            .or_else(|| look_path_in("rclone", &std::env::var("PATH").unwrap_or_default()))
    }

    #[test]
    fn real_rclone_local_copy_and_version() {
        let Some(bin) = system_rclone() else {
            eprintln!("skipping: rclone not on PATH");
            return;
        };
        let d = tempfile::tempdir().unwrap();
        let (src, dst) = (d.path().join("src"), d.path().join("dst"));
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), "hello").unwrap();
        std::fs::write(src.join("sub/b.txt"), "world!").unwrap();
        let conf = d.path().join("rclone.conf");
        std::fs::write(&conf, "").unwrap();

        let c = RcloneClient::new(Some(&bin), conf.to_str().unwrap(), &Logger::quiet()).unwrap();
        assert!(c.version().unwrap().starts_with("rclone"));

        let (store, cb) = collect_progress();
        let res = c
            .sync(
                &SyncConfig {
                    action: RcloneAction::Copy,
                    source: src.to_str().unwrap().into(),
                    dest: dst.to_str().unwrap().into(),
                    ..Default::default()
                },
                Some(&cb),
                None,
            )
            .unwrap();
        assert_eq!(res.exit_code, 0);
        assert_eq!(std::fs::read_to_string(dst.join("a.txt")).unwrap(), "hello");
        assert_eq!(
            std::fs::read_to_string(dst.join("sub/b.txt")).unwrap(),
            "world!"
        );
        assert!(!store.lock().is_empty());

        let listed = c.list_files(dst.to_str().unwrap()).unwrap();
        let mut names: Vec<_> = listed.iter().map(|e| (e.name.clone(), e.is_dir)).collect();
        names.sort();
        assert_eq!(
            names,
            [("a.txt".to_string(), false), ("sub".to_string(), true)]
        );
        let rec = c.list_files_for_pending(dst.to_str().unwrap(), 10).unwrap();
        assert_eq!(rec.len(), 2);

        c.mkdir(dst.join("newdir").to_str().unwrap()).unwrap();
        c.delete_file(dst.join("a.txt").to_str().unwrap()).unwrap();
        assert!(!dst.join("a.txt").exists());
        c.purge(dst.join("sub").to_str().unwrap()).unwrap();
        assert!(!dst.join("sub").exists());

        // Failure path: missing source directory.
        let bad = c.sync(
            &SyncConfig {
                action: RcloneAction::Copy,
                source: d.path().join("missing").to_str().unwrap().into(),
                dest: dst.to_str().unwrap().into(),
                ..Default::default()
            },
            None,
            None,
        );
        let e = bad.unwrap_err();
        assert!(!e.cancelled);
        assert_ne!(e.result.exit_code, 0);
    }
}
