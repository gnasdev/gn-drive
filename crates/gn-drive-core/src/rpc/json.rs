//! JSON encoding of models and loose value coercions (port of `RPCJSON.swift`).

use crate::auth::{AppSettings, AuthStatus};
use crate::event_bus::{RuntimeSnapshotEvent, SyncProgressEvent};
use crate::models::{
    Board, BoardEdge, BoardNode, Flow, FlowOperation, HistoryEntry, HistoryStats, Profile, Schedule,
};
use crate::rclone::types::{FileEntry, Remote, SyncStats};
use crate::sync_engine::TaskSnapshot;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};

pub fn rfc3339(d: DateTime<Utc>) -> String {
    crate::auth::rfc3339(d)
}

pub fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

pub fn millis(d: DateTime<Utc>) -> i64 {
    d.timestamp_millis()
}

// MARK: coercions ---------------------------------------------------------

/// Lenient int: numbers (floats truncate), numeric strings; never booleans.
pub fn int_value(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

pub fn double_value(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

pub fn bool_value(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => Some(n.as_f64().map(|f| f != 0.0).unwrap_or(true)),
        Value::String(s) => match s.to_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// Case-tolerant lookup for rclone JSON (PascalCase in 1.75, lowercase older).
pub fn field<'a>(dict: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    if let Some(v) = dict.get(name) {
        return Some(v);
    }
    let mut chars = name.chars();
    if let Some(first) = chars.next() {
        let lowered: String = first.to_lowercase().chain(chars).collect();
        if let Some(v) = dict.get(&lowered) {
            return Some(v);
        }
    }
    let lname = name.to_lowercase();
    dict.iter()
        .find(|(k, _)| k.to_lowercase() == lname)
        .map(|(_, v)| v)
}

pub fn string_of(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(other) => other.to_string(),
    }
}

pub fn parse_object(s: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(s) {
        Ok(Value::Object(m)) => Some(m),
        _ => None,
    }
}

pub fn stringify_json(m: &Map<String, Value>) -> String {
    serde_json::to_string(m).unwrap_or_else(|_| "{}".into())
}

fn s_or_empty<'a>(m: &'a Map<String, Value>, k: &str) -> Option<&'a Vec<Value>> {
    field(m, k).and_then(Value::as_array)
}

// MARK: models ------------------------------------------------------------

pub fn auth_status(s: &AuthStatus) -> Value {
    json!({
        "setup": s.setup,
        "unlocked": s.unlocked,
        "lockedAt": s.locked_at.map(rfc3339),
        "enabled": s.enabled,
        "lockout": {
            "failedAttempts": s.lockout.failed_attempts,
            "lockedUntil": s.lockout.locked_until,
            "isLocked": s.lockout.is_locked,
            "retryAfterSecs": s.lockout.retry_after_secs,
        },
    })
}

pub fn app_settings(s: &AppSettings) -> Value {
    json!({
        "notificationsEnabled": s.notifications_enabled,
        "debugMode": s.debug_mode,
        "minimizeToTray": s.minimize_to_tray,
        "startAtLogin": s.start_at_login,
    })
}

pub fn profile(p: &Profile) -> Value {
    serde_json::to_value(p).unwrap_or_else(|_| Value::Object(Map::new()))
}

