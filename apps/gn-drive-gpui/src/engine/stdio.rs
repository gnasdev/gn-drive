//! Opt-in fallback backend: a `gn-drive serve --stdio` child speaking NDJSON.
//!
//! Selected with `GN_DRIVE_BACKEND=stdio`, and the only real backend when the
//! `core` feature is off. The in-process core is the default otherwise.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use super::{Backend, Frame, Inbox, LinkStatus, home};

struct Shared {
    out: Mutex<Option<ChildStdin>>,
    inbox: Inbox,
    status: Mutex<LinkStatus>,
    child: Mutex<Option<Child>>,
    hello: Mutex<Option<Value>>,
    stopped: AtomicBool,
    supervising: AtomicBool,
    healthy: AtomicBool,
    fatal: AtomicBool,
    stderr: Mutex<VecDeque<String>>,
}

impl Shared {
    fn new() -> Self {
        Self {
            out: Mutex::new(None),
            inbox: Inbox::default(),
            status: Mutex::new(LinkStatus::Starting),
            child: Mutex::new(None),
            hello: Mutex::new(None),
            stopped: AtomicBool::new(false),
            supervising: AtomicBool::new(false),
            healthy: AtomicBool::new(false),
            fatal: AtomicBool::new(false),
            stderr: Mutex::new(VecDeque::new()),
        }
    }

    fn set_status(&self, status: LinkStatus) {
        *self.status.lock().unwrap() = status;
    }
}

pub(crate) struct StdioBackend {
    shared: Arc<Shared>,
}

impl StdioBackend {
    pub(crate) fn new() -> Self {
        let shared = Arc::new(Shared::new());
        spawn_supervisor(shared.clone());
        Self { shared }
    }
}

