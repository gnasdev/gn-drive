//! One look for empty, loading, and error, and the pure rules behind them.
//!
//! Loading is never drawn as empty. A status chip appears on a cloud account
//! only after a test. The unlock note keeps the folder path, shortened in the
//! middle, with the full path in a tooltip.

use gpui::{AnyElement, IntoElement, SharedString, div, prelude::*, px};
use ns_ui::{Tone, skeleton_list, theme};

use crate::format::{self, Explained};
use crate::tip::with_tip;

/// What a list region should draw. [`list_region`] never returns [`Empty`]
/// while a fetch is still running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Region {
    Loading,
    Error,
    Empty,
    Ready,
}

/// `loading` wins, including over an error and over a count of zero, so a
/// list does not flash "nothing here" while the engine is still answering.
pub(crate) fn list_region(loading: bool, error: bool, count: usize) -> Region {
    if loading {
        Region::Loading
    } else if error && count == 0 {
        Region::Error
    } else if count == 0 {
        Region::Empty
    } else {
        Region::Ready
    }
}

/// A feed that has not arrived yet is loading, even when the buffer is empty.
/// An error that already came back is an error, not an empty list.
pub(crate) fn feed_region(in_flight: bool, error: bool, arrived: bool, count: usize) -> Region {
    let loading = in_flight || (!arrived && !error);
    list_region(loading, error, count)
}

/// Cloud-account chip. Unchecked draws nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RemoteStatus {
    Unchecked,
    Testing,
    Connected,
    Failed,
}

/// Testing wins, then a failed test, then a successful one.
pub(crate) fn remote_status(testing: bool, connected: bool, failed: bool) -> RemoteStatus {
    if testing {
        RemoteStatus::Testing
    } else if failed {
        RemoteStatus::Failed
    } else if connected {
        RemoteStatus::Connected
    } else {
        RemoteStatus::Unchecked
    }
}

/// Label and colour for a chip. `None` means draw no chip.
pub(crate) fn remote_chip(status: RemoteStatus) -> Option<(&'static str, Tone)> {
    match status {
        RemoteStatus::Unchecked => None,
        RemoteStatus::Testing => Some(("Testing…", Tone::Warn)),
        RemoteStatus::Connected => Some(("Connected", Tone::Ok)),
        RemoteStatus::Failed => Some(("Failed", Tone::Err)),
    }
}

/// The sentence a busy button shows, or the idle label when the work is done.
pub(crate) fn busy_label(busy: bool, busy_text: &'static str, idle: &'static str) -> &'static str {
    if busy { busy_text } else { idle }
}

/// A failure the user can read, plus the technical code for a Details tooltip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fault {
    pub sentence: String,
    pub code: String,
    pub raw: String,
}

impl Fault {
    pub(crate) fn new(
        sentence: impl Into<String>,
        code: impl Into<String>,
        raw: impl Into<String>,
    ) -> Self {
        Self {
            sentence: sentence.into(),
            code: code.into(),
            raw: raw.into(),
        }
    }

    pub(crate) fn from_explained(explained: Explained, raw: impl Into<String>) -> Self {
        Self::new(explained.text, explained.code, raw)
    }
}

/// Tooltip body for Details: the code, then the engine text when it adds something.
pub(crate) fn details_tip(code: &str, raw: &str) -> String {
    let code = code.trim();
    let raw = raw.trim();
    if code.is_empty() && raw.is_empty() {
        "No technical code was reported.".into()
    } else if raw.is_empty() || raw == code {
        code.to_string()
    } else if code.is_empty() {
        raw.to_string()
    } else {
        format!("{code}\n{raw}")
    }
}

/// Split a stored `explain().display()` value into the sentence and the code.
pub(crate) fn error_parts(stored: &str) -> (String, String) {
    match stored.split_once("\nDetails: ") {
        Some((sentence, code)) => (sentence.trim().to_string(), code.trim().to_string()),
        None => (stored.trim().to_string(), String::new()),
    }
}

pub(crate) fn flows_empty_copy() -> (&'static str, &'static str) {
    (
        "No flows yet",
        "A flow is a set of folders that sync together, on demand or on a schedule.",
    )
}

pub(crate) fn remotes_empty_copy() -> (&'static str, &'static str) {
    (
        "No cloud accounts yet",
        "Add a cloud account, such as Google Drive or a NAS, so your flows can reach it.",
    )
}

pub(crate) fn history_empty_copy() -> (&'static str, &'static str) {
    ("No runs yet", "Run a flow to see it here.")
}

pub(crate) fn logs_empty_copy() -> (&'static str, &'static str) {
    (
        "No log lines yet",
        "Run a flow, then refresh to see what GN Drive recorded.",
    )
}

/// How many characters of the settings-folder path stay visible on the unlock card.
pub(crate) const FOLDER_NOTE_CHARS: usize = 36;

