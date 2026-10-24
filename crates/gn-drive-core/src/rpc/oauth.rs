//! OAuth remote creation: `rclone authorize` session (port of `RPCOAuth.swift`).
//!
//! Tokens live only in this process, are never logged, and are cleared when
//! the session is completed, cancelled, or expires. The long-lived
//! `rclone authorize` child is created through the [`OAuthSpawner`] seam so
//! tests never launch a real process or browser.

use super::RpcError;
use super::error::{redact, stderr_field};
use crate::logger::Logger;
use crate::rclone::StreamKind;
use parking_lot::{Condvar, Mutex};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

/// Session lifetime (10 minutes).
pub const SESSION_LIFETIME: Duration = Duration::from_secs(10 * 60);
/// How long `start` waits for rclone to print the authorization URL.
const URL_WAIT: Duration = Duration::from_secs(10);

/// Chunk callback: `(stream, text)`; may run on reader threads.
pub type ChunkFn = Arc<dyn Fn(StreamKind, &str) + Send + Sync>;
/// Exit callback: exit code (negative = killed by signal / unknown). Runs after
/// every chunk has been delivered.
pub type ExitFn = Arc<dyn Fn(i32) + Send + Sync>;

/// Handle to a spawned `rclone authorize` child.
pub trait OAuthChild: Send + Sync {
    /// Ask the process to terminate (SIGTERM); idempotent.
    fn terminate(&self);
}

/// Process-spawning seam for the long-lived authorize child.
pub trait OAuthSpawner: Send + Sync {
    fn spawn(
        &self,
        binary: &str,
        args: &[String],
        on_chunk: ChunkFn,
        on_exit: ExitFn,
    ) -> std::io::Result<Arc<dyn OAuthChild>>;
}

/// Real spawner: `std::process::Command` with piped stdout/stderr.
pub struct RealOAuthSpawner;

struct RealChild {
    pid: i32,
    exited: Arc<AtomicBool>,
}

impl OAuthChild for RealChild {
    fn terminate(&self) {
        if !self.exited.load(Ordering::SeqCst) && self.pid > 0 {
            // SAFETY: plain signal delivery to the child we spawned; guarded
            // against pid reuse by `exited`, set right after `wait` returns.
            unsafe {
                libc::kill(self.pid, libc::SIGTERM);
            }
        }
    }
}

fn pump(mut r: impl Read, kind: StreamKind, on_chunk: ChunkFn) {
    let mut buf = [0u8; 4096];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        match r.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                carry.extend_from_slice(&buf[..n]);
                let valid = match std::str::from_utf8(&carry) {
                    Ok(_) => carry.len(),
                    Err(e) if e.error_len().is_none() => e.valid_up_to(),
                    Err(_) => carry.len(),
                };
                let text = String::from_utf8_lossy(&carry[..valid]).into_owned();
                carry.drain(..valid);
                if !text.is_empty() {
                    on_chunk(kind, &text);
                }
            }
        }
    }
    if !carry.is_empty() {
        on_chunk(kind, &String::from_utf8_lossy(&carry));
    }
}

impl OAuthSpawner for RealOAuthSpawner {
    fn spawn(
        &self,
        binary: &str,
        args: &[String],
        on_chunk: ChunkFn,
        on_exit: ExitFn,
    ) -> std::io::Result<Arc<dyn OAuthChild>> {
        use std::process::{Command, Stdio};
        let mut child = Command::new(binary)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let pid = child.id() as i32;
        let exited = Arc::new(AtomicBool::new(false));
        let out = child.stdout.take();
        let err = child.stderr.take();
        let mut readers = Vec::new();
        if let Some(o) = out {
            let cb = on_chunk.clone();
            readers.push(std::thread::spawn(move || pump(o, StreamKind::Stdout, cb)));
        }
        if let Some(e) = err {
            let cb = on_chunk.clone();
            readers.push(std::thread::spawn(move || pump(e, StreamKind::Stderr, cb)));
        }
        let flag = exited.clone();
        std::thread::spawn(move || {
            let code = match child.wait() {
                Ok(st) => st.code().unwrap_or(-1),
                Err(_) => -1,
            };
            flag.store(true, Ordering::SeqCst);
            for r in readers {
                let _ = r.join();
            }
            on_exit(code);
        });
        Ok(Arc::new(RealChild { pid, exited }))
    }
}

