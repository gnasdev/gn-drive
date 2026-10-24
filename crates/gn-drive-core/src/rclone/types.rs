//! rclone value types (port of `RcloneTypes.swift`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RcloneAction {
    Pull,
    #[default]
    Push,
    Bi,
    BiResync,
    Copy,
    Move,
    Check,
    DryRun,
}

impl RcloneAction {
    pub fn as_str(self) -> &'static str {
        match self {
            RcloneAction::Pull => "pull",
            RcloneAction::Push => "push",
            RcloneAction::Bi => "bi",
            RcloneAction::BiResync => "bi-resync",
            RcloneAction::Copy => "copy",
            RcloneAction::Move => "move",
            RcloneAction::Check => "check",
            RcloneAction::DryRun => "dry-run",
        }
    }
    pub fn parse(s: &str) -> Option<RcloneAction> {
        Some(match s {
            "pull" => RcloneAction::Pull,
            "push" => RcloneAction::Push,
            "bi" => RcloneAction::Bi,
            "bi-resync" => RcloneAction::BiResync,
            "copy" => RcloneAction::Copy,
            "move" => RcloneAction::Move,
            "check" => RcloneAction::Check,
            "dry-run" => RcloneAction::DryRun,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileTransfer {
    pub name: String,
    pub size: i64,
    pub bytes: i64,
    /// 0-100
    pub progress: f64,
    /// transferring | completed | failed | checking | checked | pending
    pub status: String,
    pub speed: f64,
    pub error: String,
}

impl FileTransfer {
    pub fn new(name: impl Into<String>, status: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: status.into(),
            ..Default::default()
        }
    }
}

/// Progress snapshot during a sync operation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SyncStats {
    pub bytes: i64,
    pub bytes_total: i64,
    pub files: i64,
    pub files_total: i64,
    pub transfers: i64,
    pub errors: i64,
    pub checks: i64,
    pub checks_total: i64,
    pub deletes: i64,
    pub renames: i64,
    /// B/s
    pub speed: f64,
    /// seconds
    pub eta: i64,
    pub current_file: String,
    /// starting | connecting | listing | checking | transferring | retrying
    pub stage: String,
    pub stage_detail: String,
    pub last_update: i64,
    pub file_transfers: Vec<FileTransfer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyncResult {
    pub stats: SyncStats,
    pub started_at: i64,
    pub ended_at: i64,
    pub exit_code: i32,
    pub stderr: String,
}

impl Default for SyncResult {
    fn default() -> Self {
        Self {
            stats: SyncStats::default(),
            started_at: 0,
            ended_at: 0,
            exit_code: -1,
            stderr: String::new(),
        }
    }
}

/// Per-operation configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncConfig {
    pub action: RcloneAction,
    /// remote:path or local path
    pub source: String,
    pub source_remote: String,
    pub source_path: String,
    pub dest: String,
    pub dest_remote: String,
    pub dest_path: String,
    pub resync: bool,
    pub profile: Option<ProfileFlags>,
    pub stats_interval: String,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            action: RcloneAction::Push,
            source: String::new(),
            source_remote: String::new(),
            source_path: String::new(),
            dest: String::new(),
            dest_remote: String::new(),
            dest_path: String::new(),
            resync: false,
            profile: None,
            stats_interval: "1s".into(),
        }
    }
}

/// rclone flags a profile / flow sync_config can set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProfileFlags {
    pub bandwidth: String,
    pub transfers: i64,
    pub checkers: i64,
    pub tps_limit: f64,
    pub min_age: String,
    pub max_age: String,
    pub min_size: String,
    pub max_size: String,
    pub exclude_if_present: String,
    pub max_delete: i64,
    pub dry_run: bool,
    pub use_list_r: bool,
    pub no_unicode_normalize: bool,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub multi_thread_streams: i64,
    pub buffer_size: String,
    pub retries: i64,
    pub low_level_retries: i64,
    pub max_duration: String,
    pub retries_sleep: String,
    pub conn_timeout: String,
    pub io_timeout: String,
    pub order_by: String,
    pub check_first: bool,
    pub immutable: bool,
    pub max_transfer: String,
    pub max_delete_size: String,
    pub suffix: String,
    pub suffix_keep_extension: bool,
    pub backup_dir: String,
    pub size_only: bool,
    pub update_mode: bool,
    pub ignore_existing: bool,
    pub delete_excluded: bool,
    pub max_depth: i64,
    /// before|during|after
    pub delete_timing: String,
    pub conflict_resolve: String,
    pub conflict_loser: String,
    pub conflict_suffix: String,
    pub resilient: bool,
    pub max_lock: String,
    pub check_access: bool,
}

/// Mirrors rclone's `lsjson` output (PascalCase keys, all optional).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileEntry {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Size")]
    pub size: i64,
    #[serde(rename = "MimeType")]
    pub mime_type: String,
    #[serde(rename = "IsDir")]
    pub is_dir: bool,
    #[serde(rename = "ModTime")]
    pub mod_time: String,
    #[serde(rename = "Path")]
    pub path: String,
    #[serde(rename = "ID")]
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuotaInfo {
    pub used: i64,
    pub total: i64,
    pub free: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Remote {
    pub name: String,
    pub r#type: String,
    pub description: String,
}

/// Per-provider rate limits (simplified resolver policy).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolverPolicy {
    pub transfers: i64,
    pub checkers: i64,
    pub tps_limit: f64,
}

impl ResolverPolicy {
    pub fn apply(source_type: &str, dest_type: &str) -> ResolverPolicy {
        const LIMITED: [&str; 12] = [
            "drive",
            "onedrive",
            "dropbox",
            "box",
            "icloud",
            "iclouddrive",
            "googlephotos",
            "mega",
            "pcloud",
            "yandex",
            "mailru",
            "sharepoint",
        ];
        if LIMITED.contains(&source_type) || LIMITED.contains(&dest_type) {
            ResolverPolicy {
                transfers: 4,
                checkers: 4,
                tps_limit: 4.0,
            }
        } else {
            ResolverPolicy {
                transfers: 8,
                checkers: 8,
                tps_limit: 0.0,
            }
        }
    }
}
