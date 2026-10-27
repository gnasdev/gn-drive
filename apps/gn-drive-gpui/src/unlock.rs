//! The lock screen: first-run setup and the password prompt.
//!
//! The password never leaves this window in clear text: it goes to
//! `auth.setup` or `auth.unlock` and is dropped from the fields on success.
//! A lockout (`locked_out` / `lockout` / `locked` plus a retry duration) is
//! one sentence with a live countdown. The raw engine text is not shown.
//! The field and the submit button stay off until that countdown reaches zero.

use std::time::Instant;

use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Focusable, IntoElement, SharedString, Window,
    div, prelude::*, px,
};
use ns_ui::{IconTone, theme};

use ely_gpui_component::forms::TextInput;

use crate::GnDriveWindow;
use crate::states::{self, folder_note};
use crate::tip::{
    Look, tip_button, tip_button_off, tip_button_off_wide, tip_button_wide, tip_icon, tip_icon_off,
    with_tip,
};

/// What the unlock card shows for a countdown. `remaining_secs == 0` is the
/// open state: no message, field and button usable again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LockoutUi {
    pub remaining_secs: u64,
    pub message: Option<String>,
    pub field_disabled: bool,
    pub submit_disabled: bool,
    pub submit_tooltip: String,
}

pub(crate) fn lockout_ui(remaining_secs: u64, setup: bool) -> LockoutUi {
    if remaining_secs == 0 {
        return LockoutUi {
            remaining_secs: 0,
            message: None,
            field_disabled: false,
            submit_disabled: false,
            submit_tooltip: if setup {
                "Save this password and open GN Drive".into()
            } else {
                "Unlock GN Drive with your master password".into()
            },
        };
    }
    LockoutUi {
        remaining_secs,
        message: Some(format!(
            "Too many wrong attempts. Try again in {remaining_secs} s."
        )),
        field_disabled: true,
        submit_disabled: true,
        submit_tooltip: format!("Locked for {remaining_secs} s after too many wrong attempts"),
    }
}

/// Whole seconds left. Zero means the countdown is over (or there isn't one).
pub(crate) fn remaining_secs(until: Option<Instant>, now: Instant) -> u64 {
    until
        .map(|until| until.saturating_duration_since(now).as_secs())
        .filter(|secs| *secs > 0)
        .unwrap_or(0)
}

/// Seconds from an auth error, when the code is a lockout.
///
/// `rpc.md` names `locked_out` (`data.retryAfterSecs`) and `locked` when the
/// message carries a retry. `lockout` is accepted as the same state. A bare
/// `locked` with no duration is the data-plane "app is locked" error, not a
/// countdown. Missing duration on `locked_out` / `lockout` defaults to 30s.
pub(crate) fn lockout_seconds(code: &str, message: &str, retry_after: Option<u64>) -> Option<u64> {
    let named = matches!(code, "locked_out" | "lockout" | "locked");
    if !named {
        return None;
    }
    // A reported 0 is "not locked". Only a missing duration uses the 30s default.
    let parsed = retry_after
        .filter(|secs| *secs > 0)
        .or_else(|| seconds_in(message));
    let secs = match (code, parsed) {
        ("locked", Some(secs)) if secs > 0 => secs,
        ("locked", _) => return None,
        (_, Some(0)) => return None,
        (_, Some(secs)) => secs,
        (_, None) => 30,
    };
    Some(secs.min(3600))
}

/// The engine's own lockout sentence, which must not sit next to the countdown.
pub(crate) fn is_raw_lockout_copy(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    let mentions = lower.contains("too many")
        || lower.contains("retry in")
        || lower.contains("locked out")
        || lower.contains("lockout");
    mentions && (seconds_in(message).is_some() || lower.contains("too many"))
}

/// The red error, or nothing while a countdown is up or the text is the raw
/// lockout string. After the countdown the raw string stays hidden.
pub(crate) fn show_auth_error<'a>(
    auth_error: Option<&'a str>,
    lockout: &LockoutUi,
) -> Option<&'a str> {
    auth_error.filter(|message| lockout.message.is_none() && !is_raw_lockout_copy(message))
}

