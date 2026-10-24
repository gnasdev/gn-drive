//! stdio framing, request workers and shutdown for protocol v1.
//!
//! Port of the transport half of `RPCService.swift`: one reader (the caller
//! of [`RpcService::run_on`]), one serial writer thread, and a bounded pool
//! of request workers. Nothing but protocol frames is written to the sink;
//! diagnostics go through the logger (stderr).

use super::{Dispatcher, Event, Options, RpcError};
use crate::instance_lock::InstanceLocker;
use crossbeam_channel::{Receiver, Sender, unbounded};
use parking_lot::{Condvar, Mutex};
use serde_json::{Map, Value, json};
use std::collections::VecDeque;
use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::{Command, Stdio};
use std::sync::{Arc, Weak};
use std::time::Duration;

/// Maximum accepted request line (without the newline).
pub const MAX_REQUEST_BYTES: usize = 1 << 20;
/// Number of concurrent request workers.
const WORKER_SLOTS: usize = 8;
/// Droppable events are discarded once this many frames are queued.
const DROPPABLE_LIMIT: usize = 512;
/// How long shutdown waits for in-flight requests / the final flush.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(5);

// MARK: - Framing traits

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineRead {
    Line(Vec<u8>),
    TooLong,
    Eof,
}

pub trait LineSource: Send {
    /// Next newline-terminated line (newline stripped), at most `max_bytes`.
    fn next_line(&mut self, max_bytes: usize) -> LineRead;
}

pub trait LineSink: Send + Sync {
    /// Writes `line` plus a newline. Returns false when the peer is gone (EPIPE).
    fn write_line(&self, line: &[u8]) -> bool;
}

// MARK: - stdio implementations

struct FdReader(i32);

impl Read for FdReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            // SAFETY: `buf` is valid for `buf.len()` writable bytes.
            let n = unsafe { libc::read(self.0, buf.as_mut_ptr().cast(), buf.len()) };
            if n >= 0 {
                return Ok(n as usize);
            }
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::Interrupted {
                return Err(err);
            }
        }
    }
}

/// Newline framing over any reader: oversized lines yield
/// [`LineRead::TooLong`] after discarding up to the next newline; a trailing
/// partial line at EOF is dropped (rpc.md §11.3).
pub struct ReaderLineSource<R: Read + Send> {
    reader: R,
    buf: Vec<u8>,
    chunk: Vec<u8>,
}

impl<R: Read + Send> ReaderLineSource<R> {
    const CHUNK: usize = 64 * 1024;

    pub fn new(reader: R) -> Self {
        ReaderLineSource {
            reader,
            buf: Vec::with_capacity(Self::CHUNK),
            chunk: vec![0; Self::CHUNK],
        }
    }
}

