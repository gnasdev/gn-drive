//! The workspace: the flow rail, the canvas, and the inspector.
//!
//! The canvas is drawn from `graph::FlowGraph`, which itself is built from the
//! flow's operations. Moving a node or panning writes only the canvas; adding,
//! renaming, or connecting writes operations. Every edit stays a draft until
//! Save or Run. Save lives only in the inspector footer. The toolbar keeps
//! Add folder and Run. Stop takes Run's place while a flow is running.
//! A running flow's inspector is read-only.

/// Help for the canvas, shown from the "?" control beside zoom.
const CANVAS_HELP: &str =
    "Drag a node to move it · drag the ring to connect · scroll to pan · ⌘-scroll to zoom";

use std::collections::HashMap;
use std::rc::Rc;

use ely_gpui_component::primitives::{Icon, IconName, Tooltip};
use ely_gpui_component::theme::IconSize;
use gpui::{
    AnyElement, Bounds, Context, Corners, Hsla, IntoElement, MouseButton, PinchEvent, Rgba, Role,
    ScrollWheelEvent, SharedString, Window, div, point, prelude::*, px, size,
};
use ns_ui::{IconTone, SelectOption, Tone, chip, theme};

use crate::engine::{FileTransfer, Flow, SyncProgress};
use crate::format::{
    FileDot, StatusTone, action_glyph, action_help, action_label, chip_tip, connection_summary,
    cron_error, cron_label, flow_status_tone, flow_summary, fraction, human_bytes, human_duration,
    middle_ellipsis, percent_text, progress_speed, ranked_transfers, status_color, status_label,
};
use crate::graph::{
    self, CanvasLayer, EDGE_POPOVER_EST_H, GraphEdge, GraphNode, NODE_W, canvas_child_order,
    clamp_edge_popover,
};
use crate::inspector::{self, Section};
use crate::states::{self, Simulation};
use crate::sync_config::{FieldDef, FieldKind, SyncConfig};
use crate::tip::{
    Look, tip_button, tip_button_off, tip_icon, tip_on, tip_segmented, tip_select, tip_switch,
};
use crate::{BrowseTarget, GnDriveWindow, Selection};

fn prose(text: impl Into<SharedString>) -> gpui::Div {
    crate::text::wrap_muted(text)
}

fn file_status(status: &str, progress: f64) -> String {
    match status {
        "failed" => "Failed".into(),
        "transferring" | "checking" => percent_text(progress),
        "pending" => "Waiting".into(),
        "completed" | "checked" => "Done".into(),
        "cancelled" | "canceled" => "Stopped".into(),
        "running" => "Running".into(),
        "" | "idle" => "Idle".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => {
                    let mut out = String::new();
                    out.extend(first.to_uppercase());
                    out.push_str(chars.as_str());
                    out
                }
                None => "Idle".into(),
            }
        }
    }
}

fn friendly_error(raw: &str) -> (String, String) {
    let explained = crate::format::explain("", raw);
    let full = if raw.is_empty() || explained.text == raw {
        explained.text.clone()
    } else {
        format!("{}\nDetails: {raw}", explained.text)
    };
    (explained.text, full)
}

fn folder_button(
    id: impl Into<gpui::ElementId>,
    tip: &'static str,
    running: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    if running {
        tip_button_off(
            id,
            "folder",
            "Browse…",
            "Stop the flow before choosing a different folder.",
            None,
            Look::Quiet,
        )
    } else {
        tip_button(id, "folder", "Browse…", tip, None, Look::Quiet, on_click)
    }
}

// MARK: - Sidebar

/// Preferred flow-list width. Compact windows ignore it. Same number as
/// [`crate::prefs::DEFAULT_SIDEBAR_WIDTH`].
pub(crate) const SIDEBAR_WIDTH: f32 = crate::prefs::DEFAULT_SIDEBAR_WIDTH;
/// The divider will not go narrower than this.
pub(crate) const SIDEBAR_MIN: f32 = 200.;
const SIDEBAR_MAX_FRACTION: f32 = 0.40;

/// Clamp a requested flow-list width.
///
/// The maximum is 40% of `window_width` and never less than [`SIDEBAR_MIN`].
/// A missing or non-finite window width keeps the preferred width reachable.
/// A non-finite request falls back to [`SIDEBAR_WIDTH`]. The result is a
/// whole number of points.
pub(crate) fn clamp_sidebar_width(requested: f32, window_width: f32) -> f32 {
    let max = if !window_width.is_finite() || window_width <= 0.0 {
        SIDEBAR_WIDTH * 2.0
    } else {
        (window_width * SIDEBAR_MAX_FRACTION).max(SIDEBAR_MIN)
    };
    let max = max.floor().max(SIDEBAR_MIN);
    let value = if requested.is_finite() {
        requested
    } else {
        SIDEBAR_WIDTH
    };
    value.round().clamp(SIDEBAR_MIN, max)
}

/// Width while the divider is dragged. Moving right (`delta_x` positive)
/// grows the flow list, because the divider is its right edge.
pub(crate) fn drag_sidebar_width(origin_width: f32, delta_x: f32, window_width: f32) -> f32 {
    let origin = if origin_width.is_finite() {
        origin_width
    } else {
        SIDEBAR_WIDTH
    };
    let delta = if delta_x.is_finite() { delta_x } else { 0.0 };
    clamp_sidebar_width(origin + delta, window_width)
}

/// Cloud accounts and Settings, in the order the sidebar and the compact
/// flow list draw them. Both windows use these rows, so a narrow window
/// still has the names.
struct SidebarNav {
    id: &'static str,
    glyph: &'static str,
    label: &'static str,
    shortcut: &'static str,
    open_tip: &'static str,
    closed_tip: &'static str,
}

const SIDEBAR_NAV: &[SidebarNav] = &[
    SidebarNav {
        id: "nav-remotes",
        glyph: "globe",
        label: "Cloud accounts",
        shortcut: "⌘⇧R",
        open_tip: "Back to your flows",
        closed_tip: "Open the cloud accounts page",
    },
    SidebarNav {
        id: "nav-settings",
        glyph: "settings",
        label: "Settings",
        shortcut: "⌘,",
        open_tip: "Back to your flows",
        closed_tip: "Open settings",
    },
];

/// Left and right arrows nudge a divider by 16 points. The sign matches a
/// mouse drag: positive is to the right.
fn resize_key_delta(event: &gpui::KeyDownEvent) -> Option<f32> {
    match ns_ui::keys::plain_key(&event.keystroke)? {
        "left" => Some(-16.),
        "right" => Some(16.),
        _ => None,
    }
}

fn end_sidebar_drag(this: &mut GnDriveWindow) {
    if this.sidebar_drag.take().is_some() {
        crate::prefs::set_sidebar_width(this.sidebar_width);
    }
}

fn end_inspector_drag(this: &mut GnDriveWindow) {
    if this.inspector_drag.take().is_some() {
        crate::prefs::set_inspector_width(this.inspector_width);
    }
}

pub(crate) fn flow_rail(
    this: &mut GnDriveWindow,
    flows: &[Flow],
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let theme = theme();
    let unsaved = draft_unsaved(this, cx);
    // The applied query, lowercased and held back by the search debounce; the
    // rail matches against the snapshot cache's pre-lowered titles.
    let shown: Vec<&Flow> =
        crate::cache::rail_matches(&this.flow_cache.titles_lower, &this.rail_query)
            .into_iter()
            .filter_map(|index| flows.get(index as usize))
            .collect();
    let preview_empty = this.simulation == Simulation::EmptyFlows;
    let list_loading = this.simulation == Simulation::Loading
        || (!preview_empty
            && this.snapshot_pending
            && flows.is_empty()
            && this.load_error.is_none());
    let mut list = div().flex().flex_col().gap_1().px_2();
    if list_loading {
        list = list.child(states::loading_block("flows-loading", 5));
    } else if preview_empty || flows.is_empty() {
        let (title, detail) = states::flows_empty_copy();
        list = list.child(states::empty_block(
            title,
            detail,
            tip_button(
                "rail-empty-new",
                "plus",
                "New flow",
                "Create a flow. A flow is a set of folders that sync together",
                Some("⌘N"),
                Look::Primary,
                cx.listener(|this, _, _, cx| this.new_flow(cx)),
            ),
        ));
    } else if shown.is_empty() {
        list = list.child(div().p_3().child(prose(
            "No flows match that search. Try another name, or press Escape to close search.",
        )));
    } else {
        for flow in shown {
            let id = flow.id.clone();
            let rail_unsaved = format!("rail-unsaved-{id}");
            let selected = this.selected_flow.as_deref() == Some(id.as_str());
            let status = this.flow_status(&id);
            let running = crate::format::running(&status);
            let ops = flow.operations.len();
            let cron = cron_label(&flow.schedule_cron);
            let subtitle = if running {
                format!("{ops} · {}", status_label(&status))
            } else if flow.schedule_enabled && !flow.schedule_cron.trim().is_empty() {
                format!("{ops} · {cron}")
            } else {
                format!("{ops} · Manual")
            };
            let dot = status_color(&status);
            let tip = format!(
                "Open {title}. {subtitle}. Right-click for Run, Stop, and Delete.",
                title = flow.title()
            );
            let menu_id = id.clone();
            let row = tip_on(
                div().id(SharedString::from(format!("flow-row-{id}"))),
                tip,
                None,
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, _, cx| {
                    this.flow_menu = Some(menu_id.clone());
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.flow_menu = None;
                this.pick_flow(id.clone(), cx);
            }))
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded_md()
            .border_1()
            .border_color(if selected {
                Hsla::from(theme.border)
            } else {
                gpui::transparent_black()
            })
            .bg(if selected {
                theme.selected
            } else {
                theme.panel
            })
            .cursor_pointer()
            .hover(|style| style.bg(theme.hover))
            .child(div().size(px(8.)).rounded_full().bg(dot).flex_shrink_0())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .min_w(px(0.))
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .text_sm()
                                    .truncate()
                                    .font_weight(if selected {
                                        gpui::FontWeight::SEMIBOLD
                                    } else {
                                        gpui::FontWeight::NORMAL
                                    })
                                    .text_color(theme.text)
                                    .child(SharedString::from(flow.title())),
                            )
                            .when(selected && unsaved, move |title| {
                                title.child(unsaved_mark(rail_unsaved))
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(theme.muted)
                            .child(SharedString::from(subtitle)),
                    ),
            )
            .when(running, |row| {
                row.child(ns_ui::spinner("flow-rail-busy").into_any_element())
            })
            .when(!running && flow.schedule_enabled, |row| {
                row.child(ns_ui::icon_tinted("history", theme.muted))
            });
            list = list.child(row);
            if this.flow_menu.as_deref() == Some(flow.id.as_str()) {
                list = list.child(flow_menu(flow, running, cx));
            }
        }
    }

    let header = div()
        .px_3()
        .py_2()
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.muted)
                        .child("FLOWS"),
                )
                .child(div().text_xs().text_color(theme.muted).child(
                    if this.simulation == Simulation::EmptyFlows {
                        "0".to_string()
                    } else {
                        flows.len().to_string()
                    },
                )),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(tip_icon(
                    "rail-find",
                    "search",
                    "Find a flow by name",
                    Some("⌘F"),
                    IconTone::Quiet,
                    cx.listener(|this, _, _, cx| {
                        this.search_open = !this.search_open;
                        this.search_job += 1;
                        // Reopening applies the field's text at once; closing
                        // shows every flow again.
                        this.rail_query = if this.search_open {
                            this.search_input.read(cx).text().trim().to_lowercase()
                        } else {
                            String::new()
                        };
                        cx.notify();
                    }),
                ))
                .child(tip_icon(
                    "rail-new-flow",
                    "plus",
                    "New flow",
                    Some("⌘N"),
                    IconTone::Quiet,
                    cx.listener(|this, _, _, cx| this.new_flow(cx)),
                )),
        );

    let account_count = this.remotes().len();
    let accounts_open = this.panel == crate::Panel::Remotes;
    let settings_open = this.panel == crate::Panel::Settings;
    let accounts = &SIDEBAR_NAV[0];
    let settings = &SIDEBAR_NAV[1];
    // Named rows, not the rail icons. Built before the wide-only divider so a
    // compact flow list still reaches Cloud accounts and Settings by name.
    let nav = div()
        .mt_auto()
        .flex_shrink_0()
        .border_t_1()
        .border_color(theme.border)
        .px_2()
        .py_2()
        .flex()
        .flex_col()
        .gap_1()
        .child(page_nav(
            PageNav {
                id: accounts.id,
                glyph: accounts.glyph,
                label: accounts.label,
                tip: if accounts_open {
                    accounts.open_tip
                } else {
                    accounts.closed_tip
                },
                shortcut: accounts.shortcut,
                badge: Some(account_count.to_string()),
                active: accounts_open,
            },
            cx.listener(|this, _, _, cx| this.set_panel(crate::Panel::Remotes, cx)),
        ))
        .child(page_nav(
            PageNav {
                id: settings.id,
                glyph: settings.glyph,
                label: settings.label,
                tip: if settings_open {
                    settings.open_tip
                } else {
                    settings.closed_tip
                },
                shortcut: settings.shortcut,
                badge: None,
                active: settings_open,
            },
            cx.listener(|this, _, _, cx| this.set_panel(crate::Panel::Settings, cx)),
        ));

    let mut column = div()
        .size_full()
        .min_h(px(0.))
        .min_w(px(0.))
        .flex()
        .flex_col()
        .child(header);
    if this.search_open {
        column = column.child(
            div()
                .px_2()
                .py_1()
                .border_b_1()
                .border_color(theme.border)
                .child(ns_ui::text_field("flow-search", &this.search_input)),
        );
    }
    #[cfg(feature = "devtools")]
    let scroll = if crate::perf::running() && this.panel == crate::Panel::Closed {
        ns_ui::sidebar_scroll_tracked(&this.perf_list_scroll, list).into_any_element()
    } else {
        ns_ui::sidebar_scroll(list).into_any_element()
    };
    #[cfg(not(feature = "devtools"))]
    let scroll = ns_ui::sidebar_scroll(list);
    let body = column.child(scroll).child(nav);
    // A compact window is the flow list itself. The named rows stay; the
    // divider does not. A drag that was in progress ends and is remembered.
    if ns_ui::is_compact() {
        end_sidebar_drag(this);
        body.into_any_element()
    } else {
        div()
            .size_full()
            .min_w(px(0.))
            .flex()
            .child(body.flex_1().min_w(px(0.)).h_full())
            .child(sidebar_divider(this, cx))
            .into_any_element()
    }
}