fn seconds_in(message: &str) -> Option<u64> {
    let lower = message.to_ascii_lowercase();
    let marker = "retry in ";
    let start = lower.find(marker)? + marker.len();
    let rest = lower[start..].trim_start();
    let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// Below this window width the Unlock button spans the card. Wider windows
/// keep it right-aligned, with the shortcut hint on its own line underneath.
const NARROW_BELOW: f32 = 560.;

fn submit_is_full_width(width: f32) -> bool {
    width < NARROW_BELOW
}

fn prose(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .w_full()
        .min_w_0()
        .whitespace_normal()
        .child(text.into())
}

#[allow(clippy::too_many_arguments)]
fn password_row(
    field_id: &'static str,
    reveal_id: &'static str,
    input: &Entity<TextInput>,
    masked: bool,
    disabled: bool,
    value: &str,
    placeholder: &str,
    on_reveal: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = theme();
    let (glyph, tip) = if masked {
        ("eye", "Show password")
    } else {
        ("eye-off", "Hide password")
    };
    // The live field stays in the tree so the row keeps its height when the
    // countdown ends. While locked, an opaque face covers it: recessed fill,
    // quiet border, muted text, not-allowed cursor, and no focus ring.
    let mut live = div().w_full().min_w_0();
    if disabled {
        live = live.opacity(0.);
    }
    let mut field = div()
        .flex_1()
        .min_w_0()
        .relative()
        .child(live.child(ns_ui::text_field(field_id, input)));
    if disabled {
        let empty = value.is_empty();
        let shown = if empty {
            placeholder.to_string()
        } else if masked {
            "•".repeat(value.chars().count())
        } else {
            value.to_string()
        };
        field = field.child(
            div()
                .absolute()
                .inset_0()
                .px_3()
                .flex()
                .items_center()
                .rounded_md()
                .bg(theme.background)
                .border_1()
                .border_color(theme.border)
                .cursor_not_allowed()
                .occlude()
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted)
                        .opacity(if empty { 0.55 } else { 0.75 })
                        .child(shown),
                ),
        );
    }
    let eye = if disabled {
        tip_icon_off(reveal_id, glyph, "Unavailable until you can try again")
    } else {
        tip_icon(reveal_id, glyph, tip, None, IconTone::Quiet, on_reveal)
    };
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap_2()
        .child(field)
        .child(div().flex_shrink_0().child(eye))
        .into_any_element()
}

/// Turn the field off while locked out, and put the caret back when it opens.
fn sync_lock(
    this: &mut GnDriveWindow,
    disabled: bool,
    window: &mut Window,
    cx: &mut Context<GnDriveWindow>,
) {
    let mut changed = false;
    for input in [&this.password_input, &this.confirm_input] {
        if input.read(cx).is_disabled() == disabled {
            continue;
        }
        let handle = input.focus_handle(cx);
        input.update(cx, |field, cx| field.set_disabled(disabled, cx));
        changed = true;
        if disabled && handle.is_focused(window) {
            window.defer(cx, |window, cx| window.blur(cx));
        }
    }
    if changed && !disabled {
        let handle = this.password_input.focus_handle(cx);
        window.defer(cx, move |window, cx| {
            handle.focus(window, cx);
        });
    }
}

