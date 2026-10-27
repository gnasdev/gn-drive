//! Mapping between a flow's operations and the canvas graph.
//!
//! Node ids are stable: `n_` + FNV-1a32 of `remote\0normalizedPath`. Operations
//! remain the source of truth for execution; the canvas only remembers where
//! each location sits.

use serde::{Deserialize, Serialize};

use crate::engine::{Flow, Operation};

/// Column pitch. Wider than [`NODE_W`] plus the connect ring, so a card and
/// the edge chip between two columns do not cover each other.
pub const COL_GAP: f32 = 480.0;
/// Row pitch. Taller than [`NODE_H`], so stacked cards do not touch.
pub const ROW_GAP: f32 = 160.0;
pub const NODE_W: f32 = 220.0;
pub const NODE_H: f32 = 72.0;

fn one() -> f64 {
    1.0
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Viewport {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default = "one")]
    pub zoom: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CanvasLocation {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub remote: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasModel {
    #[serde(default)]
    pub viewport: Viewport,
    #[serde(default)]
    pub nodes: Vec<CanvasLocation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphNode {
    pub id: String,
    pub remote: String,
    pub path: String,
    pub label: String,
    pub x: f32,
    pub y: f32,
}

impl GraphNode {
    pub fn key(&self) -> String {
        location_key(&self.remote, &self.path)
    }

    /// Name painted on the card: the label, or the last folder in the path.
    pub fn title(&self) -> String {
        folder_title(&self.label, &self.path)
    }
}

/// Title line. A real label wins. Otherwise the last folder name, so three
/// local folders do not all read "This Mac".
pub fn folder_title(label: &str, path: &str) -> String {
    let label = label.trim();
    if heading_readable(label) {
        return label.to_string();
    }
    let name = last_folder_name(path);
    if heading_readable(&name) {
        name
    } else {
        "Folder".into()
    }
}

/// Last path segment. `/Users/me/Pictures/Vacation` is `Vacation`.
pub fn last_folder_name(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(['/', '\\']);
    let segment = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed).trim();
    segment
        .rsplit(':')
        .next()
        .unwrap_or(segment)
        .trim()
        .to_string()
}

/// Second line. Local folders say "This Mac". A cloud folder uses the
/// account's friendly name when the caller has one.
pub fn place_name(remote: &str, friendly: &str) -> String {
    let remote = remote.trim();
    if remote.is_empty() || remote == "local" {
        return "This Mac".into();
    }
    let friendly = friendly.trim();
    if friendly.is_empty() {
        remote.to_string()
    } else {
        friendly.to_string()
    }
}

/// The three lines of a full card. `path_chars` is the middle-ellipsis budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeCardCopy {
    pub title: String,
    pub place: String,
    pub path: String,
}

pub fn node_card_copy(label: &str, path: &str, place: &str, path_chars: usize) -> NodeCardCopy {
    let raw = path.trim();
    let shown = if raw.is_empty() {
        String::new()
    } else {
        crate::format::middle_ellipsis(raw, path_chars)
    };
    NodeCardCopy {
        title: folder_title(label, path),
        place: {
            let place = place.trim();
            if place.is_empty() {
                "This Mac".into()
            } else {
                place.to_string()
            }
        },
        path: shown,
    }
}

/// How many characters of a path fit on a card of `card_w` points.
///
/// The budget leaves room for the icon, the status dot, and the padding.
/// A non-finite width uses the full card.
pub fn node_path_chars(card_w: f32) -> usize {
    let width = if card_w.is_finite() && card_w > 0.0 {
        card_w
    } else {
        NODE_W
    };
    let slots = ((width - 64.0) / 7.0).floor();
    let slots = if slots.is_finite() && slots > 0.0 {
        slots as usize
    } else {
        16
    };
    slots.clamp(16, 48)
}

#[derive(Clone, Debug)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub action: String,
    pub operation: Operation,
}

#[derive(Clone, Debug, Default)]
pub struct FlowGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub viewport: Viewport,
}

/// Cubic controls for an edge. The curve leaves the source and arrives at the target.
pub fn edge_controls(from: (f32, f32), to: (f32, f32)) -> ((f32, f32), (f32, f32)) {
    let dx = ((to.0 - from.0).abs() * 0.45).max(24.0);
    ((from.0 + dx, from.1), (to.0 - dx, to.1))
}

/// Point on a cubic Bézier at `t` in 0..=1.
pub fn cubic_at(
    t: f32,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
) -> (f32, f32) {
    let u = 1.0 - t;
    let a = u * u * u;
    let b = 3.0 * u * u * t;
    let c = 3.0 * u * t * t;
    let d = t * t * t;
    (
        a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
        a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
    )
}

/// True when `point` is within 10px of the curve (16 samples).
pub fn curve_hits(from: (f32, f32), to: (f32, f32), point: (f32, f32)) -> bool {
    let (c1, c2) = edge_controls(from, to);
    let mut best = f32::MAX;
    for step in 0..=16 {
        let t = step as f32 / 16.0;
        let at = cubic_at(t, from, c1, c2, to);
        let dx = at.0 - point.0;
        let dy = at.1 - point.1;
        best = best.min(dx * dx + dy * dy);
    }
    best < 100.0
}

pub fn normalize_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        "/".into()
    } else {
        path.to_string()
    }
}

pub fn location_key(remote: &str, path: &str) -> String {
    format!("{}\0{}", remote.trim(), normalize_path(path))
}

/// FNV-1a 32-bit, matching FlowGraph.swift and the TypeScript canvas.
pub fn stable_node_id(key: &str) -> String {
    let mut hash: u32 = 2_166_136_261;
    for byte in key.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    format!("n_{hash:x}")
}

pub fn parse_canvas(raw: &str) -> CanvasModel {
    serde_json::from_str(raw).unwrap_or_default()
}

pub fn serialize_canvas(canvas: &CanvasModel) -> String {
    serde_json::to_string(canvas).unwrap_or_else(|_| "{}".into())
}

pub fn default_layout(ops: &[Operation]) -> CanvasModel {
    let mut nodes: Vec<CanvasLocation> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut place = |remote: &str, path: &str, x: f32, y: f32| {
        let key = location_key(remote, path);
        if !seen.insert(key.clone()) {
            return;
        }
        nodes.push(CanvasLocation {
            id: stable_node_id(&key),
            remote: remote.trim().to_string(),
            path: normalize_path(path),
            label: String::new(),
            x: x as f64,
            y: y as f64,
        });
    };
    // A target sits one column right of its source, so a chain
    // (A to B, then B to C) reads left to right instead of stacking.
    let mut columns: std::collections::HashMap<String, f32> = Default::default();
    for (index, op) in ops.iter().enumerate() {
        let row = index as f32 * ROW_GAP;
        let source_key = location_key(&op.source_remote, &op.source_path);
        let target_key = location_key(&op.target_remote, &op.target_path);
        let source_col = *columns.entry(source_key).or_insert(0.0);
        place(
            &op.source_remote,
            &op.source_path,
            source_col * COL_GAP,
            row,
        );
        let target_col = columns
            .get(&target_key)
            .copied()
            .unwrap_or(source_col + 1.0);
        columns.entry(target_key).or_insert(target_col);
        place(
            &op.target_remote,
            &op.target_path,
            target_col * COL_GAP,
            row,
        );
    }
    CanvasModel {
        viewport: Viewport::default(),
        nodes,
    }
}

