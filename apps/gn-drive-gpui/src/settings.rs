//! Settings: General, Service, History, Logs, and About. Each tab shows what
//! the engine last reported, a loading state until the first answer, and
//! `settings_error` when a read failed.

use gpui::{AnyElement, Context, Rgba, SharedString, div, prelude::*, px};
use ns_ui::{
    Ask, IconTone, MONO_FONT, Tone, chip, section_label, skeleton_list, tab_strip_equal, theme,
    working_row,
};

use crate::format::{human_bytes, human_duration, status_color, status_label};
use crate::states::{self, Region};
use crate::tip::{Look, tip_button, tip_button_off, tip_icon, tip_switch};
use crate::{AskAction, GnDriveWindow, Overlay, Panel, SettingsTab};

/// Labelled tabs. `keep_labels` stays on, so a narrow page never hides the words.
/// Each tab has one tooltip.
pub(crate) fn tabs(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let entity = cx.entity();
    tab_strip_equal(
        "settings-tabs",
        [
            ("general", "cog", "General"),
            ("service", "server", "Service"),
            ("history", "history", "History"),
            ("logs", "terminal", "Logs"),
            ("about", "globe", "About"),
        ],
        tab_key(this.settings_tab),
        move |value, _, cx| {
            let tab = match value.as_ref() {
                "service" => SettingsTab::Service,
                "history" => SettingsTab::History,
                "logs" => SettingsTab::Logs,
                "about" => SettingsTab::About,
                _ => SettingsTab::General,
            };
            entity.update(cx, |this, cx| this.pick_settings_tab(tab, cx));
        },
    )
    .tooltips([
        "Startup, notifications, password, and where GN Drive keeps its files",
        "The background service that runs scheduled flows when this window is closed",
        "Every run that has finished, with its size and time",
        "Health checks and the technical log, useful when something goes wrong",
        "Version and updates",
    ])
    .into_any_element()
}

/// The open tab. The page shell supplies the width and the scroll.
pub(crate) fn body(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    if this.simulation == states::Simulation::Loading {
        return states::loading_block("settings-loading", 6);
    }
    match this.settings_tab {
        SettingsTab::General => general(this, cx),
        SettingsTab::Service => service(this, cx),
        SettingsTab::History => history(this, cx),
        SettingsTab::Logs => logs(this, cx),
        SettingsTab::About => about(this, cx),
    }
}

fn tab_key(tab: SettingsTab) -> &'static str {
    match tab {
        SettingsTab::General => "general",
        SettingsTab::Service => "service",
        SettingsTab::History => "history",
        SettingsTab::Logs => "logs",
        SettingsTab::About => "about",
    }
}

fn column() -> gpui::Div {
    div().w_full().min_w_0().flex().flex_col().gap_2()
}

/// A muted sentence that wraps inside the settings column.
fn hint(text: impl Into<SharedString>) -> gpui::Div {
    crate::text::wrap_muted(text).px_2().py_1()
}

fn value_row(label: &str, value: impl Into<SharedString>) -> AnyElement {
    let theme = theme();
    let value = value.into();
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_0p5()
        .px_2()
        .py_1()
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .text_xs()
                .text_color(theme.muted)
                .child(label.to_string()),
        )
        .child(crate::text::wrap(value).text_xs())
        .into_any_element()
}

/// A backend id such as `database` or `rclone_config`, as a short label.
fn sentence_label(raw: &str) -> String {
    let text = raw.trim().replace('_', " ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::new();
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
            out
        }
        None => "Check".into(),
    }
}

