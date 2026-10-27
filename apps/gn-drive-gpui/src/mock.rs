//! In-process stand-in for `gn-drive serve --stdio`.
//!
//! Enabled with `GN_DRIVE_MOCK=1` or the devtools `mock` command. It speaks
//! the same frames, keeps a realistic fixture set, and drives a live sync so
//! the canvas, inspector, edge cards, and logs can be exercised before the
//! Swift server lands. It never touches the real config directory.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::engine::{
    AppSettings, AuthStatus, Backend, DoctorCheck, DoctorReport, FileEntry, FileTransfer, Flow,
    Frame, HealthStatus, HistoryEntry, Inbox, LinkStatus, Lockout, LogLine, Operation, Provider,
    Remote, RpcError, Runtime, RuntimeFlow, RuntimeLog, RuntimeOp, ServiceStatus, Snapshot,
    SyncProgress, UpdateCheck, now_ms,
};
use crate::graph;

/// [`Backend`] over the scripted [`Mock`]. Requests are answered
/// synchronously; `poll` first lets a running flow advance.
pub(crate) struct MockBackend {
    state: Mutex<Mock>,
    inbox: Inbox,
    hello: Value,
}

impl MockBackend {
    pub(crate) fn new() -> Self {
        let mut mock = Mock::new();
        mock.reset();
        let hello = mock.hello();
        Self {
            state: Mutex::new(mock),
            inbox: Inbox::default(),
            hello,
        }
    }
}

impl Backend for MockBackend {
    fn call(&self, id: u64, method: &str, params: Value) -> Result<(), String> {
        let result = self
            .state
            .lock()
            .unwrap()
            .handle(id, method, params, &self.inbox);
        match result {
            Ok(value) => self.inbox.push(json!({"id": id, "result": value})),
            Err(error) => self.inbox.push(json!({"id": id, "error": error})),
        }
        Ok(())
    }

    fn poll(&self) -> Vec<Frame> {
        self.state.lock().unwrap().advance(&self.inbox);
        self.inbox.drain()
    }

    fn status(&self) -> LinkStatus {
        LinkStatus::Ready
    }

    fn hello(&self) -> Option<Value> {
        Some(self.hello.clone())
    }

    fn shutdown(&self) {}

    fn is_mock(&self) -> bool {
        true
    }
}

const TICK: Duration = Duration::from_millis(450);

struct Run {
    flow_id: String,
    op_index: usize,
    op_id: String,
    task_id: String,
    action: String,
    files: Vec<(String, u64)>,
    file_index: usize,
    fraction: f64,
    files_done: u64,
    bytes_done: u64,
    started: Instant,
    last_emit: Instant,
}

pub struct Mock {
    locked: bool,
    setup: bool,
    password: String,
    failed: u32,
    locked_until: Option<Instant>,
    flows: Vec<Flow>,
    remotes: Vec<Remote>,
    profiles: Vec<Value>,
    history: Vec<HistoryEntry>,
    settings: AppSettings,
    runtime: Runtime,
    run: Option<Run>,
    logs: Vec<LogLine>,
    service: ServiceStatus,
    health: HealthStatus,
    oauth: Option<String>,
    next: u64,
}

impl Mock {
    pub fn new() -> Self {
        let mut mock = Self {
            locked: true,
            setup: true,
            password: "drive1234".into(),
            failed: 0,
            locked_until: None,
            flows: Vec::new(),
            remotes: Vec::new(),
            profiles: Vec::new(),
            history: Vec::new(),
            settings: AppSettings::default(),
            runtime: Runtime::default(),
            run: None,
            logs: Vec::new(),
            service: ServiceStatus {
                installed: true,
                running: false,
                pid: 0,
                mode: "service".into(),
                scope: "user".into(),
                label: "com.gndrive.app".into(),
                plist_path: format!(
                    "{}/Library/LaunchAgents/com.gndrive.app.plist",
                    std::env::var("HOME").unwrap_or_default()
                ),
            },
            health: HealthStatus::default(),
            oauth: None,
            next: 100,
        };
        mock.reset();
        mock
    }

    pub fn reset(&mut self) {
        self.locked = true;
        self.setup = true;
        self.password = "drive1234".into();
        self.failed = 0;
        self.locked_until = None;
        self.runtime = Runtime::default();
        self.run = None;
        self.oauth = None;
        self.next = 100;
        self.settings = AppSettings::default();
        self.flows = fixtures();
        self.remotes = vec![
            Remote {
                name: "gdrive".into(),
                kind: "drive".into(),
                description: "Google Drive".into(),
            },
            Remote {
                name: "nas".into(),
                kind: "sftp".into(),
                description: "Home NAS".into(),
            },
            Remote {
                name: "dropbox".into(),
                kind: "dropbox".into(),
                description: "Dropbox".into(),
            },
            Remote {
                name: LONG_REMOTE.into(),
                kind: "drive".into(),
                description: LONG_REMOTE_DESC.into(),
            },
        ];
        self.profiles = vec![
            json!({"name":"photos","from":"/Users/me/Pictures","to":"gdrive:Photos",
                   "direction":"push","parallel":4,"dryRun":false}),
            json!({"name":"docs","from":"/Users/me/Documents","to":"nas:Backups",
                   "direction":"push","parallel":2,"dryRun":false}),
        ];
        self.history = fixture_history();
        self.logs = fixture_logs();
        self.runtime = failed_archive_runtime();
        self.health = HealthStatus {
            present: true,
            stale: false,
            pid: 4312,
            service_name: "gn-drive background service for scheduled family archives".into(),
            mode: "service".into(),
            started_at: Some("2026-10-22T11:00:00Z".into()),
            last_heartbeat: Some("2026-10-22T12:00:00Z".into()),
            uptime_secs: 3600,
            last_sync_at: Some("2026-10-22T11:58:00Z".into()),
            next_schedule_at: Some("2026-10-23T03:00:00Z".into()),
            last_error: LONG_ERR.into(),
            active_tasks: vec![format!("copy {LONG_DIR}")],
        };
    }

    pub fn hello(&self) -> Value {
        let config = format!(
            "{}/.config/gn-drive",
            std::env::var("HOME").unwrap_or_default()
        );
        json!({
            "protocol": 1,
            "appVersion": "0.4.0",
            "engine": "GNDriveCore (mock)",
            "pid": 4312,
            "setup": self.setup,
            "unlocked": !self.locked,
            "passwordRequired": self.setup && self.locked,
            "platform": {"os": "macos", "arch": "arm64"},
            "paths": {
                "configDir": config,
                "dbPath": format!("{config}/gn-drive.db"),
                "rcloneConfigPath": format!("{config}/rclone.conf"),
                "logDir": config,
                "workingDir": "/Applications/GNDrive.app/Contents/MacOS"
            },
            "rclone": {"path": "/Applications/GNDrive.app/Contents/MacOS/rclone",
                       "version": "rclone v1.75.0", "bundled": true},
            "capabilities": {"oauth": true, "providers": true, "profiles": true,
                             "boards": true, "service": true, "update": true,
                             "logs": true, "history": true, "doctor": true},
        })
    }

    fn auth(&self) -> AuthStatus {
        let retry = self
            .locked_until
            .map(|until| until.saturating_duration_since(Instant::now()).as_secs() as u32)
            .unwrap_or(0);
        AuthStatus {
            setup: self.setup,
            unlocked: !self.locked,
            locked_at: if self.locked {
                Some("2026-10-22T12:00:00Z".into())
            } else {
                None
            },
            enabled: self.setup,
            lockout: Lockout {
                failed_attempts: self.failed,
                locked_until: if retry > 0 {
                    "2026-10-22T12:01:00Z".into()
                } else {
                    String::new()
                },
                is_locked: retry > 0,
                retry_after_secs: retry,
            },
        }
    }

    fn paths(&self) -> Value {
        let config = format!(
            "{}/.config/gn-drive",
            std::env::var("HOME").unwrap_or_default()
        );
        json!({
            "configDir": config,
            "dbPath": format!("{config}/gn-drive.db"),
            "rcloneConfigPath": format!("{config}/rclone.conf"),
            "logDir": config,
        })
    }

    fn snapshot(&self) -> Snapshot {
        if self.locked {
            Snapshot {
                locked: true,
                auth: self.auth(),
                app_settings: self.settings.clone(),
                version: "0.4.0".into(),
                paths: serde_json::from_value(self.paths()).unwrap_or_default(),
                ..Default::default()
            }
        } else {
            Snapshot {
                locked: false,
                auth: self.auth(),
                flows: self.flows.clone(),
                remotes: self.remotes.clone(),
                profiles: self.profiles.clone(),
                history: self.history.iter().take(100).cloned().collect(),
                runtime: self.runtime.clone(),
                app_settings: self.settings.clone(),
                rclone_version: "rclone v1.75.0".into(),
                version: "0.4.0".into(),
                paths: serde_json::from_value(self.paths()).unwrap_or_default(),
            }
        }
    }

    fn require_unlocked(&self) -> Result<(), RpcError> {
        if self.locked {
            Err(RpcError::with_data(
                "locked",
                "auth: app is locked",
                json!({"setup": self.setup}),
            ))
        } else {
            Ok(())
        }
    }

    fn find_flow(&self, id: &str) -> Option<usize> {
        self.flows.iter().position(|flow| flow.id == id)
    }