/// One sidebar destination: Cloud accounts or Settings.
struct PageNav {
    id: &'static str,
    glyph: &'static str,
    label: &'static str,
    tip: &'static str,
    shortcut: &'static str,
    badge: Option<String>,
    active: bool,
}

/// Icon, label, count, and shortcut. Active rows use the selected surface and
/// an accent mark. Enter and Space activate it once the row has focus.
fn page_nav(
    row: PageNav,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let PageNav {
        id,
        glyph,
        label,
        tip,
        shortcut,
        badge,
        active,
    } = row;
    let theme = theme();
    let mark = if active {
        Hsla::from(theme.accent)
    } else {
        gpui::transparent_black()
    };
    let icon_color = if active { theme.accent } else { theme.muted };
    tip_on(div().id(id), tip, Some(shortcut))
        .on_click(on_click)
        .role(Role::Button)
        .tab_index(0)
        .border_1()
        .border_color(gpui::transparent_black())
        .focus_visible(|style| style.border_color(ns_ui::theme().accent))
        .aria_selected(active)
        .w_full()
        .h(px(36.))
        .px_2()
        .flex()
        .items_center()
        .gap_2()
        .rounded_md()
        .cursor_pointer()
        .bg(if active {
            Hsla::from(theme.selected)
        } else {
            gpui::transparent_black()
        })
        .hover(|style| style.bg(if active { theme.selected } else { theme.hover }))
        .child(
            div()
                .w(px(3.))
                .h(px(16.))
                .flex_shrink_0()
                .rounded_sm()
                .bg(mark),
        )
        .child(ns_ui::icon_tinted(glyph, icon_color))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .text_sm()
                .font_weight(if active {
                    gpui::FontWeight::SEMIBOLD
                } else {
                    gpui::FontWeight::NORMAL
                })
                .text_color(theme.text)
                .child(label),
        )
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap_2()
                .children(badge.map(|text| {
                    div()
                        .min_w(px(18.))
                        .h(px(18.))
                        .px_1()
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(theme.raised)
                        .text_xs()
                        .text_color(theme.muted)
                        .child(text)
                        .into_any_element()
                }))
                .child(div().text_xs().text_color(theme.muted).child(shortcut)),
        )
        .into_any_element()
}

fn presets_include(cron: &str) -> bool {
    matches!(
        cron.trim(),
        "" | "0 * * * *" | "0 */6 * * *" | "0 0 * * *" | "0 3 * * *" | "0 9 * * 1-5" | "0 0 * * 0"
    )
}

fn runtime_log(this: &GnDriveWindow, flow_id: &str) -> AnyElement {
    let Some(flow) = this.runtime_flow(flow_id) else {
        return div().into_any_element();
    };
    let entries: Vec<_> = flow.log.iter().rev().take(12).collect();
    if entries.is_empty() {
        return div().into_any_element();
    }
    let mut list = div().flex().flex_col().gap_1();
    let mut previous = String::new();
    for (index, entry) in entries.into_iter().enumerate() {
        let text = if entry.error.is_empty() {
            if entry.label.is_empty() {
                status_label(&entry.status).to_string()
            } else {
                entry.label.clone()
            }
        } else {
            crate::format::humanize_error(&entry.error)
        };
        if text.is_empty() || text == previous {
            continue;
        }
        previous = text.clone();
        let (shown, full) = if entry.error.is_empty() {
            (text.clone(), text.clone())
        } else {
            friendly_error(&entry.error)
        };
        list = list.child(if entry.error.is_empty() {
            prose(shown).into_any_element()
        } else {
            crate::text::wrap_err_tip(
                SharedString::from(format!("runtime-log-{flow_id}-{index}")),
                shown,
                full,
            )
            .into_any_element()
        });
    }
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(quiet_heading("Recent activity"))
        .child(list)
        .into_any_element()
}

fn flow_menu(flow: &Flow, running: bool, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let theme = theme();
    let id = flow.id.clone();
    let name = flow.title();
    let mut row = div()
        .mx_2()
        .mb_1()
        .p_2()
        .rounded_md()
        .bg(theme.raised)
        .border_1()
        .border_color(theme.border)
        .flex()
        .flex_col()
        .gap_1()
        .child(prose(format!("Actions for {name}")));
    if running {
        let stop_id = id.clone();
        row = row.child(tip_button(
            format!("flow-menu-stop-{id}"),
            "stop",
            "Stop",
            format!("Stop \"{name}\" now"),
            Some("⌘."),
            Look::Outline,
            cx.listener(move |this, _, _, cx| {
                this.flow_menu = None;
                if this.selected_flow.as_deref() != Some(stop_id.as_str()) {
                    this.pick_flow(stop_id.clone(), cx);
                }
                this.stop_flow(cx);
            }),
        ));
    } else {
        let run_id = id.clone();
        row = row.child(tip_button(
            format!("flow-menu-run-{id}"),
            "play",
            "Run",
            format!("Run \"{name}\" now"),
            Some("⌘R"),
            Look::Primary,
            cx.listener(move |this, _, _, cx| {
                this.flow_menu = None;
                if this.selected_flow.as_deref() != Some(run_id.as_str()) {
                    this.pick_flow(run_id.clone(), cx);
                }
                this.run_flow(cx);
            }),
        ));
    }
    let delete_id = id.clone();
    row.child(tip_button(
        format!("flow-menu-delete-{id}"),
        "trash",
        "Delete…",
        format!("Delete flow \"{name}\". You will be asked to confirm"),
        None,
        Look::Danger,
        cx.listener(move |this, _, _, cx| {
            this.flow_menu = None;
            if this.selected_flow.as_deref() != Some(delete_id.as_str()) {
                this.pick_flow(delete_id.clone(), cx);
            }
            this.delete_selected_flow(cx);
        }),
    ))
    .into_any_element()
}

// MARK: - Body

pub(crate) fn workspace_body(
    this: &mut GnDriveWindow,
    window: &mut Window,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    if let Some(page) = crate::pages::open_page(this, window, cx) {
        return page;
    }
    let theme = theme();
    if this.simulation == Simulation::Loading {
        return div()
            .size_full()
            .p_4()
            .bg(theme.background)
            .child(states::loading_block("workspace-loading", 8))
            .into_any_element();
    }
    if this.draft.is_none() || this.simulation == Simulation::EmptyFlows {
        return empty_workspace(this, cx);
    }

    if ns_ui::is_compact() && this.inspector_drag.take().is_some() {
        crate::prefs::set_inspector_width(this.inspector_width);
    }
    if ns_ui::is_compact() && this.mobile_inspector {
        let unsaved = draft_unsaved(this, cx);
        return div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.panel)
            .child(
                div()
                    .h(px(40.))
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(tip_icon(
                        "inspector-back",
                        "back",
                        "Back to the canvas",
                        None,
                        IconTone::Quiet,
                        cx.listener(|this, _, _, cx| {
                            this.mobile_inspector = false;
                            cx.notify();
                        }),
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .min_w(px(0.))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("Inspector"),
                            )
                            .when(unsaved, |title| {
                                title.child(unsaved_mark("inspector-unsaved"))
                            }),
                    ),
            )
            .child(inspector(this, cx))
            .into_any_element();
    }

    let canvas = canvas_pane(this, cx);
    let mut content = div().flex_1().min_h(px(0.)).w_full().flex().child(canvas);
    // Compact windows keep the inspector as a full-pane overlay. Wider windows
    // put a drag handle between the canvas and the inspector.
    if !ns_ui::is_compact() {
        content = content
            .child(inspector_divider(this, cx))
            .child(inspector(this, cx));
    }

    div()
        .size_full()
        .flex()
        .flex_col()
        .child(toolbar(this, cx))
        .child(content)
        .into_any_element()
}

fn empty_workspace(_this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let theme = theme();
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .bg(theme.background)
        .child(
            div()
                .w(px(460.))
                .max_w_full()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap_3()
                .p_5()
                .rounded_lg()
                .bg(theme.panel)
                .border_1()
                .border_color(theme.border)
                .child(ns_ui::icon_tinted("sync", theme.accent))
                .child(
                    div()
                        .text_base()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("Build your first flow"),
                )
                .child(prose(
                    "A flow routes folders between this Mac and your remotes. Create one, add a node for each side, then drag a connection between them.",
                ))
                .child(
                    div()
                        .w_full()
                        .min_w(px(0.))
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(tip_button(
                            "empty-new-flow",
                            "plus",
                            "New flow",
                            "Create your first flow",
                            Some("⌘N"),
                            Look::Primary,
                            cx.listener(|this, _, _, cx| this.new_flow(cx)),
                        ))
                        .child(tip_button(
                            "empty-remotes",
                            "globe",
                            "Cloud accounts",
                            "Add or check a cloud account",
                            Some("⌘⇧R"),
                            Look::Quiet,
                            cx.listener(|this, _, _, cx| this.set_panel(crate::Panel::Remotes, cx)),
                        ))
                        .child(tip_button(
                            "empty-settings",
                            "settings",
                            "Settings",
                            "Password, service, and logs",
                            Some("⌘,"),
                            Look::Quiet,
                            cx.listener(|this, _, _, cx| {
                                this.set_panel(crate::Panel::Settings, cx)
                            }),
                        )),
                ),
        )
        .into_any_element()
}

fn draft_unsaved(this: &GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> bool {
    this.dirty || this.editors_differ(cx) || !this.inspector_issues(cx).is_empty()
}

fn unsaved_mark(id: impl Into<gpui::ElementId>) -> AnyElement {
    let theme = theme();
    crate::tip::with_tip(
        id,
        div()
            .size(px(8.))
            .flex_shrink_0()
            .rounded_full()
            .bg(theme.warn),
        inspector::UNSAVED_DOT_TIP,
        None,
    )
}

fn toolbar(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let theme = theme();
    // Icon buttons keep Add folder and Run reachable when the editor is narrow.
    let narrow = ns_ui::is_compact() || ns_ui::body_width() < 560.0;
    let running = this.selected_running();
    let unsaved = draft_unsaved(this, cx);
    let flow_title = this
        .draft
        .as_ref()
        .map(|draft| draft.title())
        .unwrap_or_else(|| "Untitled flow".into());
    let status = this
        .selected_flow
        .as_deref()
        .map(|id| this.flow_status(id))
        .unwrap_or_default();
    let ops = this.draft.as_ref().map(|d| d.operations.len()).unwrap_or(0);
    let when = this
        .draft
        .as_ref()
        .map(|draft| {
            if draft.schedule_enabled && !draft.schedule_cron.trim().is_empty() {
                cron_label(&draft.schedule_cron)
            } else {
                "Manual".into()
            }
        })
        .unwrap_or_else(|| "Manual".into());
    let summary = flow_summary(ops, &when);

    let add = if narrow {
        tip_icon(
            "tb-add-node",
            "plus",
            "Add a folder on this Mac or in a cloud account",
            None,
            IconTone::Quiet,
            cx.listener(|this, _, _, cx| this.add_node(cx)),
        )
    } else {
        tip_button(
            "tb-add-node",
            "plus",
            "Add folder",
            "Add a folder on this Mac or in a cloud account",
            None,
            Look::Quiet,
            cx.listener(|this, _, _, cx| this.add_node(cx)),
        )
    };
    // Run is the only filled button. Stop takes its place while the flow runs.
    let action = if running {
        if narrow {
            tip_icon(
                "tb-stop",
                "stop",
                "Stop this flow",
                Some("⌘."),
                IconTone::Quiet,
                cx.listener(|this, _, _, cx| this.stop_flow(cx)),
            )
        } else {
            tip_button(
                "tb-stop",
                "stop",
                "Stop",
                "Stop this flow",
                Some("⌘."),
                Look::Outline,
                cx.listener(|this, _, _, cx| this.stop_flow(cx)),
            )
        }
    } else if narrow {
        tip_icon(
            "tb-run",
            "play",
            "Run this flow now",
            Some("⌘R"),
            IconTone::Quiet,
            cx.listener(|this, _, _, cx| this.run_flow(cx)),
        )
    } else {
        tip_button(
            "tb-run",
            "play",
            "Run",
            "Run this flow now",
            Some("⌘R"),
            Look::Primary,
            cx.listener(|this, _, _, cx| this.run_flow(cx)),
        )
    };

    let name = crate::text::clip_tip("tb-name", flow_title.clone(), flow_title)
        .max_w(px(if narrow { 160. } else { 280. }))
        .text_sm()
        .font_weight(gpui::FontWeight::SEMIBOLD);
    let left = div()
        .flex()
        .flex_1()
        .min_w(px(0.))
        .items_center()
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .min_w(px(0.))
                .flex_shrink(1.)
                .child(name)
                .when(unsaved, |title| title.child(unsaved_mark("tb-unsaved"))),
        )
        .child(chip(status_label(&status), status_tone(&status)))
        .child(
            div().flex_1().min_w(px(0.)).child(
                crate::text::clip_tip("tb-summary", summary.clone(), summary)
                    .text_xs()
                    .text_color(theme.muted),
            ),
        );

    div()
        .h(px(44.))
        .w_full()
        .min_w(px(0.))
        .flex_shrink_0()
        .px_3()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.panel)
        .child(left)
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .child(add)
                .child(action),
        )
        .into_any_element()
}