fn status_words(status: &str) -> String {
    match status {
        "" | "unknown" => "Unknown".into(),
        "ok" | "pass" | "passed" | "healthy" => "Passed".into(),
        "info" => "Info".into(),
        "warn" | "warning" => "Needs attention".into(),
        "skipped" => "Skipped".into(),
        "running" => "Running".into(),
        "cancelled" | "canceled" => "Stopped".into(),
        "completed" => "Completed".into(),
        "idle" => "Idle".into(),
        "failed" | "error" | "fail" => "Failed".into(),
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

fn error_card(message: &str, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let (sentence, code) = states::error_parts(message);
    states::error_block(
        "settings-error",
        sentence,
        states::details_tip(&code, message),
        tip_button(
            "settings-retry",
            "refresh",
            "Retry",
            "Load this information again",
            None,
            Look::Primary,
            cx.listener(|this, _, _, cx| {
                this.reload(cx);
                this.load_settings_tab(cx);
            }),
        ),
    )
}

fn card() -> gpui::Div {
    let theme = theme();
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .rounded_md()
        .bg(theme.editor)
        .border_1()
        .border_color(theme.border)
}

fn tone_for_status(status: &str) -> Tone {
    match status {
        "ok" | "pass" | "passed" | "healthy" | "completed" => Tone::Ok,
        "warn" | "warning" | "skipped" | "cancelled" => Tone::Warn,
        "running" => Tone::Accent,
        "" | "idle" | "unknown" => Tone::Neutral,
        _ => Tone::Err,
    }
}

// MARK: - General

fn switch_row(
    id: &'static str,
    title: &'static str,
    hint: &'static str,
    key: &'static str,
    value: bool,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let entity = cx.entity();
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap_3()
        .px_2()
        .py_1()
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .child(div().text_sm().child(title))
                .child(crate::text::wrap_muted(hint)),
        )
        .child(tip_switch(id, value, false, hint, move |on, _, cx| {
            entity.update(cx, |this, cx| this.set_setting(key, on, cx));
        }))
        .into_any_element()
}

fn path_row(id: &'static str, label: &'static str, path: &str) -> AnyElement {
    let theme = theme();
    let target = path.to_string();
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .child(div().text_xs().text_color(theme.muted).child(label))
                .child(
                    crate::text::clip_tip(
                        SharedString::from(format!("path-text-{id}")),
                        if path.is_empty() {
                            "—".to_string()
                        } else {
                            ns_ui::display_path(path)
                        },
                        if path.is_empty() {
                            "No path yet".to_string()
                        } else {
                            path.to_string()
                        },
                    )
                    .font_family(MONO_FONT)
                    .text_xs(),
                ),
        )
        .when(!path.is_empty(), |row| {
            row.child(tip_icon(
                id,
                "reveal",
                format!("Show “{label}” in Finder"),
                None,
                IconTone::Quiet,
                move |_, _, _| ns_ui::reveal(&target),
            ))
        })
        .into_any_element()
}

