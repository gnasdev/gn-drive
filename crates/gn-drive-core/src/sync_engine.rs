//! Sync orchestration — port of `SyncEngine.swift` / `internal/syncengine`.
//!
//! Task registry, profile and flow cron schedules, and event emission.
//! Waiting blocks on a condition variable and re-checks the caller's
//! `cancelled` predicate at least every 100ms.

use crate::auth::rfc3339;
use crate::cron::{CronRunner, CronSchedule};
use crate::event_bus::{BusEvent, EventBus, SyncProgressEvent};
use crate::logger::Logger;
use crate::models::{Flow, HistoryEntry, Profile, Schedule};
use crate::rclone::stats::truncate_string;
use crate::rclone::{
    CancellationToken, RcloneAction, SyncClient, SyncConfig, SyncFailedError, SyncResult,
    SyncStats, types::ProfileFlags,
};
use crate::store::{Store, StoreError};
use chrono::{DateTime, Utc};
use parking_lot::{Condvar, Mutex};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

/// Display texts match the messages `rpcMapError` shows for each case.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SyncEngineError {
    #[error("sync engine is not running")]
    NotRunning,
    #[error("sync engine has no data plane")]
    StoreNotReady,
    #[error("a sync for this profile is already running")]
    ProfileBusy,
    #[error("{0}")]
    TaskFailed(String),
    #[error("task was cancelled")]
    TaskCancelled,
    #[error("task not found")]
    TaskNotFound,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Point-in-time view of a [`SyncTask`]. Public fields are what the RPC layer
/// and the runtime hub read.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskSnapshot {
    pub id: String,
    pub name: String,
    pub action: String,
    pub status: String,
    pub stats: SyncStats,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

struct TaskState {
    status: String,
    error: String,
    stats: SyncStats,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
}

/// A running or completed sync task. Shared across the engine thread and callers.
pub struct SyncTask {
    id: String,
    name: String,
    action: String,
    state: Mutex<TaskState>,
    token: CancellationToken,
}