pub fn remote(r: &Remote) -> Value {
    json!({ "name": r.name, "type": r.r#type, "description": r.description })
}

pub fn file_entry(e: &FileEntry) -> Value {
    json!({
        "name": e.name, "size": e.size, "mimeType": e.mime_type,
        "isDir": e.is_dir, "modTime": e.mod_time, "path": e.path, "id": e.id,
    })
}

pub fn schedule(s: &Schedule) -> Value {
    json!({
        "id": s.id, "profileName": s.profile_name, "action": s.action,
        "cron": s.cron, "enabled": s.enabled, "lastRun": s.last_run,
        "nextRun": s.next_run, "lastResult": s.last_result, "createdAt": s.created_at,
    })
}

pub fn history(e: &HistoryEntry) -> Value {
    json!({
        "id": e.id, "profileName": e.profile_name, "action": e.action,
        "state": e.state, "startedAt": e.started_at, "finishedAt": e.finished_at,
        "duration": e.duration, "bytes": e.bytes, "errors": e.errors,
        "files": e.files, "errorMessage": e.error_message,
    })
}

pub fn history_stats(s: &HistoryStats) -> Value {
    let mut by = Map::new();
    for (k, v) in &s.by_profile {
        by.insert(
            k.clone(),
            json!({ "syncs": v.syncs, "bytes": v.bytes, "duration": v.duration, "errors": v.errors }),
        );
    }
    json!({
        "totalSyncs": s.total_syncs, "totalBytes": s.total_bytes,
        "totalDuration": s.total_duration, "totalErrors": s.total_errors,
        "byProfile": Value::Object(by),
    })
}

pub fn operation(op: &FlowOperation) -> Value {
    json!({
        "id": op.id, "flowID": op.flow_id,
        "sourceRemote": op.source_remote, "sourcePath": op.source_path,
        "targetRemote": op.target_remote, "targetPath": op.target_path,
        "action": op.resolved_action(),
        "syncConfig": Value::Object(op.sync_config.clone()),
        "isExpanded": op.is_expanded, "sortOrder": op.sort_order,
    })
}

pub fn flow(f: &Flow) -> Value {
    json!({
        "id": f.id, "name": f.name, "isCollapsed": f.is_collapsed,
        "scheduleEnabled": f.schedule_enabled, "scheduleCron": f.schedule_cron,
        "sortOrder": f.sort_order,
        "operations": f.operations.iter().map(operation).collect::<Vec<_>>(),
        "canvasJSON": f.canvas_json,
        "canvas": Value::Object(parse_object(&f.canvas_json).unwrap_or_default()),
        "createdAt": f.created_at, "updatedAt": f.updated_at,
    })
}

pub fn board_node(n: &BoardNode) -> Value {
    json!({ "id": n.id, "remoteName": n.remote_name, "path": n.path,
            "label": n.label, "x": n.x, "y": n.y })
}

pub fn board_edge(e: &BoardEdge) -> Value {
    json!({ "id": e.id, "sourceID": e.source_id, "targetID": e.target_id,
            "action": e.action,
            "syncConfig": Value::Object(parse_object(&e.sync_config).unwrap_or_default()) })
}

pub fn board(b: &Board) -> Value {
    json!({
        "id": b.id, "name": b.name,
        "createdAt": b.created_at, "updatedAt": b.updated_at,
        "nodes": b.nodes.iter().map(board_node).collect::<Vec<_>>(),
        "edges": b.edges.iter().map(board_edge).collect::<Vec<_>>(),
    })
}

pub fn sync_stats(s: &SyncStats) -> Value {
    json!({
        "bytes": s.bytes, "bytesTotal": s.bytes_total,
        "files": s.files, "filesTotal": s.files_total,
        "transfers": s.transfers, "errors": s.errors,
        "checks": s.checks, "checksTotal": s.checks_total,
        "deletes": s.deletes, "renames": s.renames,
        "speed": s.speed, "eta": s.eta,
        "currentFile": s.current_file, "stage": s.stage,
        "stageDetail": s.stage_detail, "lastUpdate": s.last_update,
        "fileTransfers": s.file_transfers.iter().map(|t| serde_json::to_value(t).unwrap_or(Value::Null)).collect::<Vec<_>>(),
    })
}

pub fn task(t: &TaskSnapshot) -> Value {
    json!({
        "id": t.id, "name": t.name, "action": t.action, "status": t.status,
        "stats": sync_stats(&t.stats),
        "startedAt": rfc3339(t.started_at),
        "endedAt": t.ended_at.map(rfc3339),
    })
}

pub fn sync_progress(e: &SyncProgressEvent) -> Value {
    json!({
        "taskID": e.task_id, "profileID": e.profile_id, "action": e.action,
        "state": e.state, "transferred": e.transferred, "total": e.total,
        "bytesPerSec": e.bytes_per_sec, "eta": e.eta, "errors": e.errors,
        "currentFile": e.current_file, "stage": e.stage, "stageDetail": e.stage_detail,
        "filesTransferred": e.files_transferred, "totalFiles": e.total_files,
        "checks": e.checks, "totalChecks": e.total_checks,
        "deletes": e.deletes, "renames": e.renames,
        "transfers": e.transfers.iter().map(|t| serde_json::to_value(t).unwrap_or(Value::Null)).collect::<Vec<_>>(),
        "errorMessage": e.error_message,
    })
}

pub fn runtime_snapshot(s: &RuntimeSnapshotEvent) -> Value {
    let flows: Vec<Value> = s
        .flows
        .iter()
        .map(|f| {
            let mut d = json!({
                "id": f.id,
                "status": if f.status.is_empty() { "idle" } else { f.status.as_str() },
                "lastError": f.last_error,
                "ops": f.ops.iter().map(|o| json!({"id": o.id, "status": o.status, "lastError": o.last_error})).collect::<Vec<_>>(),
                "log": f.log.iter().map(|l| json!({"at": l.at, "status": l.status, "opID": l.op_id, "error": l.error, "label": l.label})).collect::<Vec<_>>(),
            });
            if let Some(sync) = &f.sync {
                d["sync"] = sync_progress(sync);
            }
            d
        })
        .collect();
    json!({ "revision": s.revision, "flows": flows })
}

// MARK: rclone payloads ---------------------------------------------------

pub fn normalized_question(raw: &Map<String, Value>) -> Value {
    let state = field(raw, "State")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let error = field(raw, "Error")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let result = field(raw, "Result")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let option = match field(raw, "Option").and_then(Value::as_object) {
        Some(o) => normalized_option(o),
        None => Value::Null,
    };
    json!({ "state": state, "error": error, "result": result, "option": option })
}

fn examples(o: &Map<String, Value>) -> Vec<Value> {
    s_or_empty(o, "Examples")
        .map(|a| {
            a.iter()
                .filter_map(Value::as_object)
                .map(|e| json!({ "value": string_of(field(e, "Value")), "help": string_of(field(e, "Help")) }))
                .collect()
        })
        .unwrap_or_default()
}

fn flag(o: &Map<String, Value>, k: &str) -> bool {
    field(o, k).and_then(bool_value).unwrap_or(false)
}

fn normalized_option(o: &Map<String, Value>) -> Value {
    let default = field(o, "Default")
        .or_else(|| field(o, "DefaultStr"))
        .cloned()
        .unwrap_or_else(|| Value::String(String::new()));
    json!({
        "name": string_of(field(o, "Name")),
        "help": string_of(field(o, "Help")),
        "default": default,
        "required": flag(o, "Required"),
        "type": string_of(field(o, "Type")),
        "examples": examples(o),
    })
}

pub fn provider(raw: &Map<String, Value>) -> Value {
    let options: Vec<&Map<String, Value>> = s_or_empty(raw, "Options")
        .map(|a| a.iter().filter_map(Value::as_object).collect())
        .unwrap_or_default();
    let oauth = options
        .iter()
        .any(|o| string_of(field(o, "Name")) == "auth_url");
    json!({
        "name": string_of(field(raw, "Name")),
        "description": string_of(field(raw, "Description")),
        "prefix": string_of(field(raw, "Prefix")),
        "hidden": flag(raw, "Hide"),
        "oauth": oauth,
        "options": options.iter().map(|o| json!({
            "name": string_of(field(o, "Name")),
            "help": string_of(field(o, "Help")),
            "required": flag(o, "Required"),
            "isPassword": flag(o, "IsPassword"),
            "sensitive": flag(o, "Sensitive"),
            "advanced": flag(o, "Advanced"),
            "hide": field(o, "Hide").and_then(int_value).unwrap_or(0),
            "type": string_of(field(o, "Type")),
            "default": string_of(field(o, "DefaultStr")),
            "examples": examples(o),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coercions() {
        assert_eq!(int_value(&json!(5)), Some(5));
        assert_eq!(int_value(&json!(" 7 ")), Some(7));
        assert_eq!(int_value(&json!(true)), None);
        assert_eq!(int_value(&json!(2.9)), Some(2));
        assert_eq!(bool_value(&json!("Yes")), Some(true));
        assert_eq!(bool_value(&json!(0)), Some(false));
        assert_eq!(bool_value(&json!("maybe")), None);
        assert_eq!(string_of(Some(&json!(true))), "true");
        assert_eq!(string_of(Some(&json!(3))), "3");
    }

    #[test]
    fn field_is_case_tolerant() {
        let m = parse_object(r#"{"name":"a","TYPE":"b"}"#).unwrap();
        assert_eq!(string_of(field(&m, "Name")), "a");
        assert_eq!(string_of(field(&m, "Type")), "b");
        assert!(field(&m, "Zip").is_none());
    }

    #[test]
    fn provider_detects_oauth() {
        let m = parse_object(
            r#"{"Name":"drive","Description":"d","Prefix":"drive","Options":[{"Name":"auth_url","Hide":3,"Required":true}]}"#,
        )
        .unwrap();
        let p = provider(&m);
        assert_eq!(p["oauth"], true);
        assert_eq!(p["options"][0]["hide"], 3);
        assert_eq!(p["options"][0]["required"], true);
    }

    #[test]
    fn question_normalizes() {
        let m =
            parse_object(r#"{"State":"s","Option":{"Name":"n","Required":"true","Default":5}}"#)
                .unwrap();
        let q = normalized_question(&m);
        assert_eq!(q["state"], "s");
        assert_eq!(q["option"]["required"], true);
        assert_eq!(q["option"]["default"], 5);
        assert!(normalized_question(&Map::new())["option"].is_null());
    }
}
