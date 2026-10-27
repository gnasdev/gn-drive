//! GN Drive's window. The view and the inspector draft live here. The engine
//! runs in-process when the `core` feature is on (`GN_DRIVE_MOCK=1` uses the
//! mock; `GN_DRIVE_BACKEND=stdio` is the child-process fallback).

mod cache;
mod engine;
mod format;
mod graph;
mod inspector;
#[cfg(any(feature = "devtools", test))]
mod mock;
mod overlays;
mod pages;
#[cfg(feature = "devtools")]
mod perf;
mod prefs;
mod remotes;
mod settings;
mod states;
mod sync_config;
mod text;
mod tip;
mod unlock;
mod workspace;

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use ely_gpui_component::feedback::{ToastViewport, Toaster};
use ely_gpui_component::forms::{InputEvent, TextInput};
use ely_gpui_component::primitives::Severity;
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, KeyBinding, Menu, MenuItem, MouseButton, Render,
    ScrollHandle, Subscription, Window, div, prelude::*, px,
};
use ns_ui::{Ask, Tone, chip, settings_rail, theme};

use crate::tip::{Look, tip_button, tip_icon};
use serde::Deserialize;
use serde_json::{Value, json};

use engine::{
    AppSettings, AuthStatus, DoctorReport, Engine, FileEntry, Flow, Frame, HealthStatus,
    HistoryEntry, LinkStatus, LogsPage, Paths, Provider, RpcError, Runtime, RuntimeFlow,
    ServiceStatus, Snapshot, SyncProgress, UpdateCheck,
};
use graph::{FlowGraph, GraphEdge, GraphNode, NODE_H, NODE_W};
use sync_config::{FIELDS, FieldKind, SyncConfig};

gpui::actions!(
    gn_drive,
    [
        Quit,
        NewFlow,
        SaveFlow,
        RunFlow,
        StopFlow,
        ToggleInspector,
        ToggleRemotes,
        ToggleSettings,
        Lock,
        FindFlow,
        ToggleLocale,
        ToggleTheme,
        DeleteSelection,
        ZoomIn,
        ZoomOut,
        FitCanvas,
        CloseOverlay,
        SubmitEnter
    ]
);

/// What the inspector is looking at.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) enum Selection {
    #[default]
    Flow,
    Node(String),
    Edge(String),
    /// Canvas background. The inspector shows the empty state.
    None,
}

/// Which page is open in the editor. `Closed` is the flow workspace.
/// The inspector is part of that workspace, so a compact window can still
/// show the canvas and its details.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Panel {
    #[default]
    Closed,
    Remotes,
    Settings,
}

/// Opening the page that is already open returns to the flow workspace.
pub(crate) fn next_panel(current: Panel, target: Panel) -> Panel {
    if current == target {
        Panel::Closed
    } else {
        target
    }
}

/// Every GN Drive toast, success or failure, leaves after 4 seconds (UC-17).
/// The other GPUI apps keep the shared 5 s / 12 s default.
pub(crate) const TOAST_STAY: std::time::Duration = std::time::Duration::from_secs(4);

/// A flow row always leaves Cloud accounts and Settings.
pub(crate) fn panel_after_flow_pick(_panel: Panel) -> Panel {
    Panel::Closed
}

/// `remotes` or `settings` while that page is open. The flow workspace has none.
// Read by the dev channel and tests only.
#[cfg_attr(not(any(test, feature = "devtools")), allow(dead_code))]
pub(crate) fn page_key(panel: Panel) -> Option<&'static str> {
    match panel {
        Panel::Remotes => Some("remotes"),
        Panel::Settings => Some("settings"),
        Panel::Closed => None,
    }
}

/// One status-bar word. "Unlocked" is the same fact as Ready: the lock control
/// already says the vault is open, so it does not get a second message.
pub(crate) fn engine_status_word(
    locked: bool,
    busy: bool,
    cancelling: bool,
    link: &LinkStatus,
    remembered: &str,
) -> &'static str {
    if locked || remembered == "Locked" {
        return "Locked";
    }
    if cancelling {
        return "Stopping";
    }
    if busy {
        return "Syncing";
    }
    match link {
        LinkStatus::Starting => "Starting",
        LinkStatus::Ready => "Ready",
        LinkStatus::Fatal { .. } => "Error",
        LinkStatus::Restarting { .. } => "Reconnecting",
        LinkStatus::Stopped => "Stopped",
    }
}

