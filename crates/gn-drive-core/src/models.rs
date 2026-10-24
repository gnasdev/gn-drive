//! Persistence models (port of `Models.swift`).
//!
//! `Profile` serializes with the Swift `Codable` key names (camelCase, `None`
//! omitted) because the RPC layer emits it verbatim.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const DIRECTION_PUSH: &str = "push";
pub const DIRECTION_BI: &str = "bi";
pub const DIRECTION_BI_RESYNC: &str = "bi-resync";

pub mod profile_direction {
    use super::*;
    pub fn is_valid(d: &str) -> bool {
        matches!(
            d.trim(),
            DIRECTION_PUSH | DIRECTION_BI | DIRECTION_BI_RESYNC
        )
    }
    /// Empty or legacy values (e.g. pull) → push.
    pub fn normalize(d: &str) -> String {
        let d = d.trim();
        if is_valid(d) {
            d.to_string()
        } else {
            DIRECTION_PUSH.to_string()
        }
    }
}

pub mod flow_action {
    pub use super::profile_direction::{is_valid, normalize};
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub name: String,
    pub from: String,
    pub to: String,
    /// push | bi | bi-resync
    pub direction: String,
    pub included_paths: Vec<String>,
    pub excluded_paths: Vec<String>,
    pub bandwidth: i64,
    pub parallel: i64,
    pub backup_path: String,
    pub cache_path: String,

    pub min_size: String,
    pub max_size: String,
    pub filter_from_file: String,
    pub exclude_if_present: String,
    pub use_regex: bool,
    pub max_age: String,
    pub min_age: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<i64>,
    pub delete_excluded: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_delete: Option<i64>,
    pub immutable: bool,
    pub conflict_resolution: String,
    pub dry_run: bool,
    pub max_transfer: String,
    pub max_delete_size: String,
    pub suffix: String,
    pub suffix_keep_extension: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub multi_thread_streams: Option<i64>,
    pub buffer_size: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low_level_retries: Option<i64>,
    pub max_duration: String,
    pub check_first: bool,
    pub order_by: String,
    pub retries_sleep: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tps_limit: Option<f64>,
    pub conn_timeout: String,
    pub io_timeout: String,

    pub size_only: bool,
    pub update_mode: bool,
    pub ignore_existing: bool,

    pub delete_timing: String,

    pub resilient: bool,
    pub max_lock: String,
    pub check_access: bool,
    pub conflict_loser: String,
    pub conflict_suffix: String,

