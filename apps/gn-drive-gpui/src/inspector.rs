//! Inspector information architecture.
//!
//! Pure helpers only: which section a flag lives in, which sections start
//! collapsed, how a toggle row must be laid out, whether a number is usable,
//! and what the pinned save bar says. The view in `workspace.rs` renders this
//! and does not invent a second grouping.

use crate::sync_config::{FIELDS, FieldDef, FieldKind, SyncConfig};

/// Preferred inspector width. Compact windows ignore it and use the full pane.
pub const WIDTH: f32 = 320.;
/// The divider will not go narrower than this.
pub const MIN_WIDTH: f32 = 260.;

/// Clamp a requested inspector width.
///
/// The maximum is half of `editor_width` (the canvas and the inspector
/// together) and never less than [`MIN_WIDTH`]. A missing or non-finite
/// editor width keeps the preferred width reachable. A non-finite request
/// falls back to [`WIDTH`]. The result is a whole number of points.
pub fn clamp_inspector_width(requested: f32, editor_width: f32) -> f32 {
    let max = if !editor_width.is_finite() || editor_width <= 0.0 {
        WIDTH * 2.0
    } else {
        (editor_width * 0.5).max(MIN_WIDTH)
    };
    let max = max.floor().max(MIN_WIDTH);
    let value = if requested.is_finite() {
        requested
    } else {
        WIDTH
    };
    value.round().clamp(MIN_WIDTH, max)
}

/// Width while the divider is dragged. Moving right (`delta_x` positive)
/// shrinks the inspector, because the divider is its left edge.
pub fn drag_inspector_width(origin_width: f32, delta_x: f32, editor_width: f32) -> f32 {
    let origin = if origin_width.is_finite() {
        origin_width
    } else {
        WIDTH
    };
    let delta = if delta_x.is_finite() { delta_x } else { 0.0 };
    clamp_inspector_width(origin - delta, editor_width)
}

/// One collapsible block in the panel. Field sections are [`WhatToSync`],
/// [`Safety`], and [`Advanced`]. The others are structural.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Section {
    Basics,
    Schedule,
    WhatToSync,
    Safety,
    Advanced,
    Connections,
    Danger,
}

/// Who a flag applies to. Two-way and re-sync share one set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    All,
    Push,
    TwoWay,
}

/// How the toggle row is built. The view applies these flags directly, so a
/// test can lock the contract without a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToggleRowStyle {
    /// Label column grows into the free space (`flex_1`).
    pub label_grows: bool,
    /// Label column may shrink, but not by collapsing its min to the content
    /// (`min_w(0)`). Combined with [`Self::label_grows`] the used width stays
    /// the free space, so words wrap instead of one glyph per line.
    pub label_min_width_zero: bool,
    /// Break on spaces (`whitespace_normal`), never one character per line.
    pub label_wraps_words: bool,
    /// The switch keeps its own width (`flex_shrink_0`).
    pub switch_does_not_shrink: bool,
}

