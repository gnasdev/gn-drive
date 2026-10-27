//! Default backend: `gn-drive-core` running inside the window process.
//!
//! The dispatcher is the same one `gn-drive serve --stdio` wraps, so request
//! and event shapes are identical to the NDJSON protocol (docs/gpui/rpc.md).
//! The UI thread never blocks on it: construction happens on a start-up
//! thread, every request runs on a small worker pool, and pushed events are
//! forwarded by their own thread. All three feed the same [`Inbox`].

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use gn_drive_core::container::{AppContainer, AppError, ContainerOptions};
use gn_drive_core::instance_lock::{InstanceError, InstanceLocker};
use gn_drive_core::keychain::{KeychainStore, SecureStore};
use gn_drive_core::logger::LogMode;
use gn_drive_core::paths::Paths;
use gn_drive_core::rpc::stdio::RpcService;
use gn_drive_core::rpc::{Dispatcher, Options, RpcError};
use serde_json::{Value, json};

use super::{Backend, Frame, Inbox, LinkStatus};

/// Same parallelism as the stdio service (`WORKER_SLOTS`).
const WORKERS: usize = 8;
/// In-flight requests get this long to finish on shutdown (rpc.md §2.3).
const SHUTDOWN_WAIT: Duration = Duration::from_secs(5);
/// `log:line` is the only droppable push; shed it when the UI falls behind.
const LOG_LINE_BACKLOG: usize = 512;

/// Inputs of the in-process engine. Tests swap these to stay out of the
/// user's real config directory and keychain.
pub(crate) struct CoreConfig {
    /// Empty = `Paths::detect()` (honors `GN_DRIVE_CONFIG_DIR`).
    pub(crate) config_dir: String,
    /// Empty = resolve the bundled / PATH rclone.
    pub(crate) rclone: String,
    pub(crate) key_store: Arc<dyn SecureStore>,
    /// Refuse to start while the legacy SwiftUI app runs (rpc.md §2.1 step 3).
    pub(crate) check_legacy_app: bool,
}

fn env_path(name: &str) -> String {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
}

impl CoreConfig {
    pub(crate) fn system() -> Self {
        Self {
            // Empty config dir still goes through Paths::detect, which honors
            // GN_DRIVE_CONFIG_DIR. An explicit value wins over the home default.
            config_dir: env_path("GN_DRIVE_CONFIG_DIR"),
            // A fake rclone for screenshots and tests. Empty lets the core
            // resolve its bundled binary, then PATH.
            rclone: env_path("GN_DRIVE_RCLONE"),
            key_store: Arc::new(KeychainStore::new("gn-drive")),
            check_legacy_app: true,
        }
    }
}

struct Job {
    id: u64,
    method: String,
    params: Value,
}

struct Running {
    dispatcher: Arc<Dispatcher>,
    container: Arc<AppContainer>,
    locker: InstanceLocker,
    jobs: Sender<Job>,
}

enum Phase {
    /// Requests that arrive before the engine is up wait here.
    Starting(Vec<Job>),
    Running(Running),
    Failed(String),
    Stopped,
}

struct Shared {
    inbox: Inbox,
    status: Mutex<LinkStatus>,
    hello: Mutex<Option<Value>>,
    phase: Mutex<Phase>,
    in_flight: AtomicUsize,
    /// Bumped by `shutdown` and `retry`; a start-up thread from an older
    /// generation tears down what it built instead of publishing it.
    generation: AtomicUsize,
    stopped: AtomicBool,
}

impl Shared {
    fn set_status(&self, status: LinkStatus) {
        *self.status.lock().unwrap() = status;
    }

    fn fail(&self, code: &str, message: String, holder: Option<String>) {
        let mut phase = self.phase.lock().unwrap();
        // Drop the queued requests; answer each so the UI's pending map clears.
        if let Phase::Starting(queue) =
            std::mem::replace(&mut *phase, Phase::Failed(message.clone()))
        {
            for job in queue {
                self.inbox.push(error_frame(
                    job.id,
                    &RpcError::new("engine_not_ready", message.clone()),
                ));
            }
        }
        drop(phase);
        self.set_status(LinkStatus::Fatal {
            code: code.into(),
            message,
            holder,
        });
    }
}

pub(crate) struct CoreBackend {
    shared: Arc<Shared>,
    config: Arc<CoreConfig>,
}