#[derive(Default)]
struct SessionState {
    token: String,
    auth_url: Option<String>,
    /// waiting | authorized | failed | cancelled
    status: String,
    error_message: String,
    stdout_buffer: String,
    stderr_buffer: String,
    finished: bool,
}

pub struct OAuthSession {
    pub id: String,
    pub r#type: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    state: Mutex<SessionState>,
    cv: Condvar,
    child: Mutex<Option<Arc<dyn OAuthChild>>>,
}

/// `(status, token, error)` of a session.
#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    pub status: String,
    pub token: String,
    pub error: String,
}

impl OAuthSession {
    fn new(id: String, r#type: String) -> OAuthSession {
        OAuthSession {
            id,
            r#type,
            started_at: chrono::Utc::now(),
            state: Mutex::new(SessionState {
                status: "waiting".into(),
                ..Default::default()
            }),
            cv: Condvar::new(),
            child: Mutex::new(None),
        }
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        let st = self.state.lock();
        SessionSnapshot {
            status: st.status.clone(),
            token: st.token.clone(),
            error: st.error_message.clone(),
        }
    }

    pub fn auth_url(&self) -> Option<String> {
        self.state.lock().auth_url.clone()
    }

    pub fn is_finished(&self) -> bool {
        self.state.lock().finished
    }

    /// Block until the status leaves `waiting` or `timeout` elapses.
    pub fn wait_not_waiting(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let mut st = self.state.lock();
        while st.status == "waiting" {
            if self.cv.wait_until(&mut st, deadline).timed_out() {
                break;
            }
        }
    }

    fn terminate_child(&self) {
        let child = self.child.lock().clone();
        if let Some(c) = child {
            c.terminate();
        }
    }
}

/// Push callback (`remotes:oauth` events): `(event name, data)`.
pub type PushFn = Arc<dyn Fn(&str, Value) + Send + Sync>;

struct Inner {
    binary: String,
    config_path: String,
    log: Logger,
    spawner: Arc<dyn OAuthSpawner>,
    push: PushFn,
    lifetime: Duration,
    url_wait: Duration,
    reg: Mutex<Registry>,
}

#[derive(Default)]
struct Registry {
    sessions: HashMap<String, Arc<OAuthSession>>,
    active_id: Option<String>,
    counter: u64,
}

pub struct OAuthManager {
    inner: Arc<Inner>,
}

impl OAuthManager {
    pub const SESSION_LIFETIME: Duration = SESSION_LIFETIME;

    pub fn new(
        binary: &str,
        config_path: &str,
        log: Logger,
        spawner: Arc<dyn OAuthSpawner>,
        push: PushFn,
    ) -> OAuthManager {
        OAuthManager::with_timing(
            binary,
            config_path,
            log,
            spawner,
            push,
            SESSION_LIFETIME,
            URL_WAIT,
        )
    }

    /// Like [`OAuthManager::new`] with custom session lifetime / URL wait (tests).
    pub fn with_timing(
        binary: &str,
        config_path: &str,
        log: Logger,
        spawner: Arc<dyn OAuthSpawner>,
        push: PushFn,
        lifetime: Duration,
        url_wait: Duration,
    ) -> OAuthManager {
        OAuthManager {
            inner: Arc::new(Inner {
                binary: binary.to_string(),
                config_path: config_path.to_string(),
                log,
                spawner,
                push,
                lifetime,
                url_wait,
                reg: Mutex::new(Registry::default()),
            }),
        }
    }

    pub fn session_lifetime(&self) -> Duration {
        self.inner.lifetime
    }

    pub fn has_active_session(&self) -> bool {
        self.inner.reg.lock().active_id.is_some()
    }

