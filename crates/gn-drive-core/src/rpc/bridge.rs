//! Event side of the RPC layer: bus subscription → pushed [`Event`]s.
//!
//! Port of the `onBusEvent` / `enqueueProgress` / `requestRuntimeSnapshot` /
//! heartbeat / `log:line` logic of `RPCService.swift`, delivering to
//! subscribers over bounded crossbeam channels instead of writing to stdout.

use super::{Event, Options, json};
use crate::container::AppContainer;
use crate::event_bus::{BusEvent, EventBus, RuntimeSnapshotEvent, topic};
use crate::logger::{LogLevel, LogRecord, Logger};
use crate::rpc::LogBuffer;
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use parking_lot::{Condvar, Mutex};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Per-subscriber queue capacity.
const SUBSCRIBER_CAPACITY: usize = 4096;
/// Droppable events (`log:line`) are discarded once this many frames are queued.
const DROPPABLE_LIMIT: usize = 512;
/// Name of the only droppable event.
const LOG_LINE: &str = "log:line";

/// Intervals of the bridge's timers. Injectable so tests run fast.
#[derive(Debug, Clone, Copy)]
pub struct BridgeTimings {
    /// `sync:progress` coalescing window.
    pub progress_flush: Duration,
    /// Minimum spacing between `runtime:snapshot` pushes.
    pub runtime_interval: Duration,
    /// Heartbeat period (a snapshot is pushed while a flow is active).
    pub heartbeat: Duration,
}

impl Default for BridgeTimings {
    fn default() -> Self {
        BridgeTimings {
            progress_flush: Duration::from_millis(100),
            runtime_interval: Duration::from_millis(100),
            heartbeat: Duration::from_secs(2),
        }
    }
}

type SnapshotFn = Arc<dyn Fn() -> RuntimeSnapshotEvent + Send + Sync>;

struct Subscriber {
    tx: Sender<Event>,
    /// Used to evict the oldest frame when the queue is full.
    evict: Receiver<Event>,
}

#[derive(Default)]
struct Timers {
    pending_progress: HashMap<String, Value>,
    progress_order: Vec<String>,
    progress_due: Option<Instant>,

    runtime_pending: bool,
    runtime_scheduled: bool,
    runtime_due: Option<Instant>,
    last_runtime_push: Option<Instant>,

    next_heartbeat: Option<Instant>,
}

struct Inner {
    subscribers: Mutex<Vec<Subscriber>>,
    timers: Mutex<Timers>,
    wake: Condvar,
    stopping: AtomicBool,
    timings: BridgeTimings,
    log_level: LogLevel,
    snapshot: SnapshotFn,
    logger: Option<Logger>,
}

/// Converts bus events into protocol events and fans them out.
pub struct EventBridge {
    inner: Arc<Inner>,
}

impl EventBridge {
    /// Bridge over a container's bus; also hooks the container logger so
    /// records feed `log_buffer` and `log:line`.
    pub fn new(
        container: Arc<AppContainer>,
        log_buffer: Arc<LogBuffer>,
        opts: &Options,
    ) -> Arc<EventBridge> {
        Self::with_timings(container, log_buffer, opts, BridgeTimings::default())
    }

    pub fn with_timings(
        container: Arc<AppContainer>,
        log_buffer: Arc<LogBuffer>,
        opts: &Options,
        timings: BridgeTimings,
    ) -> Arc<EventBridge> {
        let rt = container.runtime.clone();
        let snapshot: SnapshotFn = Arc::new(move || rt.snapshot());
        let logger = container.log.clone();
        logger.set_min_level(opts.log_level);
        let bridge = Self::from_parts(
            &container.bus,
            snapshot,
            opts.log_level,
            timings,
            Some(logger.clone()),
        );
        let weak = Arc::downgrade(&bridge.inner);
        logger.set_sink(Some(Arc::new(move |rec: &LogRecord| {
            log_buffer.push(rec.clone());
            if let Some(inner) = weak.upgrade() {
                inner.on_log_record(rec);
            }
        })));
        bridge
    }