pub fn to_graph(flow: &Flow) -> FlowGraph {
    let ops = &flow.operations;
    let stored = parse_canvas(&flow.canvas_json);
    let mut by_key: std::collections::HashMap<String, GraphNode> = Default::default();
    let mut by_id: std::collections::HashMap<String, GraphNode> = Default::default();

    for node in &stored.nodes {
        let graph_node = GraphNode {
            id: if node.id.is_empty() {
                stable_node_id(&location_key(&node.remote, &node.path))
            } else {
                node.id.clone()
            },
            remote: node.remote.clone(),
            path: normalize_path(&node.path),
            label: node.label.clone(),
            x: node.x as f32,
            y: node.y as f32,
        };
        by_key.insert(graph_node.key(), graph_node.clone());
        by_id.insert(graph_node.id.clone(), graph_node);
    }

    let mut needed: Vec<String> = Vec::new();
    for op in ops {
        for key in [
            location_key(&op.source_remote, &op.source_path),
            location_key(&op.target_remote, &op.target_path),
        ] {
            if !needed.contains(&key) {
                needed.push(key);
            }
        }
    }

    if stored.nodes.is_empty() && !ops.is_empty() {
        for node in default_layout(ops).nodes {
            let graph_node = GraphNode {
                id: node.id,
                remote: node.remote,
                path: node.path,
                label: node.label,
                x: node.x as f32,
                y: node.y as f32,
            };
            by_key.insert(graph_node.key(), graph_node.clone());
            by_id.insert(graph_node.id.clone(), graph_node);
        }
    } else {
        let mut extra = 0.0f32;
        for key in needed {
            if by_key.contains_key(&key) {
                continue;
            }
            let mut parts = key.splitn(2, '\0');
            let remote = parts.next().unwrap_or_default().to_string();
            let path = parts.next().unwrap_or("/").to_string();
            let graph_node = GraphNode {
                id: stable_node_id(&key),
                remote,
                path,
                label: String::new(),
                x: COL_GAP,
                y: extra * ROW_GAP,
            };
            extra += 1.0;
            by_key.insert(graph_node.key(), graph_node.clone());
            by_id.insert(graph_node.id.clone(), graph_node);
        }
    }

    let mut edges = Vec::new();
    for op in ops {
        let source = by_key.get(&location_key(&op.source_remote, &op.source_path));
        let target = by_key.get(&location_key(&op.target_remote, &op.target_path));
        if let (Some(source), Some(target)) = (source, target) {
            edges.push(GraphEdge {
                id: op.id.clone(),
                source: source.id.clone(),
                target: target.id.clone(),
                action: normalize_action(&op.action),
                operation: op.clone(),
            });
        }
    }

    // Keep the stored order where possible; dictionary iteration in Swift was
    // unordered, so any stable order is compatible with the persisted canvas.
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut pushed = std::collections::HashSet::new();
    for saved in &stored.nodes {
        let id = if saved.id.is_empty() {
            stable_node_id(&location_key(&saved.remote, &saved.path))
        } else {
            saved.id.clone()
        };
        if let Some(node) = by_id.get(&id)
            && pushed.insert(id)
        {
            nodes.push(node.clone());
        }
    }
    for node in by_id.values() {
        if pushed.insert(node.id.clone()) {
            nodes.push(node.clone());
        }
    }

    FlowGraph {
        nodes,
        edges,
        viewport: stored.viewport,
    }
}

pub fn from_graph(graph: &FlowGraph, previous_ops: &[Operation]) -> (Vec<Operation>, String) {
    let nodes: std::collections::HashMap<&str, &GraphNode> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let previous: std::collections::HashMap<&str, &Operation> =
        previous_ops.iter().map(|op| (op.id.as_str(), op)).collect();
    let mut ops = Vec::new();
    for edge in graph.edges.iter().filter(|edge| {
        nodes.contains_key(edge.source.as_str()) && nodes.contains_key(edge.target.as_str())
    }) {
        let source = nodes[edge.source.as_str()];
        let target = nodes[edge.target.as_str()];
        let mut op = previous
            .get(edge.id.as_str())
            .map(|op| (*op).clone())
            .unwrap_or_else(|| edge.operation.clone());
        let action = if edge.action.is_empty() {
            normalize_action(&op.action)
        } else {
            normalize_action(&edge.action)
        };
        op.id = edge.id.clone();
        op.source_remote = source.remote.clone();
        op.source_path = normalize_path(&source.path);
        op.target_remote = target.remote.clone();
        op.target_path = normalize_path(&target.path);
        op.action = action.clone();
        op.sync_config
            .insert("action".into(), serde_json::json!(action));
        op.sort_order = ops.len() as i64;
        ops.push(op);
    }
    let canvas = CanvasModel {
        viewport: graph.viewport.clone(),
        nodes: graph
            .nodes
            .iter()
            .map(|node| CanvasLocation {
                id: node.id.clone(),
                remote: node.remote.clone(),
                path: normalize_path(&node.path),
                label: node.label.clone(),
                x: node.x as f64,
                y: node.y as f64,
            })
            .collect(),
    };
    (ops, serialize_canvas(&canvas))
}

pub fn normalize_action(action: &str) -> String {
    match action.trim() {
        "bi" => "bi".into(),
        "bi-resync" => "bi-resync".into(),
        _ => "push".into(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectError {
    MissingEndpoint,
    SelfLoop,
    UnknownNode,
    DuplicateEdge,
}

impl ConnectError {
    pub fn label(self) -> &'static str {
        match self {
            ConnectError::MissingEndpoint => "Cannot connect an empty endpoint",
            ConnectError::SelfLoop => "Cannot connect a node to itself",
            ConnectError::UnknownNode => "Cannot connect",
            ConnectError::DuplicateEdge => "This connection already exists",
        }
    }
}

pub fn connect_error(
    source: &str,
    target: &str,
    action: &str,
    graph: &FlowGraph,
) -> Option<ConnectError> {
    if source.is_empty() || target.is_empty() {
        return Some(ConnectError::MissingEndpoint);
    }
    if source == target {
        return Some(ConnectError::SelfLoop);
    }
    if !graph.nodes.iter().any(|node| node.id == source)
        || !graph.nodes.iter().any(|node| node.id == target)
    {
        return Some(ConnectError::UnknownNode);
    }
    let action = normalize_action(action);
    if graph.edges.iter().any(|edge| {
        edge.source == source && edge.target == target && normalize_action(&edge.action) == action
    }) {
        return Some(ConnectError::DuplicateEdge);
    }
    None
}

/// Back-to-front paint layers for the board canvas.
///
/// A later variant is appended after an earlier one, so it is drawn on top.
/// Popovers are last and sit above nodes, chrome, edge labels, and edge lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CanvasLayer {
    Edges,
    EdgeLabels,
    Nodes,
    Chrome,
    Popovers,
}

/// Indices that paint `layers` from back to front.
///
/// Equal layers keep their input order, so two edge labels stay in edge order.
pub fn canvas_child_order(layers: &[CanvasLayer]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..layers.len()).collect();
    order.sort_by(|&left, &right| layers[left].cmp(&layers[right]).then(left.cmp(&right)));
    order
}

/// Height used to flip and clamp the selected-edge file card.
pub const EDGE_POPOVER_EST_H: f32 = 200.0;

/// Below this zoom the edge chip is icon-only, even when the gap is wide.
pub const CHIP_ICON_ZOOM: f32 = 0.55;
/// Icon-only chip. Fixed so the glyph stays readable when the graph is small.
pub const CHIP_ICON_W: f32 = 24.0;
pub const CHIP_ICON_H: f32 = 22.0;
/// Room for the icon plus the longest short label ("Re-sync") at `text_xs`.
pub const CHIP_LABEL_MIN_W: f32 = 96.0;
/// Room for that label and one progress line, still at `text_xs`.
pub const CHIP_FULL_MIN_W: f32 = 132.0;
pub const CHIP_FULL_MAX_W: f32 = 156.0;

const _: () = {
    assert!(CHIP_LABEL_MIN_W > CHIP_ICON_W);
    assert!(CHIP_FULL_MIN_W > CHIP_LABEL_MIN_W);
    assert!(CHIP_FULL_MAX_W >= CHIP_FULL_MIN_W);
};

/// How much of an edge chip to draw. Text never scales below `text_xs`;
/// a chip that cannot fit the label becomes an icon instead of "…".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipMode {
    Full,
    Compact,
    IconOnly,
}

/// `IconOnly` when `zoom` is below [`CHIP_ICON_ZOOM`] or `available_w` is
/// narrower than the label. `Compact` is the label alone. `Full` adds the
/// progress line. Non-finite inputs are icon-only.
pub fn chip_mode(zoom: f32, available_w: f32) -> ChipMode {
    if !zoom.is_finite()
        || !available_w.is_finite()
        || zoom < CHIP_ICON_ZOOM
        || available_w < CHIP_LABEL_MIN_W
    {
        ChipMode::IconOnly
    } else if available_w < CHIP_FULL_MIN_W {
        ChipMode::Compact
    } else {
        ChipMode::Full
    }
}

/// Width of the chip for this zoom and gap. Icon-only is always [`CHIP_ICON_W`].
pub fn chip_draw_width(zoom: f32, available_w: f32) -> f32 {
    match chip_mode(zoom, available_w) {
        ChipMode::IconOnly => CHIP_ICON_W,
        ChipMode::Compact => available_w,
        ChipMode::Full => available_w.min(CHIP_FULL_MAX_W),
    }
}

/// Label painted in the chip. Icon-only and an ellipsis-only string paint nothing.
pub fn chip_caption(mode: ChipMode, label: &str) -> Option<&str> {
    if mode == ChipMode::IconOnly {
        return None;
    }
    let label = label.trim();
    if label.is_empty() || ellipsis_only(label) {
        None
    } else {
        Some(label)
    }
}

fn ellipsis_only(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty() && text.chars().all(|c| matches!(c, '…' | '.' | '·' | ' '))
}

