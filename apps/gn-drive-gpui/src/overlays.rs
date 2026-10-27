//! Modal surfaces drawn over the window: the new-flow, add-remote, OAuth,
//! folder-browser, and password dialogs, plus the confirmation card. They
//! only render; every action is a `GnDriveWindow` method.

use ely_gpui_component::overlays::Dialog;
use gpui::{AnyElement, ClipboardItem, Context, SharedString, Window, div, prelude::*, px, rgba};
use ns_ui::{
    Ask, Tone, ask_banner, chip, icon, section_label, skeleton_list, text_field, theme, working_row,
};

use crate::format::human_bytes;
use crate::states;
use crate::tip::{Look, tip_button, tip_button_off, tip_on};
use crate::{AskAction, GnDriveWindow, Overlay};

/// Draws the open dialog, or nothing.
pub(crate) fn render(
    this: &mut GnDriveWindow,
    window: &mut Window,
    cx: &mut Context<GnDriveWindow>,
) -> Option<AnyElement> {
    let _ = window;
    match this.overlay.clone() {
        Overlay::None => None,
        Overlay::NewFlow => Some(new_flow(this, cx)),
        Overlay::AddRemote => Some(add_remote(this, cx)),
        Overlay::OAuth => Some(oauth(this, cx)),
        Overlay::Browse(_) => Some(browse(this, cx)),
        Overlay::ChangePassword => Some(change_password(this, cx)),
        Overlay::RemovePassword => Some(remove_password(this, cx)),
    }
}

/// The confirmation card, centered over a scrim. Nothing runs until the user
/// answers; the caller's `Ask` carries all the wording.
pub(crate) fn confirm(
    ask: Ask,
    _action: AskAction,
    _this: &mut GnDriveWindow,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let card = ask_banner(
        &ask,
        cx.listener(|this, _, _, cx| this.confirm_ask(cx)),
        cx.listener(|this, _, _, cx| {
            this.ask = None;
            cx.notify();
        }),
    );
    div()
        .id("ask-scrim")
        .absolute()
        .inset_0()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .p_4()
        .bg(rgba(0x0000_00a0))
        .child(
            div()
                .id("ask-card")
                .w_full()
                .max_w(px(440.))
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.ask = None;
                    cx.notify();
                }))
                .child(card),
        )
        .into_any_element()
}

// MARK: - Shared pieces

fn dialog(
    id: &'static str,
    title: &'static str,
    detail: &'static str,
    cx: &mut Context<GnDriveWindow>,
) -> Dialog {
    let entity = cx.entity();
    let mut dialog = Dialog::new(id, title, move |_window, app| {
        entity.update(app, |this, cx| this.close_overlay(cx));
    })
    .detail(detail);
    // Ely's card is a fixed 440px; a narrow window gets the full-width form.
    if ns_ui::is_compact() {
        dialog = dialog.fullscreen();
    }
    dialog
}

fn column() -> gpui::Div {
    div().w_full().min_w(px(0.)).flex().flex_col().gap_2()
}

