//! Derived-data caches for the workspace.
//!
//! Render used to recompute every aggregate on each frame: a deep copy of the
//! flows, lowercased rail titles, per-node status scans, and a full transfer
//! ranking per panel. These caches hold the same values, rebuilt only when
//! their inputs change (a snapshot, a runtime event, or a graph edit), and are
//! shared with the render paths through [`std::rc::Rc`].
//!
//! Each cached computation has a `*_reference` twin kept under `#[cfg(test)]`
//! that reproduces the original per-frame logic, so the tests prove the cached
//! value is exactly what the old code produced.

use std::collections::HashMap;
use std::rc::Rc;

use crate::engine::{Flow, Remote, Runtime, Snapshot, SyncProgress};
use crate::format::{FileDot, card_rank, file_dots};
use crate::graph::{self, FlowGraph, GraphEdge, GraphNode};

/// Sorted flows plus the strings the rail, toolbar, and history list derive
/// from them. Rebuilt when a snapshot arrives.
#[derive(Clone)]
pub(crate) struct FlowListCache {
    /// The sorted flows, shared with every reader.
    pub flows: Rc<Vec<Flow>>,
    /// Lowercased titles for the rail search, aligned with `flows`.
    pub titles_lower: Rc<Vec<String>>,
    /// `(id, name)` pairs the history list resolves titles against.
    pub history_names: Rc<Vec<(String, String)>>,
}

impl FlowListCache {
    pub fn empty() -> Self {
        Self {
            flows: Rc::new(Vec::new()),
            titles_lower: Rc::new(Vec::new()),
            history_names: Rc::new(Vec::new()),
        }
    }

    /// Cold build from a snapshot: one deep copy of the flows, then sort and
    /// derive the per-flow strings once.
    pub fn build(snapshot: Option<&Snapshot>) -> Self {
        let mut flows = snapshot.map(|snap| snap.flows.clone()).unwrap_or_default();
        flows.sort_by_key(|flow| flow.sort_order);
        let titles: Vec<String> = flows.iter().map(|flow| flow.title()).collect();
        let titles_lower: Vec<String> = titles.iter().map(|title| title.to_lowercase()).collect();
        let history_names: Vec<(String, String)> = flows
            .iter()
            .map(|flow| (flow.id.clone(), flow.name.clone()))
            .collect();
        Self {
            flows: Rc::new(flows),
            titles_lower: Rc::new(titles_lower),
            history_names: Rc::new(history_names),
        }
    }
}

/// Remotes plus the friendly place name per account. Rebuilt with the flows.
#[derive(Clone)]
pub(crate) struct RemoteCache {
    pub remotes: Rc<Vec<Remote>>,
    /// Account name to the description, or the name when the description is
    /// blank. The first remote wins on duplicate names, matching `find`.
    pub friendly: Rc<HashMap<String, String>>,
}

impl RemoteCache {
    pub fn empty() -> Self {
        Self {
            remotes: Rc::new(Vec::new()),
            friendly: Rc::new(HashMap::new()),
        }
    }

    pub fn build(snapshot: Option<&Snapshot>) -> Self {
        let remotes = snapshot
            .map(|snap| snap.remotes.clone())
            .unwrap_or_default();
        let mut friendly = HashMap::with_capacity(remotes.len());
        for remote in &remotes {
            let shown = if remote.description.trim().is_empty() {
                remote.name.clone()
            } else {
                remote.description.clone()
            };
            friendly.entry(remote.name.clone()).or_insert(shown);
        }
        Self {
            remotes: Rc::new(remotes),
            friendly: Rc::new(friendly),
        }
    }
}

/// Flow-id to run status. Rebuilt when the runtime changes.
#[derive(Clone)]
pub(crate) struct RuntimeCache {
    pub statuses: Rc<HashMap<String, String>>,
}

impl RuntimeCache {
    pub fn empty() -> Self {
        Self {
            statuses: Rc::new(HashMap::new()),
        }
    }

    pub fn build(runtime: &Runtime) -> Self {
        let statuses = runtime
            .flows
            .iter()
            .map(|flow| {
                (
                    flow.id.clone(),
                    if flow.status.is_empty() {
                        "idle".to_string()
                    } else {
                        flow.status.clone()
                    },
                )
            })
            .collect();
        Self {
            statuses: Rc::new(statuses),
        }
    }

