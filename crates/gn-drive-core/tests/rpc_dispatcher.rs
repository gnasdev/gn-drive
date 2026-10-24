//! Port of the dispatcher half of `Tests/GNDriveCoreTests/RPCTests.swift`:
//! round-trips per method group, locked gating, validation errors. Runs
//! against a container built with `FakeRunner` / `MemorySecureStore` / a temp
//! dir (no real rclone, no network, no keychain).
#![cfg(feature = "test-util")]

use gn_drive_core::container::{AppContainer, ContainerOptions};
use gn_drive_core::event_bus::{BusEvent, topic};
use gn_drive_core::keychain::MemorySecureStore;
use gn_drive_core::logger::Logger;
use gn_drive_core::rclone::testing::{FakeRunner, Response};
use gn_drive_core::rpc::{Dispatcher, LogBuffer, Options, RpcError};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Fixture {
    _dir: tempfile::TempDir,
    dir: String,
    runner: Arc<FakeRunner>,
    container: Arc<AppContainer>,
    d: Arc<Dispatcher>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.d.shutdown();
        self.container.close();
    }
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_string_lossy().into_owned();
    let runner = Arc::new(FakeRunner::new());
    runner.on(&["version"], Response::new().stdout("rclone v1.99.0-test"));
    let opts = ContainerOptions {
        config_dir: dir.clone(),
        portal_mode: true,
        version: "test".into(),
        key_store: Some(Arc::new(MemorySecureStore::new())),
        rclone_runner: Some(runner.clone()),
        logger: Some(Logger::quiet()),
        ..Default::default()
    };
    let container = AppContainer::new(opts).expect("container");
    container.sync_engine.start();
    let o = Options {
        exit_on_shutdown: false,
        log_buffer_capacity: 100,
        ..Default::default()
    };
    let d = Dispatcher::start(container.clone(), "", o);
    Fixture {
        _dir: tmp,
        dir,
        runner,
        container,
        d,
    }
}

impl Fixture {
    fn ok(&self, method: &str, params: Value) -> Value {
        self.d
            .handle(method, params)
            .unwrap_or_else(|e| panic!("{method}: {e}"))
    }
    fn err(&self, method: &str, params: Value) -> RpcError {
        match self.d.handle(method, params) {
            Ok(v) => panic!("{method} unexpectedly succeeded: {v}"),
            Err(e) => e,
        }
    }
    fn code(&self, method: &str, params: Value) -> String {
        self.err(method, params).code
    }
}

