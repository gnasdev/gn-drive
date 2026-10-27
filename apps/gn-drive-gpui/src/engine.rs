//! The GN Drive engine link.
//!
//! The engine owns all data. [`Engine`] is a thin facade over a [`Backend`]
//! that frames requests and collects pushes; the window keeps its own copy of
//! the last snapshot. Backends:
//!
//! - in-process `gn-drive-core` dispatcher (default with the `core` feature),
//! - `gn-drive serve --stdio` child process (`GN_DRIVE_BACKEND=stdio`, and the
//!   only real backend without the `core` feature),
//! - a built-in mock (`GN_DRIVE_MOCK=1`, devtools builds and tests) that
//!   speaks the same frames so every screen can be built and exercised
//!   without an engine.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(any(feature = "devtools", test))]
use serde_json::json;

pub type JsonMap = serde_json::Map<String, Value>;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/Users/me".into())
}

// MARK: - Wire types

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lockout {
    #[serde(default)]
    pub failed_attempts: u32,
    #[serde(default)]
    pub locked_until: String,
    #[serde(default)]
    pub is_locked: bool,
    #[serde(default)]
    pub retry_after_secs: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    #[serde(default)]
    pub setup: bool,
    #[serde(default)]
    pub unlocked: bool,
    #[serde(default)]
    pub locked_at: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub lockout: Lockout,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// On unless the user turns the switch off. A missing key stays on.
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,
    #[serde(default)]
    pub debug_mode: bool,
    #[serde(default)]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub start_at_login: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            notifications_enabled: true,
            debug_mode: false,
            minimize_to_tray: false,
            start_at_login: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "flowID", alias = "flowId", default)]
    pub flow_id: String,
    #[serde(default)]
    pub source_remote: String,
    #[serde(default)]
    pub source_path: String,
    #[serde(default)]
    pub target_remote: String,
    #[serde(default)]
    pub target_path: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub sync_config: JsonMap,
    #[serde(default)]
    pub is_expanded: bool,
    #[serde(default)]
    pub sort_order: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Flow {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub is_collapsed: bool,
    #[serde(default)]
    pub schedule_enabled: bool,
    #[serde(default)]
    pub schedule_cron: String,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default)]
    pub operations: Vec<Operation>,
    #[serde(rename = "canvasJSON", alias = "canvasJson", default)]
    pub canvas_json: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