impl CoreBackend {
    pub(crate) fn new() -> Self {
        Self::with_config(CoreConfig::system())
    }

    pub(crate) fn with_config(config: CoreConfig) -> Self {
        let backend = Self {
            shared: Arc::new(Shared {
                inbox: Inbox::default(),
                status: Mutex::new(LinkStatus::Starting),
                hello: Mutex::new(None),
                phase: Mutex::new(Phase::Starting(Vec::new())),
                in_flight: AtomicUsize::new(0),
                generation: AtomicUsize::new(0),
                stopped: AtomicBool::new(false),
            }),
            config: Arc::new(config),
        };
        backend.spawn_start();
        backend
    }

    fn spawn_start(&self) {
        let generation = self.shared.generation.load(Ordering::SeqCst);
        let shared = self.shared.clone();
        let config = self.config.clone();
        thread::Builder::new()
            .name("gn-drive.core.start".into())
            .spawn(move || start(shared, config, generation))
            .expect("spawn engine start-up thread");
    }
}

impl Backend for CoreBackend {
    fn call(&self, id: u64, method: &str, params: Value) -> Result<(), String> {
        let job = Job {
            id,
            method: method.to_string(),
            params,
        };
        let mut phase = self.shared.phase.lock().unwrap();
        match &mut *phase {
            Phase::Starting(queue) => {
                queue.push(job);
                Ok(())
            }
            Phase::Running(running) => {
                self.shared.in_flight.fetch_add(1, Ordering::SeqCst);
                running.jobs.send(job).map_err(|_| {
                    self.shared.in_flight.fetch_sub(1, Ordering::SeqCst);
                    "engine is shutting down".to_string()
                })
            }
            Phase::Failed(message) => Err(format!("engine is not running: {message}")),
            Phase::Stopped => Err("engine is stopped".into()),
        }
    }

    fn poll(&self) -> Vec<Frame> {
        self.shared.inbox.drain()
    }

    fn status(&self) -> LinkStatus {
        self.shared.status.lock().unwrap().clone()
    }

    fn hello(&self) -> Option<Value> {
        self.shared.hello.lock().unwrap().clone()
    }

    fn retry(&self) {
        {
            let mut phase = self.shared.phase.lock().unwrap();
            if !matches!(&*phase, Phase::Failed(_)) {
                return;
            }
            *phase = Phase::Starting(Vec::new());
        }
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        self.shared.set_status(LinkStatus::Starting);
        self.spawn_start();
    }

    fn shutdown(&self) {
        if self.shared.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        let previous = std::mem::replace(&mut *self.shared.phase.lock().unwrap(), Phase::Stopped);
        self.shared.set_status(LinkStatus::Stopped);
        if let Phase::Running(running) = previous {
            teardown(&self.shared, running);
        }
    }
}

/// rpc.md §2.3 minus the stdio parts: let in-flight requests finish, stop the
/// sync engine and cron, close the container (re-encrypts config, keeps the
/// Keychain key) and release the instance lock.
fn teardown(shared: &Shared, running: Running) {
    let Running {
        dispatcher,
        container,
        locker,
        jobs,
    } = running;
    // Closing the queue stops the workers once they drain it.
    drop(jobs);
    let deadline = Instant::now() + SHUTDOWN_WAIT;
    while shared.in_flight.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    dispatcher.shutdown();
    container.sync_engine.stop();
    container.close();
    locker.release();
}