impl SyncTask {
    fn new(id: String, name: String, action: String) -> Arc<SyncTask> {
        Arc::new(SyncTask {
            id,
            name,
            action,
            state: Mutex::new(TaskState {
                status: "running".into(),
                error: String::new(),
                stats: SyncStats::default(),
                started_at: Utc::now(),
                ended_at: None,
            }),
            token: CancellationToken::new(),
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn action(&self) -> &str {
        &self.action
    }
    pub fn status(&self) -> String {
        self.state.lock().status.clone()
    }
    pub fn error(&self) -> String {
        self.state.lock().error.clone()
    }
    pub fn stats(&self) -> SyncStats {
        self.state.lock().stats.clone()
    }
    pub fn started_at(&self) -> DateTime<Utc> {
        self.state.lock().started_at
    }
    pub fn ended_at(&self) -> Option<DateTime<Utc>> {
        self.state.lock().ended_at
    }

    fn set_status(&self, s: &str) {
        self.state.lock().status = s.to_string();
    }
    fn set_error(&self, e: &str) {
        self.state.lock().error = e.to_string();
    }
    fn set_stats(&self, s: SyncStats) {
        self.state.lock().stats = s;
    }
    fn set_started_at(&self, d: DateTime<Utc>) {
        self.state.lock().started_at = d;
    }
    fn set_ended_at(&self, d: DateTime<Utc>) {
        self.state.lock().ended_at = Some(d);
    }

    pub fn cancel(&self) {
        self.token.cancel();
        self.set_status("cancelled");
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    fn cancellation_token(&self) -> CancellationToken {
        self.token.clone()
    }

    pub fn snapshot(&self) -> TaskSnapshot {
        let st = self.state.lock();
        TaskSnapshot {
            id: self.id.clone(),
            name: self.name.clone(),
            action: self.action.clone(),
            status: st.status.clone(),
            stats: st.stats.clone(),
            started_at: st.started_at,
            ended_at: st.ended_at,
        }
    }
}

/// Terminal outcome retained after a task leaves the active map.
struct TaskOutcome {
    status: String,
    err_msg: String,
}

struct Active {
    tasks: HashMap<String, Arc<SyncTask>>,
    outcomes: HashMap<String, TaskOutcome>,
    busy: HashSet<String>,
}

impl Active {
    fn new() -> Active {
        Active {
            tasks: HashMap::new(),
            outcomes: HashMap::new(),
            busy: HashSet::new(),
        }
    }
}

/// Records the outcome and wakes [`SyncEngine::wait_task`] even if `run_sync` panics.
struct Finish {
    inner: Arc<Inner>,
    task: Arc<SyncTask>,
    busy_key: String,
}

impl Drop for Finish {
    fn drop(&mut self) {
        let mut status = self.task.status();
        if status.is_empty() || status == "running" {
            status = "failed".into();
            self.task.set_status("failed");
        }
        let err_msg = self.task.error();
        {
            let mut g = self.inner.active.lock();
            g.outcomes
                .insert(self.task.id().to_string(), TaskOutcome { status, err_msg });
            g.tasks.remove(self.task.id());
            g.busy.remove(&self.busy_key);
        }
        self.inner.wait.notify_all();
    }
}

struct Inner {
    log: Logger,
    bus: EventBus,
    store: Mutex<Option<Arc<Store>>>,
    rclone: Mutex<Option<Arc<dyn SyncClient>>>,
    flow_exec: Mutex<Option<Arc<dyn FlowExecutor>>>,
    cron: CronRunner,
    /// Engine-level run flag. Toggled under `life` by start/stop.
    running: std::sync::atomic::AtomicBool,
    /// Serialises start() with stop()'s cron shutdown.
    life: Mutex<()>,
    /// Serialises task admission with stop()'s worker harvest.
    admit: Mutex<()>,
    active: Mutex<Active>,
    wait: Condvar,
    sched: Mutex<HashSet<String>>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

/// Something that can run a flow when its cron fires. [`crate::flow_engine::FlowEngine`]
/// implements this.
pub trait FlowExecutor: Send + Sync {
    fn execute(&self, flow_id: &str) -> Result<(), String>;
}

/// Schedule id prefix for flow cron jobs (`flow:<flow id>`).
pub const FLOW_SCHEDULE_PREFIX: &str = "flow:";

/// Task registry and cron driver.
pub struct SyncEngine {
    inner: Arc<Inner>,
}

impl SyncEngine {
    pub fn new(log: Logger, bus: EventBus) -> Arc<SyncEngine> {
        Arc::new(SyncEngine {
            inner: Arc::new(Inner {
                log,
                bus,
                store: Mutex::new(None),
                rclone: Mutex::new(None),
                flow_exec: Mutex::new(None),
                cron: CronRunner::new(),
                running: std::sync::atomic::AtomicBool::new(false),
                life: Mutex::new(()),
                admit: Mutex::new(()),
                active: Mutex::new(Active::new()),
                wait: Condvar::new(),
                sched: Mutex::new(HashSet::new()),
                workers: Mutex::new(Vec::new()),
            }),
        })
    }

    pub fn set_flow_executor(&self, fe: Arc<dyn FlowExecutor>) {
        *self.inner.flow_exec.lock() = Some(fe);
    }

    /// Wire store + rclone after portal unlock (deferred data plane).
    pub fn attach(&self, store: Arc<Store>, rclone: Arc<dyn SyncClient>) {
        *self.inner.store.lock() = Some(store);
        *self.inner.rclone.lock() = Some(rclone);
        if self.is_running() {
            self.inner.load_schedules();
        }
    }

    pub fn detach(&self) {
        *self.inner.store.lock() = None;
        *self.inner.rclone.lock() = None;
    }

    pub fn start(&self) {
        let _life = self.inner.life.lock();
        if self
            .inner
            .running
            .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            return;
        }
        self.inner.cron.start();
        if self.inner.store.lock().is_some() {
            self.inner.load_schedules();
        }
        self.inner.log.info("syncengine: started", &[]);
    }

    pub fn stop(&self) {
        // Reject new work before joining the cron thread. Handlers call
        // `start_sync`, which takes `admit` but never `life`, so joining the
        // ticker while holding `life` cannot deadlock.
        {
            let _life = self.inner.life.lock();
            self.inner
                .running
                .store(false, std::sync::atomic::Ordering::Release);
            self.inner.cron.stop();
        }
        let workers = {
            let _admit = self.inner.admit.lock();
            let tasks: Vec<Arc<SyncTask>> = {
                let mut g = self.inner.active.lock();
                let tasks = g.tasks.values().cloned().collect();
                g.tasks.clear();
                g.busy.clear();
                tasks
            };
            for t in tasks {
                t.cancel();
            }
            self.inner.sched.lock().clear();
            std::mem::take(&mut *self.inner.workers.lock())
        };
        for h in workers {
            if h.thread().id() != std::thread::current().id() {
                let _ = h.join();
            }
        }
        self.inner.log.info("syncengine: stopped", &[]);
    }

    pub fn is_running(&self) -> bool {
        self.inner
            .running
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// Start a sync for a named profile. Returns the task id.
    pub fn start_sync(&self, action: &str, profile_name: &str) -> Result<String, SyncEngineError> {
        self.inner.start_sync(action, profile_name)
    }

    /// Ad-hoc sync without a stored profile (flow operations).
    /// `busy_key` is the concurrency lock (for example `flowID:opID`).
    pub fn start_path_sync(
        &self,
        action: &str,
        busy_key: &str,
        from: &str,
        to: &str,
        opts: Option<Profile>,
    ) -> Result<String, SyncEngineError> {
        self.inner.start_path_sync(action, busy_key, from, to, opts)
    }

    /// Block until the task finishes. `Ok` only on success.
    ///
    /// When `cancelled` returns true the wait ends with [`SyncEngineError::TaskCancelled`]
    /// and the task itself is left running; the caller stops it.
    pub fn wait_task(
        &self,
        task_id: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), SyncEngineError> {
        self.inner.wait_task(task_id, cancelled)
    }

    /// Cancel an active sync task.
    pub fn stop_sync(&self, task_id: &str) -> Result<(), SyncEngineError> {
        let task = {
            let g = self.inner.active.lock();
            g.tasks.get(task_id).cloned()
        };
        let Some(task) = task else {
            return Err(SyncEngineError::TaskNotFound);
        };
        task.cancel();
        Ok(())
    }

    pub fn active_tasks(&self) -> Vec<TaskSnapshot> {
        let g = self.inner.active.lock();
        g.tasks.values().map(|t| t.snapshot()).collect()
    }

    /// Add or replace the cron job for a profile schedule.
    pub fn register_schedule(&self, sch: &Schedule) {
        self.inner.register_schedule(sch);
    }

    pub fn unregister_schedule(&self, id: &str) {
        self.inner.unregister_schedule(id);
    }

    /// Register or remove a flow's cron job from its persisted fields.
    pub fn sync_flow_schedule(&self, f: &Flow) {
        self.inner.sync_flow_schedule(f);
    }

    pub fn unregister_flow_schedule(&self, flow_id: &str) {
        self.unregister_schedule(&Self::flow_schedule_id(flow_id));
    }

    pub fn flow_schedule_id(flow_id: &str) -> String {
        format!("{FLOW_SCHEDULE_PREFIX}{}", trim_ws(flow_id))
    }

    /// Map a profile onto the rclone flag bag.
    pub fn profile_flags(p: &Profile) -> ProfileFlags {
        profile_flags(p)
    }
}

impl Drop for SyncEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Inner {
    fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::Acquire)
    }

    fn start_sync(
        self: &Arc<Self>,
        action: &str,
        profile_name: &str,
    ) -> Result<String, SyncEngineError> {
        if !self.is_running() {
            return Err(SyncEngineError::NotRunning);
        }
        let store = self
            .store
            .lock()
            .clone()
            .ok_or(SyncEngineError::StoreNotReady)?;
        let profile = store.get_profile(profile_name)?;
        self.start_with_profile(action, profile_name, profile)
    }

    fn start_path_sync(
        self: &Arc<Self>,
        action: &str,
        busy_key: &str,
        from: &str,
        to: &str,
        opts: Option<Profile>,
    ) -> Result<String, SyncEngineError> {
        if !self.is_running() {
            return Err(SyncEngineError::NotRunning);
        }
        if from.is_empty() || to.is_empty() {
            return Err(SyncEngineError::TaskFailed(
                "syncengine: from and to are required".into(),
            ));
        }
        let mut profile = opts.unwrap_or_default();
        profile.name = busy_key.to_string();
        profile.from = from.to_string();
        profile.to = to.to_string();
        if profile.parallel <= 0 {
            profile.parallel = 4;
        }
        self.start_with_profile(action, busy_key, profile)
    }

    fn start_with_profile(
        self: &Arc<Self>,
        action: &str,
        busy_key: &str,
        profile: Profile,
    ) -> Result<String, SyncEngineError> {
        {
            let mut g = self.active.lock();
            if g.busy.contains(busy_key) {
                return Err(SyncEngineError::ProfileBusy);
            }
            g.busy.insert(busy_key.to_string());
        }

        let _admit = self.admit.lock();
        if !self.is_running() {
            self.active.lock().busy.remove(busy_key);
            return Err(SyncEngineError::NotRunning);
        }

        let task = SyncTask::new(new_task_id(), busy_key.to_string(), action.to_string());
        {
            let mut g = self.active.lock();
            g.busy.insert(busy_key.to_string());
            g.tasks.insert(task.id().to_string(), task.clone());
        }

        let task_id = task.id().to_string();
        self.bus
            .publish(BusEvent::sync_started(&task_id, busy_key, action));

        let inner = Arc::clone(self);
        let task_thread = task.clone();
        let action_owned = action.to_string();
        let handle = std::thread::Builder::new()
            .name(format!("sync-{}", &task_id[..task_id.len().min(8)]))
            .spawn(move || inner.run_sync(task_thread, profile, action_owned))
            .expect("spawn sync task");
        self.workers.lock().push(handle);
        Ok(task_id)
    }

    fn wait_task(
        &self,
        task_id: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), SyncEngineError> {
        loop {
            // Checked outside the mutex so a callback that re-enters the engine
            // cannot deadlock. The wait below is bounded so this is polled
            // at least every 100ms.
            if cancelled() {
                return Err(SyncEngineError::TaskCancelled);
            }
            let mut guard = self.active.lock();
            if let Some(o) = guard.outcomes.remove(task_id) {
                return outcome_result(o);
            }
            let _ = self.wait.wait_for(&mut guard, Duration::from_millis(100));
        }
    }

    fn run_sync(self: &Arc<Self>, task: Arc<SyncTask>, profile: Profile, action: String) {
        let _finish = Finish {
            inner: Arc::clone(self),
            task: task.clone(),
            busy_key: task.name().to_string(),
        };

        let started_at = Utc::now();
        task.set_started_at(started_at);
        self.log.info(
            "sync: started",
            &[
                ("task", task.id()),
                ("profile", &profile.name),
                ("action", &action),
            ],
        );

        self.save_history(&HistoryEntry {
            id: task.id().to_string(),
            profile_name: profile.name.clone(),
            action: action.clone(),
            state: "running".into(),
            started_at: rfc3339(started_at),
            ..HistoryEntry::default()
        });

        let cfg = SyncConfig {
            action: RcloneAction::parse(&action).unwrap_or(RcloneAction::Push),
            source: profile.from.clone(),
            dest: profile.to.clone(),
            profile: Some(profile_flags(&profile)),
            ..SyncConfig::default()
        };

        let client = self.rclone.lock().clone();
        let mut sync_err: Option<SyncFailedError> = None;
        let mut result: Option<SyncResult> = None;
        if let Some(client) = client {
            let progress_task = task.clone();
            let progress_inner = Arc::clone(self);
            let progress_profile = profile.name.clone();
            let progress_action = action.clone();
            let progress_id = task.id().to_string();
            let on_progress = move |stats: SyncStats| {
                progress_task.set_stats(stats.clone());
                progress_inner.publish_progress(
                    &progress_id,
                    &progress_profile,
                    &progress_action,
                    "running",
                    &stats,
                    "",
                );
            };
            let token = task.cancellation_token();
            match client.sync(&cfg, Some(&on_progress), Some(&token)) {
                Ok(r) => result = Some(r),
                Err(e) => sync_err = Some(e),
            }
        }

        let ended_at = Utc::now();
        let mut last_stats = task.stats();
        if let Some(res) = &result {
            last_stats = res.stats.clone();
        }
        if let Some(err) = &sync_err {
            last_stats = err.result.stats.clone();
        }
        task.set_stats(last_stats.clone());

        if let Some(err) = &sync_err {
            let cancelled = task.is_cancelled() || task.status() == "cancelled" || err.cancelled;
            let state = if cancelled { "cancelled" } else { "failed" };
            task.set_status(state);
            let raw = err.to_string();
            let msg = if state == "cancelled" {
                String::new()
            } else {
                truncate_string(&sanitize_rclone_err(&raw), 240)
            };
            task.set_error(&msg);
            self.publish_progress(task.id(), &profile.name, &action, state, &last_stats, &msg);
            let mut failed = SyncProgressEvent {
                task_id: task.id().to_string(),
                profile_id: profile.name.clone(),
                action: action.clone(),
                state: state.into(),
                error_message: msg.clone(),
                transfers: last_stats.file_transfers.clone(),
                ..SyncProgressEvent::default()
            };
            failed.timestamp = Utc::now();
            self.bus.publish(BusEvent::SyncFailed(failed));
            let raw_log = raw.clone();
            self.log.error(
                &format!("sync: {state}"),
                &[
                    ("task", task.id()),
                    ("profile", &profile.name),
                    ("err", &raw_log),
                ],
            );
            let hist_err = truncate_string(&raw, 1000);
            task.set_ended_at(ended_at);
            self.save_history(&history_done(
                &task,
                &profile,
                &action,
                state,
                started_at,
                ended_at,
                &last_stats,
                &hist_err,
            ));
        } else {
            let state = "completed";
            task.set_status(state);
            self.publish_progress(task.id(), &profile.name, &action, state, &last_stats, "");
            let duration = result
                .as_ref()
                .map(|r| r.ended_at - r.started_at)
                .unwrap_or(0);
            self.bus.publish(BusEvent::sync_completed(
                task.id(),
                &profile.name,
                &action,
                duration,
                last_stats.bytes,
                last_stats.errors,
            ));
            let bytes = last_stats.bytes.to_string();
            self.log.info(
                "sync: completed",
                &[
                    ("task", task.id()),
                    ("profile", &profile.name),
                    ("bytes", &bytes),
                ],
            );
            task.set_ended_at(ended_at);
            self.save_history(&history_done(
                &task,
                &profile,
                &action,
                state,
                started_at,
                ended_at,
                &last_stats,
                "",
            ));
        }
    }

    fn publish_progress(
        &self,
        task_id: &str,
        profile_id: &str,
        action: &str,
        state: &str,
        s: &SyncStats,
        err_msg: &str,
    ) {
        let ev = SyncProgressEvent {
            timestamp: Utc::now(),
            task_id: task_id.into(),
            profile_id: profile_id.into(),
            action: action.into(),
            state: state.into(),
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
            error_message: err_msg.into(),
        };
        self.bus.publish(BusEvent::SyncProgress(ev));
    }

    fn save_history(&self, entry: &HistoryEntry) {
        let Some(store) = self.store.lock().clone() else {
            return;
        };
        if let Err(e) = store.save_history(entry) {
            let err = e.to_string();
            self.log.warn(
                "sync: persist history failed",
                &[("task", &entry.id), ("err", &err)],
            );
        }
    }

    fn register_schedule(self: &Arc<Self>, sch: &Schedule) {
        let id = sch.id.clone();
        self.sched.lock().remove(&id);
        self.cron.remove(&id);
        if !sch.enabled || sch.cron.is_empty() {
            return;
        }
        let Ok(expr) = CronSchedule::parse(&sch.cron) else {
            self.log.warn(
                "cron: invalid expression",
                &[("schedule", &id), ("cron", &sch.cron)],
            );
            return;
        };
        let cron_label = sch.cron.clone();
        let sch = sch.clone();
        let weak: Weak<Inner> = Arc::downgrade(self);
        let ok = self.cron.add(&id, expr, move || {
            if let Some(inner) = weak.upgrade() {
                inner.trigger_schedule(&sch);
            }
        });
        if ok {
            self.sched.lock().insert(id.clone());
            self.log.info(
                "cron: registered",
                &[("schedule", &id), ("cron", &cron_label)],
            );
        }
    }

    fn trigger_schedule(self: &Arc<Self>, sch: &Schedule) {
        self.log.info(
            "cron: triggering",
            &[("schedule", &sch.id), ("profile", &sch.profile_name)],
        );
        self.bus.publish(BusEvent::schedule_triggered(
            &sch.id,
            &sch.profile_name,
            &sch.action,
        ));
        if let Err(e) = self.start_sync(&sch.action, &sch.profile_name) {
            let err = e.to_string();
            self.log.warn(
                "cron: sync not started",
                &[("schedule", &sch.id), ("err", &err)],
            );
        }
    }

    fn unregister_schedule(&self, id: &str) {
        self.sched.lock().remove(id);
        self.cron.remove(id);
    }

    fn load_schedules(self: &Arc<Self>) {
        let Some(store) = self.store.lock().clone() else {
            return;
        };
        match store.list_schedules() {
            Ok(schedules) => {
                for sch in schedules {
                    self.register_schedule(&sch);
                }
            }
            Err(_) => self.log.warn("syncengine: load schedules failed", &[]),
        }
        self.load_flow_schedules();
    }

    fn load_flow_schedules(self: &Arc<Self>) {
        let Some(store) = self.store.lock().clone() else {
            return;
        };
        let Ok(flows) = store.list_flows() else {
            return;
        };
        for f in flows {
            self.sync_flow_schedule(&f);
        }
    }

    fn sync_flow_schedule(self: &Arc<Self>, f: &Flow) {
        if trim_ws(&f.id).is_empty() {
            return;
        }
        let id = SyncEngine::flow_schedule_id(&f.id);
        self.cron.remove(&id);
        self.sched.lock().remove(&id);
        let enabled = f.schedule_enabled;
        let cron_expr = trim_ws(&f.schedule_cron).to_string();
        if !enabled || cron_expr.is_empty() || self.flow_exec.lock().is_none() {
            return;
        }
        let Ok(sched) = CronSchedule::parse(&cron_expr) else {
            self.log.warn(
                "cron: flow schedule invalid",
                &[("flow", &f.id), ("cron", &cron_expr)],
            );
            return;
        };
        let flow_id = f.id.clone();
        let weak: Weak<Inner> = Arc::downgrade(self);
        let ok = self.cron.add(&id, sched, move || {
            if let Some(inner) = weak.upgrade() {
                inner.trigger_flow_schedule(&flow_id);
            }
        });
        if ok {
            self.sched.lock().insert(id);
            self.log.info(
                "cron: registered flow",
                &[("flow", &f.id), ("cron", &cron_expr)],
            );
        }
    }

    fn trigger_flow_schedule(&self, flow_id: &str) {
        self.log.info("cron: triggering flow", &[("flow", flow_id)]);
        self.bus.publish(BusEvent::schedule_triggered(
            &SyncEngine::flow_schedule_id(flow_id),
            flow_id,
            "flow",
        ));
        let exec = self.flow_exec.lock().clone();
        let Some(exec) = exec else {
            return;
        };
        if let Err(e) = exec.execute(flow_id) {
            self.log
                .warn("cron: flow execute", &[("flow", flow_id), ("err", &e)]);
        }
    }
}

fn outcome_result(o: TaskOutcome) -> Result<(), SyncEngineError> {
    match o.status.as_str() {
        "completed" => Ok(()),
        "cancelled" => Err(SyncEngineError::TaskCancelled),
        _ => Err(SyncEngineError::TaskFailed(o.err_msg)),
    }
}

#[allow(clippy::too_many_arguments)] // one argument per HistoryEntry column
fn history_done(
    task: &SyncTask,
    profile: &Profile,
    action: &str,
    state: &str,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    stats: &SyncStats,
    error_message: &str,
) -> HistoryEntry {
    HistoryEntry {
        id: task.id().to_string(),
        profile_name: profile.name.clone(),
        action: action.to_string(),
        state: state.to_string(),
        started_at: rfc3339(started_at),
        finished_at: rfc3339(ended_at),
        duration: (ended_at - started_at).num_seconds(),
        bytes: stats.bytes,
        errors: stats.errors,
        files: stats.files,
        error_message: error_message.to_string(),
    }
}

/// Swift `UUID().uuidString` is uppercase.
fn new_task_id() -> String {
    uuid::Uuid::new_v4().to_string().to_ascii_uppercase()
}

pub fn profile_flags(p: &Profile) -> ProfileFlags {
    let mut f = ProfileFlags {
        transfers: p.parallel,
        dry_run: p.dry_run,
        max_age: p.max_age.clone(),
        min_age: p.min_age.clone(),
        max_size: p.max_size.clone(),
        min_size: p.min_size.clone(),
        exclude_if_present: p.exclude_if_present.clone(),
        includes: p.included_paths.clone(),
        excludes: p.excluded_paths.clone(),
        buffer_size: p.buffer_size.clone(),
        max_duration: p.max_duration.clone(),
        retries_sleep: p.retries_sleep.clone(),
        conn_timeout: p.conn_timeout.clone(),
        io_timeout: p.io_timeout.clone(),
        order_by: p.order_by.clone(),
        check_first: p.check_first,
        immutable: p.immutable,
        max_transfer: p.max_transfer.clone(),
        max_delete_size: p.max_delete_size.clone(),
        suffix: p.suffix.clone(),
        suffix_keep_extension: p.suffix_keep_extension,
        backup_dir: p.backup_path.clone(),
        size_only: p.size_only,
        update_mode: p.update_mode,
        ignore_existing: p.ignore_existing,
        delete_excluded: p.delete_excluded,
        delete_timing: p.delete_timing.clone(),
        conflict_resolve: p.conflict_resolution.clone(),
        conflict_loser: p.conflict_loser.clone(),
        conflict_suffix: p.conflict_suffix.clone(),
        resilient: p.resilient,
        max_lock: p.max_lock.clone(),
        check_access: p.check_access,
        ..ProfileFlags::default()
    };
    if p.bandwidth > 0 {
        f.bandwidth = format!("{}M", p.bandwidth);
    }
    if let Some(v) = p.tps_limit
        && v > 0.0
    {
        f.tps_limit = v;
    }
    if let Some(v) = p.multi_thread_streams
        && v > 0
    {
        f.multi_thread_streams = v;
    }
    if let Some(v) = p.retries
        && v > 0
    {
        f.retries = v;
    }
    if let Some(v) = p.low_level_retries
        && v > 0
    {
        f.low_level_retries = v;
    }
    if let Some(v) = p.max_delete
        && v > 0
    {
        f.max_delete = v;
    }
    if let Some(v) = p.max_depth
        && v > 0
    {
        f.max_depth = v;
    }
    f
}

/// Drop signal-death and JSON blobs out of a user-facing rclone error.
fn sanitize_rclone_err(s: &str) -> String {
    let lower = s.to_lowercase();
    if lower.contains("signal: killed")
        || lower.contains("signal: interrupt")
        || lower.contains("context canceled")
        || lower.contains("context cancelled")
        || lower.contains("cancelled")
    {
        return String::new();
    }
    let mut out = s.to_string();
    if let Some(i) = out.find("(stderr:") {
        out = trim_ws(&out[..i]).to_string();
    }
    if trim_ws(&out).starts_with('{') {
        return "sync failed".into();
    }
    out
}

/// `CharacterSet.whitespaces` — spaces and tabs, not newlines.
fn trim_ws(s: &str) -> &str {
    s.trim_matches(|c: char| c == '\t' || (c.is_whitespace() && c != '\n' && c != '\r'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::topic;
    use crate::rclone::client::RcloneClient;
    use crate::rclone::testing::{FakeRunner, Response};
    use std::time::Instant;

    fn engine() -> (
        tempfile::TempDir,
        Arc<Store>,
        Arc<FakeRunner>,
        Arc<SyncEngine>,
        EventBus,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let log = Logger::quiet();
        let store =
            Arc::new(Store::open(dir.path().join("db.sqlite").to_str().unwrap(), &log).unwrap());
        let fake = Arc::new(FakeRunner::new());
        let client: Arc<dyn SyncClient> = Arc::new(RcloneClient::with_runner(
            fake.clone(),
            dir.path().join("rclone.conf").to_str().unwrap(),
            &log,
        ));
        let bus = EventBus::new();
        let sync = SyncEngine::new(log, bus.clone());
        sync.attach(store.clone(), client);
        (dir, store, fake, sync, bus)
    }

    fn profile(store: &Store, name: &str) {
        let mut p = Profile {
            name: name.into(),
            from: "/tmp/gnd-src".into(),
            to: "/tmp/gnd-dst".into(),
            direction: "push".into(),
            parallel: 2,
            bandwidth: 8,
            ..Profile::default()
        };
        store.save_profile(&mut p).unwrap();
    }

    #[test]
    fn refuses_work_until_started_and_attached() {
        let log = Logger::quiet();
        let bus = EventBus::new();
        let sync = SyncEngine::new(log, bus);
        assert_eq!(
            sync.start_sync("push", "p"),
            Err(SyncEngineError::NotRunning)
        );
        sync.start();
        assert_eq!(
            sync.start_sync("push", "p"),
            Err(SyncEngineError::StoreNotReady)
        );
        assert_eq!(
            sync.start_path_sync("push", "k", "", "/dst", None),
            Err(SyncEngineError::TaskFailed(
                "syncengine: from and to are required".into()
            ))
        );
        sync.stop();
        assert!(!sync.is_running());
    }

    #[test]
    fn runs_profile_sync_and_writes_history() {
        let (_d, store, fake, sync, bus) = engine();
        let sub = bus.subscribe(&[
            topic::SYNC_STARTED,
            topic::SYNC_COMPLETED,
            topic::SYNC_PROGRESS,
        ]);
        profile(&store, "box");
        sync.start();
        let id = sync.start_sync("push", "box").unwrap();
        assert_eq!(id, id.to_ascii_uppercase());
        sync.wait_task(&id, &|| false).unwrap();
        let hist = store.list_history(10, 0).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].state, "completed");
        assert_eq!(hist[0].profile_name, "box");
        assert!(!fake.calls_for("sync").is_empty());
        let mut saw_started = false;
        let mut saw_completed = false;
        while let Some(ev) = sub.try_recv() {
            match ev {
                BusEvent::SyncStarted(e) => {
                    assert_eq!(e.task_id, id);
                    assert_eq!(e.profile_id, "box");
                    saw_started = true;
                }
                BusEvent::SyncCompleted(e) => {
                    assert_eq!(e.task_id, id);
                    saw_completed = true;
                }
                _ => {}
            }
        }
        assert!(saw_started && saw_completed);
        assert!(sync.active_tasks().is_empty());
        sync.stop();
    }

    #[test]
    fn failure_is_task_failed_without_stderr_tail() {
        let (_d, store, fake, sync, _bus) = engine();
        fake.on(
            &["sync"],
            Response::new().stderr("Failed to sync: boom").exit(1),
        );
        profile(&store, "box");
        sync.start();
        let id = sync.start_sync("push", "box").unwrap();
        let err = sync.wait_task(&id, &|| false).unwrap_err();
        match err {
            SyncEngineError::TaskFailed(msg) => {
                assert!(!msg.is_empty());
                assert!(!msg.contains("(stderr:"));
                assert!(msg.contains("exit 1"));
            }
            other => panic!("unexpected {other:?}"),
        }
        let hist = store.list_history(10, 0).unwrap();
        assert_eq!(hist[0].state, "failed");
        sync.stop();
    }

    #[test]
    fn profile_busy_and_cancel() {
        let (_d, store, fake, sync, _bus) = engine();
        fake.on(&["sync"], Response::new().hang_until_cancelled());
        profile(&store, "box");
        sync.start();
        let id = sync.start_sync("push", "box").unwrap();
        let t0 = Instant::now();
        while sync.active_tasks().is_empty() {
            assert!(t0.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            sync.start_sync("push", "box"),
            Err(SyncEngineError::ProfileBusy)
        );
        let err = sync.wait_task(&id, &|| true).unwrap_err();
        assert_eq!(err, SyncEngineError::TaskCancelled);
        // The task is still running; the caller stops it.
        assert!(sync.active_tasks().iter().any(|t| t.id == id));
        sync.stop_sync(&id).unwrap();
        sync.stop();
        assert_eq!(
            sync.stop_sync("missing"),
            Err(SyncEngineError::TaskNotFound)
        );
    }

    #[test]
    fn profile_flags_map_optional_numbers() {
        let p = Profile {
            parallel: 3,
            bandwidth: 12,
            dry_run: true,
            included_paths: vec!["a".into()],
            excluded_paths: vec!["b".into()],
            tps_limit: Some(2.5),
            multi_thread_streams: Some(4),
            retries: Some(0),
            low_level_retries: Some(6),
            max_delete: Some(9),
            max_depth: None,
            max_age: "7d".into(),
            backup_path: "/bk".into(),
            conflict_resolution: "newer".into(),
            ..Profile::default()
        };
        let f = SyncEngine::profile_flags(&p);
        assert_eq!(f.transfers, 3);
        assert_eq!(f.bandwidth, "12M");
        assert!(f.dry_run);
        assert_eq!(f.includes, vec!["a"]);
        assert_eq!(f.excludes, vec!["b"]);
        assert_eq!(f.tps_limit, 2.5);
        assert_eq!(f.multi_thread_streams, 4);
        assert_eq!(f.retries, 0, "non-positive retries stay unset");
        assert_eq!(f.low_level_retries, 6);
        assert_eq!(f.max_delete, 9);
        assert_eq!(f.max_depth, 0);
        assert_eq!(f.max_age, "7d");
        assert_eq!(f.backup_dir, "/bk");
        assert_eq!(f.conflict_resolve, "newer");

        let zero = Profile {
            bandwidth: 0,
            tps_limit: Some(0.0),
            ..Profile::default()
        };
        let z = profile_flags(&zero);
        assert!(z.bandwidth.is_empty());
        assert_eq!(z.tps_limit, 0.0);
    }

    #[test]
    fn sanitize_strips_stderr_and_cancellations() {
        assert_eq!(
            sanitize_rclone_err("rclone sync a b: exit 1 (stderr: boom)"),
            "rclone sync a b: exit 1"
        );
        assert_eq!(sanitize_rclone_err("signal: killed"), "");
        assert_eq!(sanitize_rclone_err("context canceled by user"), "");
        assert_eq!(sanitize_rclone_err("{not json really"), "sync failed");
        assert_eq!(SyncEngine::flow_schedule_id("  abc \t"), "flow:abc");
    }

    #[test]
    fn registers_and_drops_schedules() {
        let (_d, store, _fake, sync, _bus) = engine();
        profile(&store, "box");
        sync.start();
        sync.register_schedule(&Schedule {
            id: "s1".into(),
            profile_name: "box".into(),
            action: "push".into(),
            cron: "not a cron".into(),
            enabled: true,
            ..Schedule::default()
        });
        assert!(!sync.inner.cron.contains("s1"));
        sync.register_schedule(&Schedule {
            id: "s2".into(),
            profile_name: "box".into(),
            action: "push".into(),
            cron: "0 0 * * *".into(),
            enabled: true,
            ..Schedule::default()
        });
        assert!(sync.inner.cron.contains("s2"));
        sync.register_schedule(&Schedule {
            id: "s2".into(),
            enabled: false,
            cron: "0 0 * * *".into(),
            ..Schedule::default()
        });
        assert!(!sync.inner.cron.contains("s2"));

        let fired = Arc::new(Mutex::new(Vec::new()));
        struct Rec(Arc<Mutex<Vec<String>>>);
        impl FlowExecutor for Rec {
            fn execute(&self, flow_id: &str) -> Result<(), String> {
                self.0.lock().push(flow_id.to_string());
                Ok(())
            }
        }
        sync.set_flow_executor(Arc::new(Rec(fired.clone())));
        let mut flow = Flow {
            id: "f1".into(),
            name: "F".into(),
            schedule_enabled: true,
            schedule_cron: "15 4 * * *".into(),
            ..Flow::default()
        };
        store.save_flow(&mut flow).unwrap();
        sync.sync_flow_schedule(&flow);
        assert!(sync.inner.cron.contains("flow:f1"));
        sync.unregister_flow_schedule("f1");
        assert!(!sync.inner.cron.contains("flow:f1"));
        // Disabled or missing executor removes the job.
        flow.schedule_enabled = false;
        sync.sync_flow_schedule(&flow);
        assert!(!sync.inner.cron.contains("flow:f1"));
        sync.stop();
    }

    #[test]
    fn path_sync_uses_busy_key_as_profile_name() {
        let (_d, _store, fake, sync, _bus) = engine();
        sync.start();
        let id = sync
            .start_path_sync(
                "copy",
                "flow:op",
                "/tmp/gnd-a",
                "/tmp/gnd-b",
                Some(Profile {
                    parallel: 1,
                    ..Profile::default()
                }),
            )
            .unwrap();
        sync.wait_task(&id, &|| false).unwrap();
        let calls = fake.calls_for("copy");
        assert_eq!(calls.len(), 1);
        assert!(calls[0].iter().any(|a| a == "/tmp/gnd-a"));
        assert!(calls[0].iter().any(|a| a == "/tmp/gnd-b"));
        sync.stop();
    }
}
