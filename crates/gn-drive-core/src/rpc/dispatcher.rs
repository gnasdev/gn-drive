//! Method dispatcher for protocol v1 — every method in `docs/gpui/rpc.md`
//! (port of `RPCDispatcher.swift`). Holds no transport state; the stdio
//! service / GPUI window own framing and event delivery.
//!
//! remotes.*, remotes.oauth.*, fs.list, service.* and update.* live in
//! `handlers_remotes.rs` (same `impl Dispatcher`).

use super::bridge::EventBridge;
use super::error::{RpcError, sanitize};
use super::handlers_remotes::ServiceAction;
use super::oauth::{OAuthManager, OAuthSpawner, RealOAuthSpawner};
use super::{Event, LogBuffer, Options, PROTOCOL_VERSION, Params, json};
use crate::container::AppContainer;
use crate::cron::CronSchedule;
use crate::event_bus::BusEvent;
use crate::launchd::LaunchdService;
use crate::logger::LogLevel;
use crate::models::{Board, BoardEdge, BoardNode, Flow, FlowOperation, Profile, profile_direction};
use crate::rclone::client::RcloneClient;
use crate::self_update::{HttpFetcher, UreqFetcher};
use crate::store::{Store, StoreError};
use crate::sync_engine::SyncEngineError;
use crossbeam_channel::Receiver;
use parking_lot::Mutex;
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

type Res = Result<Value, RpcError>;

const ALWAYS_METHODS: &[&str] = &[
    "ping",
    "state.snapshot",
    "app.quit",
    "doctor",
    "logs.read",
    "auth.status",
    "auth.setup",
    "auth.unlock",
    "auth.lock",
    "auth.changePassword",
    "auth.removePassword",
    "remotes.providers",
    "settings.get",
    "settings.set",
    "service.status",
    "service.install",
    "service.uninstall",
    "service.start",
    "service.stop",
    "service.restart",
    "service.health",
    "update.check",
    "update.apply",
];

const LOCKED_GATED_METHODS: &[&str] = &[
    "flows.list",
    "flows.get",
    "flows.create",
    "flows.save",
    "flows.delete",
    "flows.execute",
    "flows.stop",
    "operations.create",
    "operations.update",
    "operations.delete",
    "operations.reorder",
    "runtime.snapshot",
    "runtime.tasks",
    "remotes.list",
    "remotes.add",
    "remotes.test",
    "remotes.delete",
    "remotes.oauth.start",
    "remotes.oauth.complete",
    "remotes.oauth.cancel",
    "fs.list",
    "profiles.list",
    "profiles.get",
    "profiles.save",
    "profiles.delete",
    "schedules.list",
    "sync.profile",
    "sync.stop",
    "boards.list",
    "boards.get",
    "boards.save",
    "boards.delete",
    "boards.execute",
    "boards.stop",
    "boards.status",
    "history.list",
    "history.stats",
    "history.clear",
];

fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    }
}

fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string().to_uppercase()
}

fn join_path(dir: &str, name: &str) -> String {
    Path::new(dir).join(name).to_string_lossy().into_owned()
}

fn not_found<T>(code: &str, message: String, r: Result<T, StoreError>) -> Result<T, RpcError> {
    match r {
        Ok(v) => Ok(v),
        Err(StoreError::NotFound) => Err(RpcError::new(code, message)),
        Err(e) => Err(e.into()),
    }
}

pub struct Dispatcher {
    container: Arc<AppContainer>,
    log_buffer: Arc<LogBuffer>,
    rclone_binary: String,
    bridge: Arc<EventBridge>,
    provider_cache: Mutex<Option<Vec<Map<String, Value>>>>,
    oauth_manager: Mutex<Option<Arc<OAuthManager>>>,
    oauth_spawner: Mutex<Option<Arc<dyn OAuthSpawner>>>,
    launchd: Mutex<Arc<LaunchdService>>,
    update_fetcher: Mutex<Arc<dyn HttpFetcher>>,
    update_api_base: Mutex<Option<String>>,
}

impl Dispatcher {
    pub fn new(
        container: Arc<AppContainer>,
        log_buffer: Arc<LogBuffer>,
        rclone_binary: &str,
        bridge: Arc<EventBridge>,
    ) -> Dispatcher {
        Dispatcher {
            container,
            log_buffer,
            rclone_binary: rclone_binary.to_string(),
            bridge,
            provider_cache: Mutex::new(None),
            oauth_manager: Mutex::new(None),
            oauth_spawner: Mutex::new(None),
            launchd: Mutex::new(Arc::new(LaunchdService::new())),
            update_fetcher: Mutex::new(Arc::new(UreqFetcher)),
            update_api_base: Mutex::new(None),
        }
    }

    /// In-process convenience: builds the log buffer + event bridge (which
    /// hooks the container logger sink) and the dispatcher.
    pub fn start(
        container: Arc<AppContainer>,
        rclone_binary: &str,
        opts: Options,
    ) -> Arc<Dispatcher> {
        let log_buffer = Arc::new(LogBuffer::new(opts.log_buffer_capacity));
        let bridge = EventBridge::new(container.clone(), log_buffer.clone(), &opts);
        Arc::new(Dispatcher::new(
            container,
            log_buffer,
            rclone_binary,
            bridge,
        ))
    }

    pub fn container(&self) -> &Arc<AppContainer> {
        &self.container
    }

    pub fn log_buffer(&self) -> &Arc<LogBuffer> {
        &self.log_buffer
    }

    pub fn bridge(&self) -> &Arc<EventBridge> {
        &self.bridge
    }

    /// Pushed events (delegates to the bridge).
    pub fn subscribe(&self) -> Receiver<Event> {
        self.bridge.subscribe()
    }

    /// Stops the bridge only; the owner closes the container.
    pub fn shutdown(&self) {
        self.bridge.shutdown();
    }

    // ---- test seams ----------------------------------------------------