    /// The old `flow_status` lookup: linear scan, defaulting to idle.
    pub fn status(&self, flow_id: &str) -> String {
        self.statuses
            .get(flow_id)
            .cloned()
            .unwrap_or_else(|| "idle".into())
    }
}

/// Graph node and edge lists plus per-node status. Rebuilt when the graph or
/// the runtime changes.
#[derive(Clone)]
pub(crate) struct GraphCache {
    pub nodes: Rc<Vec<GraphNode>>,
    pub edges: Rc<Vec<GraphEdge>>,
    /// `folder_title(label, path)` per node, aligned with `nodes`.
    pub headings: Rc<Vec<String>>,
    /// Folded edge status per node id. Missing means "idle".
    pub node_status: Rc<HashMap<String, String>>,
    /// Inputs the cache was built from, so a stale cache is rebuilt, not read.
    pub flow_id: String,
    pub runtime_gen: u64,
    pub graph_gen: u64,
}

impl GraphCache {
    pub fn empty() -> Self {
        Self {
            nodes: Rc::new(Vec::new()),
            edges: Rc::new(Vec::new()),
            headings: Rc::new(Vec::new()),
            node_status: Rc::new(HashMap::new()),
            flow_id: String::new(),
            runtime_gen: 0,
            graph_gen: 0,
        }
    }

    pub fn build(
        graph: Option<&FlowGraph>,
        runtime: &Runtime,
        flow_id: &str,
        runtime_gen: u64,
        graph_gen: u64,
    ) -> Self {
        let Some(graph) = graph else {
            return Self::empty();
        };
        let nodes = Rc::new(graph.nodes.clone());
        let edges = Rc::new(graph.edges.clone());
        let headings: Vec<String> = graph
            .nodes
            .iter()
            .map(|node| graph::folder_title(&node.label, &node.path))
            .collect();
        let node_status = Rc::new(node_statuses(&graph.nodes, &graph.edges, runtime, flow_id));
        Self {
            nodes,
            edges,
            headings: Rc::new(headings),
            node_status,
            flow_id: flow_id.to_string(),
            runtime_gen,
            graph_gen,
        }
    }

    pub fn matches(&self, flow_id: &str, runtime_gen: u64, graph_gen: u64) -> bool {
        self.flow_id == flow_id && self.runtime_gen == runtime_gen && self.graph_gen == graph_gen
    }
}

/// One pass over the edges folds each endpoint's status, replacing the old
/// per-node scan of every edge (`O(N*E)`). A `running` edge stops the fold for
/// that node exactly as the old early return did; `failed` sticks; `completed`
/// only fills an idle node.
fn node_statuses(
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    runtime: &Runtime,
    flow_id: &str,
) -> HashMap<String, String> {
    let mut ops = HashMap::new();
    if let Some(flow) = runtime.flows.iter().find(|flow| flow.id == flow_id) {
        for op in &flow.ops {
            ops.insert(op.id.as_str(), op.status.as_str());
        }
    }
    let mut status: HashMap<String, String> = HashMap::with_capacity(nodes.len());
    let mut running: Vec<&str> = Vec::new();
    for edge in edges {
        // A self-loop touches the node once, as the old endpoint test did.
        if edge.source == edge.target {
            touch_node(&edge.source, &edge.id, &ops, &mut status, &mut running);
        } else {
            touch_node(&edge.source, &edge.id, &ops, &mut status, &mut running);
            touch_node(&edge.target, &edge.id, &ops, &mut status, &mut running);
        }
    }
    status
}

fn touch_node<'a>(
    node_id: &'a String,
    edge_id: &str,
    ops: &HashMap<&str, &str>,
    status: &mut HashMap<String, String>,
    running: &mut Vec<&'a str>,
) {
    if running.contains(&node_id.as_str()) {
        return;
    }
    let Some(op_status) = ops.get(edge_id) else {
        return;
    };
    if crate::format::running(op_status) {
        running.push(node_id.as_str());
        status.insert(node_id.clone(), (*op_status).to_string());
    } else if *op_status == "failed"
        || (*op_status == "completed"
            && status.get(node_id).map(String::as_str).unwrap_or("idle") == "idle")
    {
        status.insert(node_id.clone(), (*op_status).to_string());
    }
}