pub(crate) const FOLDER_NOTE_COPY: &str = "GN Drive stores only a secure hash of your password in this folder, never the password itself.";

/// The muted two-line note under the unlock form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FolderNote {
    /// `Settings folder: ` plus the path, shortened in the middle.
    pub label: String,
    pub full_path: String,
    pub copy: &'static str,
}

pub(crate) fn folder_note(path: &str) -> FolderNote {
    let shown = format::middle_ellipsis(path, FOLDER_NOTE_CHARS);
    FolderNote {
        label: format!("Settings folder: {shown}"),
        full_path: path.to_string(),
        copy: FOLDER_NOTE_COPY,
    }
}

/// Preview forced by the mock-only `simulate` dev command. `None` is real data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Simulation {
    #[default]
    None,
    EmptyFlows,
    EmptyRemotes,
    Loading,
    ErrorEngine,
    ErrorTest,
    EmptyHistory,
    EmptyLogs,
    WrongPassword,
}

// parse and key serve the dev channel only.
#[cfg_attr(not(any(test, feature = "devtools")), allow(dead_code))]
impl Simulation {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        Some(match name.trim() {
            "reset" | "none" => Self::None,
            "empty-flows" => Self::EmptyFlows,
            "empty-remotes" => Self::EmptyRemotes,
            "loading" => Self::Loading,
            "error-engine" => Self::ErrorEngine,
            "error-test" => Self::ErrorTest,
            "empty-history" => Self::EmptyHistory,
            "empty-logs" => Self::EmptyLogs,
            "wrong-password" => Self::WrongPassword,
            _ => return None,
        })
    }

    /// Value `state.simulation` reports. `none` means the preview is off.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::EmptyFlows => "empty-flows",
            Self::EmptyRemotes => "empty-remotes",
            Self::Loading => "loading",
            Self::ErrorEngine => "error-engine",
            Self::ErrorTest => "error-test",
            Self::EmptyHistory => "empty-history",
            Self::EmptyLogs => "empty-logs",
            Self::WrongPassword => "wrong-password",
        }
    }
}

/// The failure a simulated connection test shows, built with [`format::explain`].
pub(crate) fn simulated_test_fault() -> Fault {
    let raw = "Could not reach this cloud account";
    Fault::from_explained(format::explain("sync_failed", raw), raw)
}

/// The sentence and code a simulated engine-start failure shows.
pub(crate) fn simulated_engine_fault() -> Fault {
    let raw = "Could not start gn-drive";
    Fault::from_explained(format::explain("spawn_failed", raw), raw)
}

/// What it is, one sentence, and one primary action.
pub(crate) fn empty_block(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    action: AnyElement,
) -> AnyElement {
    let theme = theme();
    div()
        .w_full()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .items_start()
        .gap_2()
        .p_3()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(title.into()),
        )
        .child(crate::text::wrap_muted(detail))
        .child(action)
        .into_any_element()
}

/// Muted placeholder rows. No motion: the blocks are enough to say "still loading".
pub(crate) fn loading_block(id: &'static str, rows: usize) -> AnyElement {
    div()
        .w_full()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .gap_2()
        .py_1()
        .child(skeleton_list(id, rows))
        .into_any_element()
}

