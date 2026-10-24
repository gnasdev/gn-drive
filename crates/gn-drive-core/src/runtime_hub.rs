//! In-process projection of live run state — port of `RuntimeHub.swift` /
//! `internal/runtimehub`.
//!
//! Listens to `flow:execution` and `sync:*` events, seeds snapshots from the
//! engines' statuses and active tasks, and serves runtime snapshots for UI
//! hydration.

use crate::event_bus::{
    BusEvent, EventBus, FlowExecutionEvent, RuntimeFlowState, RuntimeLogEntry, RuntimeOpState,
    RuntimeSnapshotEvent, SyncProgressEvent, topic,
};
use crate::flow_engine::FlowEngine;
use crate::sync_engine::{SyncEngine, TaskSnapshot};
use chrono::Utc;
use crossbeam_channel::{Sender, select};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;

const MAX_LOG: usize = 40;

#[derive(Clone, Default)]
struct OpRuntime {
    status: String,
    last_error: String,
}

#[derive(Clone, Default)]
struct FlowRuntime {
    status: String,
    last_error: String,
    ops: HashMap<String, OpRuntime>,
    sync: Option<SyncProgressEvent>,
    log: Vec<RuntimeLogEntry>,
}

#[derive(Default)]
struct State {
    rev: i64,
    flows: HashMap<String, FlowRuntime>,
}

pub struct RuntimeHub {
    state: Mutex<State>,
    flows_source: Option<Weak<FlowEngine>>,
    tasks_source: Option<Weak<SyncEngine>>,
    closer: Mutex<Option<(Sender<()>, JoinHandle<()>)>>,
}

impl RuntimeHub {
    pub fn new(
        bus: &EventBus,
        flows: Option<Arc<FlowEngine>>,
        tasks: Option<Arc<SyncEngine>>,
    ) -> Arc<RuntimeHub> {
        let hub = Arc::new(RuntimeHub {
            state: Mutex::new(State::default()),
            flows_source: flows.as_ref().map(Arc::downgrade),
            tasks_source: tasks.as_ref().map(Arc::downgrade),
            closer: Mutex::new(None),
        });
        let sub = bus.subscribe(&[
            topic::FLOW_EXECUTION,
            topic::SYNC_STARTED,
            topic::SYNC_PROGRESS,
            topic::SYNC_COMPLETED,
            topic::SYNC_FAILED,
        ]);
        let (stop_tx, stop_rx) = crossbeam_channel::bounded::<()>(0);
        let weak = Arc::downgrade(&hub);
        let handle = std::thread::Builder::new()
            .name("runtime-hub".into())
            .spawn(move || {
                loop {
                    select! {
                        // Closed (sender dropped) → exit.
                        recv(stop_rx) -> _ => break,
                        recv(sub.receiver()) -> ev => match ev {
                            Ok(ev) => match weak.upgrade() {
                                Some(h) => h.on_event(&ev),
                                None => break,
                            },
                            Err(_) => break, // bus closed
                        },
                    }
                }
            })
            .expect("spawn runtime hub thread");
        *hub.closer.lock() = Some((stop_tx, handle));
        hub
    }

    /// Stop listening to the bus (idempotent).
    pub fn close(&self) {
        let taken = self.closer.lock().take();
        if let Some((tx, handle)) = taken {
            drop(tx);
            if handle.thread().id() != std::thread::current().id() {
                let _ = handle.join();
            }
        }
    }

    /// Clear the projection (lock / data-plane teardown).
    pub fn reset(&self) {
        let mut st = self.state.lock();
        st.flows.clear();
        st.rev += 1;
    }

    pub fn forget(&self, flow_id: &str) {
        if flow_id.is_empty() {
            return;
        }
        let mut st = self.state.lock();
        st.flows.remove(flow_id);
        st.rev += 1;
    }

    /// Snapshot with engine statuses and active tasks overlaid.
    pub fn snapshot(&self) -> RuntimeSnapshotEvent {
        let (rev, mut merged) = {
            let st = self.state.lock();
            (st.rev, st.flows.clone())
        };
        if let Some(fe) = self.flows_source.as_ref().and_then(Weak::upgrade) {
            for (id, status) in fe.statuses() {
                let fr = merged.entry(id).or_default();
                if !status.is_empty() {
                    fr.status = status;
                }
            }
        }
        if let Some(se) = self.tasks_source.as_ref().and_then(Weak::upgrade) {
            for t in se.active_tasks() {
                apply_task(&mut merged, &t);
            }
        }
        let mut flows: Vec<RuntimeFlowState> = merged
            .into_iter()
            .map(|(id, fr)| to_flow_state(id, fr))
            .collect();
        flows.sort_by(|a, b| a.id.cmp(&b.id));
        RuntimeSnapshotEvent {
            timestamp: Utc::now(),
            revision: rev,
            flows,
        }
    }