/// Progress per edge of the selected flow, plus the derived transfer ranking
/// and file dots. Rebuilt when the graph or the runtime changes.
#[derive(Clone)]
pub(crate) struct SyncCache {
    pub flow_id: String,
    /// `sync_for` result per edge id.
    pub progresses: Rc<HashMap<String, Rc<SyncProgress>>>,
    /// Transfer indices in card order per edge id, into `progresses` transfers.
    pub ranked: Rc<HashMap<String, Rc<Vec<u32>>>>,
    /// [`crate::format::file_dots`] per edge id, using the edge's action.
    pub dots: Rc<HashMap<String, Rc<Vec<FileDot>>>>,
    /// First edge with progress per endpoint node id, in edge order — the old
    /// per-node `find_map` over incident edges, indexed once per build.
    pub node_progress: Rc<HashMap<String, Rc<SyncProgress>>>,
    /// Inputs the cache was built from, so a stale cache is rebuilt, not read.
    pub runtime_gen: u64,
    pub graph_gen: u64,
}

impl SyncCache {
    pub fn build(
        runtime: &Runtime,
        flow_id: &str,
        edges: &[GraphEdge],
        runtime_gen: u64,
        graph_gen: u64,
    ) -> Self {
        let mut progresses = HashMap::with_capacity(edges.len());
        let mut ranked = HashMap::new();
        let mut dots = HashMap::new();
        let mut node_progress = HashMap::new();
        for edge in edges {
            if let Some(progress) = crate::sync_for(runtime, flow_id, &edge.id) {
                let progress = Rc::new(progress);
                let order: Vec<u32> = rank_indices(&progress.transfers);
                let edge_dots = file_dots(&edge.action, &progress.transfers);
                ranked.insert(edge.id.clone(), Rc::new(order));
                dots.insert(edge.id.clone(), Rc::new(edge_dots));
                progresses.insert(edge.id.clone(), progress.clone());
                // First incident edge in edge order wins, as the old per-node
                // find_map did. A self-loop hits the same key twice.
                node_progress
                    .entry(edge.source.clone())
                    .or_insert(progress.clone());
                node_progress.entry(edge.target.clone()).or_insert(progress);
            }
        }
        Self {
            flow_id: flow_id.to_string(),
            progresses: Rc::new(progresses),
            ranked: Rc::new(ranked),
            dots: Rc::new(dots),
            node_progress: Rc::new(node_progress),
            runtime_gen,
            graph_gen,
        }
    }

    pub fn matches(&self, flow_id: &str, runtime_gen: u64, graph_gen: u64) -> bool {
        self.flow_id == flow_id && self.runtime_gen == runtime_gen && self.graph_gen == graph_gen
    }
}

/// Card order as indices, so panels can take their top rows without re-sorting
/// the whole transfer list each frame.
fn rank_indices(transfers: &[crate::engine::FileTransfer]) -> Vec<u32> {
    let mut ranked: Vec<u32> = (0..transfers.len() as u32).collect();
    ranked.sort_by(|&left, &right| {
        let (left, right) = (&transfers[left as usize], &transfers[right as usize]);
        card_rank(&left.status)
            .cmp(&card_rank(&right.status))
            .then_with(|| left.name.cmp(&right.name))
    });
    ranked
}

/// The rail filter over cached lowercased titles. Returns the indices of the
/// flows whose title matches `query`, or all of them for an empty query.
pub(crate) fn rail_matches(titles_lower: &[String], query: &str) -> Vec<u32> {
    if query.is_empty() {
        return (0..titles_lower.len() as u32).collect();
    }
    titles_lower
        .iter()
        .enumerate()
        .filter(|(_, title)| title.contains(query))
        .map(|(index, _)| index as u32)
        .collect()
}

// MARK: - Reference implementations (old per-frame logic, test-only)

#[cfg(test)]
pub(crate) mod reference {
    use super::*;