    pub fn set_launchd(&self, l: Arc<LaunchdService>) {
        *self.launchd.lock() = l;
    }
    pub fn set_update_fetcher(&self, f: Arc<dyn HttpFetcher>) {
        *self.update_fetcher.lock() = f;
    }
    pub fn set_update_api_base(&self, base: Option<String>) {
        *self.update_api_base.lock() = base;
    }
    pub fn set_oauth_spawner(&self, s: Arc<dyn OAuthSpawner>) {
        *self.oauth_spawner.lock() = Some(s);
        *self.oauth_manager.lock() = None;
    }
    /// Install a ready-made OAuth manager (tests with custom timing).
    pub fn set_oauth_manager(&self, m: Arc<OAuthManager>) {
        *self.oauth_manager.lock() = Some(m);
    }

    // ---- state ---------------------------------------------------------

    pub fn is_locked(&self) -> bool {
        let a = &self.container.auth;
        a.is_setup() && !a.is_unlocked() && self.container.store().is_none()
    }

    // ---- entry ---------------------------------------------------------

    pub fn handle(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let always = ALWAYS_METHODS.contains(&method);
        if !always && !LOCKED_GATED_METHODS.contains(&method) {
            return Err(RpcError::new(
                "unknown_method",
                format!("unknown method: {method}"),
            ));
        }
        if !always && self.is_locked() {
            return Err(RpcError::locked());
        }
        let p = Params::new(&params)?;
        match method {
            "ping" => Ok(self.ping()),
            "state.snapshot" => self.state_snapshot(&p),
            "app.quit" => Ok(json!({ "quitting": true })),
            "doctor" => self.doctor(&p),
            "logs.read" => self.logs_read(&p),

            "auth.status" => Ok(json::auth_status(&self.container.auth.status())),
            "auth.setup" => self.auth_setup(&p),
            "auth.unlock" => self.auth_unlock(&p),
            "auth.lock" => self.auth_lock(),
            "auth.changePassword" => self.auth_change_password(&p),
            "auth.removePassword" => self.auth_remove_password(&p),

            "flows.list" => self.flows_list(),
            "flows.get" => self.flows_get(&p),
            "flows.create" => self.flows_create(&p),
            "flows.save" => self.flows_save(&p),
            "flows.delete" => self.flows_delete(&p),
            "flows.execute" => self.flows_execute(&p),
            "flows.stop" => self.flows_stop(&p),
            "operations.create" => self.operations_create(&p),
            "operations.update" => self.operations_update(&p),
            "operations.delete" => self.operations_delete(&p),
            "operations.reorder" => self.operations_reorder(&p),

            "runtime.snapshot" => Ok(json::runtime_snapshot(&self.container.runtime.snapshot())),
            "runtime.tasks" => Ok(json!({
                "tasks": self.container.sync_engine.active_tasks().iter().map(json::task).collect::<Vec<_>>()
            })),

            "remotes.providers" => self.remotes_providers(&p),
            "remotes.list" => self.remotes_list(),
            "remotes.add" => self.remotes_add(&p),
            "remotes.test" => self.remotes_test(&p),
            "remotes.delete" => self.remotes_delete(&p),
            "remotes.oauth.start" => self.oauth_start(&p),
            "remotes.oauth.complete" => self.oauth_complete(&p),
            "remotes.oauth.cancel" => self.oauth_cancel(&p),

            "fs.list" => self.fs_list(&p),

            "profiles.list" => self.profiles_list(),
            "profiles.get" => self.profiles_get(&p),
            "profiles.save" => self.profiles_save(&p),
            "profiles.delete" => self.profiles_delete(&p),
            "schedules.list" => self.schedules_list(),

            "sync.profile" => self.sync_profile(&p),
            "sync.stop" => self.sync_stop(&p),

            "boards.list" => self.boards_list(),
            "boards.get" => self.boards_get(&p),
            "boards.save" => self.boards_save(&p),
            "boards.delete" => self.boards_delete(&p),
            "boards.execute" => self.boards_execute(&p),
            "boards.stop" => self.boards_stop(&p),
            "boards.status" => self.boards_status(&p),

            "history.list" => self.history_list(&p),
            "history.stats" => self.history_stats(),
            "history.clear" => self.history_clear(),

            "settings.get" => {
                Ok(json!({ "settings": json::app_settings(&self.container.auth.app_settings()) }))
            }
            "settings.set" => self.settings_set(&p),

            "service.status" => self.service_status(&p),
            "service.install" => self.service_install(&p),
            "service.uninstall" => self.service_uninstall(&p),
            "service.start" => self.service_action(&p, ServiceAction::Start),
            "service.stop" => self.service_action(&p, ServiceAction::Stop),
            "service.restart" => self.service_action(&p, ServiceAction::Restart),
            "service.health" => Ok(self.service_health()),

            "update.check" => self.update_check(&p),
            "update.apply" => self.update_apply(&p),

            _ => Err(RpcError::new(
                "unknown_method",
                format!("unknown method: {method}"),
            )),
        }
    }

    fn paths_json(&self, with_working_dir: bool) -> Value {
        let c = &self.container.config;
        let mut m = Map::new();
        m.insert("configDir".into(), c.config_dir.clone().into());
        m.insert(
            "dbPath".into(),
            join_path(&c.config_dir, "gn-drive.db").into(),
        );
        m.insert(
            "rcloneConfigPath".into(),
            join_path(&c.config_dir, "rclone.conf").into(),
        );
        m.insert("logDir".into(), c.log_dir.clone().into());
        if with_working_dir {
            m.insert("workingDir".into(), c.working_dir.clone().into());
        }
        Value::Object(m)
    }

    /// Payload for the first (hello) frame.
    pub fn hello_data(&self, lock_path: &str, pid: i32) -> Value {
        let status = self.container.auth.status();
        let bin = self
            .resolved_rclone_binary()
            .ok()
            .or_else(|| self.container.rclone().map(|r| r.binary.clone()));
        let mut hello = json!({
            "protocol": PROTOCOL_VERSION,
            "appVersion": self.container.version,
            "engine": "GNDriveCore",
            "pid": pid,
            "setup": status.setup,
            "unlocked": status.unlocked,
            "passwordRequired": status.setup && !status.unlocked,
            "platform": { "os": "macos", "arch": arch() },
            "paths": self.paths_json(true),
            "capabilities": {
                "oauth": true, "providers": true, "profiles": true, "boards": true,
                "service": true, "update": true, "logs": true, "history": true, "doctor": true,
            },
            "instance": { "pid": pid, "lockPath": lock_path },
        });
        if let Some(bin) = bin {
            let version = RcloneClient::resolve_binary(None)
                .ok()
                .and_then(|_| self.container.rclone().and_then(|r| r.version().ok()));
            hello["rclone"] = json!({
                "path": bin,
                "version": version,
                "bundled": Some(&bin) == RcloneClient::bundled_binary().as_ref(),
            });
        }
        hello
    }