impl Flow {
    pub fn title(&self) -> String {
        if self.name.trim().is_empty() {
            "Untitled flow".into()
        } else {
            self.name.clone()
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTransfer {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub speed: f64,
    #[serde(default)]
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    #[serde(rename = "taskID", alias = "taskId", default)]
    pub task_id: String,
    #[serde(rename = "profileID", alias = "profileId", default)]
    pub profile_id: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub transferred: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub bytes_per_sec: f64,
    #[serde(default)]
    pub eta: i64,
    #[serde(default)]
    pub errors: u64,
    #[serde(default)]
    pub current_file: String,
    #[serde(default)]
    pub stage: String,
    #[serde(default)]
    pub stage_detail: String,
    #[serde(default)]
    pub files_transferred: u64,
    #[serde(default)]
    pub total_files: u64,
    #[serde(default)]
    pub checks: u64,
    #[serde(default)]
    pub total_checks: u64,
    #[serde(default)]
    pub deletes: u64,
    #[serde(default)]
    pub renames: u64,
    #[serde(default)]
    pub transfers: Vec<FileTransfer>,
    #[serde(default)]
    pub error_message: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOp {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub last_error: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeLog {
    #[serde(default)]
    pub at: i64,
    #[serde(default)]
    pub status: String,
    #[serde(rename = "opID", alias = "opId", default)]
    pub op_id: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeFlow {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub last_error: String,
    #[serde(default)]
    pub ops: Vec<RuntimeOp>,
    #[serde(default)]
    pub sync: Option<SyncProgress>,
    #[serde(default)]
    pub log: Vec<RuntimeLog>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    #[serde(default)]
    pub revision: i64,
    #[serde(default)]
    pub flows: Vec<RuntimeFlow>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub profile_name: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub started_at: String,
    #[serde(default)]
    pub finished_at: String,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub errors: u64,
    #[serde(default)]
    pub files: u64,
    #[serde(default)]
    pub error_message: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paths {
    #[serde(default)]
    pub config_dir: String,
    #[serde(default)]
    pub db_path: String,
    #[serde(default)]
    pub rclone_config_path: String,
    #[serde(default)]
    pub log_dir: String,
    #[serde(default)]
    pub working_dir: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub auth: AuthStatus,
    #[serde(default)]
    pub flows: Vec<Flow>,
    #[serde(default)]
    pub remotes: Vec<Remote>,
    #[serde(default)]
    pub profiles: Vec<Value>,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    #[serde(default)]
    pub runtime: Runtime,
    #[serde(default)]
    pub app_settings: AppSettings,
    #[serde(default)]
    pub rclone_version: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub paths: Paths,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOption {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub required: bool,
    #[serde(rename = "isPassword", default)]
    pub is_password: bool,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub advanced: bool,
    #[serde(default)]
    pub hide: i64,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(rename = "default", default)]
    pub default_value: String,
    #[serde(default)]
    pub examples: Vec<Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub oauth: bool,
    #[serde(default)]
    pub options: Vec<ProviderOption>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub is_dir: bool,
    #[serde(default)]
    pub mod_time: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorCheck {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub detail: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub checks: Vec<DoctorCheck>,
    #[serde(default)]
    pub data_files: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub msg: String,
    #[serde(default)]
    pub fields: JsonMap,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsPage {
    #[serde(default)]
    pub lines: Vec<LogLine>,
    #[serde(default)]
    pub dropped: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub pid: i64,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub plist_path: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    #[serde(default)]
    pub present: bool,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub pid: i64,
    #[serde(default)]
    pub service_name: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub last_heartbeat: Option<String>,
    #[serde(default)]
    pub uptime_secs: i64,
    #[serde(default)]
    pub last_sync_at: Option<String>,
    #[serde(default)]
    pub next_schedule_at: Option<String>,
    #[serde(default)]
    pub last_error: String,
    #[serde(default)]
    pub active_tasks: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    #[serde(default)]
    pub current: String,
    #[serde(default)]
    pub latest: String,
    #[serde(default)]
    pub update_available: bool,
    #[serde(default)]
    pub checked_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RpcError {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub data: Value,
}

impl RpcError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            data: Value::Null,
        }
    }

    #[cfg(any(feature = "devtools", test))]
    pub fn with_data(code: &str, message: impl Into<String>, data: Value) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            data,
        }
    }

    pub fn retry_after(&self) -> Option<u64> {
        self.data.get("retryAfterSecs").and_then(Value::as_u64)
    }
}

// MARK: - Link

#[derive(Clone, Debug, PartialEq)]
pub enum LinkStatus {
    Starting,
    Ready,
    Fatal {
        code: String,
        message: String,
        holder: Option<String>,
    },
    Restarting {
        delay: u64,
        reason: String,
    },
    Stopped,
}

impl LinkStatus {
    pub fn is_ready(&self) -> bool {
        matches!(self, LinkStatus::Ready)
    }

    pub fn label(&self) -> String {
        match self {
            LinkStatus::Starting => "Connecting".into(),
            LinkStatus::Ready => "Connected".into(),
            LinkStatus::Fatal { code, .. } => format!("Engine error ({code})"),
            LinkStatus::Restarting { delay, .. } => format!("Reconnecting in {delay}s"),
            LinkStatus::Stopped => "Disconnected".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub raw: Value,
}

impl Frame {
    pub fn event(&self) -> Option<&str> {
        self.raw.get("event").and_then(Value::as_str)
    }

    pub fn id(&self) -> Option<u64> {
        self.raw.get("id").and_then(Value::as_u64)
    }

    pub fn result(&self) -> Option<&Value> {
        self.raw.get("result")
    }

    pub fn error(&self) -> Option<RpcError> {
        self.raw
            .get("error")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn data(&self) -> &Value {
        self.raw.get("data").unwrap_or(&Value::Null)
    }
}

#[cfg(any(feature = "devtools", test))]
pub fn emit_frame(event: &str, data: Value) -> Value {
    json!({"event": event, "ts": now_ms(), "data": data})
}

/// Frames waiting for the UI thread. Every backend pushes here from its own
/// threads; `Engine::poll` drains it once per pump.
#[derive(Default)]
pub(crate) struct Inbox {
    frames: Mutex<VecDeque<Frame>>,
}

impl Inbox {
    pub(crate) fn push(&self, value: Value) {
        self.frames.lock().unwrap().push_back(Frame { raw: value });
    }

    #[cfg(any(feature = "devtools", test))]
    pub(crate) fn emit(&self, event: &str, data: Value) {
        self.push(emit_frame(event, data));
    }

    pub(crate) fn drain(&self) -> Vec<Frame> {
        self.frames.lock().unwrap().drain(..).collect()
    }

    #[cfg(feature = "core")]
    pub(crate) fn len(&self) -> usize {
        self.frames.lock().unwrap().len()
    }
}

/// Map a `fatal` payload (`{code, message, holder?}`) onto the link status.
pub(crate) fn fatal_status(data: &Value) -> LinkStatus {
    let text = |key: &str| data.get(key).and_then(Value::as_str).map(str::to_string);
    LinkStatus::Fatal {
        code: text("code").unwrap_or_else(|| "fatal".into()),
        message: text("message").unwrap_or_else(|| "The engine stopped during startup.".into()),
        holder: text("holder"),
    }
}

// MARK: - Backends

/// Transport behind [`Engine`]. Implementations MUST NOT block the caller:
/// `call` queues the request and the response later shows up in `poll` as a
/// `{"id", "result"|"error"}` frame; pushes arrive as `{"event","ts","data"}`.
pub(crate) trait Backend: Send + Sync {
    /// Queue a request. `Err` means the request could not be sent at all.
    fn call(&self, id: u64, method: &str, params: Value) -> Result<(), String>;
    fn poll(&self) -> Vec<Frame>;
    fn status(&self) -> LinkStatus;
    fn hello(&self) -> Option<Value>;
    fn stderr_tail(&self) -> Vec<String> {
        Vec::new()
    }
    /// Retry after a fatal startup condition (instance busy, engine missing).
    fn retry(&self) {}
    fn shutdown(&self);
    fn is_mock(&self) -> bool {
        false
    }
}

mod stdio;

#[cfg(feature = "core")]
mod core;

/// Which transport `Engine::start` picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackendKind {
    #[cfg(any(feature = "devtools", test))]
    Mock,
    #[cfg(feature = "core")]
    InProcess,
    Stdio,
}

impl BackendKind {
    /// `mock` wins; otherwise `GN_DRIVE_BACKEND=stdio` selects the child
    /// process; the default is the in-process core when it is compiled in.
    pub(crate) fn select(mock: bool, env: Option<&str>) -> Self {
        #[cfg(any(feature = "devtools", test))]
        if mock {
            return Self::Mock;
        }
        #[cfg(not(any(feature = "devtools", test)))]
        let _ = mock;
        match env.map(str::trim) {
            Some("stdio") => Self::Stdio,
            #[cfg(feature = "core")]
            _ => Self::InProcess,
            #[cfg(not(feature = "core"))]
            _ => Self::Stdio,
        }
    }
}

pub struct Engine {
    backend: Box<dyn Backend>,
    next_id: AtomicU64,
}

impl Engine {
    pub fn start(mock: bool) -> Self {
        let env = std::env::var("GN_DRIVE_BACKEND").ok();
        let backend: Box<dyn Backend> = match BackendKind::select(mock, env.as_deref()) {
            #[cfg(any(feature = "devtools", test))]
            BackendKind::Mock => Box::new(crate::mock::MockBackend::new()),
            #[cfg(feature = "core")]
            BackendKind::InProcess => Box::new(core::CoreBackend::new()),
            BackendKind::Stdio => Box::new(stdio::StdioBackend::new()),
        };
        Self::with_backend(backend)
    }

    pub(crate) fn with_backend(backend: Box<dyn Backend>) -> Self {
        Self {
            backend,
            next_id: AtomicU64::new(0),
        }
    }

    pub fn is_mock(&self) -> bool {
        self.backend.is_mock()
    }

    pub fn stderr_tail(&self) -> Vec<String> {
        self.backend.stderr_tail()
    }

    pub fn status(&self) -> LinkStatus {
        self.backend.status()
    }

    pub fn hello(&self) -> Option<Value> {
        self.backend.hello()
    }

    pub fn request(&self, method: &str, params: Value) -> Result<u64, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        self.backend.call(id, method, params)?;
        Ok(id)
    }

    pub fn poll(&self) -> Vec<Frame> {
        self.backend.poll()
    }

    /// Retry after a fatal startup condition (instance busy, engine missing).
    pub fn retry(&self) {
        self.backend.retry();
    }

    pub fn shutdown(&self) {
        self.backend.shutdown();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_uses_swift_acronym_spelling() {
        let flow: Flow = serde_json::from_value(json!({
            "id": "F1",
            "operations": [{"id": "O1", "flowID": "F1", "syncConfig": {"parallel": 4}}]
        }))
        .unwrap();
        assert_eq!(flow.operations[0].flow_id, "F1");
        assert_eq!(flow.operations[0].sync_config["parallel"], 4);
        let value = serde_json::to_value(&flow).unwrap();
        assert_eq!(value["operations"][0]["flowID"], "F1");
        assert!(value.get("canvasJSON").is_some());
    }

    #[test]
    fn frame_reads_errors_and_results() {
        let frame = Frame {
            raw: json!({"id": 3, "error": {"code": "locked", "message": "auth: locked"}}),
        };
        assert_eq!(frame.id(), Some(3));
        assert_eq!(frame.error().unwrap().code, "locked");
        assert!(frame.result().is_none());
    }

    #[test]
    fn facade_round_trips_over_the_mock_backend() {
        let engine = Engine::with_backend(Box::new(crate::mock::MockBackend::new()));
        assert!(engine.is_mock());
        assert!(engine.status().is_ready());
        assert!(engine.hello().is_some());
        let first = engine.request("auth.status", json!({})).unwrap();
        let second = engine.request("ping", json!({})).unwrap();
        assert_eq!((first, second), (1, 2));
        let frames = engine.poll();
        let ids: Vec<_> = frames.iter().filter_map(Frame::id).collect();
        assert_eq!(ids, vec![1, 2]);
        assert!(frames[0].result().is_some());
        assert!(engine.poll().is_empty(), "poll drains the inbox");
    }

    #[test]
    fn facade_delivers_pushed_events_and_errors() {
        let engine = Engine::with_backend(Box::new(crate::mock::MockBackend::new()));
        engine
            .request("auth.unlock", json!({"password": "drive1234"}))
            .unwrap();
        let frames = engine.poll();
        assert!(frames.iter().any(|f| f.event() == Some("auth:unlocked")));
        let id = engine.request("no.such.method", json!({})).unwrap();
        let frames = engine.poll();
        let frame = frames.iter().find(|f| f.id() == Some(id)).unwrap();
        assert!(frame.error().is_some());
    }

    #[test]
    fn backend_selection_prefers_mock_then_env() {
        assert_eq!(BackendKind::select(true, Some("stdio")), BackendKind::Mock);
        assert_eq!(
            BackendKind::select(false, Some("stdio")),
            BackendKind::Stdio
        );
        assert_eq!(
            BackendKind::select(false, Some(" stdio ")),
            BackendKind::Stdio
        );
        #[cfg(feature = "core")]
        assert_eq!(BackendKind::select(false, None), BackendKind::InProcess);
        #[cfg(not(feature = "core"))]
        assert_eq!(BackendKind::select(false, None), BackendKind::Stdio);
    }

    #[test]
    fn fatal_payload_maps_to_link_status() {
        let status = fatal_status(&json!({
            "code": "instance_busy", "message": "busy", "holder": "service"
        }));
        assert_eq!(
            status,
            LinkStatus::Fatal {
                code: "instance_busy".into(),
                message: "busy".into(),
                holder: Some("service".into()),
            }
        );
    }
}