impl Backend for StdioBackend {
    fn call(&self, id: u64, method: &str, params: Value) -> Result<(), String> {
        let line = serde_json::to_string(&json!({
            "id": id, "method": method, "params": params
        }))
        .map_err(|error| error.to_string())?;
        let mut out = self.shared.out.lock().unwrap();
        match out.as_mut() {
            Some(writer) => writer
                .write_all(line.as_bytes())
                .and_then(|_| writer.write_all(b"\n"))
                .and_then(|_| writer.flush())
                .map_err(|error| format!("engine write failed: {error}")),
            None => Err("engine is not connected".into()),
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

    fn stderr_tail(&self) -> Vec<String> {
        self.shared.stderr.lock().unwrap().iter().cloned().collect()
    }

    fn retry(&self) {
        self.shared.fatal.store(false, Ordering::SeqCst);
        self.shared.stopped.store(false, Ordering::SeqCst);
        self.shared.set_status(LinkStatus::Starting);
        if !self.shared.supervising.load(Ordering::SeqCst) {
            spawn_supervisor(self.shared.clone());
        }
    }

    fn shutdown(&self) {
        self.shared.stopped.store(true, Ordering::SeqCst);
        if let Some(writer) = self.shared.out.lock().unwrap().as_mut() {
            let line = "{\"id\":0,\"method\":\"app.quit\",\"params\":{}}\n";
            let _ = writer.write_all(line.as_bytes());
            let _ = writer.flush();
        }
        // Dropping stdin is the EOF the child uses to finish the quit.
        *self.shared.out.lock().unwrap() = None;
        let child = self.shared.child.lock().unwrap().take();
        if let Some(mut child) = child {
            thread::spawn(move || {
                let start = std::time::Instant::now();
                while start.elapsed() < Duration::from_millis(800) {
                    match child.try_wait() {
                        Ok(Some(_)) | Err(_) => return,
                        Ok(None) => thread::sleep(Duration::from_millis(40)),
                    }
                }
                let _ = child.kill();
                let _ = child.wait();
            });
        }
        self.shared.set_status(LinkStatus::Stopped);
    }
}

/// Locate the `gn-drive` CLI for the stdio fallback. Only used when
/// `GN_DRIVE_BACKEND=stdio` (or the `core` feature is disabled).
fn resolve_binary() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("GN_DRIVE_BIN") {
        let path = path.trim();
        if !path.is_empty() {
            let candidate = PathBuf::from(path);
            if candidate.is_file() {
                return Ok(candidate);
            }
            return Err(format!(
                "GN_DRIVE_BIN points at {path}, which is not a file."
            ));
        }
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join("gn-drive");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    let home = home();
    let dev = PathBuf::from(&home).join(".local/state/gn-drive/dev/bin/gn-drive");
    if dev.is_file() {
        return Ok(dev);
    }
    let bundled = PathBuf::from("/Applications/GNDrive.app/Contents/MacOS/gn-drive");
    if bundled.is_file() {
        return Ok(bundled);
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            if dir.is_empty() {
                continue;
            }
            let candidate = PathBuf::from(dir).join("gn-drive");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(
        "The stdio engine fallback needs the `gn-drive` CLI, but it was not found. \
         Put `gn-drive` on PATH or set GN_DRIVE_BIN, or unset GN_DRIVE_BACKEND to use \
         the built-in engine."
            .to_string(),
    )
}

fn spawn_supervisor(shared: Arc<Shared>) {
    if shared.supervising.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(move || {
        let mut attempt = 0usize;
        loop {
            if shared.stopped.load(Ordering::SeqCst) {
                break;
            }
            shared.set_status(LinkStatus::Starting);
            let binary = match resolve_binary() {
                Ok(binary) => binary,
                Err(message) => {
                    shared.set_status(LinkStatus::Fatal {
                        code: "binary_missing".into(),
                        message,
                        holder: None,
                    });
                    shared.fatal.store(true, Ordering::SeqCst);
                    break;
                }
            };
            let spawned = Command::new(&binary)
                .arg("serve")
                .arg("--stdio")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();
            let mut child = match spawned {
                Ok(child) => child,
                Err(error) => {
                    shared.set_status(LinkStatus::Fatal {
                        code: "spawn_failed".into(),
                        message: format!("Could not start {}: {error}", binary.display()),
                        holder: None,
                    });
                    shared.fatal.store(true, Ordering::SeqCst);
                    break;
                }
            };
            shared.healthy.store(false, Ordering::SeqCst);
            *shared.out.lock().unwrap() = child.stdin.take();
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            if let Some(stdout) = stdout {
                let shared = shared.clone();
                thread::spawn(move || {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines() {
                        let Ok(line) = line else { break };
                        if line.trim().is_empty() {
                            continue;
                        }
                        let Ok(value) = serde_json::from_str::<Value>(&line) else {
                            continue;
                        };
                        let event = value.get("event").and_then(Value::as_str);
                        match event {
                            Some("hello") => {
                                let data = value.get("data").cloned().unwrap_or(Value::Null);
                                *shared.hello.lock().unwrap() = Some(data);
                                shared.healthy.store(true, Ordering::SeqCst);
                                shared.set_status(LinkStatus::Ready);
                            }
                            Some("fatal") => {
                                shared.set_status(super::fatal_status(
                                    value.get("data").unwrap_or(&Value::Null),
                                ));
                                shared.fatal.store(true, Ordering::SeqCst);
                            }
                            _ => {}
                        }
                        shared.inbox.push(value);
                    }
                    *shared.out.lock().unwrap() = None;
                });
            }
            if let Some(stderr) = stderr {
                let shared = shared.clone();
                thread::spawn(move || {
                    let reader = BufReader::new(stderr);
                    for line in reader.lines() {
                        let Ok(line) = line else { break };
                        if line.trim().is_empty() {
                            continue;
                        }
                        // Keep the tail; the window can show it when startup fails.
                        let mut tail = shared.stderr.lock().unwrap();
                        tail.push_back(line);
                        while tail.len() > 40 {
                            tail.pop_front();
                        }
                    }
                });
            }
            *shared.child.lock().unwrap() = Some(child);
            let exit = loop {
                if shared.stopped.load(Ordering::SeqCst) {
                    if let Some(mut child) = shared.child.lock().unwrap().take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    break None;
                }
                let waited = {
                    let mut guard = shared.child.lock().unwrap();
                    match guard.as_mut() {
                        Some(child) => child.try_wait(),
                        None => break None,
                    }
                };
                match waited {
                    Ok(Some(status)) => break Some(status),
                    Ok(None) => thread::sleep(Duration::from_millis(120)),
                    Err(_) => break None,
                }
            };
            let _ = shared.child.lock().unwrap().take();
            *shared.out.lock().unwrap() = None;
            if shared.stopped.load(Ordering::SeqCst) {
                break;
            }
            if shared.fatal.load(Ordering::SeqCst) {
                break;
            }
            attempt = if shared.healthy.load(Ordering::SeqCst) {
                0
            } else {
                attempt + 1
            };
            let delay = [1u64, 2, 4, 8, 16, 30][attempt.min(5)];
            let reason = match exit {
                Some(status) => format!("engine exited ({status})"),
                None => "engine exited".into(),
            };
            shared.set_status(LinkStatus::Restarting {
                delay,
                reason: reason.clone(),
            });
            thread::sleep(Duration::from_secs(delay));
        }
        shared.supervising.store(false, Ordering::SeqCst);
    });
}