    // ---- system --------------------------------------------------------

    fn ping(&self) -> Value {
        json!({
            "pong": true,
            "protocol": PROTOCOL_VERSION,
            "version": self.container.version,
            "now": json::rfc3339(chrono::Utc::now()),
        })
    }

    fn state_snapshot(&self, p: &Params) -> Res {
        const ALL: [&str; 7] = [
            "auth", "flows", "remotes", "profiles", "history", "settings", "runtime",
        ];
        let sections: HashSet<String> = match p.string_array("sections")? {
            Some(req) => req
                .into_iter()
                .filter(|s| ALL.contains(&s.as_str()))
                .collect(),
            None => ALL.iter().map(|s| s.to_string()).collect(),
        };
        let has = |s: &str| sections.contains(s);
        let status = self.container.auth.status();
        let locked = !status.unlocked;

        let mut out = json!({
            "locked": locked,
            "auth": json::auth_status(&status),
            "version": self.container.version,
            "paths": self.paths_json(false),
        });
        if has("settings") {
            out["appSettings"] = json::app_settings(&self.container.auth.app_settings());
        }
        if has("runtime") {
            out["runtime"] = if locked {
                json!({ "revision": 0, "flows": [] })
            } else {
                json::runtime_snapshot(&self.container.runtime.snapshot())
            };
        }
        let store = if locked { None } else { self.container.store() };
        let Some(store) = store else {
            for k in ["flows", "remotes", "profiles", "history"] {
                if has(k) {
                    out[k] = json!([]);
                }
            }
            return Ok(out);
        };
        let rc = self.container.rclone();
        if has("flows") {
            out["flows"] = store
                .list_flows()
                .map(|v| v.iter().map(json::flow).collect())
                .unwrap_or_else(|_| Vec::<Value>::new())
                .into();
        }
        if has("remotes") {
            out["remotes"] = rc
                .as_ref()
                .and_then(|r| r.list_remotes().ok())
                .map(|v| v.iter().map(json::remote).collect::<Vec<_>>())
                .unwrap_or_default()
                .into();
        }
        if has("profiles") {
            out["profiles"] = store
                .list_profiles()
                .map(|v| v.iter().map(json::profile).collect())
                .unwrap_or_else(|_| Vec::<Value>::new())
                .into();
        }
        if has("history") {
            out["history"] = store
                .list_history(100, 0)
                .map(|v| v.iter().map(json::history).collect())
                .unwrap_or_else(|_| Vec::<Value>::new())
                .into();
        }
        if let Some(v) = rc.and_then(|r| r.version().ok()) {
            out["rcloneVersion"] = v.into();
        }
        Ok(out)
    }