/// A short version for the status bar, or nothing when the engine has not
/// reported one. "—" is the placeholder `version()` uses before a snapshot.
pub(crate) fn status_meta(version: &str, rclone: &str) -> String {
    let version = version.trim();
    let rclone = rclone.trim();
    let version = (version != "—" && !version.is_empty()).then_some(version);
    let rclone = (!rclone.is_empty()).then_some(rclone);
    let version = version.map(|version| {
        if version.starts_with('v') || version.starts_with('V') {
            version.to_string()
        } else {
            format!("v{version}")
        }
    });
    let rclone = rclone.map(|rclone| {
        if rclone.to_ascii_lowercase().starts_with("rclone") {
            rclone.to_string()
        } else {
            format!("rclone {rclone}")
        }
    });
    match (version, rclone) {
        (Some(version), Some(rclone)) => format!("{version} · {rclone}"),
        (Some(version), None) => version,
        (None, Some(rclone)) => rclone,
        (None, None) => String::new(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum SettingsTab {
    #[default]
    General,
    Service,
    History,
    Logs,
    About,
}

impl SettingsTab {
    #[cfg(feature = "devtools")]
    fn key(self) -> &'static str {
        match self {
            SettingsTab::General => "general",
            SettingsTab::Service => "service",
            SettingsTab::History => "history",
            SettingsTab::Logs => "logs",
            SettingsTab::About => "about",
        }
    }

    #[cfg(feature = "devtools")]
    fn from_key(key: &str) -> Self {
        match key {
            "service" => SettingsTab::Service,
            "history" => SettingsTab::History,
            "logs" | "diagnostics" => SettingsTab::Logs,
            "about" => SettingsTab::About,
            _ => SettingsTab::General,
        }
    }
}

/// The one modal the window owns. Confirmations live in `ask`.
#[derive(Clone, Default, PartialEq)]
pub(crate) enum Overlay {
    #[default]
    None,
    NewFlow,
    AddRemote,
    OAuth,
    Browse(BrowseTarget),
    ChangePassword,
    RemovePassword,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BrowseTarget {
    NodePath(String),
    EdgeSource(String),
    EdgeTarget(String),
}

/// The destructive or otherwise consequential action a confirmation asks
/// about. Kept until the user answers; nothing runs while the card is drawn.
#[derive(Clone, Debug)]
pub(crate) enum AskAction {
    DeleteFlow(String),
    DeleteRemote(String),
    /// Discard the current draft and open this flow.
    SwitchFlow {
        id: String,
    },
    ClearHistory,
    UninstallService,
    RemovePassword,
    ApplyUpdate,
    Lock,
}

/// What a request expects back. Responses arrive by id in `pump`; a request
/// that never answers is reported after `REQUEST_TIMEOUT`.
enum Pending {
    Snapshot,
    SaveFlow { then_run: bool, quiet: bool },
    Execute,
    Stop,
    CreateFlow,
    DeleteFlow,
    Providers,
    FsList,
    OAuthStart,
    OAuthComplete,
    RemoteAdd,
    RemoteTest { name: String },
    RemoteDelete { name: String },
    Auth,
    ChangePassword,
    RemovePassword,
    Doctor,
    Logs,
    Health,
    ServiceStatus,
    ServiceAction { label: &'static str },
    History,
    ClearHistory,
    SettingsSet,
    UpdateCheck,
    UpdateApply,
    Generic { label: String },
}

const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);
const RELOAD_DELAY: Duration = Duration::from_millis(120);
/// Quiet period after a keystroke before the flow rail refilters.
const SEARCH_DEBOUNCE_MS: u64 = 130;
const LAYOUT_SAVE_DELAY: Duration = Duration::from_millis(750);

/// A click on a flow row or a dev command picks a flow. Unsaved edits ask
/// before they are discarded. A busy flow refuses and keeps the current selection.
pub(crate) struct GnDriveWindow {
    pub(crate) engine: Engine,
    pub(crate) toaster: Entity<Toaster>,
    pub(crate) hello: Option<Value>,
    pub(crate) link: LinkStatus,
    pub(crate) snapshot: Option<Snapshot>,
    pub(crate) runtime: Runtime,
    /// Derived data shared with render through `Rc`; rebuilt only when its
    /// inputs change. See the [`cache`] module.
    pub(crate) flow_cache: cache::FlowListCache,
    pub(crate) remote_cache: cache::RemoteCache,
    pub(crate) runtime_cache: cache::RuntimeCache,
    pub(crate) graph_cache: Option<cache::GraphCache>,
    pub(crate) sync_cache: Option<cache::SyncCache>,
    /// Bumped whenever the runtime changes, so dependent caches rebuild.
    pub(crate) runtime_gen: u64,
    /// Bumped whenever the graph changes, so dependent caches rebuild.
    pub(crate) graph_gen: u64,
    /// Search text applied to the flow rail: the field's text, lowercased, held
    /// back by the search debounce. Enter applies it at once.
    pub(crate) rail_query: String,
    /// Bumped for every search change so only the latest debounce fires.
    pub(crate) search_job: u64,
    pub(crate) snapshot_pending: bool,
    pub(crate) reload_at: Option<Instant>,
    pub(crate) started: bool,
    pub(crate) load_error: Option<String>,
    pub(crate) pending: HashMap<u64, (Instant, Pending)>,

    pub(crate) selected_flow: Option<String>,
    pub(crate) draft: Option<Flow>,
    pub(crate) graph: Option<FlowGraph>,
    pub(crate) dirty: bool,
    pub(crate) selection: Selection,
    pub(crate) panel: Panel,
    pub(crate) settings_tab: SettingsTab,
    pub(crate) mobile_inspector: bool,
    pub(crate) status: String,
    pub(crate) busy: bool,

    pub(crate) canvas: CanvasState,
    pub(crate) connect_from: Option<String>,
    pub(crate) layout_save_at: Option<Instant>,
    pub(crate) layout_saving: bool,

    pub(crate) password_input: Entity<TextInput>,
    pub(crate) confirm_input: Entity<TextInput>,
    pub(crate) old_password_input: Entity<TextInput>,
    pub(crate) new_password_input: Entity<TextInput>,
    pub(crate) remove_password_input: Entity<TextInput>,
    /// Remotes whose last Test failed: only these offer Reconnect.
    pub(crate) remote_failed: HashSet<String>,
    /// Remotes whose last Test succeeded.
    pub(crate) remote_ok: HashSet<String>,
    /// Last test failure for a remote: the sentence, the code, and the raw text.
    pub(crate) remote_faults: HashMap<String, states::Fault>,
    /// Secret add-remote fields the user chose to reveal. Never logged.
    pub(crate) revealed_secrets: HashSet<String>,
    pub(crate) add_busy: bool,
    pub(crate) add_error: Option<String>,
    /// One authorize at a time. A failure never starts another by itself.
    pub(crate) oauth_busy: bool,
    /// The user closed the file card for the current edge.
    pub(crate) card_dismissed: bool,
    /// Edge that was auto-opened for a live run, so a later tick does not reopen it.
    pub(crate) auto_opened: Option<String>,
    pub(crate) flow_menu: Option<String>,
    /// Pan or zoom changed the viewport. Saved on its own, without the Unsaved chip.
    pub(crate) viewport_dirty: bool,
    pub(crate) search_open: bool,
    pub(crate) search_input: Entity<TextInput>,
    /// Logs tab: show warn/error lines only.
    pub(crate) log_errors_only: bool,
    pub(crate) auth_busy: bool,
    /// One update check at a time. The About button hides while this is set.
    pub(crate) update_busy: bool,
    /// Plain-English auth failure, shown under the password field.
    pub(crate) auth_error: Option<String>,
    /// Engine code for that failure. The Details tooltip shows it.
    pub(crate) auth_error_code: Option<String>,
    /// The wrong-password preview set this error. `reset` clears only that preview.
    pub(crate) auth_error_simulated: bool,
    pub(crate) password_error: Option<String>,
    pub(crate) lockout_until: Option<Instant>,
    pub(crate) last_lock_second: u64,

    pub(crate) overlay: Overlay,
    pub(crate) ask: Option<(Ask, AskAction)>,
    pub(crate) new_flow_input: Entity<TextInput>,

    pub(crate) providers: Vec<Provider>,
    pub(crate) provider_rclone: String,
    pub(crate) add_name_input: Entity<TextInput>,
    pub(crate) add_provider: Option<String>,
    pub(crate) add_options: Vec<engine::ProviderOption>,
    pub(crate) add_fields: HashMap<String, Entity<TextInput>>,
    pub(crate) oauth_url: String,
    pub(crate) oauth_session: String,
    pub(crate) oauth_type: String,
    pub(crate) oauth_status: String,

    pub(crate) browse_path: String,
    pub(crate) browse_entries: Vec<FileEntry>,
    pub(crate) browse_error: Option<String>,
    pub(crate) browse_loading: bool,
    pub(crate) browse_input: Entity<TextInput>,

    pub(crate) service: Option<ServiceStatus>,
    pub(crate) health: Option<HealthStatus>,
    pub(crate) doctor: Option<DoctorReport>,
    pub(crate) logs: Option<LogsPage>,
    pub(crate) history: Vec<HistoryEntry>,
    pub(crate) update: Option<UpdateCheck>,
    pub(crate) update_text: String,
    pub(crate) update_url: String,
    pub(crate) settings_error: Option<String>,
    /// History has received at least one successful reply. Until then, empty is loading.
    pub(crate) history_loaded: bool,
    pub(crate) history_error: Option<String>,
    pub(crate) history_error_code: Option<String>,
    pub(crate) logs_error: Option<String>,
    pub(crate) logs_error_code: Option<String>,
    /// Mock-only preview. `none` leaves the real snapshot on screen.
    pub(crate) simulation: states::Simulation,

    pub(crate) flow_name_input: Entity<TextInput>,
    pub(crate) cron_input: Entity<TextInput>,
    pub(crate) node_label_input: Entity<TextInput>,
    pub(crate) node_path_input: Entity<TextInput>,
    pub(crate) edge_source_input: Entity<TextInput>,
    pub(crate) edge_target_input: Entity<TextInput>,
    pub(crate) sync_inputs: HashMap<&'static str, Entity<TextInput>>,
    /// Subscriptions for the sync text fields. Cleared when those fields are rebuilt.
    pub(crate) sync_input_subs: Vec<Subscription>,
    pub(crate) sync_edge: Option<String>,
    pub(crate) sync_config: Option<SyncConfig>,
    /// Open or closed inspector sections. Missing means the section default
    /// (Advanced starts closed).
    pub(crate) inspector_open: HashMap<crate::inspector::Section, bool>,
    /// Scroll offset of the inspector body (`id="inspector-scroll"`).
    pub(crate) inspector_scroll: ScrollHandle,
    /// Inspector width in points, before the editor-width clamp.
    pub(crate) inspector_width: f32,
    /// Set while the divider between the canvas and the inspector is dragged.
    pub(crate) inspector_drag: Option<InspectorDrag>,
    /// Flow-list width in points, before the window-width clamp.
    pub(crate) sidebar_width: f32,
    /// Set while the divider on the flow list's right edge is dragged.
    pub(crate) sidebar_drag: Option<SidebarDrag>,
    /// The empty-canvas hint was dismissed. Remembered in window prefs.
    pub(crate) canvas_hint_dismissed: bool,
    pub(crate) form_flow: Option<String>,
    pub(crate) form_selection: Option<Selection>,
    pub(crate) updating_fields: bool,
    pub(crate) inputs_locked: bool,

    pub(crate) canvas_focus: FocusHandle,
    #[allow(dead_code)]
    pub(crate) subs: Vec<Subscription>,
    #[cfg(feature = "devtools")]
    pub(crate) dev_key: Option<String>,
    #[cfg(feature = "devtools")]
    pub(crate) perf_list_scroll: ScrollHandle,
}

/// The canvas view: where the graph is scrolled and zoomed, and the gesture
/// the mouse is in the middle of.
pub(crate) struct CanvasState {
    pub(crate) pan_x: f32,
    pub(crate) pan_y: f32,
    pub(crate) zoom: f32,
    pub(crate) size: Rc<Cell<(f32, f32)>>,
    pub(crate) origin: Rc<Cell<(f32, f32)>>,
    pub(crate) panning: Option<(f32, f32)>,
    pub(crate) dragging: Option<NodeDrag>,
    /// A drag that started on a node's connect ring (UC-57).
    pub(crate) connect_drag: Option<ConnectDrag>,
    pub(crate) fit_pending: bool,
    pub(crate) pan_moved: bool,
    /// Canvas size at the last automatic fit. While set, a resize refits;
    /// any pan, zoom, or drag by the user clears it.
    pub(crate) auto_fit_size: Option<(f32, f32)>,
}

/// Drag from a connect ring. `pointer` is in canvas-local points. A press
/// that never moves 4 points stays a click, which keeps click-then-click.
pub(crate) struct ConnectDrag {
    pub(crate) start_mouse: (f32, f32),
    pub(crate) pointer: (f32, f32),
    pub(crate) moved: bool,
}

/// Points a press must travel before a ring press counts as a drag.
pub(crate) const CONNECT_DRAG_SLOP: f32 = 4.0;

pub(crate) struct NodeDrag {
    pub(crate) id: String,
    pub(crate) start_mouse: (f32, f32),
    pub(crate) start_node: (f32, f32),
    pub(crate) moved: bool,
}

/// Divider drag. `editor_width` is the editor column at mouse-down, so the
/// half-width cap stays put for the gesture.
pub(crate) struct InspectorDrag {
    pub(crate) origin_x: f32,
    pub(crate) origin_width: f32,
    pub(crate) editor_width: f32,
}

/// Flow-list divider drag. `window_width` is the window at mouse-down, so the
/// 40% cap stays put for the gesture.
pub(crate) struct SidebarDrag {
    pub(crate) origin_x: f32,
    pub(crate) origin_width: f32,
    pub(crate) window_width: f32,
}

impl CanvasState {
    fn new() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            size: Rc::new(Cell::new((0.0, 0.0))),
            origin: Rc::new(Cell::new((0.0, 0.0))),
            panning: None,
            dragging: None,
            connect_drag: None,
            fit_pending: true,
            pan_moved: false,
            auto_fit_size: None,
        }
    }

    pub(crate) fn screen(&self, x: f32, y: f32) -> (f32, f32) {
        ((x - self.pan_x) * self.zoom, (y - self.pan_y) * self.zoom)
    }

    pub(crate) fn graph_point(&self, local_x: f32, local_y: f32) -> (f32, f32) {
        (
            self.pan_x + local_x / self.zoom,
            self.pan_y + local_y / self.zoom,
        )
    }

    pub(crate) fn center_graph(&self) -> (f32, f32) {
        let (w, h) = self.size.get();
        self.graph_point(w / 2.0, h / 2.0)
    }

    pub(crate) fn set_zoom(&mut self, zoom: f32, local: Option<(f32, f32)>) {
        let zoom = zoom.clamp(0.25, 2.0);
        self.auto_fit_size = None;
        let (anchor_x, anchor_y) = local.unwrap_or_else(|| {
            let (w, h) = self.size.get();
            (w / 2.0, h / 2.0)
        });
        let before = self.graph_point(anchor_x, anchor_y);
        self.zoom = zoom;
        self.pan_x = before.0 - anchor_x / zoom;
        self.pan_y = before.1 - anchor_y / zoom;
    }

    pub(crate) fn zoom_by(&mut self, factor: f32, local: Option<(f32, f32)>) {
        self.set_zoom(self.zoom * factor, local);
    }

    pub(crate) fn fit(&mut self, nodes: &[GraphNode]) {
        let (w, h) = self.size.get();
        if w <= 0.0 || h <= 0.0 || nodes.is_empty() {
            return;
        }
        let min_x = nodes.iter().map(|n| n.x).fold(f32::INFINITY, f32::min);
        let min_y = nodes.iter().map(|n| n.y).fold(f32::INFINITY, f32::min);
        let max_x = nodes
            .iter()
            .map(|n| n.x + NODE_W)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = nodes
            .iter()
            .map(|n| n.y + NODE_H)
            .fold(f32::NEG_INFINITY, f32::max);
        let graph_w = (max_x - min_x).max(1.0);
        let graph_h = (max_y - min_y).max(1.0);
        // Compact windows stay at a readable floor. A graph that cannot fit
        // at that floor stays centered and the user pans.
        let zoom = graph::choose_fit_zoom(w, h, graph_w, graph_h, ns_ui::is_compact());
        self.zoom = zoom;
        self.pan_x = min_x - (w / zoom - graph_w) / 2.0;
        self.pan_y = min_y - (h / zoom - graph_h) / 2.0;
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProvidersResponse {
    #[serde(default)]
    rclone_version: String,
    #[serde(default)]
    providers: Vec<Provider>,
}

impl GnDriveWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let password_input =
            cx.new(|cx| TextInput::new(window, cx).masked().placeholder("Password"));
        let confirm_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked()
                .placeholder("Repeat password")
        });
        let old_password_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked()
                .placeholder("Current password")
        });
        let new_password_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked()
                .placeholder("New password")
        });
        let remove_password_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked()
                .placeholder("Current password")
        });
        let new_flow_input = cx.new(|cx| TextInput::new(window, cx).placeholder("Flow name"));
        let add_name_input =
            cx.new(|cx| TextInput::new(window, cx).placeholder("Remote name, e.g. gdrive"));
        let browse_input =
            cx.new(|cx| TextInput::new(window, cx).placeholder("remote:path or /absolute/path"));
        let flow_name_input = cx.new(|cx| TextInput::new(window, cx).placeholder("Flow name"));
        let cron_input = cx.new(|cx| TextInput::new(window, cx).placeholder("0 * * * *"));
        let node_label_input = cx.new(|cx| TextInput::new(window, cx).placeholder("Label"));
        let node_path_input =
            cx.new(|cx| TextInput::new(window, cx).placeholder("remote:path or /path"));
        let edge_source_input = cx.new(|cx| TextInput::new(window, cx).placeholder("source path"));
        let edge_target_input = cx.new(|cx| TextInput::new(window, cx).placeholder("target path"));
        let search_input = cx.new(|cx| TextInput::new(window, cx).placeholder("Find a flow"));

        let flow_sub = {
            let input = flow_name_input.clone();
            cx.subscribe_in(
                &flow_name_input,
                window,
                move |this: &mut Self, _, event: &InputEvent, _, cx| {
                    if !matches!(event, InputEvent::Changed) || this.updating_fields {
                        return;
                    }
                    let text = input.read(cx).text().to_string();
                    if let Some(draft) = &mut this.draft
                        && draft.name != text
                    {
                        draft.name = text;
                        this.dirty = true;
                        cx.notify();
                    }
                },
            )
        };
        let cron_sub = {
            let input = cron_input.clone();
            cx.subscribe_in(
                &cron_input,
                window,
                move |this: &mut Self, _, event: &InputEvent, _, cx| {
                    if !matches!(event, InputEvent::Changed) || this.updating_fields {
                        return;
                    }
                    let text = input.read(cx).text().to_string();
                    if let Some(draft) = &mut this.draft
                        && draft.schedule_cron != text
                    {
                        draft.schedule_cron = text;
                        this.dirty = true;
                        cx.notify();
                    }
                },
            )
        };
        let label_sub = {
            let input = node_label_input.clone();
            cx.subscribe_in(
                &node_label_input,
                window,
                move |this: &mut Self, _, event: &InputEvent, _, cx| {
                    if !matches!(event, InputEvent::Changed) || this.updating_fields {
                        return;
                    }
                    let Selection::Node(id) = this.selection.clone() else {
                        return;
                    };
                    let text = input.read(cx).text().to_string();
                    let Some(graph) = &mut this.graph else { return };
                    let Some(node) = graph.nodes.iter_mut().find(|node| node.id == id) else {
                        return;
                    };
                    if node.label != text {
                        node.label = text;
                        this.touch_graph();
                        this.sync_canvas_only();
                        this.dirty = true;
                        cx.notify();
                    }
                },
            )
        };

        let search_sub = cx.subscribe_in(
            &search_input,
            window,
            |this, _, event: &InputEvent, _, cx| {
                match event {
                    // The rail refilters after a quiet period, not per keystroke.
                    InputEvent::Changed => {
                        this.search_job += 1;
                        let job = this.search_job;
                        cx.spawn(async move |this, cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(SEARCH_DEBOUNCE_MS))
                                .await;
                            let _ = this.update(cx, |this, cx| {
                                if this.search_job == job {
                                    let text =
                                        this.search_input.read(cx).text().trim().to_lowercase();
                                    if text != this.rail_query {
                                        this.rail_query = text;
                                        cx.notify();
                                    }
                                }
                            });
                        })
                        .detach();
                    }
                    // Enter applies the search at once.
                    InputEvent::Submit => {
                        this.search_job += 1;
                        let text = this.search_input.read(cx).text().trim().to_lowercase();
                        if text != this.rail_query {
                            this.rail_query = text;
                        }
                        cx.notify();
                    }
                    _ => {}
                }
            },
        );
        // Path fields apply on Save. Notify so the save bar and validation
        // update on each keystroke.
        let node_path_sub = cx.subscribe_in(
            &node_path_input,
            window,
            |this: &mut Self, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Changed) && !this.updating_fields {
                    cx.notify();
                }
            },
        );
        let edge_source_sub = cx.subscribe_in(
            &edge_source_input,
            window,
            |this: &mut Self, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Changed) && !this.updating_fields {
                    cx.notify();
                }
            },
        );
        let edge_target_sub = cx.subscribe_in(
            &edge_target_input,
            window,
            |this: &mut Self, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Changed) && !this.updating_fields {
                    cx.notify();
                }
            },
        );
        let toaster = cx.new(|_| Toaster::default());
        let mut window_state = Self {
            engine: Self::start_engine(),
            toaster,
            hello: None,
            link: LinkStatus::Starting,
            snapshot: None,
            runtime: Runtime::default(),
            flow_cache: cache::FlowListCache::empty(),
            remote_cache: cache::RemoteCache::empty(),
            runtime_cache: cache::RuntimeCache::empty(),
            graph_cache: None,
            sync_cache: None,
            runtime_gen: 0,
            graph_gen: 0,
            rail_query: String::new(),
            search_job: 0,
            snapshot_pending: false,
            reload_at: None,
            started: false,
            load_error: None,
            pending: HashMap::new(),
            selected_flow: None,
            draft: None,
            graph: None,
            dirty: false,
            selection: Selection::Flow,
            panel: Panel::Closed,
            settings_tab: SettingsTab::General,
            mobile_inspector: false,
            status: "Connecting".into(),
            busy: false,
            canvas: CanvasState::new(),
            connect_from: None,
            layout_save_at: None,
            layout_saving: false,
            password_input,
            confirm_input,
            old_password_input,
            new_password_input,
            remove_password_input,
            remote_failed: HashSet::new(),
            remote_ok: HashSet::new(),
            remote_faults: HashMap::new(),
            revealed_secrets: HashSet::new(),
            add_busy: false,
            add_error: None,
            oauth_busy: false,
            card_dismissed: false,
            auto_opened: None,
            flow_menu: None,
            viewport_dirty: false,
            search_open: false,
            search_input,
            log_errors_only: false,
            auth_busy: false,
            update_busy: false,
            auth_error: None,
            auth_error_code: None,
            auth_error_simulated: false,
            password_error: None,
            lockout_until: None,
            last_lock_second: 0,
            overlay: Overlay::None,
            ask: None,
            new_flow_input,
            providers: Vec::new(),
            provider_rclone: String::new(),
            add_name_input,
            add_provider: None,
            add_options: Vec::new(),
            add_fields: HashMap::new(),
            oauth_url: String::new(),
            oauth_session: String::new(),
            oauth_type: String::new(),
            oauth_status: String::new(),
            browse_path: String::new(),
            browse_entries: Vec::new(),
            browse_error: None,
            browse_loading: false,
            browse_input,
            service: None,
            health: None,
            doctor: None,
            logs: None,
            history: Vec::new(),
            update: None,
            update_text: String::new(),
            update_url: String::new(),
            settings_error: None,
            history_loaded: false,
            history_error: None,
            history_error_code: None,
            logs_error: None,
            logs_error_code: None,
            simulation: states::Simulation::None,
            flow_name_input,
            cron_input,
            node_label_input,
            node_path_input,
            edge_source_input,
            edge_target_input,
            sync_inputs: HashMap::new(),
            sync_input_subs: Vec::new(),
            sync_edge: None,
            sync_config: None,
            inspector_open: HashMap::new(),
            inspector_scroll: ScrollHandle::new(),
            inspector_width: prefs::inspector_width(),
            inspector_drag: None,
            sidebar_width: prefs::sidebar_width(),
            sidebar_drag: None,
            canvas_hint_dismissed: prefs::canvas_hint_dismissed(),
            form_flow: None,
            form_selection: None,
            updating_fields: false,
            inputs_locked: false,
            canvas_focus: cx.focus_handle(),
            subs: vec![
                flow_sub,
                cron_sub,
                label_sub,
                search_sub,
                node_path_sub,
                edge_source_sub,
                edge_target_sub,
            ],
            #[cfg(feature = "devtools")]
            dev_key: None,
            #[cfg(feature = "devtools")]
            perf_list_scroll: ScrollHandle::new(),
        };
        window_state.install_actions(cx);
        window_state.spawn_tick(cx);
        // Minimize to tray (UC-144): the close button hides GN Drive instead of
        // closing it, so syncs keep running. The Dock icon brings it back and
        // Quit still exits. GPUI has no status-bar item, so there is no tray icon.
        let view = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            let hide = view
                .upgrade()
                .is_some_and(|view| view.read(cx).app_settings().minimize_to_tray);
            if hide {
                cx.hide();
            }
            !hide
        });
        window_state
    }

    fn start_engine() -> Engine {
        #[cfg(any(feature = "devtools", test))]
        let mock = std::env::var("GN_DRIVE_MOCK")
            .map(|value| value == "1")
            .unwrap_or(false);
        #[cfg(not(any(feature = "devtools", test)))]
        let mock = false;
        Engine::start(mock)
    }

    pub(crate) fn is_mock(&self) -> bool {
        self.engine.is_mock()
    }

    pub(crate) fn locked(&self) -> bool {
        self.snapshot
            .as_ref()
            .map(|snap| snap.locked)
            .unwrap_or(false)
    }

    pub(crate) fn auth(&self) -> AuthStatus {
        self.snapshot
            .as_ref()
            .map(|snap| snap.auth.clone())
            .unwrap_or_default()
    }

    /// The sorted flows, shared by `Rc`. A frame clones only the pointer; the
    /// deep copy and sort happen once per snapshot.
    pub(crate) fn flows(&self) -> Rc<Vec<Flow>> {
        self.flow_cache.flows.clone()
    }

    /// The remotes, shared by `Rc`; rebuilt with each snapshot.
    pub(crate) fn remotes(&self) -> Rc<Vec<engine::Remote>> {
        self.remote_cache.remotes.clone()
    }

    /// Refresh the caches derived from the snapshot (flows and remotes).
    fn refresh_snapshot_caches(&mut self) {
        self.flow_cache = cache::FlowListCache::build(self.snapshot.as_ref());
        self.remote_cache = cache::RemoteCache::build(self.snapshot.as_ref());
    }

    /// Refresh the runtime-derived cache and mark dependent caches stale.
    fn refresh_runtime_cache(&mut self) {
        self.runtime_gen += 1;
        self.runtime_cache = cache::RuntimeCache::build(&self.runtime);
    }

    /// Mark the graph-derived caches stale. The next reader rebuilds them.
    fn touch_graph(&mut self) {
        self.graph_gen += 1;
    }

    /// Graph node/edge lists, headings, and node status, rebuilt when the
    /// graph, the runtime, or the selected flow changes.
    pub(crate) fn graph_cache(&mut self) -> cache::GraphCache {
        let flow_id = self.selected_flow.clone().unwrap_or_default();
        let runtime_gen = self.runtime_gen;
        let graph_gen = self.graph_gen;
        if self
            .graph_cache
            .as_ref()
            .is_none_or(|cache| !cache.matches(&flow_id, runtime_gen, graph_gen))
        {
            self.graph_cache = Some(cache::GraphCache::build(
                self.graph.as_ref(),
                &self.runtime,
                &flow_id,
                runtime_gen,
                graph_gen,
            ));
        }
        self.graph_cache.clone().unwrap()
    }

    /// Per-edge sync progress, transfer ranking, and file dots for the
    /// selected flow, rebuilt when the graph or the runtime changes.
    pub(crate) fn sync_cache(&mut self) -> cache::SyncCache {
        let flow_id = self.selected_flow.clone().unwrap_or_default();
        let runtime_gen = self.runtime_gen;
        let graph_gen = self.graph_gen;
        if self
            .sync_cache
            .as_ref()
            .is_none_or(|cache| !cache.matches(&flow_id, runtime_gen, graph_gen))
        {
            let graph = self.graph_cache();
            self.sync_cache = Some(cache::SyncCache::build(
                &self.runtime,
                &flow_id,
                &graph.edges,
                runtime_gen,
                graph_gen,
            ));
        }
        self.sync_cache.clone().unwrap()
    }

    pub(crate) fn remote_test_in_flight(&self, name: &str) -> bool {
        self.pending.values().any(|(_, pending)| {
            matches!(pending, Pending::RemoteTest { name: testing } if testing == name)
        })
    }

    pub(crate) fn history_in_flight(&self) -> bool {
        self.pending
            .values()
            .any(|(_, pending)| matches!(pending, Pending::History))
    }

    pub(crate) fn logs_in_flight(&self) -> bool {
        self.pending
            .values()
            .any(|(_, pending)| matches!(pending, Pending::Logs))
    }

    pub(crate) fn flow_save_in_flight(&self) -> bool {
        self.pending
            .values()
            .any(|(_, pending)| matches!(pending, Pending::SaveFlow { .. }))
    }

    fn clear_auth_error(&mut self) {
        self.auth_error = None;
        self.auth_error_code = None;
        self.auth_error_simulated = false;
    }

    fn note_remote_failure(&mut self, name: String, sentence: String, code: String, raw: String) {
        self.remote_ok.remove(&name);
        self.remote_failed.insert(name.clone());
        self.remote_faults
            .insert(name, states::Fault::new(sentence, code, raw));
    }

    pub(crate) fn app_settings(&self) -> AppSettings {
        self.snapshot
            .as_ref()
            .map(|snap| snap.app_settings.clone())
            .unwrap_or_default()
    }

    pub(crate) fn engine_paths(&self) -> Paths {
        self.snapshot
            .as_ref()
            .map(|snap| snap.paths.clone())
            .unwrap_or_default()
    }

    pub(crate) fn version(&self) -> String {
        self.snapshot
            .as_ref()
            .map(|snap| snap.version.clone())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                self.hello
                    .as_ref()
                    .and_then(|hello| hello.get("appVersion"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "—".into())
    }

    pub(crate) fn rclone_version(&self) -> String {
        self.snapshot
            .as_ref()
            .map(|snap| snap.rclone_version.clone())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| self.provider_rclone.clone())
    }

    pub(crate) fn runtime_flow(&self, flow_id: &str) -> Option<&RuntimeFlow> {
        self.runtime.flows.iter().find(|flow| flow.id == flow_id)
    }

    pub(crate) fn flow_status(&self, flow_id: &str) -> String {
        self.runtime_cache.status(flow_id)
    }

    /// Whether a flow is running, without the status string copy.
    pub(crate) fn flow_running(&self, flow_id: &str) -> bool {
        self.runtime_cache
            .statuses
            .get(flow_id)
            .is_some_and(|status| format::running(status))
    }

    pub(crate) fn selected_running(&self) -> bool {
        self.selected_flow
            .as_deref()
            .is_some_and(|id| self.flow_running(id))
    }

    pub(crate) fn any_running(&self) -> bool {
        self.runtime
            .flows
            .iter()
            .any(|flow| format::running(&flow.status))
    }

    pub(crate) fn edge(&self, id: &str) -> Option<&GraphEdge> {
        self.graph
            .as_ref()
            .and_then(|graph| graph.edges.iter().find(|edge| edge.id == id))
    }

    pub(crate) fn node(&self, id: &str) -> Option<&GraphNode> {
        self.graph
            .as_ref()
            .and_then(|graph| graph.nodes.iter().find(|node| node.id == id))
    }

    pub(crate) fn op_runtime(&self, flow_id: &str, op_id: &str) -> Option<engine::RuntimeOp> {
        self.runtime_flow(flow_id)?
            .ops
            .iter()
            .find(|op| op.id == op_id)
            .cloned()
    }

    pub(crate) fn op_sync(&self, flow_id: &str, op_id: &str) -> Option<SyncProgress> {
        sync_for(&self.runtime, flow_id, op_id)
    }

    // MARK: - Requests

    fn send(&mut self, method: &str, params: Value, pending: Pending, cx: &mut Context<Self>) {
        match self.engine.request(method, params) {
            Ok(id) => {
                self.pending.insert(id, (Instant::now(), pending));
            }
            Err(error) => {
                self.notice(Severity::Danger, &error, cx);
            }
        }
    }

    pub(crate) fn reload(&mut self, cx: &mut Context<Self>) {
        if self.snapshot_pending {
            self.reload_at = Some(Instant::now() + RELOAD_DELAY);
            return;
        }
        self.snapshot_pending = true;
        self.send("state.snapshot", json!({}), Pending::Snapshot, cx);
    }

    pub(crate) fn load_providers(&mut self, cx: &mut Context<Self>) {
        if self.providers.is_empty() {
            self.send("remotes.providers", json!({}), Pending::Providers, cx);
        }
    }

    pub(crate) fn notice(&self, severity: Severity, text: &str, cx: &mut Context<Self>) {
        if severity == Severity::Success && !self.app_settings().notifications_enabled {
            return;
        }
        ns_ui::notify_for(&self.toaster, severity, text, TOAST_STAY, cx);
    }

    fn toast_error(&self, error: &RpcError, what: &str, cx: &mut Context<Self>) {
        let explained = format::explain(&error.code, &error.message);
        let sentence = if explained.text.is_empty() {
            what.to_string()
        } else {
            explained.text
        };
        if sentence.is_empty() {
            return;
        }
        // One sentence. The engine code is a second line, not a second error.
        let text = if explained.code.is_empty() {
            sentence
        } else {
            format!("{sentence}\nDetails: {}", explained.code)
        };
        self.notice(Severity::Danger, &text, cx);
    }

    // MARK: - Ticker

    fn spawn_tick(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(70))
                    .await;
                if this.update(cx, |this, cx| this.pump(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn pump(&mut self, cx: &mut Context<Self>) {
        let frames = self.engine.poll();
        let mut changed = !frames.is_empty();
        for frame in frames {
            self.handle_frame(frame, cx);
        }

        let link = self.engine.status();
        if link != self.link {
            self.link = link.clone();
            changed = true;
        }
        if link.is_ready() {
            if !self.started {
                self.started = true;
                self.hello = self.engine.hello();
                self.reload(cx);
                self.load_providers(cx);
                self.status = "Ready".into();
            }
        } else {
            self.started = false;
            if matches!(link, LinkStatus::Restarting { .. }) {
                self.status = link.label();
            }
        }

        if let Some(at) = self.reload_at
            && Instant::now() >= at
        {
            self.reload_at = None;
            if !self.snapshot_pending {
                self.reload(cx);
            }
        }

        // Text fields follow the flow's run state. Updating them only on a
        // change keeps the caret of the field being typed in.
        let locked = self.selected_running();
        if locked != self.inputs_locked {
            self.inputs_locked = locked;
            self.apply_input_lock(cx);
            changed = true;
        }

        let running = self.any_running();
        if running != self.busy {
            self.busy = running;
            changed = true;
        }

        if let Some(at) = self.layout_save_at
            && Instant::now() >= at
        {
            self.layout_save_at = None;
            // A pan or zoom must not write a half-edited name. It saves only
            // when the draft has no other unsaved edits.
            let viewport_only = self.viewport_dirty && !self.dirty;
            let content = self.dirty && !self.viewport_dirty;
            if !self.selected_running() && !self.layout_saving && (viewport_only || content) {
                self.layout_saving = true;
                self.viewport_dirty = false;
                self.save_flow_inner(false, viewport_only, cx);
            }
        }

        let now = Instant::now();
        let stale: Vec<u64> = self
            .pending
            .iter()
            .filter(|(_, (sent, _))| now.duration_since(*sent) > REQUEST_TIMEOUT)
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if let Some((_, pending)) = self.pending.remove(&id) {
                match &pending {
                    Pending::UpdateCheck => self.update_busy = false,
                    Pending::Auth => self.auth_busy = false,
                    Pending::RemoteAdd | Pending::OAuthStart | Pending::OAuthComplete => {
                        self.add_busy = false;
                        self.oauth_busy = false;
                    }
                    Pending::RemoteTest { name } => {
                        let name = name.clone();
                        self.note_remote_failure(
                            name,
                            "The connection test timed out.".into(),
                            "timeout".into(),
                            "The engine has not answered in 25 seconds.".into(),
                        );
                    }
                    Pending::History => {
                        self.history_error = Some("Loading run history timed out.".into());
                        self.history_error_code = Some("timeout".into());
                    }
                    Pending::Logs => {
                        self.logs_error = Some("Loading the log timed out.".into());
                        self.logs_error_code = Some("timeout".into());
                    }
                    Pending::SaveFlow { .. } => self.layout_saving = false,
                    _ => {}
                }
                let what = pending_label(&pending);
                self.notice(
                    Severity::Warning,
                    &format!("{what} timed out\nThe engine has not answered in 25 seconds."),
                    cx,
                );
            }
        }

        if let Some(until) = self.lockout_until {
            let remaining = until.saturating_duration_since(now).as_secs();
            if remaining == 0 {
                self.lockout_until = None;
                if self
                    .auth_error
                    .as_deref()
                    .is_some_and(unlock::is_raw_lockout_copy)
                {
                    self.auth_error = None;
                    self.auth_error_code = None;
                }
                changed = true;
            } else if remaining != self.last_lock_second {
                self.last_lock_second = remaining;
                changed = true;
            }
        }

        if changed {
            cx.notify();
        }
    }

    fn apply_input_lock(&mut self, cx: &mut Context<Self>) {
        let locked = self.inputs_locked;
        for input in [
            &self.flow_name_input,
            &self.cron_input,
            &self.node_label_input,
            &self.node_path_input,
            &self.edge_source_input,
            &self.edge_target_input,
        ] {
            input.update(cx, |input, cx| input.set_disabled(locked, cx));
        }
        let sync: Vec<Entity<TextInput>> = self.sync_inputs.values().cloned().collect();
        for input in sync {
            input.update(cx, |input, cx| input.set_disabled(locked, cx));
        }
    }

    // MARK: - Frames

    fn handle_frame(&mut self, frame: Frame, cx: &mut Context<Self>) {
        if let Some(id) = frame.id() {
            let Some((_, pending)) = self.pending.remove(&id) else {
                return;
            };
            if let Some(error) = frame.error() {
                self.handle_error(pending, error, cx);
            } else {
                let result = frame.result().cloned().unwrap_or(Value::Null);
                self.handle_result(pending, &result, cx);
            }
            return;
        }
        let Some(event) = frame.event().map(str::to_string) else {
            return;
        };
        let data = frame.data().clone();
        self.handle_event(&event, &data, cx);
    }

    fn handle_event(&mut self, event: &str, data: &Value, cx: &mut Context<Self>) {
        match event {
            "hello" => {
                self.hello = Some(data.clone());
            }
            "state:changed" => {
                self.reload_at = Some(Instant::now() + RELOAD_DELAY);
            }
            "auth:unlocked" => {
                self.auth_busy = false;
                self.clear_auth_error();
                self.password_error = None;
                self.ask = None;
                self.overlay = Overlay::None;
                self.status = "Unlocked".into();
                self.reload(cx);
            }
            "auth:locked" => {
                self.auth_busy = false;
                self.ask = None;
                self.overlay = Overlay::None;
                self.panel = Panel::Closed;
                self.runtime = Runtime::default();
                self.refresh_runtime_cache();
                self.status = "Locked".into();
                self.reload(cx);
            }
            "runtime:snapshot" => {
                if let Ok(runtime) = serde_json::from_value::<Runtime>(data.clone())
                    && runtime.revision >= self.runtime.revision
                {
                    self.runtime = runtime;
                    self.refresh_runtime_cache();
                }
            }
            "flow:execution" => {
                let flow_id = data
                    .get("flowID")
                    .or_else(|| data.get("flowId"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let status = data
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let op_id = data
                    .get("opID")
                    .or_else(|| data.get("opId"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let error = data
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if !flow_id.is_empty() {
                    let flow = self.runtime_flow_mut(&flow_id);
                    if !status.is_empty() {
                        flow.status = status.clone();
                        if !error.is_empty() {
                            flow.last_error = error.clone();
                        }
                    }
                    if !op_id.is_empty() {
                        let op = flow.ops.iter_mut().find(|op| op.id == op_id);
                        if let Some(op) = op {
                            if !status.is_empty() {
                                op.status = status.clone();
                            }
                            if !error.is_empty() {
                                op.last_error = error.clone();
                            }
                        } else {
                            flow.ops.push(engine::RuntimeOp {
                                id: op_id,
                                status: status.clone(),
                                last_error: error.clone(),
                            });
                        }
                    }
                    if status == "failed" && !error.is_empty() {
                        self.toast_error(&RpcError::new("flow_failed", error), "Flow failed", cx);
                    }
                    self.refresh_runtime_cache();
                }
            }
            "sync:started" | "sync:progress" | "sync:completed" | "sync:failed" => {
                let mut progress =
                    serde_json::from_value::<SyncProgress>(data.clone()).unwrap_or_default();
                if progress.profile_id.is_empty() {
                    progress.profile_id = data
                        .get("profileID")
                        .or_else(|| data.get("profileId"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                }
                if progress.action.is_empty() {
                    progress.action = data
                        .get("action")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                }
                if progress.state.is_empty() {
                    progress.state = event.trim_start_matches("sync:").to_string();
                }
                self.apply_sync(progress, cx);
            }
            "remotes:oauth" => {
                let status = data
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.oauth_status = status.clone();
                match status.as_str() {
                    "authorized" => {
                        let session = self.oauth_session.clone();
                        if !session.is_empty() {
                            let mut config = serde_json::Map::new();
                            for (key, input) in &self.add_fields {
                                let value = input.read(cx).text().trim().to_string();
                                if !value.is_empty() {
                                    config.insert(key.clone(), json!(value));
                                }
                            }
                            let name = self.add_name_input.read(cx).text().trim().to_string();
                            self.send(
                                "remotes.oauth.complete",
                                json!({
                                    "sessionID": session,
                                    "name": name,
                                    "type": self.oauth_type,
                                    "config": config,
                                }),
                                Pending::OAuthComplete,
                                cx,
                            );
                        }
                    }
                    "failed" => {
                        let error = data
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("Authorization failed");
                        self.notice(Severity::Danger, error, cx);
                        self.overlay = Overlay::None;
                    }
                    "cancelled" => {
                        self.overlay = Overlay::None;
                    }
                    _ => {}
                }
            }
            "log:line" => {
                if let Ok(line) = serde_json::from_value::<engine::LogLine>(data.clone()) {
                    let page = self.logs.get_or_insert_with(LogsPage::default);
                    page.lines.push(line);
                    if page.lines.len() > 2000 {
                        page.lines.remove(0);
                    }
                }
            }
            _ => {}
        }
    }

    fn runtime_flow_mut(&mut self, flow_id: &str) -> &mut RuntimeFlow {
        if let Some(index) = self
            .runtime
            .flows
            .iter()
            .position(|flow| flow.id == flow_id)
        {
            return &mut self.runtime.flows[index];
        }
        self.runtime.flows.push(RuntimeFlow {
            id: flow_id.to_string(),
            status: "idle".into(),
            ..Default::default()
        });
        let index = self.runtime.flows.len() - 1;
        &mut self.runtime.flows[index]
    }

    fn apply_sync(&mut self, progress: SyncProgress, cx: &mut Context<Self>) {
        let flow_id = progress
            .profile_id
            .split(':')
            .next()
            .unwrap_or_default()
            .to_string();
        if flow_id.is_empty() {
            return;
        }
        let op_id = progress
            .profile_id
            .split_once(':')
            .map(|(_, op)| op.to_string())
            .unwrap_or_default();
        let state = progress.state.clone();
        {
            let flow = self.runtime_flow_mut(&flow_id);
            flow.sync = Some(progress.clone());
            if format::running(&state) {
                flow.status = "running".into();
            } else if state == "completed" {
                flow.status = "completed".into();
            } else if state == "cancelled" {
                flow.status = "cancelled".into();
                flow.last_error.clear();
            } else if state == "failed" {
                flow.status = state.clone();
                let message = format::humanize_error(&progress.error_message);
                if !message.is_empty() {
                    flow.last_error = message;
                }
            }
            if !op_id.is_empty() {
                let op_error = if state == "cancelled" {
                    String::new()
                } else {
                    format::humanize_error(&progress.error_message)
                };
                if let Some(op) = flow.ops.iter_mut().find(|op| op.id == op_id) {
                    op.status = state.clone();
                    op.last_error = op_error;
                } else {
                    flow.ops.push(engine::RuntimeOp {
                        id: op_id.clone(),
                        status: state.clone(),
                        last_error: op_error,
                    });
                }
            }
        }
        if state == "failed" {
            let message = format::humanize_error(&progress.error_message);
            if !message.is_empty() {
                self.toast_error(&RpcError::new("sync_failed", message), "Sync failed", cx);
            }
        }
        self.refresh_runtime_cache();
        if matches!(self.selection, Selection::Flow) {
            self.follow_edge();
        }
        cx.notify();
    }

    fn handle_error(&mut self, pending: Pending, error: RpcError, cx: &mut Context<Self>) {
        match pending {
            Pending::Snapshot => {
                self.snapshot_pending = false;
                self.load_error = Some(format::explain(&error.code, &error.message).display());
            }
            Pending::Auth => {
                self.auth_busy = false;
                if let Some(secs) =
                    unlock::lockout_seconds(&error.code, &error.message, error.retry_after())
                {
                    // The countdown sentence replaces the raw "retry in 30s" text.
                    self.clear_auth_error();
                    self.lockout_until = Some(Instant::now() + Duration::from_secs(secs));
                    self.last_lock_second = secs;
                } else {
                    let explained = format::explain(&error.code, &error.message);
                    self.auth_error = Some(explained.text);
                    self.auth_error_code = Some(explained.code).filter(|code| !code.is_empty());
                    self.auth_error_simulated = false;
                }
            }
            Pending::ChangePassword | Pending::RemovePassword => {
                self.password_error = Some(format::explain(&error.code, &error.message).display());
            }
            Pending::SaveFlow { .. } => {
                self.layout_saving = false;
                self.toast_error(&error, "Save failed", cx);
            }
            Pending::RemoteAdd | Pending::OAuthStart | Pending::OAuthComplete => {
                self.add_busy = false;
                self.oauth_busy = false;
                // The dialog already shows this. A toast would repeat it.
                self.add_error = Some(format::explain(&error.code, &error.message).display());
            }
            Pending::RemoteTest { name } => {
                let explained = format::explain(&error.code, &error.message);
                self.note_remote_failure(
                    name.clone(),
                    explained.text.clone(),
                    explained.code,
                    error.message.clone(),
                );
                self.toast_error(
                    &error,
                    &format!("{name} needs a reconnect. Press Reconnect when you are ready"),
                    cx,
                );
            }
            Pending::RemoteDelete { .. } => {
                self.toast_error(&error, "Could not remove the cloud account", cx);
            }
            Pending::UpdateCheck => {
                self.update_busy = false;
                // The About card shows this. A toast would repeat the same sentence.
                self.update_text = format::explain(&error.code, &error.message).display();
            }
            Pending::UpdateApply => {
                self.toast_error(&error, "Update failed", cx);
            }
            Pending::ServiceAction { label } => {
                self.toast_error(&error, label, cx);
            }
            Pending::History => {
                let explained = format::explain(&error.code, &error.message);
                self.history_error = Some(explained.text);
                self.history_error_code = Some(explained.code).filter(|code| !code.is_empty());
            }
            Pending::Logs => {
                let explained = format::explain(&error.code, &error.message);
                self.logs_error = Some(explained.text);
                self.logs_error_code = Some(explained.code).filter(|code| !code.is_empty());
            }
            Pending::Doctor | Pending::Health | Pending::ServiceStatus => {
                // The tab draws this card. A toast would say the same thing again.
                self.settings_error = Some(format::explain(&error.code, &error.message).display());
            }
            Pending::Execute | Pending::Stop => {
                self.toast_error(&error, "Flow", cx);
            }
            Pending::CreateFlow => {
                self.toast_error(&error, "New flow", cx);
            }
            Pending::DeleteFlow => {
                self.toast_error(&error, "Delete flow", cx);
            }
            Pending::ClearHistory => {
                self.toast_error(&error, "Clear history", cx);
            }
            Pending::SettingsSet => {
                self.settings_error = Some(format::explain(&error.code, &error.message).display());
            }
            Pending::Providers => {
                self.notice(
                    Severity::Warning,
                    &format::explain(&error.code, &error.message).display(),
                    cx,
                );
            }
            Pending::FsList => {
                self.browse_loading = false;
                self.browse_error = Some(format::explain(&error.code, &error.message).display());
            }
            Pending::Generic { label } => {
                self.toast_error(&error, &label, cx);
            }
        }
        cx.notify();
    }

    fn handle_result(&mut self, pending: Pending, result: &Value, cx: &mut Context<Self>) {
        match pending {
            Pending::Snapshot => {
                self.snapshot_pending = false;
                match serde_json::from_value::<Snapshot>(result.clone()) {
                    Ok(snapshot) => self.apply_snapshot(snapshot, cx),
                    Err(error) => {
                        self.load_error = Some(format!("The snapshot did not parse: {error}"));
                    }
                }
            }
            Pending::Providers => {
                if let Ok(response) = serde_json::from_value::<ProvidersResponse>(result.clone()) {
                    self.provider_rclone = response.rclone_version;
                    self.providers = response.providers;
                }
            }
            Pending::SaveFlow { then_run, quiet } => {
                self.layout_saving = false;
                self.viewport_dirty = false;
                match serde_json::from_value::<Flow>(
                    result.get("flow").cloned().unwrap_or(Value::Null),
                ) {
                    Ok(flow) => {
                        let id = flow.id.clone();
                        if let Some(snapshot) = &mut self.snapshot
                            && let Some(index) =
                                snapshot.flows.iter().position(|item| item.id == id)
                        {
                            snapshot.flows[index] = flow.clone();
                        }
                        self.refresh_snapshot_caches();
                        if self.selected_flow.as_deref() == Some(id.as_str()) {
                            self.draft = Some(flow.clone());
                            self.graph = Some(graph::to_graph(&flow));
                            self.touch_graph();
                            self.dirty = false;
                            if !quiet {
                                self.status = "Saved".into();
                                self.notice(Severity::Success, "Flow saved", cx);
                            }
                        }
                        if let Some(warnings) = result.get("warnings").and_then(Value::as_array)
                            && !warnings.is_empty()
                        {
                            let text = warnings
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join("\n");
                            self.notice(Severity::Warning, &text, cx);
                        }
                        if then_run {
                            self.execute_flow(id, cx);
                        }
                    }
                    Err(error) => {
                        self.notice(
                            Severity::Warning,
                            &format!("Saved, but the reply did not parse: {error}"),
                            cx,
                        );
                        self.reload(cx);
                    }
                }
            }
            Pending::CreateFlow => {
                match serde_json::from_value::<Flow>(
                    result.get("flow").cloned().unwrap_or(Value::Null),
                ) {
                    Ok(flow) => {
                        let id = flow.id.clone();
                        if let Some(snapshot) = &mut self.snapshot {
                            snapshot.flows.push(flow.clone());
                        }
                        self.pick_flow(id, cx);
                        self.notice(Severity::Success, "Flow created", cx);
                    }
                    Err(error) => {
                        self.notice(
                            Severity::Warning,
                            &format!("Could not read the new flow: {error}"),
                            cx,
                        );
                    }
                }
                self.reload(cx);
            }
            Pending::DeleteFlow => {
                self.status = "Flow deleted".into();
                self.reload(cx);
            }
            Pending::Execute => {
                self.status = "Syncing".into();
                self.reload(cx);
            }
            Pending::Stop => {
                self.status = "Stopping".into();
                self.reload(cx);
            }
            Pending::Auth => {
                self.auth_busy = false;
                self.clear_auth_error();
                self.password_error = None;
                self.password_input
                    .update(cx, |input, cx| input.set_text("", cx));
                self.confirm_input
                    .update(cx, |input, cx| input.set_text("", cx));
                self.reload(cx);
            }
            Pending::ChangePassword => {
                self.password_error = None;
                self.overlay = Overlay::None;
                self.old_password_input
                    .update(cx, |input, cx| input.set_text("", cx));
                self.new_password_input
                    .update(cx, |input, cx| input.set_text("", cx));
                self.notice(Severity::Success, "Password changed", cx);
                // The new password takes effect on the next unlock.
                self.lock_now(cx);
            }
            Pending::RemovePassword => {
                self.password_error = None;
                self.overlay = Overlay::None;
                self.notice(Severity::Success, "Password removed", cx);
                self.reload(cx);
            }
            Pending::FsList => {
                self.browse_loading = false;
                self.browse_error = None;
                if let Some(entries) = result.get("entries").and_then(Value::as_array) {
                    self.browse_entries = entries
                        .iter()
                        .filter_map(|entry| serde_json::from_value(entry.clone()).ok())
                        .collect();
                }
            }
            Pending::OAuthStart => {
                self.add_busy = false;
                self.oauth_session = result
                    .get("sessionID")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.oauth_url = result
                    .get("authURL")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.oauth_type = result
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.oauth_status = "waiting".into();
                self.overlay = Overlay::OAuth;
                if !self.oauth_url.is_empty() && !self.engine.is_mock() {
                    ns_ui::open_url(&self.oauth_url);
                }
            }
            Pending::OAuthComplete => {
                self.oauth_busy = false;
                self.add_busy = false;
                self.overlay = Overlay::None;
                self.notice(Severity::Success, "Cloud account connected", cx);
                self.reload(cx);
            }
            Pending::RemoteAdd => {
                self.add_busy = false;
                self.oauth_busy = false;
                self.overlay = Overlay::None;
                self.add_name_input
                    .update(cx, |input, cx| input.set_text("", cx));
                self.notice(Severity::Success, "Remote added", cx);
                self.reload(cx);
            }
            Pending::RemoteTest { name } => {
                let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(true);
                if ok {
                    self.remote_failed.remove(&name);
                    self.remote_faults.remove(&name);
                    self.remote_ok.insert(name.clone());
                    self.notice(Severity::Success, &format!("{name} is connected"), cx);
                } else {
                    let raw = result
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("connection test failed");
                    let explained = format::explain("sync_failed", raw);
                    self.note_remote_failure(
                        name.clone(),
                        explained.text,
                        explained.code,
                        raw.to_string(),
                    );
                    self.notice(
                        Severity::Warning,
                        &format!("{name} needs a reconnect. Press Reconnect when you are ready."),
                        cx,
                    );
                }
            }
            Pending::RemoteDelete { name } => {
                self.notice(Severity::Success, &format!("{name} deleted"), cx);
                self.reload(cx);
            }
            Pending::Doctor => {
                self.doctor = serde_json::from_value(result.clone()).ok();
            }
            Pending::Logs => match serde_json::from_value(result.clone()) {
                Ok(page) => {
                    self.logs = Some(page);
                    self.logs_error = None;
                    self.logs_error_code = None;
                }
                Err(_) => {
                    self.logs_error = Some("The log reply could not be read.".into());
                    self.logs_error_code = None;
                }
            },
            Pending::Health => {
                self.health = serde_json::from_value(result.clone()).ok();
            }
            Pending::ServiceStatus | Pending::ServiceAction { .. } => {
                self.service = serde_json::from_value(result.clone()).ok();
                if let Pending::ServiceAction { label } = &pending {
                    self.notice(Severity::Success, label, cx);
                }
            }
            Pending::History => {
                if let Some(entries) = result.get("entries").and_then(Value::as_array) {
                    self.history = entries
                        .iter()
                        .filter_map(|entry| serde_json::from_value(entry.clone()).ok())
                        .collect();
                    self.history_loaded = true;
                    self.history_error = None;
                    self.history_error_code = None;
                } else {
                    self.history_loaded = true;
                    self.history_error = Some("The history reply could not be read.".into());
                    self.history_error_code = None;
                }
            }
            Pending::ClearHistory => {
                self.history.clear();
                self.history_loaded = true;
                self.history_error = None;
                self.history_error_code = None;
                self.notice(Severity::Success, "History cleared", cx);
            }
            Pending::SettingsSet => {
                if let Some(settings) = result.get("settings").cloned()
                    && let Ok(settings) = serde_json::from_value::<AppSettings>(settings)
                    && let Some(snapshot) = &mut self.snapshot
                {
                    snapshot.app_settings = settings;
                }
                self.settings_error = None;
            }
            Pending::UpdateCheck => {
                self.update_busy = false;
                if let Ok(update) = serde_json::from_value::<UpdateCheck>(result.clone()) {
                    let report = ns_ui::parse_update(
                        "gn-drive",
                        &serde_json::to_string(&update).unwrap_or_default(),
                    );
                    self.update_text = report.text;
                    self.update_url = report.url;
                    self.update = Some(update);
                }
            }
            Pending::UpdateApply => {
                let hint = result
                    .get("restartHint")
                    .and_then(Value::as_str)
                    .unwrap_or("Restart GN Drive to finish the update.");
                let new_version = result
                    .get("newVersion")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                self.notice(
                    Severity::Success,
                    &format!(
                        "Update applied{}.\n{hint}",
                        if new_version.is_empty() {
                            String::new()
                        } else {
                            format!(" to {new_version}")
                        }
                    ),
                    cx,
                );
                self.update = None;
                self.update_text.clear();
            }
            Pending::Generic { .. } => {}
        }
        cx.notify();
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        let was_locked = self.locked();
        self.load_error = None;
        let locked = snapshot.locked;
        // The runtime in a snapshot is authoritative only when it is at least
        // as new as the one pushed by events.
        if snapshot.runtime.revision >= self.runtime.revision {
            self.runtime = snapshot.runtime.clone();
            self.refresh_runtime_cache();
        }
        self.snapshot = Some(snapshot);
        self.refresh_snapshot_caches();

        if locked {
            self.runtime = Runtime::default();
            self.refresh_runtime_cache();
            self.selected_flow = None;
            self.draft = None;
            self.graph = None;
            self.touch_graph();
            self.dirty = false;
            self.selection = Selection::Flow;
            self.panel = Panel::Closed;
            self.mobile_inspector = false;
            self.status = "Locked".into();
            self.clear_sync_fields();
            self.form_flow = None;
            self.form_selection = None;
            return;
        }

        if was_locked {
            self.status = "Unlocked".into();
            self.panel = Panel::Closed;
        }

        let flows = self.flows();
        let selected_exists = self
            .selected_flow
            .as_deref()
            .map(|id| flows.iter().any(|flow| flow.id == id))
            .unwrap_or(false);
        if !selected_exists {
            match flows.first() {
                Some(flow) => {
                    let id = flow.id.clone();
                    self.adopt_flow(flow, true);
                    self.selected_flow = Some(id);
                }
                None => {
                    self.selected_flow = None;
                    self.draft = None;
                    self.graph = None;
                    self.touch_graph();
                    self.selection = Selection::Flow;
                }
            }
        } else if !self.dirty {
            // Adopt the server's copy of the selected flow, keeping the
            // user's draft when it has unsaved edits.
            if let Some(flow) = flows
                .iter()
                .find(|flow| Some(flow.id.as_str()) == self.selected_flow.as_deref())
                && self
                    .draft
                    .as_ref()
                    .map(|draft| draft.updated_at != flow.updated_at || draft.name != flow.name)
                    .unwrap_or(true)
            {
                self.adopt_flow(flow, false);
            }
        }

        // A selection whose operation disappeared falls back to the flow.
        match self.selection.clone() {
            Selection::Node(id) if self.node(&id).is_none() => {
                self.selection = Selection::Flow;
                self.form_selection = None;
            }
            Selection::Edge(id) if self.edge(&id).is_none() => {
                self.selection = Selection::Flow;
                self.form_selection = None;
                self.clear_sync_fields();
            }
            _ => {}
        }
        cx.notify();
    }

    /// Replace the working copy and graph. `fit` also asks the canvas to zoom
    /// to the graph on the next measured frame.
    pub(crate) fn adopt_flow(&mut self, flow: &Flow, fit: bool) {
        self.draft = Some(flow.clone());
        let graph = graph::to_graph(flow);
        if fit || graph.nodes.is_empty() {
            self.canvas.fit_pending = true;
        }
        self.canvas.pan_x = graph.viewport.x as f32;
        self.canvas.pan_y = graph.viewport.y as f32;
        self.canvas.zoom = (graph.viewport.zoom as f32).clamp(0.25, 2.0);
        self.graph = Some(graph);
        self.touch_graph();
        self.dirty = false;
    }

    // MARK: - Flow actions

    pub(crate) fn pick_flow(&mut self, id: String, cx: &mut Context<Self>) {
        self.panel = panel_after_flow_pick(self.panel);
        if self.selected_flow.as_deref() == Some(id.as_str()) {
            // Clicking the open flow again opens its settings. On a compact
            // window the same click still returns to the canvas.
            if !ns_ui::is_compact() && !matches!(self.selection, Selection::Flow) {
                self.pick_selection(Selection::Flow, cx);
            }
            self.mobile_inspector = false;
            if ns_ui::is_compact() {
                ns_ui::show_detail();
            }
            cx.notify();
            return;
        }
        if (self.dirty || self.editors_differ(cx)) && self.selected_flow.is_some() {
            let current = self
                .draft
                .as_ref()
                .map(|draft| draft.title())
                .unwrap_or_else(|| "This flow".into());
            let next = self
                .flows()
                .iter()
                .find(|flow| flow.id == id)
                .map(|flow| flow.title())
                .unwrap_or_else(|| "the other flow".into());
            self.ask = Some((
                Ask {
                    title: "Discard unsaved changes?".into(),
                    message: format!(
                        "\"{current}\" has changes that are not saved. Discard them and open \"{next}\"?"
                    )
                    .into(),
                    confirm: "Discard and switch".into(),
                    confirm_icon: "trash",
                    destructive: true,
                },
                AskAction::SwitchFlow { id },
            ));
            cx.notify();
            return;
        }
        let Some(flow) = self.flows().iter().find(|flow| flow.id == id).cloned() else {
            return;
        };
        self.selected_flow = Some(id);
        self.selection = Selection::Flow;
        self.form_flow = None;
        self.form_selection = None;
        self.clear_sync_fields();
        self.connect_from = None;
        self.mobile_inspector = false;
        self.adopt_flow(&flow, true);
        self.card_dismissed = false;
        self.auto_opened = None;
        self.follow_edge();
        if ns_ui::is_compact() {
            ns_ui::show_detail();
        }
        cx.notify();
    }

    /// One edge opens its card. A live run opens its card when nothing else is selected.
    fn follow_edge(&mut self) {
        if !matches!(self.selection, Selection::Flow) {
            return;
        }
        let Some(graph) = self.graph.clone() else {
            return;
        };
        if graph.edges.len() == 1 {
            self.selection = Selection::Edge(graph.edges[0].id.clone());
            self.card_dismissed = false;
            return;
        }
        let Some(flow_id) = self.selected_flow.clone() else {
            return;
        };
        for edge in &graph.edges {
            let live = self
                .op_runtime(&flow_id, &edge.id)
                .is_some_and(|op| format::running(&op.status))
                || self
                    .op_sync(&flow_id, &edge.id)
                    .is_some_and(|sync| sync.state == "running" || sync.state == "checking");
            if live && self.auto_opened.as_deref() != Some(edge.id.as_str()) {
                self.auto_opened = Some(edge.id.clone());
                self.selection = Selection::Edge(edge.id.clone());
                self.card_dismissed = false;
                return;
            }
        }
    }

    pub(crate) fn discard_draft(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_flow.clone() else {
            return;
        };
        let Some(flow) = self.flows().iter().find(|flow| flow.id == id).cloned() else {
            return;
        };
        self.adopt_flow(&flow, false);
        self.form_flow = None;
        self.form_selection = None;
        self.notice(Severity::Success, "Changes discarded", cx);
        cx.notify();
    }

    pub(crate) fn new_flow(&mut self, cx: &mut Context<Self>) {
        self.overlay = Overlay::NewFlow;
        self.new_flow_input
            .update(cx, |input, cx| input.set_text("", cx));
        cx.notify();
    }

    pub(crate) fn submit_new_flow(&mut self, cx: &mut Context<Self>) {
        let name = self.new_flow_input.read(cx).text().trim().to_string();
        if name.is_empty() {
            self.notice(Severity::Warning, "Give the flow a name first.", cx);
            return;
        }
        self.overlay = Overlay::None;
        self.send(
            "flows.create",
            json!({"name": name}),
            Pending::CreateFlow,
            cx,
        );
    }

    pub(crate) fn save_flow(&mut self, then_run: bool, cx: &mut Context<Self>) {
        self.save_flow_inner(then_run, false, cx);
    }

    fn save_flow_inner(&mut self, then_run: bool, quiet: bool, cx: &mut Context<Self>) {
        if self.draft.is_none() {
            cx.propagate();
            return;
        }
        if self.selected_running() {
            if !quiet {
                self.notice(
                    Severity::Warning,
                    "Stop the flow before saving changes.",
                    cx,
                );
            }
            return;
        }
        if let Some(issue) = self.inspector_issues(cx).into_iter().next() {
            if !quiet {
                self.notice(Severity::Warning, &issue, cx);
            }
            return;
        }
        self.collect_editors(cx);
        let Some(flow) = self.draft.clone() else {
            return;
        };
        self.send(
            "flows.save",
            json!({ "flow": flow }),
            Pending::SaveFlow { then_run, quiet },
            cx,
        );
    }

    pub(crate) fn run_flow(&mut self, cx: &mut Context<Self>) {
        let Some(flow) = self.draft.clone() else {
            cx.propagate();
            return;
        };
        if flow.operations.is_empty() {
            self.notice(
                Severity::Warning,
                "This flow has no connections to run.",
                cx,
            );
            return;
        }
        if self.selected_running() {
            cx.propagate();
            return;
        }
        if let Some(issue) = self.inspector_issues(cx).into_iter().next() {
            self.notice(Severity::Warning, &issue, cx);
            return;
        }
        self.collect_editors(cx);
        if self.dirty || self.sync_inputs_changed(cx) {
            self.save_flow(true, cx);
            return;
        }
        self.execute_flow(flow.id, cx);
    }

    fn execute_flow(&mut self, id: String, cx: &mut Context<Self>) {
        self.status = "Starting".into();
        self.pick_selection(Selection::Flow, cx);
        self.send("flows.execute", json!({ "id": id }), Pending::Execute, cx);
    }

    pub(crate) fn stop_flow(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_flow.clone() else {
            cx.propagate();
            return;
        };
        self.send("flows.stop", json!({ "id": id }), Pending::Stop, cx);
    }

    pub(crate) fn delete_selected_flow(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_flow.clone() else {
            return;
        };
        let name = self
            .draft
            .as_ref()
            .filter(|draft| draft.id == id)
            .map(|draft| draft.title())
            .or_else(|| {
                self.flows()
                    .iter()
                    .find(|flow| flow.id == id)
                    .map(|flow| flow.title())
            })
            .unwrap_or_else(|| "this flow".into());
        self.ask = Some((
            Ask {
                title: format!("Delete flow \"{name}\"?").into(),
                message: format!(
                    "\"{name}\" and its connections, schedule, and canvas are removed. Run history stays."
                )
                .into(),
                confirm: "Delete flow".into(),
                confirm_icon: "trash",
                destructive: true,
            },
            AskAction::DeleteFlow(id),
        ));
        cx.notify();
    }

    // MARK: - Inspector edits

    fn sync_inputs_changed(&self, cx: &mut Context<Self>) -> bool {
        let Some(config) = &self.sync_config else {
            return false;
        };
        let Some(edge_id) = self.sync_edge.clone() else {
            return false;
        };
        let Some(edge) = self.edge(&edge_id) else {
            return false;
        };
        let saved = SyncConfig::parse(&edge.operation.sync_config, &edge.action);
        let current = self.collect_sync_values(config, cx);
        current.map != saved.map
    }

    fn collect_sync_values(&self, config: &SyncConfig, cx: &mut Context<Self>) -> SyncConfig {
        let mut config = config.clone();
        for def in FIELDS {
            let Some(input) = self.sync_inputs.get(def.key) else {
                continue;
            };
            let text = input.read(cx).text().to_string();
            match def.kind {
                FieldKind::Int => {
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        config.set_int(def.key, None);
                    } else if crate::inspector::number_error(FieldKind::Int, trimmed).is_none()
                        && let Ok(value) = trimmed.parse::<i64>()
                    {
                        config.set_int(def.key, Some(value));
                    }
                }
                FieldKind::Double => {
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        config.set_double(def.key, None);
                    } else if crate::inspector::number_error(FieldKind::Double, trimmed).is_none()
                        && let Ok(value) = trimmed.parse::<f64>()
                    {
                        config.set_double(def.key, Some(value));
                    }
                }
                FieldKind::Text | FieldKind::Select(_) => config.set_text(def.key, &text),
                FieldKind::List => {
                    let items: Vec<String> = text
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(str::to_string)
                        .collect();
                    config.set_list(def.key, items);
                }
                FieldKind::Bool => {}
            }
        }
        config
    }

    /// Fold the inspector's text fields into the working copy. Called before a
    /// save or a run, so typing an option and pressing either applies it.
    pub(crate) fn collect_editors(&mut self, cx: &mut Context<Self>) {
        if self.updating_fields {
            return;
        }
        match self.selection.clone() {
            Selection::Node(id) => {
                let text = self.node_path_input.read(cx).text().to_string();
                self.set_node_path(&id, text.trim(), None);
            }
            Selection::Edge(id) => {
                let source = self.edge_source_input.read(cx).text().to_string();
                let target = self.edge_target_input.read(cx).text().to_string();
                if let Some(edge) = self.edge(&id).cloned() {
                    self.set_node_path(&edge.source, source.trim(), None);
                    self.set_node_path(&edge.target, target.trim(), None);
                }
                if let Some(config) = self.sync_config.clone() {
                    let config = self.collect_sync_values(&config, cx);
                    self.apply_sync_config(&id, config, cx);
                }
            }
            Selection::Flow | Selection::None => {}
        }
    }

    fn apply_sync_config(&mut self, edge_id: &str, config: SyncConfig, _cx: &mut Context<Self>) {
        self.sync_config = Some(config.clone());
        let Some(graph) = &mut self.graph else { return };
        let Some(edge) = graph.edges.iter_mut().find(|edge| edge.id == edge_id) else {
            return;
        };
        edge.operation.sync_config = config.to_json();
        edge.operation.action = config.action();
        edge.action = config.action();
        self.sync_graph_to_draft();
    }

    pub(crate) fn set_sync_flag(&mut self, key: &str, value: bool, cx: &mut Context<Self>) {
        if self.selected_running() {
            return;
        }
        let Some(config) = self.sync_config.clone() else {
            return;
        };
        let mut config = config;
        config.set_flag(key, value);
        self.apply_current_sync_with(config, cx);
    }

    pub(crate) fn set_sync_choice(&mut self, key: &str, value: &str, cx: &mut Context<Self>) {
        if self.selected_running() {
            return;
        }
        let Some(config) = self.sync_config.clone() else {
            return;
        };
        let mut config = config;
        config.set_text(key, value);
        if let Some(input) = self.sync_inputs.get(key).cloned() {
            input.update(cx, |input, cx| input.set_text(value.to_string(), cx));
        }
        self.apply_current_sync_with(config, cx);
    }

    fn apply_current_sync_with(&mut self, config: SyncConfig, cx: &mut Context<Self>) {
        let Some(edge_id) = self.sync_edge.clone() else {
            return;
        };
        let config = self.collect_sync_values(&config, cx);
        self.apply_sync_config(&edge_id, config, cx);
        self.dirty = true;
        cx.notify();
    }

    pub(crate) fn set_cron(&mut self, cron: &str, cx: &mut Context<Self>) {
        if self.selected_running() {
            return;
        }
        self.updating_fields = true;
        self.cron_input
            .update(cx, |input, cx| input.set_text(cron.to_string(), cx));
        self.updating_fields = false;
        if let Some(draft) = &mut self.draft
            && draft.schedule_cron != cron
        {
            draft.schedule_cron = cron.to_string();
            if !cron.trim().is_empty() {
                draft.schedule_enabled = true;
            }
            self.dirty = true;
        }
        cx.notify();
    }

    pub(crate) fn set_edge_action(&mut self, action: &str, cx: &mut Context<Self>) {
        if self.selected_running() {
            return;
        }
        let Selection::Edge(id) = self.selection.clone() else {
            return;
        };
        let Some(graph) = &mut self.graph else { return };
        let Some(edge) = graph.edges.iter_mut().find(|edge| edge.id == id) else {
            return;
        };
        let action = graph::normalize_action(action);
        edge.action = action.clone();
        edge.operation.action = action.clone();
        edge.operation
            .sync_config
            .insert("action".into(), json!(action));
        if let Some(config) = &mut self.sync_config {
            config.set_action(&action);
        }
        self.sync_graph_to_draft();
        cx.notify();
    }

    fn set_node_path(&mut self, node_id: &str, path: &str, remote: Option<String>) -> bool {
        if self.selected_running() || path.is_empty() {
            return false;
        }
        let (new_remote, new_path) = split_location(path);
        let remote = remote.unwrap_or(new_remote);
        let new_path = graph::normalize_path(&new_path);
        let Some(graph) = &mut self.graph else {
            return false;
        };
        let Some(node) = graph.nodes.iter_mut().find(|node| node.id == node_id) else {
            return false;
        };
        let old_remote = node.remote.clone();
        let old_path = node.path.clone();
        let old_key = graph::location_key(&old_remote, &old_path);
        if old_remote == remote && old_path == new_path {
            return false;
        }
        if let Some(draft) = &mut self.draft {
            for op in &mut draft.operations {
                if graph::location_key(&op.source_remote, &op.source_path) == old_key {
                    op.source_remote = remote.clone();
                    op.source_path = new_path.clone();
                }
                if graph::location_key(&op.target_remote, &op.target_path) == old_key {
                    op.target_remote = remote.clone();
                    op.target_path = new_path.clone();
                }
            }
        }
        node.remote = remote;
        node.path = new_path;
        self.rebind_graph();
        true
    }

    /// Rebuild nodes from the operations after an endpoint was renamed, so
    /// every incident edge follows the location.
    fn rebind_graph(&mut self) {
        let Some(graph) = self.graph.take() else {
            return;
        };
        let Some(draft) = &mut self.draft else { return };
        let (ops, canvas) = graph::from_graph(&graph, &draft.operations);
        draft.operations = ops;
        draft.canvas_json = canvas;
        self.graph = Some(graph::to_graph(draft));
        self.touch_graph();
        self.dirty = true;
    }

    /// Write node positions and the viewport into the canvas without touching
    /// operations. Used while dragging, panning, and zooming.
    pub(crate) fn sync_canvas_only(&mut self) {
        let Some(graph) = self.graph.as_ref() else {
            return;
        };
        let Some(draft) = &mut self.draft else { return };
        let mut canvas = graph::CanvasModel {
            viewport: graph.viewport.clone(),
            nodes: graph
                .nodes
                .iter()
                .map(|node| graph::CanvasLocation {
                    id: node.id.clone(),
                    remote: node.remote.clone(),
                    path: node.path.clone(),
                    label: node.label.clone(),
                    x: node.x as f64,
                    y: node.y as f64,
                })
                .collect(),
        };
        canvas.viewport.x = self.canvas.pan_x as f64;
        canvas.viewport.y = self.canvas.pan_y as f64;
        canvas.viewport.zoom = self.canvas.zoom as f64;
        draft.canvas_json = graph::serialize_canvas(&canvas);
    }

    /// After an edge or node was added or removed, rebuild operations and
    /// canvas from the graph in one step.
    pub(crate) fn sync_graph_to_draft(&mut self) {
        let Some(graph) = self.graph.take() else {
            return;
        };
        let Some(draft) = &mut self.draft else { return };
        let (ops, canvas) = graph::from_graph(&graph, &draft.operations);
        draft.operations = ops;
        draft.canvas_json = canvas;
        self.graph = Some(graph::to_graph(draft));
        self.touch_graph();
        self.dirty = true;
    }

    pub(crate) fn schedule_layout_save(&mut self) {
        if !self.selected_running() {
            self.layout_save_at = Some(Instant::now() + LAYOUT_SAVE_DELAY);
        }
    }

    pub(crate) fn add_node(&mut self, cx: &mut Context<Self>) {
        if self.selected_running() {
            return;
        }
        if self.draft.is_none() {
            return;
        }
        let (center_x, center_y) = self.canvas.center_graph();
        let mut index = 1;
        let mut path = format!("{}/New folder", ns_ui::home_dir().display());
        loop {
            let key = graph::location_key("local", &path);
            let id = graph::stable_node_id(&key);
            let exists = self
                .graph
                .as_ref()
                .map(|graph| graph.nodes.iter().any(|node| node.id == id))
                .unwrap_or(false);
            if !exists {
                let Some(graph) = &mut self.graph else { return };
                graph.nodes.push(GraphNode {
                    id: id.clone(),
                    remote: "local".into(),
                    path: path.clone(),
                    label: "New folder".into(),
                    x: center_x - NODE_W / 2.0,
                    y: center_y - NODE_H / 2.0,
                });
                self.sync_canvas_only();
                self.dirty = true;
                self.pick_selection(Selection::Node(id), cx);
                self.schedule_layout_save();
                cx.notify();
                return;
            }
            index += 1;
            path = format!("{}/New folder {index}", ns_ui::home_dir().display());
        }
    }

    pub(crate) fn connect_nodes(&mut self, source: String, target: String, cx: &mut Context<Self>) {
        if self.selected_running() {
            return;
        }
        let Some(graph) = &self.graph else { return };
        if let Some(error) = graph::connect_error(&source, &target, "push", graph) {
            self.notice(Severity::Warning, error.label(), cx);
            return;
        }
        let Some(flow_id) = self.selected_flow.clone() else {
            return;
        };
        let id = format!("O{}", crate::engine::now_ms() % 1_000_000);
        let mut operation = engine::Operation {
            id: id.clone(),
            flow_id,
            source_remote: String::new(),
            source_path: String::new(),
            target_remote: String::new(),
            target_path: String::new(),
            action: "push".into(),
            ..Default::default()
        };
        if let Some(node) = graph.nodes.iter().find(|node| node.id == source) {
            operation.source_remote = node.remote.clone();
            operation.source_path = node.path.clone();
        }
        if let Some(node) = graph.nodes.iter().find(|node| node.id == target) {
            operation.target_remote = node.remote.clone();
            operation.target_path = node.path.clone();
        }
        operation.sync_config.insert("action".into(), json!("push"));
        if let Some(graph) = &mut self.graph {
            graph.edges.push(GraphEdge {
                id: id.clone(),
                source,
                target,
                action: "push".into(),
                operation,
            });
        }
        self.connect_from = None;
        self.sync_graph_to_draft();
        self.pick_selection(Selection::Edge(id), cx);
        self.schedule_layout_save();
        self.status = "Connected".into();
        cx.notify();
    }

    pub(crate) fn delete_node(&mut self, id: String, cx: &mut Context<Self>) {
        if self.selected_running() {
            return;
        }
        let Some(graph) = &mut self.graph else { return };
        graph.nodes.retain(|node| node.id != id);
        graph
            .edges
            .retain(|edge| edge.source != id && edge.target != id);
        self.sync_graph_to_draft();
        self.pick_selection(Selection::Flow, cx);
        self.schedule_layout_save();
        cx.notify();
    }

    pub(crate) fn delete_edge(&mut self, id: String, cx: &mut Context<Self>) {
        if self.selected_running() {
            return;
        }
        let Some(graph) = &mut self.graph else { return };
        graph.edges.retain(|edge| edge.id != id);
        self.sync_graph_to_draft();
        self.pick_selection(Selection::Flow, cx);
        self.schedule_layout_save();
        cx.notify();
    }

    // MARK: - Selection

    pub(crate) fn pick_selection(&mut self, selection: Selection, cx: &mut Context<Self>) {
        if self.selection != selection {
            self.commit_open_editors(cx);
        }
        self.selection = selection;
        if ns_ui::is_compact() {
            self.mobile_inspector = true;
        }
        cx.notify();
    }

    pub(crate) fn section_open(&self, section: crate::inspector::Section) -> bool {
        self.inspector_open
            .get(&section)
            .copied()
            .unwrap_or_else(|| crate::inspector::section_starts_open(section))
    }

    pub(crate) fn toggle_section(
        &mut self,
        section: crate::inspector::Section,
        cx: &mut Context<Self>,
    ) {
        let open = !self.section_open(section);
        self.inspector_open.insert(section, open);
        cx.notify();
    }

    /// Fold valid text into the draft before the selection changes, so a typed
    /// option is not dropped. Invalid numbers stay out of the stored config.
    fn commit_open_editors(&mut self, cx: &mut Context<Self>) {
        if self.updating_fields || self.inputs_locked {
            return;
        }
        match self.selection.clone() {
            Selection::Node(id) => {
                let text = self.node_path_input.read(cx).text().to_string();
                if !text.trim().is_empty() {
                    self.set_node_path(&id, text.trim(), None);
                }
            }
            Selection::Edge(id) => {
                let source = self.edge_source_input.read(cx).text().to_string();
                let target = self.edge_target_input.read(cx).text().to_string();
                if let Some(edge) = self.edge(&id).cloned() {
                    if !source.trim().is_empty() {
                        self.set_node_path(&edge.source, source.trim(), None);
                    }
                    if !target.trim().is_empty() {
                        self.set_node_path(&edge.target, target.trim(), None);
                    }
                }
                if let Some(config) = self.sync_config.clone() {
                    let next = self.collect_sync_values(&config, cx);
                    let changed = self
                        .edge(&id)
                        .map(|edge| {
                            SyncConfig::parse(&edge.operation.sync_config, &edge.action).map
                                != next.map
                        })
                        .unwrap_or(true);
                    if changed {
                        self.apply_sync_config(&id, next, cx);
                    }
                }
            }
            Selection::Flow | Selection::None => {}
        }
    }

    pub(crate) fn inspector_issues(&self, cx: &mut Context<Self>) -> Vec<String> {
        let mut issues = Vec::new();
        if let Some(draft) = &self.draft
            && let Some(error) = crate::format::cron_error(&draft.schedule_cron)
        {
            issues.push(error.to_string());
        }
        match &self.selection {
            Selection::Node(_) => {
                if self.node_path_input.read(cx).text().trim().is_empty() {
                    issues.push(crate::inspector::PATH_REQUIRED.into());
                }
            }
            Selection::Edge(_) => {
                if self.edge_source_input.read(cx).text().trim().is_empty() {
                    issues.push(crate::inspector::SOURCE_REQUIRED.into());
                }
                if self.edge_target_input.read(cx).text().trim().is_empty() {
                    issues.push(crate::inspector::TARGET_REQUIRED.into());
                }
                let action = self
                    .sync_config
                    .as_ref()
                    .map(|config| config.action())
                    .unwrap_or_else(|| "push".into());
                for def in FIELDS {
                    if !crate::inspector::field_applies(def.key, &action) {
                        continue;
                    }
                    if !matches!(def.kind, FieldKind::Int | FieldKind::Double) {
                        continue;
                    }
                    let Some(input) = self.sync_inputs.get(def.key) else {
                        continue;
                    };
                    let text = input.read(cx).text().to_string();
                    if let Some(error) = crate::inspector::number_error(def.kind, &text) {
                        let label = crate::inspector::field_label(def.key);
                        issues.push(format!("{label}: {error}"));
                    }
                }
            }
            Selection::Flow | Selection::None => {}
        }
        issues
    }

    pub(crate) fn editors_differ(&self, cx: &mut Context<Self>) -> bool {
        if let Some(draft) = &self.draft {
            if self.flow_name_input.read(cx).text() != draft.name {
                return true;
            }
            if self.cron_input.read(cx).text() != draft.schedule_cron {
                return true;
            }
        }
        match &self.selection {
            Selection::Node(id) => {
                if let Some(node) = self.node(id) {
                    let path = location_text(&node.remote, &node.path);
                    if self.node_path_input.read(cx).text().trim() != path.trim() {
                        return true;
                    }
                }
            }
            Selection::Edge(id) => {
                if let Some(edge) = self.edge(id) {
                    let source = self
                        .node(&edge.source)
                        .map(|node| location_text(&node.remote, &node.path))
                        .unwrap_or_default();
                    let target = self
                        .node(&edge.target)
                        .map(|node| location_text(&node.remote, &node.path))
                        .unwrap_or_default();
                    if self.edge_source_input.read(cx).text().trim() != source.trim() {
                        return true;
                    }
                    if self.edge_target_input.read(cx).text().trim() != target.trim() {
                        return true;
                    }
                }
                if self.sync_text_differs(cx) {
                    return true;
                }
            }
            Selection::Flow | Selection::None => {}
        }
        false
    }

    fn sync_text_differs(&self, cx: &mut Context<Self>) -> bool {
        let Some(config) = &self.sync_config else {
            return false;
        };
        for def in FIELDS {
            let Some(input) = self.sync_inputs.get(def.key) else {
                continue;
            };
            let edited = input.read(cx).text().to_string();
            let stored = crate::inspector::stored_field_text(config, def);
            if crate::inspector::field_text_differs(def.kind, &stored, &edited) {
                return true;
            }
        }
        false
    }

    fn clear_sync_fields(&mut self) {
        self.sync_edge = None;
        self.sync_config = None;
        self.sync_inputs.clear();
        self.sync_input_subs.clear();
    }

    pub(crate) fn select_node(&mut self, id: String, cx: &mut Context<Self>) {
        self.connect_from = None;
        self.pick_selection(Selection::Node(id), cx);
    }

    pub(crate) fn select_edge(&mut self, id: String, cx: &mut Context<Self>) {
        self.connect_from = None;
        self.card_dismissed = false;
        self.pick_selection(Selection::Edge(id), cx);
    }

    /// Called each frame before the inspector is built: copies the current
    /// selection into the plain input entities once, and builds the sync
    /// option fields when the edge changes.
    pub(crate) fn prepare_form_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let flow_id = self.draft.as_ref().map(|draft| draft.id.clone());
        if self.form_flow != flow_id {
            self.form_flow = flow_id;
            if let Some(draft) = self.draft.clone() {
                self.updating_fields = true;
                let name = draft.name.clone();
                self.flow_name_input
                    .update(cx, |input, cx| input.set_text(name, cx));
                let cron = draft.schedule_cron.clone();
                self.cron_input
                    .update(cx, |input, cx| input.set_text(cron, cx));
                self.updating_fields = false;
                self.form_selection = None;
            }
        }

        let selection = self.selection.clone();
        if self.form_selection.as_ref() == Some(&selection) {
            return;
        }
        self.form_selection = Some(selection.clone());
        self.updating_fields = true;
        match selection {
            Selection::Flow => {
                if let Some(draft) = self.draft.clone() {
                    let name = draft.name.clone();
                    self.flow_name_input
                        .update(cx, |input, cx| input.set_text(name, cx));
                    let cron = draft.schedule_cron.clone();
                    self.cron_input
                        .update(cx, |input, cx| input.set_text(cron, cx));
                }
                self.clear_sync_fields();
            }
            Selection::Node(id) => {
                if let Some(node) = self.node(&id).cloned() {
                    self.node_label_input
                        .update(cx, |input, cx| input.set_text(node.label, cx));
                    let path = location_text(&node.remote, &node.path);
                    self.node_path_input
                        .update(cx, |input, cx| input.set_text(path, cx));
                }
                self.clear_sync_fields();
            }
            Selection::None => {
                self.clear_sync_fields();
            }
            Selection::Edge(id) => {
                if let Some(edge) = self.edge(&id).cloned() {
                    let source = self
                        .node(&edge.source)
                        .map(|node| location_text(&node.remote, &node.path))
                        .unwrap_or_default();
                    let target = self
                        .node(&edge.target)
                        .map(|node| location_text(&node.remote, &node.path))
                        .unwrap_or_default();
                    self.edge_source_input
                        .update(cx, |input, cx| input.set_text(source, cx));
                    self.edge_target_input
                        .update(cx, |input, cx| input.set_text(target, cx));
                    self.build_sync_fields(&edge, window, cx);
                }
            }
        }
        self.updating_fields = false;
    }

    fn build_sync_fields(&mut self, edge: &GraphEdge, window: &mut Window, cx: &mut Context<Self>) {
        if self.sync_edge.as_deref() == Some(edge.id.as_str()) && !self.sync_inputs.is_empty() {
            return;
        }
        self.sync_edge = Some(edge.id.clone());
        self.sync_inputs.clear();
        self.sync_input_subs.clear();
        let config = SyncConfig::parse(&edge.operation.sync_config, &edge.action);
        self.sync_config = Some(config.clone());
        for def in FIELDS {
            let (value, multi) = match def.kind {
                FieldKind::Int => (config.int(def.key).map(|v| v.to_string()), false),
                FieldKind::Double => (config.double(def.key).map(|v| v.to_string()), false),
                FieldKind::Text | FieldKind::Select(_) => {
                    let text = config.text(def.key);
                    (Some(text).filter(|text| !text.is_empty()), false)
                }
                FieldKind::List => {
                    let items = config.list(def.key);
                    ((!items.is_empty()).then(|| items.join("\n")), true)
                }
                FieldKind::Bool => (None, false),
            };
            let placeholder = def.hint.to_string();
            let input = if multi {
                cx.new(|cx| {
                    TextInput::new(window, cx)
                        .multi_line(3, 6)
                        .placeholder(placeholder)
                })
            } else {
                cx.new(|cx| TextInput::new(window, cx).placeholder(placeholder))
            };
            if let Some(value) = value {
                input.update(cx, |input, cx| input.set_text(value, cx));
            }
            if self.inputs_locked {
                input.update(cx, |input, cx| input.set_disabled(true, cx));
            }
            let sub = cx.subscribe_in(
                &input,
                window,
                |this: &mut Self, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Changed) && !this.updating_fields {
                        cx.notify();
                    }
                },
            );
            self.sync_input_subs.push(sub);
            self.sync_inputs.insert(def.key, input);
        }
    }

    // MARK: - Folders

    pub(crate) fn open_browse(
        &mut self,
        target: BrowseTarget,
        start: String,
        cx: &mut Context<Self>,
    ) {
        if self.selected_running() {
            self.notice(
                Severity::Warning,
                "Stop the flow before choosing a different folder.",
                cx,
            );
            return;
        }
        self.overlay = Overlay::Browse(target);
        let path = if start.trim().is_empty() {
            ns_ui::home_dir().to_string_lossy().into_owned()
        } else {
            start
        };
        self.browse_input
            .update(cx, |input, cx| input.set_text(path.clone(), cx));
        self.load_folder(path, cx);
        cx.notify();
    }

    pub(crate) fn load_folder(&mut self, mut path: String, cx: &mut Context<Self>) {
        path = expand_home(path.trim());
        if path.is_empty() {
            return;
        }
        self.browse_path = path.clone();
        self.browse_loading = true;
        self.browse_error = None;
        self.browse_entries.clear();
        self.browse_input
            .update(cx, |input, cx| input.set_text(path.clone(), cx));
        self.send("fs.list", json!({ "path": path }), Pending::FsList, cx);
        cx.notify();
    }

    pub(crate) fn apply_browse(&mut self, cx: &mut Context<Self>) {
        let path = split_location(&self.browse_path);
        match self.overlay.clone() {
            Overlay::Browse(BrowseTarget::NodePath(id)) => {
                self.set_node_path(&id, &location_text(&path.0, &path.1), None);
                self.form_selection = None;
            }
            Overlay::Browse(BrowseTarget::EdgeSource(edge_id)) => {
                if let Some(edge) = self.edge(&edge_id).cloned() {
                    self.set_node_path(&edge.source, &location_text(&path.0, &path.1), None);
                    self.form_selection = None;
                }
            }
            Overlay::Browse(BrowseTarget::EdgeTarget(edge_id)) => {
                if let Some(edge) = self.edge(&edge_id).cloned() {
                    self.set_node_path(&edge.target, &location_text(&path.0, &path.1), None);
                    self.form_selection = None;
                }
            }
            _ => {}
        }
        self.overlay = Overlay::None;
        cx.notify();
    }

    pub(crate) fn browse_dir(&self, entry: &FileEntry) -> String {
        let current = split_location(&self.browse_path);
        let path = if entry.path.is_empty() {
            entry.name.clone()
        } else {
            entry.path.clone()
        };
        if path.contains(':') || current.0.is_empty() {
            path
        } else {
            format!("{}:{}", current.0, ensure_leading(&path))
        }
    }

    pub(crate) fn browse_parent(&self) -> String {
        let (remote, path) = split_location(&self.browse_path);
        let parent = path
            .trim_end_matches('/')
            .rsplit_once('/')
            .map(|(head, _)| {
                if head.is_empty() {
                    "/".to_string()
                } else {
                    head.to_string()
                }
            })
            .unwrap_or_else(|| "/".into());
        location_text(&remote, &parent)
    }

    // MARK: - Remotes

    pub(crate) fn open_add_remote(&mut self, cx: &mut Context<Self>) {
        self.overlay = Overlay::AddRemote;
        self.add_provider = None;
        self.add_options.clear();
        self.add_fields.clear();
        self.add_error = None;
        self.add_busy = false;
        self.revealed_secrets.clear();
        self.add_name_input
            .update(cx, |input, cx| input.set_text("", cx));
        self.load_providers(cx);
        cx.notify();
    }

    pub(crate) fn pick_provider(
        &mut self,
        kind: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_provider = Some(kind.to_string());
        self.add_fields.clear();
        self.add_options = self
            .providers
            .iter()
            .find(|provider| provider.name == kind)
            .map(|provider| {
                provider
                    .options
                    .iter()
                    .filter(|option| option.required && !option.advanced && option.hide <= 0)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        for option in self.add_options.clone() {
            let placeholder = if option.help.is_empty() {
                option.name.clone()
            } else {
                option.help.clone()
            };
            let secret = option.is_password || option.sensitive;
            let mut input = cx.new(|cx| {
                let field = TextInput::new(window, cx).placeholder(placeholder);
                if secret { field.masked() } else { field }
            });
            let _ = &mut input;
            if !option.default_value.is_empty() {
                let default = option.default_value.clone();
                input.update(cx, |input, cx| input.set_text(default, cx));
            }
            self.add_fields.insert(option.name, input);
        }
        cx.notify();
    }

    fn add_provider_is_oauth(&self) -> bool {
        self.add_provider
            .as_deref()
            .and_then(|kind| self.providers.iter().find(|provider| provider.name == kind))
            .map(|provider| provider.oauth)
            .unwrap_or(false)
    }

    pub(crate) fn submit_add_remote(&mut self, cx: &mut Context<Self>) {
        if self.add_busy || self.oauth_busy {
            return;
        }
        let Some(kind) = self.add_provider.clone() else {
            self.add_error = Some("Choose a service first.".into());
            cx.notify();
            return;
        };
        let name = self.add_name_input.read(cx).text().trim().to_string();
        if name.is_empty() {
            self.add_error = Some("Give the account a name.".into());
            cx.notify();
            return;
        }
        for option in &self.add_options {
            if !option.required {
                continue;
            }
            let missing = self
                .add_fields
                .get(&option.name)
                .map(|input| input.read(cx).text().trim().is_empty())
                .unwrap_or(true);
            if missing {
                self.add_error = Some(format!("Enter {}.", option.name));
                cx.notify();
                return;
            }
        }
        self.add_error = None;
        if self.add_provider_is_oauth() {
            // One in-flight authorize. Closing the dialog cancels it; nothing retries it.
            self.oauth_busy = true;
            self.add_busy = true;
            self.send(
                "remotes.oauth.start",
                json!({ "type": kind }),
                Pending::OAuthStart,
                cx,
            );
            return;
        }
        self.add_busy = true;
        let mut config = serde_json::Map::new();
        for (key, input) in &self.add_fields {
            let value = input.read(cx).text().trim().to_string();
            if !value.is_empty() {
                config.insert(key.clone(), json!(value));
            }
        }
        self.send(
            "remotes.add",
            json!({ "name": name, "type": kind, "config": config }),
            Pending::RemoteAdd,
            cx,
        );
    }

    /// The mock engine waits for an outside callback that never comes, so its
    /// OAuth dialog offers this button. It sends the completion the real
    /// engine sends from `remotes:oauth authorized`.
    pub(crate) fn complete_oauth_mock(&mut self, cx: &mut Context<Self>) {
        let session = self.oauth_session.clone();
        if session.is_empty() {
            return;
        }
        let mut config = serde_json::Map::new();
        for (key, input) in &self.add_fields {
            let value = input.read(cx).text().trim().to_string();
            if !value.is_empty() {
                config.insert(key.clone(), json!(value));
            }
        }
        let name = self.add_name_input.read(cx).text().trim().to_string();
        self.oauth_session.clear();
        self.send(
            "remotes.oauth.complete",
            json!({
                "sessionID": session,
                "name": name,
                "type": self.oauth_type,
                "config": config,
            }),
            Pending::OAuthComplete,
            cx,
        );
    }

    pub(crate) fn test_remote(&mut self, name: String, cx: &mut Context<Self>) {
        if self.remote_test_in_flight(&name) {
            return;
        }
        self.send(
            "remotes.test",
            json!({ "name": name.clone() }),
            Pending::RemoteTest { name },
            cx,
        );
    }

    pub(crate) fn delete_remote(&mut self, name: String, cx: &mut Context<Self>) {
        if self.any_running() {
            self.notice(
                Severity::Warning,
                "Stop the running flow before removing a cloud account.",
                cx,
            );
            return;
        }
        self.ask = Some((
            Ask {
                title: format!("Remove cloud account \"{name}\"?").into(),
                message: format!(
                    "Removing \"{name}\" disconnects it from GN Drive. Flows that use it stay and fail until you pick another account. Files already in the cloud stay there."
                )
                .into(),
                confirm: "Remove account".into(),
                confirm_icon: "trash",
                destructive: true,
            },
            AskAction::DeleteRemote(name),
        ));
        cx.notify();
    }

    // MARK: - Security

    pub(crate) fn submit_auth(&mut self, cx: &mut Context<Self>) {
        if self.auth_busy || !self.locked() {
            cx.propagate();
            return;
        }
        if let Some(until) = self.lockout_until
            && Instant::now() < until
        {
            cx.propagate();
            return;
        }
        let password = self.password_input.read(cx).text().to_string();
        if password.is_empty() {
            self.auth_error = Some("Enter the password.".into());
            self.auth_error_code = None;
            self.auth_error_simulated = false;
            cx.notify();
            return;
        }
        if !self.auth().setup {
            let confirm = self.confirm_input.read(cx).text().to_string();
            if password.chars().count() < 4 {
                self.auth_error = Some("Use at least 4 characters.".into());
                self.auth_error_code = None;
                self.auth_error_simulated = false;
                cx.notify();
                return;
            }
            if password != confirm {
                self.auth_error = Some("The passwords do not match.".into());
                self.auth_error_code = None;
                self.auth_error_simulated = false;
                cx.notify();
                return;
            }
            self.auth_busy = true;
            self.clear_auth_error();
            self.send(
                "auth.setup",
                json!({ "password": password }),
                Pending::Auth,
                cx,
            );
        } else {
            self.auth_busy = true;
            self.clear_auth_error();
            self.send(
                "auth.unlock",
                json!({ "password": password }),
                Pending::Auth,
                cx,
            );
        }
        cx.notify();
    }

    pub(crate) fn lock_now(&mut self, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        self.send(
            "auth.lock",
            json!({}),
            Pending::Generic {
                label: "Lock".into(),
            },
            cx,
        );
    }

    pub(crate) fn submit_change_password(&mut self, cx: &mut Context<Self>) {
        let old = self.old_password_input.read(cx).text().to_string();
        let new = self.new_password_input.read(cx).text().to_string();
        if new.chars().count() < 4 {
            self.password_error = Some("Use at least 4 characters.".into());
            cx.notify();
            return;
        }
        self.password_error = None;
        self.send(
            "auth.changePassword",
            json!({ "oldPassword": old, "newPassword": new }),
            Pending::ChangePassword,
            cx,
        );
        cx.notify();
    }

    pub(crate) fn submit_remove_password(&mut self, cx: &mut Context<Self>) {
        let password = self.remove_password_input.read(cx).text().to_string();
        self.password_error = None;
        self.send(
            "auth.removePassword",
            json!({ "password": password }),
            Pending::RemovePassword,
            cx,
        );
        cx.notify();
    }

    // MARK: - Settings

    pub(crate) fn set_panel(&mut self, panel: Panel, cx: &mut Context<Self>) {
        self.panel = next_panel(self.panel, panel);
        self.connect_from = None;
        if self.panel == Panel::Remotes {
            self.load_providers(cx);
        }
        if self.panel == Panel::Settings {
            self.load_settings_tab(cx);
        }
        // The page and the workspace both live in the editor. On a narrow
        // window that editor is the detail pane, so either result shows it.
        ns_ui::show_detail();
        cx.notify();
    }

    pub(crate) fn pick_settings_tab(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        self.settings_tab = tab;
        self.load_settings_tab(cx);
        cx.notify();
    }

    pub(crate) fn load_settings_tab(&mut self, cx: &mut Context<Self>) {
        match self.settings_tab {
            SettingsTab::Service => {
                if self.service.is_none() {
                    self.send("service.status", json!({}), Pending::ServiceStatus, cx);
                }
                self.send("service.health", json!({}), Pending::Health, cx);
            }
            SettingsTab::History => {
                self.send("history.list", json!({ "limit": 20 }), Pending::History, cx);
            }
            SettingsTab::Logs => {
                self.send(
                    "doctor",
                    json!({ "includeDataFiles": true }),
                    Pending::Doctor,
                    cx,
                );
                self.refresh_logs(cx);
                self.send("service.health", json!({}), Pending::Health, cx);
            }
            SettingsTab::About => {}
            SettingsTab::General => {}
        }
    }

    pub(crate) fn set_setting(&mut self, key: &str, value: bool, cx: &mut Context<Self>) {
        if let Some(snapshot) = &mut self.snapshot {
            match key {
                "notificationsEnabled" => snapshot.app_settings.notifications_enabled = value,
                "debugMode" => snapshot.app_settings.debug_mode = value,
                "minimizeToTray" => snapshot.app_settings.minimize_to_tray = value,
                "startAtLogin" => snapshot.app_settings.start_at_login = value,
                _ => {}
            }
        }
        let settings = self.app_settings();
        self.send(
            "settings.set",
            json!({ "settings": {
                "notificationsEnabled": settings.notifications_enabled,
                "debugMode": settings.debug_mode,
                "minimizeToTray": settings.minimize_to_tray,
                "startAtLogin": settings.start_at_login,
            }}),
            Pending::SettingsSet,
            cx,
        );
        if key == "startAtLogin" && value {
            self.service_action("service.install", "Start at login", cx);
            self.notice(Severity::Success, "GN Drive will open when you log in.", cx);
        }
        if key == "startAtLogin" && !value {
            // The login item is the user service: turning it off removes it.
            self.service_action("service.uninstall", "Stop at login", cx);
            self.notice(Severity::Success, "GN Drive will not open at login.", cx);
        }
        if key == "minimizeToTray" {
            self.notice(
                Severity::Success,
                if value {
                    "Closing the window hides GN Drive and syncs keep running. Quit still exits."
                } else {
                    "Closing the window closes GN Drive. Syncs stop when you quit."
                },
                cx,
            );
        }
        cx.notify();
    }

    pub(crate) fn service_action(
        &mut self,
        method: &str,
        label: &'static str,
        cx: &mut Context<Self>,
    ) {
        let params = if method == "service.install" {
            json!({ "scope": "user", "execPath": discover_exec_path() })
        } else {
            json!({})
        };
        self.send(method, params, Pending::ServiceAction { label }, cx);
    }

    pub(crate) fn check_update(&mut self, cx: &mut Context<Self>) {
        if self.update_busy {
            return;
        }
        self.update_busy = true;
        self.send("update.check", json!({}), Pending::UpdateCheck, cx);
    }

    pub(crate) fn refresh_logs(&mut self, cx: &mut Context<Self>) {
        let params = if self.log_errors_only {
            json!({ "limit": 400, "level": "warn" })
        } else {
            json!({ "limit": 400, "level": "info" })
        };
        self.send("logs.read", params, Pending::Logs, cx);
    }

    pub(crate) fn ask_lock(&mut self, cx: &mut Context<Self>) {
        if self.locked() || !self.auth().enabled && !self.auth().setup {
            cx.propagate();
            return;
        }
        self.ask = Some((
            Ask {
                title: "Lock GN Drive?".into(),
                message: "Your flows hide until you enter the master password again.".into(),
                confirm: "Lock".into(),
                confirm_icon: "eye-off",
                destructive: false,
            },
            AskAction::Lock,
        ));
        cx.notify();
    }

    // MARK: - Confirmations

    pub(crate) fn confirm_ask(&mut self, cx: &mut Context<Self>) {
        let Some((_, action)) = self.ask.take() else {
            return;
        };
        self.run_ask(action, cx);
        cx.notify();
    }

    /// Run the action a confirmation asked about. The card calls this after it
    /// has closed, so the window never holds a half-answered question.
    pub(crate) fn run_ask(&mut self, action: AskAction, cx: &mut Context<Self>) {
        match action {
            AskAction::DeleteFlow(id) => {
                self.send("flows.delete", json!({ "id": id }), Pending::DeleteFlow, cx);
                if self.selected_flow.as_deref() == Some(id.as_str()) {
                    self.selected_flow = None;
                    self.draft = None;
                    self.graph = None;
                    self.touch_graph();
                    self.selection = Selection::Flow;
                }
                self.reload(cx);
            }
            AskAction::SwitchFlow { id } => {
                self.dirty = false;
                self.pick_flow(id, cx);
            }
            AskAction::DeleteRemote(name) => {
                self.send(
                    "remotes.delete",
                    json!({ "name": name.clone() }),
                    Pending::RemoteDelete { name },
                    cx,
                );
            }
            AskAction::ClearHistory => {
                self.send("history.clear", json!({}), Pending::ClearHistory, cx);
            }
            AskAction::UninstallService => {
                self.service_action("service.uninstall", "Service uninstalled", cx);
            }
            AskAction::RemovePassword => {
                self.overlay = Overlay::RemovePassword;
                self.remove_password_input
                    .update(cx, |input, cx| input.set_text("", cx));
                self.password_error = None;
            }
            AskAction::ApplyUpdate => {
                self.send("update.apply", json!({}), Pending::UpdateApply, cx);
            }
            AskAction::Lock => self.lock_now(cx),
        }
        cx.notify();
    }

    // MARK: - Menus and actions

    fn install_actions(&mut self, cx: &mut Context<Self>) {
        let entity = cx.entity();
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-n", NewFlow, None),
            // None: ⌘S saves the open draft even when a compact window hides the inspector.
            KeyBinding::new("cmd-s", SaveFlow, None),
            KeyBinding::new("cmd-r", RunFlow, None),
            KeyBinding::new("cmd-.", StopFlow, None),
            KeyBinding::new("cmd-,", ToggleSettings, None),
            KeyBinding::new("cmd-shift-r", ToggleRemotes, None),
            KeyBinding::new("cmd-i", ToggleInspector, None),
            KeyBinding::new("cmd-shift-l", Lock, None),
            KeyBinding::new("cmd-f", FindFlow, None),
            KeyBinding::new("cmd-=", ZoomIn, None),
            KeyBinding::new("cmd--", ZoomOut, None),
            KeyBinding::new("cmd-0", FitCanvas, None),
            KeyBinding::new("delete", DeleteSelection, Some("FlowCanvas")),
            KeyBinding::new("escape", CloseOverlay, None),
            KeyBinding::new("enter", SubmitEnter, None),
        ]);
        let app: &mut App = cx;
        let quit = entity.clone();
        app.on_action(move |_: &Quit, app| {
            quit.update(app, |this, _| {
                let _ = this.engine.request("app.quit", json!({}));
                this.engine.shutdown();
            });
            app.quit();
        });
        let close_window = entity.clone();
        app.on_action(move |_: &ns_ui::keys::CloseWindow, app| {
            if close_window.read(app).app_settings().minimize_to_tray {
                app.hide();
                app.stop_propagation();
            } else {
                app.propagate();
            }
        });
        let new_flow = entity.clone();
        app.on_action(move |_: &NewFlow, app| {
            new_flow.update(app, |this, cx| this.new_flow(cx));
        });
        let save = entity.clone();
        app.on_action(move |_: &SaveFlow, app| {
            save.update(app, |this, cx| this.save_flow(false, cx));
        });
        let run = entity.clone();
        app.on_action(move |_: &RunFlow, app| {
            run.update(app, |this, cx| this.run_flow(cx));
        });
        let stop = entity.clone();
        app.on_action(move |_: &StopFlow, app| {
            stop.update(app, |this, cx| this.stop_flow(cx));
        });
        let inspector = entity.clone();
        app.on_action(move |_: &ToggleInspector, app| {
            inspector.update(app, |this, cx| {
                this.mobile_inspector = !this.mobile_inspector;
                if this.mobile_inspector && ns_ui::is_compact() {
                    ns_ui::show_detail();
                }
                cx.notify();
            });
        });
        let remotes = entity.clone();
        app.on_action(move |_: &ToggleRemotes, app| {
            remotes.update(app, |this, cx| this.set_panel(Panel::Remotes, cx));
        });
        let settings = entity.clone();
        app.on_action(move |_: &ToggleSettings, app| {
            settings.update(app, |this, cx| this.set_panel(Panel::Settings, cx));
        });
        let lock = entity.clone();
        app.on_action(move |_: &Lock, app| {
            lock.update(app, |this, cx| this.ask_lock(cx));
        });
        let find = entity.clone();
        app.on_action(move |_: &FindFlow, app| {
            find.update(app, |this, cx| {
                if this.search_open && this.flow_menu.is_none() {
                    cx.propagate();
                    return;
                }
                this.search_open = true;
                this.flow_menu = None;
                this.search_job += 1;
                this.rail_query = this.search_input.read(cx).text().trim().to_lowercase();
                cx.notify();
            });
        });
        let locale = entity.clone();
        app.on_action(move |_: &ToggleLocale, app| {
            locale.update(app, |_, cx| {
                prefs::set_locale(!prefs::is_vi());
                cx.notify();
            });
        });
        let theme_toggle = entity.clone();
        app.on_action(move |_: &ToggleTheme, app| {
            theme_toggle.update(app, |_, cx| {
                prefs::set_light(!prefs::is_light());
                cx.notify();
            });
        });
        let delete = entity.clone();
        app.on_action(move |_: &DeleteSelection, app| {
            delete.update(app, |this, cx| this.request_delete_selection(cx));
        });
        let zoom_in = entity.clone();
        app.on_action(move |_: &ZoomIn, app| {
            zoom_in.update(app, |this, cx| {
                if this.canvas.zoom >= 2.0 && this.canvas.auto_fit_size.is_none() {
                    cx.propagate();
                    return;
                }
                this.canvas.zoom_by(1.15, None);
                this.sync_canvas_only();
                this.schedule_layout_save();
                cx.notify();
            });
        });
        let zoom_out = entity.clone();
        app.on_action(move |_: &ZoomOut, app| {
            zoom_out.update(app, |this, cx| {
                if this.canvas.zoom <= 0.25 && this.canvas.auto_fit_size.is_none() {
                    cx.propagate();
                    return;
                }
                this.canvas.zoom_by(1.0 / 1.15, None);
                this.sync_canvas_only();
                this.schedule_layout_save();
                cx.notify();
            });
        });
        let fit = entity.clone();
        app.on_action(move |_: &FitCanvas, app| {
            fit.update(app, |this, cx| this.fit_canvas(cx));
        });
        let close = entity.clone();
        app.on_action(move |_: &CloseOverlay, app| {
            close.update(app, |this, cx| this.escape(cx));
        });
        let enter = entity.clone();
        app.on_action(move |_: &SubmitEnter, app| {
            enter.update(app, |this, cx| this.submit_auth(cx));
        });
        self.publish_menus(cx);
    }

    fn publish_menus(&self, cx: &mut Context<Self>) {
        let remotes = self.panel == Panel::Remotes;
        let settings = self.panel == Panel::Settings;
        cx.set_menus(
            ns_ui::keys::StandardMenus::new("GN Drive", cx)
                .app_items([MenuItem::action("Lock", Lock)])
                .quit_action(Quit)
                .menu(Menu::new("File").items(vec![
                    MenuItem::action("New Flow", NewFlow),
                    MenuItem::action("Find Flow", FindFlow),
                    MenuItem::separator(),
                    MenuItem::action("Save Flow", SaveFlow),
                    MenuItem::action("Run Flow", RunFlow),
                    MenuItem::action("Stop Flow", StopFlow),
                ]))
                .menu(Menu::new("View").items(vec![
                    MenuItem::action("Inspector", ToggleInspector),
                    MenuItem::action("Remotes", ToggleRemotes).checked(remotes),
                    MenuItem::action("Settings", ToggleSettings).checked(settings),
                    MenuItem::separator(),
                    MenuItem::action("Fit Canvas", FitCanvas),
                    MenuItem::action("Zoom In", ZoomIn),
                    MenuItem::action("Zoom Out", ZoomOut),
                    MenuItem::separator(),
                    MenuItem::action("English / Tiếng Việt", ToggleLocale),
                    MenuItem::action("Light / Dark", ToggleTheme),
                ]))
                .build(),
        );
    }

    pub(crate) fn request_delete_selection(&mut self, cx: &mut Context<Self>) {
        if self.selected_running() {
            self.notice(
                Severity::Warning,
                "Stop the flow before removing a node or connection.",
                cx,
            );
            return;
        }
        match self.selection.clone() {
            Selection::Node(id) => self.delete_node(id, cx),
            Selection::Edge(id) => {
                self.card_dismissed = true;
                self.delete_edge(id, cx);
            }
            Selection::Flow => {
                if self.selected_flow.is_some() {
                    self.delete_selected_flow(cx);
                    return;
                }
                cx.propagate();
                return;
            }
            Selection::None => {
                cx.propagate();
                return;
            }
        }
        cx.notify();
    }

    pub(crate) fn escape(&mut self, cx: &mut Context<Self>) {
        if self.overlay != Overlay::None {
            self.close_overlay(cx);
            return;
        }
        if self.flow_menu.is_some() {
            self.flow_menu = None;
        } else if self.search_open {
            self.search_open = false;
            self.search_job += 1;
            self.rail_query.clear();
        } else if self.ask.is_some() {
            self.ask = None;
        } else if self.connect_from.is_some() {
            self.connect_from = None;
            self.status = "Connection cancelled".into();
        } else if self.panel != Panel::Closed {
            self.panel = Panel::Closed;
        } else if self.mobile_inspector {
            self.mobile_inspector = false;
        } else {
            cx.propagate();
            return;
        }
        cx.notify();
    }

    pub(crate) fn close_overlay(&mut self, cx: &mut Context<Self>) {
        if matches!(self.overlay, Overlay::OAuth) && !self.oauth_session.is_empty() {
            let session = self.oauth_session.clone();
            // The devtools preview never starts a real session. Cancelling it
            // only produces a toast about a session that was never opened.
            if session != "mock-session" {
                self.send(
                    "remotes.oauth.cancel",
                    json!({ "sessionID": session }),
                    Pending::Generic {
                        label: "Cancel authorization".into(),
                    },
                    cx,
                );
            }
        }
        self.oauth_session.clear();
        self.oauth_url.clear();
        self.oauth_busy = false;
        self.add_busy = false;
        self.password_error = None;
        self.overlay = Overlay::None;
        cx.notify();
    }

    pub(crate) fn fit_canvas(&mut self, cx: &mut Context<Self>) {
        let nodes = self
            .graph
            .as_ref()
            .map(|graph| graph.nodes.clone())
            .unwrap_or_default();
        let (width, height) = self.canvas.size.get();
        if nodes.is_empty() || width <= 0.0 || height <= 0.0 {
            cx.propagate();
            return;
        }
        let viewport = (self.canvas.pan_x, self.canvas.pan_y, self.canvas.zoom);
        self.canvas.fit(&nodes);
        if viewport == (self.canvas.pan_x, self.canvas.pan_y, self.canvas.zoom) {
            cx.propagate();
            return;
        }
        self.note_viewport(cx);
    }

    /// Pan, zoom, and fit change the view only. They must not mark the flow unsaved.
    pub(crate) fn note_viewport(&mut self, cx: &mut Context<Self>) {
        self.viewport_dirty = true;
        self.sync_canvas_only();
        self.schedule_layout_save();
        cx.notify();
    }

    // MARK: - Rendering

    pub(crate) fn workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.prepare_form_fields(window, cx);
        if let Some(fitted) = self.canvas.auto_fit_size {
            let now = self.canvas.size.get();
            if (fitted.0 - now.0).abs() > 1.0 || (fitted.1 - now.1).abs() > 1.0 {
                self.canvas.fit_pending = true;
            }
        }
        if self.canvas.fit_pending {
            let nodes = self
                .graph
                .as_ref()
                .map(|graph| graph.nodes.clone())
                .unwrap_or_default();
            if !nodes.is_empty() {
                let (w, h) = self.canvas.size.get();
                if w > 0.0 && h > 0.0 {
                    self.canvas.fit(&nodes);
                    self.canvas.fit_pending = false;
                    self.canvas.auto_fit_size = Some((w, h));
                    self.sync_canvas_only();
                }
            } else {
                self.canvas.fit_pending = false;
            }
        }

        let flows = self.flows();
        let sidebar = workspace::flow_rail(self, &flows, cx);
        let title = match self.panel {
            Panel::Remotes => "Cloud accounts".to_string(),
            Panel::Settings => "Settings".to_string(),
            Panel::Closed => self
                .draft
                .as_ref()
                .map(|draft| draft.title())
                .unwrap_or_else(|| "Workspace".into()),
        };
        // The shell title is a string. The unsaved dot, with its tooltip, sits
        // beside the flow name in the workspace toolbar. A page uses its own name.
        let body = workspace::workspace_body(self, window, cx);
        // Cloud accounts and Settings fill the editor. The side column stays closed.
        let panel: Option<AnyElement> = None;
        let remote_count = self.remotes().len();
        let rail = settings_rail(
            "rail-remotes",
            "globe",
            self.panel == Panel::Remotes,
            format!("Cloud accounts ({remote_count})  ⌘⇧R"),
            cx.listener(|this, _, _, cx| this.set_panel(Panel::Remotes, cx)),
            self.panel == Panel::Settings,
            "Settings  ⌘,",
            cx.listener(|this, _, _, cx| this.set_panel(Panel::Settings, cx)),
        );

        let cancelling = self
            .runtime
            .flows
            .iter()
            .any(|flow| flow.status == "cancelling");
        let status = engine_status_word(
            self.locked(),
            self.busy,
            cancelling,
            &self.link,
            &self.status,
        );
        let meta = status_meta(&self.version(), &self.rclone_version());
        let extra = Some(
            div()
                .flex()
                .items_center()
                .gap_1()
                .children(
                    self.is_mock()
                        .then(|| chip(prefs::t("Demo data"), Tone::Warn).into_any_element()),
                )
                .child(tip_icon(
                    "status-lock",
                    "eye-off",
                    "Lock GN Drive. You will need the password to open it again",
                    Some("⌘⇧L"),
                    ns_ui::IconTone::Quiet,
                    cx.listener(|this, _, _, cx| this.ask_lock(cx)),
                ))
                .into_any_element(),
        );

        let chrome = ns_ui::chrome_full_height_sized(
            sidebar,
            title,
            body,
            panel,
            Some(rail.into_any_element()),
            status,
            self.busy || self.auth_busy,
            meta,
            extra,
            self.effective_sidebar_width(),
        );
        chrome.into_any_element()
    }

    fn overlay_element(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        overlays::render(self, window, cx)
    }

    fn log_block(text: &str) -> AnyElement {
        let theme = theme();
        let mut block = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .rounded_md()
            .bg(theme.raised)
            .border_1()
            .border_color(theme.border);
        let mut lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            lines.push(" ");
        }
        for line in lines {
            block = block.child(
                crate::text::wrap(line)
                    .font_family(ns_ui::MONO_FONT)
                    .text_xs()
                    .text_color(theme.text),
            );
        }
        block.into_any_element()
    }

    fn starting_screen(&self) -> AnyElement {
        let theme = theme();
        let message = if let LinkStatus::Restarting { delay, reason } = &self.link {
            let why = format::explain("", reason).text;
            format!("Reconnecting in {delay}s. {why}")
        } else {
            "Starting the GN Drive engine…".into()
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .bg(theme.background)
            .text_color(theme.text)
            .child(ns_ui::spinner("engine-starting"))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("GN Drive"),
            )
            .child(
                div()
                    .w(px(420.))
                    .max_w_full()
                    .px_4()
                    .child(crate::text::wrap_muted(message)),
            )
            .child(
                div()
                    .w(px(420.))
                    .max_w_full()
                    .px_6()
                    .child(states::loading_block("engine-skeleton", 6)),
            )
            .into_any_element()
    }

    fn fatal_screen(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = theme();
        let simulated = self.simulation == states::Simulation::ErrorEngine;
        let (code, message, holder) = if simulated {
            let fault = states::simulated_engine_fault();
            (fault.code, fault.raw, None)
        } else {
            match &self.link {
                LinkStatus::Fatal {
                    code,
                    message,
                    holder,
                } => (code.clone(), message.clone(), holder.clone()),
                _ => (String::new(), String::new(), None),
            }
        };
        let stderr = self.engine.stderr_tail();
        let explained = format::explain(&code, &message);
        let mut sentence = explained.text;
        if let Some(holder) = &holder {
            sentence.push_str(&format!(" Held by process {holder}."));
        }
        let mut raw = message.clone();
        if let Some(error) = &self.load_error
            && !raw.contains(error.as_str())
        {
            if !raw.is_empty() {
                raw.push('\n');
            }
            raw.push_str(error);
        }
        let _ = window;
        let hint = match code.as_str() {
            "instance_busy" => {
                "Close the other GN Drive window or stop the service, then try again."
            }
            "rclone_missing" => "Install rclone or put it on PATH, then try again.",
            "binary_missing" | "spawn_failed" => {
                "GN_DRIVE_BACKEND=stdio needs the `gn-drive` CLI: put it on PATH or set GN_DRIVE_BIN."
            }
            _ => "Check the details above, then try again.",
        };
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .bg(theme.background)
            .text_color(theme.text)
            .child(
                div()
                    .w(px(560.))
                    .max_w_full()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_5()
                    .rounded_lg()
                    .bg(theme.panel)
                    .border_1()
                    .border_color(theme.err)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_base()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("The engine did not start"),
                    )
                    .child(states::error_block(
                        "engine-error",
                        sentence,
                        states::details_tip(&code, &raw),
                        tip_button(
                            "engine-retry",
                            "refresh",
                            "Retry",
                            "Start the engine again",
                            None,
                            Look::Primary,
                            cx.listener(|this, _, _, cx| {
                                this.simulation = states::Simulation::None;
                                this.engine.retry();
                                this.started = false;
                                this.load_error = None;
                                this.status = "Connecting".into();
                                cx.notify();
                            }),
                        ),
                    ))
                    .children((!stderr.is_empty()).then(|| Self::log_block(&stderr.join("\n"))))
                    .child(crate::text::wrap_muted(hint)),
            )
            .into_any_element()
    }
}