    pub(crate) fn on_event(&self, ev: &BusEvent) {
        let mut st = self.state.lock();
        match ev {
            BusEvent::FlowExecution(fe) => {
                if fe.flow_id.is_empty() {
                    return;
                }
                apply_flow(&mut st.flows, fe);
            }
            BusEvent::SyncStarted(_)
            | BusEvent::SyncProgress(_)
            | BusEvent::SyncCompleted(_)
            | BusEvent::SyncFailed(_) => apply_sync(&mut st.flows, ev),
            _ => return,
        }
        st.rev += 1;
    }

    /// `"flowID:opID"` → `(flowID, opID)`; busy keys without a usable colon
    /// (missing, leading, or trailing) return `None`.
    pub fn split_busy_key(profile_id: &str) -> Option<(String, String)> {
        let i = profile_id.find(':')?;
        if i == 0 || i + 1 == profile_id.len() {
            return None;
        }
        Some((profile_id[..i].to_string(), profile_id[i + 1..].to_string()))
    }
}

impl Drop for RuntimeHub {
    fn drop(&mut self) {
        // Dropping the sender wakes the worker; it may not be joinable here
        // (the worker can hold the last strong ref only transiently).
        self.closer.lock().take();
    }
}

fn apply_flow(flows: &mut HashMap<String, FlowRuntime>, fe: &FlowExecutionEvent) {
    let fr = flows.entry(fe.flow_id.clone()).or_default();
    let soft = fe.status == "cancelled" || fe.status == "cancelling";
    if !fe.op_id.is_empty() {
        let op = fr.ops.entry(fe.op_id.clone()).or_default();
        op.status = fe.status.clone();
        if !fe.error.is_empty() {
            op.last_error = fe.error.clone();
            if !soft {
                fr.last_error = fe.error.clone();
            }
        }
        if ["running", "cancelling", "failed", "cancelled"].contains(&fe.status.as_str()) {
            fr.status = fe.status.clone();
        }
    } else {
        fr.status = fe.status.clone();
        if !fe.error.is_empty() && !soft {
            fr.last_error = fe.error.clone();
        }
        if soft {
            fr.last_error.clear();
        }
    }
    let label = if fe.op_id.is_empty() {
        "Flow".to_string()
    } else {
        format!("Op {}", fe.op_id.chars().take(8).collect::<String>())
    };
    append_log(
        &mut fr.log,
        RuntimeLogEntry {
            at: Utc::now().timestamp_millis(),
            status: fe.status.clone(),
            op_id: fe.op_id.clone(),
            error: fe.error.clone(),
            label,
        },
    );
}

fn apply_sync(flows: &mut HashMap<String, FlowRuntime>, ev: &BusEvent) {
    let (started, prog) = match ev {
        BusEvent::SyncStarted(st) => (
            true,
            SyncProgressEvent {
                task_id: st.task_id.clone(),
                profile_id: st.profile_id.clone(),
                action: st.action.clone(),
                state: "running".into(),
                ..Default::default()
            },
        ),
        BusEvent::SyncProgress(p) | BusEvent::SyncFailed(p) => (false, p.clone()),
        BusEvent::SyncCompleted(c) => (
            false,
            SyncProgressEvent {
                task_id: c.task_id.clone(),
                profile_id: c.profile_id.clone(),
                action: c.action.clone(),
                state: "completed".into(),
                transferred: c.bytes,
                errors: c.errors,
                ..Default::default()
            },
        ),
        _ => return,
    };
    let Some((flow_id, op_id)) = RuntimeHub::split_busy_key(&prog.profile_id) else {
        return;
    };
    let fr = flows.entry(flow_id).or_default();
    if !op_id.is_empty() {
        let op = fr.ops.entry(op_id).or_default();
        if !prog.state.is_empty() {
            op.status = prog.state.clone();
        } else if started {
            op.status = "running".into();
        }
        if !prog.error_message.is_empty() {
            op.last_error = prog.error_message.clone();
        }
    }
    if started || prog.state == "running" {
        fr.status = "running".into();
    } else if ["failed", "cancelled", "completed"].contains(&prog.state.as_str())
        && fr.status.is_empty()
    {
        fr.status = prog.state.clone();
    }
    if !prog.error_message.is_empty() && prog.state != "cancelled" {
        fr.last_error = prog.error_message.clone();
    }
    fr.sync = Some(prog);
}