/// The dialog footer is one row and clips a long primary button. On a compact
/// window the buttons move into a wrapping row in the body. A wide window
/// keeps them in the footer.
fn attach_actions(mut dialog: Dialog, actions: Vec<AnyElement>) -> AnyElement {
    if ns_ui::is_compact() {
        let mut row = div()
            .w_full()
            .min_w(px(0.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2();
        for action in actions {
            row = row.child(action);
        }
        dialog.child(row).into_any_element()
    } else {
        for action in actions {
            dialog = dialog.action(move |_| action);
        }
        dialog.into_any_element()
    }
}

fn note(text: impl Into<SharedString>) -> gpui::Div {
    crate::text::wrap_muted(text).px_2().py_3()
}

fn error_line(text: impl Into<SharedString>) -> gpui::Div {
    let theme = theme();
    crate::text::wrap_err(text)
        .px_2()
        .py_1()
        .rounded_md()
        .bg(theme.err_bg)
}

fn cancel_button(id: &'static str, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    tip_button(
        id,
        "back",
        "Cancel",
        "Close this window without making any changes",
        Some("Esc"),
        Look::Outline,
        cx.listener(|this, _, _, cx| this.close_overlay(cx)),
    )
}

// MARK: - New flow

fn new_flow(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let body = column()
        .child(section_label("Flow name"))
        .child(text_field("new-flow-name", &this.new_flow_input))
        .child(note(
            "A flow is a set of folders that stay in sync. Pick where files come from and where they go.",
        ));
    let cancel = cancel_button("new-flow-cancel", cx);
    let create = tip_button(
        "new-flow-create",
        "plus",
        "Create",
        "Create the flow and open it so you can add folders",
        None,
        Look::Primary,
        cx.listener(|this, _, _, cx| this.submit_new_flow(cx)),
    );
    attach_actions(
        dialog("new-flow", "New flow", "You can rename it later.", cx).child(body),
        vec![cancel, create],
    )
}

// MARK: - Add remote

fn add_remote(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let theme = theme();
    let mut body = column()
        .child(section_label("Account name"))
        .child(text_field("add-remote-name", &this.add_name_input))
        .child(note(
            "Pick a short name you will recognise, like “nas” or “work-drive”.",
        ))
        .child(section_label("Service"));

    if this.providers.is_empty() {
        body = body
            .child(working_row(
                "add-remote-loading",
                "Loading the list of cloud services…",
            ))
            .child(skeleton_list("add-remote-skeleton", 5));
    } else {
        let mut list = div()
            .id("add-remote-providers")
            .w_full()
            .max_h(px(220.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .rounded_md()
            .border_1()
            .border_color(theme.border);
        let mut shown = 0;
        for provider in this.providers.iter().filter(|provider| !provider.hidden) {
            shown += 1;
            let selected = this.add_provider.as_deref() == Some(provider.name.as_str());
            let kind = provider.name.clone();
            let mut row = div()
                .id(SharedString::from(format!("provider-{}", provider.name)))
                .w_full()
                .min_w(px(0.))
                .px_2()
                .py_1()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .cursor_pointer()
                .hover(move |style| style.bg(theme.hover))
                .when(selected, move |row| row.bg(theme.selected))
                .child(
                    div().w(px(96.)).max_w_full().min_w(px(0.)).child(
                        crate::text::clip(
                            SharedString::from(format!("provider-name-{}", provider.name)),
                            crate::remotes::provider_label(&provider.name),
                        )
                        .font_weight(gpui::FontWeight::MEDIUM),
                    ),
                )
                .child(
                    div().flex_1().min_w_0().child(
                        crate::text::clip(
                            SharedString::from(format!("provider-desc-{}", provider.name)),
                            provider.description.clone(),
                        )
                        .text_xs()
                        .text_color(theme.muted),
                    ),
                );
            if provider.oauth {
                row = row.child(chip("Browser sign-in", Tone::Accent));
            }
            let row = tip_on(
                row,
                format!(
                    "Choose {} for this cloud account. {}",
                    crate::remotes::provider_label(&provider.name),
                    provider.description
                ),
                None,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.pick_provider(&kind, window, cx);
            }));
            list = list.child(row);
        }
        if shown == 0 {
            body = body.child(note(
                "No cloud services are available right now. Close this window and try again.",
            ));
        } else {
            body = body.child(list);
        }
    }

    let oauth = this
        .add_provider
        .as_deref()
        .and_then(|kind| this.providers.iter().find(|provider| provider.name == kind))
        .map(|provider| provider.oauth)
        .unwrap_or(false);

    if this.add_provider.is_some() {
        if oauth {
            body = body.child(note(
                "Next, you will sign in with your account in the browser. GN Drive never sees your password.",
            ));
        } else if this.add_options.is_empty() {
            body = body.child(note("Nothing else to fill in for this service."));
        } else {
            body = body.child(section_label("Settings"));
            if this.add_busy {
                body = body.child(working_row("add-remote-busy", "Saving this cloud account…"));
            }
            if let Some(error) = &this.add_error {
                body = body.child(error_line(error.clone()));
            }
            for option in &this.add_options {
                let Some(input) = this.add_fields.get(&option.name).cloned() else {
                    continue;
                };
                let secret = option.is_password || option.sensitive;
                let name = option.name.clone();
                let revealed = this.revealed_secrets.contains(&name);
                let label = if secret {
                    format!("{name} (kept private)")
                } else {
                    name.clone()
                };
                let mut row =
                    column()
                        .child(crate::text::wrap_muted(label).px_1())
                        .child(text_field(
                            SharedString::from(format!("add-field-{name}")),
                            &input,
                        ));
                if secret {
                    let field = input.clone();
                    let key = name.clone();
                    row = row.child(tip_button(
                        format!("add-reveal-{name}"),
                        if revealed { "eye-off" } else { "eye" },
                        if revealed { "Hide" } else { "Show" },
                        "Show or hide this secret. It is never written to the log",
                        None,
                        Look::Quiet,
                        cx.listener(move |this, _, _, cx| {
                            let show = !this.revealed_secrets.contains(&key);
                            if show {
                                this.revealed_secrets.insert(key.clone());
                            } else {
                                this.revealed_secrets.remove(&key);
                            }
                            field.update(cx, |input, cx| input.set_masked(!show, cx));
                            cx.notify();
                        }),
                    ));
                }
                body = body.child(row);
            }
        }
    }

    let cancel = cancel_button("add-remote-cancel", cx);
    let saving = this.add_busy || this.oauth_busy;
    let submit = if saving {
        tip_button_off(
            "add-remote-submit",
            if oauth { "globe" } else { "plus" },
            states::busy_label(true, "Saving…", "Add"),
            "Wait for this cloud account to finish saving",
            None,
            Look::Primary,
        )
    } else {
        tip_button(
            "add-remote-submit",
            if oauth { "globe" } else { "plus" },
            if oauth { "Sign in" } else { "Add" },
            if oauth {
                "Open your browser to sign in and connect this cloud account"
            } else {
                "Save this cloud account so your flows can use it"
            },
            None,
            Look::Primary,
            cx.listener(|this, _, _, cx| this.submit_add_remote(cx)),
        )
    };
    attach_actions(
        dialog(
            "add-remote",
            "Add cloud account",
            "Name it, then pick the service.",
            cx,
        )
        .child(body),
        vec![cancel, submit],
    )
}

// MARK: - OAuth

fn oauth(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let theme = theme();
    let status = if this.oauth_status.is_empty() {
        "starting".to_string()
    } else {
        this.oauth_status.clone()
    };
    let tone = match status.as_str() {
        "authorized" => Tone::Ok,
        "failed" | "error" | "expired" | "cancelled" => Tone::Warn,
        _ => Tone::Accent,
    };
    let mut body = column().child(section_label("Sign-in progress")).child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(chip(status, tone)),
    );
    if this.oauth_url.is_empty() {
        body = body
            .child(working_row("oauth-wait", "Preparing the sign-in link…"))
            .child(skeleton_list("oauth-skeleton", 2));
    } else {
        body = body
            .child(note(
                "Open the link, sign in with your account in the browser, and approve access. GN Drive never sees your password. This window updates when you are done.",
            ))
            .child(
                div()
                    .id("oauth-url")
                    .w_full()
                    .min_w_0()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.editor)
                    .tooltip(ely_gpui_component::primitives::Tooltip::text(
                        this.oauth_url.clone(),
                    ))
                    .child(
                        crate::text::wrap(this.oauth_url.clone())
                            .text_xs()
                            .text_color(theme.muted),
                    ),
            );
    }

    // The dialog footer is one row and clips when four buttons do not fit.
    // A wrapping row inside the body stays inside the card at every width.
    let mut actions = div().w_full().flex().flex_wrap().gap_2();
    actions = actions.child(cancel_button("oauth-cancel", cx));
    if !this.oauth_url.is_empty() {
        actions = actions.child(tip_button(
            "oauth-copy",
            "copy",
            "Copy link",
            "Copy the sign-in link to the clipboard",
            None,
            Look::Outline,
            cx.listener(|this, _, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(this.oauth_url.clone()));
            }),
        ));
        actions = actions.child(tip_button(
            "oauth-open",
            "globe",
            "Open in browser",
            "Open the sign-in page in your web browser",
            None,
            Look::Primary,
            cx.listener(|this, _, _, cx| {
                if !this.oauth_url.is_empty() {
                    cx.open_url(&this.oauth_url);
                }
            }),
        ));
    }
    if this.is_mock() {
        actions = actions.child(tip_button(
            "oauth-mock",
            "check",
            "Complete (mock)",
            "Pretend the sign-in finished (demo mode only)",
            None,
            Look::Outline,
            cx.listener(|this, _, _, cx| this.complete_oauth_mock(cx)),
        ));
    }
    body = body.child(actions);
    dialog(
        "oauth",
        "Sign in to your cloud account",
        "Finish in your browser.",
        cx,
    )
    .child(body)
    .into_any_element()
}