impl Render for GnDriveWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "devtools")]
        let _clock = perf::clock();
        // A compact window has no divider. End a drag that a resize interrupted
        // so the pointer overlay does not stay up over the next page.
        if ns_ui::is_compact() {
            if self.sidebar_drag.take().is_some() {
                crate::prefs::set_sidebar_width(self.sidebar_width);
            }
            if self.inspector_drag.take().is_some() {
                crate::prefs::set_inspector_width(self.inspector_width);
            }
        }
        let body: AnyElement = if matches!(self.link, LinkStatus::Fatal { .. })
            || self.simulation == states::Simulation::ErrorEngine
        {
            self.fatal_screen(window, cx)
        } else if self.simulation == states::Simulation::WrongPassword {
            unlock::screen(self, window, cx)
        } else if self.snapshot.is_none() {
            if let Some(error) = &self.load_error {
                let theme = theme();
                let (sentence, code) = states::error_parts(error);
                let stored = error.clone();
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .flex_col()
                    .bg(theme.background)
                    .text_color(theme.text)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("GN Drive could not load your flows"),
                    )
                    .child(
                        div()
                            .w(px(420.))
                            .max_w_full()
                            .px_4()
                            .child(states::error_block(
                                "snapshot-error",
                                sentence,
                                states::details_tip(&code, &stored),
                                tip_button(
                                    "snapshot-retry",
                                    "refresh",
                                    "Retry",
                                    "Ask the engine for your flows again",
                                    None,
                                    Look::Primary,
                                    cx.listener(|this, _, _, cx| {
                                        this.load_error = None;
                                        this.reload(cx);
                                    }),
                                ),
                            )),
                    )
                    .into_any_element()
            } else {
                self.starting_screen()
            }
        } else if self.locked() {
            unlock::screen(self, window, cx)
        } else {
            self.workspace(window, cx)
        };

        #[cfg(feature = "devtools")]
        if let Some(key) = self.dev_key.take() {
            window.defer(cx, move |window, cx| {
                if let Ok(keystroke) = gpui::Keystroke::parse(&key) {
                    window.dispatch_keystroke(keystroke, cx);
                }
            });
        }
        let overlay = self.overlay_element(window, cx);
        let sidebar_drag = self
            .sidebar_drag
            .is_some()
            .then(|| workspace::sidebar_drag_overlay(self, cx));
        let inspector_drag = self
            .inspector_drag
            .is_some()
            .then(|| workspace::inspector_drag_overlay(self, cx));
        let toasts = ToastViewport::new("gn-drive-toasts", &self.toaster);
        let ask = self
            .ask
            .clone()
            .map(|(ask, action)| overlays::confirm(ask, action, self, cx));
        div()
            .size_full()
            .relative()
            .bg(theme().background)
            .text_color(theme().text)
            .text_sm()
            .child(body)
            .children(sidebar_drag)
            .children(inspector_drag)
            .children(overlay)
            .children(ask)
            .child(toasts)
    }
}