pub fn toggle_row_style() -> ToggleRowStyle {
    ToggleRowStyle {
        label_grows: true,
        label_min_width_zero: true,
        label_wraps_words: true,
        switch_does_not_shrink: true,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Present {
    pub key: &'static str,
    pub section: Section,
    pub scope: Scope,
    /// Empty means “use the storage label”.
    pub label: &'static str,
    pub help: &'static str,
    /// Short unit drawn on the label row. Never the placeholder value.
    pub unit: &'static str,
}

const PRESENTS: &[Present] = &[
    Present {
        key: "included_paths",
        section: Section::WhatToSync,
        scope: Scope::All,
        label: "Include paths",
        help: "Only these paths are copied. One path per line. Blank copies everything.",
        unit: "",
    },
    Present {
        key: "excluded_paths",
        section: Section::WhatToSync,
        scope: Scope::All,
        label: "Exclude paths",
        help: "Skip these paths. One path per line.",
        unit: "",
    },
    Present {
        key: "use_regex",
        section: Section::WhatToSync,
        scope: Scope::All,
        label: "Use regex",
        help: "Treat include and exclude lines as regular expressions.",
        unit: "",
    },
    Present {
        key: "min_size",
        section: Section::WhatToSync,
        scope: Scope::All,
        label: "Min size",
        help: "Skip files smaller than this. Example: 1M.",
        unit: "",
    },
    Present {
        key: "max_size",
        section: Section::WhatToSync,
        scope: Scope::All,
        label: "Max size",
        help: "Skip files larger than this. Example: 1G.",
        unit: "",
    },
    Present {
        key: "max_age",
        section: Section::WhatToSync,
        scope: Scope::All,
        label: "Max age",
        help: "Skip files older than this. Example: 24h.",
        unit: "",
    },
    Present {
        key: "min_age",
        section: Section::WhatToSync,
        scope: Scope::All,
        label: "Min age",
        help: "Skip files newer than this. Example: 0s.",
        unit: "",
    },
    Present {
        key: "max_depth",
        section: Section::WhatToSync,
        scope: Scope::All,
        label: "Max depth",
        help: "How many folders deep to look. 0 means no limit.",
        unit: "levels",
    },
    Present {
        key: "dry_run",
        section: Section::Safety,
        scope: Scope::All,
        label: "Dry run",
        help: "Show what would change, and do not write anything.",
        unit: "",
    },
    Present {
        key: "max_delete",
        section: Section::Safety,
        scope: Scope::All,
        label: "Max delete",
        help: "Stop if more files than this would be deleted. 0 means no limit.",
        unit: "files",
    },
    Present {
        key: "delete_excluded",
        section: Section::Safety,
        scope: Scope::All,
        label: "Delete excluded",
        help: "Also delete excluded files on the destination.",
        unit: "",
    },
    Present {
        key: "immutable",
        section: Section::Safety,
        scope: Scope::All,
        label: "Immutable",
        help: "Do not change or delete a file that already exists.",
        unit: "",
    },
    Present {
        key: "suffix",
        section: Section::Safety,
        scope: Scope::All,
        label: "Backup suffix",
        help: "Add this ending to a file before it is replaced. Example: .bak.",
        unit: "",
    },
    Present {
        key: "suffix_keep_extension",
        section: Section::Safety,
        scope: Scope::All,
        label: "Keep extension",
        help: "Keep the original extension, so photo.jpg becomes photo.bak.jpg.",
        unit: "",
    },
    Present {
        key: "backup_path",
        section: Section::Safety,
        scope: Scope::All,
        label: "Backup folder",
        help: "Move replaced files here instead of deleting them.",
        unit: "",
    },
    Present {
        key: "parallel",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Parallel transfers",
        help: "How many files copy at once.",
        unit: "transfers",
    },
    Present {
        key: "bandwidth",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Bandwidth",
        help: "Cap the speed. 0 means no limit.",
        unit: "MB/s",
    },
    Present {
        key: "multi_thread_streams",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Multi-thread streams",
        help: "Threads used for one large file.",
        unit: "streams",
    },
    Present {
        key: "buffer_size",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Buffer size",
        help: "Memory used per transfer. Example: 16M.",
        unit: "",
    },
    Present {
        key: "retries",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Retries",
        help: "How many times to retry a failed file.",
        unit: "tries",
    },
    Present {
        key: "low_level_retries",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Low-level retries",
        help: "Retries for one network request.",
        unit: "tries",
    },
    Present {
        key: "max_duration",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Max duration",
        help: "Stop the run after this long. Example: 24h.",
        unit: "",
    },
    Present {
        key: "check_first",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Check first",
        help: "Compare every file before copying, instead of during.",
        unit: "",
    },
    Present {
        key: "order_by",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Order by",
        help: "Copy order. Example: size,ascending.",
        unit: "",
    },
    Present {
        key: "retries_sleep",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Pause between retries",
        help: "How long to wait before trying again. Example: 0s.",
        unit: "",
    },
    Present {
        key: "tps_limit",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Request limit",
        help: "Cap API calls per second. 0 means no limit.",
        unit: "per second",
    },
    Present {
        key: "conn_timeout",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Connect timeout",
        help: "How long to wait to connect. Example: 60s.",
        unit: "",
    },
    Present {
        key: "io_timeout",
        section: Section::Advanced,
        scope: Scope::All,
        label: "I/O timeout",
        help: "How long to wait for data. Example: 300s.",
        unit: "",
    },
    Present {
        key: "filter_from_file",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Filter file",
        help: "Read extra include and exclude rules from this file.",
        unit: "",
    },
    Present {
        key: "exclude_if_present",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Exclude if present",
        help: "Skip a folder when it contains this file. Example: .ignore.",
        unit: "",
    },
    Present {
        key: "max_transfer",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Max transfer",
        help: "Stop after this much data has been sent. Example: 100G.",
        unit: "",
    },
    Present {
        key: "max_delete_size",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Max delete size",
        help: "Stop if deletions would remove more than this. Example: 1G.",
        unit: "",
    },
    Present {
        key: "size_only",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Compare size only",
        help: "Use file size, and ignore modification time.",
        unit: "",
    },
    Present {
        key: "update_mode",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Skip newer files",
        help: "Do not replace a destination file that is newer.",
        unit: "",
    },
    Present {
        key: "ignore_existing",
        section: Section::Advanced,
        scope: Scope::All,
        label: "Ignore existing",
        help: "Do not overwrite a file that is already there.",
        unit: "",
    },
    Present {
        key: "delete_timing",
        section: Section::Advanced,
        scope: Scope::Push,
        label: "When to delete",
        help: "When to remove files that are gone from the source.",
        unit: "",
    },
    Present {
        key: "conflict_resolution",
        section: Section::Advanced,
        scope: Scope::TwoWay,
        label: "If both sides changed",
        help: "Which copy wins when both folders changed the same file.",
        unit: "",
    },
    Present {
        key: "resilient",
        section: Section::Advanced,
        scope: Scope::TwoWay,
        label: "Keep going after errors",
        help: "Continue the two-way sync when one file fails.",
        unit: "",
    },
    Present {
        key: "max_lock",
        section: Section::Advanced,
        scope: Scope::TwoWay,
        label: "Lock wait",
        help: "How long to wait if the other side is still syncing. Example: 2m.",
        unit: "",
    },
    Present {
        key: "check_access",
        section: Section::Advanced,
        scope: Scope::TwoWay,
        label: "Check access first",
        help: "Make sure both folders can be read before syncing.",
        unit: "",
    },
    Present {
        key: "conflict_loser",
        section: Section::Advanced,
        scope: Scope::TwoWay,
        label: "Losing copy",
        help: "How to name the copy that lost. Example: num.",
        unit: "",
    },
    Present {
        key: "conflict_suffix",
        section: Section::Advanced,
        scope: Scope::TwoWay,
        label: "Conflict suffix",
        help: "Ending added to the losing copy. Example: .conflict1.",
        unit: "",
    },
];

pub fn section_title(section: Section) -> &'static str {
    match section {
        Section::Basics => "Basics",
        Section::Schedule => "Schedule",
        Section::WhatToSync => "What to sync",
        Section::Safety => "Safety",
        Section::Advanced => "Advanced",
        Section::Connections => "Connections",
        Section::Danger => "Danger zone",
    }
}

/// Short name for the dev command `inspector section=<name>`.
#[cfg(any(feature = "devtools", test))]
pub fn section_key(section: Section) -> &'static str {
    match section {
        Section::Basics => "basics",
        Section::Schedule => "schedule",
        Section::WhatToSync => "what",
        Section::Safety => "safety",
        Section::Advanced => "advanced",
        Section::Connections => "connections",
        Section::Danger => "danger",
    }
}