    pub fn start(
        &self,
        r#type: &str,
        client_id: Option<&str>,
        client_secret: Option<&str>,
    ) -> Result<Arc<OAuthSession>, RpcError> {
        let inner = &self.inner;
        let session = {
            let mut reg = inner.reg.lock();
            if let Some(active) = reg.active_id.clone()
                && let Some(existing) = reg.sessions.get(&active)
                && !existing.is_finished()
            {
                return Err(RpcError::new(
                    "oauth_session_active",
                    format!("another OAuth authorization is already running (session {active})"),
                ));
            }
            reg.counter += 1;
            let s = Arc::new(OAuthSession::new(
                format!("O{}", reg.counter),
                r#type.to_string(),
            ));
            reg.sessions.insert(s.id.clone(), s.clone());
            reg.active_id = Some(s.id.clone());
            s
        };

        let mut args = vec!["authorize".to_string(), r#type.to_string()];
        if let (Some(id), Some(secret)) = (client_id, client_secret) {
            args.push(id.to_string());
            args.push(secret.to_string());
        }
        args.extend([
            "--auth-no-open-browser".to_string(),
            "--config".to_string(),
            inner.config_path.clone(),
        ]);

        let weak = Arc::downgrade(inner);
        let chunk_session = Arc::downgrade(&session);
        let on_chunk: ChunkFn = Arc::new(move |kind, text| {
            if let Some(s) = chunk_session.upgrade() {
                consume(&s, kind, text);
            }
        });
        let exit_session = Arc::downgrade(&session);
        let exit_weak = weak.clone();
        let on_exit: ExitFn = Arc::new(move |code| {
            if let (Some(inner), Some(s)) = (exit_weak.upgrade(), exit_session.upgrade()) {
                process_exited(&inner, &s, code);
            }
        });

        match inner.spawner.spawn(&inner.binary, &args, on_chunk, on_exit) {
            Ok(child) => {
                *session.child.lock() = Some(child);
                // The child may have exited before the handle was stored.
                if session.is_finished() {
                    session.child.lock().take();
                }
            }
            Err(e) => {
                finish(
                    inner,
                    &session,
                    "failed",
                    &format!("could not start rclone authorize: {e}"),
                );
                return Err(RpcError::new(
                    "oauth_failed",
                    "could not start rclone authorize",
                ));
            }
        }

        schedule_expiry(weak, session.clone(), inner.lifetime);

        // rclone prints the URL to stderr within a few seconds.
        let deadline = Instant::now() + inner.url_wait;
        {
            let mut st = session.state.lock();
            loop {
                if st.auth_url.as_deref().is_some_and(|u| !u.is_empty()) || st.finished {
                    break;
                }
                if session.cv.wait_until(&mut st, deadline).timed_out() {
                    break;
                }
            }
        }
        let (url, status, err, finished) = {
            let st = session.state.lock();
            (
                st.auth_url.clone(),
                st.status.clone(),
                st.error_message.clone(),
                st.finished,
            )
        };

        if let Some(url) = url.filter(|u| !u.is_empty()) {
            (inner.push)(
                "remotes:oauth",
                json!({"sessionID": session.id, "status": "waiting", "authURL": url}),
            );
            return Ok(session);
        }
        if finished || status != "waiting" {
            self.forget(&session.id);
            return Err(RpcError::new(
                "oauth_failed",
                if err.is_empty() {
                    "rclone authorize did not produce a URL".to_string()
                } else {
                    redact::sanitize(&err)
                },
            ));
        }
        // Still waiting: return without a URL (the client may poll the event).
        Ok(session)
    }

    pub fn session(&self, id: &str) -> Option<Arc<OAuthSession>> {
        self.inner.reg.lock().sessions.get(id).cloned()
    }

    pub fn cancel(&self, id: &str) -> bool {
        let Some(session) = self.session(id) else {
            return false;
        };
        if session.is_finished() {
            return false;
        }
        {
            let mut st = session.state.lock();
            st.status = "cancelled".into();
            st.token.clear();
        }
        session.terminate_child();
        finish(&self.inner, &session, "cancelled", "");
        true
    }

    pub fn forget(&self, id: &str) {
        let removed = {
            let mut reg = self.inner.reg.lock();
            let removed = reg.sessions.remove(id);
            if reg.active_id.as_deref() == Some(id) {
                reg.active_id = None;
            }
            removed
        };
        if let Some(s) = removed {
            s.state.lock().token.clear();
        }
    }

    // ---- Parsing ----

    /// First `https?://[^\s"']+` match with trailing `).,;"'` stripped.
    ///
    /// rclone's `authorize` stderr prints the redirect root inside a NOTICE
    /// (`"http://127.0.0.1:53682/"`) before the consent link the user must
    /// open (`http://127.0.0.1:53682/auth?state=…`). When both are present,
    /// the consent link wins; otherwise the first URL wins, matching Swift.
    pub fn extract_url(text: &str) -> Option<String> {
        let mut first: Option<String> = None;
        let mut from = 0;
        while from < text.len() {
            let rest = &text[from..];
            let pos = match (rest.find("http://"), rest.find("https://")) {
                (Some(a), Some(b)) => a.min(b),
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => break,
            };
            let start = from + pos;
            let after = if text[start..].starts_with("https://") {
                start + 8
            } else {
                start + 7
            };
            let end = text[after..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .map(|i| after + i)
                .unwrap_or(text.len());
            if end > after {
                let url = text[start..end].trim_end_matches([')', '.', ',', ';', '"', '\'']);
                if !url.is_empty() {
                    if is_consent_url(url) {
                        return Some(url.to_string());
                    }
                    if first.is_none() {
                        first = Some(url.to_string());
                    }
                }
            }
            from = if end > start { end } else { start + 1 };
        }
        first
    }

    pub fn extract_token(text: &str) -> String {
        const START: &str = "Paste the following into your remote machine --->";
        const END: &str = "<---End paste";
        let Some(s) = text.find(START) else {
            return String::new();
        };
        let body = &text[s + START.len()..];
        let Some(e) = body.find(END) else {
            return String::new();
        };
        body[..e].trim().to_string()
    }
}

/// rclone consent link (`…/auth?state=`), not the quoted redirect-root notice.
fn is_consent_url(url: &str) -> bool {
    url.contains("/auth?") || url.ends_with("/auth")
}

fn consume(session: &OAuthSession, kind: StreamKind, text: &str) {
    let mut st = session.state.lock();
    match kind {
        StreamKind::Stderr => {
            st.stderr_buffer.push_str(text);
            if let Some(url) = OAuthManager::extract_url(&st.stderr_buffer) {
                let upgrade = st
                    .auth_url
                    .as_deref()
                    .is_none_or(|cur| !is_consent_url(cur) && is_consent_url(&url));
                if upgrade {
                    st.auth_url = Some(url);
                }
            }
        }
        StreamKind::Stdout => st.stdout_buffer.push_str(text),
    }
    session.cv.notify_all();
}

fn process_exited(inner: &Arc<Inner>, session: &Arc<OAuthSession>, code: i32) {
    let (stdout, stderr, already) = {
        let st = session.state.lock();
        (
            st.stdout_buffer.clone(),
            st.stderr_buffer.clone(),
            st.finished,
        )
    };
    if !already {
        let mut token = OAuthManager::extract_token(&stdout);
        if token.is_empty() {
            token = OAuthManager::extract_token(&stderr);
        }
        if code == 0 && !token.is_empty() {
            {
                let mut st = session.state.lock();
                st.token = token;
                st.status = "authorized".into();
                st.finished = true;
            }
            session.cv.notify_all();
            (inner.push)(
                "remotes:oauth",
                json!({"sessionID": session.id, "status": "authorized"}),
            );
        } else {
            let msg = stderr_field(if stderr.is_empty() { &stdout } else { &stderr });
            let was_cancelled = session.snapshot().status == "cancelled";
            finish(
                inner,
                session,
                if was_cancelled { "cancelled" } else { "failed" },
                &if was_cancelled {
                    String::new()
                } else if msg.is_empty() {
                    format!("rclone authorize exited {code}")
                } else {
                    msg
                },
            );
        }
    }
    session.child.lock().take();
    let mut reg = inner.reg.lock();
    if reg.active_id.as_deref() == Some(session.id.as_str()) {
        reg.active_id = None;
    }
}

fn finish(inner: &Arc<Inner>, session: &Arc<OAuthSession>, status: &str, error: &str) {
    {
        let mut st = session.state.lock();
        if st.finished {
            return;
        }
        st.finished = true;
        st.status = status.to_string();
        st.error_message = error.to_string();
        if status != "authorized" {
            st.token.clear();
        }
    }
    session.cv.notify_all();
    let mut data = json!({"sessionID": session.id, "status": status});
    if !error.is_empty() {
        data["error"] = Value::String(error.to_string());
    }
    (inner.push)("remotes:oauth", data);
    let mut reg = inner.reg.lock();
    if reg.active_id.as_deref() == Some(session.id.as_str()) {
        reg.active_id = None;
    }
}

fn schedule_expiry(weak: Weak<Inner>, session: Arc<OAuthSession>, lifetime: Duration) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + lifetime;
        {
            let mut st = session.state.lock();
            while !st.finished {
                if session.cv.wait_until(&mut st, deadline).timed_out() {
                    break;
                }
            }
        }
        let Some(inner) = weak.upgrade() else { return };
        if session.snapshot().status != "waiting" {
            return;
        }
        // Finish first. The test spawner (and a fast SIGTERM) delivers the
        // exit synchronously; `process_exited` would otherwise record a
        // generic failure and `finish` would refuse to overwrite it.
        finish(&inner, &session, "cancelled", "OAuth session expired");
        session.terminate_child();
        inner
            .log
            .info("oauth: session expired", &[("session", &session.id)]);
    });
}