fn general(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    if this.snapshot.is_none() {
        return match &this.load_error {
            Some(error) => error_card(error, cx),
            None => skeleton_list("settings-general-loading", 6).into_any_element(),
        };
    }
    let settings = this.app_settings();
    let auth = this.auth();
    let paths = this.engine_paths();

    let mut body = column()
        .child(section_label("Preferences"))
        .child(switch_row(
            "set-notifications",
            "Notifications",
            "Tell me when a run finishes or fails.",
            "notificationsEnabled",
            settings.notifications_enabled,
            cx,
        ))
        .child(switch_row(
            "set-debug",
            "Debug mode",
            "Write extra-detailed technical logs. Turn on only if support asks you to.",
            "debugMode",
            settings.debug_mode,
            cx,
        ))
        .child(switch_row(
            "set-tray",
            "Minimize to tray",
            "Closing the window hides GN Drive so syncs keep running. Quit still exits.",
            "minimizeToTray",
            settings.minimize_to_tray,
            cx,
        ))
        .child(switch_row(
            "set-login",
            "Start at login",
            "Open GN Drive when you sign in. Turning it off removes the background service.",
            "startAtLogin",
            settings.start_at_login,
            cx,
        ))
        .child(section_label("Security"));

    if auth.enabled {
        body = body
            .child(hint("A password protects GN Drive."))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .px_2()
                    .child(tip_button(
                        "set-lock",
                        "eye-off",
                        "Lock now",
                        "Lock GN Drive now; you will need the password to open it again",
                        Some("⌘⇧L"),
                        Look::Quiet,
                        cx.listener(|this, _, _, cx| {
                            this.ask = Some((
                                Ask {
                                    title: "Lock GN Drive?".into(),
                                    message: "You will need the password to open it again. \
                                              Running flows keep going."
                                        .into(),
                                    confirm: "Lock".into(),
                                    confirm_icon: "eye-off",
                                    destructive: false,
                                },
                                AskAction::Lock,
                            ));
                            cx.notify();
                        }),
                    ))
                    .child(tip_button(
                        "set-change-password",
                        "user",
                        "Change password",
                        "Choose a new master password",
                        None,
                        Look::Quiet,
                        cx.listener(|this, _, _, cx| {
                            for input in [&this.old_password_input, &this.new_password_input] {
                                input.update(cx, |input, cx| input.set_text("", cx));
                            }
                            this.password_error = None;
                            this.overlay = Overlay::ChangePassword;
                            cx.notify();
                        }),
                    ))
                    .child(tip_button(
                        "set-remove-password",
                        "trash",
                        "Remove password",
                        "Stop asking for a password when GN Drive opens. You will confirm your current password first",
                        None,
                        Look::Danger,
                        cx.listener(|this, _, _, cx| {
                            this.ask = Some((
                                Ask {
                                    title: "Remove password?".into(),
                                    message: "GN Drive opens without a password afterwards. \
                                              You will confirm the current password next."
                                        .into(),
                                    confirm: "Continue".into(),
                                    confirm_icon: "trash",
                                    destructive: true,
                                },
                                AskAction::RemovePassword,
                            ));
                            cx.notify();
                        }),
                    )),
            );
    } else {
        body = body.child(hint("No password is set. GN Drive opens without one."));
    }

    body.child(section_label("Files"))
        .child(path_row(
            "reveal-config",
            "Settings folder",
            &paths.config_dir,
        ))
        .child(path_row(
            "reveal-db",
            "Run history database",
            &paths.db_path,
        ))
        .child(path_row(
            "reveal-rclone",
            "Connection settings file",
            &paths.rclone_config_path,
        ))
        .child(path_row("reveal-logs", "Log folder", &paths.log_dir))
        .child(path_row(
            "reveal-working",
            "Working folder",
            &paths.working_dir,
        ))
        .into_any_element()
}

// MARK: - Service