// MARK: - Helpers

/// The uncached sort, kept for the benchmark and the equivalence tests. The
/// window reads the same ordering from [`cache::FlowListCache`].
#[cfg(test)]
fn sorted_flows(snapshot: Option<&Snapshot>) -> Vec<Flow> {
    let mut flows = snapshot.map(|snap| snap.flows.clone()).unwrap_or_default();
    flows.sort_by_key(|flow| flow.sort_order);
    flows
}

/// The progress of one operation. The profile is matched on the borrowed sync
/// first, so a flow's whole transfer list is cloned only when the edge really
/// owns it; the old form cloned before matching every edge.
fn sync_for(runtime: &Runtime, flow_id: &str, op_id: &str) -> Option<SyncProgress> {
    let flow = runtime.flows.iter().find(|flow| flow.id == flow_id)?;
    let sync = flow.sync.as_ref()?;
    let exact = sync
        .profile_id
        .strip_prefix(flow_id)
        .and_then(|rest| rest.strip_prefix(':'))
        .is_some_and(|op| op == op_id);
    (sync.profile_id.ends_with(op_id) || exact).then(|| sync.clone())
}

pub(crate) fn split_location(path: &str) -> (String, String) {
    let path = path.trim();
    if let Some((remote, rest)) = path.split_once(':') {
        if remote.contains('/') {
            (String::new(), path.to_string())
        } else {
            (
                remote.to_string(),
                if rest.is_empty() {
                    "/".into()
                } else {
                    rest.to_string()
                },
            )
        }
    } else {
        (String::new(), path.to_string())
    }
}

