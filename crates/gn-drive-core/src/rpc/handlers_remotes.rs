//! remotes.*, remotes.oauth.*, fs.list, service.*, update.* handlers
//! (port of the matching sections of `RPCDispatcher.swift`).

use super::dispatcher::Dispatcher;
use super::error::stderr_field;
use super::{Params, RpcError, json, map_error, redact};
use crate::launchd::{LaunchdService, ServiceScope, ServiceSpec, ServiceStatus};
use crate::rclone::client::RcloneClient;
use crate::rclone::{ProcessOutput, RcloneError};
use crate::self_update::{self, UpdateOptions};
use serde_json::{Map, Value, json};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
}

/// Last JSON-object line of `stdout` that carries `Option` or `State`
/// (rclone `--non-interactive` question).
fn question(stdout: &str) -> Option<Map<String, Value>> {
    for line in stdout.split('\n').rev() {
        let text = line.trim_matches(|c| c == ' ' || c == '\t');
        if !text.starts_with('{') {
            continue;
        }
        let Some(obj) = json::parse_object(text) else {
            continue;
        };
        if json::field(&obj, "Option").is_some() || json::field(&obj, "State").is_some() {
            return Some(obj);
        }
    }
    None
}

fn run_failed_stderr(e: &RcloneError) -> String {
    match e {
        RcloneError::RunFailed { stderr, .. } => stderr.clone(),
        other => other.to_string(),
    }
}

fn is_null(v: Option<&Value>) -> bool {
    matches!(v, None | Some(Value::Null))
}

impl Dispatcher {
    // ---- Remotes ----

    pub(crate) fn remotes_providers(&self, p: &Params) -> Result<Value, RpcError> {
        let refresh = p.bool("refresh")?.unwrap_or(false);
        let cached = self.provider_cache().lock().clone();
        let from_cache = !refresh && cached.is_some();
        let raw = match cached {
            Some(c) if from_cache => c,
            _ => {
                let rc = self.admin_rclone()?;
                let raw = rc.providers().map_err(|e| map_error(&e))?;
                *self.provider_cache().lock() = Some(raw.clone());
                raw
            }
        };
        let version = self
            .admin_rclone()
            .ok()
            .and_then(|rc| rc.version().ok())
            .map(Value::String)
            .unwrap_or(Value::Null);
        Ok(json!({
            "rcloneVersion": version,
            "providers": raw.iter().map(json::provider).collect::<Vec<_>>(),
            "cached": from_cache,
        }))
    }

    pub(crate) fn remotes_list(&self) -> Result<Value, RpcError> {
        let rc = self.require_rclone()?;
        let remotes = rc.list_remotes().map_err(|e| map_error(&e))?;
        Ok(json!({ "remotes": remotes.iter().map(json::remote).collect::<Vec<_>>() }))
    }