fn status_tone(status: &str) -> Tone {
    match flow_status_tone(status) {
        StatusTone::Ok => Tone::Ok,
        StatusTone::Warn => Tone::Warn,
        StatusTone::Err => Tone::Err,
    }
}

// MARK: - Canvas

fn canvas_pane(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let theme = theme();
    // Nodes, edges, headings, and per-node status come from the graph cache;
    // per-edge progress, rankings, dots, and node progress from the sync
    // cache. Both rebuild only when the graph, runtime, or selection changes.
    let graph_cache = this.graph_cache();
    let nodes = graph_cache.nodes.clone();
    let edges = graph_cache.edges.clone();
    let headings = graph_cache.headings.clone();
    let sync_cache = this.sync_cache();
    let progresses = sync_cache.progresses.clone();
    let ranked_map = sync_cache.ranked.clone();
    let node_progress = sync_cache.node_progress.clone();
    let selection = this.selection.clone();
    let connecting = this.connect_from.is_some();
    let zoom = this.canvas.zoom;
    let pan = (this.canvas.pan_x, this.canvas.pan_y);
    let size_cell = this.canvas.size.clone();
    let origin_cell = this.canvas.origin.clone();

    let paint_nodes: Vec<(
        GraphNode,
        graph::NodeRect,
        String,
        bool,
        String,
        graph::NodeMode,
    )> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let (rect, heading, mode) = placed_node(this, node, &headings[index]);
            let status = node_status(node, &graph_cache);
            let selected = matches!(&selection, Selection::Node(id) if id == &node.id);
            (node.clone(), rect, status, selected, heading, mode)
        })
        .collect();

    // The edge painter sees the same rects the node cards use.
    let frames: HashMap<String, graph::NodeRect> = paint_nodes
        .iter()
        .map(|(node, rect, _, _, _, _)| (node.id.clone(), *rect))
        .collect();
    let paint_frames = frames.clone();
    // The rubber band of a ring drag: source ring to pointer.
    let rubber = match (&this.connect_from, &this.canvas.connect_drag) {
        (Some(source), Some(drag)) if drag.moved => frames
            .get(source)
            .map(|rect| (rect.right_center(), drag.pointer)),
        _ => None,
    };
    let rubber_color: Hsla = theme.accent.into();
    let paint_edges = edges.clone();
    let paint_progress = progresses.clone();
    let paint_dots = sync_cache.dots.clone();
    let selected_edge_for_paint = match &selection {
        Selection::Edge(id) => Some(id.clone()),
        _ => None,
    };
    let grid_color: Hsla = theme.border.into();
    let palette = EdgePalette {
        idle: Hsla::from(theme.border).opacity(0.85),
        running: Hsla::from(theme.accent),
        ok: Hsla::from(theme.ok),
        failed: Hsla::from(theme.err),
        warn: Hsla::from(theme.warn),
        checking: Hsla::from(Rgba {
            r: 0.12,
            g: 0.62,
            b: 0.58,
            a: 1.0,
        }),
    };

    let layer = gpui::canvas(
        move |bounds, window, _cx| {
            let now = (bounds.size.width.as_f32(), bounds.size.height.as_f32());
            if (size_cell.get().0 - now.0).abs() > 0.5 || (size_cell.get().1 - now.1).abs() > 0.5 {
                size_cell.set(now);
                window.refresh();
            }
            origin_cell.set((bounds.origin.x.into(), bounds.origin.y.into()));
            bounds
        },
        move |bounds, _, window, _cx| {
            let (w, h) = (bounds.size.width.as_f32(), bounds.size.height.as_f32());
            // A faint grid that moves with the graph.
            let step = (32.0 * zoom).max(8.0);
            let mut x = (-pan.0 * zoom).rem_euclid(step);
            while x < w {
                window.paint_quad(gpui::fill(
                    Bounds {
                        origin: point(bounds.origin.x + px(x), bounds.origin.y),
                        size: size(px(1.), px(h)),
                    },
                    grid_color.opacity(0.14),
                ));
                x += step;
            }
            let mut y = (-pan.1 * zoom).rem_euclid(step);
            while y < h {
                window.paint_quad(gpui::fill(
                    Bounds {
                        origin: point(bounds.origin.x, bounds.origin.y + px(y)),
                        size: size(px(w), px(1.)),
                    },
                    grid_color.opacity(0.14),
                ));
                y += step;
            }

            paint_curves(
                window,
                &CurvePaint {
                    origin: bounds.origin,
                    edges: &paint_edges,
                    frames: &paint_frames,
                    progress: &paint_progress,
                    dots: &paint_dots,
                    selected: selected_edge_for_paint.as_deref(),
                    palette: &palette,
                },
            );
            if let Some((from, to)) = rubber {
                let (c1, c2) = graph::edge_controls(from, to);
                let at =
                    |p: (f32, f32)| point(bounds.origin.x + px(p.0), bounds.origin.y + px(p.1));
                let mut path = gpui::PathBuilder::stroke(px(2.0));
                path.move_to(at(from));
                path.cubic_bezier_to(at(to), at(c1), at(c2));
                if let Ok(built) = path.build() {
                    window.paint_path(built, rubber_color);
                }
            }
        },
    )
    .absolute()
    .size_full();

    let mut canvas_body = div()
        .id("flow-canvas")
        .relative()
        .flex_1()
        .min_w(px(0.))
        .h_full()
        .overflow_hidden()
        .bg(theme.editor)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                this.canvas_down(event, window, cx)
            }),
        )
        .on_mouse_move(
            cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| this.canvas_move(event, cx)),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| this.canvas_up(cx)),
        )
        .on_scroll_wheel(
            cx.listener(|this, event: &ScrollWheelEvent, _, cx| this.zoom_event(event, cx)),
        )
        .on_pinch(cx.listener(|this, event: &PinchEvent, _, cx| this.pinch_event(event, cx)));

    // Children are flushed in `canvas_child_order`: edge lines, then labels,
    // then nodes, then chrome, then popovers. A file card therefore paints
    // above every node.
    let mut stacked: Vec<(CanvasLayer, AnyElement)> = Vec::new();
    stacked.push((CanvasLayer::Edges, layer.into_any_element()));
    let mut popovers: Vec<AnyElement> = Vec::new();

    // Edge labels sit on the strokes and under the node cards. The selected
    // edge's file card is held back for the popover layer.
    for edge in edges.iter() {
        let (Some(source), Some(target)) = (frames.get(&edge.source), frames.get(&edge.target))
        else {
            continue;
        };
        let from = source.right_center();
        let to = target.left_center();
        let forward = to.0 >= from.0 + 36.0;
        // Raw gap, not a forced minimum. A gap too small for the label
        // becomes an icon instead of a chip whose text is only "…".
        let available = if forward {
            (to.0 - from.0 - 12.0).max(0.0)
        } else {
            140.0
        };
        let mode = graph::chip_mode(zoom, available);
        let chip_w = graph::chip_draw_width(zoom, available);
        let mid_x = (from.0 + to.0) / 2.0;
        let mid_y = (from.1 + to.1) / 2.0;
        let left = mid_x - chip_w / 2.0;
        let top = mid_y
            - if mode == graph::ChipMode::IconOnly {
                graph::CHIP_ICON_H / 2.0
            } else {
                13.0
            };
        let selected = matches!(&selection, Selection::Edge(id) if id == &edge.id);
        let progress = progresses.get(&edge.id).map(|item| item.as_ref());
        let ranked = ranked_map.get(&edge.id).map(|order| order.as_slice());
        let id = edge.id.clone();
        let card = edge_card(edge, progress, selected, mode);
        stacked.push((
            CanvasLayer::EdgeLabels,
            div()
                .id(SharedString::from(format!("edge-card-{id}")))
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(chip_w))
                .when(mode == graph::ChipMode::IconOnly, |chip| {
                    chip.h(px(graph::CHIP_ICON_H))
                })
                .child(card)
                .tooltip(Tooltip::text(chip_tip(&edge.action)))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.select_edge(id.clone(), cx);
                }))
                .into_any_element(),
        ));
        if selected && !this.card_dismissed {
            let place = clamp_edge_popover(
                left,
                mid_y,
                this.canvas.size.get().0,
                this.canvas.size.get().1,
                EDGE_POPOVER_EST_H,
            );
            let source_name = nodes
                .iter()
                .find(|node| node.id == edge.source)
                .map(|node| node_heading(this, node))
                .unwrap_or_else(|| "Source".into());
            let target_name = nodes
                .iter()
                .find(|node| node.id == edge.target)
                .map(|node| node_heading(this, node))
                .unwrap_or_else(|| "Target".into());
            popovers.push(
                div()
                    .absolute()
                    .left(px(place.left))
                    .top(px(place.top))
                    .w(px(place.width))
                    .child(file_card(
                        edge,
                        &source_name,
                        &target_name,
                        progress,
                        ranked,
                        cx,
                    ))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .into_any_element(),
            );
        }
    }

    // Node cards. The connect ring is a sibling of the clipped face so the
    // full 14px target stays outside `overflow_hidden` in every mode.
    for (node, rect, status, selected, _heading, mode) in &paint_nodes {
        let node = node.clone();
        let id = node.id.clone();
        // The first incident edge with progress, indexed once per snapshot.
        let flow_progress = node_progress.get(&id);
        let running = flow_progress
            .map(|progress| progress.state == "running")
            .unwrap_or(false);
        let is_local = node.remote.is_empty() || node.remote == "local";
        let place = node_place(this, &node);
        let full_path = if node.path.trim().is_empty() {
            "/".to_string()
        } else {
            node.path.clone()
        };
        let copy = graph::node_card_copy(
            &node.label,
            &full_path,
            &place,
            graph::node_path_chars(rect.w),
        );
        // The card may shorten the path. The tooltip keeps every line whole.
        let tip = format!("{}\n{}\n{full_path}", copy.title, copy.place);
        let selected = *selected;
        let mode = *mode;
        let rect = *rect;

        let mut face = div()
            .id(SharedString::from(format!("node-{id}")))
            .tooltip(Tooltip::text(tip))
            .relative()
            .w(px(rect.w))
            .h(px(rect.h))
            .overflow_hidden()
            .rounded_md()
            .bg(theme.panel)
            .border_1()
            .border_color(if selected { theme.accent } else { theme.border })
            .cursor_pointer()
            .hover(|style| style.border_color(theme.accent_hover))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    this.node_down(id.clone(), event, cx)
                }),
            )
            .child(node_face(&copy, status, is_local, mode, rect.w, rect.h));
        if running && let Some(progress) = flow_progress {
            face = face.child(chip_progress_bar(progress));
        }
        let mut card = div()
            .absolute()
            .left(px(rect.x))
            .top(px(rect.y))
            .w(px(rect.w))
            .h(px(rect.h))
            .child(face);
        // Icon-only draws the status dot on the card, not inside the clipped
        // face, so the top-right corner can hang off the tile.
        if mode == graph::NodeMode::IconOnly {
            let marks = graph::icon_only_marks(rect);
            card = card.child(
                div()
                    .absolute()
                    .left(px(marks.dot.x - rect.x))
                    .top(px(marks.dot.y - rect.y))
                    .size(px(marks.dot.w))
                    .rounded_full()
                    .bg(theme.editor)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .size(px(graph::NODE_DOT))
                            .rounded_full()
                            .bg(status_color(status)),
                    ),
            );
        }
        // The ring on the right edge starts a connection.
        if !this.selected_running() {
            let from = node.id.clone();
            let handle = graph::NODE_HANDLE;
            card = card.child(
                div()
                    .id(SharedString::from(format!("connect-{}", node.id)))
                    .absolute()
                    .right(px(-handle / 2.0))
                    .top(gpui::relative(0.5))
                    .mt(px(-handle / 2.0))
                    .size(px(handle))
                    .rounded_full()
                    .bg(theme.raised)
                    .border_1()
                    .border_color(theme.border)
                    .cursor_pointer()
                    .tooltip(Tooltip::text(
                        "Drag onto another folder to connect, or click here and then click the other folder",
                    ))
                    .hover(|style| style.border_color(theme.accent).bg(theme.accent))
                    .on_mouse_down(MouseButton::Left, {
                        let from = from.clone();
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.begin_connect_drag(from.clone(), event, cx);
                        })
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.start_connect(from.clone(), cx);
                    })),
            );
        }
        stacked.push((CanvasLayer::Nodes, card.into_any_element()));
    }

    if crate::prefs::show_canvas_hint(nodes.len(), this.canvas_hint_dismissed) {
        stacked.push((
            CanvasLayer::Chrome,
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(280.))
                        .max_w_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .p_4()
                        .text_center()
                        .rounded_md()
                        .bg(theme.panel)
                        .border_1()
                        .border_color(theme.border)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child("Add a folder to start"),
                        )
                        .child(
                            div()
                                .w_full()
                                .min_w(px(0.))
                                .flex()
                                .flex_wrap()
                                .justify_center()
                                .items_center()
                                .gap_2()
                                .child(tip_button(
                                    "canvas-add-node",
                                    "plus",
                                    "Add folder",
                                    "Add a folder on this Mac or in a cloud account",
                                    None,
                                    Look::Primary,
                                    cx.listener(|this, _, _, cx| this.add_node(cx)),
                                ))
                                .child(tip_button(
                                    "canvas-hint-dismiss",
                                    "eye-off",
                                    "Dismiss",
                                    "Hide this hint",
                                    None,
                                    Look::Quiet,
                                    cx.listener(|this, _, _, cx| {
                                        this.canvas_hint_dismissed = true;
                                        crate::prefs::set_canvas_hint_dismissed(true);
                                        cx.notify();
                                    }),
                                )),
                        ),
                )
                .into_any_element(),
        ));
    }

    // Zoom and fit controls.
    let controls = div()
        .absolute()
        .left(px(12.))
        .top(px(12.))
        .flex()
        .items_center()
        .gap_1()
        .px_1()
        .py_1()
        .rounded_md()
        .bg(theme.panel)
        .border_1()
        .border_color(theme.border)
        .child(tip_icon(
            "canvas-zoom-out",
            "minus",
            "Zoom out",
            Some("⌘−"),
            IconTone::Quiet,
            cx.listener(|this, _, _, cx| {
                this.canvas.zoom_by(1.0 / 1.15, None);
                this.note_viewport(cx);
            }),
        ))
        .child(
            div()
                .text_xs()
                .text_color(theme.muted)
                .w(px(38.))
                .text_center()
                .child(format!("{:.0}%", zoom * 100.0)),
        )
        .child(tip_icon(
            "canvas-zoom-in",
            "plus",
            "Zoom in",
            Some("⌘="),
            IconTone::Quiet,
            cx.listener(|this, _, _, cx| {
                this.canvas.zoom_by(1.15, None);
                this.note_viewport(cx);
            }),
        ))
        .child(tip_icon(
            "canvas-fit",
            "scope",
            "Fit the graph in the window",
            Some("⌘0"),
            IconTone::Quiet,
            cx.listener(|this, _, _, cx| this.fit_canvas(cx)),
        ))
        .child(canvas_help());
    stacked.push((CanvasLayer::Chrome, controls.into_any_element()));

    if connecting {
        stacked.push((
            CanvasLayer::Chrome,
            div()
                .absolute()
                .right(px(12.))
                .top(px(if ns_ui::is_compact() { 48. } else { 12. }))
                .max_w_full()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(theme.warn_bg)
                .text_xs()
                .text_color(theme.warn)
                .child("Choose the target node")
                .into_any_element(),
        ));
    }

    // Popovers were collected in the edge loop. Append them after chrome, then
    // let the layer order put them above edges, labels, nodes, and chrome.
    for popover in popovers {
        stacked.push((CanvasLayer::Popovers, popover));
    }
    let layer_tags: Vec<CanvasLayer> = stacked.iter().map(|(layer, _)| *layer).collect();
    let order = canvas_child_order(&layer_tags);
    let mut slots: Vec<Option<AnyElement>> =
        stacked.into_iter().map(|(_, child)| Some(child)).collect();
    for index in order {
        if let Some(child) = slots[index].take() {
            canvas_body = canvas_body.child(child);
        }
    }

    canvas_body.into_any_element()
}