fn service(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let theme = theme();
    let mut body = column();
    if let Some(error) = &this.settings_error {
        body = body.child(error_card(error, cx));
    }
    body = body.child(section_label("Background service"));
    body = body.child(hint(
        "The background service runs your scheduled flows even when this window is closed.",
    ));

    match &this.service {
        None => {
            if this.settings_error.is_none() {
                body = body.child(working_row("service-loading", "Reading service status"));
                body = body.child(skeleton_list("service-skeleton", 3));
            }
        }
        Some(status) => {
            let installed = status.installed;
            let running = status.running;
            let mut chips = div().flex().flex_wrap().gap_1().px_2();
            chips = chips.child(chip(
                if installed {
                    "Installed"
                } else {
                    "Not installed"
                },
                if installed { Tone::Ok } else { Tone::Neutral },
            ));
            chips = chips.child(chip(
                if running { "Running" } else { "Stopped" },
                if running { Tone::Ok } else { Tone::Warn },
            ));
            if !status.mode.is_empty() {
                chips = chips.child(chip(sentence_label(&status.mode), Tone::Neutral));
            }
            if !status.scope.is_empty() {
                chips = chips.child(chip(sentence_label(&status.scope), Tone::Neutral));
            }
            body = body.child(chips);
            if status.pid > 0 {
                body = body.child(value_row("PID", status.pid.to_string()));
            }
            if !status.label.is_empty() {
                body = body.child(value_row("Label", status.label.clone()));
            }
            if !status.plist_path.is_empty() {
                body = body.child(path_row("reveal-plist", "Service file", &status.plist_path));
            }

            let mut actions = div().flex().flex_wrap().gap_2().px_2();
            if !installed {
                actions = actions.child(tip_button(
                    "service-install",
                    "download",
                    "Install",
                    "Install the background service so scheduled flows run even when this window is closed",
                    None,
                    Look::Primary,
                    cx.listener(|this, _, _, cx| {
                        this.service_action("service.install", "Service installed", cx)
                    }),
                ));
            } else {
                if running {
                    actions = actions
                        .child(tip_button(
                            "service-stop",
                            "stop",
                            "Stop",
                            "Stop the background service. Scheduled flows pause until you start it",
                            None,
                            Look::Quiet,
                            cx.listener(|this, _, _, cx| {
                                this.service_action("service.stop", "Service stopped", cx)
                            }),
                        ))
                        .child(tip_button(
                            "service-restart",
                            "restart",
                            "Restart",
                            "Stop and start the background service again",
                            None,
                            Look::Quiet,
                            cx.listener(|this, _, _, cx| {
                                this.service_action("service.restart", "Service restarted", cx)
                            }),
                        ));
                } else {
                    actions = actions.child(tip_button(
                        "service-start",
                        "play",
                        "Start",
                        "Start the background service so scheduled flows run",
                        None,
                        Look::Primary,
                        cx.listener(|this, _, _, cx| {
                            this.service_action("service.start", "Service started", cx)
                        }),
                    ));
                }
                actions = actions.child(tip_button(
                    "service-uninstall",
                    "trash",
                    "Uninstall",
                    "Remove the background service. Scheduled flows stop running when GN Drive is closed",
                    None,
                    Look::Danger,
                    cx.listener(|this, _, _, cx| {
                        this.ask = Some((
                            Ask {
                                title: "Uninstall service?".into(),
                                message: "Scheduled flows stop running in the background \
                                          until the service is installed again."
                                    .into(),
                                confirm: "Uninstall".into(),
                                confirm_icon: "trash",
                                destructive: true,
                            },
                            AskAction::UninstallService,
                        ));
                        cx.notify();
                    }),
                ));
            }
            body = body.child(actions);
        }
    }

    body = body.child(section_label("Health"));
    match &this.health {
        None => {
            if this.settings_error.is_none() {
                body = body.child(skeleton_list("health-skeleton", 3));
            }
        }
        Some(health) => {
            let (label, tone) = if !health.present {
                ("No heartbeat", Tone::Neutral)
            } else if health.stale {
                ("Stale", Tone::Warn)
            } else {
                ("Healthy", Tone::Ok)
            };
            let mut card = card().child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(chip(label, tone))
                    .when(!health.service_name.is_empty(), |row| {
                        row.child(
                            div().flex_1().min_w(px(0.)).child(
                                crate::text::clip("health-service", health.service_name.clone())
                                    .text_xs()
                                    .text_color(theme.muted),
                            ),
                        )
                    }),
            );
            if !health.last_error.is_empty() {
                let explained = crate::format::explain("", &health.last_error);
                let full = if explained.text == health.last_error {
                    explained.text.clone()
                } else {
                    format!("{}\nDetails: {}", explained.text, health.last_error)
                };
                card = card.child(crate::text::wrap_err_tip(
                    "health-error",
                    explained.text,
                    full,
                ));
            }
            if health.present {
                if health.pid > 0 {
                    card = card.child(value_row("PID", health.pid.to_string()));
                }
                if !health.mode.is_empty() {
                    card = card.child(value_row("Mode", sentence_label(&health.mode)));
                }
                if health.uptime_secs > 0 {
                    card = card.child(value_row(
                        "Uptime",
                        human_duration(health.uptime_secs as f64),
                    ));
                }
                if let Some(at) = &health.last_heartbeat {
                    card = card.child(value_row("Last heartbeat", at.clone()));
                }
                if let Some(at) = &health.last_sync_at {
                    card = card.child(value_row("Last sync", at.clone()));
                }
                if let Some(at) = &health.next_schedule_at {
                    card = card.child(value_row("Next scheduled run", at.clone()));
                }
                if !health.active_tasks.is_empty() {
                    card = card.child(value_row("Active tasks", health.active_tasks.join(", ")));
                }
            } else {
                card = card.child(hint("The service has not reported in."));
            }
            body = body.child(card);
        }
    }
    body.into_any_element()
}

