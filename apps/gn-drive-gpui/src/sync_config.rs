//! Operation sync options — the ~40 rclone flags the inspector edits.
//!
//! The stored object keeps its unknown keys. Known keys accept both spellings
//! on read and are written in both on save, like the SwiftUI editor.

use serde_json::{Value, json};

use crate::engine::JsonMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Int,
    Double,
    Text,
    Bool,
    List,
    Select(&'static [(&'static str, &'static str)]),
}

pub struct FieldDef {
    pub key: &'static str,
    pub camel: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub kind: FieldKind,
}

const DELETE_TIMINGS: &[(&str, &str)] = &[
    ("", "Default"),
    ("before", "Before"),
    ("during", "During"),
    ("after", "After"),
];

const CONFLICT_RESOLUTIONS: &[(&str, &str)] = &[
    ("none", "None"),
    ("path1", "Path1 wins"),
    ("path2", "Path2 wins"),
    ("newer", "Newer"),
    ("older", "Older"),
    ("larger", "Larger"),
    ("smaller", "Smaller"),
];

/// Historical rclone bucket named at each `field!` site.
///
/// Inspector sections live in `inspector`. The bucket is not stored: reading
/// it would compete with that table.
#[derive(Clone, Copy)]
enum FieldGroup {
    Performance,
    Filtering,
    Safety,
    Comparison,
    Sync,
    Bisync,
}

macro_rules! field {
    ($key:literal, $camel:literal, $label:literal, $hint:literal, $kind:expr, $group:expr) => {{
        // `$group` names the historical bucket. Sections live in `inspector`.
        let _group: FieldGroup = $group;
        FieldDef {
            key: $key,
            camel: $camel,
            label: $label,
            hint: $hint,
            kind: $kind,
        }
    }};
}