impl<R: Read + Send> LineSource for ReaderLineSource<R> {
    fn next_line(&mut self, max_bytes: usize) -> LineRead {
        let mut discarding = false;
        let mut scanned = 0;
        loop {
            if let Some(rel) = self.buf[scanned..].iter().position(|b| *b == b'\n') {
                let idx = scanned + rel;
                let rest = self.buf.split_off(idx + 1);
                self.buf.pop();
                let line = std::mem::replace(&mut self.buf, rest);
                if discarding || line.len() > max_bytes {
                    return LineRead::TooLong;
                }
                return LineRead::Line(line);
            }
            scanned = self.buf.len();
            if self.buf.len() > max_bytes {
                // Oversized line without a terminator yet: drop and discard
                // until the next newline.
                self.buf.clear();
                scanned = 0;
                discarding = true;
            }
            match self.reader.read(&mut self.chunk) {
                Ok(0) => return LineRead::Eof,
                Ok(n) => self.buf.extend_from_slice(&self.chunk[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return LineRead::Eof,
            }
        }
    }
}

/// Reads lines from a file descriptor (default stdin).
pub struct StdioLineSource(ReaderLineSource<FdReader>);

impl StdioLineSource {
    pub fn new() -> Self {
        Self::with_fd(0)
    }
    pub fn with_fd(fd: i32) -> Self {
        StdioLineSource(ReaderLineSource::new(FdReader(fd)))
    }
}

impl Default for StdioLineSource {
    fn default() -> Self {
        Self::new()
    }
}

impl LineSource for StdioLineSource {
    fn next_line(&mut self, max_bytes: usize) -> LineRead {
        self.0.next_line(max_bytes)
    }
}

/// Writes whole lines to a file descriptor (default stdout), serialized.
pub struct StdioLineSink {
    fd: i32,
    lock: Mutex<()>,
}

impl StdioLineSink {
    pub fn new() -> Self {
        Self::with_fd(1)
    }
    pub fn with_fd(fd: i32) -> Self {
        StdioLineSink {
            fd,
            lock: Mutex::new(()),
        }
    }
}

impl Default for StdioLineSink {
    fn default() -> Self {
        Self::new()
    }
}

impl LineSink for StdioLineSink {
    fn write_line(&self, line: &[u8]) -> bool {
        let mut data = Vec::with_capacity(line.len() + 1);
        data.extend_from_slice(line);
        data.push(b'\n');
        let _g = self.lock.lock();
        let mut offset = 0;
        while offset < data.len() {
            // SAFETY: the pointer/length pair stays inside `data`.
            let n = unsafe {
                libc::write(self.fd, data[offset..].as_ptr().cast(), data.len() - offset)
            };
            if n < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return false;
            }
            offset += n as usize;
        }
        true
    }
}

// MARK: - Output queue

#[derive(Default)]
struct OutState {
    queue: VecDeque<String>,
    /// Frames queued or currently being written.
    pending: usize,
    closed: bool,
}

#[derive(Default)]
struct Out {
    st: Mutex<OutState>,
    cv: Condvar,
}

impl Out {
    fn enqueue(&self, frame: String, droppable: bool) {
        let mut st = self.st.lock();
        if st.closed || (droppable && st.pending > DROPPABLE_LIMIT) {
            return;
        }
        st.pending += 1;
        st.queue.push_back(frame);
        self.cv.notify_all();
    }

    /// Final frame: closes the output atomically with the enqueue so nothing
    /// can be written after `app:shutdown`.
    fn enqueue_final(&self, frame: String) {
        let mut st = self.st.lock();
        st.closed = true;
        st.pending += 1;
        st.queue.push_back(frame);
        self.cv.notify_all();
    }

    fn is_closed(&self) -> bool {
        self.st.lock().closed
    }

    /// Wait until everything queued has been handed to the sink.
    fn flush(&self, timeout: Duration) {
        let deadline = std::time::Instant::now() + timeout;
        let mut st = self.st.lock();
        while st.pending > 0 {
            if self.cv.wait_until(&mut st, deadline).timed_out() {
                break;
            }
        }
    }
}

// MARK: - Service

struct Job {
    id: Value,
    method: String,
    params: Value,
}

#[derive(Default)]
struct State {
    stopping: bool,
    did_shutdown: bool,
}

pub struct RpcService {
    dispatcher: Arc<Dispatcher>,
    lock_path: String,
    locker: Option<InstanceLocker>,
    opts: Options,
    out: Arc<Out>,
    state: Mutex<State>,
    in_flight: Mutex<usize>,
    in_flight_cv: Condvar,
    jobs: Mutex<Option<Sender<Job>>>,
    weak: Weak<RpcService>,
}

impl RpcService {
    pub fn new(
        dispatcher: Arc<Dispatcher>,
        lock_path: &str,
        locker: Option<InstanceLocker>,
        opts: Options,
    ) -> Arc<RpcService> {
        Arc::new_cyclic(|weak| RpcService {
            dispatcher,
            lock_path: lock_path.to_string(),
            locker,
            opts,
            out: Arc::new(Out::default()),
            state: Mutex::new(State::default()),
            in_flight: Mutex::new(0),
            in_flight_cv: Condvar::new(),
            jobs: Mutex::new(None),
            weak: weak.clone(),
        })
    }

    // MARK: Lifecycle

    /// Serve over the process stdin/stdout until EOF or shutdown.
    pub fn run(&self) -> i32 {
        // SAFETY: ignoring SIGPIPE so a closed stdout surfaces as EPIPE.
        unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };
        self.run_on(
            Box::new(StdioLineSource::new()),
            Arc::new(StdioLineSink::new()),
        )
    }