    fn version_of(bin: &str) -> Option<String> {
        let out = std::process::Command::new(bin)
            .arg("version")
            .stdin(std::process::Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.split('\n').next().map(|l| l.trim().to_string())
    }

    fn doctor(&self, p: &Params) -> Res {
        let include_files = p.bool("includeDataFiles")?.unwrap_or(false);
        let mut checks: Vec<Value> = Vec::new();
        let mut add = |id: &str, label: &str, status: &str, detail: &str| {
            checks.push(json!({ "id": id, "label": label, "status": status, "detail": detail }));
        };

        let rc = self.container.rclone();
        let bin = self
            .resolved_rclone_binary()
            .ok()
            .or_else(|| rc.as_ref().map(|r| r.binary.clone()));
        if let Some(bin) = bin {
            add("rclone_binary", "rclone binary", "ok", &bin);
            let version = rc
                .as_ref()
                .and_then(|r| r.version().ok())
                .or_else(|| Self::version_of(&bin))
                .unwrap_or_else(|| "unknown".into());
            add(
                "rclone_version",
                "rclone version",
                if version == "unknown" { "warn" } else { "ok" },
                &version,
            );
        } else {
            add("rclone_binary", "rclone binary", "error", "not found");
        }

        let cfg_dir = self.container.config.config_dir.clone();
        let exists = Path::new(&cfg_dir).exists();
        add(
            "config_dir",
            "config directory",
            if exists { "ok" } else { "error" },
            &cfg_dir,
        );
        let db_exists = Path::new(&join_path(&cfg_dir, "gn-drive.db")).exists();
        add(
            "database",
            "database",
            if db_exists { "ok" } else { "info" },
            if db_exists {
                "gn-drive.db"
            } else {
                "gn-drive.db (not created yet)"
            },
        );

        let status = self.container.auth.status();
        let auth_text = if status.setup {
            format!(
                "configured: yes, unlocked: {}",
                if status.unlocked { "yes" } else { "no" }
            )
        } else {
            "configured: no".to_string()
        };
        add("auth", "auth", "info", &auth_text);

        if self.is_locked() {
            add("remotes", "remotes", "info", "locked");
            add("profiles", "profiles", "info", "locked");
        } else {
            match rc.as_ref().and_then(|r| r.list_remotes().ok()) {
                Some(r) => add(
                    "remotes",
                    "remotes",
                    "info",
                    &format!("{} configured", r.len()),
                ),
                None => add("remotes", "remotes", "warn", "unavailable"),
            }
            match self.container.store().and_then(|s| s.list_profiles().ok()) {
                Some(pr) => add(
                    "profiles",
                    "profiles",
                    "info",
                    &format!("{} configured", pr.len()),
                ),
                None => add("profiles", "profiles", "warn", "unavailable"),
            }
        }
        add("platform", "platform", "ok", &format!("darwin {}", arch()));

        let ok = !checks.iter().any(|c| c["status"] == "error");
        let mut out = json!({
            "ok": ok,
            "checks": checks,
            "platform": { "os": "macos", "arch": arch() },
        });
        if include_files {
            let mut names: Vec<String> = std::fs::read_dir(&cfg_dir)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            out["dataFiles"] = names.into();
        }
        Ok(out)
    }

    fn logs_read(&self, p: &Params) -> Res {
        let mut limit = p.int("limit")?.unwrap_or(200);
        if limit < 0 {
            return Err(RpcError::invalid_params("limit", None));
        }
        limit = limit.min(2000);
        let mut min_level = None;
        if let Some(level) = p.string("level")? {
            min_level = Some(
                LogLevel::from_label(&level)
                    .ok_or_else(|| RpcError::invalid_params("level", None))?,
            );
        }
        let since = p.int("since")?;
        Ok(self.log_buffer.read(limit as usize, min_level, since))
    }

    // ---- auth ----------------------------------------------------------

    fn publish_state(&self, ev: BusEvent) {
        self.container.bus.publish(ev);
    }

    fn auth_setup(&self, p: &Params) -> Res {
        let password = p.require_string("password")?;
        self.container.auth.setup_password(&password)?;
        self.container.after_unlock()?;
        self.publish_state(BusEvent::auth_unlocked());
        Ok(json::auth_status(&self.container.auth.status()))
    }

    fn auth_unlock(&self, p: &Params) -> Res {
        use crate::auth::AuthError;
        let password = p.require_string("password")?;
        match self.container.auth.unlock(&password) {
            Ok(()) => {}
            Err(AuthError::InvalidPassword) => {
                let l = self.container.auth.lockout_status();
                return Err(RpcError::new("invalid_password", "auth: invalid password")
                    .with_data(json!({ "failedAttempts": l.failed_attempts })));
            }
            Err(AuthError::Locked { retry_after_secs }) => {
                let l = self.container.auth.lockout_status();
                return Err(RpcError::new(
                    "locked_out",
                    format!("auth: locked — retry in {retry_after_secs}s"),
                )
                .with_data(
                    json!({ "retryAfterSecs": retry_after_secs, "lockedUntil": l.locked_until }),
                ));
            }
            Err(e) => return Err(e.into()),
        }
        self.container.after_unlock()?;
        self.publish_state(BusEvent::auth_unlocked());
        Ok(json::auth_status(&self.container.auth.status()))
    }

    fn auth_lock(&self) -> Res {
        self.container.before_lock();
        self.container.auth.lock()?;
        self.publish_state(BusEvent::auth_locked());
        Ok(json::auth_status(&self.container.auth.status()))
    }

    fn auth_change_password(&self, p: &Params) -> Res {
        let old = p.require_string("oldPassword")?;
        let new = p.require_string("newPassword")?;
        self.container.auth.change_password(&old, &new)?;
        Ok(json::auth_status(&self.container.auth.status()))
    }

    fn auth_remove_password(&self, p: &Params) -> Res {
        let password = p.require_string("password")?;
        self.container.before_lock();
        self.container.auth.remove_password(&password)?;
        self.container.after_unlock()?;
        self.publish_state(BusEvent::auth_unlocked());
        Ok(json::auth_status(&self.container.auth.status()))
    }

    // ---- flows ---------------------------------------------------------

    fn flows_list(&self) -> Res {
        let store = self.require_store()?;
        Ok(json!({ "flows": store.list_flows()?.iter().map(json::flow).collect::<Vec<_>>() }))
    }

    fn flows_get(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let id = p.require_string("id")?;
        let flow = not_found(
            "flow_not_found",
            format!("flow {id} not found"),
            store.get_flow(&id),
        )?;
        Ok(json!({ "flow": json::flow(&flow) }))
    }

    fn flows_create(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let name = p.string("name")?.unwrap_or_default();
        let id = p.string("id")?.unwrap_or_default();
        let flow_id = if id.is_empty() { new_uuid() } else { id };
        if store.get_flow(&flow_id).is_ok() {
            return Err(RpcError::new(
                "flow_exists",
                format!("flow {flow_id} already exists"),
            ));
        }
        let mut f = Flow {
            id: flow_id.clone(),
            ..Default::default()
        };
        f.name = if name.trim_matches([' ', '\t']).is_empty() {
            "Untitled flow".into()
        } else {
            name
        };
        f.canvas_json = match p.string("canvasJSON")? {
            Some(c) if !c.is_empty() => c,
            _ => match p.dict("canvas")? {
                Some(c) => json::stringify_json(&c),
                None => "{}".into(),
            },
        };
        store.save_flow(&mut f)?;
        let saved = store.get_flow(&flow_id)?;
        self.emit_state_changed("flows", &flow_id);
        Ok(json!({ "flow": json::flow(&saved) }))
    }

    fn flows_save(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let raw = p
            .dict("flow")?
            .ok_or_else(|| RpcError::invalid_params("flow", None))?;
        let mut flow = Self::parse_flow(&raw)?;
        if flow.id.is_empty() {
            return Err(RpcError::invalid_params("flow.id", None));
        }
        self.ensure_flow_mutable(&flow.id)?;

        let flow_id = flow.id.clone();
        for (i, op) in flow.operations.iter_mut().enumerate() {
            if op.id.is_empty() {
                op.id = new_uuid();
            }
            op.flow_id = flow_id.clone();
            if op.sort_order == 0 {
                op.sort_order = i as i64;
            }
            op.normalize_action();
        }

        let mut warnings: Vec<Value> = Vec::new();
        if flow.schedule_enabled
            && !flow.schedule_cron.trim_matches([' ', '\t']).is_empty()
            && CronSchedule::parse(&flow.schedule_cron).is_err()
        {
            warnings.push(json!({
                "code": "invalid_cron",
                "message": format!("invalid cron expression: {}", flow.schedule_cron),
            }));
        }

        store.save_flow(&mut flow)?;
        self.container.sync_engine.sync_flow_schedule(&flow);
        self.emit_state_changed("flows", &flow.id);
        let saved = store.get_flow(&flow.id)?;
        Ok(json!({ "flow": json::flow(&saved), "warnings": warnings }))
    }

    fn flows_delete(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let id = p.require_string("id")?;
        not_found(
            "flow_not_found",
            format!("flow {id} not found"),
            store.get_flow(&id),
        )?;
        self.ensure_flow_mutable(&id)?;
        store.delete_flow(&id)?;
        self.container.sync_engine.unregister_flow_schedule(&id);
        self.container.runtime.forget(&id);
        self.emit_state_changed("flows", &id);
        Ok(json!({ "deleted": true, "id": id }))
    }

    fn flows_execute(&self, p: &Params) -> Res {
        let id = p.require_string("id")?;
        // Background run; the handle is detached (status flows via events).
        drop(self.container.flow_engine.start(&id)?);
        Ok(json!({ "flowID": id, "status": "running" }))
    }

    fn flows_stop(&self, p: &Params) -> Res {
        let id = p.require_string("id")?;
        self.container.flow_engine.stop(&id)?;
        Ok(json!({ "flowID": id, "status": "cancelling" }))
    }

    fn load_flow(&self, store: &Store, flow_id: &str) -> Result<Flow, RpcError> {
        not_found(
            "flow_not_found",
            format!("flow {flow_id} not found"),
            store.get_flow(flow_id),
        )
    }

    fn operations_create(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let flow_id = p.require_string("flowID")?;
        let raw = p
            .dict("operation")?
            .ok_or_else(|| RpcError::invalid_params("operation", None))?;
        let mut flow = self.load_flow(&store, &flow_id)?;
        self.ensure_flow_mutable(&flow_id)?;
        let mut op = Self::parse_operation(&raw, &flow_id)?;
        if op.id.is_empty() {
            op.id = new_uuid();
        }
        op.sort_order = flow
            .operations
            .iter()
            .map(|o| o.sort_order)
            .max()
            .unwrap_or(-1)
            + 1;
        op.normalize_action();
        let op_id = op.id.clone();
        flow.operations.push(op.clone());
        store.save_flow(&mut flow)?;
        self.container.sync_engine.sync_flow_schedule(&flow);
        self.emit_state_changed("flows", &flow_id);
        let saved = store.get_flow(&flow_id)?;
        let saved_op = saved
            .operations
            .iter()
            .find(|o| o.id == op_id)
            .cloned()
            .unwrap_or(op);
        Ok(json!({ "flow": json::flow(&saved), "operation": json::operation(&saved_op) }))
    }

    fn operations_update(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let flow_id = p.require_string("flowID")?;
        let op_id = p.require_string("operationID")?;
        let patch = p
            .dict("patch")?
            .ok_or_else(|| RpcError::invalid_params("patch", None))?;
        let mut flow = self.load_flow(&store, &flow_id)?;
        self.ensure_flow_mutable(&flow_id)?;
        let idx = flow
            .operations
            .iter()
            .position(|o| o.id == op_id)
            .ok_or_else(|| {
                RpcError::new(
                    "operation_not_found",
                    format!("operation {op_id} not found"),
                )
            })?;
        let Value::Object(mut merged) = json::operation(&flow.operations[idx]) else {
            unreachable!()
        };
        for (k, v) in patch {
            if !["id", "flowID", "sortOrder"].contains(&k.as_str()) {
                merged.insert(k, v);
            }
        }
        let mut op = Self::parse_operation(&merged, &flow_id)?;
        op.id = op_id.clone();
        op.sort_order = flow.operations[idx].sort_order;
        op.normalize_action();
        flow.operations[idx] = op.clone();
        store.save_flow(&mut flow)?;
        self.container.sync_engine.sync_flow_schedule(&flow);
        self.emit_state_changed("flows", &flow_id);
        let saved = store.get_flow(&flow_id)?;
        let saved_op = saved
            .operations
            .iter()
            .find(|o| o.id == op_id)
            .cloned()
            .unwrap_or(op);
        Ok(json!({ "flow": json::flow(&saved), "operation": json::operation(&saved_op) }))
    }

    fn operations_delete(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let flow_id = p.require_string("flowID")?;
        let op_id = p.require_string("operationID")?;
        let mut flow = self.load_flow(&store, &flow_id)?;
        self.ensure_flow_mutable(&flow_id)?;
        if !flow.operations.iter().any(|o| o.id == op_id) {
            return Err(RpcError::new(
                "operation_not_found",
                format!("operation {op_id} not found"),
            ));
        }
        flow.operations.retain(|o| o.id != op_id);
        for (i, o) in flow.operations.iter_mut().enumerate() {
            o.sort_order = i as i64;
        }
        store.save_flow(&mut flow)?;
        self.container.sync_engine.sync_flow_schedule(&flow);
        self.emit_state_changed("flows", &flow_id);
        Ok(json!({ "flow": json::flow(&store.get_flow(&flow_id)?) }))
    }

    fn operations_reorder(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let flow_id = p.require_string("flowID")?;
        let ids = p.string_array("operationIDs")?.unwrap_or_default();
        let mut flow = self.load_flow(&store, &flow_id)?;
        self.ensure_flow_mutable(&flow_id)?;
        let existing: HashSet<&str> = flow.operations.iter().map(|o| o.id.as_str()).collect();
        let given: HashSet<&str> = ids.iter().map(String::as_str).collect();
        if ids.len() != flow.operations.len() || given.len() != ids.len() || given != existing {
            return Err(RpcError::invalid_params(
                "operationIDs",
                Some("operationIDs must be a permutation of the flow's operations"),
            ));
        }
        let mut reordered: Vec<FlowOperation> = Vec::with_capacity(ids.len());
        for (i, id) in ids.iter().enumerate() {
            let mut op = flow
                .operations
                .iter()
                .find(|o| &o.id == id)
                .cloned()
                .ok_or_else(|| RpcError::invalid_params("operationIDs", None))?;
            op.sort_order = i as i64;
            reordered.push(op);
        }
        flow.operations = reordered;
        store.save_flow(&mut flow)?;
        self.container.sync_engine.sync_flow_schedule(&flow);
        self.emit_state_changed("flows", &flow_id);
        Ok(json!({ "flow": json::flow(&store.get_flow(&flow_id)?) }))
    }

    // ---- profiles / schedules -----------------------------------------

    fn profiles_list(&self) -> Res {
        let store = self.require_store()?;
        Ok(
            json!({ "profiles": store.list_profiles()?.iter().map(json::profile).collect::<Vec<_>>() }),
        )
    }

    fn profiles_get(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let name = p.require_string("name")?;
        let profile = not_found(
            "profile_not_found",
            format!("profile {name} not found"),
            store.get_profile(&name),
        )?;
        Ok(json!({ "profile": json::profile(&profile) }))
    }

    fn profiles_save(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let raw = p
            .dict("profile")?
            .ok_or_else(|| RpcError::invalid_params("profile", None))?;
        let mut profile = Self::parse_profile(&raw)?;
        store.save_profile(&mut profile)?;
        self.emit_state_changed("profiles", &profile.name);
        Ok(json!({ "profile": json::profile(&store.get_profile(&profile.name)?) }))
    }

    fn profiles_delete(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let name = p.require_string("name")?;
        not_found(
            "profile_not_found",
            format!("profile {name} not found"),
            store.delete_profile(&name),
        )?;
        self.emit_state_changed("profiles", &name);
        Ok(json!({ "deleted": true, "name": name }))
    }

    fn schedules_list(&self) -> Res {
        let store = self.require_store()?;
        Ok(
            json!({ "schedules": store.list_schedules()?.iter().map(json::schedule).collect::<Vec<_>>() }),
        )
    }

    // ---- sync ----------------------------------------------------------

    fn sync_profile(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let name = p.require_string("profile")?;
        let mut profile = not_found(
            "profile_not_found",
            format!("profile {name} not found"),
            store.get_profile(&name),
        )?;
        let requested = p.string("action")?.unwrap_or_default();
        let action = if requested.is_empty() {
            profile_direction::normalize(&profile.direction)
        } else {
            if !["pull", "push", "bi", "bi-resync", "dry-run"].contains(&requested.as_str()) {
                return Err(RpcError::new(
                    "invalid_action",
                    format!("unknown sync action: {requested}"),
                ));
            }
            requested
        };
        let dry_run_override = p.bool("dryRun")?.unwrap_or(false);
        let wait = p.bool("wait")?.unwrap_or(false);

        let engine = &self.container.sync_engine;
        let started = if dry_run_override {
            profile.dry_run = true;
            let (from, to, key) = (
                profile.from.clone(),
                profile.to.clone(),
                profile.name.clone(),
            );
            engine.start_path_sync(&action, &key, &from, &to, Some(profile))
        } else {
            engine.start_sync(&action, &profile.name)
        };
        let task_id = started.map_err(RpcError::from)?;

        if wait {
            match engine.wait_task(&task_id, &|| false) {
                Ok(()) => {}
                Err(SyncEngineError::TaskFailed(msg)) => {
                    return Err(RpcError::new("task_failed", sanitize(&msg))
                        .with_data(json!({ "taskID": task_id })));
                }
                Err(SyncEngineError::TaskCancelled) => {
                    return Err(RpcError::new("task_cancelled", "task was cancelled")
                        .with_data(json!({ "taskID": task_id })));
                }
                Err(e) => return Err(e.into()),
            }
            return Ok(
                json!({ "taskID": task_id, "profile": name, "action": action, "state": "completed" }),
            );
        }
        Ok(json!({ "taskID": task_id, "profile": name, "action": action, "state": "running" }))
    }

    fn sync_stop(&self, p: &Params) -> Res {
        let task_id = p.require_string("taskID")?;
        match self.container.sync_engine.stop_sync(&task_id) {
            Ok(()) => {}
            Err(SyncEngineError::TaskNotFound) => {
                return Err(RpcError::new(
                    "task_not_found",
                    format!("task {task_id} not found"),
                ));
            }
            Err(e) => return Err(e.into()),
        }
        Ok(json!({ "cancelled": true, "taskID": task_id }))
    }

    // ---- boards --------------------------------------------------------

    fn boards_list(&self) -> Res {
        let store = self.require_store()?;
        let boards: Vec<Value> = store
            .list_boards()?
            .iter()
            .map(|b| json!({ "id": b.id, "name": b.name, "createdAt": b.created_at, "updatedAt": b.updated_at }))
            .collect();
        Ok(json!({ "boards": boards }))
    }

    fn boards_get(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let id = p.require_string("id")?;
        let board = not_found(
            "board_not_found",
            format!("board {id} not found"),
            store.load_board_graph(&id),
        )?;
        Ok(json!({ "board": json::board(&board) }))
    }

    fn boards_save(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let raw = p
            .dict("board")?
            .ok_or_else(|| RpcError::invalid_params("board", None))?;
        let mut board = Self::parse_board(&raw)?;
        let blank = |s: &str| s.trim_matches([' ', '\t']).is_empty();
        if blank(&board.id) {
            return Err(RpcError::invalid_params("board.id", None));
        }
        if blank(&board.name) {
            return Err(RpcError::invalid_params("board.name", None));
        }
        for (i, n) in board.nodes.iter().enumerate() {
            if n.id.is_empty() {
                return Err(RpcError::invalid_params(
                    &format!("board.nodes[{i}].id"),
                    None,
                ));
            }
        }
        for e in board.edges.iter_mut() {
            if e.id.is_empty() {
                e.id = new_uuid();
            }
        }
        store.save_board_graph(&board)?;
        Ok(json!({ "board": json::board(&store.load_board_graph(&board.id)?) }))
    }

    fn boards_delete(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let id = p.require_string("id")?;
        not_found(
            "board_not_found",
            format!("board {id} not found"),
            store.delete_board(&id),
        )?;
        Ok(json!({ "deleted": true, "id": id }))
    }

    fn board_exists(&self, store: &Store, id: &str) -> Result<(), RpcError> {
        not_found(
            "board_not_found",
            format!("board {id} not found"),
            store.get_board(id),
        )
        .map(|_| ())
    }

    fn boards_execute(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let id = p.require_string("id")?;
        self.board_exists(&store, &id)?;
        let stop_on_error = p.bool("stopOnError")?.unwrap_or(true);
        let run_id = self.container.board_engine.execute(&id, stop_on_error)?;
        Ok(json!({ "runID": run_id, "boardID": id, "status": "running" }))
    }

    fn boards_stop(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let id = p.require_string("id")?;
        self.board_exists(&store, &id)?;
        self.container.board_engine.stop(&id)?;
        Ok(json!({ "boardID": id, "status": "cancelling" }))
    }

    fn boards_status(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let id = p.require_string("id")?;
        self.board_exists(&store, &id)?;
        match self.container.board_engine.status(&id) {
            None => Ok(json!({ "run": null })),
            Some(st) => Ok(json!({ "run": {
                "runID": st.run_id, "boardID": st.board_id, "status": st.status, "error": st.error,
            } })),
        }
    }

    // ---- history -------------------------------------------------------

    fn history_list(&self, p: &Params) -> Res {
        let store = self.require_store()?;
        let mut limit = p.int("limit")?.unwrap_or(20);
        if limit < 0 {
            return Err(RpcError::invalid_params("limit", None));
        }
        limit = limit.min(1000);
        let offset = p.int("offset")?.unwrap_or(0);
        if offset < 0 {
            return Err(RpcError::invalid_params("offset", None));
        }
        let entries = match p.string("profile")? {
            Some(profile) if !profile.is_empty() => {
                store.list_history_for_profile(&profile, limit, offset)?
            }
            _ => store.list_history(limit, offset)?,
        };
        Ok(
            json!({ "entries": entries.iter().map(json::history).collect::<Vec<_>>(), "total": null }),
        )
    }

    fn history_stats(&self) -> Res {
        let store = self.require_store()?;
        Ok(json::history_stats(&store.history_stats()?))
    }

    fn history_clear(&self) -> Res {
        let store = self.require_store()?;
        store.clear_history()?;
        Ok(json!({ "cleared": true }))
    }

    // ---- settings ------------------------------------------------------

    fn settings_set(&self, p: &Params) -> Res {
        let patch = p
            .dict("settings")?
            .ok_or_else(|| RpcError::invalid_params("settings", None))?;
        let mut s = self.container.auth.app_settings();
        if let Some(v) = Self::optional_bool(&patch, "notificationsEnabled")? {
            s.notifications_enabled = v;
        }
        if let Some(v) = Self::optional_bool(&patch, "debugMode")? {
            s.debug_mode = v;
        }
        if let Some(v) = Self::optional_bool(&patch, "minimizeToTray")? {
            s.minimize_to_tray = v;
        }
        if let Some(v) = Self::optional_bool(&patch, "startAtLogin")? {
            s.start_at_login = v;
        }
        self.container.auth.set_app_settings(s.clone())?;
        Ok(json!({ "settings": json::app_settings(&s) }))
    }

    fn optional_bool(d: &Map<String, Value>, key: &str) -> Result<Option<bool>, RpcError> {
        match d.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => json::bool_value(v)
                .map(Some)
                .ok_or_else(|| RpcError::invalid_params(&format!("settings.{key}"), None)),
        }
    }