pub(crate) fn location_text(remote: &str, path: &str) -> String {
    if remote.is_empty() || remote == "local" {
        path.to_string()
    } else {
        format!("{remote}:{}", graph::normalize_path(path))
    }
}

fn ensure_leading(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

fn expand_home(path: &str) -> String {
    if path == "~" {
        ns_ui::home_dir().to_string_lossy().into_owned()
    } else if let Some(rest) = path.strip_prefix("~/") {
        format!("{}/{rest}", ns_ui::home_dir().display())
    } else {
        path.to_string()
    }
}

fn pending_label(pending: &Pending) -> String {
    match pending {
        Pending::Snapshot => "Loading".into(),
        Pending::SaveFlow { .. } => "Saving the flow".into(),
        Pending::Execute => "Starting the flow".into(),
        Pending::Stop => "Stopping the flow".into(),
        Pending::CreateFlow => "Creating the flow".into(),
        Pending::DeleteFlow => "Deleting the flow".into(),
        Pending::Providers => "Loading remote types".into(),
        Pending::FsList => "Listing the folder".into(),
        Pending::OAuthStart | Pending::OAuthComplete => "Authorization".into(),
        Pending::RemoteAdd => "Adding the remote".into(),
        Pending::RemoteTest { name } => format!("Testing {name}"),
        Pending::RemoteDelete { name } => format!("Deleting {name}"),
        Pending::Auth => "Unlocking".into(),
        Pending::ChangePassword => "Changing the password".into(),
        Pending::RemovePassword => "Removing the password".into(),
        Pending::Doctor => "Running doctor".into(),
        Pending::Logs => "Reading logs".into(),
        Pending::Health => "Reading service health".into(),
        Pending::ServiceStatus => "Reading the service".into(),
        Pending::ServiceAction { label } => label.to_string(),
        Pending::History => "Loading history".into(),
        Pending::ClearHistory => "Clearing history".into(),
        Pending::SettingsSet => "Saving settings".into(),
        Pending::UpdateCheck => "Checking for updates".into(),
        Pending::UpdateApply => "Applying the update".into(),
        Pending::Generic { label } => label.clone(),
    }
}

/// Path the background service should launch. A sibling `gn-drive` next to
/// this window wins, then `GN_DRIVE_BIN`, then the name on PATH.
fn discover_exec_path() -> String {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let sibling = dir.join("gn-drive");
        if sibling.is_file() {
            return sibling.to_string_lossy().into_owned();
        }
    }
    if let Ok(path) = std::env::var("GN_DRIVE_BIN") {
        let path = path.trim();
        if !path.is_empty() {
            return path.to_string();
        }
    }
    "gn-drive".into()
}