    /// Announce `hello`, then read requests until EOF or shutdown.
    pub fn run_on(&self, mut source: Box<dyn LineSource>, sink: Arc<dyn LineSink>) -> i32 {
        self.start(sink);
        while !self.is_stopping() {
            match source.next_line(MAX_REQUEST_BYTES) {
                LineRead::Eof => {
                    self.begin_shutdown("eof");
                    return 0;
                }
                LineRead::TooLong => self.send_error(
                    Value::Null,
                    RpcError::new("invalid_request", "request exceeds 1 MiB"),
                ),
                LineRead::Line(data) => self.handle_line(&data),
            }
        }
        0
    }

    /// Start writer, workers and event forwarding; `hello` is the first frame.
    fn start(&self, sink: Arc<dyn LineSink>) {
        // Serial writer.
        {
            let out = self.out.clone();
            let weak = self.weak.clone();
            std::thread::Builder::new()
                .name("rpc-writer".into())
                .spawn(move || {
                    loop {
                        let frame = {
                            let mut st = out.st.lock();
                            loop {
                                if let Some(f) = st.queue.pop_front() {
                                    break f;
                                }
                                if st.closed {
                                    return;
                                }
                                out.cv.wait(&mut st);
                            }
                        };
                        let ok = sink.write_line(frame.as_bytes());
                        {
                            let mut st = out.st.lock();
                            st.pending -= 1;
                            out.cv.notify_all();
                        }
                        if !ok && let Some(svc) = weak.upgrade() {
                            // Never block the writer on its own flush.
                            std::thread::spawn(move || svc.begin_shutdown("eof"));
                        }
                    }
                })
                .expect("spawn rpc writer");
        }

        // Request workers.
        let (tx, rx) = unbounded::<Job>();
        *self.jobs.lock() = Some(tx);
        for i in 0..WORKER_SLOTS {
            let rx = rx.clone();
            let weak = self.weak.clone();
            std::thread::Builder::new()
                .name(format!("rpc-worker-{i}"))
                .spawn(move || {
                    while let Ok(job) = rx.recv() {
                        let Some(svc) = weak.upgrade() else { return };
                        svc.run_job(job);
                    }
                })
                .expect("spawn rpc worker");
        }

        // Subscribe before `hello` so no event is missed, but forward only
        // after it is queued so `hello` stays the first frame.
        let events = self.dispatcher.subscribe();
        self.enqueue_event(
            "hello",
            self.dispatcher
                .hello_data(&self.lock_path, std::process::id() as i32),
            false,
        );
        let out = self.out.clone();
        std::thread::Builder::new()
            .name("rpc-events".into())
            .spawn(move || forward_events(events, out))
            .expect("spawn rpc event forwarder");
    }

    /// Trigger the shutdown sequence (idempotent). Called on EOF, signals,
    /// `app.quit`, and on a broken output.
    pub fn begin_shutdown(&self, reason: &str) {
        {
            let mut st = self.state.lock();
            if st.did_shutdown {
                return;
            }
            st.did_shutdown = true;
            st.stopping = true;
        }
        // Let in-flight requests finish (their responses precede the final frame).
        {
            let deadline = std::time::Instant::now() + SHUTDOWN_WAIT;
            let mut n = self.in_flight.lock();
            while *n > 0 {
                if self.in_flight_cv.wait_until(&mut n, deadline).timed_out() {
                    break;
                }
            }
        }
        *self.jobs.lock() = None;
        self.dispatcher.shutdown();
        let container = self.dispatcher.container();
        container.sync_engine.stop();
        container.close();
        if let Some(l) = &self.locker {
            l.release();
        }
        self.out
            .enqueue_final(Event::new("app:shutdown", json!({ "reason": reason })).to_frame());
        self.out.flush(SHUTDOWN_WAIT);
        if self.opts.exit_on_shutdown {
            std::process::exit(0);
        }
    }

    fn is_stopping(&self) -> bool {
        self.state.lock().stopping
    }

    // MARK: Requests

