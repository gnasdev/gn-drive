//! Protocol v1 error envelope + mapping from engine errors
//! (port of `RPCError.swift`).

use crate::auth::AuthError;
use crate::board_engine::BoardEngineError;
use crate::container::AppError;
use crate::flow_engine::FlowEngineError;
use crate::instance_lock::InstanceError;
use crate::launchd::ServiceError;
use crate::rclone::stats::truncate_string;
use crate::rclone::{RcloneError, SyncFailedError};
use crate::self_update::SelfUpdateError;
use crate::store::StoreError;
use crate::sync_engine::SyncEngineError;
use serde_json::{Map, Value, json};
use std::error::Error;

#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: String,
    pub message: String,
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: &str, message: impl Into<String>) -> RpcError {
        RpcError {
            code: code.to_string(),
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: Value) -> RpcError {
        self.data = Some(data);
        self
    }

    pub fn invalid_params(field: &str, message: Option<&str>) -> RpcError {
        let msg = message
            .map(str::to_string)
            .unwrap_or_else(|| format!("invalid params: {field}"));
        RpcError::new("invalid_params", msg).with_data(json!({ "field": field }))
    }

    pub fn locked() -> RpcError {
        RpcError::new("locked", "auth: app is locked").with_data(json!({ "setup": true }))
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl Error for RpcError {}

// ---------------------------------------------------------------------------
// Redaction (hand-written scanners equivalent to the three Swift regexes).

type Matcher = fn(&[u8], usize) -> Option<(usize, usize)>;

fn starts_with_ci(b: &[u8], i: usize, kw: &str) -> bool {
    b.len() >= i + kw.len() && b[i..i + kw.len()].eq_ignore_ascii_case(kw.as_bytes())
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn skip_space(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && is_space(b[i]) {
        i += 1;
    }
    i
}

const KEYWORDS_1: [&str; 6] = [
    "password",
    "pass",
    "token",
    "secret",
    "client_secret",
    "config_token",
];

/// `(pass(?:word)?|token|secret|client_secret|config_token|access_token)\s*[=:]\s*...`
fn match_kv(b: &[u8], i: usize) -> Option<(usize, usize)> {
    let all = KEYWORDS_1.iter().copied().chain(["access_token"]);
    for kw in all {
        if !starts_with_ci(b, i, kw) {
            continue;
        }
        let key_end = i + kw.len();
        let j = skip_space(b, key_end);
        if j >= b.len() || !matches!(b[j], b'=' | b':') {
            continue;
        }
        // `\s*` applies on both sides of `[=:]` (`config_token: abc`).
        let v = skip_space(b, j + 1);
        let mut k = v;
        while k < b.len() && !is_space(b[k]) && !matches!(b[k], b',' | b';' | b'"' | b'\'' | b'}') {
            k += 1;
        }
        if k > v {
            return Some((key_end, k));
        }
    }
    None
}

const KEYWORDS_2: [&str; 7] = [
    "password",
    "pass",
    "token",
    "secret",
    "key",
    "client_secret",
    "config_token",
];

/// `(["'](?:pass(?:word)?|token|...)["']\s*:\s*)["'][^"']*["']`
fn match_json_kv(b: &[u8], i: usize) -> Option<(usize, usize)> {
    if !matches!(b[i], b'"' | b'\'') {
        return None;
    }
    let all = KEYWORDS_2.iter().copied().chain(["access_token"]);
    for kw in all {
        let s = i + 1;
        if !starts_with_ci(b, s, kw) {
            continue;
        }
        let q = s + kw.len();
        if q >= b.len() || !matches!(b[q], b'"' | b'\'') {
            continue;
        }
        let j = skip_space(b, q + 1);
        if j >= b.len() || b[j] != b':' {
            continue;
        }
        let g_end = skip_space(b, j + 1);
        if g_end >= b.len() || !matches!(b[g_end], b'"' | b'\'') {
            continue;
        }
        let mut k = g_end + 1;
        while k < b.len() && !matches!(b[k], b'"' | b'\'') {
            k += 1;
        }
        if k < b.len() {
            return Some((g_end, k + 1));
        }
    }
    None
}

/// `(--(?:[\w-]*key[\w-]*|password|token|secret))\s+(?!-)\S+`
fn match_flag(b: &[u8], i: usize) -> Option<(usize, usize)> {
    if !b[i..].starts_with(b"--") {
        return None;
    }
    let s = i + 2;
    let mut e = s;
    while e < b.len() && (b[e].is_ascii_alphanumeric() || b[e] == b'_' || b[e] == b'-') {
        e += 1;
    }
    let run = &b[s..e];
    let has_key = run.windows(3).any(|w| w.eq_ignore_ascii_case(b"key"));
    let named = ["password", "token", "secret"]
        .iter()
        .any(|k| run.eq_ignore_ascii_case(k.as_bytes()));
    if !has_key && !named {
        return None;
    }
    let v = skip_space(b, e);
    if v == e || v >= b.len() || b[v] == b'-' {
        return None;
    }
    let mut k = v;
    while k < b.len() && !is_space(b[k]) {
        k += 1;
    }
    Some((e, k))
}

fn replace_all(text: &str, m: Matcher) -> String {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let mut last = 0;
    while i < b.len() {
        if let Some((g_end, m_end)) = m(b, i) {
            out.extend_from_slice(&b[last..i]);
            out.extend_from_slice(&b[i..g_end]);
            out.extend_from_slice(b"=***");
            i = m_end;
            last = m_end;
        } else {
            i += 1;
        }
    }
    out.extend_from_slice(&b[last..]);
    String::from_utf8_lossy(&out).into_owned()
}

pub mod redact {
    use super::*;

    /// Redact credential-looking values from rclone output before it reaches
    /// a client. Keeps command/flag names and messages readable.
    pub fn sanitize(text: &str) -> String {
        let a = replace_all(text, match_kv);
        let b = replace_all(&a, match_json_kv);
        replace_all(&b, match_flag)
    }

    pub fn stderr_field(text: &str) -> String {
        sanitize(&truncate_string(text, 500))
    }
}

/// `RPCRedact.sanitize`.
pub fn sanitize(text: &str) -> String {
    redact::sanitize(text)
}

/// `RPCRedact.stderrField`.
pub fn stderr_field(text: &str) -> String {
    redact::stderr_field(text)
}

// ---------------------------------------------------------------------------
// Error mapping (`rpcMapError`).

/// Map any core error to its protocol error. Unknown types → `internal_error`.
pub fn map_error<E: Error + 'static>(e: &E) -> RpcError {
    let dynerr: &(dyn Error + 'static) = e;
    if let Some(e) = dynerr.downcast_ref::<RpcError>() {
        return e.clone();
    }
    if let Some(e) = dynerr.downcast_ref::<AuthError>() {
        return from_auth(e);
    }
    if let Some(e) = dynerr.downcast_ref::<FlowEngineError>() {
        return from_flow(e);
    }
    if let Some(e) = dynerr.downcast_ref::<SyncEngineError>() {
        return from_sync(e);
    }
    if let Some(e) = dynerr.downcast_ref::<BoardEngineError>() {
        return from_board(e);
    }
    if let Some(e) = dynerr.downcast_ref::<RcloneError>() {
        return from_rclone(e);
    }
    if let Some(e) = dynerr.downcast_ref::<SyncFailedError>() {
        return from_sync_failed(e);
    }
    if let Some(e) = dynerr.downcast_ref::<ServiceError>() {
        return from_service(e);
    }
    if let Some(e) = dynerr.downcast_ref::<SelfUpdateError>() {
        return from_update(e);
    }
    if let Some(e) = dynerr.downcast_ref::<AppError>() {
        return from_app(e);
    }
    if let Some(e) = dynerr.downcast_ref::<InstanceError>() {
        return from_instance(e);
    }
    if let Some(e) = dynerr.downcast_ref::<StoreError>() {
        return from_store(e);
    }
    RpcError::new("internal_error", e.to_string())
}

fn from_auth(e: &AuthError) -> RpcError {
    match e {
        AuthError::NotSetup => RpcError::new("not_setup", "auth: no master password is configured"),
        AuthError::AlreadySetup => RpcError::new(
            "already_setup",
            "auth: a master password is already configured",
        ),
        AuthError::NotUnlocked => RpcError::new("not_unlocked", "auth: app is locked"),
        AuthError::InvalidPassword => RpcError::new("invalid_password", "auth: invalid password"),
        AuthError::Locked { retry_after_secs } => RpcError::new(
            "locked_out",
            format!("auth: too many attempts — retry in {retry_after_secs}s"),
        )
        .with_data(json!({ "retryAfterSecs": retry_after_secs })),
        AuthError::AlreadyUnlocked => RpcError::new("already_unlocked", "auth: already unlocked"),
        AuthError::PasswordTooShort => RpcError::new(
            "password_too_short",
            "auth: password must be at least 4 characters",
        ),
        other => RpcError::new("internal_error", other.to_string()),
    }
}

fn from_flow(e: &FlowEngineError) -> RpcError {
    match e {
        FlowEngineError::AlreadyRunning => {
            RpcError::new("already_running", "flow is already running")
        }
        FlowEngineError::NotRunning => RpcError::new("not_running", "flow is not running"),
        FlowEngineError::EmptyFlow => RpcError::new("flow_empty", "flow has no operations"),
        FlowEngineError::EngineNotReady => {
            RpcError::new("engine_not_ready", "flow engine is not ready")
        }
        FlowEngineError::Store(e) => from_store(e),
    }
}

fn from_sync(e: &SyncEngineError) -> RpcError {
    match e {
        SyncEngineError::NotRunning => {
            RpcError::new("sync_not_running", "sync engine is not running")
        }
        SyncEngineError::StoreNotReady => {
            RpcError::new("engine_not_ready", "sync engine has no data plane")
        }
        SyncEngineError::ProfileBusy => {
            RpcError::new("profile_busy", "a sync for this profile is already running")
        }
        SyncEngineError::TaskFailed(msg) => RpcError::new("task_failed", sanitize(msg)),
        SyncEngineError::TaskCancelled => RpcError::new("task_cancelled", "task was cancelled"),
        SyncEngineError::TaskNotFound => RpcError::new("task_not_found", "task not found"),
        SyncEngineError::Store(e) => from_store(e),
    }
}

fn from_board(e: &BoardEngineError) -> RpcError {
    match e {
        BoardEngineError::NotConfigured => {
            RpcError::new("engine_not_ready", "board engine is not ready")
        }
        BoardEngineError::EmptyBoard => RpcError::new("board_empty", "board has no nodes or edges"),
        BoardEngineError::AlreadyRunning => RpcError::new("board_busy", "board is already running"),
        BoardEngineError::NotRunning => RpcError::new("board_not_running", "board is not running"),
        BoardEngineError::Cancelled => {
            RpcError::new("board_not_running", "board run was cancelled")
        }
        BoardEngineError::MissingNode(edge_id) => RpcError::new(
            "board_missing_node",
            format!("board edge {edge_id} references a missing node"),
        ),
        BoardEngineError::Store(e) => from_store(e),
        BoardEngineError::Failed(msg) | BoardEngineError::CompletedWithErrors(msg) => {
            if msg.contains("cycle detected") {
                RpcError::new("board_cycle", "board graph contains a cycle")
            } else {
                RpcError::new("internal_error", sanitize(msg))
            }
        }
    }
}

fn from_rclone(e: &RcloneError) -> RpcError {
    match e {
        RcloneError::BinaryNotFound(p) => {
            RpcError::new("rclone_missing", format!("rclone binary not found: {p}"))
                .with_data(json!({ "path": p }))
        }
        RcloneError::RunFailed { command, stderr } => RpcError::new(
            "rclone_failed",
            format!("rclone {}: {}", sanitize(command), stderr_field(stderr)),
        )
        .with_data(json!({ "command": sanitize(command), "stderr": stderr_field(stderr) })),
        RcloneError::Cancelled => RpcError::new("task_cancelled", "rclone was cancelled"),
    }
}

fn from_sync_failed(e: &SyncFailedError) -> RpcError {
    let msg = if e.cancelled {
        "sync cancelled".to_string()
    } else {
        sanitize(&e.message)
    };
    RpcError::new("task_failed", msg)
        .with_data(json!({ "cancelled": if e.cancelled { "true" } else { "false" } }))
}

fn from_service(e: &ServiceError) -> RpcError {
    match e {
        ServiceError::NotInstalled => {
            RpcError::new("service_not_installed", "service is not installed")
        }
        ServiceError::Launchctl(m) => RpcError::new("service_error", sanitize(m)),
    }
}

fn from_update(e: &SelfUpdateError) -> RpcError {
    match e {
        SelfUpdateError::NoRelease => RpcError::new("no_release", "no release found"),
        SelfUpdateError::AlreadyUpToDate => {
            RpcError::new("already_up_to_date", "already on the latest version")
        }
        SelfUpdateError::NoMatchingAsset(n) => RpcError::new(
            "no_matching_asset",
            format!("no release asset matches this platform: {n}"),
        ),
        SelfUpdateError::MissingChecksum(n) => {
            RpcError::new("no_matching_asset", format!("missing checksum asset {n}"))
        }
        SelfUpdateError::ChecksumMismatch => RpcError::new(
            "checksum_mismatch",
            "downloaded archive failed SHA256 verification",
        ),
        SelfUpdateError::Http(code, body) => RpcError::new(
            "update_failed",
            format!("update HTTP {code}: {}", stderr_field(body)),
        ),
        SelfUpdateError::ExtractFailed(m) => {
            RpcError::new("update_failed", format!("update extract failed: {m}"))
        }
        SelfUpdateError::Io(m) => RpcError::new("internal_error", m.clone()),
    }
}

fn from_app(e: &AppError) -> RpcError {
    match e {
        AppError::Locked => RpcError::locked(),
        AppError::RcloneMissing(underlying) => RpcError::new(
            "rclone_missing",
            format!("rclone unavailable: {underlying}"),
        ),
        AppError::Auth(e) => from_auth(e),
        AppError::Store(e) => from_store(e),
        AppError::Io(m) => RpcError::new("internal_error", m.clone()),
    }
}

fn from_instance(e: &InstanceError) -> RpcError {
    match e {
        InstanceError::AnotherInstance { pid } => {
            let mut d = Map::new();
            if let Some(pid) = pid {
                d.insert("pid".into(), Value::String(pid.to_string()));
            }
            RpcError::new("instance_busy", e.to_string()).with_data(Value::Object(d))
        }
        InstanceError::Io(m) => RpcError::new("internal_error", m.clone()),
    }
}

fn from_store(e: &StoreError) -> RpcError {
    match e {
        StoreError::NotFound => RpcError::new("not_found", "record not found"),
        StoreError::Invalid(m) => {
            if m.contains("name is required") {
                RpcError::new("missing_name", m.clone()).with_data(json!({ "field": "name" }))
            } else if m.contains("invalid direction") {
                RpcError::new("invalid_direction", m.clone())
                    .with_data(json!({ "field": "direction" }))
            } else {
                RpcError::new("invalid_params", m.clone())
            }
        }
        StoreError::Db(m) => RpcError::new("internal_error", m.clone()),
    }
}

macro_rules! impl_from {
    ($t:ty, $f:expr) => {
        impl From<$t> for RpcError {
            fn from(e: $t) -> RpcError {
                $f(&e)
            }
        }
    };
}
impl_from!(AuthError, from_auth);
impl_from!(StoreError, from_store);
impl_from!(SyncEngineError, from_sync);
impl_from!(FlowEngineError, from_flow);
impl_from!(BoardEngineError, from_board);
impl_from!(RcloneError, from_rclone);
impl_from!(SyncFailedError, from_sync_failed);
impl_from!(ServiceError, from_service);
impl_from!(SelfUpdateError, from_update);
impl_from!(AppError, from_app);
impl_from!(InstanceError, from_instance);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_key_values() {
        assert_eq!(sanitize("password=hunter2 ok"), "password=*** ok");
        assert_eq!(sanitize("config_token: abc,def"), "config_token=***,def");
        assert_eq!(sanitize("PASS = x;y"), "PASS=***;y");
        assert_eq!(sanitize("nothing here"), "nothing here");
    }

    #[test]
    fn sanitize_json_and_flags() {
        assert_eq!(sanitize(r#"{"token": "abc"}"#), r#"{"token": =***}"#);
        assert_eq!(
            sanitize("run --password hunter2 --foo"),
            "run --password=*** --foo"
        );
        assert_eq!(
            sanitize("--drive-client-key abc rest"),
            "--drive-client-key=*** rest"
        );
        assert_eq!(sanitize("--password --next"), "--password --next");
    }

    #[test]
    fn invalid_params_shape() {
        let e = RpcError::invalid_params("limit", None);
        assert_eq!(e.code, "invalid_params");
        assert_eq!(e.message, "invalid params: limit");
        assert_eq!(e.data.unwrap()["field"], "limit");
        assert_eq!(RpcError::locked().data.unwrap()["setup"], true);
    }

    #[test]
    fn maps_auth_and_store() {
        let e = map_error(&AuthError::Locked {
            retry_after_secs: 7,
        });
        assert_eq!(e.code, "locked_out");
        assert_eq!(e.data.unwrap()["retryAfterSecs"], 7);
        assert_eq!(map_error(&StoreError::NotFound).code, "not_found");
        let m = map_error(&StoreError::Invalid("profile name is required".into()));
        assert_eq!(m.code, "missing_name");
        let rpc = RpcError::new("x", "y");
        assert_eq!(map_error(&rpc).code, "x");
    }

    #[test]
    fn maps_rclone_run_failed_redacted() {
        let e = map_error(&RcloneError::RunFailed {
            command: "config create".into(),
            stderr: "token=abc failed".into(),
        });
        assert_eq!(e.code, "rclone_failed");
        assert!(!e.message.contains("abc"));
    }
}