/// A plain-English sentence, the technical code on hover of Details, and Retry.
pub(crate) fn error_block(
    id: impl Into<gpui::ElementId>,
    sentence: impl Into<SharedString>,
    details: impl Into<SharedString>,
    retry: AnyElement,
) -> AnyElement {
    let theme = theme();
    let id = id.into();
    div()
        .w_full()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .items_start()
        .gap_2()
        .child(crate::text::wrap(sentence).text_sm().text_color(theme.err))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(with_tip(
                    (id, "details"),
                    div().text_xs().text_color(theme.muted).child("Details"),
                    details,
                    None,
                ))
                .child(retry),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_is_not_empty_even_when_the_list_has_nothing() {
        assert_eq!(list_region(true, false, 0), Region::Loading);
        assert_eq!(list_region(true, true, 0), Region::Loading);
        assert_eq!(list_region(true, false, 4), Region::Loading);
        assert_eq!(list_region(false, true, 0), Region::Error);
        assert_eq!(list_region(false, true, 2), Region::Ready);
        assert_eq!(list_region(false, false, 0), Region::Empty);
        assert_eq!(list_region(false, false, 1), Region::Ready);
    }

    #[test]
    fn a_feed_that_has_not_arrived_is_loading() {
        assert_eq!(feed_region(false, false, false, 0), Region::Loading);
        assert_eq!(feed_region(true, false, true, 0), Region::Loading);
        assert_eq!(feed_region(true, true, false, 0), Region::Loading);
        assert_eq!(feed_region(false, true, false, 0), Region::Error);
        assert_eq!(feed_region(false, false, true, 0), Region::Empty);
        assert_eq!(feed_region(false, false, true, 3), Region::Ready);
        assert_eq!(feed_region(false, true, true, 3), Region::Ready);
    }

    fn chip_name(status: RemoteStatus) -> Option<(&'static str, &'static str)> {
        remote_chip(status).map(|(label, tone)| {
            let tone = match tone {
                Tone::Ok => "ok",
                Tone::Warn => "warn",
                Tone::Err => "err",
                Tone::Neutral => "neutral",
                Tone::Accent => "accent",
            };
            (label, tone)
        })
    }

    #[test]
    fn a_remote_chip_appears_only_after_a_test() {
        assert_eq!(remote_status(false, false, false), RemoteStatus::Unchecked);
        assert_eq!(chip_name(RemoteStatus::Unchecked), None);

        assert_eq!(remote_status(true, true, true), RemoteStatus::Testing);
        assert_eq!(chip_name(RemoteStatus::Testing), Some(("Testing…", "warn")));

        assert_eq!(remote_status(false, true, false), RemoteStatus::Connected);
        assert_eq!(
            chip_name(RemoteStatus::Connected),
            Some(("Connected", "ok"))
        );

        assert_eq!(remote_status(false, true, true), RemoteStatus::Failed);
        assert_eq!(chip_name(RemoteStatus::Failed), Some(("Failed", "err")));
    }

    #[test]
    fn busy_labels_replace_the_idle_text_only_while_busy() {
        assert_eq!(busy_label(true, "Testing…", "Test connection"), "Testing…");
        assert_eq!(
            busy_label(false, "Testing…", "Test connection"),
            "Test connection"
        );
        assert_eq!(busy_label(true, "Unlocking…", "Unlock"), "Unlocking…");
        assert_eq!(busy_label(true, "Saving…", "Save"), "Saving…");
        assert_eq!(busy_label(false, "Saving…", "Save"), "Save");
    }

    #[test]
    fn details_keep_the_code_and_error_parts_split_the_stored_line() {
        assert_eq!(
            details_tip("sync_failed", "rclone: boom"),
            "sync_failed\nrclone: boom"
        );
        assert_eq!(details_tip("sync_failed", ""), "sync_failed");
        assert_eq!(details_tip("", "boom"), "boom");
        assert_eq!(details_tip("", ""), "No technical code was reported.");
        let stored = format::explain("invalid_password", "auth: invalid password").display();
        let (sentence, code) = error_parts(&stored);
        assert_eq!(sentence, "That password is not right.");
        assert_eq!(code, "invalid_password");
        assert_eq!(
            error_parts("The snapshot did not parse"),
            ("The snapshot did not parse".into(), String::new())
        );
    }

    #[test]
    fn unlock_note_ellipsizes_the_folder_in_the_middle() {
        let path = "/Users/ada/Library/Application Support/gn-drive";
        let note = folder_note(path);
        assert_eq!(
            note.label,
            format!(
                "Settings folder: {}",
                format::middle_ellipsis(path, FOLDER_NOTE_CHARS)
            )
        );
        assert!(note.label.contains('…'));
        assert!(note.label.ends_with("gn-drive"));
        assert!(!note.label.contains("Application Support"));
        assert_eq!(note.full_path, path);
        assert!(note.full_path.contains("Application Support"));
        assert_eq!(note.copy, FOLDER_NOTE_COPY);
        assert!(note.copy.contains("secure hash"));
        assert!(note.copy.contains("never the password itself"));
        assert!(!note.copy.to_ascii_lowercase().contains("password is saved"));

        let short = folder_note("/tmp/gn");
        assert_eq!(short.label, "Settings folder: /tmp/gn");
        assert!(!short.label.contains('…'));
    }

    #[test]
    fn empty_copy_names_the_thing_and_the_next_step() {
        assert_eq!(flows_empty_copy().0, "No flows yet");
        assert_eq!(remotes_empty_copy().0, "No cloud accounts yet");
        assert_eq!(history_empty_copy().0, "No runs yet");
        assert_eq!(history_empty_copy().1, "Run a flow to see it here.");
        assert!(logs_empty_copy().1.to_lowercase().contains("refresh"));
    }

    #[test]
    fn simulate_names_round_trip_and_reset_clears() {
        for name in [
            "empty-flows",
            "empty-remotes",
            "loading",
            "error-engine",
            "error-test",
            "empty-history",
            "empty-logs",
            "wrong-password",
        ] {
            let sim = Simulation::parse(name).unwrap();
            assert_eq!(sim.key(), name);
        }
        assert_eq!(Simulation::parse("reset"), Some(Simulation::None));
        assert_eq!(Simulation::parse("none"), Some(Simulation::None));
        assert_eq!(Simulation::None.key(), "none");
        assert!(Simulation::parse("banner").is_none());
        let fault = simulated_test_fault();
        assert!(!fault.sentence.is_empty());
        assert_eq!(fault.code, "sync_failed");
        assert_eq!(simulated_engine_fault().code, "spawn_failed");
    }
}