/// Screen rect, heading, and mode for one node. Edges and the card share this.
fn placed_node(
    this: &GnDriveWindow,
    node: &GraphNode,
    heading: &str,
) -> (graph::NodeRect, String, graph::NodeMode) {
    let (x, y) = this.canvas.screen(node.x, node.y);
    let zoom = this.canvas.zoom;
    let mode = graph::node_mode(zoom, NODE_W * zoom, heading);
    // One rect for the face and for edge endpoints. Compact width is
    // `compact_card_width`, grown from this left anchor.
    (
        graph::rendered_node_rect(x, y, mode, zoom, heading),
        heading.to_string(),
        mode,
    )
}

/// Icon, name, place, and path for one card. Place and path are full-mode only.
/// Compact text stays at `text_xs` and is clamped, so it never paints a bare `…`.
fn node_face(
    copy: &graph::NodeCardCopy,
    status: &str,
    is_local: bool,
    mode: graph::NodeMode,
    card_w: f32,
    card_h: f32,
) -> AnyElement {
    let heading = copy.title.as_str();
    let place = copy.place.as_str();
    let path_line = copy.path.as_str();
    let theme = theme();
    let glyph_name = if is_local { "folder" } else { "globe" };
    // A folder on this Mac uses the text colour. A cloud account uses the accent.
    let glyph = ns_ui::icon_tinted(glyph_name, if is_local { theme.text } else { theme.accent });
    let dot = div()
        .size(px(graph::NODE_DOT))
        .flex_shrink_0()
        .rounded_full()
        .bg(status_color(status));
    match mode {
        // The status dot is a sibling of this face (see the node loop) so it
        // can hang off the corner without the clip.
        graph::NodeMode::IconOnly => div()
            .relative()
            .w(px(card_w))
            .h(px(card_h))
            .flex()
            .items_center()
            .justify_center()
            .child(glyph)
            .into_any_element(),
        graph::NodeMode::Compact => {
            let mut row = div()
                .w(px(card_w))
                .h(px(card_h))
                .flex()
                .items_center()
                .gap(px(graph::NODE_COMPACT_GAP))
                .pl(px(graph::NODE_COMPACT_PAD_LEFT))
                .pr(px(graph::NODE_COMPACT_PAD_RIGHT))
                .child(div().flex_shrink_0().child(glyph));
            if let Some(name) = graph::node_caption(mode, heading, card_w) {
                row = row.child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .overflow_hidden()
                        .text_xs()
                        .whitespace_nowrap()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(SharedString::from(name)),
                );
            }
            row.child(dot).into_any_element()
        }
        graph::NodeMode::Full => {
            let mut column = div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .justify_center();
            if let Some(name) = graph::node_caption(mode, heading, card_w) {
                column = column.child(
                    div()
                        .text_sm()
                        .truncate()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(SharedString::from(name)),
                );
            }
            if !place.trim().is_empty() {
                column = column.child(
                    div()
                        .text_xs()
                        .truncate()
                        .text_color(theme.muted)
                        .child(SharedString::from(place.to_string())),
                );
            }
            if let Some(subtitle) = graph::node_detail(mode, path_line) {
                column = column.child(
                    div()
                        .text_xs()
                        .truncate()
                        .text_color(theme.muted)
                        .child(SharedString::from(subtitle)),
                );
            }
            div()
                .w(px(card_w))
                .h(px(card_h))
                .flex()
                .items_center()
                .gap_1p5()
                .px_2()
                .child(div().flex_shrink_0().child(glyph))
                .child(column)
                .child(dot)
                .into_any_element()
        }
    }
}

/// Folded status from the graph cache's per-node index; idle when missing.
fn node_status(node: &GraphNode, cache: &crate::cache::GraphCache) -> String {
    cache
        .node_status
        .get(&node.id)
        .cloned()
        .unwrap_or_else(|| "idle".into())
}

fn node_heading(_this: &GnDriveWindow, node: &GraphNode) -> String {
    graph::folder_title(&node.label, &node.path)
}

/// "This Mac", or the cloud account's friendly name.
fn node_place(this: &GnDriveWindow, node: &GraphNode) -> String {
    let friendly = if node.remote.is_empty() || node.remote == "local" {
        String::new()
    } else {
        this.remote_cache
            .friendly
            .get(&node.remote)
            .cloned()
            .unwrap_or_else(|| node.remote.clone())
    };
    graph::place_name(&node.remote, &friendly)
}

fn display_location(remote: &str, path: &str) -> String {
    if remote.is_empty() || remote == "local" {
        path.to_string()
    } else {
        format!("{remote}:{}", crate::graph::normalize_path(path))
    }
}

fn edge_card(
    edge: &GraphEdge,
    progress: Option<&SyncProgress>,
    selected: bool,
    mode: graph::ChipMode,
) -> AnyElement {
    let theme = theme();
    let action = edge.action.as_str();
    let glyph_name = action_glyph(action);
    let running = progress.is_some_and(|item| item.state == "running");
    let mut card = div()
        .relative()
        .overflow_hidden()
        .w_full()
        .rounded_md()
        .bg(if selected {
            theme.selected
        } else {
            theme.panel
        })
        .border_1()
        .border_color(if selected || running {
            theme.accent
        } else {
            theme.border
        })
        .cursor_pointer()
        .hover(|style| style.border_color(theme.accent_hover));
    card = if mode == graph::ChipMode::IconOnly {
        card.h_full()
            .flex()
            .items_center()
            .justify_center()
            .child(ns_ui::icon_tinted(glyph_name, theme.accent))
    } else {
        let caption = graph::chip_caption(mode, action_label(action));
        card.flex()
            .items_center()
            .gap_1()
            .px_1p5()
            .py_1()
            .child(
                div()
                    .flex_shrink_0()
                    .child(ns_ui::icon_tinted(glyph_name, theme.accent)),
            )
            .child({
                let mut column = div().flex_1().flex().flex_col().gap_0p5();
                if let Some(caption) = caption {
                    // text_xs is the floor. The mode decision already refused a
                    // width that would collapse this word to a lone ellipsis.
                    column = column.child(
                        div()
                            .text_xs()
                            .whitespace_nowrap()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(caption),
                    );
                }
                if mode == graph::ChipMode::Full
                    && let Some(progress) = progress
                {
                    let detail = if running {
                        format!(
                            "{} · {} · {}/{} files",
                            human_bytes(progress.transferred),
                            progress_speed(progress),
                            progress.files_transferred,
                            progress.total_files
                        )
                    } else {
                        status_label(&progress.state).to_string()
                    };
                    column = column.child(
                        div()
                            .min_w(px(0.))
                            .text_xs()
                            .truncate()
                            .text_color(status_color(&progress.state))
                            .child(SharedString::from(detail)),
                    );
                }
                column
            })
    };
    if running && let Some(progress) = progress {
        card = card.child(chip_progress_bar(progress));
    }
    card.into_any_element()
}

fn chip_progress_bar(progress: &SyncProgress) -> AnyElement {
    let theme = theme();
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(px(2.))
        .child(
            div()
                .h_full()
                .w(gpui::relative(
                    fraction(progress.transferred, progress.total).max(0.01),
                ))
                .bg(theme.accent),
        )
        .into_any_element()
}

fn file_card(
    edge: &GraphEdge,
    source: &str,
    target: &str,
    progress: Option<&SyncProgress>,
    ranked: Option<&[u32]>,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let theme = theme();
    let route = if matches!(edge.action.as_str(), "bi" | "bi-resync") {
        format!("Keep {source} and {target} the same")
    } else {
        format!("{source} → {target}")
    };
    let mut card = div()
        .p_2()
        .rounded_md()
        .bg(theme.panel)
        .border_1()
        .border_color(theme.border)
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(chip(action_label(&edge.action), Tone::Accent))
                .child(
                    div().flex_1().min_w(px(0.)).child(
                        crate::text::clip(
                            SharedString::from(format!("file-route-{}", edge.id)),
                            route,
                        )
                        .text_xs(),
                    ),
                )
                .child(tip_icon(
                    format!("file-card-close-{}", edge.id),
                    "eye-off",
                    "Hide this file list. Click the connection again to open it",
                    None,
                    IconTone::Quiet,
                    cx.listener(|this, _, _, cx| {
                        this.card_dismissed = true;
                        cx.notify();
                    }),
                )),
        )
        .child(prose(action_help(&edge.action)));
    if let Some(sync) = progress {
        let speed = progress_speed(sync);
        let mut stats = format!(
            "{}/{} files · {}",
            sync.files_transferred,
            sync.total_files,
            human_bytes(sync.transferred)
        );
        if !speed.is_empty() {
            stats.push_str(" · ");
            stats.push_str(&speed);
        }
        card = card.child(prose(stats));
        if !sync.stage.is_empty() {
            let stage = if sync.stage_detail.is_empty() {
                sync.stage.clone()
            } else {
                format!("{} · {}", sync.stage, sync.stage_detail)
            };
            card = card.child(prose(stage));
        }
        if sync.errors > 0 || !sync.error_message.is_empty() {
            let (shown, full) = if sync.error_message.is_empty() {
                let text = format!("{} errors.", sync.errors);
                (text.clone(), text)
            } else {
                friendly_error(&sync.error_message)
            };
            if !shown.is_empty() {
                card = card.child(crate::text::wrap_err_tip(
                    SharedString::from(format!("file-error-{}", edge.id)),
                    shown,
                    full,
                ));
            }
        }
    }
    // The card order comes from the sync cache's prebuilt ranking.
    let files: Vec<&FileTransfer> = progress
        .zip(ranked)
        .map(|(sync, order)| {
            order
                .iter()
                .map(|&index| &sync.transfers[index as usize])
                .collect()
        })
        .unwrap_or_default();
    if progress.is_none() {
        card = card.child(prose("No activity yet. Press Run to sync these folders."));
    } else if files.is_empty() {
        card = card.child(prose("Preparing… files show up here as they move."));
    } else {
        let mut list = div().flex().flex_col().gap_0p5();
        for (index, transfer) in files.iter().take(8).enumerate() {
            let full = transfer.name.clone();
            list = list.child(
                tip_on(
                    div().id(SharedString::from(format!("file-row-{}-{index}", edge.id))),
                    full.clone(),
                    None,
                )
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div().flex_1().min_w(px(0.)).child(
                        crate::text::clip_tip(
                            SharedString::from(format!("file-name-{}-{index}", edge.id)),
                            middle_ellipsis(&full, 32),
                            full.clone(),
                        )
                        .text_xs(),
                    ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(theme.muted)
                        .child(file_status(&transfer.status, transfer.progress)),
                ),
            );
        }
        card = card.child(list);
    }
    card.into_any_element()
}

// MARK: - Inspector

fn inspector_target(this: &GnDriveWindow) -> (&'static str, Selection) {
    match this.selection.clone() {
        Selection::Flow if this.draft.is_some() => ("Flow", Selection::Flow),
        Selection::Node(id) if this.node(&id).is_some() => ("Folder", Selection::Node(id)),
        Selection::Edge(id) if this.edge(&id).is_some() => ("Connection", Selection::Edge(id)),
        _ => ("Nothing selected", Selection::None),
    }
}