    // ---- plumbing (shared with handlers_remotes.rs) --------------------

    pub(crate) fn require_store(&self) -> Result<Arc<Store>, RpcError> {
        self.container.store().ok_or_else(RpcError::locked)
    }

    pub(crate) fn require_rclone(&self) -> Result<Arc<RcloneClient>, RpcError> {
        self.container.rclone().ok_or_else(RpcError::locked)
    }

    pub(crate) fn resolved_rclone_binary(&self) -> Result<String, RpcError> {
        if let Some(rc) = self.container.rclone() {
            return Ok(rc.binary.clone());
        }
        let ov = if self.rclone_binary.is_empty() {
            None
        } else {
            Some(self.rclone_binary.as_str())
        };
        RcloneClient::resolve_binary(ov).map_err(RpcError::from)
    }

    pub(crate) fn admin_rclone(&self) -> Result<Arc<RcloneClient>, RpcError> {
        if let Some(rc) = self.container.rclone() {
            return Ok(rc);
        }
        let cfg = join_path(&self.container.config.config_dir, "rclone.conf");
        let ov = if self.rclone_binary.is_empty() {
            None
        } else {
            Some(self.rclone_binary.as_str())
        };
        RcloneClient::new(ov, &cfg, &self.container.log)
            .map(Arc::new)
            .map_err(RpcError::from)
    }