// MARK: - History

fn history_region(this: &GnDriveWindow) -> Region {
    if this.simulation == states::Simulation::EmptyHistory {
        return Region::Empty;
    }
    let error = this.history_error.is_some();
    states::feed_region(
        this.history_in_flight(),
        error,
        this.history_loaded,
        this.history.len(),
    )
}

fn history_empty(cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let (title, detail) = states::history_empty_copy();
    states::empty_block(
        title,
        detail,
        tip_button(
            "history-empty-back",
            "arrow-left",
            "Show my flows",
            "Go back to your flows so you can run one",
            Some("Esc"),
            Look::Primary,
            cx.listener(|this, _, _, cx| {
                this.panel = Panel::Closed;
                cx.notify();
            }),
        ),
    )
}

fn history_error_block(this: &GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let sentence = this
        .history_error
        .clone()
        .unwrap_or_else(|| "Could not load run history.".into());
    let code = this.history_error_code.clone().unwrap_or_default();
    states::error_block(
        "history-error",
        sentence,
        states::details_tip(&code, &this.history_error.clone().unwrap_or_default()),
        tip_button(
            "history-retry",
            "refresh",
            "Retry",
            "Load run history again",
            None,
            Look::Primary,
            cx.listener(|this, _, _, cx| {
                this.history_error = None;
                this.history_error_code = None;
                this.load_settings_tab(cx);
            }),
        ),
    )
}