    pub(crate) fn remotes_add(&self, p: &Params) -> Result<Value, RpcError> {
        let rc = self.require_rclone()?;
        let name = p.require_string("name")?;
        let r#type = p.require_string("type")?;
        let kvs = p.config_kvs("config")?;
        if self.remote_exists(&rc, &name)? {
            return Err(RpcError::new(
                "remote_exists",
                format!("remote {name} already exists"),
            ));
        }

        let res = rc
            .create_remote_non_interactive(&name, &r#type, &kvs)
            .map_err(|e| map_error(&e))?;
        if let Some(q) = question(&res.stdout) {
            let state = json::string_of(json::field(&q, "State"));
            if !is_null(json::field(&q, "Option")) {
                let _ = rc.delete_remote(&name);
                let normalized = json::normalized_question(&q);
                if state.to_lowercase().contains("oauth") {
                    return Err(RpcError::new(
                        "oauth_required",
                        format!("remote {name} requires OAuth authorization"),
                    )
                    .with_data(json!({"state": state, "questions": [normalized]})));
                }
                return Err(RpcError::new(
                    "invalid_params",
                    format!("remote {name} requires additional options"),
                )
                .with_data(json!({"field": "config", "question": normalized})));
            }
        }
        if res.exit_code != 0 {
            let _ = rc.delete_remote(&name);
            let msg = if res.stderr.is_empty() {
                &res.stdout
            } else {
                &res.stderr
            };
            let field = stderr_field(msg);
            return Err(RpcError::new(
                "rclone_failed",
                format!("rclone config create failed: {field}"),
            )
            .with_data(json!({"command": "config create", "stderr": field})));
        }
        if let Err(e) = rc.test_remote(&name) {
            let _ = rc.delete_remote(&name);
            let field = stderr_field(&run_failed_stderr(&e));
            return Err(RpcError::new(
                "auth_failed",
                format!("remote {name} could not be verified: {field}"),
            )
            .with_data(json!({"stderr": field})));
        }
        self.emit_state_changed("remotes", &name);
        Ok(json!({
            "remote": json::remote(&crate::rclone::Remote { name, r#type, description: String::new() }),
            "verified": true,
        }))
    }

    pub(crate) fn remotes_test(&self, p: &Params) -> Result<Value, RpcError> {
        let rc = self.require_rclone()?;
        let name = p.require_string("name")?;
        if !self.remote_exists(&rc, &name)? {
            return Err(RpcError::new(
                "remote_not_found",
                format!("remote {name} not found"),
            ));
        }
        match rc.test_remote(&name) {
            Ok(()) => {}
            Err(RcloneError::RunFailed { stderr, .. }) => {
                let field = stderr_field(&stderr);
                return Err(
                    RpcError::new("test_failed", format!("remote {name} failed: {field}"))
                        .with_data(json!({"stderr": field})),
                );
            }
            Err(e) => return Err(map_error(&e)),
        }
        Ok(json!({"name": name, "ok": true}))
    }

    pub(crate) fn remotes_delete(&self, p: &Params) -> Result<Value, RpcError> {
        let rc = self.require_rclone()?;
        let name = p.require_string("name")?;
        if !self.remote_exists(&rc, &name)? {
            return Err(RpcError::new(
                "remote_not_found",
                format!("remote {name} not found"),
            ));
        }
        rc.delete_remote(&name).map_err(|e| map_error(&e))?;
        self.emit_state_changed("remotes", &name);
        Ok(json!({"deleted": true, "name": name}))
    }

    // ---- OAuth ----

    pub(crate) fn oauth_start(&self, p: &Params) -> Result<Value, RpcError> {
        let r#type = p.require_string("type")?;
        let client_id = p.string("clientID")?;
        let client_secret = p.string("clientSecret")?;
        if client_id.is_some() != client_secret.is_some() {
            return Err(RpcError::invalid_params(
                "clientID",
                Some("clientID and clientSecret must be supplied together"),
            ));
        }
        let manager = self.oauth()?;
        let session = manager.start(&r#type, client_id.as_deref(), client_secret.as_deref())?;
        let expires = session.started_at
            + chrono::Duration::from_std(manager.session_lifetime())
                .unwrap_or_else(|_| chrono::Duration::minutes(10));
        Ok(json!({
            "sessionID": session.id,
            "type": r#type,
            "expiresAt": json::rfc3339(expires),
            "authURL": session.auth_url().map(Value::String).unwrap_or(Value::Null),
        }))
    }

    pub(crate) fn oauth_complete(&self, p: &Params) -> Result<Value, RpcError> {
        let rc = self.require_rclone()?;
        let session_id = p.require_string("sessionID")?;
        let name = p.require_string("name")?;
        let config = p.dict("config")?.unwrap_or_default();
        let manager = self.oauth()?;
        let Some(session) = manager.session(&session_id) else {
            return Err(RpcError::new(
                "oauth_session_not_found",
                format!("OAuth session {session_id} not found"),
            ));
        };
        if self.remote_exists(&rc, &name)? {
            return Err(RpcError::new(
                "remote_exists",
                format!("remote {name} already exists"),
            ));
        }

        // Wait briefly for the authorize child to hand over its token.
        session.wait_not_waiting(std::time::Duration::from_secs(5));
        let snap = session.snapshot();
        match snap.status.as_str() {
            "cancelled" => {
                return Err(RpcError::new(
                    "oauth_cancelled",
                    format!("OAuth session {session_id} was cancelled"),
                ));
            }
            "failed" => {
                return Err(RpcError::new(
                    "oauth_failed",
                    if snap.error.is_empty() {
                        "OAuth authorization failed".to_string()
                    } else {
                        snap.error
                    },
                ));
            }
            "authorized" => {}
            _ => {
                return Err(RpcError::new(
                    "oauth_cancelled",
                    "OAuth authorization has not completed yet",
                ));
            }
        }
        let token = snap.token;
        if token.is_empty() {
            return Err(RpcError::new(
                "oauth_failed",
                format!("OAuth session {session_id} has no token"),
            ));
        }

        let r#type = p.string("type")?.unwrap_or_else(|| session.r#type.clone());
        let mut keys: Vec<&String> = config.keys().collect();
        keys.sort();
        let kvs: Vec<String> = keys
            .into_iter()
            .map(|k| format!("{k}={}", json::string_of(config.get(k))))
            .collect();

        let cleanup = || {
            let _ = rc.delete_remote(&name);
        };

        let created = self.oauth_create_loop(&rc, &name, &r#type, &kvs, &token, &config, &cleanup);
        if let Err(e) = created {
            manager.forget(&session_id);
            return Err(e);
        }

        if let Err(e) = rc.test_remote(&name) {
            cleanup();
            manager.forget(&session_id);
            let field = stderr_field(&run_failed_stderr(&e));
            return Err(RpcError::new(
                "test_failed",
                format!("remote {name} could not be verified: {field}"),
            )
            .with_data(json!({"stderr": field})));
        }
        manager.forget(&session_id);
        self.emit_state_changed("remotes", &name);
        Ok(json!({
            "remote": json::remote(&crate::rclone::Remote { name, r#type, description: String::new() }),
            "verified": true,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn oauth_create_loop(
        &self,
        rc: &RcloneClient,
        name: &str,
        r#type: &str,
        kvs: &[String],
        token: &str,
        config: &Map<String, Value>,
        cleanup: &dyn Fn(),
    ) -> Result<(), RpcError> {
        let rclone_err = |e: RcloneError| {
            cleanup();
            RpcError::new("oauth_failed", redact::sanitize(&e.to_string()))
        };
        let mut current: ProcessOutput = rc
            .create_remote_non_interactive(name, r#type, kvs)
            .map_err(rclone_err)?;
        let mut steps = 0;
        loop {
            let Some(q) = question(&current.stdout) else {
                if current.exit_code != 0 {
                    cleanup();
                    return Err(RpcError::new(
                        "oauth_failed",
                        format!("remote creation failed: {}", stderr_field(&current.stderr)),
                    ));
                }
                return Ok(());
            };
            let state = json::string_of(json::field(&q, "State"));
            let Some(Value::Object(option)) = json::field(&q, "Option") else {
                if state.is_empty() && current.exit_code == 0 {
                    return Ok(());
                }
                cleanup();
                return Err(RpcError::new(
                    "oauth_failed",
                    format!("remote creation stopped at state {state}"),
                )
                .with_data(json!({"question": json::normalized_question(&q)})));
            };
            steps += 1;
            if steps > 8 {
                cleanup();
                return Err(RpcError::new(
                    "oauth_failed",
                    "too many configuration steps",
                ));
            }
            let option_name = json::string_of(json::field(option, "Name"));
            let answer = if option_name == "config_token"
                || (state.to_lowercase().contains("oauth-authorize")
                    && option_name.contains("token"))
            {
                token.to_string()
            } else if option_name == "config_shared_client_id" {
                "true".to_string()
            } else if option_name == "config_is_local" {
                "false".to_string()
            } else if let Some(v) = config.get(&option_name) {
                json::string_of(Some(v))
            } else {
                cleanup();
                return Err(RpcError::new(
                    "oauth_failed",
                    format!("unexpected configuration question: {option_name}"),
                )
                .with_data(json!({"question": json::normalized_question(&q)})));
            };
            current = rc
                .continue_remote_create(name, r#type, &state, &answer, kvs)
                .map_err(rclone_err)?;
            if current.exit_code != 0 {
                cleanup();
                return Err(RpcError::new(
                    "oauth_failed",
                    format!("remote creation failed: {}", stderr_field(&current.stderr)),
                ));
            }
        }
    }

    pub(crate) fn oauth_cancel(&self, p: &Params) -> Result<Value, RpcError> {
        let session_id = p.require_string("sessionID")?;
        let manager = self.oauth()?;
        if manager.session(&session_id).is_none() {
            return Err(RpcError::new(
                "oauth_session_not_found",
                format!("OAuth session {session_id} not found"),
            ));
        }
        manager.cancel(&session_id);
        manager.forget(&session_id);
        Ok(json!({"cancelled": true, "sessionID": session_id}))
    }

    // ---- fs.list ----

    pub(crate) fn fs_list(&self, p: &Params) -> Result<Value, RpcError> {
        let rc = self.require_rclone()?;
        let path = p.require_string("path")?;
        let recursive = p.bool("recursive")?.unwrap_or(false);
        let mut limit = p.int("limit")?.unwrap_or(150);
        if limit < 0 {
            return Err(RpcError::invalid_params("limit", None));
        }
        limit = limit.min(5000);
        if !(path.contains(':') || path.starts_with('/')) {
            return Err(RpcError::new(
                "fs_missing_path",
                "path must be absolute or remote:path",
            ));
        }
        let limit = limit as usize;
        let listed = if recursive {
            rc.list_files_for_pending(&path, limit + 1)
        } else {
            rc.list_files(&path)
        };
        match listed {
            Ok(mut entries) => {
                let mut truncated = false;
                if entries.len() > limit {
                    entries.truncate(limit);
                    truncated = true;
                }
                Ok(json!({
                    "path": path,
                    "entries": entries.iter().map(json::file_entry).collect::<Vec<_>>(),
                    "truncated": truncated,
                }))
            }
            Err(RcloneError::RunFailed { stderr, .. }) => {
                let field = stderr_field(&stderr);
                Err(
                    RpcError::new("browse_failed", format!("could not list {path}: {field}"))
                        .with_data(json!({"path": path, "stderr": field})),
                )
            }
            Err(e) => Err(map_error(&e)),
        }
    }

    // ---- Service ----

    fn service_spec(&self, p: &Params) -> Result<ServiceSpec, RpcError> {
        let scope_raw = p.string("scope")?.unwrap_or_else(|| "user".to_string());
        let Some(scope) = ServiceScope::parse(&scope_raw) else {
            return Err(RpcError::invalid_params("scope", None));
        };
        let exec_path = match p.string("execPath")? {
            Some(e) => e,
            None => std::env::current_exe()
                .ok()
                .map(|e| e.to_string_lossy().into_owned())
                .or_else(|| std::env::args().next())
                .unwrap_or_default(),
        };
        Ok(ServiceSpec {
            scope,
            config_dir: self.container().config.config_dir.clone(),
            exec_path,
            ..Default::default()
        })
    }

    fn service_status_data(
        spec: &ServiceSpec,
        mgr: &LaunchdService,
        st: Option<&ServiceStatus>,
    ) -> Value {
        let mut out = json!({
            "installed": mgr.is_installed(spec),
            "running": st.map(|s| s.running).unwrap_or(false),
            "mode": st.map(|s| s.mode.clone()).unwrap_or_else(|| "service".to_string()),
            "scope": spec.scope.as_str(),
            "label": LaunchdService::LABEL,
            "plistPath": mgr.plist_path(spec),
        });
        if let Some(s) = st
            && s.pid > 0
        {
            out["pid"] = Value::from(s.pid);
        }
        out
    }

    pub(crate) fn service_status(&self, p: &Params) -> Result<Value, RpcError> {
        let spec = self.service_spec(p)?;
        let mgr = self.launchd();
        if !mgr.is_installed(&spec) {
            return Ok(Self::service_status_data(&spec, &mgr, None));
        }
        let st = mgr.status(&spec).map_err(|e| map_error(&e))?;
        Ok(Self::service_status_data(&spec, &mgr, Some(&st)))
    }

    pub(crate) fn service_install(&self, p: &Params) -> Result<Value, RpcError> {
        let spec = self.service_spec(p)?;
        // SAFETY: getuid never fails.
        if spec.scope == ServiceScope::System && unsafe { libc::getuid() } != 0 {
            return Err(RpcError::new(
                "permission_denied",
                "system-scope install requires root (sudo)",
            ));
        }
        let mgr = self.launchd();
        mgr.install(&spec).map_err(|e| map_error(&e))?;
        let st = mgr.status(&spec).ok();
        Ok(Self::service_status_data(&spec, &mgr, st.as_ref()))
    }

    pub(crate) fn service_uninstall(&self, p: &Params) -> Result<Value, RpcError> {
        let spec = self.service_spec(p)?;
        self.launchd().uninstall(&spec).map_err(|e| map_error(&e))?;
        Ok(json!({"installed": false, "running": false, "scope": spec.scope.as_str()}))
    }

    pub(crate) fn service_action(
        &self,
        p: &Params,
        action: ServiceAction,
    ) -> Result<Value, RpcError> {
        let spec = self.service_spec(p)?;
        let mgr = self.launchd();
        match action {
            ServiceAction::Start => mgr.start(&spec),
            ServiceAction::Stop => mgr.stop(&spec),
            ServiceAction::Restart => mgr.restart(&spec),
        }
        .map_err(|e| map_error(&e))?;
        let st = mgr.status(&spec).ok();
        Ok(Self::service_status_data(&spec, &mgr, st.as_ref()))
    }

    pub(crate) fn service_health(&self) -> Value {
        let mut v = crate::health::rpc_health_json(&self.container().config.config_dir);
        if v.get("present") == Some(&Value::Bool(true)) {
            // Swift reports 0: the RPC server has no web UI.
            v["webPort"] = Value::from(0);
        }
        v
    }

    // ---- Update ----

    fn update_options(&self, p: &Params, apply: bool) -> Result<UpdateOptions, RpcError> {
        let mut opts = UpdateOptions::default();
        if let Some(o) = p.string("repoOwner")? {
            opts.repo_owner = o;
        }
        if let Some(r) = p.string("repo")? {
            opts.repo_name = r;
        }
        opts.current_version = p
            .string("currentVersion")?
            .unwrap_or_else(|| self.container().version.clone());
        if apply {
            opts.force = p.bool("force")?.unwrap_or(false);
        }
        let log = self.container().log.clone();
        opts.log = Arc::new(move |msg| log.info(&format!("update: {msg}"), &[]));
        opts.fetcher = self.update_fetcher();
        if let Some(base) = self.update_api_base() {
            opts.api_base = base;
        }
        Ok(opts)
    }

    pub(crate) fn update_check(&self, p: &Params) -> Result<Value, RpcError> {
        let opts = self.update_options(p, false)?;
        let (current, latest) = self_update::check(&opts).map_err(|e| map_error(&e))?;
        Ok(json!({
            "current": current,
            "updateAvailable": current != latest,
            "latest": latest,
            "checkedAt": json::rfc3339(chrono::Utc::now()),
        }))
    }

    pub(crate) fn update_apply(&self, p: &Params) -> Result<Value, RpcError> {
        if self.bundled_app() {
            return Err(RpcError::new(
                "unsupported",
                "self-update is not supported inside the app bundle",
            )
            .with_data(json!({"reason": "bundled_app"})));
        }
        let opts = self.update_options(p, true)?;
        let r = self_update::update(&opts).map_err(|e| map_error(&e))?;
        Ok(json!({
            "updated": r.updated,
            "oldVersion": r.old_version,
            "newVersion": r.new_version,
            "binaryPath": r.binary_path,
            "restartHint": r.restart_hint,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::{AppContainer, ContainerOptions};
    use crate::keychain::MemorySecureStore;
    use crate::logger::Logger;
    use crate::rclone::StreamKind;
    use crate::rclone::testing::{FakeRunner, Response};
    use crate::rpc::Options;
    use crate::rpc::oauth::testing::{FakeSpawner, Script, TOKEN_OUTPUT, URL_OUTPUT};
    use crate::self_update::{HttpFetcher, HttpResponse};
    use parking_lot::Mutex;
    use std::time::Duration;

    struct Fx {
        _dir: tempfile::TempDir,
        dir: String,
        fake: Arc<FakeRunner>,
        d: Arc<Dispatcher>,
    }

    fn fx() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        let fake = Arc::new(FakeRunner::new());
        let opts = ContainerOptions {
            config_dir: path.clone(),
            portal_mode: true,
            rclone_runner: Some(fake.clone()),
            key_store: Some(Arc::new(MemorySecureStore::new())),
            logger: Some(Logger::quiet()),
            version: "1.0.0".into(),
            ..Default::default()
        };
        let c = AppContainer::new(opts).unwrap();
        let d = Dispatcher::start(
            c,
            "/fake/rclone",
            Options {
                exit_on_shutdown: false,
                ..Options::default()
            },
        );
        Fx {
            _dir: dir,
            dir: path,
            fake,
            d,
        }
    }

    fn err(r: Result<Value, RpcError>) -> RpcError {
        r.expect_err("expected an error")
    }

    fn calls_with(f: &FakeRunner, a: &str, b: &str) -> Vec<Vec<String>> {
        f.calls()
            .into_iter()
            .filter(|c| {
                c.first().map(String::as_str) == Some(a) && c.get(1).map(String::as_str) == Some(b)
            })
            .collect()
    }

    // ---- remotes ----

    #[test]
    fn question_parsing_picks_last_json_line_with_option_or_state() {
        let out = "noise\n{\"x\":1}\n{\"State\":\"s1\",\"Option\":null}\n  {\"State\":\"s2\",\"Option\":{\"Name\":\"a\"}}  \n";
        let q = question(out).unwrap();
        assert_eq!(json::string_of(json::field(&q, "State")), "s2");
        assert!(question("plain\n{\"x\":1}").is_none());
        assert!(question("").is_none());
    }

    #[test]
    fn providers_cached_flag_and_oauth_detection() {
        let f = fx();
        f.fake.on(
            &["config", "providers"],
            Response::new().stdout(r#"[{"Name":"drive","Description":"Google Drive","Prefix":"drive","Options":[{"Name":"auth_url"},{"Name":"client_id"}]},{"Name":"local","Description":"Local","Prefix":"local","Options":[]}]"#),
        );
        f.fake
            .on(&["version"], Response::new().stdout("rclone v1.70.0"));
        let a = f.d.handle("remotes.providers", json!({})).unwrap();
        assert_eq!(a["cached"], false);
        assert_eq!(a["providers"][0]["name"], "drive");
        assert_eq!(a["providers"][0]["oauth"], true);
        assert_eq!(a["providers"][1]["oauth"], false);
        assert!(a["rcloneVersion"].is_string());
        let b = f.d.handle("remotes.providers", json!({})).unwrap();
        assert_eq!(b["cached"], true);
        assert_eq!(f.fake.calls_for("providers").len(), 1);
        let c =
            f.d.handle("remotes.providers", json!({"refresh": true}))
                .unwrap();
        assert_eq!(c["cached"], false);
        assert_eq!(f.fake.calls_for("providers").len(), 2);
    }

    #[test]
    fn remotes_list_shape() {
        let f = fx();
        f.fake.on(
            &["listremotes"],
            Response::new().stdout("gdrive:").stdout("loc:"),
        );
        f.fake.on(
            &["config", "dump"],
            Response::new().stdout(r#"{"gdrive":{"type":"drive"},"loc":{"type":"local"}}"#),
        );
        let r = f.d.handle("remotes.list", json!({})).unwrap();
        assert_eq!(
            r["remotes"][0],
            json!({"name":"gdrive","type":"drive","description":""})
        );
        assert_eq!(r["remotes"][1]["type"], "local");
    }

    #[test]
    fn remotes_add_validation_and_exists() {
        let f = fx();
        assert_eq!(
            err(f.d.handle("remotes.add", json!({"type":"local"}))).code,
            "invalid_params"
        );
        assert_eq!(
            err(f.d.handle("remotes.add", json!({"name":"a"}))).code,
            "invalid_params"
        );
        f.fake
            .on(&["listremotes"], Response::new().stdout("scratch:"));
        let e = err(f
            .d
            .handle("remotes.add", json!({"name":"scratch","type":"local"})));
        assert_eq!(e.code, "remote_exists");
        assert_eq!(e.message, "remote scratch already exists");
        assert!(calls_with(&f.fake, "config", "create").is_empty());
    }

    #[test]
    fn remotes_add_success_passes_sorted_kvs_and_verifies() {
        let f = fx();
        let r =
            f.d.handle(
                "remotes.add",
                json!({"name":"s","type":"local","config":{"z":"1","a":true,"n":5}}),
            )
            .unwrap();
        assert_eq!(r["verified"], true);
        assert_eq!(
            r["remote"],
            json!({"name":"s","type":"local","description":""})
        );
        let c = calls_with(&f.fake, "config", "create");
        assert_eq!(c.len(), 1);
        assert_eq!(&c[0][2..7], ["s", "local", "a=true", "n=5", "z=1"]);
        assert!(c[0].contains(&"--non-interactive".to_string()));
        assert_eq!(f.fake.calls_for("lsd").len(), 1);
        assert!(calls_with(&f.fake, "config", "delete").is_empty());
    }

    #[test]
    fn remotes_add_publishes_state_changed() {
        let f = fx();
        let sub =
            f.d.container()
                .bus
                .subscribe(&[crate::event_bus::topic::STATE_CHANGED]);
        f.d.handle("remotes.add", json!({"name":"s","type":"local"}))
            .unwrap();
        let ev = sub.recv_timeout(Duration::from_secs(2)).unwrap();
        match ev {
            crate::event_bus::BusEvent::StateChanged(s) => {
                assert_eq!((s.domain.as_str(), s.id.as_str()), ("remotes", "s"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn remotes_add_oauth_question_cleans_up_and_reports_oauth_required() {
        let f = fx();
        f.fake.on(
            &["config", "create"],
            Response::new().stdout(r#"{"State":"*oauth-islocal,drive,,","Option":{"Name":"config_is_local","Help":"h","Default":true,"Examples":[{"Value":"true","Help":"yes"}]},"Error":"","Result":""}"#),
        );
        let e = err(f
            .d
            .handle("remotes.add", json!({"name":"g","type":"drive"})));
        assert_eq!(e.code, "oauth_required");
        let d = e.data.unwrap();
        assert_eq!(d["state"], "*oauth-islocal,drive,,");
        assert_eq!(d["questions"][0]["option"]["name"], "config_is_local");
        assert_eq!(calls_with(&f.fake, "config", "delete").len(), 1);
    }

    #[test]
    fn remotes_add_extra_option_question_is_invalid_params() {
        let f = fx();
        f.fake.on(
            &["config", "create"],
            Response::new().stdout(r#"{"State":"bucket","Option":{"Name":"bucket","Help":"b"}}"#),
        );
        let e = err(f.d.handle("remotes.add", json!({"name":"g","type":"s3"})));
        assert_eq!(e.code, "invalid_params");
        assert_eq!(e.message, "remote g requires additional options");
        let d = e.data.unwrap();
        assert_eq!(d["field"], "config");
        assert_eq!(d["question"]["option"]["name"], "bucket");
        assert_eq!(calls_with(&f.fake, "config", "delete").len(), 1);
    }

    #[test]
    fn remotes_add_state_only_question_continues_to_verify() {
        let f = fx();
        f.fake.on(
            &["config", "create"],
            Response::new().stdout(r#"{"State":"","Option":null}"#),
        );
        let r =
            f.d.handle("remotes.add", json!({"name":"g","type":"alias"}))
                .unwrap();
        assert_eq!(r["verified"], true);
    }

    #[test]
    fn remotes_add_nonzero_exit_is_rclone_failed_and_redacted() {
        let f = fx();
        f.fake.on(
            &["config", "create"],
            Response::new().stderr("boom password=hunter2").exit(1),
        );
        let e = err(f.d.handle("remotes.add", json!({"name":"g","type":"x"})));
        assert_eq!(e.code, "rclone_failed");
        assert!(!e.message.contains("hunter2"), "{}", e.message);
        assert!(e.message.starts_with("rclone config create failed: "));
        assert_eq!(e.data.unwrap()["command"], "config create");
        assert_eq!(calls_with(&f.fake, "config", "delete").len(), 1);
    }

    #[test]
    fn remotes_add_verification_failure_is_auth_failed() {
        let f = fx();
        f.fake.on(
            &["lsd"],
            Response::new().stderr("token=abc bad creds").exit(1),
        );
        let e = err(f.d.handle("remotes.add", json!({"name":"g","type":"s3"})));
        assert_eq!(e.code, "auth_failed");
        assert!(e.message.starts_with("remote g could not be verified: "));
        assert!(!e.message.contains("abc "), "{}", e.message);
        assert_eq!(calls_with(&f.fake, "config", "delete").len(), 1);
    }

    #[test]
    fn remotes_test_and_delete() {
        let f = fx();
        assert_eq!(
            err(f.d.handle("remotes.test", json!({"name":"nope"}))).code,
            "remote_not_found"
        );
        assert_eq!(
            err(f.d.handle("remotes.delete", json!({"name":"nope"}))).code,
            "remote_not_found"
        );
        f.fake.on(&["listremotes"], Response::new().stdout("a:"));
        assert_eq!(
            f.d.handle("remotes.test", json!({"name":"a"})).unwrap(),
            json!({"name":"a","ok":true})
        );
        f.fake.on(
            &["lsd"],
            Response::new().stderr("secret=zzz denied").exit(3),
        );
        let e = err(f.d.handle("remotes.test", json!({"name":"a"})));
        assert_eq!(e.code, "test_failed");
        assert!(!e.message.contains("zzz"));
        assert_eq!(
            f.d.handle("remotes.delete", json!({"name":"a"})).unwrap(),
            json!({"deleted":true,"name":"a"})
        );
        assert_eq!(calls_with(&f.fake, "config", "delete").len(), 1);
    }

    // ---- oauth ----

    fn oauth_fx() -> (Fx, FakeSpawner) {
        let f = fx();
        let sp = FakeSpawner::new();
        f.d.set_oauth_spawner(Arc::new(sp.clone()));
        (f, sp)
    }

    fn authorize(sp: &FakeSpawner) {
        sp.emit(StreamKind::Stdout, TOKEN_OUTPUT);
        sp.exit(0);
    }

    #[test]
    fn oauth_start_validation_and_result() {
        let (f, sp) = oauth_fx();
        sp.push_script(Script {
            initial: vec![(StreamKind::Stderr, URL_OUTPUT.into())],
            ..Default::default()
        });
        let e = err(f.d.handle(
            "remotes.oauth.start",
            json!({"type":"drive","clientID":"x"}),
        ));
        assert_eq!(e.code, "invalid_params");
        assert_eq!(e.data.unwrap()["field"], "clientID");
        assert_eq!(
            err(f.d.handle("remotes.oauth.start", json!({}))).code,
            "invalid_params"
        );
        assert!(sp.calls().is_empty());

        let r =
            f.d.handle(
                "remotes.oauth.start",
                json!({"type":"drive","clientID":"i","clientSecret":"s"}),
            )
            .unwrap();
        assert_eq!(r["sessionID"], "O1");
        assert_eq!(r["type"], "drive");
        assert_eq!(r["authURL"], "http://127.0.0.1:53682/auth?state=abc");
        assert!(r["expiresAt"].as_str().unwrap().ends_with('Z'));
        let args = &sp.calls()[0];
        assert_eq!(args.0, "/fake/rclone");
        assert_eq!(&args.1[..4], ["authorize", "drive", "i", "s"]);
        assert!(args.1.contains(&"--auth-no-open-browser".to_string()));
        let e = err(f.d.handle("remotes.oauth.start", json!({"type":"drive"})));
        assert_eq!(e.code, "oauth_session_active");
    }

    #[test]
    fn oauth_cancel_flow() {
        let (f, sp) = oauth_fx();
        sp.push_script(Script {
            initial: vec![(StreamKind::Stderr, URL_OUTPUT.into())],
            ..Default::default()
        });
        assert_eq!(
            err(f
                .d
                .handle("remotes.oauth.cancel", json!({"sessionID":"O9"})))
            .code,
            "oauth_session_not_found"
        );
        f.d.handle("remotes.oauth.start", json!({"type":"drive"}))
            .unwrap();
        let r =
            f.d.handle("remotes.oauth.cancel", json!({"sessionID":"O1"}))
                .unwrap();
        assert_eq!(r, json!({"cancelled":true,"sessionID":"O1"}));
        assert_eq!(sp.terminate_count(), 1);
        assert_eq!(
            err(f
                .d
                .handle("remotes.oauth.cancel", json!({"sessionID":"O1"})))
            .code,
            "oauth_session_not_found"
        );
    }

    fn start_authorized(f: &Fx, sp: &FakeSpawner) {
        sp.push_script(Script {
            initial: vec![(StreamKind::Stderr, URL_OUTPUT.into())],
            ..Default::default()
        });
        f.d.handle("remotes.oauth.start", json!({"type":"drive"}))
            .unwrap();
        authorize(sp);
    }

    #[test]
    fn oauth_complete_wizard_answers_questions_and_verifies() {
        let (f, sp) = oauth_fx();
        start_authorized(&f, &sp);
        f.fake.on(
            &["config", "create"],
            Response::new().stdout(
                r#"{"State":"*oauth-islocal,drive,,","Option":{"Name":"config_is_local"}}"#,
            ),
        );
        f.fake.on_contains(
            &["--continue", "--state", "*oauth-islocal,drive,,"],
            Response::new()
                .stdout(r#"{"State":"*oauth-authorize,drive,,","Option":{"Name":"config_token"}}"#),
        );
        f.fake.on_contains(
            &["--continue", "--state", "*oauth-authorize,drive,,"],
            Response::new()
                .stdout(r#"{"State":"*oauth-confirm,drive,,","Option":{"Name":"team_drive"}}"#),
        );
        f.fake.on_contains(
            &["--continue", "--state", "*oauth-confirm,drive,,"],
            Response::new().stdout(r#"{"State":"","Option":null}"#),
        );
        let sub =
            f.d.container()
                .bus
                .subscribe(&[crate::event_bus::topic::STATE_CHANGED]);
        let r = f
            .d
            .handle("remotes.oauth.complete", json!({"sessionID":"O1","name":"mine","config":{"team_drive":"td1","scope":"drive"}}))
            .unwrap();
        assert_eq!(
            r,
            json!({"remote":{"name":"mine","type":"drive","description":""},"verified":true})
        );
        let cont: Vec<Vec<String>> = f
            .fake
            .calls()
            .into_iter()
            .filter(|c| c.contains(&"--continue".to_string()))
            .collect();
        assert_eq!(cont.len(), 3);
        let result_of = |c: &Vec<String>| {
            let i = c.iter().position(|a| a == "--result").unwrap();
            c[i + 1].clone()
        };
        assert_eq!(result_of(&cont[0]), "false");
        assert!(result_of(&cont[1]).contains("access_token"));
        assert_eq!(result_of(&cont[2]), "td1");
        // kvs are sorted and passed on each step.
        assert!(cont[0].contains(&"scope=drive".to_string()));
        assert_eq!(f.fake.calls_for("lsd").len(), 1);
        assert!(calls_with(&f.fake, "config", "delete").is_empty());
        assert!(sub.recv_timeout(Duration::from_secs(2)).is_some());
        // Session forgotten afterwards.
        assert_eq!(
            err(f
                .d
                .handle("remotes.oauth.cancel", json!({"sessionID":"O1"})))
            .code,
            "oauth_session_not_found"
        );
    }

    #[test]
    fn oauth_complete_error_paths() {
        let (f, sp) = oauth_fx();
        let e = err(f.d.handle(
            "remotes.oauth.complete",
            json!({"sessionID":"O7","name":"x"}),
        ));
        assert_eq!(e.code, "oauth_session_not_found");
        assert_eq!(e.message, "OAuth session O7 not found");

        start_authorized(&f, &sp);
        // Existing remote.
        f.fake
            .on(&["listremotes"], Response::new().stdout("taken:"));
        assert_eq!(
            err(f.d.handle(
                "remotes.oauth.complete",
                json!({"sessionID":"O1","name":"taken"})
            ))
            .code,
            "remote_exists"
        );
        f.fake.on(&["listremotes"], Response::new());

        // Unexpected question → cleanup + oauth_failed, session forgotten.
        f.fake.on(
            &["config", "create"],
            Response::new().stdout(r#"{"State":"s","Option":{"Name":"mystery"}}"#),
        );
        let e = err(f.d.handle(
            "remotes.oauth.complete",
            json!({"sessionID":"O1","name":"n"}),
        ));
        assert_eq!(e.code, "oauth_failed");
        assert_eq!(e.message, "unexpected configuration question: mystery");
        assert_eq!(e.data.unwrap()["question"]["option"]["name"], "mystery");
        assert_eq!(calls_with(&f.fake, "config", "delete").len(), 1);
        assert_eq!(
            err(f.d.handle(
                "remotes.oauth.complete",
                json!({"sessionID":"O1","name":"n"})
            ))
            .code,
            "oauth_session_not_found"
        );
    }

    #[test]
    fn oauth_complete_nonzero_create_and_too_many_steps_and_verify_failure() {
        let (f, sp) = oauth_fx();
        // create exits non-zero without question
        start_authorized(&f, &sp);
        f.fake.on(
            &["config", "create"],
            Response::new().stderr("nope token=ttt").exit(1),
        );
        let e = err(f.d.handle(
            "remotes.oauth.complete",
            json!({"sessionID":"O1","name":"n"}),
        ));
        assert_eq!(e.code, "oauth_failed");
        assert!(e.message.starts_with("remote creation failed: "));
        assert!(!e.message.contains("ttt"));
        assert_eq!(calls_with(&f.fake, "config", "delete").len(), 1);

        // endless questions → too many configuration steps
        start_authorized(&f, &sp);
        let q = r#"{"State":"loop","Option":{"Name":"config_is_local"}}"#;
        f.fake.on(&["config", "create"], Response::new().stdout(q));
        let e = err(f.d.handle(
            "remotes.oauth.complete",
            json!({"sessionID":"O2","name":"n"}),
        ));
        assert_eq!(e.message, "too many configuration steps");

        // verification failure
        start_authorized(&f, &sp);
        f.fake.on(&["config", "create"], Response::new());
        f.fake
            .on(&["lsd"], Response::new().stderr("denied").exit(1));
        let e = err(f.d.handle(
            "remotes.oauth.complete",
            json!({"sessionID":"O3","name":"n"}),
        ));
        assert_eq!(e.code, "test_failed");
        assert!(
            e.message
                .starts_with("remote n could not be verified: denied")
        );
    }

    #[test]
    fn oauth_complete_session_state_errors() {
        let (f, sp) = oauth_fx();
        // cancelled
        sp.push_script(Script {
            initial: vec![(StreamKind::Stderr, URL_OUTPUT.into())],
            ..Default::default()
        });
        f.d.handle("remotes.oauth.start", json!({"type":"drive"}))
            .unwrap();
        let m = f.d.oauth().unwrap();
        m.cancel("O1");
        let e = err(f.d.handle(
            "remotes.oauth.complete",
            json!({"sessionID":"O1","name":"n"}),
        ));
        assert_eq!(e.code, "oauth_cancelled");
        assert_eq!(e.message, "OAuth session O1 was cancelled");
        // failed
        sp.push_script(Script {
            initial: vec![(StreamKind::Stderr, URL_OUTPUT.into())],
            ..Default::default()
        });
        f.d.handle("remotes.oauth.start", json!({"type":"drive"}))
            .unwrap();
        sp.emit(StreamKind::Stderr, "access denied\n");
        sp.exit(1);
        let e = err(f.d.handle(
            "remotes.oauth.complete",
            json!({"sessionID":"O2","name":"n"}),
        ));
        assert_eq!(e.code, "oauth_failed");
        assert!(e.message.contains("access denied"));
    }

    // ---- fs.list ----

    const LSJSON: &str = r#"[{"Name":"a.txt","Size":3,"MimeType":"text/plain","IsDir":false,"ModTime":"2024-01-01T00:00:00Z","Path":"a.txt","ID":"1"},{"Name":"d","Size":-1,"MimeType":"inode/directory","IsDir":true,"ModTime":"","Path":"d","ID":""},{"Name":"b","Size":1,"IsDir":false,"Path":"b"}]"#;

    #[test]
    fn fs_list_shapes_and_validation() {
        let f = fx();
        f.fake.on(
            &["--log-level", "ERROR", "lsjson"],
            Response::new().stdout(LSJSON),
        );
        let r = f.d.handle("fs.list", json!({"path":"/tmp/x"})).unwrap();
        assert_eq!(r["path"], "/tmp/x");
        assert_eq!(r["truncated"], false);
        assert_eq!(r["entries"].as_array().unwrap().len(), 3);
        assert_eq!(
            r["entries"][0],
            json!({"name":"a.txt","size":3,"mimeType":"text/plain","isDir":false,"modTime":"2024-01-01T00:00:00Z","path":"a.txt","id":"1"})
        );
        let r =
            f.d.handle("fs.list", json!({"path":"gd:docs","limit":2}))
                .unwrap();
        assert_eq!(r["entries"].as_array().unwrap().len(), 2);
        assert_eq!(r["truncated"], true);
        let r =
            f.d.handle("fs.list", json!({"path":"gd:","limit":0}))
                .unwrap();
        assert_eq!(r["entries"].as_array().unwrap().len(), 0);
        assert_eq!(r["truncated"], true);

        assert_eq!(
            err(f.d.handle("fs.list", json!({"path":"relative"}))).code,
            "fs_missing_path"
        );
        assert_eq!(
            err(f.d.handle("fs.list", json!({"path":"/x","limit":-1}))).code,
            "invalid_params"
        );
        assert_eq!(err(f.d.handle("fs.list", json!({}))).code, "invalid_params");
    }

    #[test]
    fn fs_list_failure_is_browse_failed_and_recursive_uses_pending_listing() {
        let f = fx();
        f.fake.on(
            &["--log-level", "ERROR", "lsjson"],
            Response::new()
                .stderr("directory not found secret=s1")
                .exit(3),
        );
        let e = err(f.d.handle("fs.list", json!({"path":"gd:nope"})));
        assert_eq!(e.code, "browse_failed");
        assert!(e.message.starts_with("could not list gd:nope: "));
        assert!(!e.message.contains("s1"));
        let d = e.data.unwrap();
        assert_eq!(d["path"], "gd:nope");

        let g = fx();
        // Swift `listFilesForPending` / rpc.md: `lsjson --recursive --files-only`.
        g.fake.on(
            &["--log-level", "ERROR", "lsjson"],
            Response::new().stdout(LSJSON),
        );
        let r =
            g.d.handle(
                "fs.list",
                json!({"path":"/tmp/x","recursive":true,"limit":1}),
            )
            .unwrap();
        assert_eq!(r["entries"].as_array().unwrap().len(), 1);
        assert_eq!(r["truncated"], true);
        assert!(g.fake.calls().iter().any(|c| {
            c.iter().any(|a| a == "lsjson")
                && c.iter().any(|a| a == "--recursive")
                && c.iter().any(|a| a == "--files-only")
        }));
    }

    // ---- service ----

    #[derive(Default)]
    struct FakeCtl {
        calls: Mutex<Vec<Vec<String>>>,
        print: Mutex<String>,
    }
    impl crate::launchd::LaunchctlRunner for FakeCtl {
        fn run(&self, args: &[String]) -> Result<String, String> {
            self.calls.lock().push(args.to_vec());
            if args[0] == "print" {
                Ok(self.print.lock().clone())
            } else {
                Ok(String::new())
            }
        }
    }

    fn svc_fx() -> (Fx, Arc<FakeCtl>, tempfile::TempDir) {
        let f = fx();
        let home = tempfile::tempdir().unwrap();
        let ctl = Arc::new(FakeCtl::default());
        f.d.set_launchd(Arc::new(
            LaunchdService::with_parts(home.path().to_str().unwrap(), 501, ctl.clone())
                .with_default_exec("/x/gn-drive"),
        ));
        (f, ctl, home)
    }

    #[test]
    fn service_status_install_start_stop_uninstall() {
        let (f, ctl, home) = svc_fx();
        let plist = format!(
            "{}/Library/LaunchAgents/com.gndrive.app.plist",
            home.path().display()
        );

        let s = f.d.handle("service.status", json!({})).unwrap();
        assert_eq!(
            s,
            json!({"installed":false,"running":false,"mode":"service","scope":"user","label":"com.gndrive.app","plistPath":plist})
        );
        assert!(ctl.calls.lock().is_empty());

        *ctl.print.lock() = "state = running\n\tpid = 4242\n".into();
        let s =
            f.d.handle("service.install", json!({"execPath":"/opt/gn-drive"}))
                .unwrap();
        assert_eq!(s["installed"], true);
        assert_eq!(s["running"], true);
        assert_eq!(s["pid"], 4242);
        let body = std::fs::read_to_string(&plist).unwrap();
        assert!(body.contains("/opt/gn-drive"));
        assert!(body.contains(&f.dir));
        assert!(
            ctl.calls
                .lock()
                .iter()
                .any(|c| c[0] == "bootstrap" && c[1] == "gui/501")
        );

        let s =
            f.d.handle("service.status", json!({"scope":"user"}))
                .unwrap();
        assert_eq!(s["pid"], 4242);

        *ctl.print.lock() = String::new();
        let s = f.d.handle("service.stop", json!({})).unwrap();
        assert_eq!(s["running"], false);
        assert!(s.get("pid").is_none());
        f.d.handle("service.start", json!({})).unwrap();
        f.d.handle("service.restart", json!({})).unwrap();
        let names: Vec<String> = ctl.calls.lock().iter().map(|c| c[0].clone()).collect();
        assert!(names.contains(&"kickstart".to_string()) && names.contains(&"bootout".to_string()));

        let u = f.d.handle("service.uninstall", json!({})).unwrap();
        assert_eq!(u, json!({"installed":false,"running":false,"scope":"user"}));
        assert!(!std::path::Path::new(&plist).exists());
    }

    #[test]
    fn service_errors() {
        let (f, _ctl, _home) = svc_fx();
        assert_eq!(
            err(f.d.handle("service.status", json!({"scope":"bogus"}))).code,
            "invalid_params"
        );
        assert_eq!(
            err(f.d.handle("service.start", json!({}))).code,
            "service_not_installed"
        );
        assert_eq!(
            err(f.d.handle("service.stop", json!({}))).code,
            "service_not_installed"
        );
        // SAFETY: getuid never fails.
        if unsafe { libc::getuid() } != 0 {
            let e = err(f.d.handle("service.install", json!({"scope":"system"})));
            assert_eq!(e.code, "permission_denied");
            assert_eq!(e.message, "system-scope install requires root (sudo)");
        }
    }

    #[test]
    fn service_health_present_and_absent() {
        let f = fx();
        assert_eq!(
            f.d.handle("service.health", json!({})).unwrap(),
            json!({"present":false})
        );
        crate::health::HealthWriter::new(&f.dir)
            .write_now()
            .unwrap();
        let h = f.d.handle("service.health", json!({})).unwrap();
        assert_eq!(h["present"], true);
        assert_eq!(h["webPort"], 0);
        assert_eq!(h["stale"], false);
        assert!(h["pid"].as_i64().unwrap() > 0);
        assert!(h["lastSyncAt"].is_null());
    }

    // ---- update ----

    struct FakeHttp {
        status: i32,
        body: String,
        urls: Mutex<Vec<String>>,
    }
    impl HttpFetcher for FakeHttp {
        fn get(
            &self,
            url: &str,
            _h: &[(String, String)],
            _t: Duration,
        ) -> Result<HttpResponse, String> {
            self.urls.lock().push(url.to_string());
            Ok(HttpResponse {
                status: self.status,
                body: self.body.clone().into_bytes(),
            })
        }
    }

    fn update_fx(status: i32, body: &str) -> (Fx, Arc<FakeHttp>) {
        let f = fx();
        let http = Arc::new(FakeHttp {
            status,
            body: body.into(),
            urls: Mutex::new(Vec::new()),
        });
        f.d.set_update_fetcher(http.clone());
        f.d.set_update_api_base(Some("https://api.test".into()));
        (f, http)
    }

    #[test]
    fn update_check_reports_versions() {
        let (f, http) = update_fx(200, r#"{"tag_name":"v1.2.0","assets":[]}"#);
        let r = f.d.handle("update.check", json!({})).unwrap();
        assert_eq!(r["current"], "1.0.0");
        assert_eq!(r["latest"], "1.2.0");
        assert_eq!(r["updateAvailable"], true);
        assert!(r["checkedAt"].as_str().unwrap().ends_with('Z'));
        assert_eq!(
            http.urls.lock()[0],
            "https://api.test/repos/gnasdev/gn-drive/releases/latest"
        );

        let r =
            f.d.handle(
                "update.check",
                json!({"currentVersion":"1.2.0","repoOwner":"o","repo":"r"}),
            )
            .unwrap();
        assert_eq!(r["updateAvailable"], false);
        assert_eq!(
            http.urls.lock()[1],
            "https://api.test/repos/o/r/releases/latest"
        );
    }

    #[test]
    fn update_check_and_apply_error_mapping() {
        let (f, _) = update_fx(404, "{}");
        assert_eq!(
            err(f.d.handle("update.check", json!({}))).code,
            "no_release"
        );
        let (f, _) = update_fx(500, "server down");
        let e = err(f.d.handle("update.check", json!({})));
        assert_eq!(e.code, "update_failed");
        assert!(e.message.contains("500"));
        let (f, _) = update_fx(200, r#"{"tag_name":"v1.0.0","assets":[]}"#);
        if !f.d.bundled_app() {
            assert_eq!(
                err(f.d.handle("update.apply", json!({}))).code,
                "already_up_to_date"
            );
            // force skips the up-to-date guard and then fails on missing assets.
            assert_eq!(
                err(f.d.handle("update.apply", json!({"force":true}))).code,
                "no_matching_asset"
            );
        }
    }
}