fn inspector(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let theme = theme();
    let running = this.selected_running();
    let issues = this.inspector_issues(cx);
    let unsaved = draft_unsaved(this, cx);
    let bar = inspector::save_bar(unsaved, running, &issues);
    let (context, resolved) = inspector_target(this);
    let header_note = if running { "Read-only" } else { context };
    let header = div()
        .h(px(40.))
        .px_3()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap_2()
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.muted)
                .child("Inspector"),
        )
        .child(div().text_xs().text_color(theme.muted).child(header_note));
    let body = match resolved {
        Selection::Flow => flow_inspector(this, cx),
        Selection::Node(id) => node_inspector(this, &id, cx),
        Selection::Edge(id) => edge_inspector(this, &id, cx),
        Selection::None => empty_inspector(this, cx),
    };
    let scroll = div()
        .id("inspector-scroll")
        .flex_1()
        .min_h(px(0.))
        .w_full()
        .overflow_y_scroll()
        .track_scroll(&this.inspector_scroll)
        .child(
            div()
                .w_full()
                .min_w(px(0.))
                .p_3()
                .flex()
                .flex_col()
                .gap_3()
                .when(running, |column| column.child(running_banner()))
                .child(body),
        );
    let mut panel = div()
        .id("inspector")
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(theme.panel)
        .border_l_1()
        .border_color(theme.border)
        .child(header)
        .child(scroll)
        .child(inspector_footer(
            bar,
            unsaved,
            this.flow_save_in_flight(),
            cx,
        ));
    if ns_ui::is_compact() {
        panel = panel.w_full().flex_1().min_h(px(0.));
    } else {
        let width = this.effective_inspector_width();
        panel = panel
            .w(px(width))
            .min_w(px(inspector::MIN_WIDTH))
            .flex_shrink_0();
    }
    panel.into_any_element()
}

fn running_banner() -> AnyElement {
    let theme = theme();
    div()
        .w_full()
        .rounded_md()
        .px_2()
        .py_1p5()
        .bg(theme.warn_bg)
        .child(
            crate::text::wrap("Stop this flow before editing.")
                .text_xs()
                .text_color(theme.warn),
        )
        .into_any_element()
}

fn inspector_footer(
    bar: inspector::SaveBar,
    unsaved: bool,
    saving: bool,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let theme = theme();
    let mut status = div().flex_1().min_w(px(0.)).flex().flex_col().gap_0p5();
    status = status.child(
        div()
            .text_xs()
            .whitespace_normal()
            .text_color(if unsaved { theme.text } else { theme.muted })
            .child(bar.status),
    );
    if let Some(note) = bar.note {
        status = status.child(
            div()
                .text_xs()
                .whitespace_normal()
                .text_color(theme.muted)
                .child(note),
        );
    }
    let mut row = div()
        .id("inspector-footer")
        .w_full()
        .flex_shrink_0()
        .px_3()
        .py_2()
        .gap_2()
        .flex()
        .flex_wrap()
        .items_center()
        .border_t_1()
        .border_color(theme.border)
        .bg(theme.panel)
        .child(status);
    if bar.show_discard {
        let discard = if bar.discard_enabled {
            tip_button(
                "ins-discard",
                "restart",
                "Discard",
                bar.discard_tooltip,
                None,
                Look::Quiet,
                cx.listener(|this, _, _, cx| this.discard_draft(cx)),
            )
        } else {
            tip_button_off(
                "ins-discard",
                "restart",
                "Discard",
                bar.discard_tooltip,
                None,
                Look::Quiet,
            )
        };
        row = row.child(discard);
    }
    let save = if saving {
        tip_button_off(
            "ins-save",
            "check",
            "Saving…",
            "Saving this flow",
            Some("⌘S"),
            Look::Primary,
        )
    } else if bar.save_enabled {
        tip_button(
            "ins-save",
            "check",
            "Save",
            bar.save_tooltip,
            Some("⌘S"),
            Look::Primary,
            cx.listener(|this, _, _, cx| this.save_flow(false, cx)),
        )
    } else {
        tip_button_off(
            "ins-save",
            "check",
            "Save",
            bar.save_tooltip,
            Some("⌘S"),
            Look::Quiet,
        )
    };
    row.child(save).into_any_element()
}

fn empty_inspector(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let (title, hint) = inspector::empty_state_copy();
    let running = this.selected_running();
    let add = if running {
        tip_button_off(
            "ins-add-folder",
            "plus",
            "Add folder",
            "Stop the flow before adding a folder.",
            None,
            Look::Primary,
        )
    } else {
        tip_button(
            "ins-add-folder",
            "plus",
            "Add folder",
            "Add a folder on this Mac or in a cloud account",
            None,
            Look::Primary,
            cx.listener(|this, _, _, cx| this.add_node(cx)),
        )
    };
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_3()
        .py_2()
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(prose(hint))
        .child(prose("Flow settings holds the name and the schedule."))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(add)
                .child(tip_button(
                    "ins-flow-settings",
                    "settings",
                    "Flow settings",
                    "Edit this flow's name, schedule, and delete action",
                    None,
                    Look::Quiet,
                    cx.listener(|this, _, _, cx| this.pick_selection(Selection::Flow, cx)),
                )),
        )
        .into_any_element()
}

fn flow_inspector(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    if this.draft.is_none() {
        return empty_inspector(this, cx);
    }
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_3()
        .child(section_block(this, Section::Basics, cx, |this, cx| {
            flow_basics(this, cx)
        }))
        .child(section_block(this, Section::Schedule, cx, |this, cx| {
            flow_schedule(this, cx)
        }))
        .child(section_block(this, Section::Danger, cx, |this, cx| {
            flow_danger(this, cx)
        }))
        .into_any_element()
}

fn flow_basics(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let Some(draft) = this.draft.clone() else {
        return div().into_any_element();
    };
    let running = this.selected_running();
    let id = draft.id.clone();
    let status = this.flow_status(&id);
    let connections = draft.operations.len();
    let empty = this
        .graph
        .as_ref()
        .map(|graph| graph.nodes.is_empty())
        .unwrap_or(true);
    let mut column = div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(stacked_field(
            "Name",
            "",
            ns_ui::text_field("flow-name", &this.flow_name_input).into_any_element(),
            "Shown in the sidebar.",
            None,
        ))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(chip(status_label(&status), status_tone(&status)))
                .child(chip(
                    format!(
                        "{connections} connection{}",
                        if connections == 1 { "" } else { "s" }
                    ),
                    Tone::Neutral,
                )),
        )
        .child(div().flex().items_center().gap_2().child(if running {
            tip_button(
                "ins-stop",
                "stop",
                "Stop",
                "Stop this flow",
                Some("⌘."),
                Look::Outline,
                cx.listener(|this, _, _, cx| this.stop_flow(cx)),
            )
        } else {
            tip_button(
                "ins-run",
                "play",
                "Run now",
                "Run this flow now",
                Some("⌘R"),
                Look::Primary,
                cx.listener(|this, _, _, cx| this.run_flow(cx)),
            )
        }))
        .child(runtime_log(this, &id));
    if empty {
        column = column.child(prose(
            "Add a folder, then connect two folders to run this flow.",
        ));
    }
    column.into_any_element()
}

fn flow_schedule(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let Some(draft) = this.draft.clone() else {
        return div().into_any_element();
    };
    let running = this.selected_running();
    let entity = cx.entity();
    let entity_cron = entity.clone();
    let presets: Vec<(&'static str, &'static str)> = vec![
        ("", "Manual"),
        ("0 * * * *", "Hourly"),
        ("0 */6 * * *", "Every 6 hours"),
        ("0 0 * * *", "Daily at midnight"),
        ("0 3 * * *", "Daily at 03:00"),
        ("0 9 * * 1-5", "Weekdays at 09:00"),
        ("0 0 * * 0", "Weekly on Sunday"),
    ];
    let current = draft.schedule_cron.clone();
    let custom = !current.trim().is_empty() && !presets.iter().any(|(cron, _)| *cron == current);
    let selected = if custom {
        "__custom__"
    } else {
        current.as_str()
    };
    let mut preset_options: Vec<SelectOption> = presets
        .iter()
        .map(|(cron, label)| SelectOption::new(*cron, *label))
        .collect();
    preset_options.push(SelectOption::new("__custom__", "Custom"));
    let preview = if draft.schedule_enabled {
        cron_label(&draft.schedule_cron)
    } else {
        "Runs only when you press Run".into()
    };
    let cron_issue = cron_error(&current).map(str::to_string);
    let mut column = div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(toggle_row(
            "Run on a schedule",
            "When this is off, the flow runs only when you press Run.",
            tip_switch(
                "flow-schedule",
                draft.schedule_enabled,
                running,
                "Turn the schedule on. The flow then runs by itself at the time you pick",
                {
                    let entity = entity.clone();
                    move |on, _, cx| {
                        entity.update(cx, |this, cx| {
                            if this.selected_running() {
                                return;
                            }
                            if let Some(draft) = &mut this.draft {
                                draft.schedule_enabled = on;
                                this.dirty = true;
                            }
                            cx.notify();
                        });
                    }
                },
            ),
        ))
        .child(
            div()
                .text_sm()
                .whitespace_normal()
                .child(SharedString::from(preview)),
        )
        .child(stacked_field(
            "When",
            "",
            div()
                .w_full()
                .child(tip_select(
                    "flow-cron-preset",
                    preset_options,
                    Some(selected),
                    "Choose a schedule",
                    "Pick how often this flow runs. Custom lets you type a schedule",
                    running,
                    move |value: &SharedString, window, cx| {
                        entity_cron.update(cx, |this, cx| {
                            if value.as_ref() == "__custom__" {
                                if let Some(draft) = &mut this.draft
                                    && presets_include(&draft.schedule_cron)
                                {
                                    draft.schedule_cron = "15 4 * * *".into();
                                    draft.schedule_enabled = true;
                                    this.dirty = true;
                                }
                            } else {
                                this.set_cron(value.as_ref(), cx);
                            }
                            this.prepare_form_fields(window, cx);
                        });
                    },
                ))
                .into_any_element(),
            "The sentence above is the same schedule in plain English.",
            None,
        ));
    if custom {
        column = column.child(stacked_field(
            "Cron",
            "",
            ns_ui::text_field("flow-cron", &this.cron_input).into_any_element(),
            "Five fields: minute hour day month weekday.",
            cron_issue,
        ));
    }
    column.into_any_element()
}

fn flow_danger(_this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(prose(
            "Deleting a flow removes its connections, schedule, and canvas. Run history stays.",
        ))
        .child(tip_button(
            "ins-delete-flow",
            "trash",
            "Delete flow",
            "Delete this flow. You will be asked to confirm, and the question names the flow",
            None,
            Look::OutlineDanger,
            cx.listener(|this, _, _, cx| this.delete_selected_flow(cx)),
        ))
        .into_any_element()
}

fn node_inspector(
    this: &mut GnDriveWindow,
    node_id: &str,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    if this.node(node_id).is_none() {
        return empty_inspector(this, cx);
    }
    let id = node_id.to_string();
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_3()
        .child(section_block(this, Section::Basics, cx, {
            let id = id.clone();
            move |this, cx| node_basics(this, &id, cx)
        }))
        .child(section_block(this, Section::Connections, cx, {
            let id = id.clone();
            move |this, cx| node_connections(this, &id, cx)
        }))
        .child(section_block(this, Section::Danger, cx, {
            move |this, cx| node_danger(this, &id, cx)
        }))
        .into_any_element()
}

fn node_basics(
    this: &mut GnDriveWindow,
    node_id: &str,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let Some(node) = this.node(node_id).cloned() else {
        return div().into_any_element();
    };
    let running = this.selected_running();
    let id = node.id.clone();
    let location = display_location(&node.remote, &node.path);
    let mut remotes = vec![("local".to_string(), "This Mac".to_string())];
    for remote in this.remotes().iter() {
        let raw = if remote.description.is_empty() {
            remote.kind.clone()
        } else {
            remote.description.clone()
        };
        remotes.push((remote.name.clone(), middle_ellipsis(&raw, 36)));
    }
    let options: Vec<SelectOption> = remotes
        .iter()
        .map(|(value, label)| SelectOption::new(value.clone(), label.clone()))
        .collect();
    let current_remote = if node.remote.is_empty() {
        "local".to_string()
    } else {
        node.remote.clone()
    };
    let path = node.path.clone();
    let entity = cx.entity();
    let path_error = this
        .node_path_input
        .read(cx)
        .text()
        .trim()
        .is_empty()
        .then(|| inspector::PATH_REQUIRED.to_string());
    let mut column = div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(stacked_field(
            "Label",
            "",
            ns_ui::text_field("node-label", &this.node_label_input).into_any_element(),
            "A short name on the canvas. The path stays the real location.",
            None,
        ))
        .child(stacked_field(
            "Path",
            "",
            div()
                .w_full()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .child(ns_ui::text_field("node-path", &this.node_path_input)),
                )
                .child(div().flex_shrink_0().child(folder_button(
                    "node-browse",
                    "Pick the folder in a list instead of typing the path",
                    running,
                    cx.listener({
                        let id = id.clone();
                        let location = location.clone();
                        move |this, _, _, cx| {
                            this.open_browse(
                                BrowseTarget::NodePath(id.clone()),
                                location.clone(),
                                cx,
                            )
                        }
                    }),
                )))
                .into_any_element(),
            "A folder on this Mac, or remote:path for a cloud account.",
            path_error,
        ))
        .child(stacked_field(
            "Account",
            "",
            div()
                .w_full()
                .child(tip_select(
                    "node-remote",
                    options,
                    Some(current_remote.as_str()),
                    "This Mac",
                    "Choose whether this folder is on this Mac or in a cloud account",
                    running,
                    {
                        let id = id.clone();
                        let path = path.clone();
                        move |value: &SharedString, _, cx| {
                            let id = id.clone();
                            let path = path.clone();
                            let remote = value.to_string();
                            entity.update(cx, |this, cx| {
                                this.set_node_path(
                                    &id,
                                    &display_location(&remote, &path),
                                    Some(remote),
                                );
                                this.form_selection = None;
                                cx.notify();
                            });
                        }
                    },
                ))
                .into_any_element(),
            "This Mac, or a cloud account you have added.",
            None,
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(field_label_row("Location", ""))
                .child(
                    crate::text::clip_tip(
                        "node-location",
                        middle_ellipsis(&location, 42),
                        location.clone(),
                    )
                    .text_xs(),
                ),
        );
    if !node.label.trim().is_empty() {
        column = column.child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(field_label_row("Shown as", ""))
                .child(crate::text::clip("node-shown-as", node.label.clone()).text_xs()),
        );
    }
    if running {
        column = column.child(prose("Stop the flow before editing its locations."));
    }
    column.into_any_element()
}

