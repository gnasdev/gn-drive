//! Sequential flow execution — port of `FlowEngine.swift` / `internal/flowengine`.
//!
//! A flow runs its operations one after another through
//! [`SyncEngine::start_path_sync`] + [`SyncEngine::wait_task`] using the busy
//! key `flowID:opID`. Status transitions are published on `flow:execution`
//! (and mirrored on `board:execution` for older clients).

use crate::event_bus::{BusEvent, EventBus};
use crate::logger::Logger;
use crate::models::{JsonDict, Profile, compose_path};
use crate::store::{Store, StoreError};
use crate::sync_engine::{FlowExecutor, SyncEngine, SyncEngineError};
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread::JoinHandle;

/// Display texts match the Swift RPC messages (`rpcMapError`).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FlowEngineError {
    #[error("flow is already running")]
    AlreadyRunning,
    #[error("flow is not running")]
    NotRunning,
    #[error("flow has no operations")]
    EmptyFlow,
    #[error("flow engine is not ready")]
    EngineNotReady,
    /// Store failure while loading the flow (Swift rethrows the store error).
    #[error(transparent)]
    Store(#[from] StoreError),
}

struct Run {
    cancelled: bool,
    status: String,
}

#[derive(Default)]
struct State {
    runs: HashMap<String, Run>,
    last_status: HashMap<String, String>,
}

pub struct FlowEngine {
    store: Mutex<Option<Arc<Store>>>,
    sync: Mutex<Option<Arc<SyncEngine>>>,
    bus: EventBus,
    log: Logger,
    state: Mutex<State>,
}

impl FlowEngine {
    pub fn new(log: Logger, bus: EventBus) -> Arc<FlowEngine> {
        Arc::new(FlowEngine {
            store: Mutex::new(None),
            sync: Mutex::new(None),
            bus,
            log,
            state: Mutex::new(State::default()),
        })
    }

    pub fn attach(&self, store: Arc<Store>, sync_engine: Arc<SyncEngine>) {
        *self.store.lock() = Some(store);
        *self.sync.lock() = Some(sync_engine);
    }

    pub fn detach(&self) {
        *self.store.lock() = None;
        *self.sync.lock() = None;
    }

    pub fn is_running(&self, flow_id: &str) -> bool {
        self.state.lock().runs.contains_key(flow_id)
    }

    /// Active run → running/cancelling; after finish → last terminal status;
    /// never started → "idle".
    pub fn status(&self, flow_id: &str) -> String {
        let st = self.state.lock();
        if let Some(r) = st.runs.get(flow_id) {
            return r.status.clone();
        }
        st.last_status
            .get(flow_id)
            .cloned()
            .unwrap_or_else(|| "idle".into())
    }

    pub fn statuses(&self) -> HashMap<String, String> {
        let st = self.state.lock();
        let mut out: HashMap<String, String> = st
            .last_status
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (id, r) in &st.runs {
            if !r.status.is_empty() {
                out.insert(id.clone(), r.status.clone());
            }
        }
        out
    }

    /// Validate and register a run (publishing `running`); the caller then
    /// drives [`Self::run`].
    fn begin(
        &self,
        flow_id: &str,
    ) -> Result<(crate::models::Flow, Arc<SyncEngine>), FlowEngineError> {
        let store = self.store.lock().clone();
        let sync = self.sync.lock().clone();
        let (Some(store), Some(sync)) = (store, sync) else {
            return Err(FlowEngineError::EngineNotReady);
        };
        let f = store.get_flow(flow_id)?;
        if f.operations.is_empty() {
            return Err(FlowEngineError::EmptyFlow);
        }
        {
            let mut st = self.state.lock();
            if st.runs.contains_key(flow_id) {
                return Err(FlowEngineError::AlreadyRunning);
            }
            st.runs.insert(
                flow_id.to_string(),
                Run {
                    cancelled: false,
                    status: "running".into(),
                },
            );
            st.last_status.remove(flow_id);
        }
        self.publish(flow_id, "", "running", "");
        Ok((f, sync))
    }

