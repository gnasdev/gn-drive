//! Board DAG execution — port of `BoardEngine.swift` / `internal/boardengine`.
//!
//! Edges are grouped into topological layers; edges inside one layer run in
//! parallel, layers run in order. Runs are tracked per board id and publish
//! `board:execution` events.

use crate::event_bus::{BusEvent, EventBus};
use crate::logger::Logger;
use crate::models::{Board, BoardEdge, BoardNode};
use crate::rclone::{
    CancellationToken, ProfileFlags, RcloneAction, SyncClient, SyncConfig, SyncFailedError,
};
use crate::store::{Store, StoreError};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

/// Display texts match the Swift RPC messages / error descriptions.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BoardEngineError {
    #[error("board engine is not ready")]
    NotConfigured,
    #[error("board has no nodes or edges")]
    EmptyBoard,
    #[error("board is already running")]
    AlreadyRunning,
    #[error("board is not running")]
    NotRunning,
    #[error("board run was cancelled")]
    Cancelled,
    #[error("board edge {0} references a missing node")]
    MissingNode(String),
    #[error("{0}")]
    Failed(String),
    #[error("{0}")]
    CompletedWithErrors(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BoardRunStatus {
    pub run_id: String,
    pub board_id: String,
    /// running | completed | failed | cancelled
    pub status: String,
    pub error: String,
}

struct Run {
    id: String,
    token: CancellationToken,
    done: bool,
    err: Option<BoardEngineError>,
}

pub type EdgeProgress<'a> =
    &'a (dyn Fn(usize, usize, usize, &BoardEdge, &BoardNode, &BoardNode, Option<&str>) + Sync);

pub struct BoardEngine {
    store: Mutex<Option<Arc<Store>>>,
    sync: Mutex<Option<Arc<dyn SyncClient>>>,
    bus: EventBus,
    log: Logger,
    runs: Mutex<HashMap<String, Run>>,
}

impl BoardEngine {
    pub fn new(log: Logger, bus: EventBus) -> Arc<BoardEngine> {
        Arc::new(BoardEngine {
            store: Mutex::new(None),
            sync: Mutex::new(None),
            bus,
            log,
            runs: Mutex::new(HashMap::new()),
        })
    }

    pub fn attach(&self, store: Arc<Store>, sync: Arc<dyn SyncClient>) {
        *self.store.lock() = Some(store);
        *self.sync.lock() = Some(sync);
    }

    pub fn detach(&self) {
        *self.store.lock() = None;
        *self.sync.lock() = None;
    }

    pub fn status(&self, board_id: &str) -> Option<BoardRunStatus> {
        let runs = self.runs.lock();
        let r = runs.get(board_id)?;
        let mut st = BoardRunStatus {
            run_id: r.id.clone(),
            board_id: board_id.to_string(),
            ..Default::default()
        };
        if r.done {
            match &r.err {
                Some(BoardEngineError::Cancelled) => st.status = "cancelled".into(),
                Some(e) => {
                    st.status = "failed".into();
                    st.error = e.to_string();
                }
                None => st.status = "completed".into(),
            }
        } else {
            st.status = "running".into();
        }
        Some(st)
    }

    fn configured(&self) -> Result<(Arc<Store>, Arc<dyn SyncClient>), BoardEngineError> {
        match (self.store.lock().clone(), self.sync.lock().clone()) {
            (Some(s), Some(c)) => Ok((s, c)),
            _ => Err(BoardEngineError::NotConfigured),
        }
    }

    fn load(store: &Store, board_id: &str) -> Result<Board, BoardEngineError> {
        let b = store.load_board_graph(board_id)?;
        if b.nodes.is_empty() || b.edges.is_empty() {
            return Err(BoardEngineError::EmptyBoard);
        }
        Ok(b)
    }

    /// Execute in the background; returns the run id.
    pub fn execute(
        self: &Arc<Self>,
        board_id: &str,
        stop_on_error: bool,
    ) -> Result<String, BoardEngineError> {
        let (store, sync) = self.configured()?;
        let b = Self::load(&store, board_id)?;

        let token = CancellationToken::new();
        let run_id = uuid::Uuid::new_v4().to_string().to_uppercase();
        {
            let mut runs = self.runs.lock();
            if runs.get(board_id).is_some_and(|r| !r.done) {
                return Err(BoardEngineError::AlreadyRunning);
            }
            runs.insert(
                board_id.to_string(),
                Run {
                    id: run_id.clone(),
                    token: token.clone(),
                    done: false,
                    err: None,
                },
            );
        }
        self.publish(board_id, "", "", "running", "");

        let me = self.clone();
        let board_id = board_id.to_string();
        let rid = run_id.clone();
        std::thread::Builder::new()
            .name(format!("board-{board_id}"))
            .spawn(move || {
                let res = me.run_board(&b, sync.as_ref(), &token, stop_on_error, None);
                let err = res.err();
                if let Some(r) = me.runs.lock().get_mut(&board_id) {
                    r.done = true;
                    r.err = err.clone();
                }
                match err {
                    Some(err) => {
                        let status = if err == BoardEngineError::Cancelled {
                            "cancelled"
                        } else {
                            "failed"
                        };
                        me.publish(&board_id, "", "", status, &err.to_string());
                        me.log.error(
                            "boardengine: finished with error",
                            &[("board", &board_id), ("err", &err.to_string())],
                        );
                    }
                    None => {
                        me.publish(&board_id, "", "", "completed", "");
                        me.log.info(
                            "boardengine: completed",
                            &[("board", &board_id), ("run", &rid)],
                        );
                    }
                }
            })
            .expect("spawn board thread");
        Ok(run_id)
    }

    /// Synchronous execution with a per-edge progress callback (used by the CLI).
    pub fn execute_sync(
        &self,
        board_id: &str,
        stop_on_error: bool,
        on_progress: EdgeProgress<'_>,
    ) -> Result<(), BoardEngineError> {
        let (store, sync) = self.configured()?;
        let b = Self::load(&store, board_id)?;
        self.run_board(
            &b,
            sync.as_ref(),
            &CancellationToken::new(),
            stop_on_error,
            Some(on_progress),
        )
    }

    pub fn stop(&self, board_id: &str) -> Result<(), BoardEngineError> {
        let token = {
            let runs = self.runs.lock();
            match runs.get(board_id) {
                Some(r) if !r.done => r.token.clone(),
                _ => return Err(BoardEngineError::NotRunning),
            }
        };
        token.cancel();
        Ok(())
    }

    fn run_board(
        &self,
        b: &Board,
        sync: &dyn SyncClient,
        token: &CancellationToken,
        stop_on_error: bool,
        on_progress: Option<EdgeProgress<'_>>,
    ) -> Result<(), BoardEngineError> {
        let node_by_id: HashMap<&str, &BoardNode> =
            b.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let layers = Self::topo_layers(&b.nodes, &b.edges)?;
        let total = b.edges.len();
        let mut base = 0usize;
        let mut first_err: Option<String> = None;

        for (layer_i, layer) in layers.iter().enumerate() {
            if token.is_cancelled() {
                return Err(BoardEngineError::Cancelled);
            }
            let mut resolved = Vec::with_capacity(layer.len());
            for edge in layer {
                match (
                    node_by_id.get(edge.source_id.as_str()),
                    node_by_id.get(edge.target_id.as_str()),
                ) {
                    (Some(s), Some(d)) => resolved.push((edge, *s, *d)),
                    _ => return Err(BoardEngineError::MissingNode(edge.id.clone())),
                }
            }

            // Edges of one layer are independent: run them in parallel.
            let results: Vec<Option<Result<(), SyncFailedError>>> = std::thread::scope(|scope| {
                let handles: Vec<_> = resolved
                    .iter()
                    .enumerate()
                    .map(|(i, (edge, src, dst))| {
                        let idx = base + i + 1;
                        scope.spawn(move || {
                            if token.is_cancelled() {
                                return None;
                            }
                            self.publish(&b.id, &edge.source_id, &edge.id, "running", &edge.action);
                            let r = Self::run_edge(sync, edge, src, dst, Some(token));
                            // A sync that was cancelled (stop, or the token
                            // firing mid-transfer) aborts the board run. It is
                            // not an edge failure.
                            if token.is_cancelled() || r.as_ref().err().is_some_and(|e| e.cancelled)
                            {
                                return None;
                            }
                            let msg = r.as_ref().err().map(|e| e.to_string());
                            if let Some(cb) = on_progress {
                                cb(layer_i, idx, total, edge, src, dst, msg.as_deref());
                            }
                            match &msg {
                                Some(m) => {
                                    self.publish(&b.id, &edge.source_id, &edge.id, "failed", m)
                                }
                                None => self.publish(
                                    &b.id,
                                    &edge.source_id,
                                    &edge.id,
                                    "completed",
                                    &edge.action,
                                ),
                            }
                            Some(r)
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap_or(None))
                    .collect()
            });
            base += resolved.len();

            // Layer order (edge id) decides which error is reported first.
            let mut cancelled = false;
            for (i, res) in results.iter().enumerate() {
                match res {
                    None => cancelled = true,
                    Some(Ok(())) => {}
                    Some(Err(e)) => {
                        if stop_on_error {
                            return Err(BoardEngineError::Failed(format!(
                                "board: stopped at edge {}: {}",
                                resolved[i].0.id, e
                            )));
                        }
                        if first_err.is_none() {
                            first_err = Some(e.to_string());
                        }
                    }
                }
            }
            if cancelled {
                return Err(BoardEngineError::Cancelled);
            }
        }
        if let Some(e) = first_err {
            return Err(BoardEngineError::CompletedWithErrors(format!(
                "board: completed with errors (first: {e})"
            )));
        }
        Ok(())
    }

    fn publish(&self, board_id: &str, node_id: &str, edge_id: &str, status: &str, action: &str) {
        self.bus.publish(BusEvent::board_execution(
            board_id, node_id, edge_id, status, action,
        ));
    }

    // MARK: - Edge + topo helpers

    #[allow(clippy::result_large_err)]
    pub fn run_edge(
        sync: &dyn SyncClient,
        edge: &BoardEdge,
        src: &BoardNode,
        dst: &BoardNode,
        token: Option<&CancellationToken>,
    ) -> Result<(), SyncFailedError> {
        let cfg = SyncConfig {
            action: RcloneAction::parse(&edge.action).unwrap_or(RcloneAction::Push),
            source: node_path(src),
            dest: node_path(dst),
            profile: Some(ProfileFlags {
                transfers: 4,
                ..Default::default()
            }),
            ..Default::default()
        };
        sync.sync(&cfg, None, token).map(|_| ())
    }

    /// Edges grouped by topological layer (sorted by id); error on cycle.
    pub fn topo_layers(
        nodes: &[BoardNode],
        edges: &[BoardEdge],
    ) -> Result<Vec<Vec<BoardEdge>>, BoardEngineError> {
        let mut indeg: HashMap<&str, i64> = nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
        for e in edges {
            *indeg.entry(e.target_id.as_str()).or_insert(0) += 1;
        }
        let mut pending = vec![true; edges.len()];
        let mut layers: Vec<Vec<BoardEdge>> = Vec::new();
        let mut processed = 0usize;
        while processed < edges.len() {
            let mut layer: Vec<usize> = (0..edges.len())
                .filter(|&i| {
                    pending[i] && indeg.get(edges[i].source_id.as_str()).copied().unwrap_or(0) == 0
                })
                .collect();
            if layer.is_empty() {
                return Err(BoardEngineError::Failed(format!(
                    "cycle detected: {} edges could not be ordered",
                    edges.len() - processed
                )));
            }
            layer.sort_by(|&a, &b| edges[a].id.cmp(&edges[b].id));
            for &i in &layer {
                pending[i] = false;
                *indeg.entry(edges[i].target_id.as_str()).or_insert(0) -= 1;
            }
            processed += layer.len();
            layers.push(layer.into_iter().map(|i| edges[i].clone()).collect());
        }
        Ok(layers)
    }
}

fn node_path(n: &BoardNode) -> String {
    if !n.remote_name.is_empty() {
        if n.path.is_empty() || n.path == "/" {
            format!("{}:", n.remote_name)
        } else {
            format!("{}:{}", n.remote_name, n.path)
        }
    } else {
        n.path.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::{Subscriber, topic};
    use crate::rclone::{ProgressFn, SyncResult};
    use std::time::{Duration, Instant};

    fn node(id: &str) -> BoardNode {
        BoardNode {
            id: id.into(),
            remote_name: String::new(),
            path: format!("/{id}"),
            label: id.into(),
            x: 0.0,
            y: 0.0,
        }
    }
    fn edge(id: &str, s: &str, t: &str) -> BoardEdge {
        BoardEdge {
            id: id.into(),
            source_id: s.into(),
            target_id: t.into(),
            action: "push".into(),
            ..Default::default()
        }
    }
    fn ids(layers: &[Vec<BoardEdge>]) -> Vec<Vec<String>> {
        layers
            .iter()
            .map(|l| l.iter().map(|e| e.id.clone()).collect())
            .collect()
    }

    #[test]
    fn topo_simple_chain() {
        let layers = BoardEngine::topo_layers(
            &[node("a"), node("b"), node("c")],
            &[edge("e2", "b", "c"), edge("e1", "a", "b")],
        )
        .unwrap();
        assert_eq!(ids(&layers), vec![vec!["e1"], vec!["e2"]]);
    }

    #[test]
    fn topo_diamond() {
        let nodes = [node("a"), node("b"), node("c"), node("d")];
        let edges = [
            edge("e4", "c", "d"),
            edge("e2", "a", "c"),
            edge("e1", "a", "b"),
            edge("e3", "b", "d"),
        ];
        let layers = BoardEngine::topo_layers(&nodes, &edges).unwrap();
        // d has two incoming edges, but layering is by source indegree: both
        // d-bound edges wait for their source to be fully fed.
        assert_eq!(ids(&layers), vec![vec!["e1", "e2"], vec!["e3", "e4"]]);
    }

    #[test]
    fn topo_cycle_and_missing_node() {
        let e = BoardEngine::topo_layers(
            &[node("a"), node("b")],
            &[edge("e1", "a", "b"), edge("e2", "b", "a")],
        )
        .unwrap_err();
        assert_eq!(
            e,
            BoardEngineError::Failed("cycle detected: 2 edges could not be ordered".into())
        );
        // Edge to an unknown node still orders (target indegree is tracked
        // lazily); the missing node is reported at run time.
        let layers = BoardEngine::topo_layers(&[node("a")], &[edge("e1", "a", "zzz")]).unwrap();
        assert_eq!(ids(&layers), vec![vec!["e1"]]);
        // Unknown *source* has indegree 0 → ordered too.
        assert!(BoardEngine::topo_layers(&[node("a")], &[edge("e1", "q", "a")]).is_ok());
        assert!(BoardEngine::topo_layers(&[], &[]).unwrap().is_empty());
    }

    #[test]
    fn error_texts() {
        assert_eq!(
            BoardEngineError::NotConfigured.to_string(),
            "board engine is not ready"
        );
        assert_eq!(
            BoardEngineError::EmptyBoard.to_string(),
            "board has no nodes or edges"
        );
        assert_eq!(
            BoardEngineError::AlreadyRunning.to_string(),
            "board is already running"
        );
        assert_eq!(
            BoardEngineError::NotRunning.to_string(),
            "board is not running"
        );
        assert_eq!(
            BoardEngineError::MissingNode("e9".into()).to_string(),
            "board edge e9 references a missing node"
        );
    }

    // --- stub sync client ----------------------------------------------

    #[derive(Default)]
    struct StubSync {
        calls: Mutex<Vec<SyncConfig>>,
        fail_sources: Vec<String>,
        block_until_cancel: bool,
    }

    impl SyncClient for StubSync {
        fn sync(
            &self,
            cfg: &SyncConfig,
            _p: Option<ProgressFn<'_>>,
            token: Option<&CancellationToken>,
        ) -> Result<SyncResult, SyncFailedError> {
            self.calls.lock().push(cfg.clone());
            if self.block_until_cancel {
                let t = Instant::now();
                while !token.is_some_and(|t| t.is_cancelled())
                    && t.elapsed() < Duration::from_secs(5)
                {
                    std::thread::sleep(Duration::from_millis(2));
                }
                return Err(SyncFailedError {
                    result: SyncResult::default(),
                    message: "rclone: cancelled".into(),
                    cancelled: true,
                });
            }
            if self.fail_sources.contains(&cfg.source) {
                return Err(SyncFailedError {
                    result: SyncResult::default(),
                    message: format!("boom {}", cfg.source),
                    cancelled: false,
                });
            }
            Ok(SyncResult::default())
        }
    }

    #[test]
    fn run_edge_paths_and_default_action() {
        let s = StubSync::default();
        let mut e = edge("e1", "", "");
        let src = BoardNode {
            remote_name: "r1".into(),
            path: "src".into(),
            ..node("n1")
        };
        let dst = BoardNode {
            path: "/local/dst".into(),
            ..node("n2")
        };
        BoardEngine::run_edge(&s, &e, &src, &dst, None).unwrap();
        e.action = String::new();
        let root = BoardNode {
            remote_name: "b".into(),
            path: "/".into(),
            ..node("n3")
        };
        BoardEngine::run_edge(&s, &e, &src, &root, None).unwrap();
        let calls = s.calls.lock();
        assert_eq!(calls[0].source, "r1:src");
        assert_eq!(calls[0].dest, "/local/dst");
        assert_eq!(calls[0].action, RcloneAction::Push);
        assert_eq!(calls[0].profile.as_ref().unwrap().transfers, 4);
        assert_eq!(calls[1].dest, "b:");
        assert_eq!(calls[1].action, RcloneAction::Push);
    }

    #[test]
    fn run_edge_error_propagates() {
        let s = StubSync {
            fail_sources: vec!["/a".into()],
            ..Default::default()
        };
        let e = BoardEngine::run_edge(&s, &edge("e", "a", "b"), &node("a"), &node("b"), None)
            .unwrap_err();
        assert_eq!(e.to_string(), "boom /a");
    }

    // --- engine ---------------------------------------------------------

    struct Fx {
        _dir: tempfile::TempDir,
        eng: Arc<BoardEngine>,
        stub: Arc<StubSync>,
        sub: Subscriber,
    }

    /// a→b (e1), a→c (e2) in layer 0; b→d (e3) in layer 1.
    fn fixture(stub: StubSync) -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let log = Logger::quiet();
        let store =
            Arc::new(Store::open(dir.path().join("db.db").to_str().unwrap(), &log).unwrap());
        let board = Board {
            id: "b1".into(),
            name: "B".into(),
            nodes: vec![node("a"), node("b"), node("c"), node("d")],
            edges: vec![
                edge("e1", "a", "b"),
                edge("e2", "a", "c"),
                edge("e3", "b", "d"),
            ],
            ..Default::default()
        };
        store.save_board_graph(&board).unwrap();
        let empty = Board {
            id: "empty".into(),
            name: "E".into(),
            ..Default::default()
        };
        store.save_board_graph(&empty).unwrap();
        let bus = EventBus::new();
        let sub = bus.subscribe(&[topic::BOARD_EXECUTION]);
        let stub = Arc::new(stub);
        let eng = BoardEngine::new(log, bus);
        eng.attach(store, stub.clone());
        Fx {
            _dir: dir,
            eng,
            stub,
            sub,
        }
    }

    fn drain(fx: &Fx) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        while let Some(BusEvent::BoardExecution(e)) = fx.sub.try_recv() {
            out.push((e.edge_id, e.status, e.action));
        }
        out
    }

    fn wait_status(fx: &Fx, want: &str) -> BoardRunStatus {
        let t = Instant::now();
        loop {
            if let Some(s) = fx.eng.status("b1")
                && s.status == want
            {
                return s;
            }
            assert!(
                t.elapsed() < Duration::from_secs(5),
                "timeout waiting {want}"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn not_configured_empty_missing_and_not_running() {
        let e = BoardEngine::new(Logger::quiet(), EventBus::new());
        assert_eq!(e.execute("x", true), Err(BoardEngineError::NotConfigured));
        assert_eq!(e.stop("x"), Err(BoardEngineError::NotRunning));
        assert!(e.status("x").is_none());

        let fx = fixture(StubSync::default());
        assert_eq!(
            fx.eng.execute("empty", true),
            Err(BoardEngineError::EmptyBoard)
        );
        assert_eq!(
            fx.eng.execute("nope", true),
            Err(BoardEngineError::Store(StoreError::NotFound))
        );
        assert_eq!(fx.eng.stop("b1"), Err(BoardEngineError::NotRunning));
        fx.eng.detach();
        assert_eq!(
            fx.eng.execute("b1", true),
            Err(BoardEngineError::NotConfigured)
        );
    }

    #[test]
    fn execute_sync_success_layers_and_progress() {
        let fx = fixture(StubSync::default());
        let seen = Mutex::new(Vec::new());
        fx.eng
            .execute_sync("b1", true, &|layer, idx, total, e, _s, _d, err| {
                seen.lock()
                    .push((layer, idx, total, e.id.clone(), err.is_none()));
            })
            .unwrap();
        let mut seen = seen.into_inner();
        seen.sort();
        assert_eq!(
            seen,
            vec![
                (0, 1, 3, "e1".into(), true),
                (0, 2, 3, "e2".into(), true),
                (1, 3, 3, "e3".into(), true),
            ]
        );
        let evs = drain(&fx);
        let completed: Vec<_> = evs.iter().filter(|e| e.1 == "completed").collect();
        assert_eq!(completed.len(), 3);
        assert!(completed.iter().all(|e| e.2 == "push"));
        // e3 (layer 1) only starts after both layer-0 edges finished.
        let pos = |id: &str, st: &str| evs.iter().position(|e| e.0 == id && e.1 == st).unwrap();
        assert!(pos("e3", "running") > pos("e1", "completed"));
        assert!(pos("e3", "running") > pos("e2", "completed"));
    }

    #[test]
    fn stop_on_error_true_stops_after_failing_layer() {
        let fx = fixture(StubSync {
            fail_sources: vec!["/a".into()],
            ..Default::default()
        });
        let e = fx
            .eng
            .execute_sync("b1", true, &|_, _, _, _, _, _, _| {})
            .unwrap_err();
        assert_eq!(
            e,
            BoardEngineError::Failed("board: stopped at edge e1: boom /a".into())
        );
        // Layer 1 (e3, source /b) never ran.
        assert!(fx.stub.calls.lock().iter().all(|c| c.source == "/a"));
        assert_eq!(fx.stub.calls.lock().len(), 2);
    }

    #[test]
    fn stop_on_error_false_continues_and_reports_first() {
        let fx = fixture(StubSync {
            fail_sources: vec!["/b".into()],
            ..Default::default()
        });
        let e = fx
            .eng
            .execute_sync("b1", false, &|_, _, _, _, _, _, _| {})
            .unwrap_err();
        assert_eq!(
            e,
            BoardEngineError::CompletedWithErrors(
                "board: completed with errors (first: boom /b)".into()
            )
        );
        assert_eq!(fx.stub.calls.lock().len(), 3);
        let evs = drain(&fx);
        assert!(
            evs.iter()
                .any(|e| e.0 == "e3" && e.1 == "failed" && e.2 == "boom /b")
        );
    }

    #[test]
    fn background_execute_completes_and_reports_status() {
        let fx = fixture(StubSync::default());
        let run = fx.eng.execute("b1", true).unwrap();
        let st = wait_status(&fx, "completed");
        assert_eq!(st.run_id, run);
        assert_eq!(st.board_id, "b1");
        assert_eq!(st.error, "");
        let evs = drain(&fx);
        assert_eq!(evs.first().unwrap().1, "running"); // board-level
        assert_eq!(evs.first().unwrap().0, "");
        assert_eq!(evs.last().unwrap().1, "completed");
        assert_eq!(evs.last().unwrap().0, "");
        // Finished runs can be started again.
        fx.eng.execute("b1", true).unwrap();
    }

    #[test]
    fn background_execute_failure_status() {
        let fx = fixture(StubSync {
            fail_sources: vec!["/a".into()],
            ..Default::default()
        });
        fx.eng.execute("b1", true).unwrap();
        let st = wait_status(&fx, "failed");
        assert_eq!(st.error, "board: stopped at edge e1: boom /a");
    }

    #[test]
    fn stop_cancels_running_board() {
        let fx = fixture(StubSync {
            block_until_cancel: true,
            ..Default::default()
        });
        fx.eng.execute("b1", true).unwrap();
        assert_eq!(
            fx.eng.execute("b1", true),
            Err(BoardEngineError::AlreadyRunning)
        );
        let t = Instant::now();
        while fx.stub.calls.lock().len() < 2 {
            assert!(t.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(2));
        }
        fx.eng.stop("b1").unwrap();
        let st = wait_status(&fx, "cancelled");
        assert_eq!(st.error, "");
        let evs = drain(&fx);
        assert_eq!(evs.last().unwrap().1, "cancelled");
        assert_eq!(fx.eng.stop("b1"), Err(BoardEngineError::NotRunning));
    }
}