pub static FIELDS: &[FieldDef] = &[
    field!(
        "parallel",
        "parallel",
        "Parallel transfers",
        "4",
        FieldKind::Int,
        FieldGroup::Performance
    ),
    field!(
        "bandwidth",
        "bandwidth",
        "Bandwidth (MB/s)",
        "0",
        FieldKind::Int,
        FieldGroup::Performance
    ),
    field!(
        "multi_thread_streams",
        "multiThreadStreams",
        "Multi-thread streams",
        "4",
        FieldKind::Int,
        FieldGroup::Performance
    ),
    field!(
        "buffer_size",
        "bufferSize",
        "Buffer size",
        "16M",
        FieldKind::Text,
        FieldGroup::Performance
    ),
    field!(
        "retries",
        "retries",
        "Retries",
        "3",
        FieldKind::Int,
        FieldGroup::Performance
    ),
    field!(
        "low_level_retries",
        "lowLevelRetries",
        "Low-level retries",
        "10",
        FieldKind::Int,
        FieldGroup::Performance
    ),
    field!(
        "max_duration",
        "maxDuration",
        "Max duration",
        "24h",
        FieldKind::Text,
        FieldGroup::Performance
    ),
    field!(
        "check_first",
        "checkFirst",
        "Check first",
        "",
        FieldKind::Bool,
        FieldGroup::Performance
    ),
    field!(
        "order_by",
        "orderBy",
        "Order by",
        "size,ascending",
        FieldKind::Text,
        FieldGroup::Performance
    ),
    field!(
        "retries_sleep",
        "retriesSleep",
        "Retries sleep",
        "0s",
        FieldKind::Text,
        FieldGroup::Performance
    ),
    field!(
        "tps_limit",
        "tpsLimit",
        "TPS limit",
        "0",
        FieldKind::Double,
        FieldGroup::Performance
    ),
    field!(
        "conn_timeout",
        "connTimeout",
        "Connect timeout",
        "60s",
        FieldKind::Text,
        FieldGroup::Performance
    ),
    field!(
        "io_timeout",
        "ioTimeout",
        "I/O timeout",
        "300s",
        FieldKind::Text,
        FieldGroup::Performance
    ),
    field!(
        "included_paths",
        "includedPaths",
        "Include paths",
        "one path per line",
        FieldKind::List,
        FieldGroup::Filtering
    ),
    field!(
        "excluded_paths",
        "excludedPaths",
        "Exclude paths",
        "one path per line",
        FieldKind::List,
        FieldGroup::Filtering
    ),
    field!(
        "min_size",
        "minSize",
        "Min size",
        "1M",
        FieldKind::Text,
        FieldGroup::Filtering
    ),
    field!(
        "max_size",
        "maxSize",
        "Max size",
        "1G",
        FieldKind::Text,
        FieldGroup::Filtering
    ),
    field!(
        "max_age",
        "maxAge",
        "Max age",
        "24h",
        FieldKind::Text,
        FieldGroup::Filtering
    ),
    field!(
        "min_age",
        "minAge",
        "Min age",
        "0s",
        FieldKind::Text,
        FieldGroup::Filtering
    ),
    field!(
        "max_depth",
        "maxDepth",
        "Max depth",
        "0",
        FieldKind::Int,
        FieldGroup::Filtering
    ),
    field!(
        "filter_from_file",
        "filterFromFile",
        "Filter from file",
        "/path/to/filters",
        FieldKind::Text,
        FieldGroup::Filtering
    ),
    field!(
        "exclude_if_present",
        "excludeIfPresent",
        "Exclude if present",
        ".ignore",
        FieldKind::Text,
        FieldGroup::Filtering
    ),
    field!(
        "use_regex",
        "useRegex",
        "Use regex",
        "",
        FieldKind::Bool,
        FieldGroup::Filtering
    ),
    field!(
        "delete_excluded",
        "deleteExcluded",
        "Delete excluded",
        "",
        FieldKind::Bool,
        FieldGroup::Filtering
    ),
    field!(
        "dry_run",
        "dryRun",
        "Dry run",
        "",
        FieldKind::Bool,
        FieldGroup::Safety
    ),
    field!(
        "max_delete",
        "maxDelete",
        "Max delete",
        "0",
        FieldKind::Int,
        FieldGroup::Safety
    ),
    field!(
        "immutable",
        "immutable",
        "Immutable",
        "",
        FieldKind::Bool,
        FieldGroup::Safety
    ),
    field!(
        "max_transfer",
        "maxTransfer",
        "Max transfer",
        "100G",
        FieldKind::Text,
        FieldGroup::Safety
    ),
    field!(
        "max_delete_size",
        "maxDeleteSize",
        "Max delete size",
        "1G",
        FieldKind::Text,
        FieldGroup::Safety
    ),
    field!(
        "suffix",
        "suffix",
        "Suffix",
        ".bak",
        FieldKind::Text,
        FieldGroup::Safety
    ),
    field!(
        "suffix_keep_extension",
        "suffixKeepExtension",
        "Suffix keeps extension",
        "",
        FieldKind::Bool,
        FieldGroup::Safety
    ),
    field!(
        "backup_path",
        "backupPath",
        "Backup path",
        "remote:backup",
        FieldKind::Text,
        FieldGroup::Safety
    ),
    field!(
        "size_only",
        "sizeOnly",
        "Size only",
        "",
        FieldKind::Bool,
        FieldGroup::Comparison
    ),
    field!(
        "update_mode",
        "updateMode",
        "Update mode",
        "",
        FieldKind::Bool,
        FieldGroup::Comparison
    ),
    field!(
        "ignore_existing",
        "ignoreExisting",
        "Ignore existing",
        "",
        FieldKind::Bool,
        FieldGroup::Comparison
    ),
    field!(
        "delete_timing",
        "deleteTiming",
        "Delete timing",
        "",
        FieldKind::Select(DELETE_TIMINGS),
        FieldGroup::Sync
    ),
    field!(
        "conflict_resolution",
        "conflictResolution",
        "Conflict resolution",
        "",
        FieldKind::Select(CONFLICT_RESOLUTIONS),
        FieldGroup::Bisync
    ),
    field!(
        "resilient",
        "resilient",
        "Resilient",
        "",
        FieldKind::Bool,
        FieldGroup::Bisync
    ),
    field!(
        "max_lock",
        "maxLock",
        "Max lock",
        "2m",
        FieldKind::Text,
        FieldGroup::Bisync
    ),
    field!(
        "check_access",
        "checkAccess",
        "Check access",
        "",
        FieldKind::Bool,
        FieldGroup::Bisync
    ),
    field!(
        "conflict_loser",
        "conflictLoser",
        "Conflict loser",
        "num",
        FieldKind::Text,
        FieldGroup::Bisync
    ),
    field!(
        "conflict_suffix",
        "conflictSuffix",
        "Conflict suffix",
        ".conflict1",
        FieldKind::Text,
        FieldGroup::Bisync
    ),
];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncConfig {
    pub map: JsonMap,
}

impl SyncConfig {
    pub fn parse(raw: &JsonMap, fallback_action: &str) -> Self {
        let mut map = raw.clone();
        let action = normalize_action(
            &string_from(&map, "action").unwrap_or_else(|| fallback_action.to_string()),
        );
        for def in FIELDS {
            let value = match def.kind {
                FieldKind::Int => int_from(&map, def.key, def.camel).map(|v| json!(v)),
                FieldKind::Double => double_from(&map, def.key, def.camel).map(|v| json!(v)),
                FieldKind::Text | FieldKind::Select(_) => {
                    string_from_alias(&map, def.key, def.camel).map(|v| json!(v))
                }
                FieldKind::Bool => bool_from(&map, def.key, def.camel).map(|v| json!(v)),
                FieldKind::List => list_from(&map, def.key, def.camel).map(|v| json!(v)),
            };
            map.remove(def.key);
            map.remove(def.camel);
            if let Some(value) = value {
                map.insert(def.key.into(), value);
            }
        }
        map.insert("action".into(), json!(action));
        Self { map }
    }

