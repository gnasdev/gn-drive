//! Selected-workspace pure-data subtotal; excludes GPUI, node-status/edge geometry and other pages.

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

use crate::engine::{FileTransfer, Flow, Remote, Runtime, Snapshot, SyncProgress};
use crate::format::{file_dots, ranked_transfers};
use crate::graph::{self, CanvasLayer, FlowGraph, GraphEdge, GraphNode};
use crate::mock::synthetic_snapshot;
use crate::{sorted_flows, sync_for};

fn median_ms<T>(name: &str, mut work: impl FnMut() -> T) {
    drop(black_box(work()));
    let mut samples = [0.0_f64; 7];
    for sample in &mut samples {
        let start = Instant::now();
        drop(black_box(work()));
        *sample = start.elapsed().as_secs_f64() * 1000.0;
    }
    samples.sort_by(f64::total_cmp);
    println!("BENCH {name}_ms={:.6}", samples[3]);
}

fn rail_filter_and_status(flows: &[Flow], runtime: &Runtime) {
    let query = black_box("Synthetic").trim().to_lowercase();
    let shown: Vec<&Flow> = flows
        .iter()
        .filter(|flow| query.is_empty() || flow.title().to_lowercase().contains(&query))
        .collect();
    for flow in &shown {
        // Same linear lookup and status clone as GnDriveWindow::flow_status.
        let status = runtime
            .flows
            .iter()
            .find(|entry| entry.id == flow.id)
            .map(|entry| entry.status.clone())
            .filter(|status| !status.is_empty())
            .unwrap_or_else(|| "idle".into());
        black_box(status);
    }
    black_box(shown);
}

fn canvas_progresses(
    runtime: &Runtime,
    flow_id: &str,
    edges: &[GraphEdge],
) -> HashMap<String, SyncProgress> {
    let mut progresses = HashMap::new();
    for edge in edges {
        // Preserve the production helper's full clone even for nonmatching profiles.
        if let Some(progress) =
            sync_for(black_box(runtime), black_box(flow_id), black_box(&edge.id))
        {
            progresses.insert(edge.id.clone(), progress);
        }
    }
    progresses
}

fn rank_rows(transfers: &[FileTransfer], limit: usize) {
    let ranked = ranked_transfers(black_box(transfers));
    for transfer in ranked.iter().take(limit) {
        black_box(transfer);
    }
    black_box(ranked);
}

fn node_card_copies(nodes: &[GraphNode], remotes: &[Remote]) {
    for node in nodes {
        let node = black_box(node.clone());
        let heading = graph::folder_title(&node.label, &node.path);
        let mode = graph::node_mode(1.0, graph::NODE_W, &heading);
        let rect = graph::rendered_node_rect(node.x, node.y, mode, 1.0, &heading);
        // node_place currently clones all remotes for each nonlocal node.
        let friendly = if node.remote.is_empty() || node.remote == "local" {
            String::new()
        } else {
            let owned_remotes = remotes.to_vec();
            owned_remotes
                .into_iter()
                .find(|remote| remote.name == node.remote)
                .map(|remote| {
                    if remote.description.trim().is_empty() {
                        remote.name
                    } else {
                        remote.description
                    }
                })
                .unwrap_or_else(|| node.remote.clone())
        };
        let place = graph::place_name(&node.remote, &friendly);
        let path = if node.path.trim().is_empty() {
            "/".to_string()
        } else {
            node.path.clone()
        };
        let copy =
            graph::node_card_copy(&node.label, &path, &place, graph::node_path_chars(rect.w));
        let tip = format!("{}\n{}\n{path}", copy.title, copy.place);
        black_box((node, rect, heading, copy, tip));
    }
}

fn canvas_layers(edges: usize, nodes: usize) -> Vec<CanvasLayer> {
    let mut layers = vec![CanvasLayer::Edges];
    layers.resize(1 + edges, CanvasLayer::EdgeLabels);
    layers.resize(1 + edges + nodes, CanvasLayer::Nodes);
    // Nonempty canvas: zoom controls and one open selected-edge popover.
    layers.push(CanvasLayer::Chrome);
    layers.push(CanvasLayer::Popovers);
    layers
}