fn history(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    match history_region(this) {
        Region::Loading => return states::loading_block("history-loading", 6),
        Region::Error => return history_error_block(this, cx),
        Region::Empty => return history_empty(cx),
        Region::Ready => {}
    }

    let mut list = column();
    if this.history_error.is_some() {
        list = list.child(history_error_block(this, cx));
    }
    list = list.child(div().flex().justify_end().child(tip_button(
        "history-clear",
        "trash",
        "Clear history",
        "Delete the list of past runs. Your flows and cloud accounts are kept",
        None,
        Look::OutlineDanger,
        cx.listener(|this, _, _, cx| {
            this.ask = Some((
                Ask {
                    title: "Clear history?".into(),
                    message: "Every recorded run is removed from this list. Your flows and cloud accounts are kept. This cannot be undone.".into(),
                    confirm: "Clear history".into(),
                    confirm_icon: "trash",
                    destructive: true,
                },
                AskAction::ClearHistory,
            ));
            cx.notify();
        }),
    )));
    // The history list reads the snapshot cache's (id, name) pairs, already
    // in rail order.
    let known = this.flow_cache.history_names.clone();
    let flows: Vec<(&str, &str)> = known
        .iter()
        .map(|(id, name)| (id.as_str(), name.as_str()))
        .collect();
    for entry in &this.history {
        let title = crate::format::history_title(&entry.profile_name, &entry.id, &flows);
        let mut meta = Vec::new();
        if !entry.action.is_empty() {
            meta.push(crate::format::action_label(&entry.action).to_string());
        }
        if entry.duration > 0.0 {
            meta.push(human_duration(entry.duration));
        }
        if entry.bytes > 0 {
            meta.push(human_bytes(entry.bytes));
        }
        if entry.files > 0 {
            meta.push(format!("{} files", entry.files));
        }
        let mut row = card()
            .child(
                div()
                    .w_full()
                    .min_w(px(0.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w(px(0.)).child(crate::text::clip(
                        SharedString::from(format!("history-title-{}", entry.id)),
                        title,
                    )))
                    .child(chip(
                        if entry.state.is_empty() {
                            "Idle"
                        } else {
                            status_label(&entry.state)
                        },
                        tone_for_status(&entry.state),
                    )),
            )
            .child(crate::text::wrap_muted(crate::format::friendly_time(
                &entry.started_at,
            )));
        if !meta.is_empty() {
            row = row.child(
                crate::text::wrap(meta.join(" · "))
                    .text_xs()
                    .text_color(status_color(&entry.state)),
            );
        }
        if entry.errors > 0 || !entry.error_message.is_empty() {
            let (shown, full) = if entry.error_message.is_empty() {
                let text = format!("{} errors.", entry.errors);
                (text.clone(), text)
            } else {
                let explained = crate::format::explain("", &entry.error_message);
                let full = if explained.text == entry.error_message {
                    explained.text.clone()
                } else {
                    format!("{}\nDetails: {}", explained.text, entry.error_message)
                };
                (explained.text, full)
            };
            row = row.child(crate::text::wrap_err_tip(
                SharedString::from(format!("history-error-{}", entry.id)),
                shown,
                full,
            ));
        }
        list = list.child(row);
    }
    list.into_any_element()
}

// MARK: - Logs

fn level_color(level: &str) -> Rgba {
    let theme = theme();
    match level.to_ascii_lowercase().as_str() {
        "error" | "fatal" | "panic" => theme.err,
        "warn" | "warning" => theme.warn,
        "debug" | "trace" => theme.muted,
        _ => theme.text,
    }
}

fn logs(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let theme = theme();
    let mut body = column();
    if let Some(error) = &this.settings_error {
        body = body.child(error_card(error, cx));
    }

    body = body.child(section_label("Health checks"));
    match &this.doctor {
        None => {
            if this.settings_error.is_none() {
                body = body.child(working_row("doctor-loading", "Running checks"));
                body = body.child(skeleton_list("doctor-skeleton", 4));
            }
        }
        Some(report) => {
            body = body.child(div().px_2().child(chip(
                if report.ok {
                    "All checks passed"
                } else {
                    "Needs attention"
                },
                if report.ok { Tone::Ok } else { Tone::Warn },
            )));
            if report.checks.is_empty() {
                body = body.child(hint("The engine returned no checks."));
            }
            if !report.data_files.is_empty() {
                body = body.child(section_label("Data files"));
                body = body.child(hint(
                    "These files live in the settings folder. They hold your password setup, history, and connection settings.",
                ));
                for file in &report.data_files {
                    body = body.child(div().px_2().child(crate::text::clip(
                        SharedString::from(format!("data-file-{file}")),
                        file.clone(),
                    )));
                }
            }
            for check in &report.checks {
                let raw_label = if check.label.is_empty() {
                    check.id.clone()
                } else {
                    check.label.clone()
                };
                let label = sentence_label(&raw_label);
                body = body.child(
                    card()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div().flex_1().min_w(px(0.)).child(
                                        crate::text::clip(
                                            SharedString::from(format!("doctor-{}", check.id)),
                                            label,
                                        )
                                        .text_sm(),
                                    ),
                                )
                                .child(chip(
                                    status_words(&check.status),
                                    tone_for_status(&check.status),
                                )),
                        )
                        .when(!check.detail.is_empty(), |row| {
                            row.child(crate::text::wrap_muted(check.detail.clone()))
                        }),
                );
            }
        }
    }

    let copy_text = this.logs.as_ref().map(|page| {
        page.lines
            .iter()
            .map(|line| format!("{} {} {}", line.time, line.level, line.msg))
            .collect::<Vec<_>>()
            .join("\n")
    });
    body = body.child(tip_button(
        "logs-filter",
        "alert",
        if this.log_errors_only {
            "Showing problems"
        } else {
            "Showing all lines"
        },
        "Switch between every log line and warnings plus errors",
        None,
        Look::Quiet,
        cx.listener(|this, _, _, cx| {
            this.log_errors_only = !this.log_errors_only;
            this.refresh_logs(cx);
        }),
    ));
    let mut header = div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap_1()
        .child(div().flex_1().min_w_0().child(section_label("Log")));
    header = header.child(tip_icon(
        "logs-refresh",
        "refresh",
        "Load the newest log lines",
        None,
        IconTone::Quiet,
        cx.listener(|this, _, _, cx| {
            this.refresh_logs(cx);
            cx.notify();
        }),
    ));
    if let Some(text) = copy_text.clone().filter(|text| !text.is_empty()) {
        let entity = cx.entity();
        header = header.child(tip_icon(
            "logs-copy",
            "copy",
            "Copy the log text to the clipboard so you can paste it into a message",
            None,
            IconTone::Quiet,
            move |_, _, cx| {
                let copied = ns_ui::copy_text(&text);
                entity.update(cx, |this, cx| {
                    this.notice(
                        if copied {
                            ely_gpui_component::primitives::Severity::Success
                        } else {
                            ely_gpui_component::primitives::Severity::Warning
                        },
                        if copied {
                            "Log copied"
                        } else {
                            "Could not copy the log"
                        },
                        cx,
                    );
                });
            },
        ));
    }
    body = body.child(header);

    let log_count = this.logs.as_ref().map(|page| page.lines.len()).unwrap_or(0);
    let log_region = if this.simulation == states::Simulation::EmptyLogs {
        Region::Empty
    } else {
        states::feed_region(
            this.logs_in_flight(),
            this.logs_error.is_some(),
            this.logs.is_some(),
            log_count,
        )
    };
    match log_region {
        Region::Loading => {
            body = body.child(states::loading_block("logs-skeleton", 6));
        }
        Region::Error => {
            let sentence = this
                .logs_error
                .clone()
                .unwrap_or_else(|| "Could not load the log.".into());
            let code = this.logs_error_code.clone().unwrap_or_default();
            body = body.child(states::error_block(
                "logs-error",
                sentence,
                states::details_tip(&code, &this.logs_error.clone().unwrap_or_default()),
                tip_button(
                    "logs-retry",
                    "refresh",
                    "Retry",
                    "Load the log again",
                    None,
                    Look::Primary,
                    cx.listener(|this, _, _, cx| {
                        this.logs_error = None;
                        this.logs_error_code = None;
                        this.refresh_logs(cx);
                    }),
                ),
            ));
        }
        Region::Empty => {
            let (title, detail) = states::logs_empty_copy();
            body = body.child(states::empty_block(
                title,
                detail,
                tip_button(
                    "logs-empty-refresh",
                    "refresh",
                    "Refresh",
                    "Check again for new log lines",
                    None,
                    Look::Primary,
                    cx.listener(|this, _, _, cx| {
                        this.refresh_logs(cx);
                        cx.notify();
                    }),
                ),
            ));
        }
        Region::Ready => {
            let Some(page) = this.logs.clone() else {
                return body.into_any_element();
            };
            if page.dropped > 0 {
                body = body.child(hint(format!("{} older lines were dropped.", page.dropped)));
            }
            let mut lines = div()
                .flex()
                .flex_col()
                .p_2()
                .rounded_md()
                .bg(theme.raised)
                .border_1()
                .border_color(theme.border)
                .font_family(MONO_FONT)
                .text_xs();
            for (index, line) in page.lines.iter().enumerate() {
                let level = line.level.to_ascii_lowercase();
                let time = ns_ui::strip_ansi(&line.time);
                if matches!(level.as_str(), "error" | "fatal" | "panic") {
                    let explained = crate::format::explain("", &line.msg);
                    let shown = format!("{time} {} {}", line.level, explained.text);
                    let full = if explained.text == line.msg {
                        shown.clone()
                    } else {
                        format!("{shown}\nDetails: {}", line.msg)
                    };
                    lines = lines.child(
                        crate::text::wrap_err_tip(
                            SharedString::from(format!("log-line-{index}")),
                            shown,
                            full,
                        )
                        .font_family(MONO_FONT)
                        .text_xs(),
                    );
                } else {
                    lines = lines.child(
                        crate::text::wrap(format!("{time} {:<5} {}", line.level, line.msg))
                            .font_family(MONO_FONT)
                            .text_xs()
                            .text_color(level_color(&line.level)),
                    );
                }
            }
            body = body.child(lines);
        }
    }
    body.into_any_element()
}