/// `advanced`, `section-advanced`, `What to sync`, and `danger zone`.
#[cfg(any(feature = "devtools", test))]
pub fn section_from_name(name: &str) -> Option<Section> {
    let folded = name.trim().to_ascii_lowercase().replace(['_', ' '], "-");
    let key = folded.strip_prefix("section-").unwrap_or(&folded);
    match key {
        "basics" => Some(Section::Basics),
        "schedule" => Some(Section::Schedule),
        "what" | "what-to-sync" => Some(Section::WhatToSync),
        "safety" => Some(Section::Safety),
        "advanced" => Some(Section::Advanced),
        "connections" => Some(Section::Connections),
        "danger" | "danger-zone" => Some(Section::Danger),
        _ => None,
    }
}

pub fn section_dom_id(section: Section) -> &'static str {
    match section {
        Section::Basics => "section-basics",
        Section::Schedule => "section-schedule",
        Section::WhatToSync => "section-what",
        Section::Safety => "section-safety",
        Section::Advanced => "section-advanced",
        Section::Connections => "section-connections",
        Section::Danger => "section-danger",
    }
}

/// Advanced starts closed. Everything a person needs for a normal sync starts open.
pub fn section_starts_open(section: Section) -> bool {
    !matches!(section, Section::Advanced)
}

pub fn section_tip(section: Section, open: bool) -> String {
    let title = section_title(section);
    if open {
        format!("Hide {title} options")
    } else {
        format!("Show {title} options")
    }
}

pub fn present(key: &str) -> Option<&'static Present> {
    PRESENTS.iter().find(|item| item.key == key)
}