/// Scriptable fake spawner for tests (never launches a process).
#[cfg(any(test, feature = "test-util"))]
pub mod testing {
    use super::*;

    /// What the fake does when spawned.
    #[derive(Default, Clone)]
    pub struct Script {
        /// Chunks delivered immediately inside `spawn`.
        pub initial: Vec<(StreamKind, String)>,
        /// Exit code delivered immediately inside `spawn` (after the chunks).
        pub exit: Option<i32>,
        /// Fail `spawn` with this message.
        pub spawn_error: Option<String>,
    }

    struct FakeChild {
        id: usize,
        spawner: Arc<FakeInner>,
    }

    impl OAuthChild for FakeChild {
        fn terminate(&self) {
            self.spawner.terminated.lock().push(self.id);
            // Like SIGTERM: the process exits non-zero.
            self.spawner.exit_child(self.id, 143);
        }
    }

    #[derive(Default)]
    struct FakeInner {
        scripts: Mutex<Vec<Script>>,
        calls: Mutex<Vec<(String, Vec<String>)>>,
        live: Mutex<Vec<(ChunkFn, ExitFn, bool)>>,
        terminated: Mutex<Vec<usize>>,
    }

    impl FakeInner {
        fn exit_child(&self, id: usize, code: i32) {
            let cb = {
                let mut live = self.live.lock();
                match live.get_mut(id) {
                    Some(e) if !e.2 => {
                        e.2 = true;
                        Some(e.1.clone())
                    }
                    _ => None,
                }
            };
            if let Some(cb) = cb {
                cb(code);
            }
        }
    }

