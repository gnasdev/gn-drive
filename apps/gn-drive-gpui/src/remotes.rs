//! Cloud accounts: every rclone remote the engine knows, with Test and
//! Delete per row and the one entry point for adding or reconnecting.
//!
//! Authorization never starts by itself. OAuth only begins from the explicit
//! Add remote dialog, which Reconnect opens preset to the remote's name and
//! type.

use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use ns_ui::{Tone, chip, theme};

use crate::GnDriveWindow;
use crate::engine::Remote;
use crate::states::{self, Region, RemoteStatus};
use crate::tip::{Look, tip_button, tip_button_off};

impl GnDriveWindow {
    /// Whether the provider catalog marks `kind` as an OAuth type.
    ///
    /// Before the catalog arrives, the well-known browser sign-in types still
    /// offer Reconnect. A loaded catalog that says no is trusted.
    fn remote_is_oauth(&self, kind: &str) -> bool {
        if let Some(provider) = self.providers.iter().find(|provider| provider.name == kind) {
            return provider.oauth;
        }
        self.providers.is_empty()
            && matches!(
                kind,
                "drive" | "dropbox" | "onedrive" | "box" | "pcloud" | "yandex" | "googlephotos"
            )
    }

    /// Open the Add remote dialog preset to an existing remote's name and
    /// type. This only fills the form; the user still presses the button that
    /// starts authorization.
    pub(crate) fn reconnect_remote(
        &mut self,
        name: &str,
        kind: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_add_remote(cx);
        let name = name.to_string();
        self.add_name_input
            .update(cx, |input, cx| input.set_text(name, cx));
        self.pick_provider(kind, window, cx);
    }
}

/// The add button, pinned above the scrolling list.
pub(crate) fn pinned(cx: &mut Context<GnDriveWindow>) -> AnyElement {
    tip_button(
        "remotes-add",
        "plus",
        "Add cloud account",
        "Connect a cloud account or server (Google Drive, Dropbox, a NAS…) so your flows can reach it",
        Some("⌘⇧R"),
        Look::Primary,
        cx.listener(|this, _, _, cx| this.open_add_remote(cx)),
    )
}

/// The pinned Add button stays only when the list itself is showing accounts,
/// so an empty page has a single primary action.
pub(crate) fn pin_add(this: &GnDriveWindow) -> bool {
    matches!(accounts_region(this), Region::Ready)
}

/// Accounts the page and `state.remotes` should count. Empty and error-test
/// previews do not rewrite the mock engine.
pub(crate) fn visible_remotes(this: &GnDriveWindow) -> Vec<Remote> {
    if this.simulation == states::Simulation::EmptyRemotes {
        return Vec::new();
    }
    let mut remotes = this.remotes().as_ref().to_vec();
    if this.simulation == states::Simulation::ErrorTest && remotes.is_empty() {
        remotes.push(Remote {
            name: "gdrive".into(),
            kind: "drive".into(),
            description: "Google Drive".into(),
        });
    }
    remotes
}

fn accounts_region(this: &GnDriveWindow) -> Region {
    match this.simulation {
        states::Simulation::Loading => return Region::Loading,
        states::Simulation::EmptyRemotes => return Region::Empty,
        states::Simulation::ErrorTest => return Region::Ready,
        _ => {}
    }
    let remotes = this.remotes();
    let loading = (this.snapshot.is_none() && this.load_error.is_none())
        || (this.snapshot_pending && remotes.is_empty() && this.load_error.is_none());
    states::list_region(loading, this.load_error.is_some(), remotes.len())
}

fn add_account(id: &'static str, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    tip_button(
        id,
        "plus",
        "Add cloud account",
        "Connect a cloud account or server so your flows can reach it",
        None,
        Look::Primary,
        cx.listener(|this, _, _, cx| this.open_add_remote(cx)),
    )
}

/// The account list. The page shell supplies the width and the scroll.
pub(crate) fn body(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let body = match accounts_region(this) {
        Region::Loading => states::loading_block("remotes-loading", 5),
        Region::Error => {
            let stored = this.load_error.clone().unwrap_or_default();
            let (sentence, code) = states::error_parts(&stored);
            states::error_block(
                "remotes-error",
                sentence,
                states::details_tip(&code, &stored),
                tip_button(
                    "remotes-retry",
                    "refresh",
                    "Retry",
                    "Load your cloud accounts again",
                    None,
                    Look::Primary,
                    cx.listener(|this, _, _, cx| this.reload(cx)),
                ),
            )
        }
        Region::Empty => {
            let (title, detail) = states::remotes_empty_copy();
            states::empty_block(title, detail, add_account("remotes-empty-add", cx))
        }
        Region::Ready => {
            let remotes = visible_remotes(this);
            let mut list = div().w_full().min_w(px(0.)).flex().flex_col().gap_2();
            for (index, remote) in remotes.iter().enumerate() {
                list = list.child(remote_row(this, remote, index, cx));
            }
            list.child(
                crate::text::wrap_muted(
                    "Signing in only starts when you press Add cloud account. If a test fails, \
                     Reconnect appears so you can sign in again.",
                )
                .px_2()
                .py_1(),
            )
            .into_any_element()
        }
    };
    div().w_full().min_w(px(0.)).child(body).into_any_element()
}