/// Below this zoom a name cannot stay readable at `text_xs`, so the card is an icon.
pub const NODE_ICON_ZOOM: f32 = 0.40;
/// At or above this zoom, and with [`NODE_FULL_MIN_W`], the card shows the place
/// and the path. Below it, and still at or above [`NODE_ICON_ZOOM`], the card is
/// title-only. That band is the really small zoom: a compact fit never lands
/// there ([`FIT_ZOOM_MIN_COMPACT`] is this same floor).
pub const NODE_FULL_ZOOM: f32 = 0.62;
/// Lowest zoom [`choose_fit_zoom`] returns for a compact window. Tied to
/// [`NODE_FULL_ZOOM`] so the fitted cards still have room for place and path.
pub const FIT_ZOOM_MIN_COMPACT: f32 = NODE_FULL_ZOOM;
/// Lowest zoom a wide window will fit to. The user can still pan.
pub const FIT_ZOOM_MIN: f32 = 0.25;
/// Highest zoom a fit will choose. Zooming in further is a separate gesture.
pub const FIT_ZOOM_MAX: f32 = 1.2;
/// Padding reserved around a fitted graph on a wide window.
pub const FIT_PAD_WIDE: f32 = 120.0;
/// Padding reserved around a fitted graph on a compact window.
pub const FIT_PAD_COMPACT: f32 = 32.0;
/// Icon tile. Fixed so the glyph and the connect ring stay usable when the graph is small.
pub const NODE_ICON_W: f32 = 28.0;
pub const NODE_ICON_H: f32 = 28.0;
/// Provider glyph. `icon_tinted` draws 16px in every mode.
pub const NODE_GLYPH: f32 = 16.0;
/// Status dot fill. Icon-only wraps it in a canvas-coloured border.
pub const NODE_DOT: f32 = 7.0;
/// Icon-only status border, in the canvas background, so the dot separates from the tile.
pub const NODE_DOT_BORDER: f32 = 1.0;
/// Compact card never draws smaller than this, even when the name is one letter.
pub const NODE_COMPACT_MIN_W: f32 = 108.0;
/// Compact card never draws wider than this. A longer name clamps with `…`.
/// Stays inside a column gap at [`NODE_ICON_ZOOM`] (`COL_GAP` is 480px in
/// graph space, 192px on screen), so a card growing from its left anchor
/// does not cover the next column.
pub const NODE_COMPACT_MAX_W: f32 = 150.0;
pub const NODE_COMPACT_MIN_H: f32 = 28.0;
/// Width of the full card at [`NODE_FULL_ZOOM`]. Narrower hides the path.
pub const NODE_FULL_MIN_W: f32 = NODE_W * NODE_FULL_ZOOM;
/// A shortened name keeps at least this many letters before `…`.
pub const NODE_NAME_MIN_CHARS: usize = 6;
/// Connect ring. The same size in every mode, and never under 14px.
pub const NODE_HANDLE: f32 = 14.0;
/// Compact left padding (`pl` 6px).
pub const NODE_COMPACT_PAD_LEFT: f32 = 6.0;
/// Compact right padding. The ring hangs [`NODE_HANDLE`] / 2 over the right
/// edge; this is that radius plus a gap, so the name stops before the ring.
pub const NODE_COMPACT_PAD_RIGHT: f32 = 12.0;
/// Gap between compact-row children (`gap` 4px). Icon to name, and name to dot.
pub const NODE_COMPACT_GAP: f32 = 4.0;

/// Estimated advance of one `text_xs` character (12px). Capitals in the system
/// UI face are about this wide, so "Home NAS" and "Google Drive" fit the slot
/// the caption budget describes.
const COMPACT_CHAR_PX: f32 = 8.0;

const _: () = {
    assert!(NODE_FULL_ZOOM > NODE_ICON_ZOOM);
    assert!(NODE_COMPACT_MIN_W > NODE_ICON_W);
    assert!(NODE_COMPACT_MAX_W >= NODE_COMPACT_MIN_W);
    // The connect ring hangs half its diameter past the card. At the compact
    // zoom floor the next column is `COL_GAP * NODE_ICON_ZOOM` (192px) to the
    // right, and the ring must still clear it.
    assert!(NODE_COMPACT_MAX_W + NODE_HANDLE / 2.0 < COL_GAP * NODE_ICON_ZOOM);
    assert!(NODE_COMPACT_MIN_H >= NODE_ICON_H);
    assert!(NODE_HANDLE >= 14.0);
    assert!(NODE_NAME_MIN_CHARS >= 6);
    assert!(NODE_COMPACT_PAD_RIGHT >= NODE_HANDLE / 2.0 + 5.0);
    assert!(COL_GAP > NODE_W + NODE_HANDLE);
    assert!(ROW_GAP > NODE_H);
    assert!(NODE_DOT_BORDER >= 1.0);
    assert!(NODE_GLYPH > NODE_DOT);
    assert!(COMPACT_CHAR_PX >= 6.0);
};

/// How much of a node card to draw. Text never scales below `text_xs`.
/// A card that cannot show a readable name becomes an icon instead of `…`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeMode {
    Full,
    Compact,
    IconOnly,
}

/// `IconOnly` when `zoom` is below [`NODE_ICON_ZOOM`], `available_w` is narrower
/// than the icon tile, or `heading` has no readable letters. `Compact` is the
/// provider icon and a short name. `Full` adds the path. Non-finite inputs are
/// icon-only.
pub fn node_mode(zoom: f32, available_w: f32, heading: &str) -> NodeMode {
    if !zoom.is_finite()
        || !available_w.is_finite()
        || zoom < NODE_ICON_ZOOM
        || available_w < NODE_ICON_W
        || !heading_readable(heading)
    {
        NodeMode::IconOnly
    } else if zoom < NODE_FULL_ZOOM || available_w < NODE_FULL_MIN_W {
        NodeMode::Compact
    } else {
        NodeMode::Full
    }
}

/// Zoom that fits `graph_w` by `graph_h` into a view, then clamps it.
///
/// A compact window never goes below [`FIT_ZOOM_MIN_COMPACT`]. A graph that
/// cannot be readable at that floor stays at the floor and the user pans.
/// Non-finite sizes return the floor for that window.
pub fn choose_fit_zoom(view_w: f32, view_h: f32, graph_w: f32, graph_h: f32, compact: bool) -> f32 {
    let floor = if compact {
        FIT_ZOOM_MIN_COMPACT
    } else {
        FIT_ZOOM_MIN
    };
    if !view_w.is_finite() || !view_h.is_finite() || view_w <= 0.0 || view_h <= 0.0 {
        return floor;
    }
    let graph_w = if graph_w.is_finite() {
        graph_w.max(1.0)
    } else {
        1.0
    };
    let graph_h = if graph_h.is_finite() {
        graph_h.max(1.0)
    } else {
        1.0
    };
    let pad = if compact {
        FIT_PAD_COMPACT
    } else {
        FIT_PAD_WIDE
    };
    let raw = ((view_w - pad) / graph_w).min((view_h - pad) / graph_h);
    if !raw.is_finite() {
        return floor;
    }
    raw.clamp(floor, FIT_ZOOM_MAX)
}

fn heading_readable(heading: &str) -> bool {
    let heading = heading.trim();
    !heading.is_empty() && !ellipsis_only(heading)
}

/// Name painted on a compact card.
///
/// `max_chars` includes the ellipsis. A heading that already fits is returned
/// whole, including a one-letter name. A longer heading keeps at least
/// [`NODE_NAME_MIN_CHARS`] readable letters and then `…`. Empty, blank, and
/// ellipsis-only strings return `None`. A budget too small for that minimum
/// returns `None` instead of a bare ellipsis.
pub fn node_short_name(heading: &str, max_chars: usize) -> Option<String> {
    let heading = heading.trim();
    if !heading_readable(heading) {
        return None;
    }
    let chars: Vec<char> = heading.chars().collect();
    if chars.len() <= max_chars {
        return Some(heading.to_string());
    }
    if max_chars < NODE_NAME_MIN_CHARS + 1 {
        return None;
    }
    let mut keep = max_chars - 1;
    while keep > NODE_NAME_MIN_CHARS && chars[keep - 1].is_whitespace() {
        keep -= 1;
    }
    let mut out: String = chars[..keep].iter().collect();
    out.push('…');
    if ellipsis_only(&out) { None } else { Some(out) }
}

/// Pixels of a compact card that are not the name: left padding, glyph, gaps,
/// the status dot, and the 12px connect-ring reserve.
fn compact_name_chrome() -> f32 {
    NODE_COMPACT_PAD_LEFT + NODE_COMPACT_PAD_RIGHT + NODE_GLYPH + NODE_COMPACT_GAP * 2.0 + NODE_DOT
}

/// Width of a compact card for `heading`.
///
/// `icon + gaps + dot + estimated text width at text_xs + 12px ring reserve +
/// left padding`, clamped to [`NODE_COMPACT_MIN_W`, `NODE_COMPACT_MAX_W`].
/// The node's left edge stays put; the card grows to the right. Render, the
/// hit rect, and edge endpoints all use this width via [`node_draw_size`].
pub fn compact_card_width(heading: &str) -> f32 {
    let heading = heading.trim();
    let chars = if heading_readable(heading) {
        heading.chars().count()
    } else {
        0
    };
    let desired = compact_name_chrome() + chars as f32 * COMPACT_CHAR_PX;
    if !desired.is_finite() {
        return NODE_COMPACT_MIN_W;
    }
    desired.clamp(NODE_COMPACT_MIN_W, NODE_COMPACT_MAX_W)
}

