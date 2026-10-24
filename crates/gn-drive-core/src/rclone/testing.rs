//! Test doubles for the rclone seam.
//!
//! [`FakeRunner`] implements [`RcloneRunner`] without spawning anything. Enable
//! from other crates with the `test-util` cargo feature.
//!
//! ```ignore
//! let fake = Arc::new(FakeRunner::new());
//! fake.on(&["listremotes"], Response::new().stdout("a:").stdout("b:"));
//! fake.on(&["copy"], Response::new().stderr(r#"{"level":"info","msg":"Copied","object":"x"}"#).exit(0));
//! let client = RcloneClient::with_runner(fake.clone(), "/tmp/c.conf", &Logger::quiet());
//! ```
//!
//! Matching: the best-matching script wins — longest matcher first, then the
//! most recently registered. Prefix matchers (`on`) must equal the first N
//! arguments; contains matchers (`on_contains`) need every token somewhere in
//! the argument list. `*_once` scripts are consumed by their first use.
//! Unmatched invocations return an empty successful response (exit 0).

use super::{CancellationToken, ProcessOutput, RcloneRunner, StreamKind};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Canned outcome of one invocation.
#[derive(Clone, Default)]
pub struct Response {
    /// Output lines in emission order (stream, text without trailing newline).
    pub lines: Vec<(StreamKind, String)>,
    pub exit_code: i32,
    /// When set, spawning fails with this message (`io::ErrorKind::NotFound`).
    pub spawn_error: Option<String>,
    /// After emitting all lines, block until the token is cancelled
    /// (safety timeout 5s), then exit with `-15` like a SIGTERM'd child.
    pub hang_until_cancelled: bool,
}

impl Response {
    pub fn new() -> Self {
        Self::default()
    }
    /// Append a stdout line.
    pub fn stdout(mut self, line: impl Into<String>) -> Self {
        self.lines.push((StreamKind::Stdout, line.into()));
        self
    }
    /// Append a stderr line.
    pub fn stderr(mut self, line: impl Into<String>) -> Self {
        self.lines.push((StreamKind::Stderr, line.into()));
        self
    }
    pub fn exit(mut self, code: i32) -> Self {
        self.exit_code = code;
        self
    }
    pub fn spawn_error(mut self, msg: impl Into<String>) -> Self {
        self.spawn_error = Some(msg.into());
        self
    }
    pub fn hang_until_cancelled(mut self) -> Self {
        self.hang_until_cancelled = true;
        self
    }
    fn joined(&self, kind: StreamKind) -> String {
        let mut out = String::new();
        for (k, l) in &self.lines {
            if *k == kind {
                out.push_str(l);
                out.push('\n');
            }
        }
        out
    }
}

#[derive(Clone)]
enum Matcher {
    Prefix(Vec<String>),
    Contains(Vec<String>),
}

impl Matcher {
    fn score(&self, args: &[String]) -> Option<usize> {
        match self {
            Matcher::Prefix(p) => {
                (args.len() >= p.len() && args[..p.len()] == p[..]).then_some(p.len())
            }
            Matcher::Contains(c) => c.iter().all(|t| args.contains(t)).then_some(c.len()),
        }
    }
}

struct Script {
    matcher: Matcher,
    response: Response,
    once: bool,
    seq: usize,
}

/// Hook run before each streamed line: `(line_index, stream, text)`. It may
/// sleep (delay) or cancel a token.
pub type LineHook = Arc<dyn Fn(usize, StreamKind, &str) + Send + Sync>;

pub struct FakeRunner {
    binary: String,
    scripts: Mutex<Vec<Script>>,
    calls: Mutex<Vec<Vec<String>>>,
    hook: Mutex<Option<LineHook>>,
    seq: Mutex<usize>,
}

impl Default for FakeRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeRunner {
    pub fn new() -> Self {
        Self::with_binary("/fake/rclone")
    }

    pub fn with_binary(binary: &str) -> Self {
        FakeRunner {
            binary: binary.to_string(),
            scripts: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
            hook: Mutex::new(None),
            seq: Mutex::new(0),
        }
    }

    fn add(&self, matcher: Matcher, response: Response, once: bool) -> &Self {
        let mut seq = self.seq.lock();
        *seq += 1;
        self.scripts.lock().push(Script {
            matcher,
            response,
            once,
            seq: *seq,
        });
        self
    }

    fn owned(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    /// Respond to invocations whose arguments start with `prefix`.
    pub fn on(&self, prefix: &[&str], response: Response) -> &Self {
        self.add(Matcher::Prefix(Self::owned(prefix)), response, false)
    }
    /// Like [`on`](Self::on) but consumed by the first matching call.
    pub fn on_once(&self, prefix: &[&str], response: Response) -> &Self {
        self.add(Matcher::Prefix(Self::owned(prefix)), response, true)
    }
    /// Respond to invocations containing every token in `tokens` (any position).
    pub fn on_contains(&self, tokens: &[&str], response: Response) -> &Self {
        self.add(Matcher::Contains(Self::owned(tokens)), response, false)
    }
    pub fn on_contains_once(&self, tokens: &[&str], response: Response) -> &Self {
        self.add(Matcher::Contains(Self::owned(tokens)), response, true)
    }

    /// Install a per-line hook for streaming calls (delay / cancel injection).
    pub fn set_line_hook(&self, hook: impl Fn(usize, StreamKind, &str) + Send + Sync + 'static) {
        *self.hook.lock() = Some(Arc::new(hook));
    }

    /// Every invocation so far (`run` and `run_streaming`), in order.
    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().clone()
    }
    pub fn call_count(&self) -> usize {
        self.calls.lock().len()
    }
    pub fn last_call(&self) -> Option<Vec<String>> {
        self.calls.lock().last().cloned()
    }
    /// Invocations whose first argument (after any `--log-level X`) is `cmd`.
    pub fn calls_for(&self, cmd: &str) -> Vec<Vec<String>> {
        self.calls()
            .into_iter()
            .filter(|a| a.iter().any(|x| x == cmd))
            .collect()
    }

