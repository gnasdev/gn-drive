use std::cell::RefCell;
use std::time::{Duration, Instant};

use gpui::{AppContext as _, Context, point, px};
use serde_json::{Value, json};

use crate::{GnDriveWindow, Overlay, Panel, Selection, SettingsTab, graph};

pub(crate) const SCENARIOS: &[&str] = &[
    "idle-overview",
    "scroll-list",
    "select-item",
    "graph-pan-zoom",
    "page-switch",
    "search-typing",
];

#[derive(Default)]
struct Probe {
    running: bool,
    recording: bool,
    renders: u64,
    render_us: Vec<f64>,
}

thread_local! {
    static PROBE: RefCell<Probe> = RefCell::new(Probe::default());
}

pub(crate) struct Clock(Instant);

impl Drop for Clock {
    fn drop(&mut self) {
        PROBE.with(|probe| {
            let mut probe = probe.borrow_mut();
            probe.renders += 1;
            if probe.recording {
                probe
                    .render_us
                    .push(self.0.elapsed().as_secs_f64() * 1_000_000.0);
            }
        });
    }
}

pub(crate) fn clock() -> Clock {
    Clock(Instant::now())
}

pub(crate) fn running() -> bool {
    PROBE.with(|probe| probe.borrow().running)
}

struct Run;

impl Drop for Run {
    fn drop(&mut self) {
        PROBE.with(|probe| {
            let mut probe = probe.borrow_mut();
            probe.running = false;
            probe.recording = false;
            probe.render_us.clear();
        });
    }
}

fn renders() -> u64 {
    PROBE.with(|probe| probe.borrow().renders)
}

fn record(on: bool) -> Vec<f64> {
    PROBE.with(|probe| {
        let mut probe = probe.borrow_mut();
        probe.recording = on;
        std::mem::take(&mut probe.render_us)
    })
}

fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values
        .get((values.len() as f64 * fraction) as usize)
        .copied()
        .unwrap_or(0.0)
}

fn round(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

pub(crate) fn args(args: &Value) -> Result<(String, usize), String> {
    let label = match args.get("label") {
        None => "run",
        Some(value) => value.as_str().ok_or("label must be a string")?,
    };
    if label.is_empty()
        || label.len() > 80
        || !label
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return Err("label must contain 1–80 letters, digits, hyphens or underscores".into());
    }
    let frames = match args.get("frames") {
        None => 60,
        Some(value) => value
            .as_u64()
            .ok_or("frames must be an integer from 1 to 1000")?,
    };
    if !(1..=1000).contains(&frames) {
        return Err("frames must be an integer from 1 to 1000".into());
    }
    Ok((label.to_string(), frames as usize))
}

fn ready(window: &GnDriveWindow) -> Result<(), String> {
    if !window.is_mock() {
        return Err("perf requires the mock engine (GN_DRIVE_MOCK=1)".into());
    }
    if !window.link.is_ready() || window.snapshot.is_none() || window.locked() {
        return Err("wait for the mock snapshot and unlock the mock before running perf".into());
    }
    if window.dirty
        || window.viewport_dirty
        || window.layout_save_at.is_some()
        || window.layout_saving
        || !window.pending.is_empty()
        || window.reload_at.is_some()
        || window.snapshot_pending
        || window.any_running()
        || window.busy
        || window.oauth_busy
        || window.auth_busy
    {
        return Err("perf requires an idle mock with no pending requests or unsaved edits".into());
    }
    if window.overlay != Overlay::None
        || window.ask.is_some()
        || window.sidebar_drag.is_some()
        || window.inspector_drag.is_some()
        || window.canvas.dragging.is_some()
        || window.canvas.panning.is_some()
        || window.canvas.connect_drag.is_some()
        || window.dev_key.is_some()
        || window.flow_menu.is_some()
        || window.connect_from.is_some()
        || window.simulation != crate::states::Simulation::None
    {
        return Err("close dialogs, finish gestures and reset simulations before perf".into());
    }
    Ok(())
}

fn longest_list(window: &GnDriveWindow) -> (Panel, SettingsTab, &'static str, usize) {
    let snapshot = window.snapshot.as_ref();
    [
        (
            Panel::Closed,
            SettingsTab::General,
            "flows",
            snapshot.map_or(0, |s| s.flows.len()),
        ),
        (
            Panel::Remotes,
            SettingsTab::General,
            "remotes",
            snapshot.map_or(0, |s| s.remotes.len()),
        ),
        (
            Panel::Settings,
            SettingsTab::History,
            "history",
            window.history.len(),
        ),
        (
            Panel::Settings,
            SettingsTab::Logs,
            "logs",
            window.logs.as_ref().map_or(0, |logs| logs.lines.len()),
        ),
    ]
    .into_iter()
    .max_by_key(|item| item.3)
    .unwrap()
}

