//! In-memory ring buffer fed by the logger sink for `logs.read` / `log:line`
//! (port of `RPCLogBuffer.swift`).

use super::json;
use crate::logger::{LogLevel, LogRecord, LogSink};
use parking_lot::Mutex;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Line {
    pub time: chrono::DateTime<chrono::Utc>,
    pub level: LogLevel,
    pub msg: String,
    pub fields: BTreeMap<String, String>,
}

type OnLine = Arc<dyn Fn(&LogRecord) + Send + Sync>;

struct Inner {
    lines: VecDeque<Line>,
    dropped: usize,
}

pub struct LogBuffer {
    capacity: usize,
    inner: Mutex<Inner>,
    /// Called for every accepted line, outside the buffer lock.
    on_line: Mutex<Option<OnLine>>,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> LogBuffer {
        let capacity = capacity.max(1);
        LogBuffer {
            capacity,
            inner: Mutex::new(Inner {
                lines: VecDeque::with_capacity(capacity.min(4096)),
                dropped: 0,
            }),
            on_line: Mutex::new(None),
        }
    }

    pub fn set_on_line(&self, f: Option<OnLine>) {
        *self.on_line.lock() = f;
    }

    pub fn push(&self, record: LogRecord) {
        {
            let mut g = self.inner.lock();
            g.lines.push_back(Line {
                time: record.time,
                level: record.level,
                msg: record.msg.clone(),
                fields: record.fields.clone(),
            });
            while g.lines.len() > self.capacity {
                g.lines.pop_front();
                g.dropped += 1;
            }
        }
        let cb = self.on_line.lock().clone();
        if let Some(cb) = cb {
            cb(&record);
        }
    }

    /// Logger sink that appends to this buffer.
    pub fn sink(self: &Arc<Self>) -> LogSink {
        let me = Arc::clone(self);
        Arc::new(move |r: &LogRecord| me.push(r.clone()))
    }

    /// Newest `limit` lines at/above `min_level` newer than `since` (unix ms),
    /// plus the dropped counter.
    pub fn read_lines(
        &self,
        limit: usize,
        min_level: Option<LogLevel>,
        since: Option<i64>,
    ) -> (Vec<Line>, usize) {
        let g = self.inner.lock();
        let mut sel: Vec<Line> = g
            .lines
            .iter()
            .filter(|l| min_level.is_none_or(|m| l.level >= m))
            .filter(|l| since.is_none_or(|s| json::millis(l.time) > s))
            .cloned()
            .collect();
        if sel.len() > limit {
            sel.drain(..sel.len() - limit);
        }
        (sel, g.dropped)
    }

    /// `logs.read` result: `{"lines":[...],"dropped":n}`.
    pub fn read(&self, limit: usize, min_level: Option<LogLevel>, since: Option<i64>) -> Value {
        let (lines, dropped) = self.read_lines(limit, min_level, since);
        json!({ "lines": lines.iter().map(Self::encode).collect::<Vec<_>>(), "dropped": dropped })
    }

    pub fn encode(line: &Line) -> Value {
        let mut fields = Map::new();
        for (k, v) in &line.fields {
            fields.insert(k.clone(), Value::String(v.clone()));
        }
        json!({
            "time": json::rfc3339(line.time),
            "level": line.level.label(),
            "msg": line.msg,
            "fields": Value::Object(fields),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn rec(level: LogLevel, msg: &str) -> LogRecord {
        LogRecord {
            time: Utc::now(),
            level,
            msg: msg.into(),
            fields: BTreeMap::new(),
        }
    }

    #[test]
    fn ring_drops_oldest_and_counts() {
        let b = LogBuffer::new(2);
        for m in ["a", "b", "c"] {
            b.push(rec(LogLevel::Info, m));
        }
        let v = b.read(10, None, None);
        assert_eq!(v["dropped"], 1);
        let lines = v["lines"].as_array().unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["msg"], "b");
    }

    #[test]
    fn filters_level_limit_since() {
        let b = LogBuffer::new(10);
        b.push(rec(LogLevel::Debug, "d"));
        b.push(rec(LogLevel::Warn, "w"));
        b.push(rec(LogLevel::Error, "e"));
        let v = b.read(1, Some(LogLevel::Warn), None);
        assert_eq!(v["lines"].as_array().unwrap().len(), 1);
        assert_eq!(v["lines"][0]["msg"], "e");
        assert_eq!(v["lines"][0]["level"], "ERROR");
        let far = json::millis(Utc::now()) + 100_000;
        assert!(
            b.read(10, None, Some(far))["lines"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn on_line_fires() {
        let b = Arc::new(LogBuffer::new(4));
        let n = Arc::new(Mutex::new(0));
        let n2 = n.clone();
        b.set_on_line(Some(Arc::new(move |_| *n2.lock() += 1)));
        (b.sink())(&rec(LogLevel::Info, "x"));
        assert_eq!(*n.lock(), 1);
    }
}