/// A provider id as a short name. Unknown ids keep a capital and the rest.
pub(crate) fn provider_label(kind: &str) -> String {
    match kind {
        "" => "Unknown".into(),
        "drive" => "Google Drive".into(),
        "dropbox" => "Dropbox".into(),
        "sftp" => "SFTP".into(),
        "onedrive" => "OneDrive".into(),
        "box" => "Box".into(),
        "pcloud" => "pCloud".into(),
        "iclouddrive" | "icloud" => "iCloud".into(),
        "yandex" => "Yandex".into(),
        "googlephotos" => "Google Photos".into(),
        "s3" => "S3".into(),
        "webdav" => "WebDAV".into(),
        "ftp" => "FTP".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => {
                    let mut out = String::new();
                    out.extend(first.to_uppercase());
                    out.push_str(chars.as_str());
                    out
                }
                None => "Unknown".into(),
            }
        }
    }
}

fn remote_row(
    this: &GnDriveWindow,
    remote: &Remote,
    index: usize,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let theme = theme();
    let name = remote.name.clone();
    let kind = remote.kind.clone();
    let oauth = this.remote_is_oauth(&kind);
    let preview_fail = this.simulation == states::Simulation::ErrorTest && index == 0;
    let testing = !preview_fail && this.remote_test_in_flight(&name);
    let failed = preview_fail || this.remote_failed.contains(&name);
    let connected = !preview_fail && this.remote_ok.contains(&name);
    let status = states::remote_status(testing, connected, failed);
    let fault = if preview_fail {
        Some(states::simulated_test_fault())
    } else {
        this.remote_faults.get(&name).cloned()
    };

    let test_name = name.clone();
    let delete_name = name.clone();
    let test_label = states::busy_label(testing, "Testing…", "Test connection");
    let test_tip = if testing {
        "Checking this cloud account"
    } else {
        "Check that GN Drive can still reach this cloud account"
    };
    let mut actions = div().flex().flex_wrap().items_center().gap_1().max_w_full();
    actions = actions.child(if testing {
        tip_button_off(
            format!("remote-test-{name}"),
            "plug",
            test_label,
            test_tip,
            None,
            Look::Quiet,
        )
    } else {
        tip_button(
            format!("remote-test-{name}"),
            "plug",
            test_label,
            test_tip,
            None,
            Look::Quiet,
            cx.listener(move |this, _, _, cx| this.test_remote(test_name.clone(), cx)),
        )
    });
    actions = actions.child(tip_button(
        format!("remote-delete-{name}"),
        "trash",
        "Remove…",
        format!(
            "Remove cloud account \"{name}\". Flows that use it stop working until you choose another"
        ),
        None,
        Look::OutlineDanger,
        cx.listener(move |this, _, _, cx| this.delete_remote(delete_name.clone(), cx)),
    ));

    let mut meta = div()
        .w_full()
        .min_w(px(0.))
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(chip(
            provider_label(&kind),
            if oauth { Tone::Accent } else { Tone::Neutral },
        ));
    if !remote.description.is_empty() {
        meta = meta.child(
            div().flex_1().min_w(px(0.)).child(
                crate::text::clip(
                    SharedString::from(format!("remote-desc-{name}")),
                    remote.description.clone(),
                )
                .text_xs()
                .text_color(theme.muted),
            ),
        );
    }
    if let Some((label, tone)) = states::remote_chip(status) {
        let mut mark = div()
            .flex()
            .items_center()
            .gap_1()
            .flex_shrink_0()
            .child(chip(label, tone));
        if status == RemoteStatus::Testing {
            mark = mark.child(
                div()
                    .size(px(8.))
                    .rounded_full()
                    .flex_shrink_0()
                    .bg(theme.warn),
            );
        }
        meta = meta.child(mark);
    }

    let mut row = div()
        .w_full()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .rounded_md()
        .bg(theme.editor)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .w_full()
                .min_w(px(0.))
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    div().flex_1().min_w(px(0.)).text_sm().child(
                        crate::text::clip(
                            SharedString::from(format!("remote-name-{name}")),
                            name.clone(),
                        )
                        .text_sm(),
                    ),
                )
                .child(actions),
        )
        .child(meta);

    if status == RemoteStatus::Failed
        && let Some(fault) = fault
    {
        let retry_name = name.clone();
        row = row.child(states::error_block(
            SharedString::from(format!("remote-fault-{name}")),
            fault.sentence.clone(),
            states::details_tip(&fault.code, &fault.raw),
            tip_button(
                format!("remote-retry-{name}"),
                "refresh",
                "Retry",
                "Test this cloud account again",
                None,
                Look::Outline,
                cx.listener(move |this, _, _, cx| this.test_remote(retry_name.clone(), cx)),
            ),
        ));
    }

    if oauth && status == RemoteStatus::Failed {
        let reconnect_name = name.clone();
        let reconnect_kind = kind.clone();
        row = row.child(div().child(tip_button(
            format!("remote-reconnect-{name}"),
            "refresh",
            "Reconnect",
            "Sign in again in your browser; GN Drive never sees your password",
            None,
            Look::Outline,
            cx.listener(move |this, _, window, cx| {
                this.reconnect_remote(&reconnect_name, &reconnect_kind, window, cx)
            }),
        )));
    }
    row.into_any_element()
}