fn start(shared: Arc<Shared>, config: Arc<CoreConfig>, generation: usize) {
    let built = catch_unwind(AssertUnwindSafe(|| build(&config)));
    let (dispatcher, container, locker, lock_path) = match built {
        Ok(Ok(parts)) => parts,
        Ok(Err((code, message, holder))) => {
            if shared.generation.load(Ordering::SeqCst) == generation {
                shared.fail(&code, message, holder);
            }
            return;
        }
        Err(_) => {
            if shared.generation.load(Ordering::SeqCst) == generation {
                shared.fail(
                    "init_failed",
                    "initialization failed: the engine panicked during start-up".into(),
                    None,
                );
            }
            return;
        }
    };

    let (jobs, queue) = channel::<Job>();
    let running = Running {
        dispatcher: dispatcher.clone(),
        container: container.clone(),
        locker,
        jobs: jobs.clone(),
    };

    // Subscribe before `hello` so nothing is missed, forward only after it is
    // queued so `hello` stays the first frame.
    let events = dispatcher.subscribe();
    let hello = dispatcher.hello_data(&lock_path, std::process::id() as i32);

    {
        let mut phase = shared.phase.lock().unwrap();
        if shared.generation.load(Ordering::SeqCst) != generation {
            // Shut down (or retried) while we were building.
            drop(phase);
            teardown(&shared, running);
            return;
        }
        let Phase::Starting(waiting) = std::mem::replace(&mut *phase, Phase::Stopped) else {
            drop(phase);
            teardown(&shared, running);
            return;
        };
        *shared.hello.lock().unwrap() = Some(hello.clone());
        shared
            .inbox
            .push(json!({"event": "hello", "ts": super::now_ms(), "data": hello}));
        shared.in_flight.fetch_add(waiting.len(), Ordering::SeqCst);
        for job in waiting {
            let _ = jobs.send(job);
        }
        *phase = Phase::Running(running);
    }
    drop(jobs);

    let queue = Arc::new(Mutex::new(queue));
    for index in 0..WORKERS {
        let shared = shared.clone();
        let dispatcher = dispatcher.clone();
        let queue = queue.clone();
        thread::Builder::new()
            .name(format!("gn-drive.core.worker-{index}"))
            .spawn(move || work(&shared, &dispatcher, &queue))
            .expect("spawn engine worker");
    }
    {
        let shared = shared.clone();
        thread::Builder::new()
            .name("gn-drive.core.events".into())
            .spawn(move || {
                loop {
                    match events.recv_timeout(Duration::from_millis(250)) {
                        Ok(event) => {
                            if event.name == "log:line" && shared.inbox.len() > LOG_LINE_BACKLOG {
                                continue;
                            }
                            shared.inbox.push(json!({
                                "event": event.name,
                                "ts": event.ts,
                                "data": event.data,
                            }));
                        }
                        Err(error) if error.is_disconnected() => break,
                        Err(_) => {
                            if shared.stopped.load(Ordering::SeqCst) {
                                break;
                            }
                        }
                    }
                }
            })
            .expect("spawn engine event forwarder");
    }

    container.sync_engine.start();
    shared.set_status(LinkStatus::Ready);
}

type Built = (Arc<Dispatcher>, Arc<AppContainer>, InstanceLocker, String);
type StartError = (String, String, Option<String>);

/// rpc.md §2.1 steps 3–6; the failure codes match the `fatal` frame table.
fn build(config: &CoreConfig) -> Result<Built, StartError> {
    let fail = |code: &str, message: String| -> StartError { (code.into(), message, None) };

    if config.check_legacy_app && RpcService::is_app_running() {
        return Err((
            "instance_busy".into(),
            "the GN Drive app is running. Close it before starting another window.".into(),
            Some("app".into()),
        ));
    }

    let config_dir = if config.config_dir.is_empty() {
        Paths::detect().config_dir
    } else {
        config.config_dir.clone()
    };
    let locker = match InstanceLocker::acquire(&config_dir) {
        Ok(locker) => locker,
        Err(InstanceError::AnotherInstance { pid }) => {
            let holder = pid
                .map(RpcService::classify_holder)
                .unwrap_or_else(|| "cli".to_string());
            let mut message = "another gn-drive instance is running".to_string();
            if let Some(pid) = pid {
                message.push_str(&format!(" (pid={pid}, holder={holder})"));
            }
            message.push_str(". Stop it with 'gn-drive service stop' or close the other window.");
            return Err(("instance_busy".into(), message, Some(holder)));
        }
        Err(error) => {
            return Err(fail(
                "init_failed",
                format!("initialization failed: {error}"),
            ));
        }
    };

    let options = ContainerOptions {
        portal_mode: true,
        key_store: Some(config.key_store.clone()),
        version: gn_drive_core::build_version::CURRENT.to_string(),
        log_mode: LogMode::Foreground,
        config_dir: config.config_dir.clone(),
        rclone_binary: config.rclone.clone(),
        ..ContainerOptions::default()
    };
    let container = match AppContainer::new(options) {
        Ok(container) => container,
        Err(error) => {
            locker.release();
            return Err(match error {
                AppError::RcloneMissing(error) => {
                    fail("rclone_missing", format!("rclone unavailable: {error}"))
                }
                AppError::Locked => fail("locked", "auth: app is locked".into()),
                error => fail("init_failed", format!("initialization failed: {error}")),
            });
        }
    };

    let dispatcher = Dispatcher::start(
        container.clone(),
        &config.rclone,
        Options {
            exit_on_shutdown: false,
            ..Options::default()
        },
    );
    let lock_path = std::path::Path::new(&config_dir)
        .join("gn-drive.lock")
        .to_string_lossy()
        .into_owned();
    Ok((dispatcher, container, locker, lock_path))
}

