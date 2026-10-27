//! Window-only preferences: language, light/dark, sidebar and inspector
//! widths, and whether the empty-canvas hint has been dismissed.
//!
//! These are not engine settings. They live in a small file under Application
//! Support so a restart keeps the choice, and they never touch rclone.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::inspector;

static VI: AtomicBool = AtomicBool::new(false);
static LIGHT: AtomicBool = AtomicBool::new(false);
/// Bit pattern of the inspector width. `320.0f32` is `0x43a0_0000`.
static INSPECTOR_WIDTH_BITS: AtomicU32 = AtomicU32::new(0x43a0_0000);
/// Bit pattern of the sidebar width. `260.0f32` is `0x4382_0000`.
static SIDEBAR_WIDTH_BITS: AtomicU32 = AtomicU32::new(0x4382_0000);
static HINT_DISMISSED: AtomicBool = AtomicBool::new(false);

/// What `window.json` remembers. Parsing is separate from the file so tests
/// never write the real preference file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WindowPrefs {
    pub locale_vi: bool,
    pub light: bool,
    pub inspector_width: f32,
    pub sidebar_width: f32,
    pub canvas_hint_dismissed: bool,
}

impl Default for WindowPrefs {
    fn default() -> Self {
        Self {
            locale_vi: false,
            light: false,
            inspector_width: inspector::WIDTH,
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            canvas_hint_dismissed: false,
        }
    }
}

pub(crate) fn is_vi() -> bool {
    VI.load(Ordering::Relaxed)
}

pub(crate) fn is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

/// Historical flow-list width. `workspace::SIDEBAR_WIDTH` is the same number;
/// this copy lives here so preference parsing does not depend on the view.
pub(crate) const DEFAULT_SIDEBAR_WIDTH: f32 = 260.;

pub(crate) fn inspector_width() -> f32 {
    let width = f32::from_bits(INSPECTOR_WIDTH_BITS.load(Ordering::Relaxed));
    if width.is_finite() && width > 0.0 {
        width
    } else {
        inspector::WIDTH
    }
}

pub(crate) fn sidebar_width() -> f32 {
    let width = f32::from_bits(SIDEBAR_WIDTH_BITS.load(Ordering::Relaxed));
    if width.is_finite() && width > 0.0 {
        width
    } else {
        DEFAULT_SIDEBAR_WIDTH
    }
}

pub(crate) fn canvas_hint_dismissed() -> bool {
    HINT_DISMISSED.load(Ordering::Relaxed)
}

/// The empty-canvas hint shows only when the flow has no folders, and it
/// stays hidden after the user dismisses it.
pub(crate) fn show_canvas_hint(node_count: usize, dismissed: bool) -> bool {
    node_count == 0 && !dismissed
}

pub(crate) fn load() {
    let Some(text) = std::fs::read_to_string(path()).ok() else {
        return;
    };
    apply(&parse_prefs(&text));
}

pub(crate) fn set_locale(vi: bool) {
    VI.store(vi, Ordering::Relaxed);
    store();
}

pub(crate) fn set_light(light: bool) {
    LIGHT.store(light, Ordering::Relaxed);
    ns_ui::set_light(light);
    store();
}

pub(crate) fn set_sidebar_width(width: f32) {
    let width = if width.is_finite() && width > 0.0 {
        width
    } else {
        DEFAULT_SIDEBAR_WIDTH
    };
    SIDEBAR_WIDTH_BITS.store(width.to_bits(), Ordering::Relaxed);
    store();
}

pub(crate) fn set_inspector_width(width: f32) {
    let width = if width.is_finite() && width > 0.0 {
        width
    } else {
        inspector::WIDTH
    };
    INSPECTOR_WIDTH_BITS.store(width.to_bits(), Ordering::Relaxed);
    store();
}

pub(crate) fn set_canvas_hint_dismissed(dismissed: bool) {
    HINT_DISMISSED.store(dismissed, Ordering::Relaxed);
    store();
}

pub(crate) fn parse_prefs(text: &str) -> WindowPrefs {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return WindowPrefs {
            locale_vi: text.contains("\"locale\": \"vi\""),
            light: text.contains("\"light\": true"),
            ..WindowPrefs::default()
        };
    };
    let inspector_width = value
        .get("inspectorWidth")
        .and_then(serde_json::Value::as_f64)
        .filter(|width| width.is_finite() && *width > 0.0)
        .map(|width| width as f32)
        .unwrap_or(inspector::WIDTH);
    WindowPrefs {
        locale_vi: value.get("locale").and_then(serde_json::Value::as_str) == Some("vi"),
        light: value
            .get("light")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        inspector_width,
        sidebar_width: value
            .get("sidebarWidth")
            .and_then(serde_json::Value::as_f64)
            .filter(|width| width.is_finite() && *width > 0.0)
            .map(|width| width as f32)
            .unwrap_or(DEFAULT_SIDEBAR_WIDTH),
        canvas_hint_dismissed: value
            .get("canvasHintDismissed")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    }
}