pub fn field_label(key: &str) -> &str {
    present(key)
        .filter(|item| !item.label.is_empty())
        .map(|item| item.label)
        .or_else(|| {
            FIELDS
                .iter()
                .find(|def| def.key == key)
                .map(|def| def.label)
        })
        .unwrap_or(key)
}

pub fn field_section(key: &str) -> Option<Section> {
    present(key).map(|item| item.section)
}

pub fn is_advanced(key: &str) -> bool {
    field_section(key) == Some(Section::Advanced)
}

pub fn scope_applies(scope: Scope, action: &str) -> bool {
    match scope {
        Scope::All => true,
        Scope::Push => !matches!(action, "bi" | "bi-resync"),
        Scope::TwoWay => matches!(action, "bi" | "bi-resync"),
    }
}

pub fn field_applies(key: &str, action: &str) -> bool {
    present(key).is_some_and(|item| scope_applies(item.scope, action))
}

pub fn fields_in(section: Section, action: &str) -> Vec<&'static FieldDef> {
    FIELDS
        .iter()
        .filter(|def| placed_in(def.key, section) && field_applies(def.key, action))
        .collect()
}

/// Advanced membership is `is_advanced`. Every other section uses `field_section`.
fn placed_in(key: &str, section: Section) -> bool {
    if is_advanced(key) {
        section == Section::Advanced
    } else {
        field_section(key) == Some(section)
    }
}

/// Empty is “leave this unset”. Anything else must be a non-negative number.
pub fn number_error(kind: FieldKind, raw: &str) -> Option<&'static str> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    match kind {
        FieldKind::Int => match raw.parse::<i64>() {
            Ok(value) if value >= 0 => None,
            Ok(_) => Some("Use 0 or a positive whole number."),
            Err(_) => Some("Enter a whole number, or leave this blank."),
        },
        FieldKind::Double => match raw.parse::<f64>() {
            Ok(value) if value.is_finite() && value >= 0.0 => None,
            Ok(_) => Some("Use 0 or a positive number."),
            Err(_) => Some("Enter a number, or leave this blank."),
        },
        _ => None,
    }
}

pub fn stored_field_text(config: &SyncConfig, def: &FieldDef) -> String {
    match def.kind {
        FieldKind::Int => config
            .int(def.key)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        FieldKind::Double => config
            .double(def.key)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        FieldKind::Text | FieldKind::Select(_) => config.text(def.key),
        FieldKind::List => config.list(def.key).join("\n"),
        FieldKind::Bool => String::new(),
    }
}