fn main() {
    prefs::load();
    ns_ui::use_tint(ns_ui::Tint::GnDrive);
    ns_ui::set_light(prefs::is_light());
    #[cfg(feature = "devtools")]
    ns_ui::open_dev("GN Drive", GnDriveWindow::new);
    #[cfg(not(feature = "devtools"))]
    ns_ui::open("GN Drive", GnDriveWindow::new);
}

/// Mouse-down on the empty canvas: a press in the inspector or a dialog never
/// reaches this listener because those elements are separate panes.
impl GnDriveWindow {
    /// Inspector width after the 260pt floor and the half-editor cap.
    pub(crate) fn effective_inspector_width(&self) -> f32 {
        inspector::clamp_inspector_width(self.inspector_width, ns_ui::body_width())
    }

    /// Flow-list width after the 200pt floor and the 40% window cap.
    pub(crate) fn effective_sidebar_width(&self) -> f32 {
        workspace::clamp_sidebar_width(self.sidebar_width, ns_ui::viewport_width())
    }

    pub(crate) fn canvas_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        window.focus(&self.canvas_focus, cx);
        let pos = (f32::from(event.position.x), f32::from(event.position.y));
        let (ox, oy) = self.canvas.origin.get();
        if let Some(id) = workspace::hit_edge(self, pos.0 - ox, pos.1 - oy) {
            self.select_edge(id, cx);
            return;
        }
        self.card_dismissed = true;
        self.canvas.panning = Some(pos);
        self.canvas.pan_moved = false;
        if self.connect_from.is_some() {
            self.connect_from = None;
            self.status = "Connection cancelled".into();
        }
        if self.selection != Selection::None || self.mobile_inspector {
            self.commit_open_editors(cx);
            self.selection = Selection::None;
            self.form_selection = None;
            self.mobile_inspector = false;
        }
        cx.notify();
    }

    pub(crate) fn canvas_move(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        let pos = (f32::from(event.position.x), f32::from(event.position.y));
        if self.canvas.connect_drag.is_some() {
            // Released outside the canvas: no mouse-up reached us.
            if event.pressed_button != Some(MouseButton::Left) {
                self.canvas_up(cx);
                return;
            }
            let local = self.local_position(event.position).unwrap_or(pos);
            if let Some(drag) = &mut self.canvas.connect_drag {
                drag.pointer = local;
                let (dx, dy) = (pos.0 - drag.start_mouse.0, pos.1 - drag.start_mouse.1);
                if dx.hypot(dy) >= CONNECT_DRAG_SLOP {
                    drag.moved = true;
                }
            }
            cx.notify();
            return;
        }
        if let Some(drag) = &mut self.canvas.dragging {
            let dx = (pos.0 - drag.start_mouse.0) / self.canvas.zoom;
            let dy = (pos.1 - drag.start_mouse.1) / self.canvas.zoom;
            if dx.abs() > 1.0 || dy.abs() > 1.0 {
                drag.moved = true;
            }
            let id = drag.id.clone();
            let x = drag.start_node.0 + dx;
            let y = drag.start_node.1 + dy;
            if let Some(graph) = &mut self.graph
                && let Some(node) = graph.nodes.iter_mut().find(|node| node.id == id)
            {
                node.x = x;
                node.y = y;
                self.touch_graph();
                self.sync_canvas_only();
                self.dirty = true;
                cx.notify();
            }
            return;
        }
        if let Some(last) = self.canvas.panning {
            let dx = pos.0 - last.0;
            let dy = pos.1 - last.1;
            if dx.abs() > 0.5 || dy.abs() > 0.5 {
                self.canvas.pan_moved = true;
                self.canvas.auto_fit_size = None;
            }
            self.canvas.pan_x -= dx / self.canvas.zoom;
            self.canvas.pan_y -= dy / self.canvas.zoom;
            self.canvas.panning = Some(pos);
            if self.draft.is_some() {
                self.sync_canvas_only();
                self.viewport_dirty = true;
            }
            cx.notify();
        }
    }

    pub(crate) fn canvas_up(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.canvas.connect_drag.take() {
            if drag.moved
                && let Some(source) = self.connect_from.clone()
            {
                match workspace::hit_node(self, drag.pointer.0, drag.pointer.1) {
                    Some(target) if target == source => {
                        self.connect_from = None;
                        self.notice(Severity::Warning, graph::ConnectError::SelfLoop.label(), cx);
                    }
                    Some(target) => self.connect_nodes(source, target, cx),
                    None => {
                        self.connect_from = None;
                        self.status = "Connection cancelled".into();
                    }
                }
            }
            cx.notify();
            return;
        }
        let dragged = self
            .canvas
            .dragging
            .take()
            .map(|drag| drag.moved)
            .unwrap_or(false);
        let panned = self.canvas.pan_moved;
        self.canvas.panning = None;
        self.canvas.pan_moved = false;
        if (dragged || panned) && self.draft.is_some() {
            self.schedule_layout_save();
        }
        cx.notify();
    }

    pub(crate) fn node_down(
        &mut self,
        id: String,
        event: &gpui::MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        cx.stop_propagation();
        if let Some(source) = self.connect_from.clone() {
            if source == id {
                self.notice(Severity::Warning, graph::ConnectError::SelfLoop.label(), cx);
            } else {
                self.connect_nodes(source, id, cx);
            }
            return;
        }
        self.select_node(id.clone(), cx);
        if self.selected_running() {
            return;
        }
        if let Some(node) = self.node(&id) {
            self.canvas.dragging = Some(NodeDrag {
                id: id.clone(),
                start_mouse: (f32::from(event.position.x), f32::from(event.position.y)),
                start_node: (node.x, node.y),
                moved: false,
            });
        }
    }

    /// Press on a connect ring: arm a drag, and pick the source so a plain
    /// click still works as click-then-click.
    pub(crate) fn begin_connect_drag(
        &mut self,
        id: String,
        event: &gpui::MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        if self.selected_running() {
            return;
        }
        let start = (f32::from(event.position.x), f32::from(event.position.y));
        let pointer = self.local_position(event.position).unwrap_or(start);
        self.canvas.connect_drag = Some(ConnectDrag {
            start_mouse: start,
            pointer,
            moved: false,
        });
        self.start_connect(id, cx);
    }

    pub(crate) fn start_connect(&mut self, id: String, cx: &mut Context<Self>) {
        if self.selected_running() {
            return;
        }
        self.connect_from = Some(id);
        self.status = "Pick a target node".into();
        cx.notify();
    }

    pub(crate) fn zoom_event(&mut self, event: &gpui::ScrollWheelEvent, cx: &mut Context<Self>) {
        self.canvas.auto_fit_size = None;
        let delta = match event.delta {
            gpui::ScrollDelta::Pixels(point) => (f32::from(point.x), f32::from(point.y)),
            gpui::ScrollDelta::Lines(point) => (point.x * 20.0, point.y * 20.0),
        };
        if event.modifiers.control || event.modifiers.platform || event.modifiers.alt {
            let local = self.local_position(event.position);
            let factor = (1.0 + delta.1 * 0.0025).clamp(0.85, 1.15);
            self.canvas.zoom_by(factor, local);
            self.note_viewport(cx);
        } else {
            self.canvas.pan_x += delta.0 / self.canvas.zoom;
            self.canvas.pan_y += delta.1 / self.canvas.zoom;
            self.note_viewport(cx);
        }
    }

    pub(crate) fn pinch_event(&mut self, event: &gpui::PinchEvent, cx: &mut Context<Self>) {
        self.canvas.auto_fit_size = None;
        let local = self.local_position(event.position);
        self.canvas.zoom_by(1.0 + event.delta, local);
        self.note_viewport(cx);
    }

    fn local_position(&self, position: gpui::Point<gpui::Pixels>) -> Option<(f32, f32)> {
        let (ox, oy) = self.canvas.origin.get();
        Some((f32::from(position.x) - ox, f32::from(position.y) - oy))
    }
}