    /// Begin sequential execution in the background (Swift `execute`).
    /// The returned handle completes when the flow has finished.
    pub fn start(self: &Arc<Self>, flow_id: &str) -> Result<JoinHandle<()>, FlowEngineError> {
        let (f, sync) = self.begin(flow_id)?;
        let me = self.clone();
        Ok(std::thread::Builder::new()
            .name(format!("flow-{flow_id}"))
            .spawn(move || me.run(&f, &sync))
            .expect("spawn flow thread"))
    }

    /// Execute the flow on the calling thread; returns once it has finished
    /// (completed, failed or cancelled — see [`Self::status`]). Errors only
    /// for pre-run failures.
    pub fn execute(&self, flow_id: &str) -> Result<(), FlowEngineError> {
        let (f, sync) = self.begin(flow_id)?;
        self.run(&f, &sync);
        Ok(())
    }

    pub fn stop(&self, flow_id: &str) -> Result<(), FlowEngineError> {
        {
            let mut st = self.state.lock();
            let Some(r) = st.runs.get_mut(flow_id) else {
                return Err(FlowEngineError::NotRunning);
            };
            r.cancelled = true;
            r.status = "cancelling".into();
        }
        self.publish(flow_id, "", "cancelling", "");
        Ok(())
    }

    fn is_cancelled(&self, flow_id: &str) -> bool {
        self.state
            .lock()
            .runs
            .get(flow_id)
            .map(|r| r.cancelled)
            .unwrap_or(false)
    }

    fn run(&self, f: &crate::models::Flow, sync: &SyncEngine) {
        let final_status = self.run_ops(f, sync);
        {
            let mut st = self.state.lock();
            st.runs.remove(&f.id);
            st.last_status
                .insert(f.id.clone(), final_status.to_string());
        }
        self.publish(&f.id, "", final_status, "");
        self.log.info(
            "flow finished",
            &[("flow", &f.id), ("status", final_status)],
        );
    }