    /// The old `sync_for`: clone the whole progress before matching.
    pub(crate) fn sync_for(runtime: &Runtime, flow_id: &str, op_id: &str) -> Option<SyncProgress> {
        let flow = runtime.flows.iter().find(|flow| flow.id == flow_id)?;
        let sync = flow.sync.clone()?;
        let profile = flow_id.to_string() + ":" + op_id;
        (sync.profile_id == profile || sync.profile_id.ends_with(op_id)).then_some(sync)
    }

    /// The old `node_status`: scan every edge for each node.
    pub(crate) fn node_status(
        graph: &FlowGraph,
        runtime: &Runtime,
        flow_id: &str,
        node_id: &str,
    ) -> String {
        let mut status = "idle".to_string();
        for edge in &graph.edges {
            if edge.source != node_id && edge.target != node_id {
                continue;
            }
            let Some(flow) = runtime.flows.iter().find(|flow| flow.id == flow_id) else {
                continue;
            };
            let Some(op) = flow.ops.iter().find(|op| op.id == edge.id) else {
                continue;
            };
            if crate::format::running(&op.status) {
                return op.status.clone();
            }
            if op.status == "failed" || (op.status == "completed" && status == "idle") {
                status = op.status.clone();
            }
        }
        status
    }

    /// The old rail filter: lowercase the query and every title on the spot.
    pub(crate) fn rail_shown(flows: &[Flow], query: &str) -> Vec<usize> {
        flows
            .iter()
            .enumerate()
            .filter(|(_, flow)| query.is_empty() || flow.title().to_lowercase().contains(query))
            .map(|(index, _)| index)
            .collect()
    }

    /// The old per-frame flow status lookup.
    pub(crate) fn flow_status(runtime: &Runtime, flow_id: &str) -> String {
        runtime
            .flows
            .iter()
            .find(|flow| flow.id == flow_id)
            .map(|flow| flow.status.clone())
            .filter(|status| !status.is_empty())
            .unwrap_or_else(|| "idle".into())
    }