    fn respond(&self, args: &[String]) -> Response {
        self.calls.lock().push(args.to_vec());
        let mut scripts = self.scripts.lock();
        let best = scripts
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.matcher.score(args).map(|sc| (sc, s.seq, i)))
            .max();
        match best {
            Some((_, _, i)) => {
                let resp = scripts[i].response.clone();
                if scripts[i].once {
                    scripts.remove(i);
                }
                resp
            }
            None => Response::default(),
        }
    }
}

fn spawn_err(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, msg.to_string())
}

impl RcloneRunner for FakeRunner {
    fn binary(&self) -> String {
        self.binary.clone()
    }

    fn run(&self, args: &[String]) -> std::io::Result<ProcessOutput> {
        let r = self.respond(args);
        if let Some(m) = &r.spawn_error {
            return Err(spawn_err(m));
        }
        Ok(ProcessOutput {
            stdout: r.joined(StreamKind::Stdout),
            stderr: r.joined(StreamKind::Stderr),
            exit_code: r.exit_code,
        })
    }

    fn run_streaming(
        &self,
        args: &[String],
        token: Option<&CancellationToken>,
        on_line: &(dyn Fn(StreamKind, &str) + Sync),
    ) -> std::io::Result<i32> {
        let r = self.respond(args);
        if let Some(m) = &r.spawn_error {
            return Err(spawn_err(m));
        }
        let cancelled = || token.is_some_and(|t| t.is_cancelled());
        let hook = self.hook.lock().clone();
        for (i, (kind, line)) in r.lines.iter().enumerate() {
            if cancelled() {
                return Ok(-15);
            }
            if let Some(h) = &hook {
                h(i, *kind, line);
                if cancelled() {
                    return Ok(-15);
                }
            }
            on_line(*kind, line);
        }
        if r.hang_until_cancelled {
            let start = Instant::now();
            while !cancelled() && start.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(1));
            }
            return Ok(-15);
        }
        if cancelled() {
            return Ok(-15);
        }
        Ok(r.exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn longest_prefix_then_latest_wins_and_records_calls() {
        let f = FakeRunner::new();
        f.on(&["config"], Response::new().stdout("generic"));
        f.on(&["config", "dump"], Response::new().stdout("dump"));
        assert_eq!(
            f.run(&a(&["config", "dump", "--x"])).unwrap().stdout,
            "dump\n"
        );
        assert_eq!(f.run(&a(&["config", "other"])).unwrap().stdout, "generic\n");
        assert_eq!(f.run(&a(&["nothing"])).unwrap().exit_code, 0);
        assert_eq!(f.call_count(), 3);
        assert_eq!(f.last_call().unwrap(), a(&["nothing"]));
    }

    #[test]
    fn once_and_contains() {
        let f = FakeRunner::new();
        f.on_once(&["x"], Response::new().stdout("first"));
        f.on(&["x"], Response::new().stdout("later").exit(3));
        // The more recent persistent script wins over the older once-script.
        assert_eq!(f.run(&a(&["x"])).unwrap().exit_code, 3);
        let g = FakeRunner::new();
        g.on(&["x"], Response::new().stdout("persistent"));
        g.on_once(&["x"], Response::new().stdout("once"));
        assert_eq!(g.run(&a(&["x"])).unwrap().stdout, "once\n");
        assert_eq!(g.run(&a(&["x"])).unwrap().stdout, "persistent\n");
        g.on_contains(&["--recursive", "lsjson"], Response::new().stdout("ls"));
        assert_eq!(
            g.run(&a(&["--log-level", "ERROR", "lsjson", "--recursive"]))
                .unwrap()
                .stdout,
            "ls\n"
        );
        assert_eq!(g.calls_for("lsjson").len(), 1);
    }

    #[test]
    fn streaming_cancel_and_hang() {
        let f = FakeRunner::new();
        f.on(
            &["s"],
            Response::new().stderr("l1").stdout("l2").stderr("l3"),
        );
        let token = CancellationToken::new();
        let t2 = token.clone();
        f.set_line_hook(move |i, _, _| {
            if i == 1 {
                t2.cancel();
            }
        });
        let seen = Mutex::new(Vec::new());
        let code = f
            .run_streaming(&a(&["s"]), Some(&token), &|k, l| {
                seen.lock().push((k, l.to_string()))
            })
            .unwrap();
        assert_eq!(code, -15);
        assert_eq!(seen.lock().len(), 1);

        let h = FakeRunner::new();
        h.on(&["h"], Response::new().hang_until_cancelled());
        let tok = CancellationToken::new();
        let t3 = tok.clone();
        let th = std::thread::spawn(move || t3.cancel());
        assert_eq!(
            h.run_streaming(&a(&["h"]), Some(&tok), &|_, _| {}).unwrap(),
            -15
        );
        th.join().unwrap();
    }

    #[test]
    fn spawn_error() {
        let f = FakeRunner::new();
        f.on(&["x"], Response::new().spawn_error("nope"));
        assert!(f.run(&a(&["x"])).is_err());
        assert!(f.run_streaming(&a(&["x"]), None, &|_, _| {}).is_err());
    }
}