    #[derive(Clone, Default)]
    pub struct FakeSpawner {
        inner: Arc<FakeInner>,
    }

    impl FakeSpawner {
        pub fn new() -> FakeSpawner {
            FakeSpawner::default()
        }
        /// Queue a script for the next spawn (default script: do nothing).
        pub fn push_script(&self, s: Script) {
            self.inner.scripts.lock().push(s);
        }
        pub fn calls(&self) -> Vec<(String, Vec<String>)> {
            self.inner.calls.lock().clone()
        }
        pub fn terminate_count(&self) -> usize {
            self.inner.terminated.lock().len()
        }
        /// Deliver output on the latest child.
        pub fn emit(&self, kind: StreamKind, text: &str) {
            let cb = self.inner.live.lock().last().map(|e| e.0.clone());
            if let Some(cb) = cb {
                cb(kind, text);
            }
        }
        /// Make the latest child exit.
        pub fn exit(&self, code: i32) {
            let n = self.inner.live.lock().len();
            if n > 0 {
                self.inner.exit_child(n - 1, code);
            }
        }
    }

    impl OAuthSpawner for FakeSpawner {
        fn spawn(
            &self,
            binary: &str,
            args: &[String],
            on_chunk: ChunkFn,
            on_exit: ExitFn,
        ) -> std::io::Result<Arc<dyn OAuthChild>> {
            self.inner
                .calls
                .lock()
                .push((binary.to_string(), args.to_vec()));
            let script = {
                let mut s = self.inner.scripts.lock();
                if s.is_empty() {
                    Script::default()
                } else {
                    s.remove(0)
                }
            };
            if let Some(e) = script.spawn_error {
                return Err(std::io::Error::other(e));
            }
            let id = {
                let mut live = self.inner.live.lock();
                live.push((on_chunk.clone(), on_exit, false));
                live.len() - 1
            };
            for (k, t) in &script.initial {
                on_chunk(*k, t);
            }
            if let Some(code) = script.exit {
                self.inner.exit_child(id, code);
            }
            Ok(Arc::new(FakeChild {
                id,
                spawner: self.inner.clone(),
            }))
        }
    }