    pub fn action(&self) -> String {
        normalize_action(
            self.map
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("push"),
        )
    }

    pub fn set_action(&mut self, action: &str) {
        let action = normalize_action(action);
        self.map.insert("action".into(), json!(action));
    }

    pub fn int(&self, key: &str) -> Option<i64> {
        let def = FIELDS.iter().find(|def| def.key == key);
        match def {
            Some(def) => int_from(&self.map, def.key, def.camel),
            None => int_from(&self.map, key, key),
        }
    }

    pub fn double(&self, key: &str) -> Option<f64> {
        let def = FIELDS.iter().find(|def| def.key == key);
        match def {
            Some(def) => double_from(&self.map, def.key, def.camel),
            None => double_from(&self.map, key, key),
        }
    }

    pub fn text(&self, key: &str) -> String {
        let def = FIELDS.iter().find(|def| def.key == key);
        match def {
            Some(def) => string_from_alias(&self.map, def.key, def.camel).unwrap_or_default(),
            None => string_from(&self.map, key).unwrap_or_default(),
        }
    }

    pub fn flag(&self, key: &str) -> bool {
        let def = FIELDS.iter().find(|def| def.key == key);
        match def {
            Some(def) => bool_from(&self.map, def.key, def.camel).unwrap_or(false),
            None => bool_from(&self.map, key, key).unwrap_or(false),
        }
    }

    pub fn list(&self, key: &str) -> Vec<String> {
        let def = FIELDS.iter().find(|def| def.key == key);
        match def {
            Some(def) => list_from(&self.map, def.key, def.camel).unwrap_or_default(),
            None => list_from(&self.map, key, key).unwrap_or_default(),
        }
    }

    pub fn set_int(&mut self, key: &str, value: Option<i64>) {
        self.set_known(key, value.map(|value| json!(value)));
    }

    pub fn set_double(&mut self, key: &str, value: Option<f64>) {
        self.set_known(key, value.map(|value| json!(value)));
    }

    pub fn set_text(&mut self, key: &str, value: &str) {
        let value = value.trim();
        self.set_known(
            key,
            if value.is_empty() {
                None
            } else {
                Some(json!(value))
            },
        );
    }

    pub fn set_flag(&mut self, key: &str, value: bool) {
        self.set_known(key, Some(json!(value)));
    }

    pub fn set_list(&mut self, key: &str, value: Vec<String>) {
        self.set_known(
            key,
            if value.is_empty() {
                None
            } else {
                Some(json!(value))
            },
        );
    }

    fn set_known(&mut self, key: &str, value: Option<Value>) {
        let def = FIELDS.iter().find(|def| def.key == key);
        if let Some(def) = def {
            self.map.remove(def.key);
            self.map.remove(def.camel);
        }
        if let Some(value) = value {
            self.map.insert(key.into(), value);
        }
    }

    /// Values in the shape `flows.save` accepts: snake_case keys plus the
    /// camelCase aliases the SwiftUI editor writes. Unknown keys survive.
    pub fn to_json(&self) -> JsonMap {
        let mut out = self.map.clone();
        for (key, value) in &self.map {
            if let Some(def) = FIELDS.iter().find(|def| def.key == key) {
                if value.as_str().is_some_and(str::is_empty) {
                    out.remove(def.key);
                    continue;
                }
                out.insert(def.camel.into(), value.clone());
            }
        }
        out.insert("action".into(), json!(self.action()));
        out
    }

    /// One-line summary for the inspector header.
    #[cfg(test)]
    pub fn chips(&self) -> Vec<String> {
        let mut chips = Vec::new();
        if self.flag("dry_run") {
            chips.push("dry-run".into());
        }
        if let Some(parallel) = self.int("parallel")
            && parallel > 0
        {
            chips.push(format!("×{parallel}"));
        }
        if let Some(bandwidth) = self.int("bandwidth")
            && bandwidth > 0
        {
            chips.push(format!("{bandwidth}M"));
        }
        let included = self.list("included_paths").len();
        if included > 0 {
            chips.push(format!("+{included} include"));
        }
        let excluded = self.list("excluded_paths").len();
        if excluded > 0 {
            chips.push(format!("−{excluded} exclude"));
        }
        let max_age = self.text("max_age");
        if !max_age.is_empty() {
            chips.push(format!("max-age {max_age}"));
        }
        let conflict = self.text("conflict_resolution");
        if !conflict.is_empty() && conflict != "none" {
            chips.push(format!("conflict:{conflict}"));
        }
        if self.flag("immutable") {
            chips.push("immutable".into());
        }
        if self.flag("size_only") {
            chips.push("size-only".into());
        }
        chips
    }
}