fn edge_link(
    this: &mut GnDriveWindow,
    edge_id: &str,
    label: &str,
    arrow: &'static str,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let theme = theme();
    let edge = this.edge(edge_id).cloned();
    let action = edge
        .as_ref()
        .map(|edge| edge.action.clone())
        .unwrap_or_default();
    let id = edge_id.to_string();
    let help = action_help(&action);
    tip_on(
        div().id(SharedString::from(format!("edge-link-{edge_id}"))),
        format!("{label}\n{help}"),
        None,
    )
    .on_click(cx.listener(move |this, _, _, cx| this.select_edge(id.clone(), cx)))
    .flex()
    .items_center()
    .gap_2()
    .px_2()
    .py_1p5()
    .rounded_md()
    .border_1()
    .border_color(theme.border)
    .cursor_pointer()
    .hover(|style| style.bg(theme.hover))
    .child(div().text_xs().text_color(theme.muted).child(arrow))
    .child(
        div()
            .flex_1()
            .min_w(px(0.))
            .truncate()
            .text_sm()
            .child(SharedString::from(label.to_string())),
    )
    .child(chip(
        match action.as_str() {
            "bi" => "Two-way",
            "bi-resync" => "Re-sync",
            _ => "Push",
        },
        Tone::Neutral,
    ))
    .into_any_element()
}

fn progress_panel(sync: &SyncProgress) -> AnyElement {
    let theme = theme();
    let mut panel = div()
        .rounded_md()
        .p_3()
        .bg(theme.raised)
        .border_1()
        .border_color(theme.border)
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(chip(status_label(&sync.state), status_tone(&sync.state)))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted)
                        .child(progress_speed(sync)),
                ),
        )
        .child(
            div()
                .h(px(4.))
                .w_full()
                .rounded_full()
                .bg(theme.panel)
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .bg(status_color(&sync.state))
                        .w(gpui::relative(
                            fraction(sync.transferred, sync.total).max(0.01),
                        )),
                ),
        )
        .child(
            div()
                .flex()
                .justify_between()
                .text_xs()
                .text_color(theme.muted)
                .child(format!(
                    "{} / {}",
                    human_bytes(sync.transferred),
                    human_bytes(sync.total)
                ))
                .child(format!(
                    "{} / {} files",
                    sync.files_transferred, sync.total_files
                )),
        );
    if !sync.current_file.is_empty() {
        panel = panel.child(
            crate::text::clip("progress-current-file", sync.current_file.clone())
                .text_xs()
                .text_color(theme.text),
        );
    }
    if sync.eta > 0 && sync.state == "running" {
        panel = panel.child(
            div()
                .text_xs()
                .text_color(theme.muted)
                .child(format!("About {} left", human_duration(sync.eta as f64))),
        );
    }
    let ranked = ranked_transfers(&sync.transfers);
    if !ranked.is_empty() {
        let mut list = div().flex().flex_col().gap_1().pt_1();
        for (index, transfer) in ranked.iter().take(6).enumerate() {
            list = list.child(transfer_row(transfer, index));
        }
        panel = panel.child(list);
    }
    if sync.errors > 0 || !sync.error_message.is_empty() {
        let (shown, full) = if sync.error_message.is_empty() {
            let text = format!("{} errors.", sync.errors);
            (text.clone(), text)
        } else {
            friendly_error(&sync.error_message)
        };
        panel = panel.child(
            div()
                .rounded_md()
                .p_2()
                .bg(theme.err_bg)
                .child(crate::text::wrap_err_tip("progress-error", shown, full)),
        );
    }
    panel.into_any_element()
}

fn transfer_row(transfer: &FileTransfer, index: usize) -> AnyElement {
    let theme = theme();
    let progress = if transfer.progress > 1.0 {
        (transfer.progress / 100.0).clamp(0.0, 1.0)
    } else {
        transfer.progress.clamp(0.0, 1.0)
    } as f32;
    let status = if transfer.status == "failed" {
        "Failed".to_string()
    } else if transfer.status == "pending" {
        "Waiting".to_string()
    } else if matches!(transfer.status.as_str(), "completed" | "checked") {
        format!("Done · {}", human_bytes(transfer.size))
    } else {
        format!(
            "{} · {}",
            percent_text(transfer.progress),
            human_bytes(transfer.size)
        )
    };
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div().flex_1().min_w(px(0.)).child(
                        crate::text::clip(
                            SharedString::from(format!("xfer-name-{index}")),
                            transfer.name.clone(),
                        )
                        .text_xs(),
                    ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(theme.muted)
                        .child(status),
                ),
        )
        .child(
            div()
                .h(px(2.))
                .w_full()
                .rounded_full()
                .bg(theme.panel)
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .bg(if transfer.status == "failed" {
                            theme.err
                        } else {
                            theme.accent
                        })
                        .w(gpui::relative(progress.max(0.02))),
                ),
        )
        .into_any_element()
}

fn node_connections(
    this: &mut GnDriveWindow,
    node_id: &str,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let theme = theme();
    let mut incoming = div().flex().flex_col().gap_1();
    let mut outgoing = div().flex().flex_col().gap_1();
    let mut in_count = 0;
    let mut out_count = 0;
    if let Some(graph) = this.graph.clone() {
        for edge in &graph.edges {
            if edge.target == node_id {
                let label = graph
                    .nodes
                    .iter()
                    .find(|node| node.id == edge.source)
                    .map(|node| node.title())
                    .unwrap_or_else(|| "Unknown".into());
                incoming = incoming.child(edge_link(this, &edge.id, &label, "→", cx));
                in_count += 1;
            }
            if edge.source == node_id {
                let label = graph
                    .nodes
                    .iter()
                    .find(|node| node.id == edge.target)
                    .map(|node| node.title())
                    .unwrap_or_else(|| "Unknown".into());
                outgoing = outgoing.child(edge_link(this, &edge.id, &label, "→", cx));
                out_count += 1;
            }
        }
    }
    if in_count == 0 {
        incoming = incoming.child(prose("Nothing copies into this folder yet."));
    }
    if out_count == 0 {
        outgoing = outgoing.child(prose("This folder does not copy anywhere yet."));
    }
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_xs().text_color(theme.muted).child("From"))
        .child(incoming)
        .child(div().text_xs().text_color(theme.muted).child("To"))
        .child(outgoing)
        .child(prose(
            "Rename a folder here and every connection that uses it follows.",
        ))
        .into_any_element()
}

fn node_danger(
    _this: &mut GnDriveWindow,
    node_id: &str,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let id = node_id.to_string();
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(prose(
            "Removing a folder also removes every connection to it. This happens immediately.",
        ))
        .child(tip_button(
            "node-delete",
            "trash",
            "Remove folder",
            "Remove this folder and every connection to it. This happens immediately",
            Some("Delete"),
            Look::OutlineDanger,
            cx.listener(move |this, _, _, cx| this.delete_node(id.clone(), cx)),
        ))
        .into_any_element()
}

fn edge_inspector(
    this: &mut GnDriveWindow,
    edge_id: &str,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let Some(edge) = this.edge(edge_id).cloned() else {
        return empty_inspector(this, cx);
    };
    let id = edge_id.to_string();
    let action = edge.action.clone();
    let summary = connection_sentence(this, &edge);
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .text_sm()
                .whitespace_normal()
                .child(SharedString::from(summary)),
        )
        .child(section_block(this, Section::Basics, cx, {
            let id = id.clone();
            move |this, cx| edge_basics(this, &id, cx)
        }))
        .child(section_block(this, Section::WhatToSync, cx, {
            let action = action.clone();
            move |this, cx| sync_section_body(this, Section::WhatToSync, &action, cx)
        }))
        .child(section_block(this, Section::Safety, cx, {
            let action = action.clone();
            move |this, cx| sync_section_body(this, Section::Safety, &action, cx)
        }))
        .child(section_block(this, Section::Advanced, cx, {
            let action = action.clone();
            move |this, cx| sync_section_body(this, Section::Advanced, &action, cx)
        }))
        .child(section_block(this, Section::Danger, cx, {
            move |this, cx| edge_danger(this, &id, cx)
        }))
        .into_any_element()
}

fn edge_basics(
    this: &mut GnDriveWindow,
    edge_id: &str,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let theme = theme();
    let Some(edge) = this.edge(edge_id).cloned() else {
        return div().into_any_element();
    };
    let running = this.selected_running();
    let flow_id = this.selected_flow.clone().unwrap_or_default();
    let sync = this.op_sync(&flow_id, edge_id);
    let runtime_error = this
        .op_runtime(&flow_id, edge_id)
        .map(|op| op.last_error)
        .unwrap_or_default();
    let current_action = edge.action.clone();
    let edge_source = this
        .node(&edge.source)
        .map(|node| display_location(&node.remote, &node.path))
        .unwrap_or_default();
    let edge_target = this
        .node(&edge.target)
        .map(|node| display_location(&node.remote, &node.path))
        .unwrap_or_default();
    let source_error = this
        .edge_source_input
        .read(cx)
        .text()
        .trim()
        .is_empty()
        .then(|| inspector::SOURCE_REQUIRED.to_string());
    let target_error = this
        .edge_target_input
        .read(cx)
        .text()
        .trim()
        .is_empty()
        .then(|| inspector::TARGET_REQUIRED.to_string());
    let source_browse = folder_button(
        "edge-source-browse",
        "Pick the source folder in a list",
        running,
        cx.listener({
            let id = edge_id.to_string();
            let source = edge_source.clone();
            move |this, _, _, cx| {
                this.open_browse(BrowseTarget::EdgeSource(id.clone()), source.clone(), cx)
            }
        }),
    );
    let target_browse = folder_button(
        "edge-target-browse",
        "Pick the destination folder in a list",
        running,
        cx.listener({
            let id = edge_id.to_string();
            let target = edge_target.clone();
            move |this, _, _, cx| {
                this.open_browse(BrowseTarget::EdgeTarget(id.clone()), target.clone(), cx)
            }
        }),
    );
    let mut body = div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(tip_segmented(
            "edge-action",
            [
                ("push", "Push", "arrow-down-to-line"),
                ("bi", "Two-way", "sync"),
                ("bi-resync", "Re-sync", "restart"),
            ],
            &current_action,
            action_help(&current_action),
            running,
            cx.listener(|this, value: &SharedString, _, cx| {
                this.set_edge_action(value.as_ref(), cx);
            }),
        ))
        .child(prose(action_help(&current_action)))
        .child(stacked_field(
            "From",
            "",
            path_and_browse("edge-source", &this.edge_source_input, source_browse),
            "The folder files are copied from.",
            source_error,
        ))
        .child(stacked_field(
            "To",
            "",
            path_and_browse("edge-target", &this.edge_target_input, target_browse),
            "The folder files are copied to.",
            target_error,
        ))
        .child(prose(
            "Switches and menus apply immediately. Text applies when you save.",
        ));
    if let Some(sync) = &sync {
        body = body.child(progress_panel(sync));
    }
    let panel_has_error = sync
        .as_ref()
        .is_some_and(|sync| sync.errors > 0 || !sync.error_message.is_empty());
    if !runtime_error.is_empty() && !panel_has_error {
        let (shown, full) = friendly_error(&runtime_error);
        body = body.child(div().rounded_md().p_2().bg(theme.err_bg).child(
            crate::text::wrap_err_tip(
                SharedString::from(format!("edge-error-{edge_id}")),
                shown,
                full,
            ),
        ));
    }
    body.into_any_element()
}

fn edge_danger(
    _this: &mut GnDriveWindow,
    edge_id: &str,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let id = edge_id.to_string();
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(prose("Removing a connection does not delete the folders."))
        .child(tip_button(
            "edge-delete",
            "trash",
            "Delete connection",
            "Remove this connection. The folders stay. Press Delete to do the same",
            Some("Delete"),
            Look::OutlineDanger,
            cx.listener(move |this, _, _, cx| {
                this.card_dismissed = true;
                this.delete_edge(id.clone(), cx);
            }),
        ))
        .into_any_element()
}

fn sync_section_body(
    this: &mut GnDriveWindow,
    section: Section,
    action: &str,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let running = this.selected_running();
    let config = this.sync_config.clone().unwrap_or_default();
    let entity = cx.entity();
    let mut column = div().w_full().flex().flex_col().gap_3();
    for def in inspector::fields_in(section, action) {
        column = column.child(sync_field(this, def, &config, running, entity.clone(), cx));
    }
    column.into_any_element()
}