fn step(window: &mut GnDriveWindow, name: &str, frame: usize, cx: &mut Context<GnDriveWindow>) {
    match name {
        "idle-overview" => {
            window.panel = Panel::Closed;
            window.selection = Selection::Flow;
            window.mobile_inspector = false;
        }
        "scroll-list" => {
            let (panel, tab, _, _) = longest_list(window);
            window.panel = panel;
            window.settings_tab = tab;
            let max = f32::from(window.perf_list_scroll.max_offset().y).max(0.0);
            let y = (frame as f32 * 160.0) % (max + 1.0);
            window.perf_list_scroll.set_offset(point(px(0.0), px(-y)));
        }
        "select-item" => {
            window.panel = Panel::Closed;
            window.mobile_inspector = true;
            if let Some(graph) = &window.graph {
                window.selection = if frame.is_multiple_of(2) && !graph.nodes.is_empty() {
                    Selection::Node(graph.nodes[(frame / 2) % graph.nodes.len()].id.clone())
                } else if !graph.edges.is_empty() {
                    Selection::Edge(graph.edges[(frame / 2) % graph.edges.len()].id.clone())
                } else {
                    Selection::Flow
                };
                window.card_dismissed = false;
            }
        }
        "graph-pan-zoom" => {
            window.panel = Panel::Closed;
            window.selection = Selection::None;
            window.mobile_inspector = false;
            window.canvas.pan_x = (frame % 12) as f32 * 24.0;
            window.canvas.pan_y = (frame % 10) as f32 * 32.0;
            window.canvas.zoom = [0.35, 0.5, 0.75, 1.0, 1.2][frame % 5];
        }
        "page-switch" => {
            let (panel, tab) = [
                (Panel::Closed, SettingsTab::General),
                (Panel::Remotes, SettingsTab::General),
                (Panel::Settings, SettingsTab::General),
                (Panel::Settings, SettingsTab::History),
                (Panel::Settings, SettingsTab::Logs),
            ][frame % 5];
            window.panel = panel;
            window.settings_tab = tab;
        }
        "search-typing" => {
            window.panel = Panel::Closed;
            window.search_open = true;
            let word = "archive";
            window.search_input.update(cx, |input, cx| {
                input.set_text(&word[..frame % (word.len() + 1)], cx)
            });
        }
        _ => unreachable!(),
    }
    cx.notify();
}

