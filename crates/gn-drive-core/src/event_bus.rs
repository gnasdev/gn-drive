//! In-process typed event bus (port of `EventBus.swift`).
//!
//! Each subscription has a bounded mailbox (64). When full, the oldest queued
//! event is dropped to make room for the newest. `publish` never blocks.

use crate::rclone::types::FileTransfer;
use chrono::{DateTime, Utc};
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub mod topic {
    pub const SYNC_STARTED: &str = "sync:started";
    pub const SYNC_PROGRESS: &str = "sync:progress";
    pub const SYNC_COMPLETED: &str = "sync:completed";
    pub const SYNC_FAILED: &str = "sync:failed";
    pub const AUTH_UNLOCKED: &str = "auth:unlocked";
    pub const AUTH_LOCKED: &str = "auth:locked";
    pub const SERVICE_STATUS: &str = "service:status";
    pub const STATE_CHANGED: &str = "state:changed";
    pub const SCHEDULE_TRIGGERED: &str = "schedule:triggered";
    pub const BOARD_EXECUTION: &str = "board:execution";
    pub const FLOW_EXECUTION: &str = "flow:execution";
    pub const RUNTIME_SNAPSHOT: &str = "runtime:snapshot";

    /// Topics the RPC bridge forwards (runtime:snapshot is derived on demand).
    pub const ALL: [&str; 11] = [
        SYNC_STARTED,
        SYNC_PROGRESS,
        SYNC_COMPLETED,
        SYNC_FAILED,
        AUTH_UNLOCKED,
        AUTH_LOCKED,
        SERVICE_STATUS,
        STATE_CHANGED,
        SCHEDULE_TRIGGERED,
        BOARD_EXECUTION,
        FLOW_EXECUTION,
    ];
}

/// Per-file row inside a progress event (same shape as `FileTransfer`).
pub type FileTransferEvent = FileTransfer;

#[derive(Debug, Clone, PartialEq)]
pub struct SyncProgressEvent {
    pub timestamp: DateTime<Utc>,
    pub task_id: String,
    pub profile_id: String,
    pub action: String,
    pub state: String,
    pub transferred: i64,
    pub total: i64,
    pub bytes_per_sec: f64,
    pub eta: i64,
    pub errors: i64,
    pub current_file: String,
    pub stage: String,
    pub stage_detail: String,
    pub files_transferred: i64,
    pub total_files: i64,
    pub checks: i64,
    pub total_checks: i64,
    pub deletes: i64,
    pub renames: i64,
    pub transfers: Vec<FileTransferEvent>,
    pub error_message: String,
}