fn work(shared: &Shared, dispatcher: &Dispatcher, queue: &Mutex<Receiver<Job>>) {
    loop {
        // Hold the lock only while waiting; handlers run unlocked.
        let job = {
            let queue = queue.lock().unwrap();
            queue.recv()
        };
        let Ok(Job { id, method, params }) = job else {
            return;
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| dispatcher.handle(&method, params)));
        let frame = match outcome {
            Ok(Ok(result)) => json!({"id": id, "result": result}),
            Ok(Err(error)) => error_frame(id, &error),
            Err(_) => error_frame(
                id,
                &RpcError::new("internal_error", "request handler panicked"),
            ),
        };
        shared.inbox.push(frame);
        shared.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

fn error_frame(id: u64, error: &RpcError) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert("code".into(), Value::String(error.code.clone()));
    payload.insert("message".into(), Value::String(error.message.clone()));
    if let Some(data) = &error.data {
        payload.insert("data".into(), data.clone());
    }
    json!({"id": id, "error": Value::Object(payload)})
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use gn_drive_core::keychain::MemorySecureStore;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gn-drive-gpui-{tag}-{}-{}",
            std::process::id(),
            crate::engine::now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn config(dir: &std::path::Path) -> CoreConfig {
        CoreConfig {
            config_dir: dir.to_string_lossy().into_owned(),
            // Never resolve or run a real rclone from tests.
            rclone: dir.join("no-such-rclone").to_string_lossy().into_owned(),
            key_store: Arc::new(MemorySecureStore::new()),
            check_legacy_app: false,
        }
    }

    fn wait_for_link(backend: &CoreBackend) -> LinkStatus {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let status = backend.status();
            if !matches!(status, LinkStatus::Starting) || Instant::now() > deadline {
                return status;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_for_frame(backend: &CoreBackend, id: u64) -> Frame {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut seen = VecDeque::new();
        loop {
            for frame in backend.poll() {
                if frame.id() == Some(id) {
                    return frame;
                }
                seen.push_back(frame);
            }
            assert!(Instant::now() < deadline, "no response for request {id}");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn in_process_engine_answers_read_only_calls() {
        let dir = temp_dir("smoke");
        let backend = CoreBackend::with_config(config(&dir));
        let status = wait_for_link(&backend);
        let engine = crate::engine::Engine::with_backend(Box::new(backend));
        // Whether rclone-less start-up is fatal is the core's call; either way
        // the link must settle without blocking this thread.
        match status {
            LinkStatus::Ready => {
                let hello = engine.hello().expect("hello after ready");
                assert_eq!(hello["protocol"], 1);
                assert_eq!(hello["paths"]["configDir"], dir.to_string_lossy().as_ref());
                let id = engine.request("ping", json!({})).unwrap();
                let backend = &engine;
                let deadline = Instant::now() + Duration::from_secs(20);
                let frame = loop {
                    if let Some(frame) = backend.poll().into_iter().find(|f| f.id() == Some(id)) {
                        break frame;
                    }
                    assert!(Instant::now() < deadline, "ping never answered");
                    thread::sleep(Duration::from_millis(20));
                };
                assert_eq!(frame.result().unwrap()["pong"], true);
            }
            LinkStatus::Fatal { code, .. } => {
                assert_eq!(code, "rclone_missing");
            }
            other => panic!("unexpected link status {other:?}"),
        }
        engine.shutdown();
        assert_eq!(engine.status(), LinkStatus::Stopped);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn second_instance_reports_instance_busy() {
        let dir = temp_dir("busy");
        let first = CoreBackend::with_config(config(&dir));
        let first_status = wait_for_link(&first);
        if first_status.is_ready() {
            let second = CoreBackend::with_config(config(&dir));
            match wait_for_link(&second) {
                LinkStatus::Fatal { code, holder, .. } => {
                    assert_eq!(code, "instance_busy");
                    assert!(holder.is_some());
                }
                other => panic!("expected instance_busy, got {other:?}"),
            }
        }
        first.shutdown();
        let _ = wait_for_frame;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