// MARK: - Folder browser

fn browse(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let theme = theme();
    let up_target = this.browse_parent();
    let up = tip_button(
        "browse-up",
        "back",
        "Up",
        "Go to the parent folder",
        None,
        Look::Outline,
        cx.listener(move |this, _, _, cx| this.load_folder(up_target.clone(), cx)),
    );
    let go = tip_button(
        "browse-go",
        "folder",
        "Go",
        "Open the folder path typed in the box",
        Some("↵"),
        Look::Outline,
        cx.listener(|this, _, _, cx| {
            let path = this.browse_input.read(cx).text().to_string();
            this.load_folder(path, cx);
        }),
    );
    let bar = div()
        .w_full()
        .min_w(px(0.))
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(up)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(text_field("browse-path", &this.browse_input)),
        )
        .child(go);

    let mut list = div()
        .id("browse-list")
        .w_full()
        .h(px(260.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .rounded_md()
        .border_1()
        .border_color(theme.border);
    if this.browse_loading {
        list = list
            .child(working_row("browse-loading", "Opening folder…"))
            .child(skeleton_list("browse-skeleton", 6));
    } else if let Some(error) = this.browse_error.clone() {
        list = list.child(div().p_2().child(error_line(error)));
    } else if this.browse_entries.is_empty() {
        list = list.child(note(
            "This folder is empty. Press “Choose this folder” to use it, or go up to pick another.",
        ));
    } else {
        for (index, entry) in this.browse_entries.iter().enumerate() {
            let label = if entry.name.is_empty() {
                entry.path.clone()
            } else {
                entry.name.clone()
            };
            let label_text = label.clone();
            let row = div()
                .id(("browse-entry", index))
                .w_full()
                .px_2()
                .h(px(28.))
                .flex()
                .items_center()
                .gap_2()
                .child(icon(if entry.is_dir { "folder" } else { "file" }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(crate::text::clip(("browse-name", index), label.clone()).text_sm()),
                );
            if entry.is_dir {
                let target = this.browse_dir(entry);
                let row = tip_on(row, format!("Open the folder “{label_text}”"), None);
                list = list.child(
                    row.cursor_pointer()
                        .hover(move |style| style.bg(theme.hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.load_folder(target.clone(), cx);
                        })),
                );
            } else {
                list = list.child(
                    row.text_color(theme.muted).child(
                        div()
                            .flex_none()
                            .text_xs()
                            .child(human_bytes(entry.size.max(0) as u64)),
                    ),
                );
            }
        }
    }

    let body = column()
        .child(note(
            "Click a folder to open it, then press “Choose this folder”. You can also type a path, or cloud-account:folder.",
        ))
        .child(bar)
        .child(list);
    let cancel = cancel_button("browse-cancel", cx);
    let apply = tip_button(
        "browse-apply",
        "check",
        "Choose this folder",
        "Use the folder shown above as the location",
        None,
        Look::Primary,
        cx.listener(|this, _, _, cx| this.apply_browse(cx)),
    );
    attach_actions(
        dialog(
            "browse",
            "Choose a folder",
            "Open a folder, then choose it.",
            cx,
        )
        .child(body),
        vec![cancel, apply],
    )
}

// MARK: - Passwords

fn change_password(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let mut body = column()
        .child(section_label("Current password"))
        .child(text_field("pw-old", &this.old_password_input))
        .child(section_label("New password"))
        .child(text_field("pw-new", &this.new_password_input));
    if this.auth_busy {
        body = body.child(working_row("pw-change-busy", "Changing password…"));
    }
    if let Some(error) = this.password_error.clone() {
        body = body.child(error_line(error));
    }
    let cancel = cancel_button("pw-change-cancel", cx);
    let submit = tip_button(
        "pw-change-submit",
        "check",
        "Change password",
        "Replace the current password with the new one",
        None,
        Look::Primary,
        cx.listener(|this, _, _, cx| this.submit_change_password(cx)),
    );
    attach_actions(
        dialog(
            "change-password",
            "Change password",
            "Enter the current password, then a new one.",
            cx,
        )
        .child(body),
        vec![cancel, submit],
    )
}

fn remove_password(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let mut body = column()
        .child(note(
            "GN Drive will stop asking for a password when it starts, so anyone using this computer can open it. You can set a new password later in Settings.",
        ))
        .child(section_label("Current password"))
        .child(text_field("pw-remove", &this.remove_password_input));
    if this.auth_busy {
        body = body.child(working_row("pw-remove-busy", "Removing password…"));
    }
    if let Some(error) = this.password_error.clone() {
        body = body.child(error_line(error));
    }
    let cancel = cancel_button("pw-remove-cancel", cx);
    let submit = tip_button(
        "pw-remove-submit",
        "trash",
        "Remove password",
        "Stop asking for a password when GN Drive starts",
        None,
        Look::Danger,
        cx.listener(|this, _, _, cx| this.submit_remove_password(cx)),
    );
    attach_actions(
        dialog(
            "remove-password",
            "Remove password",
            "Anyone using this Mac can open it.",
            cx,
        )
        .child(body),
        vec![cancel, submit],
    )
}