    /// Container-free constructor (also used by tests).
    pub fn from_parts(
        bus: &EventBus,
        snapshot: SnapshotFn,
        log_level: LogLevel,
        timings: BridgeTimings,
        logger: Option<Logger>,
    ) -> Arc<EventBridge> {
        let inner = Arc::new(Inner {
            subscribers: Mutex::new(Vec::new()),
            timers: Mutex::new(Timers {
                next_heartbeat: Some(Instant::now() + timings.heartbeat),
                ..Default::default()
            }),
            wake: Condvar::new(),
            stopping: AtomicBool::new(false),
            timings,
            log_level,
            snapshot,
            logger,
        });
        let sub = bus.subscribe(&topic::ALL);
        {
            let inner = inner.clone();
            std::thread::Builder::new()
                .name("rpc-bridge-bus".into())
                .spawn(move || {
                    while !inner.stopping.load(Ordering::SeqCst) {
                        match sub.receiver().recv_timeout(Duration::from_millis(50)) {
                            Ok(ev) => inner.on_bus_event(ev),
                            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                        }
                    }
                })
                .expect("spawn bridge bus thread");
        }
        {
            let inner = inner.clone();
            std::thread::Builder::new()
                .name("rpc-bridge-timers".into())
                .spawn(move || inner.timer_loop())
                .expect("spawn bridge timer thread");
        }
        Arc::new(EventBridge { inner })
    }

    /// New receiver of every pushed event from now on.
    pub fn subscribe(&self) -> Receiver<Event> {
        let (tx, rx) = bounded(SUBSCRIBER_CAPACITY);
        if !self.inner.stopping.load(Ordering::SeqCst) {
            self.inner.subscribers.lock().push(Subscriber {
                tx,
                evict: rx.clone(),
            });
        }
        rx
    }

    /// Push a non-droppable event (e.g. OAuth progress).
    pub fn push_event(&self, name: &str, data: Value) {
        self.inner.enqueue_event(name, data, false);
    }

    /// Forward a log record as `log:line` (droppable).
    pub fn on_log_record(&self, record: &LogRecord) {
        self.inner.on_log_record(record);
    }

    /// Schedule a `runtime:snapshot` push (throttled unless `immediate`).
    pub fn request_runtime_snapshot(&self, immediate: bool) {
        self.inner.request_runtime_snapshot(immediate);
    }

    /// Stop timers/threads, unhook the logger and disconnect subscribers.
    /// Idempotent.
    pub fn shutdown(&self) {
        if self.inner.stopping.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(l) = &self.inner.logger {
            l.set_sink(None);
        }
        self.inner.wake.notify_all();
        self.inner.subscribers.lock().clear();
    }
}

impl Drop for EventBridge {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Inner {
    fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    fn on_bus_event(&self, ev: BusEvent) {
        let name = ev.topic();
        match ev {
            BusEvent::SyncProgress(p) => {
                let key = if p.profile_id.is_empty() {
                    p.task_id.clone()
                } else {
                    p.profile_id.clone()
                };
                self.enqueue_progress(key, json::sync_progress(&p));
                self.request_runtime_snapshot(false);
            }
            BusEvent::SyncStarted(e) => {
                self.enqueue_event(
                    name,
                    json!({"taskID": e.task_id, "profileID": e.profile_id, "action": e.action}),
                    false,
                );
                self.request_runtime_snapshot(false);
            }
            BusEvent::SyncCompleted(e) => {
                self.enqueue_event(
                    name,
                    json!({
                        "taskID": e.task_id, "profileID": e.profile_id, "action": e.action,
                        "duration": e.duration, "bytes": e.bytes, "errors": e.errors,
                    }),
                    false,
                );
                self.request_runtime_snapshot(false);
            }
            BusEvent::SyncFailed(p) => {
                self.enqueue_event(name, json::sync_progress(&p), false);
                self.request_runtime_snapshot(false);
            }
            BusEvent::AuthUnlocked(_) | BusEvent::AuthLocked(_) => {
                self.enqueue_event(name, json!({}), false);
                self.request_runtime_snapshot(true);
            }
            BusEvent::FlowExecution(e) => {
                self.enqueue_event(
                    name,
                    json!({"flowID": e.flow_id, "opID": e.op_id, "status": e.status, "error": e.error}),
                    false,
                );
                self.request_runtime_snapshot(false);
            }
            BusEvent::BoardExecution(e) => self.enqueue_event(
                name,
                json!({
                    "boardID": e.board_id, "nodeID": e.node_id, "edgeID": e.edge_id,
                    "status": e.status, "action": e.action,
                }),
                false,
            ),
            BusEvent::ScheduleTriggered(e) => self.enqueue_event(
                name,
                json!({"scheduleID": e.schedule_id, "profileID": e.profile_id, "action": e.action}),
                false,
            ),
            BusEvent::StateChanged(e) => {
                self.enqueue_event(name, json!({"domain": e.domain, "id": e.id}), false)
            }
            BusEvent::ServiceStatus(e) => self.enqueue_event(
                name,
                json!({"running": e.running, "webPort": e.web_port, "uptimeSecs": e.uptime_secs}),
                false,
            ),
            BusEvent::RuntimeSnapshot(_) => {}
        }
    }