fn lines_of(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

/// Whether the text in the field differs from what is stored. A bad number
/// counts as different, so the save bar can say the edit is unsaved.
pub fn field_text_differs(kind: FieldKind, stored: &str, edited: &str) -> bool {
    match kind {
        FieldKind::List => lines_of(stored) != lines_of(edited),
        FieldKind::Bool => false,
        _ => stored.trim() != edited.trim(),
    }
}

/// Tooltip on the unsaved dot beside the flow name.
pub const UNSAVED_DOT_TIP: &str = "Unsaved changes — press ⌘S to save";

pub const PATH_REQUIRED: &str = "Enter a path for this folder.";
pub const SOURCE_REQUIRED: &str = "Enter a path for the source folder.";
pub const TARGET_REQUIRED: &str = "Enter a path for the destination folder.";

pub fn empty_state_copy() -> (&'static str, &'static str) {
    (
        "Nothing selected",
        "Click a folder or a connection on the canvas. Add a folder if the canvas is empty.",
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveBar {
    pub status: &'static str,
    pub note: Option<&'static str>,
    pub save_enabled: bool,
    pub discard_enabled: bool,
    pub show_discard: bool,
    pub save_tooltip: String,
    pub discard_tooltip: &'static str,
}

pub fn save_bar(unsaved: bool, running: bool, errors: &[String]) -> SaveBar {
    let discard_tooltip = "Throw away unsaved changes and reload this flow";
    let status = if unsaved {
        "Unsaved changes"
    } else {
        "All changes saved"
    };
    if running {
        return SaveBar {
            status,
            note: Some("Read-only while running"),
            save_enabled: false,
            discard_enabled: false,
            show_discard: unsaved,
            save_tooltip: "Stop the flow before saving changes.".into(),
            discard_tooltip: "Stop the flow before discarding changes.",
        };
    }
    if let Some(first) = errors.first() {
        return SaveBar {
            status: "Unsaved changes",
            note: None,
            save_enabled: false,
            discard_enabled: true,
            show_discard: true,
            save_tooltip: format!("Save is unavailable. {first}"),
            discard_tooltip,
        };
    }
    if unsaved {
        SaveBar {
            status,
            note: None,
            save_enabled: true,
            discard_enabled: true,
            show_discard: true,
            save_tooltip: "Save this flow".into(),
            discard_tooltip,
        }
    } else {
        SaveBar {
            status,
            note: None,
            save_enabled: false,
            discard_enabled: false,
            show_discard: false,
            save_tooltip: "Nothing to save.".into(),
            discard_tooltip,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(section: Section, action: &str) -> Vec<&'static str> {
        fields_in(section, action)
            .into_iter()
            .map(|def| def.key)
            .collect()
    }

    #[test]
    fn every_field_has_one_presentation_and_numeric_hints_are_not_help() {
        assert_eq!(PRESENTS.len(), FIELDS.len());
        for def in FIELDS {
            let item = present(def.key).unwrap_or_else(|| panic!("missing {}", def.key));
            assert!(!item.help.is_empty(), "{}", def.key);
            assert_ne!(item.help, def.hint, "{}", def.key);
        }
        let max_delete = present("max_delete").unwrap();
        assert_eq!(max_delete.unit, "files");
        assert!(max_delete.help.contains("no limit"));
        assert_ne!(max_delete.help, "0");
    }

    #[test]
    fn sections_group_common_flags_and_hide_advanced() {
        let sync = keys(Section::WhatToSync, "push");
        assert!(sync.contains(&"included_paths"));
        assert!(sync.contains(&"excluded_paths"));
        assert!(sync.contains(&"use_regex"));
        assert!(!sync.contains(&"dry_run"));
        assert!(!sync.contains(&"parallel"));

        let safety = keys(Section::Safety, "push");
        for key in [
            "dry_run",
            "max_delete",
            "delete_excluded",
            "backup_path",
            "suffix",
        ] {
            assert!(safety.contains(&key), "{key}");
            assert!(!is_advanced(key));
        }

        let push = keys(Section::Advanced, "push");
        assert!(push.contains(&"parallel"));
        assert!(push.contains(&"bandwidth"));
        assert!(push.contains(&"delete_timing"));
        assert!(push.contains(&"size_only"));
        assert!(!push.contains(&"conflict_resolution"));
        assert!(!push.contains(&"dry_run"));

        for action in ["bi", "bi-resync"] {
            let two_way = keys(Section::Advanced, action);
            assert!(two_way.contains(&"conflict_resolution"));
            assert!(two_way.contains(&"resilient"));
            assert!(two_way.contains(&"parallel"));
            assert!(!two_way.contains(&"delete_timing"), "{action}");
        }

        assert!(!section_starts_open(Section::Advanced));
        assert!(section_starts_open(Section::Safety));
        assert!(section_starts_open(Section::WhatToSync));
        assert!(section_starts_open(Section::Basics));
        assert_eq!(section_tip(Section::Safety, false), "Show Safety options");
        assert_eq!(
            section_tip(Section::Advanced, true),
            "Hide Advanced options"
        );
    }

    #[test]
    fn section_names_match_the_dev_command() {
        for section in [
            Section::Basics,
            Section::Schedule,
            Section::WhatToSync,
            Section::Safety,
            Section::Advanced,
            Section::Connections,
            Section::Danger,
        ] {
            assert_eq!(section_from_name(section_key(section)), Some(section));
            assert_eq!(
                section_from_name(&section_title(section).to_ascii_lowercase()),
                Some(section)
            );
            assert_eq!(
                section_from_name(section_dom_id(section)),
                Some(section),
                "{}",
                section_dom_id(section)
            );
        }
        assert_eq!(section_from_name("ADVANCED"), Some(Section::Advanced));
        assert_eq!(section_from_name("danger zone"), Some(Section::Danger));
        assert_eq!(section_from_name("what_to_sync"), Some(Section::WhatToSync));
        assert_eq!(section_from_name("nope"), None);
    }

    #[test]
    fn numbers_accept_blank_and_reject_junk() {
        assert!(number_error(FieldKind::Int, "").is_none());
        assert!(number_error(FieldKind::Int, "  ").is_none());
        assert!(number_error(FieldKind::Int, "0").is_none());
        assert!(number_error(FieldKind::Int, "12").is_none());
        assert_eq!(
            number_error(FieldKind::Int, "-1"),
            Some("Use 0 or a positive whole number.")
        );
        assert_eq!(
            number_error(FieldKind::Int, "nope"),
            Some("Enter a whole number, or leave this blank.")
        );
        assert!(number_error(FieldKind::Double, "1.5").is_none());
        assert!(number_error(FieldKind::Double, "0").is_none());
        assert!(number_error(FieldKind::Double, "1.5.5").is_some());
        assert!(number_error(FieldKind::Double, "-0.1").is_some());
        assert!(number_error(FieldKind::Text, "nope").is_none());
    }

    #[test]
    fn save_bar_labels_follow_dirty_invalid_and_running() {
        let clean = save_bar(false, false, &[]);
        assert_eq!(clean.status, "All changes saved");
        assert!(!clean.save_enabled);
        assert!(!clean.show_discard);

        let dirty = save_bar(true, false, &[]);
        assert_eq!(dirty.status, "Unsaved changes");
        assert!(dirty.save_enabled);
        assert!(dirty.show_discard);
        assert_eq!(dirty.save_tooltip, "Save this flow");
        assert!(UNSAVED_DOT_TIP.contains("Unsaved changes"));
        assert!(UNSAVED_DOT_TIP.contains("⌘S"));

        let bad = save_bar(
            true,
            false,
            &["Enter a whole number, or leave this blank.".into()],
        );
        assert_eq!(bad.status, "Unsaved changes");
        assert!(!bad.save_enabled);
        assert!(bad.save_tooltip.contains("whole number"));
        assert!(bad.discard_enabled);

        let running = save_bar(true, true, &["ignored while running".into()]);
        assert_eq!(running.note, Some("Read-only while running"));
        assert!(!running.save_enabled);
        assert!(!running.discard_enabled);
        assert!(running.save_tooltip.contains("Stop the flow"));
    }

    #[test]
    fn field_text_diff_treats_a_bad_number_as_unsaved() {
        assert!(!field_text_differs(FieldKind::Int, "0", "0"));
        assert!(!field_text_differs(FieldKind::Int, "0", " 0 "));
        assert!(field_text_differs(FieldKind::Int, "0", "1"));
        assert!(field_text_differs(FieldKind::Int, "0", "abc"));
        assert!(!field_text_differs(FieldKind::List, "/a\n/b", "/a\n/b\n"));
        assert!(field_text_differs(FieldKind::List, "/a", "/a\n/c"));
    }

    #[test]
    fn toggle_row_style_keeps_the_label_width() {
        let style = toggle_row_style();
        assert!(style.label_grows);
        assert!(style.label_min_width_zero);
        assert!(style.label_wraps_words);
        assert!(style.switch_does_not_shrink);
        assert_eq!(WIDTH, 320.);
        assert_eq!(MIN_WIDTH, 260.);
        assert_eq!(clamp_inspector_width(320.0, 900.0), 320.0);
        assert_eq!(clamp_inspector_width(100.0, 900.0), MIN_WIDTH);
        assert_eq!(clamp_inspector_width(800.0, 900.0), 450.0);
        assert_eq!(clamp_inspector_width(f32::NAN, 900.0), WIDTH);
        assert_eq!(clamp_inspector_width(900.0, 0.0), WIDTH * 2.0);
        assert_eq!(clamp_inspector_width(320.0, 400.0), MIN_WIDTH);
        assert_eq!(drag_inspector_width(320.0, 40.0, 1000.0), 280.0);
        assert_eq!(drag_inspector_width(320.0, -40.0, 1000.0), 360.0);
        assert_eq!(drag_inspector_width(320.0, 200.0, 1000.0), MIN_WIDTH);
        assert_eq!(drag_inspector_width(320.0, -400.0, 1000.0), 500.0);
        let (title, hint) = empty_state_copy();
        assert_eq!(title, "Nothing selected");
        assert!(hint.contains("folder"));
    }

    #[test]
    fn stored_text_round_trips_numbers_without_echoing_a_second_zero() {
        let raw = serde_json::json!({"max_delete": 0, "parallel": 4, "included_paths": ["/a"]});
        let map = serde_json::from_value(raw).unwrap();
        let config = SyncConfig::parse(&map, "push");
        let max_delete = FIELDS.iter().find(|def| def.key == "max_delete").unwrap();
        assert_eq!(stored_field_text(&config, max_delete), "0");
        let present = present("max_delete").unwrap();
        assert_ne!(present.help, stored_field_text(&config, max_delete));
        let parallel = FIELDS.iter().find(|def| def.key == "parallel").unwrap();
        assert_eq!(stored_field_text(&config, parallel), "4");
    }
}
