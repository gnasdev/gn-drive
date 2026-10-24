//! rclone shell-out layer (port of `RcloneClient.swift`, `RcloneTypes.swift`,
//! `StatsParser.swift`). ALL process spawning goes through [`RcloneRunner`]
//! so tests can substitute a fake.

pub mod client;
pub mod stats;
pub use client::RcloneClient;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;
pub mod types;

use parking_lot::Mutex;
use std::sync::Arc;
pub use types::*;

#[derive(Debug, thiserror::Error)]
pub enum RcloneError {
    #[error("rclone: binary not found: {0}")]
    BinaryNotFound(String),
    #[error("rclone {command}: {stderr}")]
    RunFailed { command: String, stderr: String },
    #[error("rclone: cancelled")]
    Cancelled,
}

/// Cooperative cancellation token. `cancel()` marks the token and runs the
/// registered terminate hook (kills the child process).
#[derive(Clone, Default)]
pub struct CancellationToken {
    inner: Arc<TokenInner>,
}

#[derive(Default)]
struct TokenInner {
    state: Mutex<TokenState>,
}

#[derive(Default)]
struct TokenState {
    cancelled: bool,
    terminate: Option<Box<dyn Fn() + Send + Sync>>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_cancelled(&self) -> bool {
        self.inner.state.lock().cancelled
    }
    pub fn cancel(&self) {
        let mut st = self.inner.state.lock();
        st.cancelled = true;
        if let Some(t) = st.terminate.as_ref() {
            t();
        }
    }
    /// Register the terminate hook; fires immediately if already cancelled.
    pub fn on_terminate(&self, f: impl Fn() + Send + Sync + 'static) {
        let mut st = self.inner.state.lock();
        if st.cancelled {
            f();
        }
        st.terminate = Some(Box::new(f));
    }
    pub fn clear_terminate(&self) {
        self.inner.state.lock().terminate = None;
    }
}

/// Returned when a sync exits non-zero or is cancelled; carries the partial
/// result so callers can still read stats / file lists.
#[derive(Debug, Clone)]
pub struct SyncFailedError {
    pub result: SyncResult,
    pub message: String,
    pub cancelled: bool,
}

impl std::fmt::Display for SyncFailedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for SyncFailedError {}

#[derive(Debug, Clone, Default)]
pub struct ProcessOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Stdout,
    Stderr,
}

/// Sole process-spawning seam. The real implementation lives in
/// `client::ProcessRunner`; tests use fakes.
pub trait RcloneRunner: Send + Sync {
    /// Path of the rclone binary (informational).
    fn binary(&self) -> String;

    /// Run to completion, capturing both streams. Non-zero exit is NOT an
    /// error; `Err` only when the process cannot be spawned.
    fn run(&self, args: &[String]) -> std::io::Result<ProcessOutput>;

    /// Run while delivering each complete stdout/stderr line to `on_line`
    /// (may be called concurrently from two threads). When `token` is
    /// cancelled the process is terminated. Returns the exit code.
    fn run_streaming(
        &self,
        args: &[String],
        token: Option<&CancellationToken>,
        on_line: &(dyn Fn(StreamKind, &str) + Sync),
    ) -> std::io::Result<i32>;
}

/// Progress callback for syncs.
pub type ProgressFn<'a> = &'a (dyn Fn(SyncStats) + Send + Sync);

/// What the sync/flow/board engines need from rclone.
#[allow(clippy::result_large_err)]
pub trait SyncClient: Send + Sync {
    fn sync(
        &self,
        cfg: &SyncConfig,
        on_progress: Option<ProgressFn<'_>>,
        token: Option<&CancellationToken>,
    ) -> Result<SyncResult, SyncFailedError>;
}