impl Default for SyncProgressEvent {
    fn default() -> Self {
        Self {
            timestamp: Utc::now(),
            task_id: String::new(),
            profile_id: String::new(),
            action: String::new(),
            state: "running".into(),
            transferred: 0,
            total: 0,
            bytes_per_sec: 0.0,
            eta: 0,
            errors: 0,
            current_file: String::new(),
            stage: String::new(),
            stage_detail: String::new(),
            files_transferred: 0,
            total_files: 0,
            checks: 0,
            total_checks: 0,
            deletes: 0,
            renames: 0,
            transfers: Vec::new(),
            error_message: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyncStartedEvent {
    pub timestamp: DateTime<Utc>,
    pub task_id: String,
    pub profile_id: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyncCompletedEvent {
    pub timestamp: DateTime<Utc>,
    pub task_id: String,
    pub profile_id: String,
    pub action: String,
    pub duration: i64,
    pub bytes: i64,
    pub errors: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceStatusEvent {
    pub timestamp: DateTime<Utc>,
    pub running: bool,
    pub web_port: i64,
    pub uptime_secs: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleTriggeredEvent {
    pub timestamp: DateTime<Utc>,
    pub schedule_id: String,
    pub profile_id: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoardExecutionEvent {
    pub timestamp: DateTime<Utc>,
    pub board_id: String,
    pub node_id: String,
    pub edge_id: String,
    pub status: String,
    pub profile_id: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowExecutionEvent {
    pub timestamp: DateTime<Utc>,
    pub flow_id: String,
    pub op_id: String,
    pub status: String,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StateChangedEvent {
    pub timestamp: DateTime<Utc>,
    pub domain: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeLogEntry {
    pub at: i64,
    pub status: String,
    #[serde(rename = "opID")]
    pub op_id: String,
    pub error: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOpState {
    pub id: String,
    pub status: String,
    pub last_error: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RuntimeFlowState {
    pub id: String,
    pub status: String,
    pub last_error: String,
    pub ops: Vec<RuntimeOpState>,
    pub sync: Option<SyncProgressEvent>,
    pub log: Vec<RuntimeLogEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeSnapshotEvent {
    pub timestamp: DateTime<Utc>,
    pub revision: i64,
    pub flows: Vec<RuntimeFlowState>,
}

/// Every event that can travel over the bus.
#[derive(Debug, Clone, PartialEq)]
pub enum BusEvent {
    SyncStarted(SyncStartedEvent),
    SyncProgress(SyncProgressEvent),
    SyncCompleted(SyncCompletedEvent),
    /// `sync:failed` carries a progress-shaped payload (state=failed|cancelled).
    SyncFailed(SyncProgressEvent),
    AuthUnlocked(DateTime<Utc>),
    AuthLocked(DateTime<Utc>),
    ServiceStatus(ServiceStatusEvent),
    StateChanged(StateChangedEvent),
    ScheduleTriggered(ScheduleTriggeredEvent),
    BoardExecution(BoardExecutionEvent),
    FlowExecution(FlowExecutionEvent),
    RuntimeSnapshot(RuntimeSnapshotEvent),
}

impl BusEvent {
    pub fn topic(&self) -> &'static str {
        match self {
            BusEvent::SyncStarted(_) => topic::SYNC_STARTED,
            BusEvent::SyncProgress(_) => topic::SYNC_PROGRESS,
            BusEvent::SyncCompleted(_) => topic::SYNC_COMPLETED,
            BusEvent::SyncFailed(_) => topic::SYNC_FAILED,
            BusEvent::AuthUnlocked(_) => topic::AUTH_UNLOCKED,
            BusEvent::AuthLocked(_) => topic::AUTH_LOCKED,
            BusEvent::ServiceStatus(_) => topic::SERVICE_STATUS,
            BusEvent::StateChanged(_) => topic::STATE_CHANGED,
            BusEvent::ScheduleTriggered(_) => topic::SCHEDULE_TRIGGERED,
            BusEvent::BoardExecution(_) => topic::BOARD_EXECUTION,
            BusEvent::FlowExecution(_) => topic::FLOW_EXECUTION,
            BusEvent::RuntimeSnapshot(_) => topic::RUNTIME_SNAPSHOT,
        }
    }

    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            BusEvent::SyncStarted(e) => e.timestamp,
            BusEvent::SyncProgress(e) | BusEvent::SyncFailed(e) => e.timestamp,
            BusEvent::SyncCompleted(e) => e.timestamp,
            BusEvent::AuthUnlocked(t) | BusEvent::AuthLocked(t) => *t,
            BusEvent::ServiceStatus(e) => e.timestamp,
            BusEvent::StateChanged(e) => e.timestamp,
            BusEvent::ScheduleTriggered(e) => e.timestamp,
            BusEvent::BoardExecution(e) => e.timestamp,
            BusEvent::FlowExecution(e) => e.timestamp,
            BusEvent::RuntimeSnapshot(e) => e.timestamp,
        }
    }

    // Convenience constructors (timestamp = now).
    pub fn sync_started(task_id: &str, profile_id: &str, action: &str) -> Self {
        BusEvent::SyncStarted(SyncStartedEvent {
            timestamp: Utc::now(),
            task_id: task_id.into(),
            profile_id: profile_id.into(),
            action: action.into(),
        })
    }
    pub fn sync_completed(
        task_id: &str,
        profile_id: &str,
        action: &str,
        duration: i64,
        bytes: i64,
        errors: i64,
    ) -> Self {
        BusEvent::SyncCompleted(SyncCompletedEvent {
            timestamp: Utc::now(),
            task_id: task_id.into(),
            profile_id: profile_id.into(),
            action: action.into(),
            duration,
            bytes,
            errors,
        })
    }
    pub fn flow_execution(flow_id: &str, op_id: &str, status: &str, error: &str) -> Self {
        BusEvent::FlowExecution(FlowExecutionEvent {
            timestamp: Utc::now(),
            flow_id: flow_id.into(),
            op_id: op_id.into(),
            status: status.into(),
            error: error.into(),
        })
    }
    pub fn board_execution(
        board_id: &str,
        node_id: &str,
        edge_id: &str,
        status: &str,
        action: &str,
    ) -> Self {
        BusEvent::BoardExecution(BoardExecutionEvent {
            timestamp: Utc::now(),
            board_id: board_id.into(),
            node_id: node_id.into(),
            edge_id: edge_id.into(),
            status: status.into(),
            profile_id: String::new(),
            action: action.into(),
        })
    }
    pub fn schedule_triggered(schedule_id: &str, profile_id: &str, action: &str) -> Self {
        BusEvent::ScheduleTriggered(ScheduleTriggeredEvent {
            timestamp: Utc::now(),
            schedule_id: schedule_id.into(),
            profile_id: profile_id.into(),
            action: action.into(),
        })
    }
    pub fn state_changed(domain: &str, id: &str) -> Self {
        BusEvent::StateChanged(StateChangedEvent {
            timestamp: Utc::now(),
            domain: domain.into(),
            id: id.into(),
        })
    }
    pub fn auth_unlocked() -> Self {
        BusEvent::AuthUnlocked(Utc::now())
    }
    pub fn auth_locked() -> Self {
        BusEvent::AuthLocked(Utc::now())
    }
}

const MAILBOX_CAPACITY: usize = 64;

struct Subscription {
    id: u64,
    tx: Sender<BusEvent>,
    /// Publisher-side receiver clone used to evict the oldest event when full.
    evict: Receiver<BusEvent>,
}

/// Receiving end of a subscription. Dropping it (or calling `cancel`)
/// unsubscribes.
pub struct Subscriber {
    rx: Receiver<BusEvent>,
    id: u64,
    topics: Vec<String>,
    bus: Arc<Inner>,
}

impl Subscriber {
    pub fn receiver(&self) -> &Receiver<BusEvent> {
        &self.rx
    }
    pub fn recv(&self) -> Option<BusEvent> {
        self.rx.recv().ok()
    }
    pub fn recv_timeout(&self, d: std::time::Duration) -> Option<BusEvent> {
        self.rx.recv_timeout(d).ok()
    }
    pub fn try_recv(&self) -> Option<BusEvent> {
        self.rx.try_recv().ok()
    }
    pub fn cancel(&self) {
        self.bus.remove(self.id, &self.topics);
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Default)]
struct Inner {
    state: Mutex<State>,
    next_id: AtomicU64,
}

#[derive(Default)]
struct State {
    topics: HashMap<String, Vec<Arc<Subscription>>>,
    closed: bool,
}

impl Inner {
    fn remove(&self, id: u64, topics: &[String]) {
        let mut st = self.state.lock();
        for t in topics {
            if let Some(list) = st.topics.get_mut(t) {
                list.retain(|s| s.id != id);
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct EventBus {
    inner: Arc<Inner>,
}

impl EventBus {
    pub fn new() -> EventBus {
        EventBus::default()
    }

    /// Subscribe to the given topics. After `close()` the returned receiver is
    /// disconnected immediately.
    pub fn subscribe(&self, topics: &[&str]) -> Subscriber {
        let (tx, rx) = bounded(MAILBOX_CAPACITY);
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let topics: Vec<String> = topics.iter().map(|t| t.to_string()).collect();
        let mut st = self.inner.state.lock();
        if st.closed {
            drop(tx);
        } else {
            let sub = Arc::new(Subscription {
                id,
                tx,
                evict: rx.clone(),
            });
            for t in &topics {
                st.topics.entry(t.clone()).or_default().push(sub.clone());
            }
        }
        drop(st);
        Subscriber {
            rx,
            id,
            topics,
            bus: self.inner.clone(),
        }
    }

    /// Subscribe to every forwarded topic (`topic::ALL`).
    pub fn subscribe_all(&self) -> Subscriber {
        self.subscribe(&topic::ALL)
    }

    /// Broadcast to every subscriber of the event's topic. Non-blocking;
    /// drops the oldest queued event on a full mailbox.
    pub fn publish(&self, event: BusEvent) {
        let subs: Vec<Arc<Subscription>> = {
            let st = self.inner.state.lock();
            if st.closed {
                return;
            }
            st.topics.get(event.topic()).cloned().unwrap_or_default()
        };
        for s in subs {
            let mut ev = event.clone();
            loop {
                match s.tx.try_send(ev) {
                    Ok(()) => break,
                    Err(TrySendError::Full(back)) => {
                        let _ = s.evict.try_recv();
                        ev = back;
                    }
                    Err(TrySendError::Disconnected(_)) => break,
                }
            }
        }
    }

    pub fn close(&self) {
        let mut st = self.inner.state.lock();
        st.closed = true;
        st.topics.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn delivers_only_subscribed_topics() {
        let bus = EventBus::new();
        let sub = bus.subscribe(&[topic::FLOW_EXECUTION]);
        bus.publish(BusEvent::state_changed("flows", "x"));
        bus.publish(BusEvent::flow_execution("f", "", "running", ""));
        let ev = sub.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(ev.topic(), topic::FLOW_EXECUTION);
        assert!(sub.try_recv().is_none());
    }

    #[test]
    fn full_mailbox_drops_oldest() {
        let bus = EventBus::new();
        let sub = bus.subscribe(&[topic::STATE_CHANGED]);
        for i in 0..(MAILBOX_CAPACITY + 10) {
            bus.publish(BusEvent::state_changed("d", &i.to_string()));
        }
        let mut ids = Vec::new();
        while let Some(BusEvent::StateChanged(e)) = sub.try_recv() {
            ids.push(e.id);
        }
        assert_eq!(ids.len(), MAILBOX_CAPACITY);
        assert_eq!(ids.first().unwrap(), "10");
        assert_eq!(ids.last().unwrap(), &(MAILBOX_CAPACITY + 9).to_string());
    }

    #[test]
    fn cancel_and_close_stop_delivery() {
        let bus = EventBus::new();
        let sub = bus.subscribe(&[topic::AUTH_LOCKED]);
        sub.cancel();
        bus.publish(BusEvent::auth_locked());
        assert!(sub.try_recv().is_none());

        let sub2 = bus.subscribe(&[topic::AUTH_LOCKED]);
        bus.close();
        bus.publish(BusEvent::auth_locked());
        assert!(sub2.try_recv().is_none());
        let sub3 = bus.subscribe(&[topic::AUTH_LOCKED]);
        assert!(
            sub3.recv().is_none(),
            "subscribe after close is disconnected"
        );
    }

    #[test]
    fn dropping_subscriber_unsubscribes() {
        let bus = EventBus::new();
        {
            let _s = bus.subscribe(&[topic::AUTH_LOCKED]);
        }
        bus.publish(BusEvent::auth_locked()); // must not panic / block
    }
}