    pub fast_list: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Schedule {
    pub id: String,
    pub profile_name: String,
    pub action: String,
    pub cron: String,
    pub enabled: bool,
    pub last_run: String,
    pub next_run: String,
    pub last_result: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HistoryEntry {
    pub id: String,
    pub profile_name: String,
    pub action: String,
    pub state: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration: i64,
    pub bytes: i64,
    pub errors: i64,
    pub files: i64,
    pub error_message: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProfileStats {
    pub syncs: i64,
    pub bytes: i64,
    pub duration: i64,
    pub errors: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HistoryStats {
    pub total_syncs: i64,
    pub total_bytes: i64,
    pub total_duration: i64,
    pub total_errors: i64,
    pub by_profile: std::collections::BTreeMap<String, ProfileStats>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BoardNode {
    pub id: String,
    pub remote_name: String,
    pub path: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoardEdge {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub action: String,
    /// Raw JSON object text.
    pub sync_config: String,
}

impl Default for BoardEdge {
    fn default() -> Self {
        Self {
            id: String::new(),
            source_id: String::new(),
            target_id: String::new(),
            action: String::new(),
            sync_config: "{}".into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Board {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub nodes: Vec<BoardNode>,
    pub edges: Vec<BoardEdge>,
}

/// Untyped JSON object (operation `sync_config`).
pub type JsonDict = Map<String, Value>;

#[derive(Debug, Clone, PartialEq)]
pub struct FlowOperation {
    pub id: String,
    pub flow_id: String,
    pub source_remote: String,
    pub source_path: String,
    pub target_remote: String,
    pub target_path: String,
    pub action: String,
    pub sync_config: JsonDict,
    pub is_expanded: bool,
    pub sort_order: i64,
}

impl Default for FlowOperation {
    fn default() -> Self {
        Self {
            id: String::new(),
            flow_id: String::new(),
            source_remote: String::new(),
            source_path: "/".into(),
            target_remote: String::new(),
            target_path: "/".into(),
            action: "push".into(),
            sync_config: JsonDict::new(),
            is_expanded: false,
            sort_order: 0,
        }
    }
}

impl FlowOperation {
    /// Effective flow action (column or `sync_config.action`), normalized.
    pub fn resolved_action(&self) -> String {
        if !self.action.trim().is_empty() {
            return flow_action::normalize(&self.action);
        }
        if let Some(Value::String(a)) = self.sync_config.get("action")
            && !a.trim().is_empty()
        {
            return flow_action::normalize(a);
        }
        DIRECTION_PUSH.to_string()
    }

    /// Keep action column and `sync_config.action` aligned.
    pub fn normalize_action(&mut self) {
        let a = self.resolved_action();
        self.sync_config
            .insert("action".into(), Value::String(a.clone()));
        self.action = a;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Flow {
    pub id: String,
    pub name: String,
    pub is_collapsed: bool,
    pub schedule_enabled: bool,
    pub schedule_cron: String,
    pub sort_order: i64,
    pub operations: Vec<FlowOperation>,
    pub canvas_json: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for Flow {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            is_collapsed: false,
            schedule_enabled: false,
            schedule_cron: String::new(),
            sort_order: 0,
            operations: Vec::new(),
            canvas_json: "{}".into(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

/// Build an rclone path from remote name + path. Empty/local remote → local path.
pub fn compose_path(remote: &str, path: &str) -> String {
    let path = path.trim();
    let remote = remote.trim();
    if remote.is_empty() || remote == "local" {
        return if path.is_empty() {
            "/".into()
        } else {
            path.into()
        };
    }
    if path.is_empty() || path == "/" {
        return format!("{remote}:");
    }
    format!("{remote}:{path}")
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeltaState {
    pub remote_key: String,
    pub provider: String,
    pub last_full_sync: String,
    pub delta_count: i64,
    pub is_watching: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_normalizes_legacy() {
        assert_eq!(profile_direction::normalize("pull"), "push");
        assert_eq!(profile_direction::normalize(" bi "), "bi");
        assert_eq!(profile_direction::normalize(""), "push");
        assert!(profile_direction::is_valid("bi-resync"));
        assert!(!profile_direction::is_valid("pull"));
    }

    #[test]
    fn compose_path_cases() {
        assert_eq!(compose_path("", ""), "/");
        assert_eq!(compose_path("local", "/tmp/a"), "/tmp/a");
        assert_eq!(compose_path("gd", ""), "gd:");
        assert_eq!(compose_path("gd", "/"), "gd:");
        assert_eq!(compose_path("gd", "a/b"), "gd:a/b");
    }

    #[test]
    fn resolved_action_falls_back_to_sync_config() {
        let mut op = FlowOperation {
            action: String::new(),
            ..Default::default()
        };
        assert_eq!(op.resolved_action(), "push");
        op.sync_config
            .insert("action".into(), Value::String("bi".into()));
        assert_eq!(op.resolved_action(), "bi");
        op.normalize_action();
        assert_eq!(op.action, "bi");
        op.action = "pull".into();
        assert_eq!(op.resolved_action(), "push");
    }

    #[test]
    fn profile_json_matches_swift_codable() {
        let mut p = Profile {
            name: "n".into(),
            ..Default::default()
        };
        p.max_depth = Some(3);
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["includedPaths"], serde_json::json!([]));
        assert_eq!(v["maxDepth"], 3);
        assert!(v.get("maxDelete").is_none());
        let back: Profile = serde_json::from_value(v).unwrap();
        assert_eq!(back, p);
    }
}