// MARK: - About

fn about(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let mut body = column()
        .child(section_label("GN Drive"))
        .child(value_row("Version", this.version()))
        .child(value_row("rclone", this.rclone_version()))
        .child(section_label("Updates"));

    let available = this
        .update
        .as_ref()
        .map(|update| update.update_available)
        .unwrap_or(false);
    let mut actions = div().flex().flex_wrap().gap_2().px_2();
    if this.update_busy {
        actions = actions.child(tip_button_off(
            "update-check",
            "refresh",
            "Checking…",
            "Looking online for a newer version of GN Drive",
            None,
            Look::Quiet,
        ));
    } else {
        actions = actions.child(tip_button(
            "update-check",
            "refresh",
            "Check for updates",
            "Look online for a newer version of GN Drive",
            None,
            Look::Quiet,
            cx.listener(|this, _, _, cx| this.check_update(cx)),
        ));
    }
    if available {
        actions = actions.child(tip_button(
            "update-apply",
            "download",
            "Apply update",
            "Download the new version and install it over this one",
            None,
            Look::Primary,
            cx.listener(|this, _, _, cx| {
                this.ask = Some((
                    Ask {
                        title: "Apply update?".into(),
                        message: "GN Drive downloads the new version and replaces itself. \
                                  Restart it afterwards to finish."
                            .into(),
                        confirm: "Apply update".into(),
                        confirm_icon: "download",
                        destructive: false,
                    },
                    AskAction::ApplyUpdate,
                ));
                cx.notify();
            }),
        ));
    }
    body = body.child(actions);

    if this.update_text.is_empty() {
        body = body.child(hint("Check to see whether a newer release exists."));
    } else {
        let mut card = card().child(crate::text::wrap(this.update_text.clone()).text_sm());
        if let Some(update) = &this.update {
            card = card.child(
                div()
                    .flex()
                    .gap_1()
                    .child(chip(
                        if update.update_available {
                            "Update available"
                        } else {
                            "Up to date"
                        },
                        if update.update_available {
                            Tone::Accent
                        } else {
                            Tone::Ok
                        },
                    ))
                    .when(!update.latest.is_empty(), |row| {
                        row.child(chip(format!("latest {}", update.latest), Tone::Neutral))
                    }),
            );
            if !update.checked_at.is_empty() {
                card = card.child(crate::text::wrap_muted(format!(
                    "Checked {}",
                    update.checked_at
                )));
            }
        }
        if !this.update_url.is_empty() {
            let url = this.update_url.clone();
            let copy = this.update_url.clone();
            card = card.child(
                div()
                    .flex()
                    .gap_2()
                    .child(tip_button(
                        "update-open",
                        "globe",
                        "Open release",
                        "Open the release page in your web browser",
                        None,
                        Look::Quiet,
                        move |_, _, _| ns_ui::open_url(&url),
                    ))
                    .child(tip_button(
                        "update-copy",
                        "copy",
                        "Copy link",
                        "Copy the download link for this release to the clipboard",
                        None,
                        Look::Quiet,
                        move |_, _, _| {
                            ns_ui::copy_text(&copy);
                        },
                    )),
            );
        }
        body = body.child(card);
    }
    body.into_any_element()
}