fn apply_task(dst: &mut HashMap<String, FlowRuntime>, t: &TaskSnapshot) {
    let Some((flow_id, op_id)) = RuntimeHub::split_busy_key(&t.name) else {
        return;
    };
    let fr = dst.entry(flow_id).or_default();
    let status = if t.status.is_empty() {
        "running".to_string()
    } else {
        t.status.clone()
    };
    fr.status = status.clone();
    if !op_id.is_empty() {
        fr.ops.insert(
            op_id,
            OpRuntime {
                status: status.clone(),
                last_error: String::new(),
            },
        );
    }
    let s = &t.stats;
    fr.sync = Some(SyncProgressEvent {
        timestamp: Utc::now(),
        task_id: t.id.clone(),
        profile_id: t.name.clone(),
        action: t.action.clone(),
        state: status,
        transferred: s.bytes,
        total: s.bytes_total,
        bytes_per_sec: s.speed,
        eta: s.eta,
        errors: s.errors,
        current_file: s.current_file.clone(),
        stage: s.stage.clone(),
        stage_detail: s.stage_detail.clone(),
        files_transferred: s.files,
        total_files: s.files_total,
        checks: s.checks,
        total_checks: s.checks_total,
        deletes: s.deletes,
        renames: s.renames,
        transfers: s.file_transfers.clone(),
        error_message: String::new(),
    });
}

fn to_flow_state(id: String, fr: FlowRuntime) -> RuntimeFlowState {
    let mut ops: Vec<RuntimeOpState> = fr
        .ops
        .into_iter()
        .map(|(id, o)| RuntimeOpState {
            id,
            status: o.status,
            last_error: o.last_error,
        })
        .collect();
    ops.sort_by(|a, b| a.id.cmp(&b.id));
    RuntimeFlowState {
        id,
        status: if fr.status.is_empty() {
            "idle".into()
        } else {
            fr.status
        },
        last_error: fr.last_error,
        ops,
        sync: fr.sync,
        log: fr.log,
    }
}