fn poll<T>(timeout_ms: u64, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let end = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if Instant::now() >= end {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

// MARK: - System

#[test]
fn ping_snapshot_doctor_and_settings() {
    let f = fixture();
    let ping = f.ok("ping", json!({}));
    assert_eq!(ping["pong"], true);
    assert_eq!(ping["protocol"], 1);
    assert_eq!(ping["version"], "test");

    let snap = f.ok("state.snapshot", json!({"sections": ["auth", "settings"]}));
    assert_eq!(snap["locked"], false);
    assert!(snap.get("auth").is_some());
    assert!(snap.get("appSettings").is_some());
    assert!(snap.get("flows").is_none());

    let doctor = f.ok("doctor", json!({"includeDataFiles": true}));
    assert!(doctor.get("ok").is_some());
    assert!(!doctor["checks"].as_array().unwrap().is_empty());
    assert!(doctor.get("dataFiles").is_some());

    let set = f.ok("settings.set", json!({"settings": {"debugMode": true}}));
    assert_eq!(set["settings"]["debugMode"], true);
    assert_eq!(
        f.ok("settings.get", json!({}))["settings"]["debugMode"],
        true
    );

    let logs = f.ok("logs.read", json!({"limit": 5}));
    assert!(logs.get("lines").is_some());

    let health = f.ok("service.health", json!({}));
    assert_eq!(health["present"], false);

    let status = f.ok("service.status", json!({"scope": "user"}));
    assert_eq!(status["scope"], "user");
    assert!(status.get("plistPath").is_some());
}

#[test]
fn state_snapshot_full_when_unlocked() {
    let f = fixture();
    let snap = f.ok("state.snapshot", json!({}));
    for k in [
        "flows",
        "remotes",
        "profiles",
        "history",
        "runtime",
        "appSettings",
        "paths",
    ] {
        assert!(snap.get(k).is_some(), "missing {k}");
    }
    assert_eq!(snap["rcloneVersion"], "rclone v1.99.0-test");
    let rev = &snap["runtime"]["revision"];
    assert!(rev.is_i64() || rev.is_u64());
    assert_eq!(
        f.code("state.snapshot", json!({"sections": "auth"})),
        "invalid_params"
    );
    // Unknown sections are ignored.
    let s = f.ok("state.snapshot", json!({"sections": ["bogus"]}));
    assert!(s.get("flows").is_none() && s.get("appSettings").is_none());
}

#[test]
fn unknown_method_and_invalid_params() {
    let f = fixture();
    assert_eq!(f.code("nope", json!({})), "unknown_method");
    assert_eq!(f.err("nope", json!({})).message, "unknown method: nope");
    assert_eq!(f.code("flows.get", json!({})), "invalid_params");
    assert_eq!(f.code("ping", json!([1])), "invalid_params");
    assert_eq!(f.code("logs.read", json!({"limit": -1})), "invalid_params");
    assert_eq!(
        f.code("logs.read", json!({"level": "loud"})),
        "invalid_params"
    );
    assert_eq!(f.ok("app.quit", Value::Null)["quitting"], true);
    // null params are accepted
    assert_eq!(f.ok("ping", Value::Null)["pong"], true);
}

#[test]
fn logs_read_sees_logger_sink() {
    let f = fixture();
    f.container.log.info("rpc test log line", &[("k", "v")]);
    let v = f.ok("logs.read", json!({"level": "info"}));
    let lines = v["lines"].as_array().unwrap();
    let hit = lines
        .iter()
        .find(|l| l["msg"] == "rpc test log line")
        .expect("line present");
    assert_eq!(hit["level"], "INFO");
    assert_eq!(hit["fields"]["k"], "v");
}

#[test]
fn hello_data_shape() {
    let f = fixture();
    let h = f.d.hello_data("/tmp/x.lock", 42);
    assert_eq!(h["protocol"], 1);
    assert_eq!(h["pid"], 42);
    assert_eq!(h["engine"], "GNDriveCore");
    assert_eq!(h["setup"], false);
    assert_eq!(h["passwordRequired"], false);
    assert_eq!(h["instance"]["lockPath"], "/tmp/x.lock");
    assert_eq!(h["paths"]["configDir"], f.dir.as_str());
    assert!(
        h["paths"]["dbPath"]
            .as_str()
            .unwrap()
            .ends_with("gn-drive.db")
    );
    assert_eq!(h["capabilities"]["doctor"], true);
}

// MARK: - Auth gating

#[test]
fn auth_lifecycle_and_locked_gate() {
    let f = fixture();
    let setup = f.ok("auth.setup", json!({"password": "testpass"}));
    assert_eq!(setup["setup"], true);
    assert_eq!(setup["unlocked"], true);

    f.ok("auth.lock", json!({}));
    assert!(f.d.is_locked());
    assert_eq!(f.code("flows.list", json!({})), "locked");
    assert_eq!(f.code("runtime.snapshot", json!({})), "locked");
    assert_eq!(f.err("flows.list", json!({})).data.unwrap()["setup"], true);

    // Always-available methods still work while locked.
    assert_eq!(f.ok("ping", json!({}))["pong"], true);
    let snap = f.ok("state.snapshot", json!({}));
    assert!(snap.get("auth").is_some());
    assert_eq!(snap["locked"], true);
    assert_eq!(snap["flows"], json!([]));
    assert_eq!(snap["runtime"]["revision"], 0);

    assert_eq!(
        f.code("auth.unlock", json!({"password": "wrong"})),
        "invalid_password"
    );
    let e = f.err("auth.unlock", json!({"password": "wrong"}));
    assert!(e.data.unwrap()["failedAttempts"].as_i64().unwrap() >= 1);
    let unlocked = f.ok("auth.unlock", json!({"password": "testpass"}));
    assert_eq!(unlocked["unlocked"], true);

    f.ok(
        "auth.changePassword",
        json!({"oldPassword": "testpass", "newPassword": "newpass"}),
    );
    f.ok("auth.lock", json!({}));
    assert_eq!(
        f.code("auth.unlock", json!({"password": "testpass"})),
        "invalid_password"
    );
    f.ok("auth.unlock", json!({"password": "newpass"}));

    let removed = f.ok("auth.removePassword", json!({"password": "newpass"}));
    assert_eq!(removed["setup"], false);
    assert!(!f.d.is_locked());
}

#[test]
fn auth_setup_validation_and_events() {
    let f = fixture();
    assert_eq!(f.code("auth.setup", json!({})), "invalid_params");
    assert_eq!(
        f.code("auth.setup", json!({"password": "ab"})),
        "password_too_short"
    );
    let sub = f
        .container
        .bus
        .subscribe(&[topic::AUTH_UNLOCKED, topic::AUTH_LOCKED]);
    f.ok("auth.setup", json!({"password": "testpass"}));
    assert_eq!(
        f.code("auth.setup", json!({"password": "testpass"})),
        "already_setup"
    );
    assert!(matches!(
        sub.recv_timeout(Duration::from_secs(2)),
        Some(BusEvent::AuthUnlocked(_))
    ));
    f.ok("auth.lock", json!({}));
    assert!(matches!(
        sub.recv_timeout(Duration::from_secs(2)),
        Some(BusEvent::AuthLocked(_))
    ));
}

// MARK: - Flows and operations

fn op(id: &str, dir: &str, action: &str) -> Value {
    json!({"id": id, "sourceRemote": "", "sourcePath": dir, "targetRemote": "", "targetPath": dir, "action": action})
}

#[test]
fn flow_and_operation_round_trip() {
    let f = fixture();
    let dir = f.dir.clone();
    let sub = f.container.bus.subscribe(&[topic::STATE_CHANGED]);

    let created = f.ok("flows.create", json!({"name": "Nightly", "id": "F1"}));
    assert_eq!(created["flow"]["id"], "F1");
    match sub.recv_timeout(Duration::from_secs(2)) {
        Some(BusEvent::StateChanged(e)) => {
            assert_eq!((e.domain.as_str(), e.id.as_str()), ("flows", "F1"));
        }
        other => panic!("expected state:changed, got {other:?}"),
    }
    assert_eq!(f.code("flows.create", json!({"id": "F1"})), "flow_exists");

    let op1 = f.ok(
        "operations.create",
        json!({"flowID": "F1", "operation": op("O1", &dir, "push")}),
    );
    assert_eq!(op1["operation"]["id"], "O1");
    let op2 = f.ok(
        "operations.create",
        json!({"flowId": "F1", "operation": op("O2", &dir, "pull")}),
    );
    assert_eq!(op2["operation"]["sortOrder"], 1);
    // legacy "pull" normalizes to push
    assert_eq!(op2["operation"]["action"], "push");

    let updated = f.ok(
        "operations.update",
        json!({"flowID": "F1", "operationID": "O1", "patch": {"action": "bi"}}),
    );
    assert_eq!(updated["operation"]["action"], "bi");
    assert_eq!(
        f.code(
            "operations.update",
            json!({"flowID": "F1", "operationID": "zz", "patch": {}})
        ),
        "operation_not_found"
    );
    assert_eq!(
        f.code(
            "operations.update",
            json!({"flowID": "F1", "operationID": "O1"})
        ),
        "invalid_params"
    );

    let reordered = f.ok(
        "operations.reorder",
        json!({"flowID": "F1", "operationIDs": ["O2", "O1"]}),
    );
    let ids: Vec<&str> = reordered["flow"]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["O2", "O1"]);
    assert_eq!(
        f.code(
            "operations.reorder",
            json!({"flowID": "F1", "operationIDs": ["O1"]})
        ),
        "invalid_params"
    );
    assert_eq!(
        f.code(
            "operations.reorder",
            json!({"flowID": "F1", "operationIDs": ["O1", "O1"]})
        ),
        "invalid_params"
    );

    f.ok(
        "operations.delete",
        json!({"flowID": "F1", "operationID": "O2"}),
    );
    assert_eq!(
        f.code(
            "operations.delete",
            json!({"flowID": "F1", "operationID": "O2"})
        ),
        "operation_not_found"
    );
    assert_eq!(
        f.code(
            "operations.create",
            json!({"flowID": "nope", "operation": {}})
        ),
        "flow_not_found"
    );

    let got = f.ok("flows.get", json!({"id": "F1"}));
    assert_eq!(got["flow"]["name"], "Nightly");
    assert_eq!(got["flow"]["operations"].as_array().unwrap().len(), 1);
    assert_eq!(f.code("flows.get", json!({"id": "nope"})), "flow_not_found");
    assert_eq!(
        f.ok("flows.list", json!({}))["flows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let save = f.ok(
        "flows.save",
        json!({"flow": {
            "id": "F1", "name": "Nightly v2", "scheduleEnabled": true, "scheduleCron": "not a cron",
            "operations": [op("O1", &dir, "push")],
        }}),
    );
    assert_eq!(save["warnings"][0]["code"], "invalid_cron");
    assert_eq!(save["flow"]["name"], "Nightly v2");
    assert_eq!(
        f.code("flows.save", json!({"flow": {"name": "x"}})),
        "invalid_params"
    );
    assert_eq!(
        f.err("flows.save", json!({"flow": {"name": "x"}}))
            .data
            .unwrap()["field"],
        "flow.id"
    );
    assert_eq!(f.code("flows.save", json!({})), "invalid_params");

    let deleted = f.ok("flows.delete", json!({"id": "F1"}));
    assert_eq!(deleted["deleted"], true);
    assert_eq!(f.code("flows.get", json!({"id": "F1"})), "flow_not_found");
    assert_eq!(
        f.code("flows.delete", json!({"id": "F1"})),
        "flow_not_found"
    );
}

#[test]
fn flow_create_defaults_and_canvas() {
    let f = fixture();
    let c = f.ok("flows.create", json!({"canvas": {"zoom": 2}}));
    assert_eq!(c["flow"]["name"], "Untitled flow");
    assert_eq!(c["flow"]["canvas"]["zoom"], 2);
    assert!(!c["flow"]["id"].as_str().unwrap().is_empty());
}

#[test]
fn flows_execute_stop_errors() {
    let f = fixture();
    assert_eq!(f.code("flows.stop", json!({"id": "none"})), "not_running");
    f.ok("flows.create", json!({"id": "E1", "name": "empty"}));
    assert_eq!(f.code("flows.execute", json!({"id": "E1"})), "flow_empty");
    assert_eq!(f.code("flows.execute", json!({})), "invalid_params");
}

// MARK: - Profiles, schedules, sync

#[test]
fn profile_and_sync_round_trip() {
    let f = fixture();
    let (src, dst) = (format!("{}/src", f.dir), format!("{}/dst", f.dir));
    std::fs::create_dir_all(&src).unwrap();

    let saved = f.ok(
        "profiles.save",
        json!({"profile": {"name": "local", "from": src, "to": dst, "direction": "push", "parallel": 2, "bogusKey": 1}}),
    );
    assert_eq!(saved["profile"]["name"], "local");
    assert_eq!(saved["profile"]["parallel"], 2);
    assert!(saved["profile"].get("bogusKey").is_none());

    let got = f.ok("profiles.get", json!({"name": "local"}));
    assert_eq!(got["profile"]["direction"], "push");
    assert_eq!(
        f.code("profiles.get", json!({"name": "x"})),
        "profile_not_found"
    );
    assert_eq!(
        f.code("profiles.save", json!({"profile": {"name": ""}})),
        "missing_name"
    );
    assert_eq!(
        f.code(
            "profiles.save",
            json!({"profile": {"name": "p", "direction": "weird"}})
        ),
        "invalid_direction"
    );
    assert_eq!(
        f.code(
            "profiles.save",
            json!({"profile": {"name": "p", "parallel": "many"}})
        ),
        "invalid_params"
    );
    assert_eq!(f.code("profiles.save", json!({})), "invalid_params");

    assert_eq!(
        f.ok("profiles.list", json!({}))["profiles"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(f.ok("schedules.list", json!({})).get("schedules").is_some());

    let sync = f.ok("sync.profile", json!({"profile": "local", "wait": true}));
    assert_eq!(sync["state"], "completed");
    assert_eq!(sync["action"], "push");
    assert!(!sync["taskID"].as_str().unwrap().is_empty());
    assert!(!f.runner.calls().is_empty());

    assert!(f.ok("runtime.tasks", json!({})).get("tasks").is_some());

    let entries = poll(3000, || {
        let e = f.ok("history.list", json!({"limit": 10}))["entries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        (!e.is_empty()).then_some(e)
    })
    .expect("history entry recorded");
    assert_eq!(entries[0]["profileName"], "local");
    let stats = f.ok("history.stats", json!({}));
    assert!(stats["totalSyncs"].as_i64().unwrap() >= 1);
    assert_eq!(
        f.ok("history.list", json!({"profile": "other"}))["entries"],
        json!([])
    );
    assert_eq!(
        f.ok("history.list", json!({"profile": "local"}))["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(f.ok("history.list", json!({}))["total"], Value::Null);
    assert_eq!(f.ok("history.clear", json!({}))["cleared"], true);
    assert!(
        f.ok("history.list", json!({}))["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    assert_eq!(
        f.code("sync.stop", json!({"taskID": "none"})),
        "task_not_found"
    );
    assert_eq!(
        f.code("sync.profile", json!({"profile": "local", "action": "wat"})),
        "invalid_action"
    );
    assert_eq!(
        f.code("sync.profile", json!({"profile": "ghost"})),
        "profile_not_found"
    );

    let deleted = f.ok("profiles.delete", json!({"name": "local"}));
    assert_eq!(deleted["deleted"], true);
    assert_eq!(
        f.code("profiles.delete", json!({"name": "local"})),
        "profile_not_found"
    );
}

#[test]
fn history_validation() {
    let f = fixture();
    assert_eq!(
        f.code("history.list", json!({"limit": -1})),
        "invalid_params"
    );
    assert_eq!(
        f.code("history.list", json!({"offset": -1})),
        "invalid_params"
    );
    assert_eq!(
        f.code("history.list", json!({"limit": "x"})),
        "invalid_params"
    );
    assert_eq!(
        f.ok("history.list", json!({"limit": 5000}))["entries"],
        json!([])
    );
}

#[test]
fn sync_profile_dry_run_returns_running_state_without_wait() {
    let f = fixture();
    f.ok(
        "profiles.save",
        json!({"profile": {"name": "p", "from": f.dir, "to": format!("{}/o", f.dir)}}),
    );
    let r = f.ok(
        "sync.profile",
        json!({"profile": "p", "dryRun": true, "wait": true}),
    );
    assert_eq!(r["state"], "completed");
    assert!(
        f.runner
            .calls()
            .iter()
            .any(|c| c.iter().any(|a| a == "--dry-run"))
    );
}

// MARK: - Remotes and filesystem (scripted rclone only)

#[test]
fn remotes_providers_list_and_errors() {
    let f = fixture();
    f.runner.on(
        &["config", "providers"],
        Response::new().stdout(
            r#"[{"Name":"local","Description":"Local Disk","Prefix":"local","Options":[]}]"#,
        ),
    );
    f.runner
        .on(&["listremotes"], Response::new().stdout("scratch:"));
    let providers = f.ok("remotes.providers", json!({}));
    assert_eq!(providers["providers"][0]["name"], "local");
    assert_eq!(providers["cached"], false);
    assert_eq!(f.ok("remotes.providers", json!({}))["cached"], true);
    assert_eq!(
        f.ok("remotes.providers", json!({"refresh": true}))["cached"],
        false
    );

    let listed = f.ok("remotes.list", json!({}));
    assert!(
        listed["remotes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["name"] == "scratch")
    );

    assert_eq!(
        f.code("remotes.test", json!({"name": "nope"})),
        "remote_not_found"
    );
    assert_eq!(
        f.code("remotes.delete", json!({"name": "nope"})),
        "remote_not_found"
    );
    assert_eq!(
        f.code("fs.list", json!({"path": "relative"})),
        "fs_missing_path"
    );
    assert_eq!(f.code("remotes.test", json!({})), "invalid_params");
}

// MARK: - Boards

#[test]
fn board_crud_and_errors() {
    let f = fixture();
    let dir = f.dir.clone();
    let saved = f.ok(
        "boards.save",
        json!({"board": {
            "id": "B1", "name": "Legacy",
            "nodes": [{"id": "n1", "remoteName": "", "path": dir, "label": "one", "x": 1.5}],
            "edges": [{"id": "e1", "sourceID": "n1", "targetID": "n1", "action": "push"},
                      {"sourceId": "n1", "targetId": "n1", "action": "push", "syncConfig": {"a": 1}}],
        }}),
    );
    assert_eq!(saved["board"]["id"], "B1");
    assert_eq!(saved["board"]["edges"].as_array().unwrap().len(), 2);
    assert!(
        saved["board"]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| !e["id"].as_str().unwrap().is_empty())
    );

    let got = f.ok("boards.get", json!({"id": "B1"}));
    assert_eq!(got["board"]["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(got["board"]["nodes"][0]["x"], 1.5);

    let list = f.ok("boards.list", json!({}));
    assert!(
        list["boards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["id"] == "B1")
    );
    assert!(list["boards"][0].get("nodes").is_none());

    let empty = f.ok(
        "boards.save",
        json!({"board": {"id": "B2", "name": "Empty", "nodes": [], "edges": []}}),
    );
    assert_eq!(empty["board"]["id"], "B2");
    assert_eq!(f.code("boards.execute", json!({"id": "B2"})), "board_empty");
    assert_eq!(
        f.code("boards.execute", json!({"id": "nope"})),
        "board_not_found"
    );
    assert_eq!(
        f.code("boards.stop", json!({"id": "B2"})),
        "board_not_running"
    );
    assert_eq!(
        f.ok("boards.status", json!({"id": "B2"}))["run"],
        Value::Null
    );

    assert_eq!(
        f.code("boards.save", json!({"board": {"id": " ", "name": "x"}})),
        "invalid_params"
    );
    assert_eq!(
        f.code("boards.save", json!({"board": {"id": "B3", "name": ""}})),
        "invalid_params"
    );
    let e = f.err(
        "boards.save",
        json!({"board": {"id": "B3", "name": "x", "nodes": [{"label": "l"}]}}),
    );
    assert_eq!(e.data.unwrap()["field"], "board.nodes[0].id");
    assert_eq!(
        f.code(
            "boards.save",
            json!({"board": {"id": "B3", "name": "x", "nodes": [1]}})
        ),
        "invalid_params"
    );

    f.ok("boards.delete", json!({"id": "B2"}));
    assert_eq!(f.code("boards.get", json!({"id": "B2"})), "board_not_found");
    assert_eq!(
        f.code("boards.delete", json!({"id": "B2"})),
        "board_not_found"
    );
}

// MARK: - Settings

#[test]
fn settings_set_validation() {
    let f = fixture();
    assert_eq!(f.code("settings.set", json!({})), "invalid_params");
    let e = f.err("settings.set", json!({"settings": {"debugMode": "maybe"}}));
    assert_eq!(e.data.unwrap()["field"], "settings.debugMode");
    let ok = f.ok(
        "settings.set",
        json!({"settings": {"minimizeToTray": "yes", "startAtLogin": null}}),
    );
    assert_eq!(ok["settings"]["minimizeToTray"], true);
    assert_eq!(ok["settings"]["startAtLogin"], false);
}

// MARK: - Push events

#[test]
fn dispatcher_subscribe_receives_state_changed() {
    let f = fixture();
    let rx = f.d.subscribe();
    f.ok("flows.create", json!({"id": "S1"}));
    let ev = poll(3000, || {
        rx.try_recv().ok().filter(|e| e.name == "state:changed")
    })
    .expect("state:changed pushed");
    assert_eq!(ev.data["domain"], "flows");
    assert_eq!(ev.data["id"], "S1");
    assert!(ev.to_frame().starts_with("{\"event\":\"state:changed\""));
}

#[test]
fn dispatcher_new_with_explicit_parts() {
    let f = fixture();
    let buf = Arc::new(LogBuffer::new(10));
    let bridge = gn_drive_core::rpc::bridge::EventBridge::new(
        f.container.clone(),
        buf.clone(),
        &Options::default(),
    );
    let d = Dispatcher::new(f.container.clone(), buf, "", bridge);
    assert_eq!(d.handle("ping", json!({})).unwrap()["pong"], true);
    d.shutdown();
}