    fn running_flow(&self, id: &str) -> bool {
        self.run.as_ref().is_some_and(|run| run.flow_id == id)
            || self.runtime.flows.iter().any(|flow| {
                flow.id == id && matches!(flow.status.as_str(), "running" | "cancelling")
            })
    }

    fn new_id(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}{}", self.next)
    }

    fn start_run(&mut self, shared: &Inbox, flow_index: usize) {
        let Some(flow) = self.flows.get(flow_index).cloned() else {
            return;
        };
        let Some(first) = flow.operations.first().cloned() else {
            return;
        };
        let flow_id = flow.id.clone();
        let files = run_files(flow_index);
        let now = Instant::now();
        let task_id = self.new_id("T");
        self.run = Some(Run {
            flow_id: flow_id.clone(),
            op_index: 0,
            op_id: first.id.clone(),
            task_id: task_id.clone(),
            action: first.action.clone(),
            files,
            file_index: 0,
            fraction: 0.0,
            files_done: 0,
            bytes_done: 0,
            started: now,
            last_emit: now - TICK,
        });
        self.runtime.flows.retain(|f| f.id != flow_id);
        self.runtime.flows.push(RuntimeFlow {
            id: flow_id.clone(),
            status: "running".into(),
            last_error: String::new(),
            ops: vec![RuntimeOp {
                id: first.id.clone(),
                status: "running".into(),
                last_error: String::new(),
            }],
            sync: None,
            log: vec![
                RuntimeLog {
                    at: now_ms(),
                    status: "running".into(),
                    op_id: String::new(),
                    error: String::new(),
                    label: "Flow".into(),
                },
                RuntimeLog {
                    at: now_ms(),
                    status: "running".into(),
                    op_id: first.id.clone(),
                    error: String::new(),
                    label: format!("Op {}", first.id),
                },
            ],
        });
        self.runtime.revision += 1;
        shared.emit(
            "flow:execution",
            json!({"flowID": flow_id, "opID": "", "status": "running", "error": ""}),
        );
        shared.emit(
            "flow:execution",
            json!({"flowID": flow_id, "opID": first.id, "status": "running", "error": ""}),
        );
        shared.emit(
            "sync:started",
            json!({"taskID": task_id, "profileID": format!("{flow_id}:{}", first.id),
                   "action": first.action}),
        );
        self.advance(shared);
    }

    fn cancel_run(&mut self, shared: &Inbox) {
        let Some(run) = self.run.take() else { return };
        let flow_id = run.flow_id.clone();
        let profile = format!("{flow_id}:{}", run.op_id);
        shared.emit(
            "sync:failed",
            json!({"taskID": run.task_id, "profileID": profile, "action": run.action,
                   "state": "cancelled", "errors": 0, "currentFile": "",
                   "stage": "cancelled", "stageDetail": "", "transferred": 0,
                   "total": 0, "bytesPerSec": 0, "eta": 0, "filesTransferred": 0,
                   "totalFiles": 0, "checks": 0, "totalChecks": 0, "deletes": 0,
                   "renames": 0, "transfers": [], "errorMessage": ""}),
        );
        if let Some(flow) = self.runtime.flows.iter_mut().find(|f| f.id == flow_id) {
            flow.status = "cancelled".into();
            if let Some(op) = flow.ops.last_mut() {
                op.status = "cancelled".into();
            }
            flow.sync = None;
            flow.log.push(RuntimeLog {
                at: now_ms(),
                status: "cancelled".into(),
                op_id: String::new(),
                error: String::new(),
                label: "Flow".into(),
            });
        }
        self.runtime.revision += 1;
        shared.emit(
            "flow:execution",
            json!({"flowID": flow_id, "opID": run.op_id, "status": "cancelled", "error": ""}),
        );
        shared.emit(
            "flow:execution",
            json!({"flowID": flow_id, "opID": "", "status": "cancelled", "error": ""}),
        );
        self.history
            .insert(0, cancelled_history(&flow_id, &run.task_id));
        let snapshot = self.runtime.clone();
        shared.emit("runtime:snapshot", serde_json::to_value(snapshot).unwrap());
    }

    pub fn advance(&mut self, shared: &Inbox) {
        let now = Instant::now();
        let outcome = {
            let Some(run) = self.run.as_mut() else { return };
            if now.duration_since(run.last_emit) < TICK {
                return;
            }
            run.last_emit = now;
            let (_, size) = run.files[run.file_index];
            run.fraction += 0.24;
            if run.fraction >= 1.0 {
                run.bytes_done += size;
                run.files_done += 1;
                run.file_index += 1;
                run.fraction = 0.0;
            }
            if run.file_index >= run.files.len() {
                Some((
                    run.flow_id.clone(),
                    run.op_id.clone(),
                    run.task_id.clone(),
                    run.action.clone(),
                    run.started.elapsed().as_secs_f64(),
                    run.files.iter().map(|(_, size)| *size).sum::<u64>(),
                ))
            } else {
                None
            }
        };

        if let Some((flow_id, op_id, task_id, action, duration, bytes)) = outcome {
            self.complete_operation(shared, &flow_id, &op_id, &task_id, &action, duration, bytes);
            return;
        }

        let Some(run) = self.run.as_ref() else { return };
        let progress = progress_for(run);
        let (flow_id, op_id) = (run.flow_id.clone(), run.op_id.clone());
        if let Some(flow) = self.runtime.flows.iter_mut().find(|f| f.id == flow_id) {
            flow.sync = Some(progress.clone());
            if let Some(op) = flow.ops.iter_mut().find(|op| op.id == op_id) {
                op.status = "running".into();
            }
        }
        self.runtime.revision += 1;
        shared.emit(
            "sync:progress",
            serde_json::to_value(&progress).unwrap_or(Value::Null),
        );
        let snapshot = self.runtime.clone();
        shared.emit("runtime:snapshot", serde_json::to_value(snapshot).unwrap());
    }

    #[allow(clippy::too_many_arguments)]
    fn complete_operation(
        &mut self,
        shared: &Inbox,
        flow_id: &str,
        op_id: &str,
        task_id: &str,
        action: &str,
        duration: f64,
        bytes: u64,
    ) {
        shared.emit(
            "sync:completed",
            json!({"taskID": task_id, "profileID": format!("{flow_id}:{op_id}"),
                   "action": action, "duration": duration, "bytes": bytes, "errors": 0}),
        );
        shared.emit(
            "flow:execution",
            json!({"flowID": flow_id, "opID": op_id, "status": "completed", "error": ""}),
        );
        self.history.insert(
            0,
            HistoryEntry {
                id: task_id.into(),
                profile_name: format!("{flow_id}:{op_id}"),
                action: action.into(),
                state: "completed".into(),
                started_at: "2026-10-22T12:00:00Z".into(),
                finished_at: "2026-10-22T12:01:00Z".into(),
                duration,
                bytes,
                errors: 0,
                files: 5,
                error_message: String::new(),
            },
        );
        let next_index = self
            .run
            .as_ref()
            .map(|run| run.op_index + 1)
            .unwrap_or(usize::MAX);
        if let Some(flow) = self.runtime.flows.iter_mut().find(|f| f.id == flow_id) {
            if let Some(op) = flow.ops.iter_mut().find(|op| op.id == op_id) {
                op.status = "completed".into();
            }
            flow.log.push(RuntimeLog {
                at: now_ms(),
                status: "completed".into(),
                op_id: op_id.to_string(),
                error: String::new(),
                label: format!("Op {op_id}"),
            });
            flow.sync = None;
        }
        let next_op = self
            .flows
            .iter()
            .find(|f| f.id == flow_id)
            .and_then(|f| f.operations.get(next_index).cloned());
        if let Some(op) = next_op {
            let files = run_files(next_index);
            let now = Instant::now();
            let task = self.new_id("T");
            self.run = Some(Run {
                flow_id: flow_id.to_string(),
                op_index: next_index,
                op_id: op.id.clone(),
                task_id: task.clone(),
                action: op.action.clone(),
                files,
                file_index: 0,
                fraction: 0.0,
                files_done: 0,
                bytes_done: 0,
                started: now,
                last_emit: now - TICK,
            });
            if let Some(flow) = self.runtime.flows.iter_mut().find(|f| f.id == flow_id) {
                flow.ops.push(RuntimeOp {
                    id: op.id.clone(),
                    status: "running".into(),
                    last_error: String::new(),
                });
                flow.log.push(RuntimeLog {
                    at: now_ms(),
                    status: "running".into(),
                    op_id: op.id.clone(),
                    error: String::new(),
                    label: format!("Op {}", op.id),
                });
            }
            self.runtime.revision += 1;
            shared.emit(
                "flow:execution",
                json!({"flowID": flow_id, "opID": op.id, "status": "running", "error": ""}),
            );
            shared.emit(
                "sync:started",
                json!({"taskID": task, "profileID": format!("{flow_id}:{}", op.id),
                       "action": op.action}),
            );
            self.advance(shared);
        } else {
            self.run = None;
            if let Some(flow) = self.runtime.flows.iter_mut().find(|f| f.id == flow_id) {
                flow.status = "completed".into();
                flow.log.push(RuntimeLog {
                    at: now_ms(),
                    status: "completed".into(),
                    op_id: String::new(),
                    error: String::new(),
                    label: "Flow".into(),
                });
                while flow.log.len() > 40 {
                    flow.log.remove(0);
                }
            }
            self.runtime.revision += 1;
            shared.emit(
                "flow:execution",
                json!({"flowID": flow_id, "opID": "", "status": "completed", "error": ""}),
            );
            let snapshot = self.runtime.clone();
            shared.emit("runtime:snapshot", serde_json::to_value(snapshot).unwrap());
        }
    }

    // MARK: - Dispatch

    pub fn handle(
        &mut self,
        _id: u64,
        method: &str,
        params: Value,
        shared: &Inbox,
    ) -> Result<Value, RpcError> {
        let params = if params.is_null() { json!({}) } else { params };
        match method {
            "ping" => Ok(json!({"pong": true, "protocol": 1, "version": "0.4.0",
                                "now": "2026-10-22T12:00:00Z"})),
            "state.snapshot" => Ok(serde_json::to_value(self.snapshot()).unwrap_or(Value::Null)),
            "app.quit" => {
                shared.emit("app:shutdown", json!({"reason": "request"}));
                Ok(json!({"quitting": true}))
            }
            "doctor" => Ok(serde_json::to_value(self.doctor()).unwrap_or(Value::Null)),
            "logs.read" => {
                let limit = param_u64(&params, "limit").unwrap_or(200).min(2000) as usize;
                let start = self.logs.len().saturating_sub(limit);
                let lines = self.logs[start..].to_vec();
                Ok(json!({"lines": lines, "dropped": 0}))
            }
            "auth.status" => Ok(serde_json::to_value(self.auth()).unwrap_or(Value::Null)),
            "auth.setup" => {
                if self.setup {
                    return Err(RpcError::new(
                        "already_setup",
                        "auth: password already configured",
                    ));
                }
                let password = param_str(&params, "password");
                if password.chars().count() < 4 {
                    return Err(RpcError::new(
                        "password_too_short",
                        "auth: password must be at least 4 characters",
                    ));
                }
                self.password = password;
                self.setup = true;
                self.locked = false;
                self.runtime = Runtime::default();
                shared.emit("auth:unlocked", json!({}));
                Ok(serde_json::to_value(self.auth()).unwrap_or(Value::Null))
            }
            "auth.unlock" => {
                if !self.setup {
                    return Err(RpcError::new("not_setup", "auth: no password configured"));
                }
                if let Some(until) = self.locked_until
                    && until > Instant::now()
                {
                    let retry = until.saturating_duration_since(Instant::now()).as_secs();
                    return Err(RpcError::with_data(
                        "locked_out",
                        format!("auth: too many attempts, retry in {retry}s"),
                        json!({"retryAfterSecs": retry,
                               "lockedUntil": "2026-10-22T12:01:00Z"}),
                    ));
                }
                let password = param_str(&params, "password");
                if password != self.password {
                    self.failed += 1;
                    if self.failed >= 3 {
                        self.locked_until = Some(Instant::now() + Duration::from_secs(30));
                        self.failed = 0;
                        return Err(RpcError::with_data(
                            "locked_out",
                            "auth: too many attempts, retry in 30s",
                            json!({"retryAfterSecs": 30,
                                   "lockedUntil": "2026-10-22T12:01:00Z"}),
                        ));
                    }
                    return Err(RpcError::with_data(
                        "invalid_password",
                        "auth: invalid password",
                        json!({"failedAttempts": self.failed}),
                    ));
                }
                self.failed = 0;
                self.locked_until = None;
                let was_locked = self.locked;
                self.locked = false;
                // A lock clears the live run. The next unlock puts the seeded
                // failed archive back so the long error is on screen without
                // starting a sync. An unlock while already open leaves a run alone.
                if was_locked && self.run.is_none() {
                    self.runtime = failed_archive_runtime();
                }
                shared.emit("auth:unlocked", json!({}));
                Ok(serde_json::to_value(self.auth()).unwrap_or(Value::Null))
            }
            // Devtools only. The window's `lockout` command uses this so a
            // later unlock attempt fails the same way three wrong passwords do.
            "dev.lockout" => {
                let secs = param_u64(&params, "seconds").unwrap_or(30).min(3600);
                if secs == 0 {
                    self.locked_until = None;
                    self.failed = 0;
                } else {
                    self.locked_until = Some(Instant::now() + Duration::from_secs(secs));
                    self.failed = 0;
                }
                Ok(serde_json::to_value(self.auth()).unwrap_or(Value::Null))
            }
            "auth.lock" => {
                self.locked = self.setup;
                self.runtime = Runtime::default();
                self.run = None;
                shared.emit("auth:locked", json!({}));
                shared.emit("runtime:snapshot", json!({"revision": 0, "flows": []}));
                Ok(serde_json::to_value(self.auth()).unwrap_or(Value::Null))
            }
            "auth.changePassword" => {
                self.require_unlocked()?;
                let old = param_str(&params, "oldPassword");
                let new = param_str(&params, "newPassword");
                if old != self.password {
                    return Err(RpcError::new("invalid_password", "auth: invalid password"));
                }
                if new.chars().count() < 4 {
                    return Err(RpcError::new(
                        "password_too_short",
                        "auth: password must be at least 4 characters",
                    ));
                }
                self.password = new;
                Ok(serde_json::to_value(self.auth()).unwrap_or(Value::Null))
            }
            "auth.removePassword" => {
                self.require_unlocked()?;
                let password = param_str(&params, "password");
                if password != self.password {
                    return Err(RpcError::new("invalid_password", "auth: invalid password"));
                }
                self.setup = false;
                self.locked = false;
                self.password.clear();
                shared.emit("auth:unlocked", json!({}));
                Ok(serde_json::to_value(self.auth()).unwrap_or(Value::Null))
            }
            "flows.list" => {
                self.require_unlocked()?;
                Ok(json!({"flows": self.flows}))
            }
            "flows.get" => {
                self.require_unlocked()?;
                let id = param_str(&params, "id");
                let flow = self
                    .flows
                    .iter()
                    .find(|flow| flow.id == id)
                    .cloned()
                    .ok_or_else(|| {
                        RpcError::new("flow_not_found", format!("flow {id} not found"))
                    })?;
                Ok(json!({"flow": flow}))
            }
            "flows.create" => {
                self.require_unlocked()?;
                let mut id = param_str(&params, "id");
                if id.is_empty() {
                    id = self.new_id("F");
                }
                if self.find_flow(&id).is_some() {
                    return Err(RpcError::new(
                        "flow_exists",
                        format!("flow {id} already exists"),
                    ));
                }
                let name = {
                    let name = param_str(&params, "name");
                    if name.trim().is_empty() {
                        "Untitled flow".to_string()
                    } else {
                        name
                    }
                };
                let canvas = param_str(&params, "canvasJSON");
                let flow = Flow {
                    id: id.clone(),
                    name,
                    canvas_json: if canvas.is_empty() {
                        "{}".into()
                    } else {
                        canvas
                    },
                    created_at: "2026-10-22 12:00:00".into(),
                    updated_at: "2026-10-22 12:00:00".into(),
                    sort_order: self.flows.len() as i64,
                    ..Default::default()
                };
                self.flows.push(flow.clone());
                shared.emit("state:changed", json!({"domain": "flows", "id": id}));
                Ok(json!({"flow": flow}))
            }
            "flows.save" => {
                self.require_unlocked()?;
                let mut flow: Flow =
                    serde_json::from_value(params.get("flow").cloned().unwrap_or(Value::Null))
                        .map_err(|error| RpcError::new("invalid_params", error.to_string()))?;
                if flow.id.is_empty() {
                    return Err(RpcError::new("invalid_params", "flow id is required"));
                }
                if self.running_flow(&flow.id) {
                    return Err(RpcError::new("flow_busy", "flow is running"));
                }
                self.normalize_flow(&mut flow);
                if let Some(index) = self.find_flow(&flow.id) {
                    self.flows[index] = flow.clone();
                } else {
                    self.flows.push(flow.clone());
                }
                shared.emit("state:changed", json!({"domain": "flows", "id": flow.id}));
                Ok(json!({"flow": flow, "warnings": []}))
            }
            "flows.delete" => {
                self.require_unlocked()?;
                let id = param_str(&params, "id");
                if self.running_flow(&id) {
                    return Err(RpcError::new("flow_busy", "flow is running"));
                }
                let Some(index) = self.find_flow(&id) else {
                    return Err(RpcError::new(
                        "flow_not_found",
                        format!("flow {id} not found"),
                    ));
                };
                self.flows.remove(index);
                self.runtime.flows.retain(|flow| flow.id != id);
                shared.emit("state:changed", json!({"domain": "flows", "id": id}));
                Ok(json!({"deleted": true, "id": id}))
            }
            "flows.execute" => {
                self.require_unlocked()?;
                let id = param_str(&params, "id");
                let index = self.find_flow(&id).ok_or_else(|| {
                    RpcError::new("flow_not_found", format!("flow {id} not found"))
                })?;
                if self.running_flow(&id) {
                    return Err(RpcError::new("already_running", "flow is already running"));
                }
                if self.flows[index].operations.is_empty() {
                    return Err(RpcError::new("flow_empty", "flow has no operations"));
                }
                self.start_run(shared, index);
                Ok(json!({"flowID": id, "status": "running"}))
            }
            "flows.stop" => {
                self.require_unlocked()?;
                let id = param_str(&params, "id");
                if !self.running_flow(&id) {
                    return Err(RpcError::new("not_running", "flow is not running"));
                }
                self.cancel_run(shared);
                Ok(json!({"flowID": id, "status": "cancelling"}))
            }
            "operations.create" => {
                self.require_unlocked()?;
                let flow_id = param_str(&params, "flowID");
                let index = self.find_flow(&flow_id).ok_or_else(|| {
                    RpcError::new("flow_not_found", format!("flow {flow_id} not found"))
                })?;
                if self.running_flow(&flow_id) {
                    return Err(RpcError::new("flow_busy", "flow is running"));
                }
                let mut op: Operation =
                    serde_json::from_value(params.get("operation").cloned().unwrap_or(Value::Null))
                        .map_err(|error| RpcError::new("invalid_params", error.to_string()))?;
                if op.id.is_empty() {
                    op.id = self.new_id("O");
                }
                op.flow_id = flow_id.clone();
                op.sort_order = self.flows[index].operations.len() as i64;
                op.action = normalize_action(&op.action);
                op.sync_config.insert("action".into(), json!(op.action));
                self.flows[index].operations.push(op.clone());
                self.touch(index);
                let flow = self.flows[index].clone();
                shared.emit("state:changed", json!({"domain": "flows", "id": flow_id}));
                Ok(json!({"flow": flow, "operation": op}))
            }
            "operations.update" => {
                self.require_unlocked()?;
                let flow_id = param_str(&params, "flowID");
                let op_id = param_str(&params, "operationID");
                let index = self.find_flow(&flow_id).ok_or_else(|| {
                    RpcError::new("flow_not_found", format!("flow {flow_id} not found"))
                })?;
                if self.running_flow(&flow_id) {
                    return Err(RpcError::new("flow_busy", "flow is running"));
                }
                let patch = params.get("patch").cloned().unwrap_or(json!({}));
                let Some(op) = self.flows[index]
                    .operations
                    .iter_mut()
                    .find(|op| op.id == op_id)
                else {
                    return Err(RpcError::new(
                        "operation_not_found",
                        format!("operation {op_id} not found"),
                    ));
                };
                merge_operation(op, &patch);
                op.action = normalize_action(&op.action);
                op.sync_config.insert("action".into(), json!(op.action));
                let op = op.clone();
                self.touch(index);
                let flow = self.flows[index].clone();
                shared.emit("state:changed", json!({"domain": "flows", "id": flow_id}));
                Ok(json!({"flow": flow, "operation": op}))
            }
            "operations.delete" => {
                self.require_unlocked()?;
                let flow_id = param_str(&params, "flowID");
                let op_id = param_str(&params, "operationID");
                let index = self.find_flow(&flow_id).ok_or_else(|| {
                    RpcError::new("flow_not_found", format!("flow {flow_id} not found"))
                })?;
                if self.running_flow(&flow_id) {
                    return Err(RpcError::new("flow_busy", "flow is running"));
                }
                let before = self.flows[index].operations.len();
                self.flows[index].operations.retain(|op| op.id != op_id);
                if self.flows[index].operations.len() == before {
                    return Err(RpcError::new(
                        "operation_not_found",
                        format!("operation {op_id} not found"),
                    ));
                }
                for (i, op) in self.flows[index].operations.iter_mut().enumerate() {
                    op.sort_order = i as i64;
                }
                self.touch(index);
                let flow = self.flows[index].clone();
                shared.emit("state:changed", json!({"domain": "flows", "id": flow_id}));
                Ok(json!({"flow": flow}))
            }
            "operations.reorder" => {
                self.require_unlocked()?;
                let flow_id = param_str(&params, "flowID");
                let index = self.find_flow(&flow_id).ok_or_else(|| {
                    RpcError::new("flow_not_found", format!("flow {flow_id} not found"))
                })?;
                if self.running_flow(&flow_id) {
                    return Err(RpcError::new("flow_busy", "flow is running"));
                }
                let ids: Vec<String> = params
                    .get("operationIDs")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let existing: Vec<String> = self.flows[index]
                    .operations
                    .iter()
                    .map(|op| op.id.clone())
                    .collect();
                if ids.len() != existing.len() || !ids.iter().all(|id| existing.contains(id)) {
                    return Err(RpcError::new(
                        "invalid_params",
                        "operationIDs must be a permutation of the flow's operations",
                    ));
                }
                let mut ordered = Vec::with_capacity(ids.len());
                for (i, id) in ids.iter().enumerate() {
                    if let Some(op) = self.flows[index].operations.iter().find(|op| &op.id == id) {
                        let mut op = op.clone();
                        op.sort_order = i as i64;
                        ordered.push(op);
                    }
                }
                self.flows[index].operations = ordered;
                self.touch(index);
                let flow = self.flows[index].clone();
                shared.emit("state:changed", json!({"domain": "flows", "id": flow_id}));
                Ok(json!({"flow": flow}))
            }
            "runtime.snapshot" => {
                self.require_unlocked()?;
                Ok(serde_json::to_value(&self.runtime).unwrap_or(Value::Null))
            }
            "runtime.tasks" => {
                self.require_unlocked()?;
                let mut tasks = Vec::new();
                if let Some(run) = &self.run {
                    let progress = progress_for(run);
                    tasks.push(json!({
                        "id": run.task_id,
                        "name": progress.profile_id,
                        "action": run.action,
                        "status": "running",
                        "stats": {
                            "bytes": progress.transferred,
                            "bytesTotal": progress.total,
                            "files": progress.files_transferred,
                            "filesTotal": progress.total_files,
                            "speed": progress.bytes_per_sec,
                            "stage": progress.stage,
                            "fileTransfers": progress.transfers,
                        },
                        "startedAt": "2026-10-22T12:00:00Z",
                        "endedAt": null,
                    }));
                }
                Ok(json!({"tasks": tasks}))
            }
            "remotes.providers" => Ok(json!({
                "rcloneVersion": "rclone v1.75.0",
                "providers": providers(),
                "cached": true,
            })),
            "remotes.list" => {
                self.require_unlocked()?;
                Ok(json!({"remotes": self.remotes}))
            }
            "remotes.add" => {
                self.require_unlocked()?;
                let name = param_str(&params, "name");
                let kind = param_str(&params, "type");
                if name.trim().is_empty() || kind.trim().is_empty() {
                    return Err(RpcError::with_data(
                        "invalid_params",
                        "name and type are required",
                        json!({"field": "name"}),
                    ));
                }
                if self.remotes.iter().any(|remote| remote.name == name) {
                    return Err(RpcError::new(
                        "remote_exists",
                        format!("remote {name} already exists"),
                    ));
                }
                let remote = Remote {
                    name: name.clone(),
                    kind,
                    description: String::new(),
                };
                self.remotes.push(remote.clone());
                shared.emit("state:changed", json!({"domain": "remotes", "id": name}));
                Ok(json!({"remote": remote, "verified": true}))
            }
            "remotes.test" => {
                self.require_unlocked()?;
                let name = param_str(&params, "name");
                if !self.remotes.iter().any(|remote| remote.name == name) {
                    return Err(RpcError::new(
                        "remote_not_found",
                        format!("remote {name} not found"),
                    ));
                }
                Ok(json!({"name": name, "ok": true}))
            }
            "remotes.delete" => {
                self.require_unlocked()?;
                let name = param_str(&params, "name");
                let before = self.remotes.len();
                self.remotes.retain(|remote| remote.name != name);
                if self.remotes.len() == before {
                    return Err(RpcError::new(
                        "remote_not_found",
                        format!("remote {name} not found"),
                    ));
                }
                shared.emit("state:changed", json!({"domain": "remotes", "id": name}));
                Ok(json!({"deleted": true, "name": name}))
            }
            "remotes.oauth.start" => {
                self.require_unlocked()?;
                if self.oauth.is_some() {
                    return Err(RpcError::new(
                        "oauth_session_active",
                        "another OAuth session is already running",
                    ));
                }
                let kind = param_str(&params, "type");
                if kind.is_empty() {
                    return Err(RpcError::new("invalid_params", "type is required"));
                }
                let session = self.new_id("S");
                self.oauth = Some(session.clone());
                let auth_url = "http://127.0.0.1:53682/auth?state=mock-state";
                shared.emit(
                    "remotes:oauth",
                    json!({"sessionID": session, "status": "waiting", "authURL": auth_url}),
                );
                Ok(
                    json!({"sessionID": session, "type": kind, "authURL": auth_url,
                          "expiresAt": "2026-10-22T12:10:00Z"}),
                )
            }
            "remotes.oauth.complete" => {
                self.require_unlocked()?;
                let session = param_str(&params, "sessionID");
                if self.oauth.as_deref() != Some(session.as_str()) {
                    return Err(RpcError::new(
                        "oauth_session_not_found",
                        format!("OAuth session {session} not found"),
                    ));
                }
                let name = param_str(&params, "name");
                let kind = param_str(&params, "type");
                if name.is_empty() || kind.is_empty() {
                    return Err(RpcError::new(
                        "invalid_params",
                        "name and type are required",
                    ));
                }
                if self.remotes.iter().any(|remote| remote.name == name) {
                    return Err(RpcError::new(
                        "remote_exists",
                        format!("remote {name} already exists"),
                    ));
                }
                self.oauth = None;
                let remote = Remote {
                    name: name.clone(),
                    kind,
                    description: String::new(),
                };
                self.remotes.push(remote.clone());
                shared.emit(
                    "remotes:oauth",
                    json!({"sessionID": session, "status": "authorized"}),
                );
                shared.emit("state:changed", json!({"domain": "remotes", "id": name}));
                Ok(json!({"remote": remote, "verified": true}))
            }
            "remotes.oauth.cancel" => {
                self.require_unlocked()?;
                let session = param_str(&params, "sessionID");
                if self.oauth.as_deref() != Some(session.as_str()) {
                    return Err(RpcError::new(
                        "oauth_session_not_found",
                        format!("OAuth session {session} not found"),
                    ));
                }
                self.oauth = None;
                shared.emit(
                    "remotes:oauth",
                    json!({"sessionID": session, "status": "cancelled"}),
                );
                Ok(json!({"cancelled": true, "sessionID": session}))
            }
            "fs.list" => {
                self.require_unlocked()?;
                let path = param_str(&params, "path");
                if !path.contains(':') && !path.starts_with('/') {
                    return Err(RpcError::new(
                        "fs_missing_path",
                        "path must be a remote (remote:path) or absolute local path",
                    ));
                }
                let entries = fixture_entries(&path);
                Ok(json!({"path": path, "entries": entries, "truncated": false}))
            }
            "profiles.list" => {
                self.require_unlocked()?;
                Ok(json!({"profiles": self.profiles}))
            }
            "profiles.get" => {
                self.require_unlocked()?;
                let name = param_str(&params, "name");
                self.profiles
                    .iter()
                    .find(|profile| profile.get("name").and_then(Value::as_str) == Some(&name))
                    .map(|profile| json!({"profile": profile}))
                    .ok_or_else(|| {
                        RpcError::new("profile_not_found", format!("profile {name} not found"))
                    })
            }
            "profiles.save" => {
                self.require_unlocked()?;
                let profile = params.get("profile").cloned().unwrap_or(Value::Null);
                let name = profile
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if name.is_empty() {
                    return Err(RpcError::new("missing_name", "profile: name is required"));
                }
                if let Some(existing) = self
                    .profiles
                    .iter_mut()
                    .find(|item| item.get("name").and_then(Value::as_str) == Some(&name))
                {
                    *existing = profile.clone();
                } else {
                    self.profiles.push(profile.clone());
                }
                Ok(json!({"profile": profile}))
            }
            "profiles.delete" => {
                self.require_unlocked()?;
                let name = param_str(&params, "name");
                let before = self.profiles.len();
                self.profiles
                    .retain(|profile| profile.get("name").and_then(Value::as_str) != Some(&name));
                if self.profiles.len() == before {
                    return Err(RpcError::new(
                        "profile_not_found",
                        format!("profile {name} not found"),
                    ));
                }
                Ok(json!({"deleted": true, "name": name}))
            }
            "schedules.list" => {
                self.require_unlocked()?;
                Ok(json!({"schedules": []}))
            }
            "sync.profile" => {
                self.require_unlocked()?;
                let name = param_str(&params, "profile");
                if !self
                    .profiles
                    .iter()
                    .any(|profile| profile.get("name").and_then(Value::as_str) == Some(&name))
                {
                    return Err(RpcError::new(
                        "profile_not_found",
                        format!("profile {name} not found"),
                    ));
                }
                let task = self.new_id("T");
                shared.emit(
                    "sync:started",
                    json!({"taskID": task, "profileID": name, "action": "push"}),
                );
                Ok(json!({"taskID": task, "profile": name, "action": "push",
                          "state": "running"}))
            }
            "sync.stop" => {
                self.require_unlocked()?;
                let task = param_str(&params, "taskID");
                Ok(json!({"cancelled": true, "taskID": task}))
            }
            "history.list" => {
                self.require_unlocked()?;
                let limit = param_u64(&params, "limit").unwrap_or(20).min(1000) as usize;
                let offset = param_u64(&params, "offset").unwrap_or(0) as usize;
                let profile = param_str(&params, "profile");
                let entries: Vec<&HistoryEntry> = self
                    .history
                    .iter()
                    .filter(|entry| profile.is_empty() || entry.profile_name == profile)
                    .skip(offset)
                    .take(limit)
                    .collect();
                Ok(json!({"entries": entries, "total": null}))
            }
            "history.stats" => {
                self.require_unlocked()?;
                let total = self.history.len() as u64;
                let bytes: u64 = self.history.iter().map(|entry| entry.bytes).sum();
                let duration: f64 = self.history.iter().map(|entry| entry.duration).sum();
                let errors: u64 = self.history.iter().map(|entry| entry.errors).sum();
                Ok(json!({"totalSyncs": total, "totalBytes": bytes,
                          "totalDuration": duration, "totalErrors": errors,
                          "byProfile": {}}))
            }
            "history.clear" => {
                self.require_unlocked()?;
                self.history.clear();
                Ok(json!({"cleared": true}))
            }
            "settings.get" => Ok(json!({"settings": self.settings})),
            "settings.set" => {
                if let Some(patch) = params.get("settings") {
                    if let Some(value) = patch.get("notificationsEnabled").and_then(Value::as_bool)
                    {
                        self.settings.notifications_enabled = value;
                    }
                    if let Some(value) = patch.get("debugMode").and_then(Value::as_bool) {
                        self.settings.debug_mode = value;
                    }
                    if let Some(value) = patch.get("minimizeToTray").and_then(Value::as_bool) {
                        self.settings.minimize_to_tray = value;
                    }
                    if let Some(value) = patch.get("startAtLogin").and_then(Value::as_bool) {
                        self.settings.start_at_login = value;
                    }
                }
                Ok(json!({"settings": self.settings}))
            }
            "service.status" => Ok(serde_json::to_value(&self.service).unwrap_or(Value::Null)),
            "service.install" => {
                self.service.installed = true;
                self.service.scope = "user".into();
                Ok(serde_json::to_value(&self.service).unwrap_or(Value::Null))
            }
            "service.uninstall" => {
                self.service.installed = false;
                self.service.running = false;
                Ok(json!({"installed": false, "running": false, "scope": "user"}))
            }
            "service.start" | "service.restart" => {
                if !self.service.installed {
                    return Err(RpcError::new(
                        "service_not_installed",
                        "the launchd service is not installed",
                    ));
                }
                self.service.running = true;
                self.service.pid = 5123;
                Ok(serde_json::to_value(&self.service).unwrap_or(Value::Null))
            }
            "service.stop" => {
                if !self.service.installed {
                    return Err(RpcError::new(
                        "service_not_installed",
                        "the launchd service is not installed",
                    ));
                }
                self.service.running = false;
                self.service.pid = 0;
                Ok(serde_json::to_value(&self.service).unwrap_or(Value::Null))
            }
            "service.health" => Ok(serde_json::to_value(&self.health).unwrap_or(Value::Null)),
            "update.check" => {
                let check = UpdateCheck {
                    current: "0.4.0".into(),
                    latest: "0.5.0".into(),
                    update_available: true,
                    checked_at: "2026-10-22T12:00:00Z".into(),
                };
                Ok(serde_json::to_value(check).unwrap_or(Value::Null))
            }
            "update.apply" => Ok(json!({"updated": true, "oldVersion": "0.4.0",
                          "newVersion": "0.5.0",
                          "binaryPath": "/Applications/GNDrive.app/Contents/MacOS/gn-drive",
                          "restartHint": "Mock engine: restart GN Drive to apply."})),
            _ => Err(RpcError::new(
                "unknown_method",
                format!("unknown method {method}"),
            )),
        }
    }

    fn normalize_flow(&mut self, flow: &mut Flow) {
        flow.updated_at = "2026-10-22 12:00:00".into();
        if flow.created_at.is_empty() {
            flow.created_at = flow.updated_at.clone();
        }
        for (index, op) in flow.operations.iter_mut().enumerate() {
            if op.id.is_empty() {
                op.id = self.new_id("O");
            }
            op.flow_id = flow.id.clone();
            if op.sort_order == 0 {
                op.sort_order = index as i64;
            }
            op.action = normalize_action(&op.action);
            op.sync_config.insert("action".into(), json!(op.action));
            if op.source_path.is_empty() {
                op.source_path = "/".into();
            }
            if op.target_path.is_empty() {
                op.target_path = "/".into();
            }
        }
    }

    fn touch(&mut self, index: usize) {
        if let Some(flow) = self.flows.get_mut(index) {
            flow.updated_at = "2026-10-22 12:00:00".into();
        }
    }

    fn doctor(&self) -> DoctorReport {
        let config = format!(
            "{}/.config/gn-drive",
            std::env::var("HOME").unwrap_or_default()
        );
        DoctorReport {
            ok: true,
            checks: vec![
                DoctorCheck {
                    id: "rclone_binary".into(),
                    label: "rclone binary".into(),
                    status: "ok".into(),
                    detail: "/Applications/GNDrive.app/Contents/MacOS/rclone-bundled-for-the-family-archive-window".into(),
                },
                DoctorCheck {
                    id: "rclone_version".into(),
                    label: "rclone version".into(),
                    status: "ok".into(),
                    detail: "rclone v1.75.0".into(),
                },
                DoctorCheck {
                    id: "config_dir".into(),
                    label: "config directory".into(),
                    status: "ok".into(),
                    detail: config,
                },
                DoctorCheck {
                    id: "database".into(),
                    label: "database".into(),
                    status: "ok".into(),
                    detail: "gn-drive.db".into(),
                },
                DoctorCheck {
                    id: "auth".into(),
                    label: "auth".into(),
                    status: "info".into(),
                    detail: format!(
                        "configured: {}, unlocked: {}",
                        if self.setup { "yes" } else { "no" },
                        if self.locked { "no" } else { "yes" }
                    ),
                },
                DoctorCheck {
                    id: "remotes".into(),
                    label: "remotes".into(),
                    status: "info".into(),
                    detail: format!("{} configured", self.remotes.len()),
                },
                DoctorCheck {
                    id: "profiles".into(),
                    label: "profiles".into(),
                    status: "info".into(),
                    detail: format!("{} configured", self.profiles.len()),
                },
                DoctorCheck {
                    id: "platform".into(),
                    label: "platform".into(),
                    status: "ok".into(),
                    detail: "darwin arm64".into(),
                },
            ],
            data_files: vec![
                "auth.json".into(),
                "gn-drive.db".into(),
                "rclone.conf".into(),
                format!("{LONG_DIR}/rclone-family-archive.conf"),
            ],
        }
    }
}