/// Append with consecutive-duplicate suppression, capped at [`MAX_LOG`].
fn append_log(log: &mut Vec<RuntimeLogEntry>, e: RuntimeLogEntry) {
    if let Some(last) = log.last()
        && last.status == e.status
        && last.op_id == e.op_id
        && last.error == e.error
    {
        return;
    }
    log.push(e);
    if log.len() > MAX_LOG {
        let drop_n = log.len() - MAX_LOG;
        log.drain(..drop_n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::Logger;
    use crate::rclone::types::{FileTransfer, SyncStats};
    use std::time::{Duration, Instant};

    fn hub_with_bus() -> (EventBus, Arc<RuntimeHub>) {
        let bus = EventBus::new();
        let hub = RuntimeHub::new(&bus, None, None);
        (bus, hub)
    }

    fn wait_rev(h: &RuntimeHub, min: i64) {
        let t = Instant::now();
        while h.snapshot().revision < min {
            assert!(
                t.elapsed() < Duration::from_secs(2),
                "revision still {}, want >= {min}",
                h.snapshot().revision
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn snapshot_empty() {
        let bus = EventBus::new();
        let h = RuntimeHub::new(&bus, None, None);
        let s = h.snapshot();
        assert_eq!(s.revision, 0);
        assert!(s.flows.is_empty());
        h.close();
    }

    #[test]
    fn flow_execution_and_sync_projection() {
        let (bus, h) = hub_with_bus();
        bus.publish(BusEvent::flow_execution("f1", "", "running", ""));
        bus.publish(BusEvent::flow_execution("f1", "op1", "running", ""));
        bus.publish(BusEvent::SyncProgress(SyncProgressEvent {
            task_id: "t1".into(),
            profile_id: "f1:op1".into(),
            action: "push".into(),
            state: "running".into(),
            transferred: 10,
            total: 100,
            transfers: vec![FileTransfer {
                name: "a.txt".into(),
                status: "transferring".into(),
                progress: 40.0,
                ..Default::default()
            }],
            ..Default::default()
        }));
        wait_rev(&h, 3);
        let snap = h.snapshot();
        assert_eq!(snap.revision, 3);
        assert_eq!(snap.flows.len(), 1);
        let f = &snap.flows[0];
        assert_eq!((f.id.as_str(), f.status.as_str()), ("f1", "running"));
        let sync = f.sync.as_ref().unwrap();
        assert_eq!(sync.transfers.len(), 1);
        assert_eq!(sync.transfers[0].name, "a.txt");
        assert_eq!((sync.transferred, sync.total), (10, 100));
        assert!(f.log.len() >= 2);
        assert!(f.ops.iter().any(|o| o.id == "op1" && o.status == "running"));
        h.close();
    }

    #[test]
    fn forget_and_failed() {
        let (bus, h) = hub_with_bus();
        bus.publish(BusEvent::flow_execution("gone", "", "running", ""));
        wait_rev(&h, 1);
        h.forget("gone");
        assert!(h.snapshot().flows.is_empty());
        let after = h.snapshot().revision;
        assert_eq!(after, 2);

        bus.publish(BusEvent::flow_execution("f1", "op1", "failed", "boom"));
        wait_rev(&h, after + 1);
        let snap = h.snapshot();
        assert_eq!(snap.flows.len(), 1);
        assert_eq!(snap.flows[0].status, "failed");
        assert_eq!(snap.flows[0].last_error, "boom");
        assert_eq!(snap.flows[0].ops[0].last_error, "boom");

        h.forget(""); // no-op, no revision bump
        assert_eq!(h.snapshot().revision, after + 1);
        h.close();
    }

    #[test]
    fn reset_bumps_revision() {
        let bus = EventBus::new();
        let h = RuntimeHub::new(&bus, None, None);
        h.on_event(&BusEvent::flow_execution("x", "", "running", ""));
        assert_eq!(h.snapshot().flows.len(), 1);
        h.state.lock().rev = 4;
        h.reset();
        let s = h.snapshot();
        assert!(s.flows.is_empty());
        assert_eq!(s.revision, 5);
        h.close();
    }

    #[test]
    fn split_busy_key_cases() {
        assert_eq!(
            RuntimeHub::split_busy_key("abc:def"),
            Some(("abc".into(), "def".into()))
        );
        assert_eq!(
            RuntimeHub::split_busy_key("a:b:c"),
            Some(("a".into(), "b:c".into()))
        );
        assert_eq!(RuntimeHub::split_busy_key("nocolon"), None);
        assert_eq!(RuntimeHub::split_busy_key(":x"), None);
        assert_eq!(RuntimeHub::split_busy_key("x:"), None);
        assert_eq!(RuntimeHub::split_busy_key(""), None);
    }

    #[test]
    fn log_dedupe_and_cap() {
        let bus = EventBus::new();
        let h = RuntimeHub::new(&bus, None, None);
        for _ in 0..3 {
            h.on_event(&BusEvent::flow_execution("f", "", "running", ""));
        }
        let log = h.snapshot().flows[0].log.clone();
        assert_eq!(log.len(), 1, "consecutive duplicates collapse");
        assert_eq!(log[0].label, "Flow");

        // Alternating statuses are not duplicates; cap at 40 keeps newest.
        for i in 0..60 {
            h.on_event(&BusEvent::flow_execution(
                "f",
                "operation-long-id",
                "running",
                &format!("e{i}"),
            ));
        }
        let log = h.snapshot().flows[0].log.clone();
        assert_eq!(log.len(), MAX_LOG);
        assert_eq!(log.last().unwrap().error, "e59");
        assert_eq!(log.first().unwrap().error, "e20");
        assert_eq!(log.last().unwrap().label, "Op operatio");
        h.close();
    }

    #[test]
    fn flow_level_status_and_error_rules() {
        let bus = EventBus::new();
        let h = RuntimeHub::new(&bus, None, None);
        h.on_event(&BusEvent::flow_execution("f", "", "failed", "bad"));
        let s = &h.snapshot().flows[0];
        assert_eq!(
            (s.status.as_str(), s.last_error.as_str()),
            ("failed", "bad")
        );
        // cancelling/cancelled clear the flow-level error.
        h.on_event(&BusEvent::flow_execution("f", "", "cancelling", ""));
        let s = &h.snapshot().flows[0];
        assert_eq!(
            (s.status.as_str(), s.last_error.as_str()),
            ("cancelling", "")
        );
        // op-level cancelled error text does not become the flow error.
        h.on_event(&BusEvent::flow_execution("f", "o1", "cancelled", "stopped"));
        let s = &h.snapshot().flows[0];
        assert_eq!(s.status, "cancelled");
        assert_eq!(s.last_error, "");
        assert_eq!(s.ops[0].last_error, "stopped");
        // completed op does not alter flow status.
        h.on_event(&BusEvent::flow_execution("f", "o2", "completed", ""));
        assert_eq!(h.snapshot().flows[0].status, "cancelled");
        // events without a flow id and unrelated topics are ignored.
        let rev = h.snapshot().revision;
        h.on_event(&BusEvent::flow_execution("", "o", "running", ""));
        h.on_event(&BusEvent::state_changed("flows", "f"));
        assert_eq!(h.snapshot().revision, rev);
        h.close();
    }

    #[test]
    fn sync_events_started_completed_failed() {
        let bus = EventBus::new();
        let h = RuntimeHub::new(&bus, None, None);
        h.on_event(&BusEvent::sync_started("t1", "f:o", "push"));
        let f = &h.snapshot().flows[0];
        assert_eq!(f.status, "running");
        assert_eq!(f.ops[0].status, "running");
        assert_eq!(f.sync.as_ref().unwrap().task_id, "t1");

        h.on_event(&BusEvent::sync_completed("t1", "f:o", "push", 1, 99, 2));
        let f = &h.snapshot().flows[0];
        assert_eq!(
            f.status, "running",
            "completed does not override a live status"
        );
        assert_eq!(f.ops[0].status, "completed");
        let sp = f.sync.as_ref().unwrap();
        assert_eq!(
            (sp.state.as_str(), sp.transferred, sp.errors),
            ("completed", 99, 2)
        );

        h.on_event(&BusEvent::SyncFailed(SyncProgressEvent {
            task_id: "t2".into(),
            profile_id: "g:o".into(),
            state: "failed".into(),
            error_message: "kaput".into(),
            ..Default::default()
        }));
        let snap = h.snapshot();
        let g = snap.flows.iter().find(|f| f.id == "g").unwrap();
        assert_eq!(g.status, "failed");
        assert_eq!(g.last_error, "kaput");
        assert_eq!(g.ops[0].last_error, "kaput");

        // Non flow-style busy keys (profile names) are ignored.
        let rev = h.snapshot().revision;
        h.on_event(&BusEvent::sync_started("t3", "plainprofile", "push"));
        assert_eq!(h.snapshot().revision, rev + 1, "revision still bumps");
        assert_eq!(h.snapshot().flows.len(), 2);
        h.close();
    }

    #[test]
    fn overlay_active_tasks_and_engine_statuses() {
        // Active-task overlay (Go: SeedsActiveTasks).
        let mut merged: HashMap<String, FlowRuntime> = HashMap::new();
        let t = TaskSnapshot {
            id: "task-1".into(),
            name: "f2:op2".into(),
            action: "push".into(),
            status: String::new(),
            stats: SyncStats {
                bytes: 3,
                bytes_total: 9,
                file_transfers: vec![FileTransfer {
                    name: "b.bin".into(),
                    status: "transferring".into(),
                    progress: 30.0,
                    ..Default::default()
                }],
                ..Default::default()
            },
            started_at: Utc::now(),
            ended_at: None,
        };
        apply_task(&mut merged, &t);
        let st = to_flow_state("f2".into(), merged.remove("f2").unwrap());
        assert_eq!(st.status, "running", "empty task status → running");
        assert_eq!(st.ops[0].id, "op2");
        let s = st.sync.unwrap();
        assert_eq!(
            (s.transferred, s.total, s.task_id.as_str()),
            (3, 9, "task-1")
        );
        assert_eq!(s.transfers[0].name, "b.bin");

        // Non-flow task names are skipped.
        let mut m2 = HashMap::new();
        apply_task(
            &mut m2,
            &TaskSnapshot {
                name: "profile".into(),
                ..t.clone()
            },
        );
        assert!(m2.is_empty());

        // Engine statuses overlay a flow the hub never saw events for.
        let bus = EventBus::new();
        let flows = FlowEngine::new(Logger::quiet(), bus.clone());
        let h = RuntimeHub::new(&bus, Some(flows.clone()), None);
        assert!(h.snapshot().flows.is_empty());
        flows.set_last_status_for_test("f9", "completed");
        let snap = h.snapshot();
        let f9 = snap.flows.iter().find(|f| f.id == "f9").unwrap();
        assert_eq!(f9.status, "completed");
        h.close();
    }

    #[test]
    fn close_stops_projection() {
        let (bus, h) = hub_with_bus();
        bus.publish(BusEvent::flow_execution("f", "", "running", ""));
        wait_rev(&h, 1);
        h.close();
        h.close(); // idempotent
        bus.publish(BusEvent::flow_execution("f2", "", "running", ""));
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(h.snapshot().revision, 1);
    }

    #[test]
    fn bus_close_ends_worker() {
        let (bus, h) = hub_with_bus();
        bus.close();
        h.close(); // joins promptly even though the bus is gone
    }
}