pub fn normalize_action(action: &str) -> String {
    match action.trim() {
        "bi" => "bi".into(),
        "bi-resync" => "bi-resync".into(),
        _ => "push".into(),
    }
}

fn int_from(map: &JsonMap, snake: &str, camel: &str) -> Option<i64> {
    for key in [snake, camel] {
        if let Some(value) = map.get(key) {
            if let Some(value) = value.as_i64() {
                return Some(value);
            }
            if let Some(value) = value.as_f64() {
                return Some(value as i64);
            }
            if let Some(text) = value.as_str()
                && !text.trim().is_empty()
                && let Ok(value) = text.trim().parse::<i64>()
            {
                return Some(value);
            }
        }
    }
    None
}

fn double_from(map: &JsonMap, snake: &str, camel: &str) -> Option<f64> {
    for key in [snake, camel] {
        if let Some(value) = map.get(key) {
            if let Some(value) = value.as_f64() {
                return Some(value);
            }
            if let Some(text) = value.as_str()
                && !text.trim().is_empty()
                && let Ok(value) = text.trim().parse::<f64>()
            {
                return Some(value);
            }
        }
    }
    None
}

fn string_from(map: &JsonMap, key: &str) -> Option<String> {
    map.get(key).and_then(Value::as_str).map(str::to_string)
}

fn string_from_alias(map: &JsonMap, snake: &str, camel: &str) -> Option<String> {
    string_from(map, snake).or_else(|| string_from(map, camel))
}

fn bool_from(map: &JsonMap, snake: &str, camel: &str) -> Option<bool> {
    for key in [snake, camel] {
        if let Some(value) = map.get(key).and_then(Value::as_bool) {
            return Some(value);
        }
    }
    None
}

fn list_from(map: &JsonMap, snake: &str, camel: &str) -> Option<Vec<String>> {
    for key in [snake, camel] {
        if let Some(items) = map.get(key).and_then(Value::as_array) {
            let items: Vec<String> = items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            if !items.is_empty() {
                return Some(items);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_either_spelling_and_normalizes_action() {
        let raw: JsonMap = serde_json::from_value(json!({
            "action": "bi",
            "multiThreadStreams": 4,
            "dry_run": true,
            "parallel": "8",
            "included_paths": ["/a", "/b"],
        }))
        .unwrap();
        let config = SyncConfig::parse(&raw, "push");
        assert_eq!(config.action(), "bi");
        assert_eq!(config.int("multi_thread_streams"), Some(4));
        assert!(config.flag("dry_run"));
        assert_eq!(config.int("parallel"), Some(8));
        assert_eq!(config.list("included_paths"), vec!["/a", "/b"]);
    }

    #[test]
    fn saves_both_spellings_and_drops_empty() {
        let raw: JsonMap = serde_json::from_value(json!({
            "action": "push",
            "multi_thread_streams": 2,
            "unknown_key": "kept",
            "suffix": "",
        }))
        .unwrap();
        let mut config = SyncConfig::parse(&raw, "push");
        config.set_int("parallel", Some(4));
        config.set_list("excluded_paths", vec![]);
        let out = config.to_json();
        assert_eq!(out["multi_thread_streams"], 2);
        assert_eq!(out["multiThreadStreams"], 2);
        assert_eq!(out["parallel"], 4);
        assert_eq!(out["unknown_key"], "kept");
        assert!(!out.contains_key("suffix"));
        assert_eq!(out["action"], "push");
    }

    #[test]
    fn chips_describe_non_defaults() {
        let raw: JsonMap = serde_json::from_value(json!({
            "action": "bi",
            "dry_run": true,
            "parallel": 6,
            "included_paths": ["/a"],
            "conflict_resolution": "newer",
        }))
        .unwrap();
        let config = SyncConfig::parse(&raw, "push");
        let chips = config.chips();
        assert!(chips.contains(&"dry-run".to_string()));
        assert!(chips.contains(&"×6".to_string()));
        assert!(chips.contains(&"+1 include".to_string()));
        assert!(chips.contains(&"conflict:newer".to_string()));
    }

    #[test]
    fn empty_optional_is_omitted_and_false_is_kept_when_present() {
        let raw: JsonMap = serde_json::from_value(json!({"dry_run": false})).unwrap();
        let config = SyncConfig::parse(&raw, "push");
        assert!(!config.flag("dry_run"));
        assert_eq!(config.to_json()["dry_run"], false);
    }
}