    fn handle_line(&self, data: &[u8]) {
        if self.is_stopping() {
            return;
        }
        let Ok(obj) = serde_json::from_slice::<Value>(data) else {
            self.send_error(
                Value::Null,
                RpcError::new("invalid_request", "request is not valid JSON"),
            );
            return;
        };
        let Value::Object(dict) = obj else {
            self.send_error(
                Value::Null,
                RpcError::new("invalid_request", "request must be a JSON object"),
            );
            return;
        };
        let id = match dict.get("id") {
            Some(id) if is_valid_id(id) => id.clone(),
            _ => {
                self.send_error(
                    Value::Null,
                    RpcError::new("invalid_request", "request id must be a number or string"),
                );
                return;
            }
        };
        let method = match dict.get("method") {
            Some(Value::String(m)) if !m.is_empty() => m.clone(),
            _ => {
                self.send_error(
                    id,
                    RpcError::new("invalid_request", "missing required field: method"),
                );
                return;
            }
        };
        let params = match dict.get("params") {
            None | Some(Value::Null) => Value::Object(Map::new()),
            Some(p @ Value::Object(_)) => p.clone(),
            Some(_) => {
                self.send_error(
                    id,
                    RpcError::invalid_params("params", Some("params must be an object")),
                );
                return;
            }
        };

        *self.in_flight.lock() += 1;
        let job = Job { id, method, params };
        let sent = self
            .jobs
            .lock()
            .as_ref()
            .map(|tx| tx.send(job).is_ok())
            .unwrap_or(false);
        if !sent {
            self.leave();
        }
    }

    fn leave(&self) {
        let mut n = self.in_flight.lock();
        *n -= 1;
        self.in_flight_cv.notify_all();
    }

    fn run_job(&self, job: Job) {
        let Job { id, method, params } = job;
        let outcome = catch_unwind(AssertUnwindSafe(|| self.dispatcher.handle(&method, params)));
        match outcome {
            Ok(Ok(result)) => self.send_result(id, result),
            Ok(Err(err)) => self.send_error(id, err),
            Err(_) => self.send_error(
                id,
                RpcError::new("internal_error", "request handler panicked"),
            ),
        }
        if method == "app.quit"
            && let Some(svc) = self.weak.upgrade()
        {
            std::thread::spawn(move || svc.begin_shutdown("request"));
        }
        self.leave();
    }

    // MARK: Framing

    fn send_result(&self, id: Value, result: Value) {
        let mut frame = Map::new();
        frame.insert("id".into(), id);
        frame.insert("result".into(), result);
        self.out.enqueue(Value::Object(frame).to_string(), false);
    }

    fn send_error(&self, id: Value, error: RpcError) {
        let mut payload = Map::new();
        payload.insert("code".into(), Value::String(error.code));
        payload.insert("message".into(), Value::String(error.message));
        if let Some(d) = error.data {
            payload.insert("data".into(), d);
        }
        let mut frame = Map::new();
        frame.insert("id".into(), id);
        frame.insert("error".into(), Value::Object(payload));
        self.out.enqueue(Value::Object(frame).to_string(), false);
    }

    fn enqueue_event(&self, name: &str, data: Value, droppable: bool) {
        self.out
            .enqueue(Event::new(name, data).to_frame(), droppable);
    }

    // MARK: Startup helpers

    /// Write a `fatal` event to stdout (startup failures, rpc.md §2.2).
    pub fn emit_fatal(code: &str, message: &str, pid: Option<i32>, holder: Option<&str>) {
        let mut data = Map::new();
        data.insert("code".into(), code.into());
        data.insert("message".into(), message.into());
        if let Some(pid) = pid {
            data.insert("pid".into(), pid.into());
        }
        if let Some(h) = holder {
            data.insert("holder".into(), h.into());
        }
        let frame = Event::new("fatal", Value::Object(data)).to_frame();
        let _ = StdioLineSink::new().write_line(frame.as_bytes());
    }