    pub(crate) fn oauth(&self) -> Result<Arc<OAuthManager>, RpcError> {
        if let Some(m) = self.oauth_manager.lock().clone() {
            return Ok(m);
        }
        let bin = self.resolved_rclone_binary()?;
        let cfg = join_path(&self.container.config.config_dir, "rclone.conf");
        let spawner: Arc<dyn OAuthSpawner> = self
            .oauth_spawner
            .lock()
            .clone()
            .unwrap_or_else(|| Arc::new(RealOAuthSpawner));
        let bridge = self.bridge.clone();
        let manager = Arc::new(OAuthManager::new(
            &bin,
            &cfg,
            self.container.log.clone(),
            spawner,
            Arc::new(move |name: &str, data: Value| bridge.push_event(name, data)),
        ));
        let mut slot = self.oauth_manager.lock();
        if let Some(existing) = slot.clone() {
            return Ok(existing);
        }
        *slot = Some(manager.clone());
        Ok(manager)
    }

    pub(crate) fn remote_exists(&self, rc: &RcloneClient, name: &str) -> Result<bool, RpcError> {
        Ok(rc.list_remotes()?.iter().any(|r| r.name == name))
    }

    pub(crate) fn ensure_flow_mutable(&self, id: &str) -> Result<(), RpcError> {
        if self.container.flow_engine.is_running(id) {
            return Err(self.flow_busy(id));
        }
        let snap = self.container.runtime.snapshot();
        if let Some(f) = snap.flows.iter().find(|f| f.id == id)
            && (f.status == "running" || f.status == "cancelling")
        {
            return Err(self.flow_busy(id));
        }
        Ok(())
    }