    fn on_log_record(&self, record: &LogRecord) {
        if self.is_stopping() || record.level < self.log_level {
            return;
        }
        let fields = serde_json::to_value(&record.fields).unwrap_or_else(|_| json!({}));
        self.enqueue_event(
            LOG_LINE,
            json!({
                "time": json::rfc3339(record.time),
                "level": record.level.label(),
                "msg": record.msg,
                "fields": fields,
            }),
            true,
        );
    }

    fn enqueue_event(&self, name: &str, data: Value, droppable: bool) {
        if self.is_stopping() {
            return;
        }
        self.deliver(Event::new(name, data), droppable);
    }

    fn deliver(&self, event: Event, droppable: bool) {
        let mut subs = self.subscribers.lock();
        subs.retain(|s| {
            if droppable && s.tx.len() >= DROPPABLE_LIMIT {
                return true;
            }
            let mut ev = event.clone();
            // Full queue: evict the oldest frame so the newest non-droppable
            // event always lands.
            for _ in 0..8 {
                match s.tx.try_send(ev) {
                    Ok(()) => return true,
                    Err(TrySendError::Disconnected(_)) => return false,
                    Err(TrySendError::Full(back)) => {
                        if droppable {
                            return true;
                        }
                        let _ = s.evict.try_recv();
                        ev = back;
                    }
                }
            }
            true
        });
    }

    /// `sync:progress` is coalesced per profile.
    fn enqueue_progress(&self, key: String, data: Value) {
        let mut t = self.timers.lock();
        if !t.pending_progress.contains_key(&key) {
            t.progress_order.push(key.clone());
        }
        t.pending_progress.insert(key, data);
        if t.progress_due.is_none() {
            t.progress_due = Some(Instant::now() + self.timings.progress_flush);
            self.wake.notify_all();
        }
    }

    fn flush_progress(&self) {
        let (keys, mut batch) = {
            let mut t = self.timers.lock();
            t.progress_due = None;
            (
                std::mem::take(&mut t.progress_order),
                std::mem::take(&mut t.pending_progress),
            )
        };
        for key in keys {
            if let Some(data) = batch.remove(&key) {
                self.enqueue_event(topic::SYNC_PROGRESS, data, false);
            }
        }
    }

    fn request_runtime_snapshot(&self, immediate: bool) {
        let mut t = self.timers.lock();
        t.runtime_pending = true;
        if t.runtime_scheduled {
            return;
        }
        t.runtime_scheduled = true;
        let delay = if immediate {
            Duration::ZERO
        } else {
            match t.last_runtime_push {
                Some(last) => self.timings.runtime_interval.saturating_sub(last.elapsed()),
                None => Duration::ZERO,
            }
        };
        t.runtime_due = Some(Instant::now() + delay);
        self.wake.notify_all();
    }