pub(crate) fn screen(
    this: &mut GnDriveWindow,
    window: &mut Window,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let theme = theme();
    let setup = !this.auth().setup && this.simulation != states::Simulation::WrongPassword;
    let busy = this.auth_busy;
    let state = lockout_ui(remaining_secs(this.lockout_until, Instant::now()), setup);
    sync_lock(this, state.field_disabled, window, cx);

    let title = if setup {
        "Set a master password"
    } else {
        "Unlock GN Drive"
    };
    let detail = if setup {
        "Your flows, cloud accounts, and run history sit behind this password. Use at least four characters. It is stored only on this Mac, so choose one you will remember."
    } else {
        "Enter the master password to see your flows. GN Drive asks again whenever it is locked."
    };

    let mut card = div()
        .w(px(400.))
        .max_w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_4()
        .p_6()
        .rounded_xl()
        .bg(theme.panel)
        .border_1()
        .border_color(theme.border);

    card = card
        .child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(40.))
                        .rounded_lg()
                        .bg(theme.raised)
                        .border_1()
                        .border_color(theme.border)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ns_ui::icon_tinted("server", theme.accent)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(
                            prose(title)
                                .text_base()
                                .font_weight(gpui::FontWeight::SEMIBOLD),
                        )
                        .when(state.field_disabled, |column| {
                            column
                                .child(div().flex().child(ns_ui::chip("Locked", ns_ui::Tone::Warn)))
                        }),
                ),
        )
        .child(prose(detail).text_xs().text_color(theme.muted));

    let masked = this.password_input.read(cx).is_masked();
    let password_value = this.password_input.read(cx).text().to_string();
    let password_hint = this.password_input.read(cx).placeholder_text().to_string();
    card = card.child(
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                prose(if setup { "Password" } else { "Master password" })
                    .text_xs()
                    .text_color(theme.muted),
            )
            .child(password_row(
                "auth-password",
                "auth-reveal",
                &this.password_input,
                masked,
                state.field_disabled,
                &password_value,
                &password_hint,
                cx.listener(|this, _, _, cx| {
                    this.password_input.update(cx, |input, cx| {
                        input.set_masked(!input.is_masked(), cx);
                    });
                }),
            )),
    );
    if setup {
        let confirm_masked = this.confirm_input.read(cx).is_masked();
        let confirm_value = this.confirm_input.read(cx).text().to_string();
        let confirm_hint = this.confirm_input.read(cx).placeholder_text().to_string();
        card = card.child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_2()
                .child(prose("Repeat password").text_xs().text_color(theme.muted))
                .child(password_row(
                    "auth-confirm",
                    "auth-confirm-reveal",
                    &this.confirm_input,
                    confirm_masked,
                    state.field_disabled,
                    &confirm_value,
                    &confirm_hint,
                    cx.listener(|this, _, _, cx| {
                        this.confirm_input.update(cx, |input, cx| {
                            input.set_masked(!input.is_masked(), cx);
                        });
                    }),
                )),
        );
    }

    if let Some(error) = show_auth_error(this.auth_error.as_deref(), &state) {
        let mut line = div().w_full().min_w_0().flex().flex_col().gap_1().child(
            prose(error)
                .w_full()
                .min_w_0()
                .text_xs()
                .text_color(theme.err),
        );
        if let Some(code) = this.auth_error_code.clone() {
            line = line.child(with_tip(
                "auth-error-details",
                div().text_xs().text_color(theme.muted).child("Details"),
                states::details_tip(&code, error),
                None,
            ));
        }
        card = card.child(line);
    }
    if let Some(message) = state.message.clone() {
        card = card.child(
            prose(message)
                .rounded_md()
                .p_2p5()
                .bg(theme.warn_bg)
                .text_xs()
                .text_color(theme.warn),
        );
    }

    let label: SharedString = states::busy_label(
        busy,
        if setup { "Saving…" } else { "Unlocking…" },
        if setup { "Set password" } else { "Unlock" },
    )
    .into();
    let glyph = if setup {
        "check"
    } else if busy || state.submit_disabled {
        "lock"
    } else {
        "lock-open"
    };
    let narrow = submit_is_full_width(window.viewport_size().width.as_f32());
    let tip = if busy {
        if setup {
            "Saving the password on this Mac".into()
        } else {
            "Unlocking GN Drive".into()
        }
    } else {
        state.submit_tooltip.clone()
    };
    let off = busy || state.submit_disabled;
    let submit = if off && narrow {
        tip_button_off_wide("auth-submit", glyph, label, tip, None, Look::Primary)
    } else if off {
        tip_button_off("auth-submit", glyph, label, tip, None, Look::Primary)
    } else if narrow {
        tip_button_wide(
            "auth-submit",
            glyph,
            label,
            tip,
            Some("↵"),
            Look::Primary,
            cx.listener(|this, _, _, cx| this.submit_auth(cx)),
        )
    } else {
        tip_button(
            "auth-submit",
            glyph,
            label,
            tip,
            Some("↵"),
            Look::Primary,
            cx.listener(|this, _, _, cx| this.submit_auth(cx)),
        )
    };
    let hint = if setup {
        "Local only. Nothing is sent anywhere."
    } else {
        "Press Enter to unlock. ⌘⇧L locks the window again."
    };
    let actions = div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap_2()
        .when(!narrow, |row| row.justify_end())
        .child(submit);
    card = card.child(
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(actions)
            .child(prose(hint).text_xs().text_color(theme.muted)),
    );

    if let Some(paths) = this
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.paths.clone())
        && !paths.config_dir.is_empty()
    {
        let note = folder_note(&paths.config_dir);
        card = card.child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(with_tip(
                    "auth-folder",
                    div()
                        .w_full()
                        .min_w_0()
                        .text_xs()
                        .text_color(theme.muted)
                        .child(note.label.clone()),
                    note.full_path,
                    None,
                ))
                .child(prose(note.copy).text_xs().text_color(theme.muted)),
        );
    }

    let mut page = div()
        .size_full()
        .min_w_0()
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .bg(theme.background)
        .text_color(theme.text)
        .child(card);
    page = page.child(
        div()
            .absolute()
            .bottom(px(16.))
            .left(px(16.))
            .right(px(16.))
            .flex()
            .justify_center()
            .child(
                prose("Files sync on this Mac · no account needed, nothing is sent to us")
                    .max_w_full()
                    .text_center()
                    .text_xs()
                    .text_color(theme.muted),
            ),
    );
    page.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn countdown_message_disables_then_reopens_at_zero() {
        let mut secs = 23;
        let locked = lockout_ui(secs, false);
        assert_eq!(
            locked.message.as_deref(),
            Some("Too many wrong attempts. Try again in 23 s.")
        );
        assert!(locked.field_disabled);
        assert!(locked.submit_disabled);
        assert_eq!(
            locked.submit_tooltip,
            "Locked for 23 s after too many wrong attempts"
        );
        assert!(show_auth_error(Some("too many attempts, retry in 30s"), &locked).is_none());
        assert!(show_auth_error(Some("invalid password"), &locked).is_none());

        while secs > 0 {
            secs = secs.saturating_sub(1);
        }
        let open = lockout_ui(secs, false);
        assert_eq!(secs, 0);
        assert!(open.message.is_none());
        assert!(!open.field_disabled);
        assert!(!open.submit_disabled);
        assert_eq!(
            open.submit_tooltip,
            "Unlock GN Drive with your master password"
        );
        assert!(show_auth_error(Some("too many attempts, retry in 30s"), &open).is_none());
        assert_eq!(
            show_auth_error(Some("invalid password"), &open),
            Some("invalid password")
        );
        assert_eq!(
            lockout_ui(0, true).submit_tooltip,
            "Save this password and open GN Drive"
        );
    }

    #[test]
    fn remaining_hits_zero_at_the_deadline() {
        let now = Instant::now();
        assert_eq!(remaining_secs(Some(now + Duration::from_secs(23)), now), 23);
        assert_eq!(remaining_secs(Some(now), now), 0);
        assert_eq!(remaining_secs(None, now), 0);
    }

    #[test]
    fn engine_lockout_codes_map_to_seconds() {
        assert_eq!(
            lockout_seconds(
                "locked_out",
                "auth: too many attempts, retry in 30s",
                Some(30)
            ),
            Some(30)
        );
        assert_eq!(
            lockout_seconds("locked_out", "auth: too many attempts, retry in 30s", None),
            Some(30)
        );
        assert_eq!(
            lockout_seconds("locked", "auth: locked — retry in 42s", None),
            Some(42)
        );
        assert_eq!(lockout_seconds("lockout", "retry in 7 s", None), Some(7));
        assert_eq!(
            lockout_seconds("locked_out", "auth: locked — retry in 5s", Some(5)),
            Some(5)
        );
        assert_eq!(
            lockout_seconds("locked_out", "auth: locked out", None),
            Some(30)
        );
        assert_eq!(lockout_seconds("locked", "auth: app is locked", None), None);
        assert_eq!(
            lockout_seconds("invalid_password", "auth: invalid password", None),
            None
        );
        assert_eq!(lockout_seconds("locked_out", "retry in 0s", Some(0)), None);
        assert!(is_raw_lockout_copy("too many attempts, retry in 30s"));
        assert!(is_raw_lockout_copy("locked — retry in 42s"));
        assert!(!is_raw_lockout_copy("invalid password"));
        assert!(!is_raw_lockout_copy("Enter the password."));
    }

    #[test]
    fn settings_note_names_the_hash_and_shortens_the_path() {
        let note = folder_note("/Users/ada/Library/Application Support/gn-drive");
        assert!(note.label.starts_with("Settings folder: "));
        assert!(note.full_path.contains("Users"));
        assert!(note.full_path.contains("Application Support"));
        assert!(note.copy.contains("secure hash"));
        assert!(note.copy.contains("never the password itself"));
        assert!(!note.copy.to_ascii_lowercase().contains("password is saved"));
        assert!(note.label.contains('…'));
        assert!(note.label.ends_with("gn-drive"));
    }

    #[test]
    fn a_narrow_window_stretches_unlock_and_a_wide_one_does_not() {
        assert!(submit_is_full_width(390.));
        assert!(submit_is_full_width(559.));
        assert!(!submit_is_full_width(560.));
        assert!(!submit_is_full_width(1440.));
    }
}