    pub const TOKEN_OUTPUT: &str = "Paste the following into your remote machine --->\n{\"access_token\":\"AT\",\"expiry\":\"2030-01-01T00:00:00Z\"}\n<---End paste\n";
    pub const URL_OUTPUT: &str = "2024/01/01 NOTICE: Make sure your Redirect URL is set to \"http://127.0.0.1:53682/\" in your custom config.\nIf your browser doesn't open automatically go to the following link: http://127.0.0.1:53682/auth?state=abc\nLog in and authorize rclone for access\nWaiting for code...\n";
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    type PushedEvents = Arc<Mutex<Vec<(String, Value)>>>;

    fn manager(sp: &FakeSpawner) -> (OAuthManager, PushedEvents) {
        manager_timing(sp, Duration::from_secs(600), Duration::from_secs(2))
    }

    fn manager_timing(
        sp: &FakeSpawner,
        lifetime: Duration,
        url_wait: Duration,
    ) -> (OAuthManager, PushedEvents) {
        let pushed: PushedEvents = Arc::new(Mutex::new(Vec::new()));
        let p = pushed.clone();
        let m = OAuthManager::with_timing(
            "/fake/rclone",
            "/tmp/fake.conf",
            Logger::quiet(),
            Arc::new(sp.clone()),
            Arc::new(move |n, d| p.lock().push((n.to_string(), d))),
            lifetime,
            url_wait,
        );
        (m, pushed)
    }

    fn url_script() -> Script {
        Script {
            initial: vec![(StreamKind::Stderr, URL_OUTPUT.to_string())],
            ..Default::default()
        }
    }