fn sync_field(
    this: &GnDriveWindow,
    def: &FieldDef,
    config: &SyncConfig,
    running: bool,
    entity: gpui::Entity<GnDriveWindow>,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    let meta = inspector::present(def.key);
    let label = meta.map(|item| item.label).unwrap_or(def.label);
    let help = meta.map(|item| item.help).unwrap_or("");
    let unit = meta.map(|item| item.unit).unwrap_or("");
    match def.kind {
        FieldKind::Bool => {
            let key = def.key;
            let tip = if help.is_empty() { label } else { help };
            toggle_row(
                label,
                help,
                tip_switch(
                    SharedString::from(format!("sync-bool-{}", def.key)),
                    config.flag(def.key),
                    running,
                    tip,
                    move |on, _, cx| {
                        entity.update(cx, |this, cx| this.set_sync_flag(key, on, cx));
                    },
                ),
            )
        }
        FieldKind::Select(options) => {
            let key = def.key;
            let current = config.text(def.key);
            let choices: Vec<SelectOption> = options
                .iter()
                .map(|(value, name)| SelectOption::new(*value, *name))
                .collect();
            stacked_field(
                label,
                unit,
                div()
                    .w_full()
                    .child(tip_select(
                        SharedString::from(format!("sync-select-{}", def.key)),
                        choices,
                        Some(current.as_str()),
                        "Default",
                        format!("Choose {label} for this connection"),
                        running,
                        move |value: &SharedString, _, cx| {
                            entity.update(cx, |this, cx| {
                                this.set_sync_choice(key, value.as_ref(), cx)
                            });
                        },
                    ))
                    .into_any_element(),
                help,
                None,
            )
        }
        _ => {
            let Some(input) = this.sync_inputs.get(def.key).cloned() else {
                return div().into_any_element();
            };
            let edited = input.read(cx).text().to_string();
            let error =
                inspector::number_error(def.kind, &edited).map(|text| format!("{label}: {text}"));
            stacked_field(
                label,
                unit,
                ns_ui::text_field(
                    SharedString::from(format!("sync-field-{}", def.key)),
                    &input,
                )
                .into_any_element(),
                help,
                error,
            )
        }
    }
}

fn path_and_browse(
    id: &'static str,
    input: &gpui::Entity<ely_gpui_component::forms::TextInput>,
    browse: AnyElement,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .child(ns_ui::text_field(id, input)),
        )
        .child(div().flex_shrink_0().child(browse))
        .into_any_element()
}

fn section_block(
    this: &mut GnDriveWindow,
    section: Section,
    cx: &mut Context<GnDriveWindow>,
    contents: impl FnOnce(&mut GnDriveWindow, &mut Context<GnDriveWindow>) -> AnyElement,
) -> AnyElement {
    let open = this.section_open(section);
    let theme = theme();
    let header = section_toggle(
        section,
        open,
        cx.listener(move |this, _, _, cx| this.toggle_section(section, cx)),
    );
    let mut block = div().w_full().flex().flex_col().gap_2();
    if section != Section::Basics {
        block = block.pt_2().border_t_1().border_color(theme.border);
    }
    block = block.child(header);
    if open {
        block = block.child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_2()
                .px_1()
                .child(contents(this, cx)),
        );
    }
    block.into_any_element()
}

fn section_toggle(
    section: Section,
    open: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let theme = theme();
    let chevron = if open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    let title = inspector::section_title(section);
    let row = div()
        .id(inspector::section_dom_id(section))
        .role(Role::Button)
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .px_1()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .hover(move |style| style.bg(theme.hover));
    // tip_on sits on the click so the chevron explains Show/Hide.
    tip_on(row, inspector::section_tip(section, open), None)
        .on_click(on_click)
        .child(Icon::new(chevron).size(IconSize::Xs).color(theme.muted))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .whitespace_normal()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.muted)
                .child(SharedString::from(title.to_string())),
        )
        .into_any_element()
}

/// Label column of a toggle row. Width comes from [`inspector::toggle_row_style`]:
/// grow, min width 0, and word wrap, so the words never stack one letter per line.
fn style_toggle_label(mut column: gpui::Div) -> gpui::Div {
    let style = inspector::toggle_row_style();
    if style.label_grows {
        column = column.flex_1();
    }
    if style.label_min_width_zero {
        column = column.min_w(px(0.));
    }
    if style.label_wraps_words {
        column = column.whitespace_normal();
    }
    column
}

fn style_switch_slot(mut slot: gpui::Div) -> gpui::Div {
    if inspector::toggle_row_style().switch_does_not_shrink {
        slot = slot.flex_shrink_0();
    }
    slot
}

fn toggle_row(label: &str, help: &str, switch: AnyElement) -> AnyElement {
    let theme = theme();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .child(
            style_toggle_label(div())
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .w_full()
                        .min_w(px(0.))
                        .whitespace_normal()
                        .text_sm()
                        .text_color(theme.text)
                        .child(SharedString::from(label.to_string())),
                )
                .when(!help.is_empty(), |column| column.child(prose(help))),
        )
        .child(style_switch_slot(div()).child(switch))
        .into_any_element()
}

fn stacked_field(
    label: &str,
    unit: &str,
    control: AnyElement,
    help: &str,
    error: Option<String>,
) -> AnyElement {
    let mut column = div()
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .child(field_label_row(label, unit))
        .child(control);
    if !help.is_empty() {
        column = column.child(prose(help));
    }
    if let Some(error) = error {
        column = column.child(crate::text::wrap_err(error));
    }
    column.into_any_element()
}

fn field_label_row(text: &str, unit: &str) -> AnyElement {
    let theme = theme();
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .whitespace_normal()
                .text_xs()
                .text_color(theme.muted)
                .child(SharedString::from(text.to_string())),
        )
        .when(!unit.is_empty(), |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(theme.muted)
                    .child(SharedString::from(unit.to_string())),
            )
        })
        .into_any_element()
}

fn quiet_heading(text: &str) -> AnyElement {
    let theme = theme();
    div()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme.muted)
        .child(SharedString::from(text.to_string()))
        .into_any_element()
}

struct EdgePalette {
    idle: Hsla,
    running: Hsla,
    ok: Hsla,
    failed: Hsla,
    warn: Hsla,
    checking: Hsla,
}

fn edge_color(state: &str, selected: bool, palette: &EdgePalette) -> Hsla {
    match state {
        "running" | "checking" => palette.running,
        "completed" => palette.ok,
        "failed" => palette.failed,
        "cancelled" | "cancelling" => palette.warn,
        _ if selected => palette.running,
        _ => palette.idle,
    }
}

fn dot_color(kind: &str, palette: &EdgePalette) -> Hsla {
    match kind {
        "failed" => palette.failed,
        "checking" => palette.checking,
        "completed" => palette.ok,
        _ => palette.running,
    }
}

/// Everything the edge painter needs besides the window.
struct CurvePaint<'a> {
    origin: gpui::Point<gpui::Pixels>,
    edges: &'a [GraphEdge],
    frames: &'a HashMap<String, graph::NodeRect>,
    progress: &'a HashMap<String, Rc<SyncProgress>>,
    /// Cached [`crate::format::file_dots`] per edge id, prebuilt with the
    /// sync cache so the painter never sorts transfers.
    dots: &'a HashMap<String, Rc<Vec<FileDot>>>,
    selected: Option<&'a str>,
    palette: &'a EdgePalette,
}

/// Cubic edges, arrowheads, and the live file dots.
fn paint_curves(window: &mut Window, paint: &CurvePaint<'_>) {
    let origin = paint.origin;
    let edges = paint.edges;
    let frames = paint.frames;
    let progress = paint.progress;
    let selected = paint.selected;
    let palette = paint.palette;
    for edge in edges {
        let (Some(source), Some(target)) = (frames.get(&edge.source), frames.get(&edge.target))
        else {
            continue;
        };
        let from = source.right_center();
        let to = target.left_center();
        let (c1, c2) = graph::edge_controls(from, to);
        let state = progress
            .get(&edge.id)
            .map(|item| item.state.as_str())
            .unwrap_or("idle");
        let selected_edge = selected == Some(edge.id.as_str());
        let color = edge_color(state, selected_edge, palette);
        let width = if selected_edge || state == "running" {
            2.0
        } else {
            1.25
        };
        let at = |p: (f32, f32)| point(origin.x + px(p.0), origin.y + px(p.1));
        let mut path = gpui::PathBuilder::stroke(px(width));
        path.move_to(at(from));
        path.cubic_bezier_to(at(to), at(c1), at(c2));
        if let Ok(built) = path.build() {
            window.paint_path(built, color);
        }
        paint_arrow(window, origin, to, c2, color);
        if matches!(edge.action.as_str(), "bi" | "bi-resync") {
            paint_arrow(window, origin, from, c1, color);
        }
        if let Some(dots) = paint.dots.get(&edge.id) {
            for dot in dots.iter() {
                let p = graph::cubic_at(dot.t, from, c1, c2, to);
                window.paint_quad(
                    gpui::fill(
                        Bounds {
                            origin: point(origin.x + px(p.0 - 3.5), origin.y + px(p.1 - 3.5)),
                            size: size(px(7.), px(7.)),
                        },
                        dot_color(dot.kind, palette),
                    )
                    .corner_radii(Corners::all(px(3.5))),
                );
            }
        }
    }
}

fn canvas_help() -> AnyElement {
    let theme = theme();
    tip_on(
        div()
            .id("canvas-help")
            .size(px(22.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .text_xs()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(theme.muted)
            .cursor_pointer()
            .hover(|style| style.bg(theme.hover).text_color(theme.text))
            .child("?"),
        CANVAS_HELP,
        None,
    )
    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    .into_any_element()
}

/// Hit target between the canvas and the inspector. Dragging right shrinks
/// the inspector. A double-click restores the preferred width. Left and right
/// arrows do the same step while the handle has keyboard focus.
fn inspector_divider(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let theme = theme();
    let active = this.inspector_drag.is_some();
    tip_on(
        div()
            .id("inspector-resize")
            .w(px(8.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .cursor_col_resize()
            .tab_index(0)
            .border_1()
            .border_color(gpui::transparent_black())
            .focus_visible(|style| style.border_color(ns_ui::theme().accent))
            .hover(|style| style.bg(theme.hover)),
        "Drag to resize. Double-click to reset.",
        None,
    )
    .on_mouse_down(
        MouseButton::Left,
        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
            cx.stop_propagation();
            end_sidebar_drag(this);
            let editor = ns_ui::body_width();
            if event.click_count >= 2 {
                let width = inspector::clamp_inspector_width(inspector::WIDTH, editor);
                this.inspector_width = width;
                this.inspector_drag = None;
                crate::prefs::set_inspector_width(width);
                cx.notify();
                return;
            }
            this.inspector_drag = Some(crate::InspectorDrag {
                origin_x: f32::from(event.position.x),
                origin_width: this.effective_inspector_width(),
                editor_width: editor,
            });
            cx.notify();
        }),
    )
    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
        let Some(delta) = resize_key_delta(event) else {
            return;
        };
        cx.stop_propagation();
        let editor = ns_ui::body_width();
        let width =
            inspector::drag_inspector_width(this.effective_inspector_width(), delta, editor);
        this.inspector_width = width;
        this.inspector_drag = None;
        crate::prefs::set_inspector_width(width);
        cx.notify();
    }))
    .child(div().w(px(1.)).h(px(36.)).rounded_full().bg(if active {
        theme.accent
    } else {
        theme.border
    }))
    .into_any_element()
}

/// Hit target on the flow list's right edge. Dragging right grows the list.
/// A double-click restores the preferred width. Compact windows do not draw it.
fn sidebar_divider(this: &mut GnDriveWindow, cx: &mut Context<GnDriveWindow>) -> AnyElement {
    let theme = theme();
    let active = this.sidebar_drag.is_some();
    tip_on(
        div()
            .id("sidebar-resize")
            .w(px(8.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .cursor_col_resize()
            .tab_index(0)
            .border_1()
            .border_color(gpui::transparent_black())
            .focus_visible(|style| style.border_color(ns_ui::theme().accent))
            .hover(|style| style.bg(theme.hover)),
        "Drag to resize. Double-click to reset.",
        None,
    )
    .on_mouse_down(
        MouseButton::Left,
        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
            cx.stop_propagation();
            end_inspector_drag(this);
            let window_width = ns_ui::viewport_width();
            if event.click_count >= 2 {
                let width = clamp_sidebar_width(SIDEBAR_WIDTH, window_width);
                this.sidebar_width = width;
                this.sidebar_drag = None;
                crate::prefs::set_sidebar_width(width);
                cx.notify();
                return;
            }
            this.sidebar_drag = Some(crate::SidebarDrag {
                origin_x: f32::from(event.position.x),
                origin_width: this.effective_sidebar_width(),
                window_width,
            });
            cx.notify();
        }),
    )
    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
        let Some(delta) = resize_key_delta(event) else {
            return;
        };
        cx.stop_propagation();
        let window_width = ns_ui::viewport_width();
        let width = drag_sidebar_width(this.effective_sidebar_width(), delta, window_width);
        this.sidebar_width = width;
        this.sidebar_drag = None;
        crate::prefs::set_sidebar_width(width);
        cx.notify();
    }))
    .child(div().w(px(1.)).h(px(36.)).rounded_full().bg(if active {
        theme.accent
    } else {
        theme.border
    }))
    .into_any_element()
}

/// Covers the window while the divider is dragged, so the pointer is not lost
/// when it leaves the 8px handle. The canvas uses the same mouse listeners.
pub(crate) fn inspector_drag_overlay(
    _this: &mut GnDriveWindow,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    div()
        .id("inspector-drag")
        .absolute()
        .inset_0()
        .occlude()
        .cursor_col_resize()
        .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
            let Some((origin_x, origin_width, editor_width)) = this
                .inspector_drag
                .as_ref()
                .map(|drag| (drag.origin_x, drag.origin_width, drag.editor_width))
            else {
                return;
            };
            let delta = f32::from(event.position.x) - origin_x;
            this.inspector_width =
                inspector::drag_inspector_width(origin_width, delta, editor_width);
            cx.notify();
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _: &gpui::MouseUpEvent, _, cx| {
                if this.inspector_drag.take().is_some() {
                    crate::prefs::set_inspector_width(this.inspector_width);
                }
                cx.notify();
            }),
        )
        .into_any_element()
}