    pub(crate) fn flow_busy(&self, id: &str) -> RpcError {
        RpcError::new("flow_busy", format!("flow {id} is running — stop it first"))
    }

    pub(crate) fn emit_state_changed(&self, domain: &str, id: &str) {
        self.container
            .bus
            .publish(BusEvent::state_changed(domain, id));
    }

    pub(crate) fn provider_cache(&self) -> &Mutex<Option<Vec<Map<String, Value>>>> {
        &self.provider_cache
    }

    pub(crate) fn launchd(&self) -> Arc<LaunchdService> {
        self.launchd.lock().clone()
    }

    pub(crate) fn update_fetcher(&self) -> Arc<dyn HttpFetcher> {
        self.update_fetcher.lock().clone()
    }

    pub(crate) fn update_api_base(&self) -> Option<String> {
        self.update_api_base.lock().clone()
    }

    /// True when running from inside a macOS `.app` bundle.
    pub(crate) fn bundled_app(&self) -> bool {
        std::env::current_exe()
            .map(|p| {
                p.ancestors()
                    .any(|a| a.extension().is_some_and(|e| e == "app"))
            })
            .unwrap_or(false)
    }

    // ---- parsing -------------------------------------------------------

    fn parse_flow(d: &Map<String, Value>) -> Result<Flow, RpcError> {
        let p = Params(d.clone());
        let id = p.string("id")?.unwrap_or_default();
        let canvas_json = match p.string("canvasJSON")? {
            Some(c) if !c.is_empty() => c,
            _ => p
                .dict("canvas")?
                .map(|c| json::stringify_json(&c))
                .unwrap_or_else(|| "{}".into()),
        };
        let mut operations = Vec::new();
        if let Some(ops) = p.array("operations")? {
            for raw in ops {
                let Value::Object(od) = raw else {
                    return Err(RpcError::invalid_params("operations", None));
                };
                operations.push(Self::parse_operation(&od, &id)?);
            }
        }
        Ok(Flow {
            id,
            name: p.string("name")?.unwrap_or_default(),
            is_collapsed: p.bool("isCollapsed")?.unwrap_or(false),
            schedule_enabled: p.bool("scheduleEnabled")?.unwrap_or(false),
            schedule_cron: p.string("scheduleCron")?.unwrap_or_default(),
            sort_order: p.int("sortOrder")?.unwrap_or(0),
            canvas_json,
            operations,
            ..Flow::default()
        })
    }