    /// `/usr/bin/pgrep -x GNDriveApp` — the window app does not take the lock
    /// on its own yet (rpc.md §2.2).
    pub fn is_app_running() -> bool {
        Command::new("/usr/bin/pgrep")
            .args(["-x", "GNDriveApp"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Classify the lock holder for `fatal.data.holder`.
    pub fn classify_holder(pid: i32) -> String {
        let Ok(output) = Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "command="])
            .stderr(Stdio::null())
            .output()
        else {
            return "cli".into();
        };
        let command = String::from_utf8_lossy(&output.stdout);
        if command.contains("--service") {
            "service".into()
        } else if command.contains("serve") {
            "window".into()
        } else {
            "cli".into()
        }
    }
}

fn is_valid_id(id: &Value) -> bool {
    matches!(id, Value::String(_) | Value::Number(_))
}

fn forward_events(events: Receiver<Event>, out: Arc<Out>) {
    loop {
        match events.recv_timeout(Duration::from_millis(100)) {
            Ok(ev) => out.enqueue(ev.to_frame(), ev.name == "log:line"),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if out.is_closed() {
                    return;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn lines(data: &[u8], max: usize) -> Vec<LineRead> {
        let mut src = ReaderLineSource::new(Cursor::new(data.to_vec()));
        let mut v = Vec::new();
        loop {
            let r = src.next_line(max);
            let eof = r == LineRead::Eof;
            v.push(r);
            if eof {
                return v;
            }
        }
    }

    #[test]
    fn framer_splits_lines_and_drops_partial_at_eof() {
        let got = lines(b"{\"a\":1}\n\nsecond\npartial", 100);
        assert_eq!(
            got,
            vec![
                LineRead::Line(b"{\"a\":1}".to_vec()),
                LineRead::Line(vec![]),
                LineRead::Line(b"second".to_vec()),
                LineRead::Eof
            ]
        );
    }

    #[test]
    fn framer_reports_too_long_and_resyncs_at_next_newline() {
        // Terminated oversized line.
        let got = lines(b"0123456789\nok\n", 5);
        assert_eq!(
            got,
            vec![
                LineRead::TooLong,
                LineRead::Line(b"ok".to_vec()),
                LineRead::Eof
            ]
        );
        // Oversized line larger than a read chunk, spanning several reads.
        let mut big = vec![b'x'; 3 * ReaderLineSource::<Cursor<Vec<u8>>>::CHUNK];
        big.extend_from_slice(b"\nnext\n");
        let got = lines(&big, 1000);
        assert_eq!(
            got,
            vec![
                LineRead::TooLong,
                LineRead::Line(b"next".to_vec()),
                LineRead::Eof
            ]
        );
        // Oversized and never terminated: EOF only.
        let got = lines(&vec![b'y'; 5000], 1000);
        assert_eq!(got, vec![LineRead::Eof]);
    }

    #[test]
    fn framer_accepts_exact_max() {
        let got = lines(b"12345\n", 5);
        assert_eq!(got[0], LineRead::Line(b"12345".to_vec()));
    }

    #[test]
    fn fd_sink_and_source_roundtrip_over_pipe() {
        let mut fds = [0i32; 2];
        // SAFETY: valid 2-int array.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let sink = StdioLineSink::with_fd(fds[1]);
        assert!(sink.write_line(b"hello"));
        assert!(sink.write_line(b"world"));
        let mut src = StdioLineSource::with_fd(fds[0]);
        assert_eq!(src.next_line(100), LineRead::Line(b"hello".to_vec()));
        assert_eq!(src.next_line(100), LineRead::Line(b"world".to_vec()));
        // SAFETY: closing fds we own; SIGPIPE ignored for the EPIPE check.
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_IGN);
            libc::close(fds[0]);
        }
        assert!(!sink.write_line(b"gone"), "EPIPE reports false");
        unsafe { libc::close(fds[1]) };
    }

    #[test]
    fn id_validation() {
        assert!(is_valid_id(&json!(1)));
        assert!(is_valid_id(&json!("a")));
        assert!(is_valid_id(&json!(1.5)));
        assert!(!is_valid_id(&json!(true)));
        assert!(!is_valid_id(&Value::Null));
        assert!(!is_valid_id(&json!({})));
    }

    #[test]
    fn out_queue_drops_droppable_past_limit() {
        let out = Out::default();
        for i in 0..(DROPPABLE_LIMIT * 2) {
            out.enqueue(format!("d{i}"), true);
        }
        assert_eq!(out.st.lock().pending, DROPPABLE_LIMIT + 1);
        out.enqueue("keep".into(), false);
        assert_eq!(out.st.lock().queue.back().map(String::as_str), Some("keep"));
        out.enqueue_final("last".into());
        out.enqueue("after".into(), false);
        assert_eq!(out.st.lock().queue.back().map(String::as_str), Some("last"));
    }
}