    fn run_ops(&self, f: &crate::models::Flow, sync: &SyncEngine) -> &'static str {
        for op in &f.operations {
            if self.is_cancelled(&f.id) {
                return "cancelled";
            }
            // From=source, To=target always; pull reverses inside rclone.
            let action = op.resolved_action();
            let from = compose_path(&op.source_remote, &op.source_path);
            let to = compose_path(&op.target_remote, &op.target_path);
            let opts = Self::profile_from_sync_config(&op.sync_config);

            self.publish(&f.id, &op.id, "running", "");

            let busy_key = format!("{}:{}", f.id, op.id);
            let task_id = match sync.start_path_sync(&action, &busy_key, &from, &to, Some(opts)) {
                Ok(id) => id,
                Err(e) => {
                    self.log.error(
                        "flow op start failed",
                        &[("flow", &f.id), ("op", &op.id), ("err", &e.to_string())],
                    );
                    self.publish(&f.id, &op.id, "failed", &e.to_string());
                    return "failed";
                }
            };

            // Wait for the task; on flow cancel, stop the task.
            let waited = sync.wait_task(&task_id, &|| self.is_cancelled(&f.id));
            if let Err(err) = waited {
                if self.is_cancelled(&f.id) || matches!(err, SyncEngineError::TaskCancelled) {
                    let _ = sync.stop_sync(&task_id);
                    self.publish(&f.id, &op.id, "cancelled", "");
                    return "cancelled";
                }
                let msg = friendly_task_err(&err);
                self.log.error(
                    "flow op failed",
                    &[("flow", &f.id), ("op", &op.id), ("err", &err.to_string())],
                );
                self.publish(&f.id, &op.id, "failed", &msg);
                return "failed";
            }
            self.publish(&f.id, &op.id, "completed", "");
        }
        "completed"
    }

    fn publish(&self, flow_id: &str, op_id: &str, status: &str, msg: &str) {
        self.bus
            .publish(BusEvent::flow_execution(flow_id, op_id, status, msg));
        // Also emit board:execution for older clients.
        self.bus
            .publish(BusEvent::board_execution(flow_id, op_id, "", status, msg));
    }

    #[cfg(test)]
    pub(crate) fn set_last_status_for_test(&self, flow_id: &str, status: &str) {
        self.state
            .lock()
            .last_status
            .insert(flow_id.to_string(), status.to_string());
    }

    // MARK: - sync_config → Profile flags (port of syncconfig.go)

    /// Map a flow operation's `sync_config` bag (snake_case or camelCase keys)
    /// onto a [`Profile`] used as the flag carrier for `start_path_sync`.
    pub fn profile_from_sync_config(m: &JsonDict) -> Profile {
        let mut p = Profile {
            parallel: 4,
            ..Profile::default()
        };

        if let Some(v) = bool_v(m, &["dry_run", "dryRun"]) {
            p.dry_run = v;
        }
        if let Some(v) = int_v(m, &["parallel"]).filter(|v| *v > 0) {
            p.parallel = v;
        }
        if let Some(v) = int_v(m, &["bandwidth"]).filter(|v| *v > 0) {
            p.bandwidth = v;
        }
        if let Some(v) =
            int_v(m, &["multi_thread_streams", "multiThreadStreams"]).filter(|v| *v > 0)
        {
            p.multi_thread_streams = Some(v);
        }
        if let Some(v) = string_v(m, &["buffer_size", "bufferSize"]) {
            p.buffer_size = v;
        }
        if let Some(v) = int_v(m, &["retries"]).filter(|v| *v > 0) {
            p.retries = Some(v);
        }
        if let Some(v) = int_v(m, &["low_level_retries", "lowLevelRetries"]).filter(|v| *v > 0) {
            p.low_level_retries = Some(v);
        }
        if let Some(v) = string_v(m, &["max_duration", "maxDuration"]) {
            p.max_duration = v;
        }
        if let Some(v) = bool_v(m, &["check_first", "checkFirst"]) {
            p.check_first = v;
        }
        if let Some(v) = string_v(m, &["order_by", "orderBy"]) {
            p.order_by = v;
        }
        if let Some(v) = string_v(m, &["retries_sleep", "retriesSleep"]) {
            p.retries_sleep = v;
        }
        if let Some(v) = double_v(m, &["tps_limit", "tpsLimit"]).filter(|v| *v > 0.0) {
            p.tps_limit = Some(v);
        }
        if let Some(v) = string_v(m, &["conn_timeout", "connTimeout"]) {
            p.conn_timeout = v;
        }
        if let Some(v) = string_v(m, &["io_timeout", "ioTimeout"]) {
            p.io_timeout = v;
        }

        if let Some(v) = strings_v(m, &["included_paths", "includedPaths"]) {
            p.included_paths = v;
        }
        if let Some(v) = strings_v(m, &["excluded_paths", "excludedPaths"]) {
            p.excluded_paths = v;
        }
        if let Some(v) = string_v(m, &["min_size", "minSize"]) {
            p.min_size = v;
        }
        if let Some(v) = string_v(m, &["max_size", "maxSize"]) {
            p.max_size = v;
        }
        if let Some(v) = string_v(m, &["max_age", "maxAge"]) {
            p.max_age = v;
        }
        if let Some(v) = string_v(m, &["min_age", "minAge"]) {
            p.min_age = v;
        }
        if let Some(v) = int_v(m, &["max_depth", "maxDepth"]).filter(|v| *v > 0) {
            p.max_depth = Some(v);
        }
        if let Some(v) = string_v(m, &["filter_from_file", "filterFromFile"]) {
            p.filter_from_file = v;
        }
        if let Some(v) = string_v(m, &["exclude_if_present", "excludeIfPresent"]) {
            p.exclude_if_present = v;
        }
        if let Some(v) = bool_v(m, &["use_regex", "useRegex"]) {
            p.use_regex = v;
        }
        if let Some(v) = bool_v(m, &["delete_excluded", "deleteExcluded"]) {
            p.delete_excluded = v;
        }

        if let Some(v) = int_v(m, &["max_delete", "maxDelete"]).filter(|v| *v > 0) {
            p.max_delete = Some(v);
        }
        if let Some(v) = bool_v(m, &["immutable"]) {
            p.immutable = v;
        }
        if let Some(v) = string_v(m, &["max_transfer", "maxTransfer"]) {
            p.max_transfer = v;
        }
        if let Some(v) = string_v(m, &["max_delete_size", "maxDeleteSize"]) {
            p.max_delete_size = v;
        }
        if let Some(v) = string_v(m, &["suffix"]) {
            p.suffix = v;
        }
        if let Some(v) = bool_v(m, &["suffix_keep_extension", "suffixKeepExtension"]) {
            p.suffix_keep_extension = v;
        }
        if let Some(v) = string_v(m, &["backup_path", "backupPath"]) {
            p.backup_path = v;
        }

        if let Some(v) = bool_v(m, &["size_only", "sizeOnly"]) {
            p.size_only = v;
        }
        if let Some(v) = bool_v(m, &["update_mode", "updateMode"]) {
            p.update_mode = v;
        }
        if let Some(v) = bool_v(m, &["ignore_existing", "ignoreExisting"]) {
            p.ignore_existing = v;
        }

        if let Some(v) = string_v(m, &["delete_timing", "deleteTiming"]) {
            p.delete_timing = v;
        }

        if let Some(v) = string_v(m, &["conflict_resolution", "conflictResolution"]) {
            p.conflict_resolution = v;
        }
        if let Some(v) = bool_v(m, &["resilient"]) {
            p.resilient = v;
        }
        if let Some(v) = string_v(m, &["max_lock", "maxLock"]) {
            p.max_lock = v;
        }
        if let Some(v) = bool_v(m, &["check_access", "checkAccess"]) {
            p.check_access = v;
        }
        if let Some(v) = string_v(m, &["conflict_loser", "conflictLoser"]) {
            p.conflict_loser = v;
        }
        if let Some(v) = string_v(m, &["conflict_suffix", "conflictSuffix"]) {
            p.conflict_suffix = v;
        }

        p
    }
}