/// Characters of a compact name that fit in `card_w` at `text_xs`.
///
/// The budget never drops below 6 letters plus an ellipsis, which is what the
/// minimum compact card can hold. Callers draw that card even when `card_w`
/// is smaller. The right inset reserved for the connect ring is part of the
/// chrome, so the name shortens before it would run under the ring.
fn compact_name_chars(card_w: f32) -> usize {
    let width = if card_w.is_finite() {
        card_w.max(NODE_COMPACT_MIN_W)
    } else {
        NODE_COMPACT_MIN_W
    };
    let raw = ((width - compact_name_chrome()) / COMPACT_CHAR_PX).floor();
    let slots = if raw.is_finite() && raw > 0.0 {
        raw as usize
    } else {
        0
    };
    slots.max(NODE_NAME_MIN_CHARS + 1)
}

/// Label painted for this mode. Icon-only paints nothing. Compact may shorten
/// the name. The result is never empty and never only an ellipsis.
pub fn node_caption(mode: NodeMode, heading: &str, card_w: f32) -> Option<String> {
    match mode {
        NodeMode::IconOnly => None,
        NodeMode::Full => {
            let heading = heading.trim();
            if heading_readable(heading) {
                Some(heading.to_string())
            } else {
                None
            }
        }
        NodeMode::Compact => node_short_name(heading, compact_name_chars(card_w)),
    }
}

/// Path line under a full card. Other modes hide it. A blank or ellipsis-only
/// path is hidden so the card never paints a bare `…`.
pub fn node_detail(mode: NodeMode, path: &str) -> Option<String> {
    if mode != NodeMode::Full {
        return None;
    }
    let path = path.trim();
    if path.is_empty() || ellipsis_only(path) {
        None
    } else {
        Some(path.to_string())
    }
}

/// Screen rect of a node card after the mode's minimum size is applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl NodeRect {
    /// Centre of the right edge. Edges leave a node from here.
    pub fn right_center(self) -> (f32, f32) {
        (self.x + self.w, self.y + self.h * 0.5)
    }

    /// Centre of the left edge. Edges arrive at a node here.
    pub fn left_center(self) -> (f32, f32) {
        (self.x, self.y + self.h * 0.5)
    }
}

/// True when `point` lies inside `rect`, edges included.
pub fn rect_contains(rect: NodeRect, point: (f32, f32)) -> bool {
    point.0 >= rect.x
        && point.0 <= rect.x + rect.w
        && point.1 >= rect.y
        && point.1 <= rect.y + rect.h
}

/// Glyph, status dot, and connect ring of an icon-only card, in the same
/// coordinate space as `tile`.
///
/// The dot is centred on the tile's top-right corner, so half of it sits on
/// the canvas. Its rect includes the 1px canvas-coloured border. It misses
/// both the glyph and the ring on the right edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconOnlyMarks {
    pub icon: NodeRect,
    pub dot: NodeRect,
    pub ring: NodeRect,
}

/// Outer size of the icon-only status dot, border included.
pub fn node_dot_outer() -> f32 {
    NODE_DOT + NODE_DOT_BORDER * 2.0
}

/// Layout of an icon-only card. `tile` is the rendered card from
/// [`rendered_node_rect`].
pub fn icon_only_marks(tile: NodeRect) -> IconOnlyMarks {
    let icon = NodeRect {
        x: tile.x + (tile.w - NODE_GLYPH) * 0.5,
        y: tile.y + (tile.h - NODE_GLYPH) * 0.5,
        w: NODE_GLYPH,
        h: NODE_GLYPH,
    };
    let outer = node_dot_outer();
    let dot = NodeRect {
        x: tile.x + tile.w - outer * 0.5,
        y: tile.y - outer * 0.5,
        w: outer,
        h: outer,
    };
    let ring = NodeRect {
        x: tile.x + tile.w - NODE_HANDLE * 0.5,
        y: tile.y + tile.h * 0.5 - NODE_HANDLE * 0.5,
        w: NODE_HANDLE,
        h: NODE_HANDLE,
    };
    IconOnlyMarks { icon, dot, ring }
}

fn scaled_node(zoom: f32) -> (f32, f32) {
    if zoom.is_finite() && zoom > 0.0 {
        (NODE_W * zoom, NODE_H * zoom)
    } else {
        (0.0, 0.0)
    }
}

/// Width and height to paint for `mode` at `zoom`.
///
/// Icon-only is the fixed tile. Compact width is [`compact_card_width`]
/// (the name, clamped); height is `NODE_H * zoom`, never under the compact
/// minimum. Full is the scaled card. `heading` is only read for compact.
pub fn node_draw_size(mode: NodeMode, zoom: f32, heading: &str) -> (f32, f32) {
    let (scaled_w, scaled_h) = scaled_node(zoom);
    match mode {
        NodeMode::IconOnly => (NODE_ICON_W, NODE_ICON_H),
        NodeMode::Compact => (
            compact_card_width(heading),
            scaled_h.max(NODE_COMPACT_MIN_H),
        ),
        NodeMode::Full => (scaled_w, scaled_h),
    }
}

/// Placed card. `(x, y)` is the node's screen origin (its top-left).
///
/// Compact cards grow to the right of that origin. Edge endpoints use this
/// rect, and the node face paints `rect.w`, so both follow [`compact_card_width`].
pub fn rendered_node_rect(x: f32, y: f32, mode: NodeMode, zoom: f32, heading: &str) -> NodeRect {
    let (w, h) = node_draw_size(mode, zoom, heading);
    NodeRect { x, y, w, h }
}

/// Canvas-local origin and width of an edge popover.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverAnchor {
    pub left: f32,
    pub top: f32,
    pub width: f32,
}

