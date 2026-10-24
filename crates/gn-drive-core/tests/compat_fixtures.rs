//! Swift-generated fixtures (see `scripts/gen-compat-fixtures.sh`) must unlock
//! and read back identically. Password and rclone material here are fixtures
//! (`compat-test-pw-123`, a fake S3 key), never the user's config or keychain.

use gn_drive_core::auth::AuthService;
use gn_drive_core::keychain::MemorySecureStore;
use gn_drive_core::logger::Logger;
use gn_drive_core::models::{Board, Flow, HistoryEntry, HistoryStats, Profile, Schedule};
use gn_drive_core::store::Store;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const PASSWORD: &str = "compat-test-pw-123";
const NULLABLE_PROFILE_KEYS: &[&str] = &[
    "maxDepth",
    "maxDelete",
    "multiThreadStreams",
    "retries",
    "lowLevelRetries",
    "tpsLimit",
];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for ent in fs::read_dir(src).unwrap() {
        let ent = ent.unwrap();
        let to = dst.join(ent.file_name());
        if ent.file_type().unwrap().is_dir() {
            copy_dir(&ent.path(), &to);
        } else {
            fs::copy(ent.path(), &to).unwrap();
        }
    }
}

fn dump_store(store: &Store) -> Value {
    let profiles: Vec<Value> = store
        .list_profiles()
        .unwrap()
        .into_iter()
        .map(|p| profile_value(&p))
        .collect();
    let schedules: Vec<Value> = store
        .list_schedules()
        .unwrap()
        .into_iter()
        .map(|s| schedule_value(&s))
        .collect();
    let history: Vec<Value> = store
        .list_history(1000, 0)
        .unwrap()
        .into_iter()
        .map(|h| history_value(&h))
        .collect();
    let stats = store.history_stats().unwrap();
    let mut settings = serde_json::Map::new();
    for k in [
        "theme",
        "notifications",
        "legacy_flag",
        "language",
        "empty_value",
    ] {
        settings.insert(k.into(), Value::String(store.get_setting(k).unwrap()));
    }
    let boards: Vec<Value> = store
        .list_boards()
        .unwrap()
        .into_iter()
        .map(|b| {
            let g = store.load_board_graph(&b.id).unwrap();
            board_value(&g)
        })
        .collect();
    let flows: Vec<Value> = store
        .list_flows()
        .unwrap()
        .into_iter()
        .map(|f| {
            let full = store.get_flow(&f.id).unwrap();
            flow_value(&full)
        })
        .collect();
    let mut deltas = Vec::new();
    for k in ["gdrive:root", "local:/tmp/x"] {
        let d = store.get_delta_state(k).unwrap();
        deltas.push(json!({
            "remoteKey": d.remote_key,
            "provider": d.provider,
            "lastFullSync": d.last_full_sync,
            "deltaCount": d.delta_count,
            "isWatching": d.is_watching,
        }));
    }
    json!({
        "profiles": profiles,
        "schedules": schedules,
        "history": history,
        "historyStats": stats_value(&stats),
        "settings": settings,
        "boards": boards,
        "flows": flows,
        "deltaStates": deltas,
    })
}

fn profile_value(p: &Profile) -> Value {
    let mut v = serde_json::to_value(p).unwrap();
    if let Some(obj) = v.as_object_mut() {
        for k in NULLABLE_PROFILE_KEYS {
            obj.entry((*k).to_string()).or_insert(Value::Null);
        }
    }
    v
}

fn schedule_value(s: &Schedule) -> Value {
    json!({
        "id": s.id,
        "profileName": s.profile_name,
        "action": s.action,
        "cron": s.cron,
        "enabled": s.enabled,
        "lastRun": s.last_run,
        "nextRun": s.next_run,
        "lastResult": s.last_result,
        "createdAt": s.created_at,
    })
}

fn history_value(h: &HistoryEntry) -> Value {
    json!({
        "id": h.id,
        "profileName": h.profile_name,
        "action": h.action,
        "state": h.state,
        "startedAt": h.started_at,
        "finishedAt": h.finished_at,
        "duration": h.duration,
        "bytes": h.bytes,
        "errors": h.errors,
        "files": h.files,
        "errorMessage": h.error_message,
    })
}

fn stats_value(s: &HistoryStats) -> Value {
    let mut by = serde_json::Map::new();
    for (name, p) in &s.by_profile {
        by.insert(
            name.clone(),
            json!({"syncs": p.syncs, "bytes": p.bytes, "duration": p.duration, "errors": p.errors}),
        );
    }
    json!({
        "totalSyncs": s.total_syncs,
        "totalBytes": s.total_bytes,
        "totalDuration": s.total_duration,
        "totalErrors": s.total_errors,
        "byProfile": by,
    })
}