fn inspector_data(runtime: &Runtime, flow_id: &str, graph: &FlowGraph, edge_id: &str) {
    let edge = graph
        .edges
        .iter()
        .find(|edge| edge.id == edge_id)
        .unwrap()
        .clone();
    let sync = sync_for(black_box(runtime), black_box(flow_id), black_box(&edge.id));
    if let Some(sync) = &sync {
        rank_rows(&sync.transfers, 6);
    }
    black_box((edge, sync));
}

fn selected_workspace_data(snapshot: &Snapshot, graph: &FlowGraph) {
    let flows = sorted_flows(Some(black_box(snapshot)));
    rail_filter_and_status(&flows, &snapshot.runtime);

    let nodes = graph.nodes.clone();
    let edges = graph.edges.clone();
    let flow_id = snapshot.flows[0].id.clone();
    let progresses = canvas_progresses(&snapshot.runtime, &flow_id, &edges);
    let paint_nodes = nodes.clone();
    let paint_edges = edges.clone();
    let paint_progress = progresses.clone();
    let selected_edge = &edges[0];
    if let Some(sync) = progresses.get(&selected_edge.id) {
        rank_rows(&sync.transfers, 8);
    }
    node_card_copies(&paint_nodes, &snapshot.remotes);
    let layers = canvas_layers(edges.len(), nodes.len());
    black_box(graph::canvas_child_order(black_box(&layers)));
    // Only the matching edge invokes file_dots in the painter.
    for edge in &paint_edges {
        if let Some(sync) = paint_progress.get(&edge.id) {
            black_box(file_dots(
                black_box(&edge.action),
                black_box(&sync.transfers),
            ));
        }
    }
    inspector_data(&snapshot.runtime, &flow_id, graph, &selected_edge.id);
    black_box((
        flows,
        nodes,
        edges,
        progresses,
        paint_nodes,
        paint_edges,
        paint_progress,
    ));
}

fn assert_fixture(
    snapshot: &Snapshot,
    graph: &FlowGraph,
    remotes: usize,
    flows: usize,
    edges: usize,
    transfers: usize,
) {
    assert_eq!(snapshot.remotes.len(), remotes);
    assert_eq!(snapshot.flows.len(), flows);
    assert_eq!(snapshot.runtime.flows.len(), flows);
    assert_eq!(graph.edges.len(), edges);
    assert_eq!(graph.nodes.len(), edges * 2);
    let selected = &snapshot.flows[0];
    assert_eq!(selected.operations.len(), edges);
    assert_eq!(snapshot.runtime.flows.last().unwrap().id, selected.id);
    for flow in &snapshot.flows {
        let runtime = snapshot
            .runtime
            .flows
            .iter()
            .find(|entry| entry.id == flow.id)
            .unwrap();
        assert_eq!(runtime.ops.len(), flow.operations.len());
        if flow.id == selected.id {
            assert_eq!(runtime.status, "running");
            assert_eq!(runtime.ops[0].status, "running");
            assert!(runtime.ops[1..].iter().all(|op| op.status == "idle"));
        } else {
            assert_eq!(flow.operations.len(), 1);
            assert_eq!(runtime.status, "idle");
            assert!(runtime.ops.iter().all(|op| op.status == "idle"));
            assert!(runtime.sync.is_none());
            assert!(sync_for(&snapshot.runtime, &flow.id, &flow.operations[0].id).is_none());
        }
    }
    let progresses = canvas_progresses(&snapshot.runtime, &selected.id, &graph.edges);
    assert_eq!(progresses.len(), 1);
    let progress = &progresses[&graph.edges[0].id];
    assert_eq!(progress.transfers.len(), transfers);
    assert_eq!(progress.total_files, transfers as u64);
    assert_eq!(
        progress.profile_id,
        format!("{}:{}", selected.id, graph.edges[0].id)
    );
    assert!(sync_for(&snapshot.runtime, &selected.id, "missing-operation").is_none());
    assert!(sync_for(&snapshot.runtime, "missing-flow", &graph.edges[0].id).is_none());
    let ranked = ranked_transfers(&progress.transfers);
    assert_eq!(ranked.len(), transfers);
    assert_eq!(ranked.iter().take(8).count(), transfers.min(8));
    assert_eq!(ranked.iter().take(6).count(), transfers.min(6));
    assert_eq!(ranked[0].status, "transferring");
    let visible = progress
        .transfers
        .iter()
        .filter(|file| file.status != "pending")
        .count();
    assert_eq!(
        file_dots(&progress.action, &progress.transfers).len(),
        visible.min(24)
    );
    let sorted = sorted_flows(Some(snapshot));
    assert!(
        sorted
            .windows(2)
            .all(|pair| pair[0].sort_order <= pair[1].sort_order)
    );
    assert!(
        sorted
            .iter()
            .map(|flow| &flow.id)
            .eq(snapshot.flows.iter().rev().map(|flow| &flow.id))
    );
    assert!(sorted_flows(None).is_empty());
    assert_eq!(
        graph::canvas_child_order(&canvas_layers(edges, edges * 2)).len(),
        edges * 3 + 3
    );
}