impl FlowExecutor for FlowEngine {
    fn execute(&self, flow_id: &str) -> Result<(), String> {
        FlowEngine::execute(self, flow_id).map_err(|e| e.to_string())
    }
}

// MARK: - value helpers (Swift `.whitespaces` trimming semantics)

fn trim_ws(s: &str) -> &str {
    s.trim_matches(|c: char| c == '\t' || (c.is_whitespace() && c != '\n' && c != '\r'))
}

fn bool_v(m: &JsonDict, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|k| match m.get(*k) {
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    })
}

fn int_v(m: &JsonDict, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|k| match m.get(*k)? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => trim_ws(s).parse::<i64>().ok(),
        _ => None,
    })
}

fn double_v(m: &JsonDict, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|k| match m.get(*k)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => trim_ws(s).parse::<f64>().ok(),
        _ => None,
    })
}

fn string_v(m: &JsonDict, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| match m.get(*k)? {
        Value::String(s) if !trim_ws(s).is_empty() => Some(trim_ws(s).to_string()),
        _ => None,
    })
}

fn strings_v(m: &JsonDict, keys: &[&str]) -> Option<Vec<String>> {
    keys.iter().find_map(|k| match m.get(*k)? {
        Value::Array(a) => Some(
            a.iter()
                .filter_map(|v| v.as_str())
                .map(trim_ws)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
        ),
        _ => None,
    })
}