pub(crate) fn format_prefs(prefs: &WindowPrefs) -> String {
    let width = if prefs.inspector_width.is_finite() {
        prefs.inspector_width.round() as i32
    } else {
        inspector::WIDTH as i32
    };
    let sidebar = if prefs.sidebar_width.is_finite() {
        prefs.sidebar_width.round() as i32
    } else {
        DEFAULT_SIDEBAR_WIDTH as i32
    };
    format!(
        "{{\n  \"locale\": \"{}\",\n  \"light\": {},\n  \"inspectorWidth\": {width},\n  \"sidebarWidth\": {sidebar},\n  \"canvasHintDismissed\": {}\n}}\n",
        if prefs.locale_vi { "vi" } else { "en" },
        if prefs.light { "true" } else { "false" },
        if prefs.canvas_hint_dismissed {
            "true"
        } else {
            "false"
        }
    )
}

fn apply(prefs: &WindowPrefs) {
    VI.store(prefs.locale_vi, Ordering::Relaxed);
    LIGHT.store(prefs.light, Ordering::Relaxed);
    INSPECTOR_WIDTH_BITS.store(prefs.inspector_width.to_bits(), Ordering::Relaxed);
    SIDEBAR_WIDTH_BITS.store(prefs.sidebar_width.to_bits(), Ordering::Relaxed);
    HINT_DISMISSED.store(prefs.canvas_hint_dismissed, Ordering::Relaxed);
}

fn current() -> WindowPrefs {
    WindowPrefs {
        locale_vi: is_vi(),
        light: is_light(),
        inspector_width: inspector_width(),
        sidebar_width: sidebar_width(),
        canvas_hint_dismissed: canvas_hint_dismissed(),
    }
}

fn store() {
    let body = format_prefs(&current());
    if let Some(parent) = path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path(), body);
}

fn path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join("Library/Application Support/gn-drive-gpui")
        .join("window.json")
}

/// English chrome, with a Vietnamese line when the toolbar language is Tiếng Việt.
/// Unknown sentences stay English: this window's copy is not the Swift 298-key table.
pub(crate) fn t(english: &str) -> String {
    if !is_vi() {
        return english.to_string();
    }
    match english {
        "Flows" => "Luồng",
        "New flow" => "Luồng mới",
        "No flows yet" => "Chưa có luồng",
        "Run" => "Chạy",
        "Run now" => "Chạy ngay",
        "Stop" => "Dừng",
        "Save" => "Lưu",
        "Add node" => "Thêm nút",
        "Connected" => "Đã kết nối",
        "Reconnecting" => "Đang kết nối lại",
        "Offline" => "Ngoại tuyến",
        "Demo data" => "Dữ liệu mẫu",
        "This Mac" => "Máy Mac này",
        "Cloud accounts" => "Tài khoản đám mây",
        "Settings" => "Cài đặt",
        "Find a flow" => "Tìm luồng",
        "Discard changes" => "Bỏ thay đổi",
        "Delete flow" => "Xóa luồng",
        "Manual" => "Thủ công",
        "Schedule" => "Lịch",
        "Inspector" => "Thông tin",
        "Push" => "Đẩy",
        "Two-way" => "Hai chiều",
        "Re-sync" => "Đồng bộ lại",
        "Remotes" => "Máy chủ",
        "Lock" => "Khóa",
        _ => english,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_window_file_keeps_language_and_new_keys_default() {
        let old = "{\n  \"locale\": \"vi\",\n  \"light\": true\n}\n";
        let prefs = parse_prefs(old);
        assert!(prefs.locale_vi);
        assert!(prefs.light);
        assert!((prefs.inspector_width - inspector::WIDTH).abs() < 0.01);
        assert!((prefs.sidebar_width - DEFAULT_SIDEBAR_WIDTH).abs() < 0.01);
        assert!(!prefs.canvas_hint_dismissed);
        let again = parse_prefs(&format_prefs(&prefs));
        assert_eq!(again.locale_vi, prefs.locale_vi);
        assert_eq!(again.light, prefs.light);
        assert!((again.inspector_width - prefs.inspector_width.round()).abs() < 0.01);
        assert!((again.sidebar_width - DEFAULT_SIDEBAR_WIDTH).abs() < 0.01);
        assert_eq!(again.canvas_hint_dismissed, prefs.canvas_hint_dismissed);
    }

    #[test]
    fn width_and_hint_round_trip() {
        let prefs = WindowPrefs {
            locale_vi: false,
            light: false,
            inspector_width: 360.4,
            sidebar_width: 241.6,
            canvas_hint_dismissed: true,
        };
        let again = parse_prefs(&format_prefs(&prefs));
        assert!(!again.locale_vi);
        assert!(!again.light);
        assert!((again.inspector_width - 360.0).abs() < 0.01);
        assert!((again.sidebar_width - 242.0).abs() < 0.01);
        assert!(again.canvas_hint_dismissed);
    }

    #[test]
    fn empty_canvas_hint_hides_after_dismissal_and_when_folders_exist() {
        assert!(show_canvas_hint(0, false));
        assert!(!show_canvas_hint(0, true));
        assert!(!show_canvas_hint(3, false));
        assert!(!show_canvas_hint(3, true));
    }
}