    /// The old `sorted_flows` deep copy, kept for the equivalence tests.
    pub(crate) fn sorted_flows(snapshot: Option<&Snapshot>) -> Vec<Flow> {
        let mut flows = snapshot.map(|snap| snap.flows.clone()).unwrap_or_default();
        flows.sort_by_key(|flow| flow.sort_order);
        flows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RuntimeFlow;
    use crate::mock::synthetic_snapshot;

    #[test]
    fn flow_list_cache_matches_the_old_sort_and_titles() {
        let snapshot = synthetic_snapshot(3, 12, 5, 40);
        let cache = FlowListCache::build(Some(&snapshot));
        let expected = reference::sorted_flows(Some(&snapshot));
        assert_eq!(*cache.flows, expected);
        assert_eq!(
            *cache.titles_lower,
            expected
                .iter()
                .map(|flow| flow.title().to_lowercase())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            *cache.history_names,
            expected
                .iter()
                .map(|flow| (flow.id.clone(), flow.name.clone()))
                .collect::<Vec<_>>()
        );
        let empty = FlowListCache::build(None);
        assert!(empty.flows.is_empty());
        assert!(reference::sorted_flows(None).is_empty());
    }

    #[test]
    fn runtime_cache_matches_the_old_status_lookup() {
        let snapshot = synthetic_snapshot(3, 6, 4, 20);
        let cache = RuntimeCache::build(&snapshot.runtime);
        for flow in &snapshot.flows {
            assert_eq!(
                cache.status(&flow.id),
                reference::flow_status(&snapshot.runtime, &flow.id)
            );
        }
        assert_eq!(cache.status("no-such-flow"), "idle");
        assert_eq!(
            reference::flow_status(&snapshot.runtime, "no-such-flow"),
            "idle"
        );
        let mut blank = snapshot.runtime.clone();
        blank.flows[0].status = String::new();
        assert_eq!(
            RuntimeCache::build(&blank).status(&blank.flows[0].id),
            "idle"
        );
    }

    #[test]
    fn remote_cache_friendly_names_match_the_old_lookup() {
        let snapshot = synthetic_snapshot(4, 3, 3, 10);
        let cache = RemoteCache::build(Some(&snapshot));
        assert_eq!(*cache.remotes, snapshot.remotes);
        for remote in &snapshot.remotes {
            let expected = if remote.description.trim().is_empty() {
                remote.name.clone()
            } else {
                remote.description.clone()
            };
            assert_eq!(cache.friendly.get(&remote.name).unwrap(), &expected);
            // The old lookup fell back to the raw name for an unknown remote.
            let fallback = snapshot
                .remotes
                .iter()
                .find(|known| known.name == "no-such-remote")
                .map(|known| {
                    if known.description.trim().is_empty() {
                        known.name.clone()
                    } else {
                        known.description.clone()
                    }
                })
                .unwrap_or_else(|| "no-such-remote".to_string());
            assert_eq!(fallback, "no-such-remote");
        }
        assert!(cache.friendly.get("no-such-remote").is_none());
        let empty = RemoteCache::build(None);
        assert!(empty.remotes.is_empty());
        assert!(empty.friendly.is_empty());
    }

    #[test]
    fn node_status_index_matches_the_old_per_node_scan() {
        let snapshot = synthetic_snapshot(3, 6, 8, 20);
        let selected = &snapshot.flows[0];
        let graph = crate::graph::to_graph(selected);
        let cache = GraphCache::build(Some(&graph), &snapshot.runtime, &selected.id, 0, 0);
        for node in &graph.nodes {
            assert_eq!(
                cache
                    .node_status
                    .get(&node.id)
                    .map(String::as_str)
                    .unwrap_or("idle"),
                reference::node_status(&graph, &snapshot.runtime, &selected.id, &node.id)
            );
        }
        // Empty graph and missing flow fall back to idle everywhere.
        let empty = GraphCache::build(None, &snapshot.runtime, &selected.id, 0, 0);
        assert!(empty.node_status.is_empty());
        let missing = GraphCache::build(Some(&graph), &snapshot.runtime, "missing-flow", 0, 0);
        assert!(missing.node_status.values().all(|status| status == "idle"));
        assert_eq!(
            reference::node_status(
                &graph,
                &snapshot.runtime,
                "missing-flow",
                &graph.nodes[0].id
            ),
            "idle"
        );
    }

    #[test]
    fn node_status_fold_handles_failed_completed_and_running_order() {
        // Build a tiny graph and runtime by hand: one node with a failed edge
        // followed by a completed edge, one with completed then failed, and one
        // with a running edge after a completed one.
        let node = |id: &str| GraphNode {
            id: id.into(),
            remote: String::new(),
            path: String::new(),
            label: String::new(),
            x: 0.0,
            y: 0.0,
        };
        let edge = |id: &str, source: &str, target: &str| GraphEdge {
            id: id.into(),
            source: source.into(),
            target: target.into(),
            action: "push".into(),
            operation: crate::engine::Operation::default(),
        };
        let graph = FlowGraph {
            nodes: vec![node("a"), node("b"), node("c")],
            edges: vec![
                edge("e1", "a", "b"),
                edge("e2", "a", "c"),
                edge("e3", "b", "c"),
            ],
            viewport: Default::default(),
        };
        let op = |id: &str, status: &str| crate::engine::RuntimeOp {
            id: id.into(),
            status: status.into(),
            ..Default::default()
        };
        let runtime = Runtime {
            flows: vec![RuntimeFlow {
                id: "f".into(),
                ops: vec![
                    op("e1", "failed"),
                    op("e2", "completed"),
                    op("e3", "running"),
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let statuses = node_statuses(&graph.nodes, &graph.edges, &runtime, "f");
        for node in &graph.nodes {
            assert_eq!(
                statuses.get(&node.id).map(String::as_str).unwrap_or("idle"),
                reference::node_status(&graph, &runtime, "f", &node.id)
            );
        }
        assert_eq!(statuses["a"], "failed");
        assert_eq!(statuses["b"], "running");
        assert_eq!(statuses["c"], "running");
    }

    #[test]
    fn sync_cache_matches_per_edge_sync_for_and_rankings() {
        let snapshot = synthetic_snapshot(3, 8, 8, 60);
        let selected = &snapshot.flows[0];
        let graph = crate::graph::to_graph(selected);
        let cache = SyncCache::build(&snapshot.runtime, &selected.id, &graph.edges, 0, 0);
        assert_eq!(cache.flow_id, selected.id);
        for edge in &graph.edges {
            let expected = crate::sync_for(&snapshot.runtime, &selected.id, &edge.id);
            match (expected, cache.progresses.get(&edge.id)) {
                (None, None) => {
                    assert!(cache.ranked.get(&edge.id).is_none());
                    assert!(cache.dots.get(&edge.id).is_none());
                }
                (Some(expected), Some(cached)) => {
                    assert_eq!(**cached, expected);
                    let order = cache.ranked.get(&edge.id).unwrap();
                    let transfers = &cached.transfers;
                    let old: Vec<_> = crate::format::ranked_transfers(transfers);
                    let new: Vec<&_> = order
                        .iter()
                        .map(|&index| &transfers[index as usize])
                        .collect();
                    assert_eq!(new, old);
                    assert_eq!(
                        **cache.dots.get(&edge.id).unwrap(),
                        crate::format::file_dots(&edge.action, transfers)
                    );
                }
                _ => panic!("progress presence mismatch for edge {}", edge.id),
            }
        }
        assert!(!cache.matches(&selected.id, 1, 0));
        assert!(cache.matches(&selected.id, 0, 0));
        let empty = SyncCache::build(&snapshot.runtime, "missing-flow", &graph.edges, 0, 0);
        assert!(empty.progresses.is_empty());
        // Node progress: the first incident edge with progress, per node, in
        // edge order — the old `edges.iter().find_map` scan per node card.
        for node in &graph.nodes {
            let expected = graph.edges.iter().find_map(|edge| {
                if edge.source == node.id || edge.target == node.id {
                    crate::sync_for(&snapshot.runtime, &selected.id, &edge.id)
                } else {
                    None
                }
            });
            assert_eq!(
                cache.node_progress.get(&node.id).map(|p| p.as_ref()),
                expected.as_ref(),
                "node progress for {}",
                node.id
            );
        }
    }

    #[test]
    fn sync_for_equivalence_on_the_reference_clone_first_logic() {
        let snapshot = synthetic_snapshot(3, 8, 8, 30);
        let selected = &snapshot.flows[0];
        let graph = crate::graph::to_graph(selected);
        let mut probes: Vec<String> = graph.edges.iter().map(|edge| edge.id.clone()).collect();
        probes.push("missing-operation".into());
        probes.push(String::new());
        for op_id in &probes {
            for flow_id in [&selected.id, &"missing-flow".to_string(), &String::new()] {
                assert_eq!(
                    crate::sync_for(&snapshot.runtime, flow_id, op_id),
                    reference::sync_for(&snapshot.runtime, flow_id, op_id),
                    "flow {flow_id:?} op {op_id:?}"
                );
            }
        }
        // Suffix and exact-profile matches stay equivalent on a perturbed copy.
        let mut runtime = snapshot.runtime.clone();
        let flow = runtime.flows.last_mut().unwrap();
        let sync = flow.sync.as_mut().unwrap();
        sync.profile_id = selected.operations[0].id.clone();
        for op_id in probes {
            assert_eq!(
                crate::sync_for(&runtime, &selected.id, &op_id),
                reference::sync_for(&runtime, &selected.id, &op_id),
                "perturbed op {op_id:?}"
            );
        }
    }

    #[test]
    fn rail_matches_equals_the_old_filter() {
        let snapshot = synthetic_snapshot(3, 25, 3, 10);
        let cache = FlowListCache::build(Some(&snapshot));
        let flows = &cache.flows;
        for query in [
            "",
            "synthetic",
            "SYNTHETIC",
            "flow 1",
            "nothing matches this",
        ] {
            let lower = query.trim().to_lowercase();
            let expected: Vec<u32> = reference::rail_shown(flows, &lower)
                .into_iter()
                .map(|index| index as u32)
                .collect();
            let got = rail_matches(&cache.titles_lower, &lower);
            assert_eq!(got, expected, "query {query:?}");
        }
        let empty = FlowListCache::empty();
        assert!(rail_matches(&empty.titles_lower, "anything").is_empty());
        assert_eq!(rail_matches(&empty.titles_lower, "").len(), 0);
    }
}