    fn poll(cond: impl Fn() -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs(2);
        while Instant::now() < end {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        cond()
    }

    #[test]
    fn extract_url_variants() {
        assert_eq!(
            OAuthManager::extract_url("go to http://127.0.0.1:53682/auth?state=abc\nLog in"),
            Some("http://127.0.0.1:53682/auth?state=abc".into())
        );
        assert_eq!(
            OAuthManager::extract_url("see (https://example.com/x)."),
            Some("https://example.com/x".into())
        );
        assert_eq!(
            OAuthManager::extract_url("\"https://example.com/a,\" more"),
            Some("https://example.com/a".into())
        );
        assert_eq!(OAuthManager::extract_url("no link here"), None);
        assert_eq!(
            OAuthManager::extract_url("bad https:// then https://ok.io/p"),
            Some("https://ok.io/p".into())
        );
        // First URL wins even when http:// appears before https://.
        assert_eq!(
            OAuthManager::extract_url("http://a.io/1 https://b.io/2"),
            Some("http://a.io/1".into())
        );
        // The quoted redirect-root notice is not the link the user opens.
        assert_eq!(
            OAuthManager::extract_url(URL_OUTPUT),
            Some("http://127.0.0.1:53682/auth?state=abc".into())
        );
    }

    #[test]
    fn extract_token_variants() {
        assert_eq!(
            OAuthManager::extract_token(TOKEN_OUTPUT),
            "{\"access_token\":\"AT\",\"expiry\":\"2030-01-01T00:00:00Z\"}"
        );
        assert_eq!(
            OAuthManager::extract_token("Paste the following into your remote machine ---> abc"),
            ""
        );
        assert_eq!(OAuthManager::extract_token("nothing"), "");
    }

    #[test]
    fn start_returns_url_and_args() {
        let sp = FakeSpawner::new();
        sp.push_script(url_script());
        let (m, pushed) = manager(&sp);
        let s = m.start("drive", Some("cid"), Some("sec")).unwrap();
        assert_eq!(s.id, "O1");
        assert_eq!(
            s.auth_url().as_deref(),
            Some("http://127.0.0.1:53682/auth?state=abc")
        );
        assert!(m.has_active_session());
        let calls = sp.calls();
        assert_eq!(calls[0].0, "/fake/rclone");
        assert_eq!(
            calls[0].1,
            [
                "authorize",
                "drive",
                "cid",
                "sec",
                "--auth-no-open-browser",
                "--config",
                "/tmp/fake.conf"
            ]
        );
        let p = pushed.lock();
        assert_eq!(p[0].0, "remotes:oauth");
        assert_eq!(p[0].1["status"], "waiting");
        assert_eq!(p[0].1["authURL"], "http://127.0.0.1:53682/auth?state=abc");
    }

    #[test]
    fn second_start_while_active_is_rejected() {
        let sp = FakeSpawner::new();
        sp.push_script(url_script());
        let (m, _) = manager(&sp);
        m.start("drive", None, None).unwrap();
        let e = m.start("onedrive", None, None).map(|_| ()).unwrap_err();
        assert_eq!(e.code, "oauth_session_active");
        assert!(e.message.contains("session O1"));
    }

    #[test]
    fn authorized_flow() {
        let sp = FakeSpawner::new();
        sp.push_script(url_script());
        let (m, pushed) = manager(&sp);
        let s = m.start("drive", None, None).unwrap();
        sp.emit(StreamKind::Stdout, TOKEN_OUTPUT);
        sp.exit(0);
        let snap = s.snapshot();
        assert_eq!(snap.status, "authorized");
        assert!(snap.token.contains("access_token"));
        assert!(!m.has_active_session());
        assert_eq!(pushed.lock().last().unwrap().1["status"], "authorized");
        // Token is dropped on forget.
        m.forget("O1");
        assert!(m.session("O1").is_none());
        assert_eq!(s.snapshot().token, "");
    }

    #[test]
    fn token_on_stderr_is_accepted() {
        let sp = FakeSpawner::new();
        sp.push_script(url_script());
        let (m, _) = manager(&sp);
        let s = m.start("drive", None, None).unwrap();
        sp.emit(StreamKind::Stderr, TOKEN_OUTPUT);
        sp.exit(0);
        assert_eq!(s.snapshot().status, "authorized");
    }

    #[test]
    fn failed_exit_sets_failed_with_redacted_message() {
        let sp = FakeSpawner::new();
        sp.push_script(url_script());
        let (m, pushed) = manager(&sp);
        let s = m.start("drive", None, None).unwrap();
        sp.emit(
            StreamKind::Stderr,
            "Failed to authorize: token=SECRETVALUE bad\n",
        );
        sp.exit(1);
        let snap = s.snapshot();
        assert_eq!(snap.status, "failed");
        assert!(!snap.error.contains("SECRETVALUE"), "{}", snap.error);
        assert_eq!(snap.token, "");
        assert!(!m.has_active_session());
        let last = pushed.lock().last().unwrap().1.clone();
        assert_eq!(last["status"], "failed");
        assert!(last["error"].is_string());
    }

    #[test]
    fn exit_zero_without_token_fails_with_exit_code_message() {
        let sp = FakeSpawner::new();
        sp.push_script(Script {
            initial: vec![(StreamKind::Stderr, "http://x.io/a\n".into())],
            ..Default::default()
        });
        let (m, _) = manager(&sp);
        let s = m.start("drive", None, None).unwrap();
        // stderr non-empty → message is the redacted stderr.
        sp.exit(0);
        let snap = s.snapshot();
        assert_eq!(snap.status, "failed");
        assert!(snap.error.contains("http://x.io/a"));
    }

    #[test]
    fn cancel_terminates_and_marks_cancelled() {
        let sp = FakeSpawner::new();
        sp.push_script(url_script());
        let (m, pushed) = manager(&sp);
        let s = m.start("drive", None, None).unwrap();
        assert!(m.cancel("O1"));
        assert_eq!(s.snapshot().status, "cancelled");
        assert_eq!(sp.terminate_count(), 1);
        assert!(!m.has_active_session());
        assert_eq!(pushed.lock().last().unwrap().1["status"], "cancelled");
        // Cancelling again (finished) is a no-op; unknown ids too.
        assert!(!m.cancel("O1"));
        assert!(!m.cancel("nope"));
        // Subsequent exit does not overwrite.
        assert_eq!(s.snapshot().status, "cancelled");
        assert_eq!(s.snapshot().error, "");
    }

    #[test]
    fn expiry_cancels_waiting_session() {
        let sp = FakeSpawner::new();
        sp.push_script(url_script());
        let (m, pushed) = manager_timing(&sp, Duration::from_millis(40), Duration::from_secs(2));
        let s = m.start("drive", None, None).unwrap();
        assert!(poll(|| s.snapshot().status == "cancelled"));
        assert_eq!(s.snapshot().error, "OAuth session expired");
        assert_eq!(sp.terminate_count(), 1);
        assert!(poll(|| !m.has_active_session()));
        let p = pushed.lock();
        let last = p.last().unwrap();
        assert_eq!(last.1["status"], "cancelled");
        assert_eq!(last.1["error"], "OAuth session expired");
    }

    #[test]
    fn expiry_does_not_touch_authorized_session() {
        let sp = FakeSpawner::new();
        sp.push_script(url_script());
        let (m, _) = manager_timing(&sp, Duration::from_millis(40), Duration::from_secs(2));
        let s = m.start("drive", None, None).unwrap();
        sp.emit(StreamKind::Stdout, TOKEN_OUTPUT);
        sp.exit(0);
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(s.snapshot().status, "authorized");
        assert_eq!(sp.terminate_count(), 0);
    }

    #[test]
    fn spawn_error_reports_oauth_failed() {
        let sp = FakeSpawner::new();
        sp.push_script(Script {
            spawn_error: Some("boom".into()),
            ..Default::default()
        });
        let (m, pushed) = manager(&sp);
        let e = m.start("drive", None, None).map(|_| ()).unwrap_err();
        assert_eq!(e.code, "oauth_failed");
        assert_eq!(e.message, "could not start rclone authorize");
        assert!(!m.has_active_session());
        let last = pushed.lock().last().unwrap().1.clone();
        assert_eq!(last["status"], "failed");
        assert!(last["error"].as_str().unwrap().contains("boom"));
    }

    #[test]
    fn early_exit_without_url_is_oauth_failed_and_forgotten() {
        let sp = FakeSpawner::new();
        sp.push_script(Script {
            initial: vec![(StreamKind::Stderr, "Error: no browser\n".into())],
            exit: Some(2),
            ..Default::default()
        });
        let (m, _) = manager(&sp);
        let e = m.start("drive", None, None).map(|_| ()).unwrap_err();
        assert_eq!(e.code, "oauth_failed");
        assert!(e.message.contains("no browser"));
        assert!(m.session("O1").is_none());
        assert!(!m.has_active_session());
    }

    #[test]
    fn no_url_in_time_returns_session_without_url() {
        let sp = FakeSpawner::new();
        let (m, _) = manager_timing(&sp, Duration::from_secs(600), Duration::from_millis(30));
        let s = m.start("drive", None, None).unwrap();
        assert!(s.auth_url().is_none());
        assert_eq!(s.snapshot().status, "waiting");
        // The URL shows up later.
        sp.emit(StreamKind::Stderr, "open http://late.io/x\n");
        assert_eq!(s.auth_url().as_deref(), Some("http://late.io/x"));
        assert!(m.cancel("O1"));
    }

    #[test]
    fn new_session_allowed_after_finish_with_incrementing_ids() {
        let sp = FakeSpawner::new();
        sp.push_script(url_script());
        sp.push_script(url_script());
        let (m, _) = manager(&sp);
        m.start("drive", None, None).unwrap();
        assert!(m.cancel("O1"));
        let s2 = m.start("drive", None, None).unwrap();
        assert_eq!(s2.id, "O2");
    }

    #[test]
    fn real_spawner_runs_and_reports_exit() {
        // Harmless local process (no rclone, no network).
        let out: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        let code: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));
        let o = out.clone();
        let c = code.clone();
        RealOAuthSpawner
            .spawn(
                "/bin/echo",
                &["hello".to_string()],
                Arc::new(move |k, t| {
                    if k == StreamKind::Stdout {
                        o.lock().push_str(t)
                    }
                }),
                Arc::new(move |x| *c.lock() = Some(x)),
            )
            .unwrap();
        assert!(poll(|| code.lock().is_some()));
        assert_eq!(*code.lock(), Some(0));
        assert_eq!(out.lock().trim(), "hello");
    }

    #[test]
    fn real_spawner_terminate_kills_child() {
        let code: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));
        let c = code.clone();
        let child = RealOAuthSpawner
            .spawn(
                "/bin/sleep",
                &["30".to_string()],
                Arc::new(|_, _| {}),
                Arc::new(move |x| *c.lock() = Some(x)),
            )
            .unwrap();
        child.terminate();
        assert!(poll(|| code.lock().is_some()));
        assert_ne!(*code.lock(), Some(0));
    }

    #[test]
    fn real_spawner_missing_binary_errors() {
        assert!(
            RealOAuthSpawner
                .spawn(
                    "/nonexistent/rclone-xyz",
                    &[],
                    Arc::new(|_, _| {}),
                    Arc::new(|_| {})
                )
                .is_err()
        );
    }
}