/// Task error → short, user-facing text (no wrapper noise, stderr tail cut,
/// ≤ 200 chars).
fn friendly_task_err(err: &SyncEngineError) -> String {
    let mut s = match err {
        SyncEngineError::TaskFailed(m) => m.clone(),
        other => other.to_string(),
    };
    for prefix in ["syncengine: task failed\n", "syncengine: task failed: "] {
        s = s.replace(prefix, "");
    }
    let mut s = s
        .trim_matches(|c: char| matches!(c, '(' | ')' | '"' | '\'' | ' '))
        .to_string();
    if let Some(i) = s.find("(stderr:") {
        s = trim_ws(&s[..i]).to_string();
    }
    if s.chars().count() > 200 {
        s = s.chars().take(200).collect::<String>() + "…";
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::{Subscriber, topic};
    use crate::models::{Flow, FlowOperation};
    use crate::rclone::SyncClient;
    use crate::rclone::client::RcloneClient;
    use crate::rclone::testing::{FakeRunner, Response};
    use serde_json::json;
    use std::time::{Duration, Instant};

    fn dict(v: Value) -> JsonDict {
        v.as_object().cloned().unwrap()
    }

    // --- profile_from_sync_config -------------------------------------

    #[test]
    fn profile_snake_and_camel() {
        let p = FlowEngine::profile_from_sync_config(&dict(json!({
            "dry_run": true,
            "parallel": 8,
            "multiThreadStreams": 4,
            "excludedPaths": ["*.tmp", "node_modules"],
            "maxDepth": 3,
            "bandwidth": 10
        })));
        assert!(p.dry_run);
        assert_eq!(p.parallel, 8);
        assert_eq!(p.bandwidth, 10);
        assert_eq!(p.multi_thread_streams, Some(4));
        assert_eq!(p.max_depth, Some(3));
        assert_eq!(p.excluded_paths, vec!["*.tmp", "node_modules"]);
    }

    #[test]
    fn profile_empty_defaults() {
        let p = FlowEngine::profile_from_sync_config(&JsonDict::new());
        assert_eq!(p.parallel, 4);
        assert_eq!(
            p,
            Profile {
                parallel: 4,
                ..Profile::default()
            }
        );
    }

    #[test]
    fn profile_full_key_matrix_snake() {
        let p = FlowEngine::profile_from_sync_config(&dict(json!({
            "dry_run": true, "parallel": 6, "bandwidth": 50, "multi_thread_streams": 3,
            "buffer_size": "16M", "retries": 5, "low_level_retries": 7,
            "max_duration": "1h", "check_first": true, "order_by": "size,desc",
            "retries_sleep": "2s", "tps_limit": 2.5, "conn_timeout": "30s", "io_timeout": "1m",
            "included_paths": ["a", " b "], "excluded_paths": ["c"],
            "min_size": "1k", "max_size": "1G", "max_age": "7d", "min_age": "1d",
            "max_depth": 2, "filter_from_file": "/f", "exclude_if_present": ".nosync",
            "use_regex": true, "delete_excluded": true,
            "max_delete": 9, "immutable": true, "max_transfer": "5G", "max_delete_size": "1G",
            "suffix": ".bak", "suffix_keep_extension": true, "backup_path": "/bk",
            "size_only": true, "update_mode": true, "ignore_existing": true,
            "delete_timing": "after", "conflict_resolution": "newer", "resilient": true,
            "max_lock": "5m", "check_access": true, "conflict_loser": "delete",
            "conflict_suffix": ".c"
        })));
        let expect = Profile {
            dry_run: true,
            parallel: 6,
            bandwidth: 50,
            multi_thread_streams: Some(3),
            buffer_size: "16M".into(),
            retries: Some(5),
            low_level_retries: Some(7),
            max_duration: "1h".into(),
            check_first: true,
            order_by: "size,desc".into(),
            retries_sleep: "2s".into(),
            tps_limit: Some(2.5),
            conn_timeout: "30s".into(),
            io_timeout: "1m".into(),
            included_paths: vec!["a".into(), "b".into()],
            excluded_paths: vec!["c".into()],
            min_size: "1k".into(),
            max_size: "1G".into(),
            max_age: "7d".into(),
            min_age: "1d".into(),
            max_depth: Some(2),
            filter_from_file: "/f".into(),
            exclude_if_present: ".nosync".into(),
            use_regex: true,
            delete_excluded: true,
            max_delete: Some(9),
            immutable: true,
            max_transfer: "5G".into(),
            max_delete_size: "1G".into(),
            suffix: ".bak".into(),
            suffix_keep_extension: true,
            backup_path: "/bk".into(),
            size_only: true,
            update_mode: true,
            ignore_existing: true,
            delete_timing: "after".into(),
            conflict_resolution: "newer".into(),
            resilient: true,
            max_lock: "5m".into(),
            check_access: true,
            conflict_loser: "delete".into(),
            conflict_suffix: ".c".into(),
            ..Profile::default()
        };
        assert_eq!(p, expect);
    }

    #[test]
    fn profile_full_key_matrix_camel_matches_snake() {
        let snake = FlowEngine::profile_from_sync_config(&dict(json!({
            "dry_run": true, "multi_thread_streams": 3, "buffer_size": "16M",
            "low_level_retries": 7, "max_duration": "1h", "check_first": true,
            "order_by": "x", "retries_sleep": "2s", "tps_limit": 1.5,
            "conn_timeout": "3s", "io_timeout": "4s", "included_paths": ["a"],
            "excluded_paths": ["b"], "min_size": "1", "max_size": "2", "max_age": "3",
            "min_age": "4", "max_depth": 5, "filter_from_file": "f",
            "exclude_if_present": "e", "use_regex": true, "delete_excluded": true,
            "max_delete": 6, "max_transfer": "7", "max_delete_size": "8",
            "suffix_keep_extension": true, "backup_path": "bp", "size_only": true,
            "update_mode": true, "ignore_existing": true, "delete_timing": "during",
            "conflict_resolution": "older", "max_lock": "9", "check_access": true,
            "conflict_loser": "l", "conflict_suffix": "s"
        })));
        let camel = FlowEngine::profile_from_sync_config(&dict(json!({
            "dryRun": true, "multiThreadStreams": 3, "bufferSize": "16M",
            "lowLevelRetries": 7, "maxDuration": "1h", "checkFirst": true,
            "orderBy": "x", "retriesSleep": "2s", "tpsLimit": 1.5,
            "connTimeout": "3s", "ioTimeout": "4s", "includedPaths": ["a"],
            "excludedPaths": ["b"], "minSize": "1", "maxSize": "2", "maxAge": "3",
            "minAge": "4", "maxDepth": 5, "filterFromFile": "f",
            "excludeIfPresent": "e", "useRegex": true, "deleteExcluded": true,
            "maxDelete": 6, "maxTransfer": "7", "maxDeleteSize": "8",
            "suffixKeepExtension": true, "backupPath": "bp", "sizeOnly": true,
            "updateMode": true, "ignoreExisting": true, "deleteTiming": "during",
            "conflictResolution": "older", "maxLock": "9", "checkAccess": true,
            "conflictLoser": "l", "conflictSuffix": "s"
        })));
        assert_eq!(snake, camel);
        assert_eq!(snake.max_delete, Some(6));
        assert_eq!(snake.delete_timing, "during");
    }

    #[test]
    fn profile_value_coercion_and_rejection() {
        let p = FlowEngine::profile_from_sync_config(&dict(json!({
            "parallel": "12",          // numeric string
            "bandwidth": 7.9,          // double truncates
            "retries": 0,              // non-positive ignored
            "max_depth": -1,
            "max_delete": "0",
            "tps_limit": "0",
            "buffer_size": "   ",      // blank ignored
            "suffix": "  .x  ",        // trimmed
            "dry_run": "true",         // strings are not bools
            "included_paths": ["", "  ", "ok"],
            "excluded_paths": "not-an-array"
        })));
        assert_eq!(p.parallel, 12);
        assert_eq!(p.bandwidth, 7);
        assert_eq!(p.retries, None);
        assert_eq!(p.max_depth, None);
        assert_eq!(p.max_delete, None);
        assert_eq!(p.tps_limit, None);
        assert_eq!(p.buffer_size, "");
        assert_eq!(p.suffix, ".x");
        assert!(!p.dry_run);
        assert_eq!(p.included_paths, vec!["ok"]);
        assert!(p.excluded_paths.is_empty());

        // Non-positive parallel keeps the default of 4.
        let q = FlowEngine::profile_from_sync_config(&dict(json!({"parallel": 0})));
        assert_eq!(q.parallel, 4);
        // The first key that yields a value wins; invalid first key falls through.
        let r = FlowEngine::profile_from_sync_config(&dict(json!({
            "dry_run": "x", "dryRun": true
        })));
        assert!(r.dry_run);
    }

    #[test]
    fn friendly_error_text() {
        let e = SyncEngineError::TaskFailed("rclone sync a b: exit 1 (stderr: boom\n)".into());
        assert_eq!(friendly_task_err(&e), "rclone sync a b: exit 1");
        let long = SyncEngineError::TaskFailed("x".repeat(300));
        let s = friendly_task_err(&long);
        assert_eq!(s.chars().count(), 201);
        assert!(s.ends_with('…'));
        let pre = SyncEngineError::TaskFailed("syncengine: task failed: nope".into());
        assert_eq!(friendly_task_err(&pre), "nope");
    }

    // --- engine integration (FakeRunner through real SyncEngine + Store)

    struct Stack {
        _dir: tempfile::TempDir,
        store: Arc<Store>,
        sync: Arc<SyncEngine>,
        flows: Arc<FlowEngine>,
        fake: Arc<FakeRunner>,
        sub: Subscriber,
    }

    impl Drop for Stack {
        fn drop(&mut self) {
            self.sync.stop();
        }
    }

    fn stack() -> Stack {
        let dir = tempfile::tempdir().unwrap();
        let log = Logger::quiet();
        let store =
            Arc::new(Store::open(dir.path().join("db.db").to_str().unwrap(), &log).unwrap());
        let fake = Arc::new(FakeRunner::new());
        let client: Arc<dyn SyncClient> = Arc::new(RcloneClient::with_runner(
            fake.clone(),
            dir.path().join("rclone.conf").to_str().unwrap(),
            &log,
        ));
        let bus = EventBus::new();
        let sub = bus.subscribe(&[topic::FLOW_EXECUTION]);
        let sync = SyncEngine::new(log.clone(), bus.clone());
        sync.attach(store.clone(), client);
        sync.start();
        let flows = FlowEngine::new(log, bus);
        flows.attach(store.clone(), sync.clone());
        Stack {
            _dir: dir,
            store,
            sync,
            flows,
            fake,
            sub,
        }
    }

    fn save_flow(st: &Stack, id: &str, ops: &[&str]) {
        let mut f = Flow {
            id: id.into(),
            name: "t".into(),
            operations: ops
                .iter()
                .enumerate()
                .map(|(i, op)| FlowOperation {
                    id: (*op).into(),
                    flow_id: id.into(),
                    source_path: format!("/tmp/gnd-src-{i}"),
                    target_path: format!("/tmp/gnd-dst-{i}"),
                    action: "push".into(),
                    sort_order: i as i64,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        st.store.save_flow(&mut f).unwrap();
    }

    fn wait_until(what: &str, cond: impl Fn() -> bool) {
        let t = Instant::now();
        while !cond() {
            assert!(t.elapsed() < Duration::from_secs(5), "timeout: {what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// (flow, op, status, error) tuples drained from the subscription.
    fn drain(st: &Stack) -> Vec<(String, String, String, String)> {
        let mut out = Vec::new();
        while let Some(BusEvent::FlowExecution(e)) = st.sub.try_recv() {
            out.push((e.flow_id, e.op_id, e.status, e.error));
        }
        out
    }

    fn ev(f: &str, o: &str, s: &str, e: &str) -> (String, String, String, String) {
        (f.into(), o.into(), s.into(), e.into())
    }

    #[test]
    fn not_ready_missing_empty_and_not_running() {
        let flows = FlowEngine::new(Logger::quiet(), EventBus::new());
        assert_eq!(flows.execute("x"), Err(FlowEngineError::EngineNotReady));
        assert_eq!(flows.stop("nope"), Err(FlowEngineError::NotRunning));
        assert_eq!(flows.status("nope"), "idle");
        assert!(flows.statuses().is_empty());
        assert_eq!(
            FlowEngineError::EmptyFlow.to_string(),
            "flow has no operations"
        );

        let st = stack();
        assert_eq!(
            st.flows.execute("missing"),
            Err(FlowEngineError::Store(StoreError::NotFound))
        );
        save_flow(&st, "empty", &[]);
        assert_eq!(st.flows.execute("empty"), Err(FlowEngineError::EmptyFlow));
        assert!(!st.flows.is_running("empty"));
        assert!(drain(&st).is_empty(), "no events for rejected starts");
    }

    #[test]
    fn execute_runs_ops_sequentially_and_retains_last_status() {
        let st = stack();
        save_flow(&st, "f1", &["op1", "op2"]);
        st.flows.execute("f1").unwrap();
        assert_eq!(
            drain(&st),
            vec![
                ev("f1", "", "running", ""),
                ev("f1", "op1", "running", ""),
                ev("f1", "op1", "completed", ""),
                ev("f1", "op2", "running", ""),
                ev("f1", "op2", "completed", ""),
                ev("f1", "", "completed", ""),
            ]
        );
        assert!(!st.flows.is_running("f1"));
        assert_eq!(st.flows.status("f1"), "completed");
        assert_eq!(
            st.flows.statuses().get("f1").map(String::as_str),
            Some("completed")
        );
        assert_eq!(st.fake.calls_for("sync").len(), 2);

        // A new run clears the retained status until it finishes again.
        st.flows.execute("f1").unwrap();
        assert_eq!(st.flows.status("f1"), "completed");
    }

    #[test]
    fn op_failure_stops_flow() {
        let st = stack();
        st.fake.on(
            &["sync"],
            Response::new().stderr("Failed to sync: boom").exit(1),
        );
        save_flow(&st, "f2", &["op1", "op2"]);
        st.flows.execute("f2").unwrap();
        let evs = drain(&st);
        assert_eq!(evs.len(), 4, "{evs:?}");
        assert_eq!(evs[0], ev("f2", "", "running", ""));
        assert_eq!(evs[1], ev("f2", "op1", "running", ""));
        assert_eq!((evs[2].1.as_str(), evs[2].2.as_str()), ("op1", "failed"));
        assert!(!evs[2].3.is_empty());
        assert!(!evs[2].3.contains("(stderr:"));
        assert_eq!(evs[3], ev("f2", "", "failed", ""));
        assert_eq!(st.flows.status("f2"), "failed");
        assert_eq!(st.fake.calls_for("sync").len(), 1, "op2 never started");
    }

    #[test]
    fn start_stop_cancels_mid_run() {
        let st = stack();
        st.fake
            .on(&["sync"], Response::new().hang_until_cancelled());
        save_flow(&st, "f3", &["op1", "op2"]);
        let h = st.flows.start("f3").unwrap();
        assert!(st.flows.is_running("f3"));
        assert_eq!(st.flows.status("f3"), "running");
        assert_eq!(st.flows.execute("f3"), Err(FlowEngineError::AlreadyRunning));

        // Busy key is flowID:opID while the op is live.
        wait_until("task visible", || {
            st.sync.active_tasks().iter().any(|t| t.name == "f3:op1")
        });

        st.flows.stop("f3").unwrap();
        assert_eq!(st.flows.status("f3"), "cancelling");
        h.join().unwrap();

        assert!(!st.flows.is_running("f3"));
        assert_eq!(st.flows.status("f3"), "cancelled");
        let evs = drain(&st);
        assert_eq!(evs[0], ev("f3", "", "running", ""));
        assert!(evs.contains(&ev("f3", "", "cancelling", "")));
        assert!(evs.contains(&ev("f3", "op1", "cancelled", "")));
        assert_eq!(evs.last().unwrap(), &ev("f3", "", "cancelled", ""));
        assert!(
            !evs.iter().any(|e| e.1 == "op2"),
            "op2 must not start after cancel: {evs:?}"
        );
    }

    #[test]
    fn start_failure_reports_op_failed() {
        let st = stack();
        save_flow(&st, "f4", &["op1"]);
        // Engine stopped → start_path_sync fails with NotRunning.
        st.sync.stop();
        st.flows.execute("f4").unwrap();
        let evs = drain(&st);
        assert_eq!(evs[1], ev("f4", "op1", "running", ""));
        assert_eq!(
            evs[2],
            ev(
                "f4",
                "op1",
                "failed",
                &SyncEngineError::NotRunning.to_string()
            )
        );
        assert_eq!(st.flows.status("f4"), "failed");
    }

    #[test]
    fn statuses_merge_runs_over_last() {
        let flows = FlowEngine::new(Logger::quiet(), EventBus::new());
        flows.set_last_status_for_test("a", "completed");
        flows.set_last_status_for_test("b", "");
        flows.state.lock().runs.insert(
            "a".into(),
            Run {
                cancelled: false,
                status: "running".into(),
            },
        );
        let s = flows.statuses();
        assert_eq!(s.get("a").map(String::as_str), Some("running"));
        assert!(!s.contains_key("b"));
    }
}