/// Place the selected-edge file card so it stays inside the canvas.
///
/// The card is 280px wide (narrower when the canvas is) and prefers to sit
/// 20px below `(anchor_left, anchor_mid_y)`. It flips above the anchor when
/// the bottom of the canvas is close. When `canvas_h` is known and tall enough
/// for `est_h`, `top` is clamped to `[8, canvas_h - est_h - 8]`.
pub fn clamp_edge_popover(
    anchor_left: f32,
    anchor_mid_y: f32,
    canvas_w: f32,
    canvas_h: f32,
    est_h: f32,
) -> PopoverAnchor {
    let width = if canvas_w > 32.0 {
        280.0_f32.min(canvas_w - 16.0)
    } else {
        280.0
    };
    let left = if canvas_w > width + 16.0 {
        anchor_left.clamp(8.0, canvas_w - width - 8.0)
    } else {
        8.0
    };
    // Prefer below the edge. Flip above when the bottom of the canvas is close.
    let mut top = if canvas_h > 220.0 && anchor_mid_y + 200.0 > canvas_h && anchor_mid_y > 180.0 {
        (anchor_mid_y - 180.0).max(8.0)
    } else {
        anchor_mid_y + 20.0
    };
    // A measured canvas is "known". Skip the clamp before the first layout
    // (`canvas_h == 0`) and when the range would be inverted.
    if canvas_h.is_finite() && canvas_h > 0.0 && est_h.is_finite() && est_h >= 0.0 {
        let max_top = canvas_h - est_h - 8.0;
        top = if max_top >= 8.0 {
            top.clamp(8.0, max_top)
        } else {
            8.0
        };
    }
    PopoverAnchor { left, top, width }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rect_contains_includes_edges_and_rejects_outside() {
        let rect = super::NodeRect {
            x: 10.0,
            y: 20.0,
            w: 100.0,
            h: 40.0,
        };
        assert!(super::rect_contains(rect, (10.0, 20.0)));
        assert!(super::rect_contains(rect, (110.0, 60.0)));
        assert!(super::rect_contains(rect, (50.0, 40.0)));
        assert!(!super::rect_contains(rect, (9.9, 40.0)));
        assert!(!super::rect_contains(rect, (50.0, 60.1)));
    }

    use super::*;
    use crate::engine::Flow;

    fn op(id: &str, source: (&str, &str), target: (&str, &str), action: &str) -> Operation {
        Operation {
            id: id.into(),
            source_remote: source.0.into(),
            source_path: source.1.into(),
            target_remote: target.0.into(),
            target_path: target.1.into(),
            action: action.into(),
            ..Default::default()
        }
    }

    #[test]
    fn node_ids_are_stable_and_path_normalized() {
        let a = stable_node_id(&location_key("gdrive", "/Photos"));
        let b = stable_node_id(&location_key("gdrive ", " /Photos "));
        assert_eq!(a, b);
        assert!(a.starts_with("n_"));
        assert_ne!(a, stable_node_id(&location_key("gdrive", "/Photos/2026")));
        assert_eq!(normalize_path("  "), "/");
    }

    #[test]
    fn default_layout_places_source_left_of_target() {
        let ops = vec![op("O1", ("", "/a"), ("gdrive", "/b"), "push")];
        let canvas = default_layout(&ops);
        assert_eq!(canvas.nodes.len(), 2);
        let source = &canvas.nodes[0];
        let target = &canvas.nodes[1];
        assert_eq!(source.x, 0.0);
        assert_eq!(target.x, COL_GAP as f64);
        assert_eq!(target.y, source.y);
    }

    #[test]
    fn default_layout_chains_a_target_that_is_the_next_source() {
        let ops = vec![
            op("O1", ("", "/a"), ("gdrive", "/b"), "push"),
            op("O2", ("gdrive", "/b"), ("", "/c"), "bi"),
        ];
        let canvas = default_layout(&ops);
        let x_of = |remote: &str, path: &str| {
            canvas
                .nodes
                .iter()
                .find(|n| n.remote == remote && n.path == path)
                .map(|n| n.x)
                .unwrap()
        };
        assert_eq!(x_of("gdrive", "/b"), COL_GAP as f64);
        assert_eq!(x_of("", "/c"), 2.0 * COL_GAP as f64);
    }

    #[test]
    fn graph_round_trip_rebuilds_operations_and_canvas() {
        let ops = vec![
            op("O1", ("", "/a"), ("gdrive", "/b"), "pull"),
            op("O2", ("gdrive", "/b"), ("", "/c"), "bi"),
        ];
        let mut flow = Flow {
            id: "F1".into(),
            operations: ops.clone(),
            ..Default::default()
        };
        flow.canvas_json = serialize_canvas(&default_layout(&ops));
        let mut graph = to_graph(&flow);
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
        assert_eq!(graph.edges[0].action, "push");
        assert_eq!(graph.edges[1].action, "bi");
        graph.nodes[0].x = 42.0;
        let (ops, canvas) = from_graph(&graph, &flow.operations);
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].sort_order, 0);
        assert_eq!(ops[0].action, "push");
        assert_eq!(ops[0].sync_config["action"], "push");
        let saved = parse_canvas(&canvas);
        assert_eq!(saved.nodes[0].x, 42.0);
    }

    #[test]
    fn duplicate_connections_are_rejected() {
        let ops = vec![op("O1", ("", "/a"), ("gdrive", "/b"), "push")];
        let mut flow = Flow {
            id: "F1".into(),
            operations: ops,
            ..Default::default()
        };
        flow.canvas_json = serialize_canvas(&default_layout(&flow.operations));
        let graph = to_graph(&flow);
        let (source, target) = (graph.edges[0].source.clone(), graph.edges[0].target.clone());
        assert_eq!(
            connect_error(&source, &target, "push", &graph),
            Some(ConnectError::DuplicateEdge)
        );
        assert_eq!(
            connect_error(&source, &source, "bi", &graph),
            Some(ConnectError::SelfLoop)
        );
    }

    #[test]
    fn a_curve_is_hit_along_its_middle_and_missed_far_away() {
        let from = (0.0, 0.0);
        let to = (200.0, 0.0);
        let mid = cubic_at(
            0.5,
            from,
            edge_controls(from, to).0,
            edge_controls(from, to).1,
            to,
        );
        assert!(curve_hits(from, to, mid));
        assert!(!curve_hits(from, to, (100.0, 80.0)));
    }

    #[test]
    fn chip_mode_follows_zoom_and_width_boundaries() {
        let wide = CHIP_FULL_MIN_W + 40.0;
        assert_eq!(chip_mode(CHIP_ICON_ZOOM, wide), ChipMode::Full);
        assert_eq!(chip_mode(CHIP_ICON_ZOOM, CHIP_FULL_MIN_W), ChipMode::Full);
        assert_eq!(
            chip_mode(CHIP_ICON_ZOOM, CHIP_FULL_MIN_W - 0.1),
            ChipMode::Compact
        );
        assert_eq!(
            chip_mode(CHIP_ICON_ZOOM, CHIP_LABEL_MIN_W),
            ChipMode::Compact
        );
        assert_eq!(
            chip_mode(CHIP_ICON_ZOOM, CHIP_LABEL_MIN_W - 0.1),
            ChipMode::IconOnly
        );
        assert_eq!(chip_mode(CHIP_ICON_ZOOM - 0.01, wide), ChipMode::IconOnly);
        assert_eq!(chip_mode(1.0, 0.0), ChipMode::IconOnly);
        assert_eq!(chip_mode(1.0, -4.0), ChipMode::IconOnly);
        assert_eq!(chip_mode(f32::NAN, wide), ChipMode::IconOnly);
        assert_eq!(chip_mode(1.0, f32::NAN), ChipMode::IconOnly);
        assert_eq!(chip_mode(0.25, wide), ChipMode::IconOnly);
        assert_eq!(chip_mode(2.0, CHIP_LABEL_MIN_W), ChipMode::Compact);

        assert!((chip_draw_width(0.4, wide) - CHIP_ICON_W).abs() < 0.01);
        assert!((chip_draw_width(1.0, 10.0) - CHIP_ICON_W).abs() < 0.01);
        assert!((chip_draw_width(1.0, CHIP_LABEL_MIN_W) - CHIP_LABEL_MIN_W).abs() < 0.01);
        assert!((chip_draw_width(1.0, 400.0) - CHIP_FULL_MAX_W).abs() < 0.01);
        assert!((CHIP_ICON_H - 22.0).abs() < 0.01);
    }

    #[test]
    fn chip_labels_are_never_only_an_ellipsis() {
        for action in ["push", "pull", "bi", "bi-resync", ""] {
            let label = crate::format::action_label(action);
            assert_eq!(chip_caption(ChipMode::IconOnly, label), None);
            assert_eq!(chip_caption(ChipMode::Full, label), Some(label));
            assert_eq!(chip_caption(ChipMode::Compact, label), Some(label));
            let tip = crate::format::chip_tip(action);
            assert!(tip.starts_with(label));
            assert_ne!(tip, "…");
            assert_ne!(label, "…");
        }
        assert_eq!(chip_caption(ChipMode::Full, "…"), None);
        assert_eq!(chip_caption(ChipMode::Compact, "..."), None);
        assert_eq!(chip_caption(ChipMode::Full, " . "), None);
        assert_eq!(chip_caption(ChipMode::Compact, "  "), None);
        assert_eq!(chip_caption(ChipMode::Full, " Push "), Some("Push"));
        for mode in [ChipMode::Full, ChipMode::Compact, ChipMode::IconOnly] {
            if let Some(text) = chip_caption(mode, "Re-sync") {
                assert!(text.chars().any(|c| c.is_alphabetic()), "{text}");
                assert!(!ellipsis_only(text));
            }
        }
    }

    #[test]
    fn popovers_paint_above_nodes_edge_labels_and_edges() {
        assert!(CanvasLayer::Popovers > CanvasLayer::Nodes);
        assert!(CanvasLayer::Nodes > CanvasLayer::EdgeLabels);
        assert!(CanvasLayer::EdgeLabels > CanvasLayer::Edges);
        assert!(CanvasLayer::Chrome > CanvasLayer::Nodes);
        assert!(CanvasLayer::Popovers > CanvasLayer::Chrome);

        let layers = [
            CanvasLayer::Popovers,
            CanvasLayer::Edges,
            CanvasLayer::Chrome,
            CanvasLayer::EdgeLabels,
            CanvasLayer::Nodes,
            CanvasLayer::EdgeLabels,
        ];
        assert_eq!(canvas_child_order(&layers), vec![1, 3, 5, 4, 2, 0]);
        let painted: Vec<_> = canvas_child_order(&layers)
            .into_iter()
            .map(|index| layers[index])
            .collect();
        assert_eq!(
            painted,
            vec![
                CanvasLayer::Edges,
                CanvasLayer::EdgeLabels,
                CanvasLayer::EdgeLabels,
                CanvasLayer::Nodes,
                CanvasLayer::Chrome,
                CanvasLayer::Popovers,
            ]
        );
    }

    fn popover_inside(place: PopoverAnchor, canvas_w: f32, canvas_h: f32, est_h: f32) {
        assert!(
            place.left >= 8.0,
            "left {} is outside the canvas",
            place.left
        );
        assert!(
            place.left + place.width <= canvas_w - 8.0 + 0.05,
            "right edge {} + {} crosses {}",
            place.left,
            place.width,
            canvas_w
        );
        assert!(place.top >= 8.0, "top {} is outside the canvas", place.top);
        assert!(
            place.top + est_h <= canvas_h - 8.0 + 0.05,
            "bottom {} + {} crosses {}",
            place.top,
            est_h,
            canvas_h
        );
    }

    #[test]
    fn edge_popover_stays_inside_near_the_right_and_bottom_borders() {
        let est_h = EDGE_POPOVER_EST_H;
        let canvas_w = 640.0;
        let canvas_h = 480.0;

        let right = clamp_edge_popover(canvas_w - 4.0, 80.0, canvas_w, canvas_h, est_h);
        popover_inside(right, canvas_w, canvas_h, est_h);
        assert!((right.left - (canvas_w - right.width - 8.0)).abs() < 0.01);

        let bottom = clamp_edge_popover(40.0, canvas_h - 12.0, canvas_w, canvas_h, est_h);
        popover_inside(bottom, canvas_w, canvas_h, est_h);
        assert!((bottom.top - (canvas_h - est_h - 8.0)).abs() < 0.01);

        let corner = clamp_edge_popover(canvas_w - 2.0, canvas_h - 2.0, canvas_w, canvas_h, est_h);
        popover_inside(corner, canvas_w, canvas_h, est_h);
        assert!((corner.left - (canvas_w - corner.width - 8.0)).abs() < 0.01);
        assert!((corner.top - (canvas_h - est_h - 8.0)).abs() < 0.01);

        // The anchor is not far enough down to flip (`mid_y > 180`), but the
        // card would still hang off a 300px canvas. The vertical clamp pulls
        // it up to `canvas_h - est_h - 8`.
        let hanging = clamp_edge_popover(120.0, 100.0, canvas_w, 300.0, est_h);
        popover_inside(hanging, canvas_w, 300.0, est_h);
        assert!((hanging.top - (300.0 - est_h - 8.0)).abs() < 0.01);

        // Before the first layout the canvas size is 0. Leave the anchor alone
        // instead of clamping into an empty viewport.
        let unknown = clamp_edge_popover(100.0, 40.0, 0.0, 0.0, est_h);
        assert_eq!(unknown.left, 8.0);
        assert_eq!(unknown.top, 60.0);
        assert_eq!(unknown.width, 280.0);
    }

    fn readable_letters(text: &str) -> usize {
        text.chars()
            .filter(|c| !c.is_whitespace() && !matches!(c, '…' | '.' | '·'))
            .count()
    }

    /// Shared area. Edges that only touch do not count.
    fn intersects(a: NodeRect, b: NodeRect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn node_mode_follows_zoom_width_and_heading_boundaries() {
        let name = "This Mac";
        let wide = NODE_FULL_MIN_W + 40.0;
        assert_eq!(
            node_mode(NODE_FULL_ZOOM, NODE_FULL_MIN_W, name),
            NodeMode::Full
        );
        assert_eq!(node_mode(NODE_FULL_ZOOM, wide, name), NodeMode::Full);
        assert_eq!(
            node_mode(NODE_FULL_ZOOM, NODE_FULL_MIN_W - 0.1, name),
            NodeMode::Compact
        );
        assert_eq!(
            node_mode(NODE_FULL_ZOOM - 0.01, wide, name),
            NodeMode::Compact
        );
        assert_eq!(node_mode(NODE_ICON_ZOOM, wide, name), NodeMode::Compact);
        assert_eq!(
            node_mode(NODE_ICON_ZOOM, NODE_ICON_W, name),
            NodeMode::Compact
        );
        assert_eq!(
            node_mode(NODE_ICON_ZOOM, NODE_ICON_W - 0.1, name),
            NodeMode::IconOnly
        );
        assert_eq!(
            node_mode(NODE_ICON_ZOOM - 0.01, wide, name),
            NodeMode::IconOnly
        );
        assert_eq!(node_mode(1.0, 0.0, name), NodeMode::IconOnly);
        assert_eq!(node_mode(1.0, -4.0, name), NodeMode::IconOnly);
        assert_eq!(node_mode(f32::NAN, wide, name), NodeMode::IconOnly);
        assert_eq!(node_mode(1.0, f32::NAN, name), NodeMode::IconOnly);
        assert_eq!(node_mode(1.0, wide, ""), NodeMode::IconOnly);
        assert_eq!(node_mode(1.0, wide, "   "), NodeMode::IconOnly);
        assert_eq!(node_mode(1.0, wide, "…"), NodeMode::IconOnly);
        assert_eq!(node_mode(1.0, wide, "..."), NodeMode::IconOnly);
        assert_eq!(node_mode(1.0, wide, " . "), NodeMode::IconOnly);
        assert_eq!(node_mode(0.5, wide, "A"), NodeMode::Compact);
        assert_eq!(
            node_mode(2.0, NODE_FULL_MIN_W, "Google Drive"),
            NodeMode::Full
        );

        // 40% is the reported collapse. The slot is narrower than the compact
        // minimum, and the card still shows a name rather than an icon or `…`.
        let slot = NODE_W * 0.40;
        assert!(slot < NODE_COMPACT_MIN_W);
        assert_eq!(node_mode(0.40, slot, "This Mac"), NodeMode::Compact);
        assert_eq!(node_mode(0.40, slot, "Google Drive"), NodeMode::Compact);
        assert_eq!(
            node_mode(0.25, NODE_W * 0.25, "This Mac"),
            NodeMode::IconOnly
        );
    }

    #[test]
    fn node_short_names_are_never_only_an_ellipsis() {
        assert_eq!(node_short_name("", 12), None);
        assert_eq!(node_short_name("   ", 12), None);
        assert_eq!(node_short_name("…", 12), None);
        assert_eq!(node_short_name("...", 12), None);
        assert_eq!(node_short_name(" . ", 8), None);
        assert_eq!(node_short_name("···", 8), None);

        assert_eq!(node_short_name("A", 12), Some("A".into()));
        assert_eq!(node_short_name("A", 1), Some("A".into()));
        assert_eq!(node_short_name("  A  ", 6), Some("A".into()));
        assert_eq!(node_short_name("日", 4), Some("日".into()));
        // A one-letter name does not fit in a zero budget, and does not become `…`.
        assert_eq!(node_short_name("A", 0), None);

        assert_eq!(node_short_name("This Mac", 8), Some("This Mac".into()));
        assert_eq!(node_short_name("This Mac", 20), Some("This Mac".into()));
        assert_eq!(node_short_name("This Mac", 7), Some("This M…".into()));

        let long = "Pictures from the summer trip";
        assert_eq!(node_short_name(long, 6), None);
        let shortened = node_short_name(long, 10).expect("long name shortens");
        assert_ne!(shortened, "…");
        assert!(shortened.ends_with('…'));
        assert!(readable_letters(&shortened) >= NODE_NAME_MIN_CHARS);

        let google = node_short_name("Google Drive", NODE_NAME_MIN_CHARS + 1).unwrap();
        assert_eq!(google, "Google…");
        assert!(readable_letters(&google) >= NODE_NAME_MIN_CHARS);

        for heading in ["", "A", "This Mac", "Google Drive", long, "…", "..."] {
            for max_chars in [0, 1, 6, 7, 8, 12, 40] {
                if let Some(text) = node_short_name(heading, max_chars) {
                    assert!(!ellipsis_only(&text), "{heading:?} -> {text:?}");
                    assert_ne!(text, "…");
                    assert!(readable_letters(&text) >= 1, "{text}");
                    if heading.chars().count() > max_chars {
                        let body = text.trim_end_matches('…');
                        assert!(
                            body.chars().count() >= NODE_NAME_MIN_CHARS,
                            "{heading:?} -> {text}"
                        );
                    }
                }
            }
            for mode in [NodeMode::Full, NodeMode::Compact, NodeMode::IconOnly] {
                if let Some(text) = node_caption(mode, heading, NODE_COMPACT_MIN_W) {
                    assert!(!ellipsis_only(&text), "{text}");
                    assert_ne!(text, "…");
                }
            }
        }

        // A full-width card shows the name whole. The compact card grows to
        // the name (see `compact_card_width`), so these are not shortened.
        assert_eq!(
            node_caption(NodeMode::Compact, "This Mac", NODE_W),
            Some("This Mac".into())
        );
        for name in ["This Mac", "Home NAS", "Google Drive"] {
            let width = compact_card_width(name);
            let painted = node_caption(NodeMode::Compact, name, width).unwrap();
            assert_eq!(painted, name, "{name} truncated to {painted}");
            assert!(!ellipsis_only(&painted));
        }
        assert_eq!(node_caption(NodeMode::IconOnly, "This Mac", 170.0), None);
        assert_eq!(
            node_caption(NodeMode::Full, "Google Drive", 170.0),
            Some("Google Drive".into())
        );
        assert_eq!(node_detail(NodeMode::Compact, "/Users/me/Photos"), None);
        assert_eq!(node_detail(NodeMode::IconOnly, "/Users/me/Photos"), None);
        assert_eq!(node_detail(NodeMode::Full, "…"), None);
        assert_eq!(node_detail(NodeMode::Full, "   "), None);
        assert_eq!(
            node_detail(NodeMode::Full, "/Users/me/Photos"),
            Some("/Users/me/Photos".into())
        );
    }

    #[test]
    fn rendered_node_rect_keeps_minimums_and_edge_anchors() {
        assert!((NODE_ICON_W - 28.0).abs() < 0.01);
        assert!((NODE_ICON_H - 28.0).abs() < 0.01);
        assert!((NODE_COMPACT_MIN_W - 108.0).abs() < 0.01);
        assert!((NODE_COMPACT_MAX_W - 150.0).abs() < 0.01);
        assert!((NODE_COMPACT_MIN_H - 28.0).abs() < 0.01);
        assert!((NODE_HANDLE - 14.0).abs() < 0.01);

        let compact = rendered_node_rect(0.0, 0.0, NodeMode::Compact, 0.40, "This Mac");
        assert!(compact.w + 0.01 >= NODE_COMPACT_MIN_W);
        assert!(compact.w <= NODE_COMPACT_MAX_W + 0.01);
        assert!(compact.h + 0.01 >= NODE_COMPACT_MIN_H);
        assert!(compact.w > NODE_W * 0.40);
        // Height is the scaled card, and never under the compact floor.
        // At this zoom those two meet, so the check is not a strict greater-than.
        assert!(compact.h + 0.01 >= NODE_H * 0.40);
        assert!((compact.right_center().0 - compact.w).abs() < 0.01);
        assert!((compact.right_center().1 - compact.h * 0.5).abs() < 0.01);
        assert!((compact.left_center().0).abs() < 0.01);

        // Compact width follows the name, not `NODE_W * zoom`. A large zoom
        // still stays inside the cap. Height still scales. The left anchor
        // does not move when the card grows.
        let wide = rendered_node_rect(3.0, 5.0, NodeMode::Compact, 1.5, "Google Drive");
        assert!((wide.w - compact_card_width("Google Drive")).abs() < 0.01);
        assert!(wide.w <= NODE_COMPACT_MAX_W + 0.01);
        assert!((wide.h - NODE_H * 1.5).abs() < 0.01);
        assert!((wide.x - 3.0).abs() < 0.01);
        assert!((wide.right_center().0 - (3.0 + wide.w)).abs() < 0.01);
        assert!((wide.left_center().1 - (5.0 + wide.h * 0.5)).abs() < 0.01);

        let icon = rendered_node_rect(4.0, 8.0, NodeMode::IconOnly, 0.25, "This Mac");
        assert!((icon.w - NODE_ICON_W).abs() < 0.01);
        assert!((icon.h - NODE_ICON_H).abs() < 0.01);
        assert!((icon.left_center().0 - 4.0).abs() < 0.01);
        assert!((icon.left_center().1 - (8.0 + NODE_ICON_H * 0.5)).abs() < 0.01);
        assert!((icon.right_center().0 - (4.0 + NODE_ICON_W)).abs() < 0.01);
        // A non-finite zoom still paints the fixed tile, not a NaN rect.
        let broken = rendered_node_rect(0.0, 0.0, NodeMode::IconOnly, f32::NAN, "");
        assert!((broken.w - NODE_ICON_W).abs() < 0.01);
        assert!(broken.right_center().0.is_finite());

        let full = rendered_node_rect(0.0, 0.0, NodeMode::Full, 1.0, "Google Drive");
        assert!((NODE_W - 220.0).abs() < 0.01);
        assert!((NODE_H - 72.0).abs() < 0.01);
        assert!((full.w - NODE_W).abs() < 0.01);
        assert!((full.h - NODE_H).abs() < 0.01);
        assert!((full.right_center().1 - NODE_H * 0.5).abs() < 0.01);

        let (compact_w, compact_h) = node_draw_size(NodeMode::Compact, 0.40, "This Mac");
        assert!((compact_w - compact.w).abs() < 0.01);
        assert!((compact_h - compact.h).abs() < 0.01);
        let (icon_w, icon_h) = node_draw_size(NodeMode::IconOnly, 2.0, "This Mac");
        assert!((icon_w - NODE_ICON_W).abs() < 0.01);
        assert!((icon_h - NODE_ICON_H).abs() < 0.01);
    }

    #[test]
    fn compact_name_reserves_the_connect_ring() {
        let chrome = compact_name_chrome();
        // Right padding replaced a 6px inset, so the name column is narrower
        // by the extra ring clearance.
        assert!((chrome - (43.0 + (NODE_COMPACT_PAD_RIGHT - 6.0))).abs() < 0.01);

        let slots = compact_name_chars(NODE_COMPACT_MIN_W);
        let fitted = ((NODE_COMPACT_MIN_W - chrome) / COMPACT_CHAR_PX).floor() as usize;
        assert_eq!(slots, fitted.max(NODE_NAME_MIN_CHARS + 1));
        assert!(slots > NODE_NAME_MIN_CHARS);

        // The minimum card still reserves the ring. A name that needs more
        // than that floor uses `compact_card_width` instead of clipping here.
        let home =
            node_caption(NodeMode::Compact, "Home NAS", NODE_COMPACT_MIN_W).expect("home nas");
        assert_ne!(home, "…");
        assert!(!ellipsis_only(&home));
        assert!(home.chars().count() <= slots);
        let body = home.trim_end_matches('…');
        assert!(body.chars().count() >= NODE_NAME_MIN_CHARS);

        assert_eq!(
            node_caption(NodeMode::Compact, "Home NAS", NODE_W),
            Some("Home NAS".into())
        );

        // A non-finite width still uses the minimum card, and still not `…`.
        let fallback = node_caption(NodeMode::Compact, "Home NAS", f32::NAN).unwrap();
        assert_eq!(fallback, home);
        assert_ne!(fallback, "…");
    }

    #[test]
    fn compact_card_width_fits_short_names_and_matches_the_rect() {
        assert!((NODE_COMPACT_MIN_W - 108.0).abs() < 0.01);
        assert!((NODE_COMPACT_MAX_W - 150.0).abs() < 0.01);
        let chrome = compact_name_chrome();
        // Left pad, glyph, two gaps, dot, and the 12px ring reserve.
        assert!(
            (chrome
                - (NODE_COMPACT_PAD_LEFT
                    + NODE_GLYPH
                    + NODE_COMPACT_GAP * 2.0
                    + NODE_DOT
                    + NODE_COMPACT_PAD_RIGHT))
                .abs()
                < 0.01
        );
        assert!((NODE_COMPACT_PAD_RIGHT - 12.0).abs() < 0.01);

        for name in ["This Mac", "Home NAS", "Google Drive"] {
            let width = compact_card_width(name);
            assert!(
                width + 0.01 >= NODE_COMPACT_MIN_W && width <= NODE_COMPACT_MAX_W + 0.01,
                "{name} width {width}"
            );
            let chars = name.chars().count() as f32;
            let estimate = chrome + chars * COMPACT_CHAR_PX;
            assert!(
                (width - estimate.clamp(NODE_COMPACT_MIN_W, NODE_COMPACT_MAX_W)).abs() < 0.01,
                "{name}"
            );
            // The text slot is the estimate, so the caption is the whole name.
            assert!(width - chrome + 0.01 >= chars * COMPACT_CHAR_PX, "{name}");
            assert_eq!(
                node_caption(NodeMode::Compact, name, width).as_deref(),
                Some(name),
                "{name}"
            );
            let rect = rendered_node_rect(20.0, 4.0, NodeMode::Compact, 0.40, name);
            let (draw_w, _) = node_draw_size(NodeMode::Compact, 0.40, name);
            assert!((rect.w - width).abs() < 0.01, "{name} rect");
            assert!((draw_w - width).abs() < 0.01, "{name} draw");
            // Grows from the left anchor. The curve leaves the right edge.
            assert!((rect.x - 20.0).abs() < 0.01);
            assert!((rect.right_center().0 - (20.0 + width)).abs() < 0.01);
        }

        let mac = rendered_node_rect(8.0, 0.0, NodeMode::Compact, 0.40, "This Mac");
        let drive = rendered_node_rect(8.0, 0.0, NodeMode::Compact, 0.40, "Google Drive");
        assert!((mac.x - drive.x).abs() < 0.01);
        assert!(drive.w > mac.w);
        assert!(drive.right_center().0 > mac.right_center().0);

        // One letter stays on the floor. A blank heading does too.
        assert!((compact_card_width("A") - NODE_COMPACT_MIN_W).abs() < 0.01);
        assert!((compact_card_width("  ") - NODE_COMPACT_MIN_W).abs() < 0.01);
        assert!((compact_card_width("…") - NODE_COMPACT_MIN_W).abs() < 0.01);

        let long = "Pictures from the summer trip";
        let long_w = compact_card_width(long);
        assert!((long_w - NODE_COMPACT_MAX_W).abs() < 0.01);
        let shortened = node_caption(NodeMode::Compact, long, long_w).expect("long name");
        assert_ne!(shortened, "…");
        assert!(!ellipsis_only(&shortened));
        assert!(shortened.ends_with('…'));
        let body = shortened.trim_end_matches('…');
        assert!(body.chars().count() >= NODE_NAME_MIN_CHARS);
        assert!(readable_letters(&shortened) >= NODE_NAME_MIN_CHARS);
        assert!(shortened.chars().count() <= compact_name_chars(long_w));

        // The rect the edges use is the width the caption was measured with.
        let rect = rendered_node_rect(0.0, 0.0, NodeMode::Compact, 0.40, long);
        assert!((rect.w - long_w).abs() < 0.01);
        assert_eq!(
            node_caption(NodeMode::Compact, long, rect.w).as_deref(),
            Some(shortened.as_str())
        );
    }

    #[test]
    fn icon_only_dot_misses_the_glyph_and_the_ring() {
        let tile = rendered_node_rect(4.0, 8.0, NodeMode::IconOnly, 0.25, "");
        let marks = icon_only_marks(tile);
        assert!(
            !intersects(marks.dot, marks.icon),
            "dot {:?} hits icon {:?}",
            marks.dot,
            marks.icon
        );
        assert!(
            !intersects(marks.dot, marks.ring),
            "dot {:?} hits ring {:?}",
            marks.dot,
            marks.ring
        );

        // Half the dot, border included, hangs off the top-right corner.
        let outer = node_dot_outer();
        assert!((marks.dot.w - outer).abs() < 0.01);
        assert!((marks.dot.h - outer).abs() < 0.01);
        assert!((outer - (NODE_DOT + NODE_DOT_BORDER * 2.0)).abs() < 0.01);
        assert!(marks.dot.y < tile.y);
        assert!(marks.dot.y + marks.dot.h > tile.y);
        assert!(marks.dot.x < tile.x + tile.w);
        assert!(marks.dot.x + marks.dot.w > tile.x + tile.w);
        assert!((marks.dot.x + marks.dot.w * 0.5 - (tile.x + tile.w)).abs() < 0.01);
        assert!((marks.dot.y + marks.dot.h * 0.5 - tile.y).abs() < 0.01);

        assert!((marks.icon.w - NODE_GLYPH).abs() < 0.01);
        assert!((marks.icon.h - NODE_GLYPH).abs() < 0.01);
        assert!((marks.icon.x - (tile.x + (tile.w - NODE_GLYPH) * 0.5)).abs() < 0.01);
        assert!((marks.icon.y - (tile.y + (tile.h - NODE_GLYPH) * 0.5)).abs() < 0.01);

        // The ring is the 14px handle, centred on the right edge.
        assert!((marks.ring.w - NODE_HANDLE).abs() < 0.01);
        assert!((marks.ring.h - NODE_HANDLE).abs() < 0.01);
        assert!((marks.ring.x + marks.ring.w * 0.5 - (tile.x + tile.w)).abs() < 0.01);
        assert!((marks.ring.y + marks.ring.h * 0.5 - tile.right_center().1).abs() < 0.01);

        // Touching edges are not a collision.
        let touch = NodeRect {
            x: marks.icon.x + marks.icon.w,
            y: marks.icon.y,
            w: 4.0,
            h: marks.icon.h,
        };
        assert!(!intersects(marks.icon, touch));
    }

    #[test]
    fn node_cards_name_the_folder_and_keep_both_ends_of_the_path() {
        let vacation = node_card_copy("", "/Users/me/Pictures/Vacation", "This Mac", 20);
        let taxes = node_card_copy("", "/Users/me/Documents/Taxes", "This Mac", 20);
        let drive = node_card_copy("", "/Backup/Photos", "Google Drive", 20);
        let labeled = node_card_copy("Nightly", "/Users/me/Photos", "This Mac", 20);
        assert_eq!(vacation.title, "Vacation");
        assert_eq!(taxes.title, "Taxes");
        assert_eq!(drive.title, "Photos");
        assert_eq!(drive.place, "Google Drive");
        assert_eq!(labeled.title, "Nightly");
        assert_eq!(vacation.place, "This Mac");
        assert_eq!(taxes.place, "This Mac");
        assert_ne!(vacation.title, taxes.title);
        assert_ne!(vacation.title, "This Mac");
        assert_eq!(vacation.path, "/Users/me/…/Vacation");
        assert_eq!(
            node_card_copy("…", "/Users/me/Pictures/Vacation", "", 20).title,
            "Vacation"
        );
        assert_eq!(folder_title("", "/"), "Folder");
        assert_eq!(last_folder_name("/Users/me/Pictures/Vacation"), "Vacation");
        assert_eq!(place_name("local", "ignored"), "This Mac");
        assert_eq!(place_name("", ""), "This Mac");
        assert_eq!(place_name("gdrive", "Google Drive"), "Google Drive");
        assert_eq!(place_name("gdrive", ""), "gdrive");
        let slots = node_path_chars(NODE_W);
        assert!(slots >= 16);
        let fitted = node_card_copy("", "/Users/me/Pictures/Vacation", "This Mac", slots);
        assert!(fitted.path.contains('…') || fitted.path.contains("Vacation"));
        assert!(fitted.path.contains("Vacation"));
    }

    #[test]
    fn default_layout_leaves_a_gap_between_cards() {
        let ops = vec![
            op(
                "O1",
                ("", "/Users/me/Pictures/Vacation"),
                ("gdrive", "/Backup/Photos"),
                "push",
            ),
            op(
                "O2",
                ("", "/Users/me/Documents/Taxes"),
                ("nas", "/Archive/Taxes"),
                "bi",
            ),
        ];
        let canvas = default_layout(&ops);
        assert!(canvas.nodes.len() >= 3);
        for left in 0..canvas.nodes.len() {
            for right in (left + 1)..canvas.nodes.len() {
                let a = &canvas.nodes[left];
                let b = &canvas.nodes[right];
                let overlap_x = a.x < b.x + f64::from(NODE_W) && b.x < a.x + f64::from(NODE_W);
                let overlap_y = a.y < b.y + f64::from(NODE_H) && b.y < a.y + f64::from(NODE_H);
                assert!(!(overlap_x && overlap_y), "{} overlaps {}", a.path, b.path);
            }
        }
    }

    #[test]
    fn compact_fit_zoom_stays_readable_and_wide_fit_may_shrink() {
        // Three columns in a 390pt window (about 340pt of canvas) used to fit
        // near 27%, under the icon floor. The compact floor keeps place and path.
        let graph_w = 2.0 * COL_GAP + NODE_W;
        let graph_h = NODE_H;
        let view_w = 340.0;
        let view_h = 480.0;
        let old = ((view_w - FIT_PAD_COMPACT) / graph_w).min((view_h - FIT_PAD_COMPACT) / graph_h);
        assert!(old < 0.30, "fixture should be the 27% case, got {old}");

        let zoom = choose_fit_zoom(view_w, view_h, graph_w, graph_h, true);
        assert!(zoom + 0.001 >= FIT_ZOOM_MIN_COMPACT);
        assert!(
            zoom > 0.50,
            "compact fit must not stay near 27%, got {zoom}"
        );
        assert!((FIT_ZOOM_MIN_COMPACT - NODE_FULL_ZOOM).abs() < 0.001);
        assert_eq!(
            node_mode(zoom, NODE_W * zoom, "Photos"),
            NodeMode::Full,
            "place and path stay at the compact fit floor when the card is wide enough"
        );
        assert!(node_detail(NodeMode::Full, "/Users/me/Photos").is_some());
        // A card that is genuinely too narrow still drops the path.
        assert_eq!(
            node_mode(zoom, NODE_FULL_MIN_W - 1.0, "Photos"),
            NodeMode::Compact
        );
        // Title-only is the small-zoom band, not the fitted view.
        assert_eq!(
            node_mode(NODE_ICON_ZOOM, NODE_W, "Photos"),
            NodeMode::Compact
        );
        assert_eq!(
            node_mode(NODE_ICON_ZOOM - 0.01, NODE_W, "Photos"),
            NodeMode::IconOnly
        );

        let wide = choose_fit_zoom(500.0, 400.0, 2000.0, 400.0, false);
        assert!((wide - FIT_ZOOM_MIN).abs() < 0.01);
        let compact_floor = choose_fit_zoom(500.0, 400.0, 2000.0, 400.0, true);
        assert!((compact_floor - FIT_ZOOM_MIN_COMPACT).abs() < 0.01);

        let tiny = choose_fit_zoom(800.0, 600.0, 80.0, 40.0, true);
        assert!((tiny - FIT_ZOOM_MAX).abs() < 0.01);
        assert!(
            (choose_fit_zoom(0.0, 400.0, graph_w, graph_h, true) - FIT_ZOOM_MIN_COMPACT).abs()
                < 0.01
        );
        assert!(
            (choose_fit_zoom(f32::NAN, 400.0, graph_w, graph_h, false) - FIT_ZOOM_MIN).abs() < 0.01
        );
    }
}