    fn flush_runtime_snapshot(&self) {
        {
            let mut t = self.timers.lock();
            t.runtime_due = None;
            if !t.runtime_pending {
                t.runtime_scheduled = false;
                return;
            }
            t.runtime_pending = false;
            t.runtime_scheduled = false;
            t.last_runtime_push = Some(Instant::now());
        }
        let snap = (self.snapshot)();
        self.enqueue_event("runtime:snapshot", json::runtime_snapshot(&snap), false);
    }

    fn heartbeat_tick(&self) {
        let snap = (self.snapshot)();
        let active = snap
            .flows
            .iter()
            .any(|f| f.status == "running" || f.status == "cancelling");
        if active {
            self.enqueue_event("runtime:snapshot", json::runtime_snapshot(&snap), false);
        }
    }

    fn timer_loop(&self) {
        loop {
            let (run_progress, run_runtime, run_heartbeat) = {
                let mut t = self.timers.lock();
                loop {
                    if self.is_stopping() {
                        return;
                    }
                    let now = Instant::now();
                    let run_progress = t.progress_due.is_some_and(|d| d <= now);
                    let run_runtime = t.runtime_due.is_some_and(|d| d <= now);
                    let run_heartbeat = t.next_heartbeat.is_some_and(|d| d <= now);
                    if run_progress || run_runtime || run_heartbeat {
                        if run_heartbeat {
                            t.next_heartbeat = Some(now + self.timings.heartbeat);
                        }
                        break (run_progress, run_runtime, run_heartbeat);
                    }
                    let next = [t.progress_due, t.runtime_due, t.next_heartbeat]
                        .into_iter()
                        .flatten()
                        .min();
                    match next {
                        Some(d) => {
                            self.wake.wait_until(&mut t, d);
                        }
                        None => self.wake.wait(&mut t),
                    }
                }
            };
            if run_progress {
                self.flush_progress();
            }
            if run_runtime {
                self.flush_runtime_snapshot();
            }
            if run_heartbeat {
                self.heartbeat_tick();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::{RuntimeFlowState, SyncProgressEvent};
    use std::sync::atomic::AtomicUsize;

    fn fast() -> BridgeTimings {
        BridgeTimings {
            progress_flush: Duration::from_millis(30),
            runtime_interval: Duration::from_millis(60),
            heartbeat: Duration::from_secs(3600),
        }
    }

    struct Rig {
        bus: EventBus,
        bridge: Arc<EventBridge>,
        snaps: Arc<AtomicUsize>,
        active: Arc<AtomicBool>,
        rx: Receiver<Event>,
    }

    fn rig(timings: BridgeTimings) -> Rig {
        let bus = EventBus::new();
        let snaps = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicBool::new(false));
        let (s, a) = (snaps.clone(), active.clone());
        let snapshot: SnapshotFn = Arc::new(move || {
            let n = s.fetch_add(1, Ordering::SeqCst) as i64 + 1;
            let flows = if a.load(Ordering::SeqCst) {
                vec![RuntimeFlowState {
                    id: "f".into(),
                    status: "running".into(),
                    ..Default::default()
                }]
            } else {
                vec![]
            };
            RuntimeSnapshotEvent {
                timestamp: chrono::Utc::now(),
                revision: n,
                flows,
            }
        });
        let bridge = EventBridge::from_parts(&bus, snapshot, LogLevel::Info, timings, None);
        let rx = bridge.subscribe();
        Rig {
            bus,
            bridge,
            snaps,
            active,
            rx,
        }
    }

    fn recv(rx: &Receiver<Event>) -> Event {
        rx.recv_timeout(Duration::from_secs(2)).expect("event")
    }

    fn progress(profile: &str, task: &str, transferred: i64) -> BusEvent {
        BusEvent::SyncProgress(SyncProgressEvent {
            profile_id: profile.into(),
            task_id: task.into(),
            transferred,
            ..Default::default()
        })
    }

    #[test]
    fn progress_is_coalesced_per_profile() {
        let r = rig(fast());
        for i in 1..=5 {
            r.bus.publish(progress("p1", "t1", i));
        }
        r.bus.publish(progress("p2", "t2", 7));
        let mut got: Vec<Event> = Vec::new();
        while got.iter().filter(|e| e.name == "sync:progress").count() < 2 {
            got.push(recv(&r.rx));
        }
        let progress: Vec<&Event> = got.iter().filter(|e| e.name == "sync:progress").collect();
        assert_eq!(progress.len(), 2, "one frame per profile");
        let p1 = progress
            .iter()
            .find(|e| e.data["profileID"] == "p1")
            .expect("p1");
        assert_eq!(p1.data["transferred"], 5, "latest value wins");
        // Order of first arrival is preserved.
        assert_eq!(progress[0].data["profileID"], "p1");
        assert_eq!(progress[1].data["profileID"], "p2");
    }

    #[test]
    fn progress_without_profile_is_keyed_by_task() {
        let r = rig(fast());
        r.bus.publish(progress("", "ta", 1));
        r.bus.publish(progress("", "tb", 2));
        let mut n = 0;
        while n < 2 {
            if recv(&r.rx).name == "sync:progress" {
                n += 1;
            }
        }
    }

    #[test]
    fn runtime_snapshot_throttled_and_immediate() {
        let r = rig(BridgeTimings {
            runtime_interval: Duration::from_millis(80),
            ..fast()
        });
        // First request pushes right away (nothing pushed before).
        r.bridge.request_runtime_snapshot(false);
        assert_eq!(recv(&r.rx).name, "runtime:snapshot");
        // A burst now collapses into a single delayed snapshot.
        let t0 = Instant::now();
        for _ in 0..10 {
            r.bridge.request_runtime_snapshot(false);
        }
        assert_eq!(recv(&r.rx).name, "runtime:snapshot");
        assert!(
            t0.elapsed() >= Duration::from_millis(40),
            "throttled: {:?}",
            t0.elapsed()
        );
        assert!(
            r.rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "burst collapsed to one frame"
        );
        assert_eq!(r.snaps.load(Ordering::SeqCst), 2);
        // Immediate bypasses the throttle window.
        let t1 = Instant::now();
        r.bridge.request_runtime_snapshot(true);
        assert_eq!(recv(&r.rx).name, "runtime:snapshot");
        assert!(
            t1.elapsed() < Duration::from_millis(70),
            "immediate: {:?}",
            t1.elapsed()
        );
    }

    #[test]
    fn auth_events_push_event_then_immediate_snapshot() {
        let r = rig(BridgeTimings {
            runtime_interval: Duration::from_secs(3600),
            ..fast()
        });
        r.bridge.request_runtime_snapshot(false);
        assert_eq!(recv(&r.rx).name, "runtime:snapshot");
        r.bus.publish(BusEvent::auth_unlocked());
        let a = recv(&r.rx);
        assert_eq!(a.name, "auth:unlocked");
        assert_eq!(a.data, json!({}));
        assert_eq!(recv(&r.rx).name, "runtime:snapshot");
        r.bus.publish(BusEvent::auth_locked());
        assert_eq!(recv(&r.rx).name, "auth:locked");
    }

    #[test]
    fn simple_events_have_swift_shapes() {
        let r = rig(fast());
        r.bus.publish(BusEvent::state_changed("profiles", "p1"));
        let e = recv(&r.rx);
        assert_eq!(e.name, "state:changed");
        assert_eq!(e.data, json!({"domain": "profiles", "id": "p1"}));
        r.bus
            .publish(BusEvent::schedule_triggered("s1", "p1", "push"));
        let e = recv(&r.rx);
        assert_eq!(e.name, "schedule:triggered");
        assert_eq!(
            e.data,
            json!({"scheduleID": "s1", "profileID": "p1", "action": "push"})
        );
        r.bus
            .publish(BusEvent::board_execution("b", "n", "e", "running", "push"));
        let e = recv(&r.rx);
        assert_eq!(e.name, "board:execution");
        assert_eq!(
            e.data,
            json!({"boardID": "b", "nodeID": "n", "edgeID": "e", "status": "running", "action": "push"})
        );
        r.bus.publish(BusEvent::sync_started("t", "p", "push"));
        let e = recv(&r.rx);
        assert_eq!(e.name, "sync:started");
        assert_eq!(
            e.data,
            json!({"taskID": "t", "profileID": "p", "action": "push"})
        );
        let e = recv(&r.rx);
        assert_eq!(e.name, "runtime:snapshot");
        r.bus
            .publish(BusEvent::sync_completed("t", "p", "push", 3, 10, 0));
        let e = recv(&r.rx);
        assert_eq!(e.name, "sync:completed");
        assert_eq!(e.data["duration"], 3);
        assert_eq!(e.data["bytes"], 10);
        assert_eq!(e.data["errors"], 0);
    }

    #[test]
    fn flow_execution_event_shape() {
        let r = rig(fast());
        r.bus
            .publish(BusEvent::flow_execution("f1", "op1", "running", ""));
        let e = recv(&r.rx);
        assert_eq!(e.name, "flow:execution");
        assert_eq!(
            e.data,
            json!({"flowID": "f1", "opID": "op1", "status": "running", "error": ""})
        );
    }

    #[test]
    fn log_lines_honor_level_and_are_droppable() {
        let r = rig(fast());
        let rec = |level, msg: &str| LogRecord {
            time: chrono::Utc::now(),
            level,
            msg: msg.into(),
            fields: [("k".to_string(), "v".to_string())].into_iter().collect(),
        };
        r.bridge.on_log_record(&rec(LogLevel::Debug, "hidden"));
        r.bridge.on_log_record(&rec(LogLevel::Warn, "shown"));
        let e = recv(&r.rx);
        assert_eq!(e.name, "log:line");
        assert_eq!(e.data["msg"], "shown");
        assert_eq!(e.data["level"], "WARN");
        assert_eq!(e.data["fields"], json!({"k": "v"}));
        assert!(r.rx.try_recv().is_err());

        // Without a consumer, log lines stop at the droppable limit while
        // state events still get through.
        let slow = r.bridge.subscribe();
        for i in 0..(DROPPABLE_LIMIT * 2) {
            r.bridge
                .on_log_record(&rec(LogLevel::Error, &format!("l{i}")));
        }
        assert_eq!(slow.len(), DROPPABLE_LIMIT);
        r.bridge.push_event("oauth:test", json!({}));
        assert_eq!(slow.len(), DROPPABLE_LIMIT + 1);
    }

    #[test]
    fn full_queue_evicts_oldest_for_non_droppable() {
        let r = rig(fast());
        let slow = r.bridge.subscribe();
        for i in 0..(SUBSCRIBER_CAPACITY + 10) {
            r.bridge.push_event("x", json!({"i": i}));
        }
        assert_eq!(slow.len(), SUBSCRIBER_CAPACITY);
        let first = slow.try_recv().unwrap();
        assert_eq!(first.data["i"], 10, "oldest 10 evicted");
    }

    #[test]
    fn heartbeat_pushes_snapshot_only_while_flow_active() {
        let r = rig(BridgeTimings {
            heartbeat: Duration::from_millis(40),
            ..fast()
        });
        assert!(
            r.rx.recv_timeout(Duration::from_millis(150)).is_err(),
            "idle: no heartbeat frames"
        );
        r.active.store(true, Ordering::SeqCst);
        let e = recv(&r.rx);
        assert_eq!(e.name, "runtime:snapshot");
        assert_eq!(e.data["flows"][0]["status"], "running");
    }

    #[test]
    fn shutdown_disconnects_subscribers_and_is_idempotent() {
        let r = rig(fast());
        r.bridge.shutdown();
        r.bridge.shutdown();
        r.bus.publish(BusEvent::state_changed("a", "b"));
        assert!(r.rx.recv_timeout(Duration::from_millis(100)).is_err());
        r.bridge.push_event("x", json!({}));
        assert!(r.rx.try_recv().is_err());
    }
}