fn board_value(b: &Board) -> Value {
    json!({
        "id": b.id,
        "name": b.name,
        "createdAt": b.created_at,
        "updatedAt": b.updated_at,
        "nodes": b.nodes.iter().map(|n| json!({
            "id": n.id,
            "remoteName": n.remote_name,
            "path": n.path,
            "label": n.label,
            "x": n.x,
            "y": n.y,
        })).collect::<Vec<_>>(),
        "edges": b.edges.iter().map(|e| json!({
            "id": e.id,
            "sourceID": e.source_id,
            "targetID": e.target_id,
            "action": e.action,
            "syncConfig": e.sync_config,
        })).collect::<Vec<_>>(),
    })
}

fn flow_value(f: &Flow) -> Value {
    json!({
        "id": f.id,
        "name": f.name,
        "isCollapsed": f.is_collapsed,
        "scheduleEnabled": f.schedule_enabled,
        "scheduleCron": f.schedule_cron,
        "sortOrder": f.sort_order,
        "canvasJSON": f.canvas_json,
        "createdAt": f.created_at,
        "updatedAt": f.updated_at,
        "operations": f.operations.iter().map(|o| json!({
            "id": o.id,
            "flowID": o.flow_id,
            "sourceRemote": o.source_remote,
            "sourcePath": o.source_path,
            "targetRemote": o.target_remote,
            "targetPath": o.target_path,
            "action": o.action,
            "syncConfig": o.sync_config,
            "isExpanded": o.is_expanded,
            "sortOrder": o.sort_order,
        })).collect::<Vec<_>>(),
    })
}

fn assert_same(section: &str, got: &Value, expect: &Value) {
    let mut diffs = Vec::new();
    diff(section, got, expect, &mut diffs);
    assert!(
        diffs.is_empty(),
        "{section} drifted from the Swift fixture:\n{}",
        diffs.join("\n")
    );
}

fn diff(path: &str, got: &Value, expect: &Value, out: &mut Vec<String>) {
    match (got, expect) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<String> = a.keys().cloned().chain(b.keys().cloned()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let av = a.get(&k).unwrap_or(&Value::Null);
                let bv = b.get(&k).unwrap_or(&Value::Null);
                if av.is_null() && bv.is_null() {
                    continue;
                }
                diff(&format!("{path}.{k}"), av, bv, out);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!("{path} len {} vs {}", a.len(), b.len()));
                return;
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                diff(&format!("{path}[{i}]"), x, y, out);
            }
        }
        (Value::Number(x), Value::Number(y)) => {
            let xf = x.as_f64().unwrap_or(f64::NAN);
            let yf = y.as_f64().unwrap_or(f64::NAN);
            if (xf - yf).abs() > 1e-9 {
                out.push(format!("{path}: {x} != {y}"));
            }
        }
        (Value::String(x), Value::String(y)) if x != y => {
            out.push(format!("{path}: {x:?} != {y:?}"));
        }
        (x, y) if x != y => out.push(format!("{path}: {x} != {y}")),
        _ => {}
    }
}

fn open_locked(dir: &Path) -> (AuthService, Store) {
    let ks = Arc::new(MemorySecureStore::new());
    let auth = AuthService::new(dir.to_str().unwrap(), &Logger::quiet(), Some(ks)).unwrap();
    auth.unlock(PASSWORD)
        .expect("fixture password unlocks Swift auth.json");
    let db = dir.join("gn-drive.db");
    let store = Store::open(db.to_str().unwrap(), &Logger::quiet()).unwrap();
    (auth, store)
}