pub(crate) fn start(
    window: &mut GnDriveWindow,
    label: &str,
    frames: usize,
    cx: &mut Context<GnDriveWindow>,
) -> Result<Value, String> {
    ready(window)?;
    if window.editors_differ(cx) {
        return Err("finish or discard inspector edits before perf".into());
    }
    if running() {
        return Err("a perf run is already active".into());
    }
    let flow = window
        .snapshot
        .as_ref()
        .and_then(|snapshot| {
            snapshot
                .flows
                .iter()
                .max_by_key(|flow| flow.operations.len())
        })
        .cloned()
        .ok_or("the mock has no flows")?;
    let handle = cx.windows().into_iter().next().ok_or("no window")?;
    let (_, _, list_name, list_rows) = longest_list(window);
    let snapshot = window.snapshot.as_ref().unwrap();
    let counts = json!({
        "flows": snapshot.flows.len(), "remotes": snapshot.remotes.len(),
        "history": window.history.len(), "logs": window.logs.as_ref().map_or(0, |logs| logs.lines.len()),
        "transfers": window.runtime.flows.iter().filter_map(|flow| flow.sync.as_ref()).map(|sync| sync.transfers.len()).sum::<usize>(),
    });
    let path = std::env::temp_dir().join(format!("gn-drive-perf-{label}.json"));
    let out = path.clone();
    let label = label.to_string();
    PROBE.with(|probe| probe.borrow_mut().running = true);
    let guard = Run;
    cx.spawn(async move |this, cx| {
        let _guard = guard;
        let idle_from = renders();
        let idle_started = Instant::now();
        cx.background_executor().timer(Duration::from_secs(3)).await;
        let idle_renders = renders() - idle_from;
        let idle_ms = idle_started.elapsed().as_secs_f64() * 1000.0;
        let mut error = None;
        let saved = this.update(cx, |this, cx| {
            ready(this)?;
            if this.editors_differ(cx) {
                return Err("inspector edits changed during the idle phase".into());
            }
            let saved = (
                this.selected_flow.replace(flow.id.clone()),
                this.draft.replace(flow.clone()),
                this.graph.replace(graph::to_graph(&flow)),
                this.selection.clone(),
                this.panel,
                this.settings_tab,
                this.mobile_inspector,
                this.card_dismissed,
                this.canvas.pan_x,
                this.canvas.pan_y,
                this.canvas.zoom,
                this.canvas.fit_pending,
                this.canvas.auto_fit_size,
                this.search_open,
                this.search_input.read(cx).text().to_string(),
                this.perf_list_scroll.offset(),
                this.inspector_scroll.offset(),
            );
            this.canvas.fit_pending = false;
            this.canvas.auto_fit_size = None;
            this.canvas.pan_x = 0.0;
            this.canvas.pan_y = 0.0;
            this.canvas.zoom = 1.0;
            this.search_input
                .update(cx, |input, cx| input.set_text("", cx));
            this.form_flow = None;
            this.form_selection = None;
            Ok::<_, String>(saved)
        });
        let mut results = Vec::new();
        match saved {
            Ok(Ok(saved)) => {
                for name in SCENARIOS {
                    let mut frame_ms = Vec::with_capacity(frames);
                    record(true);
                    let started = Instant::now();
                    for frame in 0..frames {
                        let at = Instant::now();
                        let updated = this.update(cx, |this, cx| {
                            ready(this)?;
                            step(this, name, frame, cx);
                            Ok::<_, String>(())
                        });
                        match updated {
                            Ok(Ok(())) => {}
                            Ok(Err(message)) => {
                                error = Some(message);
                                break;
                            }
                            Err(err) => {
                                error = Some(err.to_string());
                                break;
                            }
                        }
                        let draw = cx.update(|app| {
                            app.update_window(handle, |_, window, cx| window.draw(cx).clear(cx))
                        });
                        if let Err(err) = draw {
                            error = Some(err.to_string());
                            break;
                        }
                        frame_ms.push(at.elapsed().as_secs_f64() * 1000.0);
                        cx.background_executor()
                            .timer(Duration::from_millis(1))
                            .await;
                    }
                    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                    let mut render_us = record(false);
                    let total: f64 = frame_ms.iter().sum();
                    results.push(json!({
                        "name": name, "frames": frame_ms.len(), "elapsed_ms": round(elapsed),
                        "fps": round(frame_ms.len() as f64 * 1000.0 / total.max(0.001)),
                        "frame_ms_median": round(percentile(&mut frame_ms, 0.5)),
                        "frame_ms_p95": round(percentile(&mut frame_ms, 0.95)),
                        "renders": render_us.len(),
                        "render_us_median": round(percentile(&mut render_us, 0.5)),
                    }));
                    if error.is_some() {
                        break;
                    }
                }
                let _ = this.update(cx, |this, cx| {
                    let (query, list_offset, inspector_offset);
                    (
                        this.selected_flow,
                        this.draft,
                        this.graph,
                        this.selection,
                        this.panel,
                        this.settings_tab,
                        this.mobile_inspector,
                        this.card_dismissed,
                        this.canvas.pan_x,
                        this.canvas.pan_y,
                        this.canvas.zoom,
                        this.canvas.fit_pending,
                        this.canvas.auto_fit_size,
                        this.search_open,
                        query,
                        list_offset,
                        inspector_offset,
                    ) = saved;
                    this.search_input
                        .update(cx, |input, cx| input.set_text(query, cx));
                    this.perf_list_scroll.set_offset(list_offset);
                    this.inspector_scroll.set_offset(inspector_offset);
                    this.form_flow = None;
                    this.form_selection = None;
                    cx.notify();
                });
            }
            Ok(Err(message)) => error = Some(message),
            Err(err) => error = Some(err.to_string()),
        }
        let report = json!({
            "label": label, "mock": true,
            "profile": if cfg!(debug_assertions) { "dev" } else { "release" },
            "flow": flow.id, "operations": flow.operations.len(), "dataset": counts,
            "scroll_list": list_name, "scroll_rows": list_rows,
            "requested_frames": frames, "error": error,
            "idle": { "ms": round(idle_ms), "renders": idle_renders,
                "renders_per_s": round(idle_renders as f64 * 1000.0 / idle_ms.max(1.0)) },
            "scenarios": results,
        });
        match serde_json::to_vec_pretty(&report) {
            Ok(bytes) => {
                if let Err(err) = std::fs::write(&out, bytes) {
                    eprintln!("perf report {}: {err}", out.display());
                }
            }
            Err(err) => eprintln!("perf report: {err}"),
        }
    })
    .detach();
    Ok(json!({"started": true, "path": path.display().to_string(), "frames": frames}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_bounded_and_cannot_escape_temp_dir() {
        assert_eq!(args(&json!({})).unwrap(), ("run".into(), 60));
        assert_eq!(
            args(&json!({"label": "before-1", "frames": 1000})).unwrap(),
            ("before-1".into(), 1000)
        );
        for label in ["", "../outside", "a/b", "a\\b", "bad name"] {
            assert!(args(&json!({"label": label})).is_err());
        }
        for frames in [json!(0), json!(-1), json!(1001), json!(1.5), json!("60")] {
            assert!(args(&json!({"frames": frames})).is_err());
        }
    }

    #[test]
    fn probe_counts_idle_and_records_only_when_enabled() {
        PROBE.with(|probe| *probe.borrow_mut() = Probe::default());
        drop(clock());
        assert_eq!(renders(), 1);
        assert!(record(true).is_empty());
        drop(clock());
        assert_eq!(record(false).len(), 1);
        assert_eq!(renders(), 2);
        let mut values = [8.0, 1.0, 2.0, 3.0];
        assert_eq!(percentile(&mut values, 0.5), 3.0);
        assert_eq!(percentile(&mut values, 0.95), 8.0);
        assert_eq!(percentile(&mut [], 0.5), 0.0);
    }
}