fn normalize_action(action: &str) -> String {
    match action.trim() {
        "bi" => "bi".into(),
        "bi-resync" => "bi-resync".into(),
        _ => "push".into(),
    }
}

fn param_str(params: &Value, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn param_u64(params: &Value, key: &str) -> Option<u64> {
    params.get(key).and_then(Value::as_u64)
}

fn merge_operation(op: &mut Operation, patch: &Value) {
    if let Some(value) = patch.get("sourceRemote").and_then(Value::as_str) {
        op.source_remote = value.into();
    }
    if let Some(value) = patch.get("sourcePath").and_then(Value::as_str) {
        op.source_path = value.into();
    }
    if let Some(value) = patch.get("targetRemote").and_then(Value::as_str) {
        op.target_remote = value.into();
    }
    if let Some(value) = patch.get("targetPath").and_then(Value::as_str) {
        op.target_path = value.into();
    }
    if let Some(value) = patch.get("action").and_then(Value::as_str) {
        op.action = value.into();
    }
    if let Some(value) = patch.get("syncConfig")
        && let Some(map) = value.as_object()
    {
        op.sync_config = map.clone();
    }
}

fn progress_for(run: &Run) -> SyncProgress {
    let total: u64 = run.files.iter().map(|(_, size)| *size).sum();
    let (_, size) = run.files[run.file_index];
    let current = (size as f64 * run.fraction.min(1.0)) as u64;
    let transferred = run.bytes_done + current;
    let speed = 8.4 * 1024.0 * 1024.0;
    let transfers = run
        .files
        .iter()
        .enumerate()
        .map(|(index, (name, file_size))| {
            let status = if index < run.file_index {
                "completed"
            } else if index == run.file_index {
                "transferring"
            } else {
                "pending"
            };
            let bytes = if index < run.file_index {
                *file_size
            } else if index == run.file_index {
                (*file_size as f64 * run.fraction.min(1.0)) as u64
            } else {
                0
            };
            let progress = if *file_size == 0 {
                100.0
            } else {
                bytes as f64 / *file_size as f64 * 100.0
            };
            FileTransfer {
                name: name.clone(),
                size: *file_size,
                bytes,
                progress,
                status: status.into(),
                speed: if status == "transferring" { speed } else { 0.0 },
                error: String::new(),
            }
        })
        .collect();
    let remaining = total.saturating_sub(transferred) as f64;
    SyncProgress {
        task_id: run.task_id.clone(),
        profile_id: format!("{}:{}", run.flow_id, run.op_id),
        action: run.action.clone(),
        state: "running".into(),
        transferred,
        total,
        bytes_per_sec: speed,
        eta: (remaining / speed) as i64,
        errors: 0,
        current_file: run.files[run.file_index].0.clone(),
        stage: "transferring".into(),
        stage_detail: format!("{} of {}", run.file_index + 1, run.files.len()),
        files_transferred: run.files_done,
        total_files: run.files.len() as u64,
        checks: 0,
        total_checks: 0,
        deletes: 0,
        renames: 0,
        transfers,
        error_message: String::new(),
    }
}

fn run_files(index: usize) -> Vec<(String, u64)> {
    let sets: [&[(&str, u64)]; 3] = [
        &[
            ("IMG_0421.heic", 4_400_000),
            ("IMG_0422.heic", 5_300_000),
            ("Family.mov", 182_000_000),
            ("Cover.jpg", 2_400_000),
            ("Notes.txt", 4_200),
            ("Archive.zip", 44_000_000),
        ],
        &[
            ("invoice.pdf", 940_000),
            ("Plan.docx", 122_000),
            ("budget.xlsx", 58_000),
            ("scan-01.png", 12_400_000),
            ("scan-02.png", 11_900_000),
        ],
        &[
            ("mix-final.wav", 68_000_000),
            ("artwork.png", 3_100_000),
            ("tracklist.txt", 1_800),
            ("bonus.flac", 41_000_000),
        ],
    ];
    sets[index % sets.len()]
        .iter()
        .map(|(name, size)| ((*name).to_string(), *size))
        .collect()
}

fn fixtures() -> Vec<Flow> {
    let mut nightly = Flow {
        id: "F1".into(),
        name: "Nightly Photos".into(),
        schedule_enabled: true,
        schedule_cron: "0 3 * * *".into(),
        sort_order: 0,
        created_at: "2026-10-18 03:00:00".into(),
        updated_at: "2026-10-22 03:00:01".into(),
        ..Default::default()
    };
    nightly.operations = vec![
        operation(
            "O1",
            "F1",
            "",
            "/Users/me/Pictures",
            "gdrive",
            "/Photos",
            "push",
            0,
            true,
        ),
        operation(
            "O2",
            "F1",
            "",
            "/Users/me/Documents",
            "nas",
            "/Backups",
            "push",
            1,
            false,
        ),
        operation(
            "O3",
            "F1",
            "gdrive",
            "/Photos",
            "",
            "/Users/me/Pictures/Cloud",
            "bi",
            2,
            false,
        ),
    ];
    let mut music = Flow {
        id: "F2".into(),
        name: "Music Mirror".into(),
        sort_order: 1,
        created_at: "2026-10-19 10:00:00".into(),
        updated_at: "2026-10-21 18:22:00".into(),
        ..Default::default()
    };
    music.operations = vec![
        operation(
            "O4",
            "F2",
            "",
            "/Users/me/Music",
            "dropbox",
            "/Music",
            "push",
            0,
            false,
        ),
        operation(
            "O5",
            "F2",
            "",
            "/Users/me/Playlists",
            "gdrive",
            "/Playlists",
            "bi-resync",
            1,
            false,
        ),
    ];
    let mut archive = Flow {
        id: "F3".into(),
        name: LONG_FLOW.into(),
        sort_order: 2,
        created_at: "2026-10-20 09:00:00".into(),
        updated_at: "2026-10-22 11:40:00".into(),
        ..Default::default()
    };
    archive.operations = vec![operation(
        "O6",
        "F3",
        "",
        LONG_DIR,
        LONG_REMOTE,
        "/Family Archive/Cabin Trips/2024/August/Originals",
        "push",
        0,
        false,
    )];
    for flow in [&mut nightly, &mut music, &mut archive] {
        let layout = graph::default_layout(&flow.operations);
        flow.canvas_json = graph::serialize_canvas(&layout);
    }
    vec![nightly, music, archive]
}

/// In-memory data with the transfer-bearing flow last in runtime lookup order.
#[cfg(test)]
pub(crate) fn synthetic_snapshot(
    remote_count: usize,
    flow_count: usize,
    selected_edges: usize,
    transfer_count: usize,
) -> Snapshot {
    assert!(remote_count > 0 && flow_count > 0 && selected_edges > 0);
    let template = fixtures().remove(0);
    let remotes: Vec<Remote> = (0..remote_count)
        .map(|index| Remote {
            name: format!("synthetic-remote-{index:03}"),
            kind: "drive".into(),
            description: format!("Synthetic account {index:03}"),
        })
        .collect();
    let flows: Vec<Flow> = (0..flow_count)
        .map(|index| {
            let mut flow = template.clone();
            flow.id = format!("synthetic-flow-{index:06}");
            flow.name = format!("Synthetic Flow {index:06}");
            // Deliberately unsorted, without depending on randomness or a clock.
            flow.sort_order = (flow_count - index - 1) as i64;
            let edge_count = if index == 0 { selected_edges } else { 1 };
            flow.operations = (0..edge_count)
                .map(|edge| {
                    let mut op = template.operations[0].clone();
                    op.id = format!("synthetic-op-{index:06}-{edge:06}");
                    op.flow_id = flow.id.clone();
                    op.source_remote = remotes[index % remote_count].name.clone();
                    op.target_remote = remotes[(index + edge + 1) % remote_count].name.clone();
                    op.source_path = format!("/synthetic/flow-{index:06}/source-{edge:06}");
                    op.target_path = format!("/synthetic/flow-{index:06}/target-{edge:06}");
                    op.sort_order = edge as i64;
                    op.sync_config.insert("dry_run".into(), json!(false));
                    op.sync_config.insert("dryRun".into(), json!(false));
                    op
                })
                .collect();
            flow.canvas_json = graph::serialize_canvas(&graph::default_layout(&flow.operations));
            flow
        })
        .collect();
    let transfers: Vec<FileTransfer> = (0..transfer_count)
        .map(|index| {
            let status = [
                "pending",
                "completed",
                "checking",
                "transferring",
                "failed",
                "checked",
            ][index % 6];
            let size = (index as u64 + 1) * 4096;
            let bytes = match status {
                "completed" | "checked" => size,
                "pending" => 0,
                _ => size / 2,
            };
            FileTransfer {
                // Interleaved folders and descending names require real tie sorting.
                name: format!(
                    "synthetic/batch-{:03}/file-{:06}.bin",
                    index % 97,
                    transfer_count - index
                ),
                size,
                bytes,
                progress: bytes as f64 / size as f64 * 100.0,
                status: status.into(),
                speed: if status == "transferring" {
                    65_536.0
                } else {
                    0.0
                },
                error: if status == "failed" {
                    "Synthetic transfer failure".into()
                } else {
                    String::new()
                },
            }
        })
        .collect();
    let selected = &flows[0];
    let progress = SyncProgress {
        task_id: "synthetic-task-000000".into(),
        profile_id: format!("{}:{}", selected.id, selected.operations[0].id),
        action: selected.operations[0].action.clone(),
        state: "running".into(),
        transferred: transfers.iter().map(|file| file.bytes).sum(),
        total: transfers.iter().map(|file| file.size).sum(),
        bytes_per_sec: transfers.iter().map(|file| file.speed).sum(),
        errors: transfers
            .iter()
            .filter(|file| file.status == "failed")
            .count() as u64,
        current_file: transfers
            .iter()
            .find(|file| file.status == "transferring")
            .map(|file| file.name.clone())
            .unwrap_or_default(),
        stage: "transferring".into(),
        files_transferred: transfers
            .iter()
            .filter(|file| file.status == "completed")
            .count() as u64,
        total_files: transfer_count as u64,
        transfers,
        ..Default::default()
    };
    let mut runtime = Runtime {
        revision: 1,
        flows: flows
            .iter()
            .rev()
            .map(|flow| RuntimeFlow {
                id: flow.id.clone(),
                status: "idle".into(),
                ops: flow
                    .operations
                    .iter()
                    .map(|op| RuntimeOp {
                        id: op.id.clone(),
                        status: "idle".into(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            })
            .collect(),
    };
    let active = runtime.flows.last_mut().unwrap();
    active.status = "running".into();
    active.ops[0].status = "running".into();
    active.sync = Some(progress);
    Snapshot {
        flows,
        remotes,
        runtime,
        ..Default::default()
    }
}

const LONG_FLOW: &str =
    "Archive of the family photo library from the old apartment and the cabin trips";
const LONG_DIR: &str = "/Users/me/Pictures/Vacation/Family Archive From The Old Apartment And The Cabin Trips/2024/August";
const LONG_FILE: &str = "IMG_family_reunion_at_the_cabin_by_the_lake_original_filename_0001.heic";
const LONG_REMOTE: &str = "family-archive-drive-from-the-old-laptop";
const LONG_REMOTE_DESC: &str = "Google Drive account used for the full family photo archive, scanned documents, and cabin trip videos";
const LONG_ERR: &str = "rclone: failed to copy /Users/me/Pictures/Vacation/Family Archive From The Old Apartment And The Cabin Trips/2024/August/IMG_family_reunion_at_the_cabin_by_the_lake_original_filename_0001.heic: connection reset by peer (stderr: read tcp 10.0.0.8:443: connection reset by peer)";

fn failed_archive_runtime() -> Runtime {
    let file = format!("{LONG_DIR}/{LONG_FILE}");
    Runtime {
        revision: 7,
        flows: vec![RuntimeFlow {
            id: "F3".into(),
            status: "failed".into(),
            last_error: LONG_ERR.into(),
            ops: vec![RuntimeOp {
                id: "O6".into(),
                status: "failed".into(),
                last_error: LONG_ERR.into(),
            }],
            sync: Some(SyncProgress {
                profile_id: "F3:O6".into(),
                action: "push".into(),
                state: "failed".into(),
                transferred: 4_100_000,
                total: 800_000_000,
                errors: 1,
                current_file: file.clone(),
                stage: "copy".into(),
                stage_detail: "family archive from the old apartment".into(),
                files_transferred: 1,
                total_files: 128,
                transfers: vec![
                    FileTransfer {
                        name: file,
                        size: 4_100_000,
                        bytes: 1_200_000,
                        progress: 0.29,
                        status: "failed".into(),
                        error: LONG_ERR.into(),
                        ..Default::default()
                    },
                    FileTransfer {
                        name: "waiting-for-the-next-cabin-photo-with-a-long-name.heic".into(),
                        size: 3_800_000,
                        status: "pending".into(),
                        ..Default::default()
                    },
                ],
                error_message: LONG_ERR.into(),
                ..Default::default()
            }),
            log: vec![RuntimeLog {
                at: 1,
                status: "failed".into(),
                op_id: "O6".into(),
                error: LONG_ERR.into(),
                label: String::new(),
            }],
        }],
    }
}

#[allow(clippy::too_many_arguments)]
fn operation(
    id: &str,
    flow_id: &str,
    source_remote: &str,
    source_path: &str,
    target_remote: &str,
    target_path: &str,
    action: &str,
    sort_order: i64,
    dry_run: bool,
) -> Operation {
    let mut config = serde_json::Map::new();
    config.insert("action".into(), json!(action));
    if dry_run {
        config.insert("dry_run".into(), json!(true));
        config.insert("dryRun".into(), json!(true));
    }
    Operation {
        id: id.into(),
        flow_id: flow_id.into(),
        source_remote: source_remote.into(),
        source_path: source_path.into(),
        target_remote: target_remote.into(),
        target_path: target_path.into(),
        action: action.into(),
        sync_config: config,
        is_expanded: false,
        sort_order,
    }
}

fn fixture_history() -> Vec<HistoryEntry> {
    vec![
        HistoryEntry {
            id: "T99".into(),
            profile_name: LONG_FLOW.into(),
            action: "push".into(),
            state: "failed".into(),
            started_at: "2026-10-21T18:20:00Z".into(),
            finished_at: "2026-10-21T18:22:00Z".into(),
            duration: 120.0,
            bytes: 68_000_000,
            errors: 1,
            files: 12,
            error_message: LONG_ERR.into(),
        },
        HistoryEntry {
            id: "T101".into(),
            profile_name: "F1:O1".into(),
            action: "push".into(),
            state: "completed".into(),
            started_at: "2026-10-22T03:00:00Z".into(),
            finished_at: "2026-10-22T03:02:14Z".into(),
            duration: 134.0,
            bytes: 248_000_000,
            errors: 0,
            files: 42,
            error_message: String::new(),
        },
        HistoryEntry {
            id: "T102".into(),
            profile_name: "F1:O2".into(),
            action: "push".into(),
            state: "completed".into(),
            started_at: "2026-10-22T03:02:14Z".into(),
            finished_at: "2026-10-22T03:02:51Z".into(),
            duration: 37.0,
            bytes: 12_400_000,
            errors: 0,
            files: 6,
            error_message: String::new(),
        },
        HistoryEntry {
            id: "T98".into(),
            profile_name: "F1:O3".into(),
            action: "bi".into(),
            state: "cancelled".into(),
            started_at: "2026-10-21T03:01:00Z".into(),
            finished_at: "2026-10-21T03:01:20Z".into(),
            duration: 20.0,
            bytes: 1_200_000,
            errors: 0,
            files: 3,
            error_message: String::new(),
        },
        HistoryEntry {
            id: "T97".into(),
            profile_name: "photos".into(),
            action: "push".into(),
            state: "completed".into(),
            started_at: "2026-10-20T09:00:00Z".into(),
            finished_at: "2026-10-20T09:04:00Z".into(),
            duration: 240.0,
            bytes: 512_000_000,
            errors: 0,
            files: 88,
            error_message: String::new(),
        },
    ]
}

fn cancelled_history(flow_id: &str, task_id: &str) -> HistoryEntry {
    HistoryEntry {
        id: task_id.into(),
        profile_name: format!("{flow_id}:O1"),
        action: "push".into(),
        state: "cancelled".into(),
        started_at: "2026-10-22T12:00:00Z".into(),
        finished_at: "2026-10-22T12:00:09Z".into(),
        duration: 9.0,
        bytes: 1_400_000,
        errors: 0,
        files: 1,
        error_message: String::new(),
    }
}

fn fixture_logs() -> Vec<LogLine> {
    let rows = [
        ("DEBUG", "engine: starting serve bridge"),
        ("INFO", "sync: started"),
        ("ERROR", LONG_ERR),
        ("INFO", "sync: 1/6 transferring IMG_0421.heic"),
        ("INFO", "sync: 2/6 transferring IMG_0422.heic"),
        ("INFO", "sync: completed"),
        ("INFO", "scheduler: registered flow:F1 (0 3 * * *)"),
        ("INFO", "remotes: listed 3 remotes"),
        ("WARN", "update: channel is arm64-only"),
        ("INFO", "service: heartbeat written"),
        ("DEBUG", "runtime: snapshot revision 42"),
        ("INFO", "auth: unlocked"),
        ("INFO", "state: flows changed (F1)"),
    ];
    rows.iter()
        .enumerate()
        .map(|(index, (level, msg))| LogLine {
            time: format!("2026-10-22T12:{:02}:{:02}Z", index, (index * 7) % 60),
            level: (*level).into(),
            msg: (*msg).into(),
            fields: {
                let mut map = serde_json::Map::new();
                map.insert("engine".into(), json!("mock"));
                map
            },
        })
        .collect()
}

fn fixture_entries(path: &str) -> Vec<FileEntry> {
    let dirs: &[&str] = if path.ends_with(':') || path == "/" {
        &["Photos", "Documents", "Music", "Backups", "Projects"]
    } else {
        &["2025", "2026", "Screenshots"]
    };
    let files: &[(&str, i64)] = if path.ends_with(':') || path == "/" {
        &[("README.txt", 2_048), ("rclone.conf.bak", 1_120)]
    } else {
        &[
            ("IMG_0419.heic", 4_100_000),
            ("IMG_0420.heic", 4_800_000),
            (LONG_FILE, 4_100_000),
        ]
    };
    let mut entries: Vec<FileEntry> = dirs
        .iter()
        .map(|name| FileEntry {
            name: (*name).into(),
            size: -1,
            mime_type: "inode/directory".into(),
            is_dir: true,
            mod_time: "2026-10-20T12:00:00Z".into(),
            path: (*name).into(),
            id: String::new(),
        })
        .collect();
    entries.extend(files.iter().map(|(name, size)| FileEntry {
        name: (*name).into(),
        size: *size,
        mime_type: "application/octet-stream".into(),
        is_dir: false,
        mod_time: "2026-10-20T12:00:00Z".into(),
        path: (*name).into(),
        id: String::new(),
    }));
    entries
}

fn providers() -> Vec<Provider> {
    fn option(name: &str, help: &str, required: bool) -> crate::engine::ProviderOption {
        crate::engine::ProviderOption {
            name: name.into(),
            help: help.into(),
            required,
            ..Default::default()
        }
    }
    let mut out = Vec::new();
    let mut add = |name: &str,
                   description: &str,
                   oauth: bool,
                   options: Vec<crate::engine::ProviderOption>| {
        out.push(Provider {
            name: name.into(),
            description: description.into(),
            prefix: name.into(),
            hidden: false,
            oauth,
            options,
        });
    };
    add(
        "drive",
        "Google Drive account for photos, documents, and shared family folders that live in the cloud",
        true,
        vec![],
    );
    add("onedrive", "Microsoft OneDrive", true, vec![]);
    add("dropbox", "Dropbox", true, vec![]);
    add("box", "Box", true, vec![]);
    add("pcloud", "Pcloud", true, vec![]);
    add("yandex", "Yandex Disk", true, vec![]);
    add(
        "sftp",
        "SSH/SFTP",
        false,
        vec![
            option("host", "SSH host to connect to.", true),
            option("user", "SSH username.", false),
            option("port", "SSH port number.", false),
            option("pass", "SSH password.", false),
        ],
    );
    add(
        "ftp",
        "FTP",
        false,
        vec![
            option("host", "FTP host to connect to.", true),
            option("user", "FTP username.", false),
            option("pass", "FTP password.", false),
        ],
    );
    add(
        "webdav",
        "WebDAV",
        false,
        vec![
            option("url", "URL of the WebDAV service.", true),
            option("user", "WebDAV username.", false),
            option("pass", "WebDAV password.", false),
        ],
    );
    add(
        "s3",
        "Amazon S3 compatible",
        false,
        vec![
            option("provider", "S3 provider name.", false),
            option("access_key_id", "Access key ID.", false),
            option("secret_access_key", "Secret access key.", false),
            option("region", "Region.", false),
        ],
    );
    add(
        "b2",
        "Backblaze B2",
        false,
        vec![
            option("account", "Account ID or application key ID.", true),
            option("key", "Application key.", true),
        ],
    );
    add(
        "mega",
        "Mega",
        false,
        vec![
            option("user", "Mega username.", true),
            option("pass", "Mega password.", true),
        ],
    );
    add("gphotos", "Google Photos", true, vec![]);
    add(
        "iclouddrive",
        "iCloud Drive",
        false,
        vec![
            option("service", "iCloud service.", true),
            option("apple_id", "Apple ID.", true),
            option("password", "Apple password.", true),
            option("trust_token", "2FA trust token.", false),
        ],
    );
    add(
        "smb",
        "SMB / Windows File Sharing",
        false,
        vec![
            option("host", "SMB host to connect to.", true),
            option("user", "SMB username.", false),
            option("pass", "SMB password.", false),
        ],
    );
    add("local", "Local Disk", false, vec![]);
    add(
        "azureblob",
        "Azure Blob Storage",
        false,
        vec![
            option("account", "Storage account name.", true),
            option("key", "Storage account key.", true),
        ],
    );
    add(
        "swift",
        "OpenStack Swift",
        false,
        vec![
            option("user", "User name.", true),
            option("key", "API key.", true),
            option("auth", "Authentication URL.", true),
        ],
    );
    add(
        "hdfs",
        "Hadoop Distributed File System",
        false,
        vec![option("namenode", "Hadoop name node.", true)],
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;

    fn result<T: serde::de::DeserializeOwned>(frames: &[crate::engine::Frame], id: u64) -> T {
        let frame = frames
            .iter()
            .find(|frame| frame.id() == Some(id))
            .expect("response frame");
        serde_json::from_value(frame.result().expect("result").clone()).unwrap()
    }

    #[test]
    fn unlock_then_execute_streams_progress() {
        let engine = Engine::start(true);
        let id = engine
            .request("auth.unlock", json!({"password": "drive1234"}))
            .unwrap();
        let frames = engine.poll();
        let auth: AuthStatus = result(&frames, id);
        assert!(auth.unlocked);

        engine.request("state.snapshot", json!({})).unwrap();
        let frames = engine.poll();
        let snapshot: Snapshot = result(&frames, 2);
        assert!(!snapshot.locked);
        assert_eq!(snapshot.flows.len(), 3);
        let archive = snapshot
            .runtime
            .flows
            .iter()
            .find(|flow| flow.id == "F3")
            .expect("seeded failed archive");
        assert_eq!(archive.status, "failed");
        let sync = archive.sync.as_ref().expect("seeded sync");
        assert_eq!(sync.profile_id, "F3:O6");
        assert!(sync.error_message.contains("connection reset"));

        engine
            .request("flows.execute", json!({"id": "F1"}))
            .unwrap();
        let frames = engine.poll();
        assert!(
            frames
                .iter()
                .any(|frame| frame.event() == Some("sync:started"))
        );
        assert!(
            frames
                .iter()
                .any(|frame| frame.event() == Some("sync:progress"))
        );
        let progress = frames
            .iter()
            .find(|frame| frame.event() == Some("sync:progress"))
            .unwrap();
        assert_eq!(progress.data()["profileID"], "F1:O1");
    }

    #[test]
    fn wrong_password_reports_attempts() {
        let engine = Engine::start(true);
        engine
            .request("auth.unlock", json!({"password": "nope"}))
            .unwrap();
        let frames = engine.poll();
        let error = frames
            .iter()
            .find_map(|frame| frame.error())
            .expect("an error frame");
        assert_eq!(error.code, "invalid_password");
        assert_eq!(error.data["failedAttempts"], 1);
    }

    #[test]
    fn three_wrong_passwords_lock_for_thirty_seconds() {
        let engine = Engine::start(true);
        for _ in 0..2 {
            engine
                .request("auth.unlock", json!({"password": "nope"}))
                .unwrap();
            let frames = engine.poll();
            let error = frames.iter().find_map(|frame| frame.error()).unwrap();
            assert_eq!(error.code, "invalid_password");
        }
        engine
            .request("auth.unlock", json!({"password": "nope"}))
            .unwrap();
        let frames = engine.poll();
        let error = frames.iter().find_map(|frame| frame.error()).unwrap();
        assert_eq!(error.code, "locked_out");
        assert_eq!(error.retry_after(), Some(30));
        assert!(error.message.to_ascii_lowercase().contains("retry in"));

        engine
            .request("auth.unlock", json!({"password": "drive1234"}))
            .unwrap();
        let frames = engine.poll();
        let error = frames.iter().find_map(|frame| frame.error()).unwrap();
        assert_eq!(error.code, "locked_out");
        let retry = error.retry_after().unwrap_or(0);
        assert!(retry > 0 && retry <= 30, "retry {retry}");
    }

    #[test]
    fn dev_lockout_rejects_the_real_password() {
        let engine = Engine::start(true);
        let id = engine
            .request("dev.lockout", json!({"seconds": 23}))
            .unwrap();
        let frames = engine.poll();
        let auth: AuthStatus = result(&frames, id);
        assert!(auth.lockout.is_locked);
        assert!(auth.lockout.retry_after_secs <= 23 && auth.lockout.retry_after_secs >= 22);

        engine
            .request("auth.unlock", json!({"password": "drive1234"}))
            .unwrap();
        let frames = engine.poll();
        let error = frames.iter().find_map(|frame| frame.error()).unwrap();
        assert_eq!(error.code, "locked_out");
    }

    #[test]
    fn locked_snapshot_is_empty() {
        let engine = Engine::start(true);
        engine.request("state.snapshot", json!({})).unwrap();
        let frames = engine.poll();
        let snapshot: Snapshot = result(&frames, 1);
        assert!(snapshot.locked);
        assert!(snapshot.flows.is_empty());
        assert!(snapshot.remotes.is_empty());
    }
}