#[test]
fn swift_locked_fixture_unlocks_and_matches_expected() {
    let fix = fixtures_dir();
    let expected: Value =
        serde_json::from_str(&fs::read_to_string(fix.join("expected.json")).unwrap()).unwrap();
    let hash = fs::read_to_string(fix.join("expected/auth.json.hash"))
        .unwrap()
        .trim()
        .to_string();
    assert!(
        gn_drive_core::argon2id::verify(PASSWORD, &hash),
        "fixture argon2id hash does not verify"
    );
    assert_eq!(expected["passwordHash"], hash);

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("locked");
    copy_dir(&fix.join("locked"), &dir);

    let auth_json: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("auth.json")).unwrap()).unwrap();
    assert_eq!(auth_json["password_hash"], hash);
    let keys: Vec<&String> = auth_json.as_object().unwrap().keys().collect();
    let mut expect_keys: Vec<String> = expected["authJSONKeys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let mut got_keys: Vec<String> = keys.iter().map(|s| (*s).clone()).collect();
    expect_keys.sort();
    got_keys.sort();
    assert_eq!(got_keys, expect_keys);

    let (auth, store) = open_locked(&dir);
    assert!(auth.is_unlocked());
    let rc = fs::read_to_string(dir.join("rclone.conf")).unwrap();
    let expect_rc = fs::read_to_string(fix.join("expected/rclone.conf")).unwrap();
    assert_eq!(rc, expect_rc);
    assert!(!dir.join("rclone.conf.enc").exists());
    assert!(!dir.join("gn-drive.db.enc").exists());

    let got = dump_store(&store);
    for section in [
        "profiles",
        "schedules",
        "history",
        "historyStats",
        "settings",
        "boards",
        "flows",
        "deltaStates",
    ] {
        assert_same(section, &got[section], &expected[section]);
    }

    // Re-encrypt with the same key and read the ciphertext back.
    store.close();
    auth.lock().unwrap();
    assert!(dir.join("gn-drive.db.enc").exists());
    assert!(dir.join("rclone.conf.enc").exists());
    assert!(!dir.join("gn-drive.db").exists());
    let (_auth2, store2) = open_locked(&dir);
    let again = dump_store(&store2);
    for section in [
        "profiles",
        "schedules",
        "history",
        "historyStats",
        "settings",
        "boards",
        "flows",
        "deltaStates",
    ] {
        assert_same(
            &format!("relock.{section}"),
            &again[section],
            &expected[section],
        );
    }
    let rc2 = fs::read_to_string(dir.join("rclone.conf")).unwrap();
    assert_eq!(rc2, expect_rc);
}

#[test]
fn swift_plain_fixture_reads_without_a_password() {
    let fix = fixtures_dir();
    let expected: Value =
        serde_json::from_str(&fs::read_to_string(fix.join("expected.json")).unwrap()).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("plain");
    copy_dir(&fix.join("plain"), &dir);
    let store = Store::open(dir.join("gn-drive.db").to_str().unwrap(), &Logger::quiet()).unwrap();
    let got = dump_store(&store);
    for section in [
        "profiles",
        "schedules",
        "history",
        "historyStats",
        "settings",
        "boards",
        "flows",
        "deltaStates",
    ] {
        assert_same(section, &got[section], &expected[section]);
    }
    let rc = fs::read_to_string(dir.join("rclone.conf")).unwrap();
    assert_eq!(
        rc,
        fs::read_to_string(fix.join("expected/rclone.conf")).unwrap()
    );
}

/// Rust re-encrypts the Swift fixture; the Swift verifier must unlock it.
/// Not part of `cargo test` — it builds the Swift package. Run with:
/// `cargo test -p gn-drive-core --test compat_fixtures -- --ignored swift_reads_rust_ciphertext`
#[test]
#[ignore = "builds and runs the Swift verifier"]
fn swift_reads_rust_ciphertext() {
    let fix = fixtures_dir();
    let expected: Value =
        serde_json::from_str(&fs::read_to_string(fix.join("expected.json")).unwrap()).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("locked");
    copy_dir(&fix.join("locked"), &dir);
    let (auth, store) = open_locked(&dir);
    store.close();
    auth.lock().unwrap();

    let verify = tmp.path().join("verify");
    copy_dir(&dir, &verify);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = root.join("scripts/gen-compat-fixtures.sh");
    let out = std::process::Command::new(&script)
        .arg("--verify")
        .arg(&verify)
        .current_dir(&root)
        .output()
        .expect("spawn swift verifier");
    assert!(
        out.status.success(),
        "swift verifier failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let dump: Value =
        serde_json::from_str(&fs::read_to_string(verify.join("swift-dump.json")).unwrap()).unwrap();
    assert_eq!(dump["authSetup"], true);
    assert_eq!(
        dump["rcloneConf"],
        fs::read_to_string(fix.join("expected/rclone.conf")).unwrap()
    );
    for section in [
        "profiles",
        "schedules",
        "history",
        "historyStats",
        "settings",
        "boards",
        "flows",
        "deltaStates",
    ] {
        assert_same(
            &format!("swift.{section}"),
            &dump[section],
            &expected[section],
        );
    }
}