#[test]
fn synthetic_snapshot_helpers_small() {
    let snapshot = synthetic_snapshot(3, 4, 5, 24);
    let selected = &snapshot.flows[0];
    let graph = graph::to_graph(selected);
    assert_fixture(&snapshot, &graph, 3, 4, 5, 24);
    assert_eq!(
        serde_json::to_value(&snapshot).unwrap(),
        serde_json::to_value(synthetic_snapshot(3, 4, 5, 24)).unwrap()
    );
    let op_id = &selected.operations[0].id;
    let mut matching = sync_for(&snapshot.runtime, &selected.id, op_id).unwrap();
    matching.transfers[0].name = "changed clone".into();
    assert_ne!(
        matching.transfers[0].name,
        snapshot
            .runtime
            .flows
            .last()
            .unwrap()
            .sync
            .as_ref()
            .unwrap()
            .transfers[0]
            .name
    );
    // Preserve the helper's legacy suffix matching as well as exact profiles.
    let mut runtime = snapshot.runtime.clone();
    runtime
        .flows
        .last_mut()
        .unwrap()
        .sync
        .as_mut()
        .unwrap()
        .profile_id = op_id.clone();
    assert!(sync_for(&runtime, &selected.id, op_id).is_some());
    assert!(sync_for(&runtime, &selected.id, &selected.operations[1].id).is_none());
    selected_workspace_data(&snapshot, &graph);
}

#[test]
#[ignore]
fn perf_bench_large() {
    let snapshot = synthetic_snapshot(20, 200, 200, 20_000);
    let selected = &snapshot.flows[0];
    let graph = graph::to_graph(selected);
    assert_fixture(&snapshot, &graph, 20, 200, 200, 20_000);
    let sorted = sorted_flows(Some(&snapshot));
    let progresses = canvas_progresses(&snapshot.runtime, &selected.id, &graph.edges);
    let progress = &progresses[&graph.edges[0].id];
    let layers = canvas_layers(graph.edges.len(), graph.nodes.len());

    median_ms("to_graph_load_only", || {
        graph::to_graph(black_box(selected))
    });
    median_ms("sorted_flows", || sorted_flows(Some(black_box(&snapshot))));
    median_ms("flow_rail_filter_status", || {
        rail_filter_and_status(black_box(&sorted), black_box(&snapshot.runtime))
    });
    median_ms("canvas_graph_copies", || {
        let nodes = black_box(&graph.nodes).clone();
        let edges = black_box(&graph.edges).clone();
        let paint_nodes = nodes.clone();
        let paint_edges = edges.clone();
        (nodes, edges, paint_nodes, paint_edges)
    });
    median_ms("canvas_sync_for_edges", || {
        canvas_progresses(&snapshot.runtime, &selected.id, &graph.edges)
    });
    median_ms("paint_progress_clone", || black_box(&progresses).clone());
    median_ms("popover_rank_all_take8", || {
        rank_rows(&progress.transfers, 8)
    });
    median_ms("inspector_sync_rank_all_take6", || {
        inspector_data(&snapshot.runtime, &selected.id, &graph, &graph.edges[0].id)
    });
    median_ms("file_dots", || {
        file_dots(black_box(&progress.action), black_box(&progress.transfers))
    });
    median_ms("canvas_child_order", || {
        graph::canvas_child_order(black_box(&layers))
    });
    median_ms("node_card_copy", || {
        node_card_copies(black_box(&graph.nodes), black_box(&snapshot.remotes))
    });
    median_ms("per_frame_total", || {
        selected_workspace_data(black_box(&snapshot), black_box(&graph))
    });
}