/// Covers the window while the flow-list divider is dragged.
pub(crate) fn sidebar_drag_overlay(
    _this: &mut GnDriveWindow,
    cx: &mut Context<GnDriveWindow>,
) -> AnyElement {
    div()
        .id("sidebar-drag")
        .absolute()
        .inset_0()
        .occlude()
        .cursor_col_resize()
        .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
            let Some((origin_x, origin_width, window_width)) = this
                .sidebar_drag
                .as_ref()
                .map(|drag| (drag.origin_x, drag.origin_width, drag.window_width))
            else {
                return;
            };
            let delta = f32::from(event.position.x) - origin_x;
            this.sidebar_width = drag_sidebar_width(origin_width, delta, window_width);
            cx.notify();
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _: &gpui::MouseUpEvent, _, cx| {
                if this.sidebar_drag.take().is_some() {
                    crate::prefs::set_sidebar_width(this.sidebar_width);
                }
                cx.notify();
            }),
        )
        .into_any_element()
}

fn connection_sentence(this: &GnDriveWindow, edge: &GraphEdge) -> String {
    let place = |id: &str| {
        this.node(id)
            .map(|node| node_place(this, node))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "the folder".into())
    };
    connection_summary(&edge.action, &place(&edge.source), &place(&edge.target))
}

fn paint_arrow(
    window: &mut Window,
    origin: gpui::Point<gpui::Pixels>,
    tip: (f32, f32),
    from: (f32, f32),
    color: Hsla,
) {
    let dx = tip.0 - from.0;
    let dy = tip.1 - from.1;
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let ux = dx / len;
    let uy = dy / len;
    let px_n = -uy;
    let py_n = ux;
    let back = (tip.0 - ux * 9.0, tip.1 - uy * 9.0);
    let left = (back.0 + px_n * 4.5, back.1 + py_n * 4.5);
    let right = (back.0 - px_n * 4.5, back.1 - py_n * 4.5);
    let at = |p: (f32, f32)| point(origin.x + px(p.0), origin.y + px(p.1));
    let mut path = gpui::PathBuilder::fill();
    path.move_to(at(tip));
    path.line_to(at(left));
    path.line_to(at(right));
    path.close();
    if let Ok(built) = path.build() {
        window.paint_path(built, color);
    }
}

/// Hit-test an edge in canvas-local pixels. The nearest curve within 10px wins.
/// The node card under a canvas-local point, topmost first.
pub(crate) fn hit_node(this: &GnDriveWindow, x: f32, y: f32) -> Option<String> {
    let graph = this.graph.as_ref()?;
    graph.nodes.iter().rev().find_map(|node| {
        let heading = node_heading(this, node);
        let (rect, _, _) = placed_node(this, node, &heading);
        graph::rect_contains(rect, (x, y)).then(|| node.id.clone())
    })
}

pub(crate) fn hit_edge(this: &GnDriveWindow, x: f32, y: f32) -> Option<String> {
    let graph = this.graph.as_ref()?;
    for edge in graph.edges.iter().rev() {
        let Some(source) = graph.nodes.iter().find(|node| node.id == edge.source) else {
            continue;
        };
        let Some(target) = graph.nodes.iter().find(|node| node.id == edge.target) else {
            continue;
        };
        let source_heading = node_heading(this, source);
        let (source, _, _) = placed_node(this, source, &source_heading);
        let target_heading = node_heading(this, target);
        let (target, _, _) = placed_node(this, target, &target_heading);
        let from = source.right_center();
        let to = target.left_center();
        if graph::curve_hits(from, to, (x, y)) {
            return Some(edge.id.clone());
        }
    }
    None
}

#[cfg(test)]
mod inspector_layout_tests {
    /// A real GPUI layout pass needs a window (`VisualTestContext`). This crate
    /// does not ship that harness, and this change must not open the app.
    /// The row builder applies `toggle_row_style()` with the width rules below,
    /// which is what keeps a toggle label from collapsing to one glyph per line.
    #[test]
    fn toggle_row_builder_uses_flex_1_and_min_w_0() {
        let src = include_str!("workspace.rs");
        let start = src
            .find("fn style_toggle_label")
            .expect("toggle label builder");
        let end = src[start..]
            .find("fn style_switch_slot")
            .expect("switch slot builder");
        let label = &src[start..start + end];
        assert!(label.contains("flex_1()"), "{label}");
        assert!(label.contains("min_w(px(0.))"), "{label}");
        assert!(label.contains("whitespace_normal()"), "{label}");
        let switch_at = start + end;
        let switch_end = src[switch_at..].find("fn toggle_row").expect("toggle row");
        let switch = &src[switch_at..switch_at + switch_end];
        assert!(switch.contains("flex_shrink_0()"), "{switch}");
        let style = crate::inspector::toggle_row_style();
        assert!(style.label_grows && style.label_min_width_zero && style.label_wraps_words);
        assert!(style.switch_does_not_shrink);
    }

    #[test]
    fn save_lives_only_in_the_inspector_footer() {
        let src = include_str!("workspace.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        assert!(!prod.contains("tb-save"), "toolbar must not draw Save");
        assert!(!prod.contains("Save •"), "toolbar must not draw Save");
        assert!(src.contains("ins-save"));
        assert!(src.contains("UNSAVED_DOT_TIP"));
        let footer = src.split("fn inspector_footer").nth(1).expect("footer");
        let footer = footer.split("fn empty_inspector").next().unwrap();
        assert_eq!(footer.matches("Some(\"⌘S\")").count(), 3);
        let main = include_str!("main.rs");
        assert!(main.contains("KeyBinding::new(\"cmd-s\", SaveFlow, None)"));
        assert!(main.contains("[\"add-folder\", \"stop\"]"));
        assert!(main.contains("[\"add-folder\", \"run\"]"));
        let card = src.split("fn edge_card").nth(1).expect("edge card");
        let card = card.split("fn chip_progress_bar").next().unwrap();
        assert!(card.contains("text_xs()"));
        assert!(card.contains("ChipMode::IconOnly"));
        assert!(card.contains("chip_progress_bar"));
        assert!(!card.contains('…'));
    }

    #[test]
    fn node_cards_stay_readable_when_zoomed_out() {
        let src = include_str!("workspace.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        let face = prod.split("fn node_face").nth(1).expect("node face");
        let face = face.split("fn node_status").next().unwrap();
        assert!(face.contains("NodeMode::IconOnly"), "{face}");
        assert!(face.contains("NodeMode::Compact"), "{face}");
        assert!(face.contains("text_xs()"), "{face}");
        assert!(face.contains("node_caption"), "{face}");
        assert!(face.contains("NODE_COMPACT_PAD_RIGHT"), "{face}");
        assert!(prod.contains("icon_only_marks"), "{prod}");
        assert!(prod.contains("theme.editor"), "{prod}");
        assert!(
            !face.contains('…'),
            "the card must not hard-code a bare ellipsis"
        );
        let paint = prod.split("fn paint_curves").nth(1).expect("paint");
        let paint = paint.split("fn paint_arrow").next().unwrap();
        assert!(paint.contains("right_center()"));
        assert!(paint.contains("left_center()"));
        assert!(!paint.contains("NODE_W * zoom"));
        assert!(prod.contains("placed_node"));
        assert!(prod.contains("NODE_HANDLE"));
        assert!(prod.contains("rendered_node_rect"));
    }

    #[test]
    fn help_lives_on_the_zoom_control_and_the_divider_can_be_dragged() {
        let src = include_str!("workspace.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        assert_eq!(
            prod.matches("Drag a node to move it").count(),
            1,
            "the gesture line is the ? tooltip, not a bar under the canvas"
        );
        assert!(prod.contains("id(\"canvas-help\")"));
        assert!(prod.contains("Add a folder to start"));
        assert!(prod.contains("canvas-hint-dismiss"));
        assert!(prod.contains("id(\"inspector-resize\")"));
        assert!(prod.contains("Drag to resize"));
        assert!(prod.contains("cursor_col_resize()"));
        assert!(prod.contains("click_count >= 2"));
        assert!(prod.contains("fn inspector_drag_overlay"));
        assert!(prod.contains("Look::OutlineDanger"));
        assert!(prod.contains("Delete connection"));
        assert!(prod.contains("connection_sentence"));
        assert!(prod.contains("focus_visible("));
        assert!(prod.contains("id(\"sidebar-resize\")"));
        assert!(prod.contains("fn sidebar_drag_overlay"));
    }

    #[test]
    fn sidebar_width_clamps_and_drag_grows_to_the_right() {
        use super::{SIDEBAR_MIN, SIDEBAR_WIDTH, clamp_sidebar_width, drag_sidebar_width};
        assert!((clamp_sidebar_width(260.0, 1200.0) - SIDEBAR_WIDTH).abs() < 0.01);
        assert!((clamp_sidebar_width(100.0, 1200.0) - SIDEBAR_MIN).abs() < 0.01);
        assert!((clamp_sidebar_width(900.0, 1000.0) - 400.0).abs() < 0.01);
        assert!((clamp_sidebar_width(241.4, 1200.0) - 241.0).abs() < 0.01);
        assert!((clamp_sidebar_width(f32::NAN, 1000.0) - SIDEBAR_WIDTH).abs() < 0.01);
        // 40% of 390 is 156, so the floor is the minimum.
        assert!((clamp_sidebar_width(260.0, 390.0) - SIDEBAR_MIN).abs() < 0.01);
        // 40% of 600 is 240.
        assert!((clamp_sidebar_width(260.0, 600.0) - 240.0).abs() < 0.01);
        assert!((clamp_sidebar_width(900.0, f32::NAN) - SIDEBAR_WIDTH * 2.0).abs() < 0.01);
        assert!((drag_sidebar_width(260.0, 40.0, 1200.0) - 300.0).abs() < 0.01);
        assert!((drag_sidebar_width(260.0, -80.0, 1200.0) - SIDEBAR_MIN).abs() < 0.01);
        // The same rightward step grows the list and shrinks the inspector.
        assert!((drag_sidebar_width(260.0, 16.0, 1200.0) - 276.0).abs() < 0.01);
        assert!((crate::inspector::drag_inspector_width(320.0, 16.0, 1200.0) - 304.0).abs() < 0.01);
        assert!((drag_sidebar_width(260.0, f32::NAN, 1200.0) - 260.0).abs() < 0.01);
        assert_eq!(SIDEBAR_WIDTH, crate::prefs::DEFAULT_SIDEBAR_WIDTH);
    }

    #[test]
    fn connect_ring_drags_and_drop_target_is_hit_tested() {
        let src = include_str!("workspace.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        let ring = prod
            .split("format!(\"connect-{}\", node.id)")
            .nth(1)
            .expect("connect ring");
        let ring = &ring[..ring.find("stacked.push").expect("ring end")];
        assert!(ring.contains("begin_connect_drag"), "press arms a drag");
        assert!(
            ring.contains("start_connect"),
            "a click still picks the source"
        );
        assert!(ring.contains(".tooltip("), "ring keeps a tooltip");
        let main = include_str!("main.rs");
        let up = main
            .split("pub(crate) fn canvas_up")
            .nth(1)
            .expect("canvas_up");
        let up = &up[..up.find("let dragged").expect("node drag branch")];
        assert!(up.contains("workspace::hit_node"));
        assert!(up.contains("connect_nodes"));
        assert!(up.contains("SelfLoop"));
        assert!(main.contains("pressed_button != Some(MouseButton::Left)"));
        assert!(prod.contains("rubber"));
    }

    #[test]
    fn close_hides_with_minimize_to_tray_and_toasts_last_four_seconds() {
        let main = include_str!("main.rs");
        let prod = main.split("#[cfg(test)]").next().unwrap();
        assert!(prod.contains("on_window_should_close"));
        assert!(prod.contains("minimize_to_tray"));
        assert!(prod.contains("cx.hide()"));
        assert_eq!(crate::TOAST_STAY, std::time::Duration::from_secs(4));
        assert!(prod.contains("ns_ui::notify_for(&self.toaster, severity, text, TOAST_STAY, cx)"));
        assert!(!prod.contains("ns_ui::notify(&self.toaster"));
    }

    #[test]
    fn compact_flow_list_keeps_named_account_and_settings_rows() {
        assert_eq!(
            super::SIDEBAR_NAV
                .iter()
                .map(|row| row.label)
                .collect::<Vec<_>>(),
            vec!["Cloud accounts", "Settings"]
        );
        let src = include_str!("workspace.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        let rail = prod
            .split("pub(crate) fn flow_rail")
            .nth(1)
            .expect("flow rail");
        let rail = rail.split("/// One sidebar destination").next().unwrap();
        let nav_at = rail.find("SIDEBAR_NAV").expect("shared nav rows");
        let gate = rail.find("is_compact()").expect("compact gate");
        assert!(
            nav_at < gate,
            "named rows are built before the compact gate drops the divider"
        );
        let before = &rail[..gate];
        assert_eq!(before.matches("page_nav(").count(), 2);
        assert!(before.contains("mt_auto()"));
        assert!(!before.contains("sidebar_divider"));
        assert!(rail[gate..].contains("sidebar_divider"));
        let main = include_str!("main.rs");
        assert!(main.contains("graph::choose_fit_zoom"));
        assert!(main.contains("chrome_full_height_sized"));
        assert!(main.contains("\"sidebar\""));
    }
}