/// Names `dev` handles. `spec()` lists these, and the match fallthrough checks them.
#[cfg(feature = "devtools")]
const DEV_COMMANDS: &[&str] = &[
    "perf",
    "state",
    "mock",
    "unlock",
    "lock",
    "lockout",
    "flow",
    "select",
    "inspector",
    "sidebar",
    "remotes",
    "settings",
    "panel",
    "dialog",
    "zoom",
    "fit",
    "key",
    "simulate",
];

/// Plain snapshot of a `state` reply. Tests build one without a GPUI window.
#[cfg(feature = "devtools")]
struct GnDevView {
    mock: bool,
    link: String,
    locked: bool,
    flows: Vec<Value>,
    remotes: usize,
    selected_flow: Option<String>,
    flow_name: Option<String>,
    runtime: Vec<Value>,
    /// The inspector selection in the same `Debug` text `state` used to return.
    inspector: String,
    panel: String,
    settings_tab: String,
    overlay: bool,
    ask: bool,
    dirty: bool,
    toolbar: Vec<String>,
    zoom: f32,
    canvas_size: (f32, f32),
    pan: [f32; 2],
    fit_size: Option<(f32, f32)>,
    nodes: Option<Vec<Value>>,
    compact: bool,
    screen: String,
    /// `remotes` or `settings` while that page fills the editor. Null on the flow workspace.
    page: Option<String>,
    busy: bool,
    /// Clamped inspector width in points. `state` reports this as `inspectorWidth`.
    inspector_width: f32,
    /// Clamped flow-list width in points. `state` reports this as `sidebarWidth`.
    sidebar_width: f32,
    /// Active `simulate` preview. `none` is real mock data.
    simulation: String,
}

#[cfg(feature = "devtools")]
fn gn_screen(
    fatal: bool,
    has_snapshot: bool,
    load_error: bool,
    locked: bool,
    panel: Panel,
) -> &'static str {
    if fatal {
        "fatal"
    } else if !has_snapshot && load_error {
        "error"
    } else if !has_snapshot {
        "starting"
    } else if locked {
        "unlock"
    } else {
        match panel {
            Panel::Remotes => "remotes",
            Panel::Settings => "settings",
            Panel::Closed => "workspace",
        }
    }
}

#[cfg(feature = "devtools")]
fn any_busy(flags: &[bool], pending: usize) -> bool {
    pending > 0 || flags.contains(&true)
}

#[cfg(feature = "devtools")]
fn gn_dev_view(window: &GnDriveWindow) -> GnDevView {
    let listed: Rc<Vec<engine::Flow>> = if window.simulation == states::Simulation::EmptyFlows {
        Rc::new(Vec::new())
    } else {
        window.flows()
    };
    let flow_name = window.selected_flow.as_ref().and_then(|id| {
        listed
            .iter()
            .find(|flow| flow.id == *id)
            .map(|flow| flow.name.clone())
    });
    let flows = listed
        .iter()
        .map(|flow| json!({"id": flow.id, "name": flow.name}))
        .collect();
    let runtime = window
        .runtime
        .flows
        .iter()
        .map(|flow| {
            json!({
                "id": flow.id,
                "status": flow.status,
                "error": !flow.last_error.is_empty(),
            })
        })
        .collect();
    let nodes = window.graph.as_ref().map(|graph| {
        graph
            .nodes
            .iter()
            .map(|node| json!([node.x, node.y]))
            .collect()
    });
    let toolbar: &[&str] = if window.selected_running() {
        &["add-folder", "stop"]
    } else {
        &["add-folder", "run"]
    };
    GnDevView {
        mock: window.is_mock(),
        link: window.link.label(),
        locked: window.locked(),
        flows,
        remotes: crate::remotes::visible_remotes(window).len(),
        selected_flow: if window.simulation == states::Simulation::EmptyFlows {
            None
        } else {
            window.selected_flow.clone()
        },
        flow_name,
        runtime,
        inspector: format!("{:?}", window.selection),
        panel: format!("{:?}", window.panel),
        settings_tab: match window.simulation {
            states::Simulation::EmptyHistory => "history".into(),
            states::Simulation::EmptyLogs => "logs".into(),
            _ => window.settings_tab.key().to_string(),
        },
        overlay: !matches!(window.overlay, Overlay::None),
        ask: window.ask.is_some(),
        dirty: window.dirty,
        toolbar: toolbar.iter().copied().map(str::to_string).collect(),
        zoom: window.canvas.zoom,
        canvas_size: window.canvas.size.get(),
        pan: [window.canvas.pan_x, window.canvas.pan_y],
        fit_size: window.canvas.auto_fit_size,
        nodes,
        compact: ns_ui::is_compact(),
        screen: match window.simulation {
            states::Simulation::ErrorEngine => "fatal".into(),
            states::Simulation::WrongPassword => "unlock".into(),
            states::Simulation::Loading => "loading".into(),
            _ => gn_screen(
                matches!(window.link, LinkStatus::Fatal { .. }),
                window.snapshot.is_some(),
                window.load_error.is_some(),
                window.locked(),
                window.panel,
            )
            .to_string(),
        },
        page: match window.simulation {
            states::Simulation::EmptyRemotes | states::Simulation::ErrorTest => {
                Some("remotes".into())
            }
            states::Simulation::EmptyHistory | states::Simulation::EmptyLogs => {
                Some("settings".into())
            }
            states::Simulation::EmptyFlows => None,
            _ => page_key(window.panel).map(str::to_string),
        },
        inspector_width: window.effective_inspector_width(),
        sidebar_width: window.effective_sidebar_width(),
        simulation: window.simulation.key().to_string(),
        busy: any_busy(
            &[
                window.busy,
                window.snapshot_pending,
                window.auth_busy,
                window.add_busy,
                window.oauth_busy,
                window.browse_loading,
                window.update_busy,
                window.layout_saving,
            ],
            window.pending.len(),
        ),
    }
}

#[cfg(feature = "devtools")]
fn gn_dev_state_value(view: &GnDevView) -> Value {
    json!({
        "mock": view.mock,
        "link": view.link,
        "locked": view.locked,
        "flows": view.flows,
        "remotes": view.remotes,
        "selectedFlow": view.selected_flow,
        "runtime": view.runtime,
        "selection": {
            "flow": view.selected_flow,
            "name": view.flow_name,
            "inspector": view.inspector,
        },
        "panel": view.panel,
        "settingsTab": view.settings_tab,
        "overlay": view.overlay,
        "ask": view.ask,
        "dirty": view.dirty,
        "toolbar": view.toolbar,
        "zoom": view.zoom,
        "canvas": {
            "size": view.canvas_size,
            "pan": view.pan,
            "fitSize": view.fit_size,
        },
        "nodes": view.nodes,
        "compact": view.compact,
        "screen": view.screen,
        "page": view.page,
        "busy": view.busy,
        "inspectorWidth": view.inspector_width,
        "sidebarWidth": view.sidebar_width,
        "simulation": view.simulation,
    })
}

#[cfg(feature = "devtools")]
fn gn_state_schema() -> Value {
    ns_ui::state_schema(
        json!({
            "mock": {"type": "boolean"},
            "link": {"type": "string"},
            "locked": {"type": "boolean"},
            "flows": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["id", "name"],
                    "properties": {
                        "id": {"type": "string"},
                        "name": {"type": "string"}
                    }
                }
            },
            "remotes": {"type": "integer", "minimum": 0},
            "selectedFlow": {"type": ["string", "null"]},
            "runtime": {"type": "array"},
            "panel": {"type": "string"},
            "settingsTab": {"type": "string"},
            "overlay": {"type": "boolean"},
            "ask": {"type": "boolean"},
            "dirty": {"type": "boolean"},
            "toolbar": {"type": "array", "items": {"type": "string"}},
            "zoom": {"type": "number"},
            "canvas": {"type": "object"},
            "nodes": {"type": ["array", "null"]},
            "compact": {"type": "boolean"},
            "page": {"type": ["string", "null"]},
            "inspectorWidth": {"type": "number"},
            "sidebarWidth": {"type": "number"},
            "simulation": {"type": "string"}
        }),
        &[
            "mock",
            "link",
            "locked",
            "flows",
            "remotes",
            "selectedFlow",
            "runtime",
            "panel",
            "settingsTab",
            "overlay",
            "ask",
            "dirty",
            "toolbar",
            "zoom",
            "canvas",
            "nodes",
            "compact",
            "page",
            "inspectorWidth",
            "sidebarWidth",
            "simulation",
        ],
    )
}

#[cfg(feature = "devtools")]
fn dev_commands() -> Vec<ns_ui::DevCommand> {
    vec![
        ns_ui::DevCommand::new(
            "perf",
            "Time mock-only forced draws after 3 seconds idle; writes gn-drive-perf-<label>.json in the temp dir.",
            true,
        )
        .arg("label", "string", false, "Report name (letters, digits, hyphens, underscores). Default run.")
        .arg("frames", "integer", false, "Frames per scenario, 1–1000. Default 60."),
        ns_ui::DevCommand::new("state", "Read the window.", false),
        ns_ui::DevCommand::new("mock", "Switch to the in-process mock engine.", true),
        ns_ui::DevCommand::new(
            "unlock",
            "Unlock the mock engine. Refused when the engine is real.",
            true,
        ),
        ns_ui::DevCommand::new(
            "lock",
            "Lock the mock engine. Refused when the engine is real.",
            true,
        ),
        ns_ui::DevCommand::new("lockout", "Set or clear the unlock lockout timer.", true).arg(
            "seconds",
            "integer",
            false,
            "Seconds to lock out. 0 clears it. Default 30. Maximum 3600.",
        ),
        ns_ui::DevCommand::new("flow", "Open a flow.", true)
            .arg("id", "string", false, "Flow id. Use this or index.")
            .arg("index", "integer", false, "Index into the flow list. Default 0.")
            .arg("back", "boolean", false, "When true, show the navigation column."),
        ns_ui::DevCommand::new("select", "Select a node, an edge, or the flow.", true)
            .arg(
                "node",
                "string",
                false,
                "Node id. A JSON integer is an index into the node list.",
            )
            .arg(
                "edge",
                "string",
                false,
                "Edge id. A JSON integer is an index into the edge list.",
            ),
        ns_ui::DevCommand::new(
            "inspector",
            "Show the inspector, scroll it, open one section, or set its width.",
            true,
        )
        .arg(
            "open",
            "boolean",
            false,
            "With section, expand or collapse that block. Alone, show or hide the inspector. Default true.",
        )
        .arg(
            "scroll",
            "string",
            false,
            "\"top\", \"end\", or a point count. A JSON number is a point count. Checked before section.",
        )
        .arg(
            "section",
            "string",
            false,
            "basics, schedule, what, safety, advanced, connections, or danger.",
        )
        .arg(
            "width",
            "number",
            false,
            "Inspector width in points. Clamped from 260 to half the editor. Checked after scroll and section.",
        ),
        ns_ui::DevCommand::new("sidebar", "Set the flow-list width.", true).arg(
            "width",
            "number",
            true,
            "Flow-list width in points. Clamped from 200 to 40% of the window.",
        ),
        ns_ui::DevCommand::new("remotes", "Open the Cloud accounts page.", true),
        ns_ui::DevCommand::new("settings", "Open the Settings page on a tab.", true).arg(
            "tab",
            "string",
            false,
            "general, service, history, logs, or about. Default general.",
        ),
        ns_ui::DevCommand::new("panel", "Close the open page.", true).arg(
            "name",
            "string",
            false,
            "\"closed\", or omit it, to close the page and return to the flow workspace.",
        ),
        ns_ui::DevCommand::new(
            "dialog",
            "Open or close a dialog. oauth is mock-only and does not start a real login.",
            true,
        )
        .arg(
            "name",
            "string",
            true,
            "new-flow, add-remote, oauth, browse, change-password, remove-password, confirm-delete-flow, confirm-lock, or close.",
        )
        .arg("path", "string", false, "Start path for the browse dialog."),
        ns_ui::DevCommand::new("zoom", "Set the canvas zoom.", true).arg(
            "value",
            "number",
            false,
            "Zoom from 0.25 to 2. Default 1.",
        ),
        ns_ui::DevCommand::new("fit", "Fit the canvas to its nodes.", true),
        ns_ui::DevCommand::new("key", "Queue one keystroke for the next frame.", true).arg(
            "key",
            "string",
            true,
            "A GPUI keystroke, such as escape or cmd-n.",
        ),
        ns_ui::DevCommand::new(
            "simulate",
            "Preview an empty, loading, or error state. Mock only. reset restores the mock data.",
            true,
        )
        .arg(
            "state",
            "string",
            true,
            "empty-flows, empty-remotes, loading, error-engine, error-test, empty-history, empty-logs, wrong-password, or reset.",
        ),
    ]
}

/// A listed command with no match arm is a bug. Anything else is unknown.
#[cfg(feature = "devtools")]
fn reject_unknown(command: &str) -> String {
    if DEV_COMMANDS.contains(&command) {
        format!("command \"{command}\" is listed but has no handler")
    } else {
        ns_ui::unknown_command(command)
    }
}

/// Dev commands for screenshots and smoke checks: `state`, `flow`, `select`,
/// `inspector`, `remotes`, `settings`, `unlock`, `lock`, `lockout`, `mock`, `dialog`,
/// `zoom`, `fit`, `key`, `simulate`, `sidebar`. `unlock` only works against the mock engine, whose
/// password is fixed; the real engine is never unlocked from here.
///
/// `simulate state=<name>` is mock-only. Names: `empty-flows`, `empty-remotes`,
/// `loading`, `error-engine`, `error-test`, `empty-history`, `empty-logs`,
/// `wrong-password`, `reset`. `state.simulation` reports the active name, or
/// `none` after reset. `scripts/dev.sh` help should list the same command.
///
/// `inspector open=true|false` shows the inspector on a compact window.
/// `inspector scroll=<points>|top|end` moves `inspector-scroll` and replies
/// `{"y","max"}`. `inspector section=<name> open=true|false` expands or
/// collapses one block (`basics`, `schedule`, `what`, `safety`, `advanced`,
/// `connections`, `danger`). `inspector width=<points>` sets the width and
/// replies `{"inspectorWidth": <clamped>}`. `sidebar width=<points>` sets the
/// flow-list width and replies `{"sidebarWidth": <clamped>}`.
#[cfg(feature = "devtools")]
impl ns_ui::DevControl for GnDriveWindow {
    fn spec() -> ns_ui::DevSpec {
        ns_ui::DevSpec {
            commands: dev_commands(),
            state_schema: gn_state_schema(),
        }
    }