    fn parse_operation(d: &Map<String, Value>, flow_id: &str) -> Result<FlowOperation, RpcError> {
        let p = Params(d.clone());
        let mut sync_config = Map::new();
        if let Some(cfg) = p.dict("syncConfig")? {
            sync_config = cfg;
        } else if let Some(s) = p.string("syncConfig")?
            && let Some(parsed) = json::parse_object(&s)
        {
            sync_config = parsed;
        }
        let mut op = FlowOperation {
            id: p.string("id")?.unwrap_or_default(),
            flow_id: flow_id.to_string(),
            source_remote: p.string("sourceRemote")?.unwrap_or_default(),
            source_path: p.string("sourcePath")?.unwrap_or_else(|| "/".into()),
            target_remote: p.string("targetRemote")?.unwrap_or_default(),
            target_path: p.string("targetPath")?.unwrap_or_else(|| "/".into()),
            action: p.string("action")?.unwrap_or_default(),
            sync_config,
            is_expanded: p.bool("isExpanded")?.unwrap_or(false),
            sort_order: p.int("sortOrder")?.unwrap_or(0),
        };
        op.normalize_action();
        Ok(op)
    }

    /// Overlay known keys of `d` onto a default profile (unknown keys ignored,
    /// exactly like the Swift merge).
    fn parse_profile(d: &Map<String, Value>) -> Result<Profile, RpcError> {
        let Value::Object(mut merged) =
            serde_json::to_value(Profile::default()).unwrap_or(Value::Null)
        else {
            return Err(RpcError::new("invalid_params", "invalid profile"));
        };
        for (k, v) in d {
            if merged.contains_key(k) {
                merged.insert(k.clone(), v.clone());
            }
        }
        serde_json::from_value::<Profile>(Value::Object(merged))
            .map_err(|e| RpcError::new("invalid_params", format!("invalid profile: {e}")))
    }

    fn parse_board(d: &Map<String, Value>) -> Result<Board, RpcError> {
        let p = Params(d.clone());
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        if let Some(raw_nodes) = p.array("nodes")? {
            for raw in raw_nodes {
                let Value::Object(nd) = raw else {
                    return Err(RpcError::invalid_params("nodes", None));
                };
                let np = Params(nd);
                let coord = |k: &str| np.any(k).and_then(json::double_value).unwrap_or(0.0);
                nodes.push(BoardNode {
                    id: np.string("id")?.unwrap_or_default(),
                    remote_name: np.string("remoteName")?.unwrap_or_default(),
                    path: np.string("path")?.unwrap_or_default(),
                    label: np.string("label")?.unwrap_or_default(),
                    x: coord("x"),
                    y: coord("y"),
                });
            }
        }
        if let Some(raw_edges) = p.array("edges")? {
            for raw in raw_edges {
                let Value::Object(ed) = raw else {
                    return Err(RpcError::invalid_params("edges", None));
                };
                let ep = Params(ed);
                let sync_config = if let Some(cfg) = ep.dict("syncConfig")? {
                    json::stringify_json(&cfg)
                } else {
                    match ep.string("syncConfig")? {
                        Some(s) if !s.is_empty() => s,
                        _ => "{}".into(),
                    }
                };
                edges.push(BoardEdge {
                    id: ep.string("id")?.unwrap_or_default(),
                    source_id: ep.string("sourceID")?.unwrap_or_default(),
                    target_id: ep.string("targetID")?.unwrap_or_default(),
                    action: ep.string("action")?.unwrap_or_default(),
                    sync_config,
                });
            }
        }
        Ok(Board {
            id: p.string("id")?.unwrap_or_default(),
            name: p.string("name")?.unwrap_or_default(),
            nodes,
            edges,
            ..Board::default()
        })
    }
}
