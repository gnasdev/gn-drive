//! Structured logging (port of `Logger.swift`). Foreground mode writes text
//! lines to stderr; service mode writes JSON lines. Optional sink feeds the
//! RPC log ring buffer.

use chrono::{DateTime, SecondsFormat, Utc};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogMode {
    Foreground,
    Service,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug = 0,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn label(self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
    pub fn from_label(s: &str) -> Option<LogLevel> {
        match s.to_ascii_lowercase().as_str() {
            "debug" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "warn" | "warning" => Some(LogLevel::Warn),
            "error" => Some(LogLevel::Error),
            _ => None,
        }
    }
}

/// One emitted log line, fed to the optional sink.
#[derive(Debug, Clone)]
pub struct LogRecord {
    pub time: DateTime<Utc>,
    pub level: LogLevel,
    pub msg: String,
    pub fields: BTreeMap<String, String>,
}

pub type LogSink = Arc<dyn Fn(&LogRecord) + Send + Sync>;

struct Shared {
    min_level: LogLevel,
    sink: Option<LogSink>,
    /// Suppress stderr output (tests / embedded use); sink still fires.
    quiet: bool,
}

#[derive(Clone)]
pub struct Logger {
    mode: LogMode,
    fields: Vec<(String, String)>,
    shared: Arc<Mutex<Shared>>,
}

impl Logger {
    pub fn new(mode: LogMode) -> Logger {
        Logger {
            mode,
            fields: Vec::new(),
            shared: Arc::new(Mutex::new(Shared {
                min_level: LogLevel::Debug,
                sink: None,
                quiet: false,
            })),
        }
    }

    /// Logger that never writes to stderr (still feeds the sink).
    pub fn quiet() -> Logger {
        let l = Logger::new(LogMode::Foreground);
        l.shared.lock().quiet = true;
        l
    }

    pub fn mode(&self) -> LogMode {
        self.mode
    }
    pub fn min_level(&self) -> LogLevel {
        self.shared.lock().min_level
    }
    pub fn set_min_level(&self, l: LogLevel) {
        self.shared.lock().min_level = l;
    }
    pub fn set_quiet(&self, q: bool) {
        self.shared.lock().quiet = q;
    }
    pub fn set_sink(&self, sink: Option<LogSink>) {
        self.shared.lock().sink = sink;
    }

    pub fn with(&self, key: &str, value: &str) -> Logger {
        let mut l = self.clone();
        l.fields.push((key.to_string(), value.to_string()));
        l
    }

    pub fn log(&self, level: LogLevel, msg: &str, fields: &[(&str, &str)]) {
        let (min, sink, quiet) = {
            let s = self.shared.lock();
            (s.min_level, s.sink.clone(), s.quiet)
        };
        if level < min {
            return;
        }
        let now = Utc::now();
        let ts = now.to_rfc3339_opts(SecondsFormat::Secs, true);
        let mut all: Vec<(String, String)> = self.fields.clone();
        all.extend(fields.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        if !quiet {
            let line = match self.mode {
                LogMode::Service => {
                    let mut m = serde_json::Map::new();
                    m.insert("time".into(), ts.clone().into());
                    m.insert("level".into(), level.label().into());
                    m.insert("msg".into(), msg.into());
                    for (k, v) in &all {
                        m.insert(k.clone(), v.clone().into());
                    }
                    serde_json::Value::Object(m).to_string()
                }
                LogMode::Foreground => {
                    let mut line = format!("{ts} {} {msg}", level.label());
                    for (k, v) in &all {
                        line.push_str(&format!(" {k}={v}"));
                    }
                    line
                }
            };
            let _ = writeln!(std::io::stderr().lock(), "{line}");
        }
        if let Some(sink) = sink {
            sink(&LogRecord {
                time: now,
                level,
                msg: msg.to_string(),
                fields: all.into_iter().collect(),
            });
        }
    }

    pub fn debug(&self, msg: &str, fields: &[(&str, &str)]) {
        self.log(LogLevel::Debug, msg, fields)
    }
    pub fn info(&self, msg: &str, fields: &[(&str, &str)]) {
        self.log(LogLevel::Info, msg, fields)
    }
    pub fn warn(&self, msg: &str, fields: &[(&str, &str)]) {
        self.log(LogLevel::Warn, msg, fields)
    }
    pub fn error(&self, msg: &str, fields: &[(&str, &str)]) {
        self.log(LogLevel::Error, msg, fields)
    }
    pub fn warn_err(&self, msg: &str, err: &dyn std::fmt::Display, fields: &[(&str, &str)]) {
        let e = err.to_string();
        let mut f = fields.to_vec();
        f.push(("err", &e));
        self.log(LogLevel::Warn, msg, &f)
    }
    pub fn error_err(&self, msg: &str, err: &dyn std::fmt::Display, fields: &[(&str, &str)]) {
        let e = err.to_string();
        let mut f = fields.to_vec();
        f.push(("err", &e));
        self.log(LogLevel::Error, msg, &f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sink_receives_filtered_records_with_context() {
        let l = Logger::quiet().with("service", "gn-drive");
        let got: Arc<Mutex<Vec<LogRecord>>> = Arc::default();
        let g = got.clone();
        l.set_sink(Some(Arc::new(move |r| g.lock().push(r.clone()))));
        l.set_min_level(LogLevel::Info);
        l.debug("hidden", &[]);
        l.warn("shown", &[("k", "v")]);
        let r = got.lock();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].msg, "shown");
        assert_eq!(r[0].fields["service"], "gn-drive");
        assert_eq!(r[0].fields["k"], "v");
    }

    #[test]
    fn level_labels_roundtrip() {
        assert_eq!(LogLevel::from_label("WARNING"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_label("nope"), None);
        assert!(LogLevel::Debug < LogLevel::Error);
    }
}