    fn dev(
        &mut self,
        command: &str,
        args: &Value,
        cx: &mut Context<Self>,
    ) -> Result<Value, String> {
        let text = |key: &str| {
            args.get(key).and_then(|value| match value {
                Value::String(text) => Some(text.clone()),
                Value::Number(number) => Some(number.to_string()),
                _ => None,
            })
        };
        if perf::running() && command != "state" {
            return Err("a perf run is active; wait for its report".into());
        }
        match command {
            "perf" => {
                let (label, frames) = perf::args(args)?;
                perf::start(self, &label, frames, cx)
            }
            "state" => Ok(gn_dev_state_value(&gn_dev_view(self))),
            "mock" => {
                self.engine = Engine::start(true);
                self.snapshot = None;
                self.refresh_snapshot_caches();
                self.started = false;
                self.snapshot_pending = false;
                self.selected_flow = None;
                self.draft = None;
                self.graph = None;
                self.touch_graph();
                self.pending.clear();
                self.load_error = None;
                self.link = LinkStatus::Starting;
                cx.notify();
                Ok(json!({"mock": true}))
            }
            "unlock" | "lock" => {
                if !self.is_mock() {
                    return Err("unlock and lock are mock-only dev commands".into());
                }
                if command == "unlock" {
                    self.send(
                        "auth.unlock",
                        json!({"password": "drive1234"}),
                        Pending::Auth,
                        cx,
                    );
                } else {
                    self.send(
                        "auth.lock",
                        json!({}),
                        Pending::Generic {
                            label: "Lock".into(),
                        },
                        cx,
                    );
                }
                Ok(json!({"requested": command}))
            }
            "lockout" => {
                let secs = args
                    .get("seconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(30)
                    .min(3600);
                self.clear_auth_error();
                self.auth_busy = false;
                if secs == 0 {
                    self.lockout_until = None;
                } else {
                    self.lockout_until = Some(Instant::now() + Duration::from_secs(secs));
                    self.last_lock_second = secs;
                }
                if self.is_mock() {
                    self.send(
                        "dev.lockout",
                        json!({ "seconds": secs }),
                        Pending::Generic {
                            label: "Lockout".into(),
                        },
                        cx,
                    );
                }
                cx.notify();
                Ok(json!({ "seconds": secs }))
            }
            "flow" => {
                let flows = self.flows();
                let flow = if let Some(id) = text("id") {
                    flows.iter().find(|f| f.id == id)
                } else {
                    let index = args.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                    flows.get(index)
                }
                .ok_or("no such flow")?;
                let id = flow.id.clone();
                self.panel = Panel::Closed;
                self.overlay = Overlay::None;
                self.ask = None;
                self.pick_flow(id.clone(), cx);
                if args.get("back").and_then(Value::as_bool) == Some(true) {
                    ns_ui::show_nav();
                }
                Ok(json!({"flow": id}))
            }
            "select" => {
                let graph = self.graph.clone().ok_or("no flow is open")?;
                if let Some(id) = text("node") {
                    let id = if id.parse::<usize>().is_ok() {
                        graph
                            .nodes
                            .get(id.parse::<usize>().unwrap_or(0))
                            .map(|n| n.id.clone())
                            .ok_or("no such node")?
                    } else {
                        id
                    };
                    self.select_node(id.clone(), cx);
                    Ok(json!({"node": id}))
                } else if let Some(id) = text("edge") {
                    let id = if id.parse::<usize>().is_ok() {
                        graph
                            .edges
                            .get(id.parse::<usize>().unwrap_or(0))
                            .map(|e| e.id.clone())
                            .ok_or("no such edge")?
                    } else {
                        id
                    };
                    self.select_edge(id.clone(), cx);
                    Ok(json!({"edge": id}))
                } else {
                    self.selection = Selection::Flow;
                    self.mobile_inspector = false;
                    cx.notify();
                    Ok(json!({"selection": "flow"}))
                }
            }
            "inspector" => self.inspector_command(args, cx),
            "sidebar" => {
                let width = args.get("width").ok_or("width is a point count")?;
                self.sidebar_set_width(width, cx)
            }
            "remotes" => {
                self.panel = Panel::Closed;
                self.set_panel(Panel::Remotes, cx);
                Ok(json!({"panel": "remotes"}))
            }
            "settings" => {
                self.settings_tab = SettingsTab::from_key(&text("tab").unwrap_or_default());
                self.panel = Panel::Closed;
                self.set_panel(Panel::Settings, cx);
                Ok(json!({"panel": "settings", "tab": self.settings_tab.key()}))
            }
            "panel" => {
                if text("name").as_deref() == Some("closed") || text("name").is_none() {
                    self.panel = Panel::Closed;
                    cx.notify();
                }
                Ok(json!({"panel": format!("{:?}", self.panel)}))
            }
            "dialog" => {
                let name = text("name").ok_or("dialog needs name=new-flow|add-remote|oauth|browse|change-password|remove-password|confirm-delete-flow|confirm-lock|close")?;
                match name.as_str() {
                    "close" => {
                        self.ask = None;
                        self.close_overlay(cx);
                    }
                    "new-flow" => self.new_flow(cx),
                    "add-remote" => self.open_add_remote(cx),
                    "oauth" => {
                        if !self.is_mock() {
                            return Err("the oauth dialog is mock-only from dev commands".into());
                        }
                        self.oauth_url = "https://accounts.example.invalid/authorize?mock=1&redirect=https://example.invalid/gn-drive/oauth/callback/family-archive-drive-from-the-old-laptop&scope=drive.readonly".into();
                        self.oauth_session = "mock-session".into();
                        self.oauth_type = "drive".into();
                        self.oauth_status = "Waiting for the browser".into();
                        self.overlay = Overlay::OAuth;
                        cx.notify();
                    }
                    "browse" => {
                        let start = text("path").unwrap_or_default();
                        let node = self
                            .graph
                            .as_ref()
                            .and_then(|g| g.nodes.first())
                            .map(|n| n.id.clone())
                            .ok_or("open a flow first")?;
                        self.open_browse(BrowseTarget::NodePath(node), start, cx);
                    }
                    "change-password" => {
                        self.password_error = None;
                        self.overlay = Overlay::ChangePassword;
                        cx.notify();
                    }
                    "remove-password" => {
                        self.password_error = None;
                        self.overlay = Overlay::RemovePassword;
                        cx.notify();
                    }
                    "confirm-delete-flow" => {
                        let id = self.selected_flow.clone().ok_or("open a flow first")?;
                        self.delete_selected_flow(cx);
                        let _ = id;
                    }
                    "confirm-lock" => {
                        self.ask = Some((
                            Ask {
                                title: "Lock GN Drive?".into(),
                                message: "Flows hide until the master password is entered again."
                                    .into(),
                                confirm: "Lock".into(),
                                confirm_icon: "eye-off",
                                destructive: false,
                            },
                            AskAction::Lock,
                        ));
                        cx.notify();
                    }
                    other => return Err(format!("unknown dialog {other}")),
                }
                Ok(json!({"dialog": name}))
            }
            "zoom" => {
                let value = args.get("value").and_then(Value::as_f64).unwrap_or(1.0) as f32;
                self.canvas.set_zoom(value, None);
                self.sync_canvas_only();
                cx.notify();
                Ok(json!({"zoom": self.canvas.zoom}))
            }
            "fit" => {
                self.fit_canvas(cx);
                Ok(json!({"zoom": self.canvas.zoom}))
            }
            "key" => {
                let key = text("key").ok_or("key needs key=escape, cmd-n, …")?;
                gpui::Keystroke::parse(&key).map_err(|error| error.to_string())?;
                self.dev_key = Some(key.clone());
                cx.notify();
                Ok(json!({"key": key}))
            }
            "simulate" => {
                if !self.is_mock() {
                    return Err("simulate is a mock-only dev command".into());
                }
                let name = text("state").ok_or(
                    "state must be empty-flows, empty-remotes, loading, error-engine, error-test, empty-history, empty-logs, wrong-password, or reset",
                )?;
                let kind = states::Simulation::parse(&name).ok_or_else(|| {
                    format!(
                        "unknown state {name}. Use empty-flows, empty-remotes, loading, error-engine, error-test, empty-history, empty-logs, wrong-password, or reset"
                    )
                })?;
                self.apply_simulation(kind, cx);
                Ok(json!({"simulation": self.simulation.key()}))
            }
            other => Err(reject_unknown(other)),
        }
    }
}

#[cfg(feature = "devtools")]
impl GnDriveWindow {
    /// Show a screenshot state without changing the mock engine's saved data.
    /// `reset` (`Simulation::None`) drops the preview.
    fn apply_simulation(&mut self, kind: states::Simulation, cx: &mut Context<Self>) {
        if self.auth_error_simulated {
            self.clear_auth_error();
        }
        self.simulation = kind;
        self.overlay = Overlay::None;
        self.ask = None;
        match kind {
            states::Simulation::None => {}
            states::Simulation::EmptyFlows => {
                self.panel = Panel::Closed;
            }
            states::Simulation::EmptyRemotes | states::Simulation::ErrorTest => {
                self.panel = Panel::Remotes;
                self.load_providers(cx);
                ns_ui::show_detail();
            }
            states::Simulation::Loading => {}
            states::Simulation::ErrorEngine => {}
            states::Simulation::EmptyHistory => {
                self.panel = Panel::Settings;
                self.settings_tab = SettingsTab::History;
                self.load_settings_tab(cx);
                ns_ui::show_detail();
            }
            states::Simulation::EmptyLogs => {
                self.panel = Panel::Settings;
                self.settings_tab = SettingsTab::Logs;
                self.load_settings_tab(cx);
                ns_ui::show_detail();
            }
            states::Simulation::WrongPassword => {
                let explained = format::explain("invalid_password", "auth: invalid password");
                self.auth_error = Some(explained.text);
                self.auth_error_code = Some(explained.code).filter(|code| !code.is_empty());
                self.auth_error_simulated = true;
                self.auth_busy = false;
                self.lockout_until = None;
            }
        }
        cx.notify();
    }

    /// `scroll` moves the inspector body. `section` plus `open` expands one
    /// block. `width` sets the panel width. `open` alone is the compact-window
    /// visibility toggle.
    fn inspector_command(&mut self, args: &Value, cx: &mut Context<Self>) -> Result<Value, String> {
        if let Some(scroll) = args.get("scroll") {
            return self.inspector_scroll_to(scroll, cx);
        }
        if args.get("section").is_some() {
            return self.inspector_section(args, cx);
        }
        if let Some(width) = args.get("width") {
            return self.inspector_set_width(width, cx);
        }
        let open = args.get("open").and_then(Value::as_bool).unwrap_or(true);
        self.mobile_inspector = open;
        if open && ns_ui::is_compact() {
            ns_ui::show_detail();
        }
        cx.notify();
        Ok(json!({"inspector": open}))
    }

    fn inspector_scroll_to(
        &mut self,
        scroll: &Value,
        cx: &mut Context<Self>,
    ) -> Result<Value, String> {
        let max = f32::from(self.inspector_scroll.max_offset().y).max(0.0);
        let y = match scroll {
            Value::String(text) => match text.as_str() {
                "top" => 0.0,
                "end" => max,
                other => {
                    return Err(format!("scroll is a point count, top, or end, not {other}"));
                }
            },
            Value::Number(number) => {
                let y = number
                    .as_f64()
                    .filter(|y| y.is_finite())
                    .ok_or("scroll is not a finite number")?;
                y as f32
            }
            _ => return Err("scroll is a point count, top, or end".into()),
        };
        let y = y.clamp(0.0, max);
        self.inspector_scroll
            .set_offset(gpui::point(px(0.), px(-y)));
        cx.notify();
        Ok(json!({"y": y, "max": max}))
    }

    fn inspector_section(&mut self, args: &Value, cx: &mut Context<Self>) -> Result<Value, String> {
        let name = match args.get("section") {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Number(number)) => number.to_string(),
            _ => return Err("section needs a name such as advanced".into()),
        };
        let section = crate::inspector::section_from_name(&name).ok_or_else(|| {
            format!(
                "unknown inspector section {name}. Use basics, schedule, what, safety, advanced, connections, or danger"
            )
        })?;
        let Some(open) = args.get("open").and_then(Value::as_bool) else {
            return Err("section needs open=true or open=false".into());
        };
        self.inspector_open.insert(section, open);
        cx.notify();
        Ok(json!({
            "section": crate::inspector::section_key(section),
            "open": open,
        }))
    }

    fn inspector_set_width(
        &mut self,
        width: &Value,
        cx: &mut Context<Self>,
    ) -> Result<Value, String> {
        let requested = match width {
            Value::Number(number) => number
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or("width is not a finite number")? as f32,
            Value::String(text) => text
                .trim()
                .parse::<f32>()
                .map_err(|_| format!("width is a point count, not {text}"))?,
            _ => return Err("width is a point count".into()),
        };
        if !requested.is_finite() {
            return Err("width is not a finite number".into());
        }
        let width = inspector::clamp_inspector_width(requested, ns_ui::body_width());
        self.inspector_width = width;
        self.inspector_drag = None;
        prefs::set_inspector_width(width);
        cx.notify();
        Ok(json!({"inspectorWidth": width}))
    }

    fn sidebar_set_width(
        &mut self,
        width: &Value,
        cx: &mut Context<Self>,
    ) -> Result<Value, String> {
        let requested = match width {
            Value::Number(number) => number
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or("width is not a finite number")? as f32,
            Value::String(text) => text
                .trim()
                .parse::<f32>()
                .map_err(|_| format!("width is a point count, not {text}"))?,
            _ => return Err("width is a point count".into()),
        };
        if !requested.is_finite() {
            return Err("width is not a finite number".into());
        }
        let width = workspace::clamp_sidebar_width(requested, ns_ui::viewport_width());
        self.sidebar_width = width;
        self.sidebar_drag = None;
        prefs::set_sidebar_width(width);
        cx.notify();
        Ok(json!({"sidebarWidth": width}))
    }
}

// Source-scanning tests treat the first cfg(test) as the production boundary.
#[cfg(test)]
mod bench;

#[cfg(all(test, feature = "devtools"))]
mod dev_contract_tests {
    use super::{
        DEV_COMMANDS, GnDevView, GnDriveWindow, LinkStatus, Panel, any_busy, engine_status_word,
        gn_dev_state_value, gn_screen, next_panel, page_key, panel_after_flow_pick, reject_unknown,
        status_meta,
    };
    use ns_ui::DevControl;
    use serde_json::json;

    #[test]
    fn spec_names_match_the_command_list() {
        let spec = GnDriveWindow::spec();
        let names: Vec<&str> = spec
            .commands
            .iter()
            .map(|command| command.name.as_str())
            .collect();
        assert_eq!(names, DEV_COMMANDS);
    }

    #[test]
    fn an_unlisted_command_points_at_help() {
        assert_eq!(reject_unknown("nope"), ns_ui::unknown_command("nope"));
        assert!(reject_unknown("state").contains("listed"));
    }

    #[test]
    fn argument_kinds_are_protocol_kinds() {
        for command in GnDriveWindow::spec().commands {
            for arg in command.args {
                assert!(
                    ns_ui::ARG_KINDS.contains(&arg.kind.as_str()),
                    "{} {}",
                    command.name,
                    arg.kind
                );
            }
        }
    }

    #[test]
    fn state_schema_requires_the_contract_keys() {
        let schema = GnDriveWindow::spec().state_schema;
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().expect("required");
        for key in ["protocol", "app", "screen", "selection", "busy"] {
            assert!(required.iter().any(|item| item == key), "{key}");
        }
    }

    #[test]
    fn screen_follows_the_engine_then_the_panel() {
        assert_eq!(gn_screen(true, true, false, false, Panel::Closed), "fatal");
        assert_eq!(gn_screen(false, false, true, false, Panel::Closed), "error");
        assert_eq!(
            gn_screen(false, false, false, false, Panel::Closed),
            "starting"
        );
        assert_eq!(
            gn_screen(false, true, false, true, Panel::Remotes),
            "unlock"
        );
        assert_eq!(
            gn_screen(false, true, false, false, Panel::Remotes),
            "remotes"
        );
        assert_eq!(
            gn_screen(false, true, false, false, Panel::Settings),
            "settings"
        );
        assert_eq!(
            gn_screen(false, true, false, false, Panel::Closed),
            "workspace"
        );
    }

    #[test]
    fn busy_is_any_in_flight_flag_or_a_pending_request() {
        assert!(!any_busy(&[false, false], 0));
        assert!(any_busy(&[false, true], 0));
        assert!(any_busy(&[false, false], 1));
    }

    #[test]
    fn a_fixture_state_keeps_old_keys_and_the_contract() {
        let mut view = GnDevView {
            mock: true,
            link: "Connected".into(),
            locked: false,
            flows: vec![json!({"id": "flow-1", "name": "Family"})],
            remotes: 2,
            selected_flow: Some("flow-1".into()),
            flow_name: Some("Family".into()),
            runtime: vec![json!({"id": "flow-1", "status": "idle", "error": false})],
            inspector: "Node(\"n1\")".into(),
            panel: "Closed".into(),
            settings_tab: "general".into(),
            overlay: false,
            ask: false,
            dirty: true,
            toolbar: vec!["add-folder".into(), "run".into()],
            zoom: 1.25,
            canvas_size: (800.0, 600.0),
            pan: [10.0, 20.0],
            fit_size: None,
            nodes: Some(vec![json!([1.0, 2.0])]),
            compact: false,
            screen: "workspace".into(),
            page: None,
            busy: true,
            inspector_width: 320.0,
            sidebar_width: 260.0,
            simulation: "none".into(),
        };
        let stamped = ns_ui::stamp_state(gn_dev_state_value(&view), "gn drive");
        assert_eq!(stamped["mock"], true);
        assert_eq!(stamped["link"], "Connected");
        assert_eq!(stamped["locked"], false);
        assert_eq!(stamped["flows"][0]["id"], "flow-1");
        assert_eq!(stamped["remotes"], 2);
        assert_eq!(stamped["selectedFlow"], "flow-1");
        assert_eq!(stamped["runtime"][0]["status"], "idle");
        assert_eq!(stamped["selection"]["flow"], "flow-1");
        assert_eq!(stamped["selection"]["name"], "Family");
        assert_eq!(stamped["selection"]["inspector"], "Node(\"n1\")");
        assert!(stamped["selection"].is_object());
        assert_eq!(stamped["panel"], "Closed");
        assert_eq!(stamped["settingsTab"], "general");
        assert_eq!(stamped["overlay"], false);
        assert_eq!(stamped["ask"], false);
        assert_eq!(stamped["dirty"], true);
        assert_eq!(stamped["toolbar"][0], "add-folder");
        assert_eq!(stamped["zoom"], 1.25);
        assert_eq!(stamped["canvas"]["size"], json!([800.0, 600.0]));
        assert_eq!(stamped["canvas"]["pan"], json!([10.0, 20.0]));
        assert!(stamped["canvas"]["fitSize"].is_null());
        assert_eq!(stamped["nodes"][0], json!([1.0, 2.0]));
        assert_eq!(stamped["compact"], false);
        assert_eq!(stamped["screen"], "workspace");
        assert!(stamped["page"].is_null());
        assert_eq!(stamped["busy"], true);
        assert_eq!(stamped["inspectorWidth"], 320.0);
        assert_eq!(stamped["sidebarWidth"], 260.0);
        assert_eq!(stamped["simulation"], "none");
        let schema = GnDriveWindow::spec().state_schema;
        let required = schema["required"].as_array().expect("required");
        assert!(required.iter().any(|item| item == "sidebarWidth"));
        assert_eq!(schema["properties"]["sidebarWidth"]["type"], "number");
        assert_eq!(stamped["protocol"], 1);
        assert_eq!(stamped["app"], "gn drive");

        view.selected_flow = None;
        view.flow_name = None;
        view.screen = "starting".into();
        view.busy = false;
        let empty = ns_ui::stamp_state(gn_dev_state_value(&view), "gn drive");
        assert!(empty["selectedFlow"].is_null());
        assert!(empty["selection"]["flow"].is_null());
        assert!(empty["selection"].is_object());
        assert_eq!(empty["screen"], "starting");
        assert!(empty["page"].is_null());
        assert_eq!(empty["busy"], false);

        view.page = Some("remotes".into());
        view.screen = "remotes".into();
        let open = ns_ui::stamp_state(gn_dev_state_value(&view), "gn drive");
        assert_eq!(open["page"], "remotes");
        assert_eq!(open["screen"], "remotes");
    }

    #[test]
    fn nav_rows_toggle_and_a_flow_row_closes_the_page() {
        assert_eq!(next_panel(Panel::Closed, Panel::Remotes), Panel::Remotes);
        assert_eq!(next_panel(Panel::Remotes, Panel::Remotes), Panel::Closed);
        assert_eq!(next_panel(Panel::Remotes, Panel::Settings), Panel::Settings);
        assert_eq!(next_panel(Panel::Settings, Panel::Settings), Panel::Closed);
        assert_eq!(next_panel(Panel::Settings, Panel::Remotes), Panel::Remotes);
        assert_eq!(next_panel(Panel::Closed, Panel::Closed), Panel::Closed);

        for panel in [Panel::Closed, Panel::Remotes, Panel::Settings] {
            assert_eq!(panel_after_flow_pick(panel), Panel::Closed);
        }

        assert_eq!(page_key(Panel::Remotes), Some("remotes"));
        assert_eq!(page_key(Panel::Settings), Some("settings"));
        assert_eq!(page_key(Panel::Closed), None);
        assert_eq!(
            gn_screen(false, true, false, false, Panel::Remotes),
            page_key(Panel::Remotes).unwrap()
        );
        assert_eq!(
            gn_screen(false, true, false, false, Panel::Settings),
            page_key(Panel::Settings).unwrap()
        );
    }

    #[test]
    fn the_status_bar_says_one_thing_about_the_engine() {
        let ready = LinkStatus::Ready;
        assert_eq!(
            engine_status_word(false, false, false, &ready, "Unlocked"),
            "Ready"
        );
        assert_eq!(
            engine_status_word(false, true, false, &ready, "Syncing"),
            "Syncing"
        );
        assert_eq!(
            engine_status_word(false, true, true, &ready, "Stopping"),
            "Stopping"
        );
        assert_eq!(
            engine_status_word(true, true, true, &ready, "Ready"),
            "Locked"
        );
        assert_eq!(
            engine_status_word(false, false, false, &ready, "Locked"),
            "Locked"
        );
        assert_eq!(
            engine_status_word(false, false, false, &LinkStatus::Starting, ""),
            "Starting"
        );
        assert_eq!(
            engine_status_word(
                false,
                false,
                false,
                &LinkStatus::Fatal {
                    code: "x".into(),
                    message: "no".into(),
                    holder: None,
                },
                "Error"
            ),
            "Error"
        );
        assert_eq!(
            engine_status_word(
                false,
                false,
                false,
                &LinkStatus::Restarting {
                    delay: 2,
                    reason: "dropped".into(),
                },
                "Reconnecting"
            ),
            "Reconnecting"
        );
        assert_eq!(
            engine_status_word(false, false, false, &LinkStatus::Stopped, "Stopped"),
            "Stopped"
        );

        assert_eq!(status_meta("0.4.0", "v1.75.0"), "v0.4.0 · rclone v1.75.0");
        assert_eq!(
            status_meta("v0.4.0", "rclone v1.75.0"),
            "v0.4.0 · rclone v1.75.0"
        );
        assert_eq!(status_meta("—", ""), "");
        assert_eq!(status_meta("", "1.75.0"), "rclone 1.75.0");
        assert_eq!(status_meta("0.4.0", ""), "v0.4.0");
    }
}
