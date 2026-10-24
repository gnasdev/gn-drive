//! Cron scheduling (port of `Cron.swift`, robfig/cron v3 semantics).
//!
//! Supports 5-field (`min hour dom mon dow`), 6-field (leading seconds) and
//! `@descriptors` (`@hourly`, `@daily`, `@weekly`, `@monthly`, `@yearly`,
//! `@every <dur>`). Evaluated in local time via `chrono::Local`.
//!
//! Day matching follows robfig: when *both* day-of-month and day-of-week are
//! restricted (neither starts with `*`/`?`) a day matches if **either**
//! matches; otherwise both must match.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, Timelike, Utc};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CronError {
    #[error("cron expression is empty")]
    Empty,
    #[error("invalid cron: want 5 or 6 fields, got {0}")]
    BadFieldCount(usize),
    #[error("invalid cron expression: {0}")]
    Invalid(String),
}

const MONTH_NAMES: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const DOW_NAMES: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

/// Bitmask of allowed values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Field(u64);

impl Field {
    fn contains(self, v: u32) -> bool {
        v < 64 && self.0 >> v & 1 == 1
    }
    /// First allowed value >= v.
    fn at_or_after(self, v: u32) -> Option<u32> {
        if v >= 64 {
            return None;
        }
        let m = self.0 >> v;
        if m == 0 {
            None
        } else {
            Some(v + m.trailing_zeros())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronSchedule {
    seconds: Field,
    minutes: Field,
    hours: Field,
    dom: Field,
    months: Field,
    dow: Field,
    dom_restricted: bool,
    dow_restricted: bool,
    /// `@every`: fixed interval in seconds.
    every_seconds: Option<i64>,
}

/// Normalise a UI expression for a seconds-aware parser: 5 fields get a
/// leading `0`; 6 fields and `@descriptors` pass through (Go `NormalizeCron`).
pub fn normalize_cron(expr: &str) -> Result<String, CronError> {
    let e = expr.trim();
    if e.is_empty() {
        return Err(CronError::Empty);
    }
    if e.starts_with('@') {
        CronSchedule::parse(e)?;
        return Ok(e.to_string());
    }
    let fields: Vec<&str> = e.split_whitespace().collect();
    match fields.len() {
        5 => Ok(format!("0 {}", fields.join(" "))),
        6 => Ok(fields.join(" ")),
        n => Err(CronError::BadFieldCount(n)),
    }
}

impl CronSchedule {
    /// Accepts 5-field UI expressions or 6-field second-aware ones.
    pub fn parse(expr: &str) -> Result<CronSchedule, CronError> {
        let mut e = expr.trim().to_string();
        if e.is_empty() {
            return Err(CronError::Empty);
        }
        if e.starts_with('@') {
            let lower = e.to_lowercase();
            match lower.as_str() {
                "@yearly" | "@annually" => e = "0 0 0 1 1 *".into(),
                "@monthly" => e = "0 0 0 1 * *".into(),
                "@weekly" => e = "0 0 0 * * 0".into(),
                "@daily" | "@midnight" => e = "0 0 0 * * *".into(),
                "@hourly" => e = "0 0 * * * *".into(),
                _ => {
                    if let Some(rest) = lower.strip_prefix("@every ")
                        && let Some(d) = parse_duration(rest)
                    {
                        let all = |lo: u32, hi: u32| Field(mask_range(lo, hi, 1));
                        return Ok(CronSchedule {
                            seconds: all(0, 59),
                            minutes: all(0, 59),
                            hours: all(0, 23),
                            dom: all(1, 31),
                            months: all(1, 12),
                            dow: all(0, 6),
                            dom_restricted: false,
                            dow_restricted: false,
                            // robfig: delays below one second round up to one second.
                            every_seconds: Some((d as i64).max(1)),
                        });
                    }
                    return Err(CronError::Invalid(expr.to_string()));
                }
            }
        }

        let mut fields: Vec<String> = e.split_whitespace().map(String::from).collect();
        match fields.len() {
            5 => fields.insert(0, "0".into()),
            6 => {}
            n => return Err(CronError::BadFieldCount(n)),
        }

        let seconds = parse_field(&fields[0], 0, 59, &[], 0)?;
        let minutes = parse_field(&fields[1], 0, 59, &[], 0)?;
        let hours = parse_field(&fields[2], 0, 23, &[], 0)?;
        let dom = parse_field(&fields[3], 1, 31, &[], 0)?;
        let months = parse_field(&fields[4], 1, 12, &MONTH_NAMES, 1)?;
        let mut dow = parse_field(&fields[5], 0, 7, &DOW_NAMES, 0)?;
        // Normalise dow 7 -> 0 (both Sunday).
        if dow.contains(7) {
            dow = Field((dow.0 & !(1 << 7)) | 1);
        }
        Ok(CronSchedule {
            seconds,
            minutes,
            hours,
            dom,
            months,
            dow,
            dom_restricted: is_restricted(&fields[3]),
            dow_restricted: is_restricted(&fields[5]),
            every_seconds: None,
        })
    }

    /// Next fire time strictly after `date` (local time).
    pub fn next_after(&self, date: DateTime<Local>) -> Option<DateTime<Local>> {
        if let Some(every) = self.every_seconds {
            let base = date.with_nanosecond(0).unwrap_or(date);
            return Some(base + Duration::seconds(every));
        }
        let mut cursor = date.naive_local();
        // Bounded: each iteration advances the cursor; DST gaps/folds are rare.
        for _ in 0..64 {
            let cand = self.next_naive_after(cursor)?;
            match resolve_local(cand) {
                Some(dt) if dt > date => return Some(dt),
                // Nonexistent (spring-forward gap) or repeated (fall-back) local
                // time at/before `date`: keep searching past this candidate.
                _ => cursor = cand,
            }
        }
        None
    }

    /// UTC convenience wrapper over [`next_after`](Self::next_after).
    pub fn next_after_utc(&self, date: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.next_after(date.with_timezone(&Local))
            .map(|d| d.with_timezone(&Utc))
    }

    /// `SystemTime` convenience wrapper over [`next_after`](Self::next_after).
    pub fn next_after_system(&self, t: SystemTime) -> Option<SystemTime> {
        self.next_after(DateTime::<Local>::from(t))
            .map(SystemTime::from)
    }

    /// Timezone-free core: next matching wall-clock time strictly after `after`
    /// (sub-second part ignored). `@every` schedules just add the interval.
    pub fn next_naive_after(&self, after: NaiveDateTime) -> Option<NaiveDateTime> {
        if let Some(every) = self.every_seconds {
            return Some(after.with_nanosecond(0)? + Duration::seconds(every));
        }
        let mut t = after.with_nanosecond(0)? + Duration::seconds(1);
        let limit_year = after.year() + 8;
        let midnight = |d: NaiveDate| d.and_hms_opt(0, 0, 0);
        while t.year() <= limit_year {
            let date = t.date();
            let Some(m) = self.months.at_or_after(t.month()) else {
                t = midnight(NaiveDate::from_ymd_opt(t.year() + 1, 1, 1)?)?;
                continue;
            };
            if m != t.month() {
                t = midnight(NaiveDate::from_ymd_opt(t.year(), m, 1)?)?;
                continue;
            }
            if !self.day_matches(date) {
                t = midnight(date.succ_opt()?)?;
                continue;
            }
            let hour = t.hour();
            if !self.hours.contains(hour) {
                t = match self.hours.at_or_after(hour + 1) {
                    Some(h) => date.and_hms_opt(h, 0, 0)?,
                    None => midnight(date.succ_opt()?)?,
                };
                continue;
            }
            let minute = t.minute();
            if !self.minutes.contains(minute) {
                t = match self.minutes.at_or_after(minute + 1) {
                    Some(mn) => date.and_hms_opt(hour, mn, 0)?,
                    None => date.and_hms_opt(hour, 0, 0)? + Duration::hours(1),
                };
                continue;
            }
            let second = t.second();
            if !self.seconds.contains(second) {
                t = match self.seconds.at_or_after(second + 1) {
                    Some(s) => date.and_hms_opt(hour, minute, s)?,
                    None => date.and_hms_opt(hour, minute, 0)? + Duration::minutes(1),
                };
                continue;
            }
            return Some(t);
        }
        None
    }

    fn day_matches(&self, d: NaiveDate) -> bool {
        let dom_ok = self.dom.contains(d.day());
        let dow_ok = self.dow.contains(d.weekday().num_days_from_sunday());
        if self.dom_restricted && self.dow_restricted {
            dom_ok || dow_ok
        } else {
            dom_ok && dow_ok
        }
    }
}

/// Map a naive local time to a real instant. Ambiguous (fold) → earliest;
/// nonexistent (gap) → `None`.
fn resolve_local(n: NaiveDateTime) -> Option<DateTime<Local>> {
    n.and_local_timezone(Local).earliest()
}

/// A field is "restricted" unless some comma-part's range starts with `*`/`?`.
fn is_restricted(field: &str) -> bool {
    !field.split(',').any(|part| {
        let range = part.split('/').next().unwrap_or("");
        range == "*" || range == "?"
    })
}

fn mask_range(lo: u32, hi: u32, step: u32) -> u64 {
    let mut m = 0u64;
    let mut v = lo;
    while v <= hi {
        m |= 1 << v;
        v += step;
    }
    m
}

fn parse_bound(tok: &str, names: &[&str], name_base: u32) -> Option<u32> {
    if let Ok(n) = tok.parse::<u32>() {
        return Some(n);
    }
    names
        .iter()
        .position(|n| *n == tok)
        .map(|i| i as u32 + name_base)
}

fn parse_field(
    field: &str,
    min: u32,
    max: u32,
    names: &[&str],
    name_base: u32,
) -> Result<Field, CronError> {
    let bad = || CronError::Invalid(field.to_string());
    let f = field.trim().to_lowercase();
    let mut mask = 0u64;
    for part in f.split(',') {
        let pieces: Vec<&str> = part.split('/').collect();
        if pieces.len() > 2 {
            return Err(bad());
        }
        let has_step = pieces.len() == 2;
        let step: u32 = if has_step {
            pieces[1].parse().map_err(|_| bad())?
        } else {
            1
        };
        if step == 0 {
            return Err(bad());
        }
        let range = pieces[0];
        let (lo, hi);
        if range == "*" || range == "?" {
            lo = min;
            hi = max;
        } else if range.contains('-') {
            let b: Vec<&str> = range.split('-').collect();
            if b.len() != 2 {
                return Err(bad());
            }
            lo = parse_bound(b[0], names, name_base).ok_or_else(bad)?;
            hi = parse_bound(b[1], names, name_base).ok_or_else(bad)?;
        } else {
            lo = parse_bound(range, names, name_base).ok_or_else(bad)?;
            // "N" alone means just N; "N/step" means N-max.
            hi = if has_step { max } else { lo };
        }
        if lo < min || hi > max || lo > hi {
            return Err(bad());
        }
        mask |= mask_range(lo, hi, step);
    }
    Ok(Field(mask))
}

/// `1h30m`, `15m`, `30s`; a trailing bare number counts as seconds.
fn parse_duration(s: &str) -> Option<f64> {
    let mut total = 0.0;
    let mut num = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
        } else {
            let v: f64 = num.parse().ok()?;
            num.clear();
            match c {
                'h' => total += v * 3600.0,
                'm' => total += v * 60.0,
                's' => total += v,
                _ => return None,
            }
        }
    }
    if !num.is_empty()
        && let Ok(v) = num.parse::<f64>()
    {
        total += v;
    }
    (total > 0.0).then_some(total)
}

// ---- Runner ---------------------------------------------------------------

type Handler = Arc<dyn Fn() + Send + Sync>;

struct Entry {
    schedule: CronSchedule,
    handler: Handler,
    next_fire: DateTime<Local>,
}

struct Inner {
    entries: Mutex<HashMap<String, Entry>>,
}

impl Inner {
    fn tick_at(&self, now: DateTime<Local>) -> usize {
        let mut fired: Vec<Handler> = Vec::new();
        {
            let mut entries = self.entries.lock();
            for e in entries.values_mut() {
                if e.next_fire <= now {
                    fired.push(e.handler.clone());
                    // Mirror Swift: a schedule with no further fire parks far away.
                    e.next_fire = e
                        .schedule
                        .next_after(now)
                        .unwrap_or(now + Duration::days(36500));
                }
            }
        }
        let n = fired.len();
        for h in fired {
            h();
        }
        n
    }
}

struct Ticker {
    stop: crossbeam_channel::Sender<()>,
    handle: std::thread::JoinHandle<()>,
}

/// Minimal cron runner: checks each registered schedule once per second on a
/// single serial tick thread (handlers run on it, like Swift's serial queue).
pub struct CronRunner {
    inner: Arc<Inner>,
    ticker: Mutex<Option<Ticker>>,
}

impl Default for CronRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl CronRunner {
    pub fn new() -> CronRunner {
        CronRunner {
            inner: Arc::new(Inner {
                entries: Mutex::new(HashMap::new()),
            }),
            ticker: Mutex::new(None),
        }
    }

    /// Start the 1 Hz tick thread (no-op when already started).
    pub fn start(&self) {
        let mut guard = self.ticker.lock();
        if guard.is_some() {
            return;
        }
        let (tx, rx) = crossbeam_channel::bounded::<()>(1);
        let inner = self.inner.clone();
        let handle = std::thread::Builder::new()
            .name("gn-drive.cron".into())
            .spawn(move || {
                while let Err(crossbeam_channel::RecvTimeoutError::Timeout) =
                    rx.recv_timeout(std::time::Duration::from_secs(1))
                {
                    inner.tick_at(Local::now());
                }
            })
            .expect("spawn cron thread");
        *guard = Some(Ticker { stop: tx, handle });
    }

    /// Stop the tick thread and drop every entry.
    pub fn stop(&self) {
        let t = self.ticker.lock().take();
        if let Some(t) = t {
            let _ = t.stop.send(());
            // A handler calling stop() runs on the tick thread: don't join self.
            if t.handle.thread().id() != std::thread::current().id() {
                let _ = t.handle.join();
            }
        }
        self.inner.entries.lock().clear();
    }

    pub fn is_running(&self) -> bool {
        self.ticker.lock().is_some()
    }

    /// Register (or replace) `id`. Returns false when the schedule never fires.
    pub fn add(
        &self,
        id: &str,
        schedule: CronSchedule,
        handler: impl Fn() + Send + Sync + 'static,
    ) -> bool {
        self.add_at(id, schedule, handler, Local::now())
    }

    /// [`add`](Self::add) with an injected clock (tests).
    pub fn add_at(
        &self,
        id: &str,
        schedule: CronSchedule,
        handler: impl Fn() + Send + Sync + 'static,
        now: DateTime<Local>,
    ) -> bool {
        let Some(next_fire) = schedule.next_after(now) else {
            return false;
        };
        self.inner.entries.lock().insert(
            id.to_string(),
            Entry {
                schedule,
                handler: Arc::new(handler),
                next_fire,
            },
        );
        true
    }

    pub fn remove(&self, id: &str) {
        self.inner.entries.lock().remove(id);
    }

    pub fn contains(&self, id: &str) -> bool {
        self.inner.entries.lock().contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.inner.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.entries.lock().is_empty()
    }

    /// Next scheduled fire time for `id`.
    pub fn next_fire(&self, id: &str) -> Option<DateTime<Local>> {
        self.inner.entries.lock().get(id).map(|e| e.next_fire)
    }

    /// Fire every entry due at `now` (on the calling thread); returns how many fired.
    pub fn tick_at(&self, now: DateTime<Local>) -> usize {
        self.inner.tick_at(now)
    }
}

impl Drop for CronRunner {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn n(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, s)
            .unwrap()
    }

    fn next(expr: &str, after: NaiveDateTime) -> Option<NaiveDateTime> {
        CronSchedule::parse(expr).unwrap().next_naive_after(after)
    }

    #[test]
    fn normalize_cron_go_cases() {
        assert_eq!(normalize_cron("0 * * * *").unwrap(), "0 0 * * * *");
        assert_eq!(normalize_cron("15 0 * * * *").unwrap(), "15 0 * * * *");
        assert!(normalize_cron("@hourly").is_ok());
        for bad in ["", "not-a-cron", "a b c", "* *"] {
            assert!(normalize_cron(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn parse_matrix_valid() {
        for ok in [
            "* * * * *",
            "*/5 * * * *",
            "0 0 1 1 *",
            "30 2 * * *",
            "0 9 * * 1-5",
            "0 9 * * mon-fri",
            "0 0 1 JAN,jul *",
            "15 * * * * *",
            "0-30/10 * * * *",
            "5/15 * * * *",
            "0 0 * * 7",
            "0 0 ? * ?",
            "  0   0 * * *  ",
            "@yearly",
            "@annually",
            "@monthly",
            "@weekly",
            "@daily",
            "@midnight",
            "@hourly",
            "@HOURLY",
            "@every 15m",
            "@every 1h30m",
            "@every 30s",
        ] {
            assert!(CronSchedule::parse(ok).is_ok(), "{ok:?} should parse");
        }
    }

    #[test]
    fn parse_matrix_invalid() {
        for bad in [
            "",
            "   ",
            "not-a-cron",
            "a b c",
            "* *",
            "* * * * * * *",
            "60 * * * *",
            "* 24 * * *",
            "* * 0 * *",
            "* * 32 * *",
            "* * * 13 *",
            "* * * * 8",
            "*/0 * * * *",
            "5-2 * * * *",
            "x * * * *",
            "1-2-3 * * * *",
            "*/x * * * *",
            "1/2/3 * * * *",
            "1,,2 * * * *",
            "@bogus",
            "@every",
            "@every xyz",
            "@every 100ms",
            "* * * * foo",
        ] {
            assert!(CronSchedule::parse(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn error_display_texts() {
        assert_eq!(
            CronSchedule::parse("").unwrap_err().to_string(),
            "cron expression is empty"
        );
        assert_eq!(
            CronSchedule::parse("* *").unwrap_err().to_string(),
            "invalid cron: want 5 or 6 fields, got 2"
        );
        assert_eq!(
            CronSchedule::parse("99 * * * *").unwrap_err().to_string(),
            "invalid cron expression: 99"
        );
        assert_eq!(
            CronSchedule::parse("@nope").unwrap_err(),
            CronError::Invalid("@nope".into())
        );
    }

    #[test]
    fn next_basic_swift_cases() {
        let now = n(2026, 9, 26, 10, 0, 0);
        assert_eq!(next("30 2 * * *", now), Some(n(2026, 9, 27, 2, 30, 0)));
        assert_eq!(next("15 * * * * *", now), Some(n(2026, 9, 26, 10, 0, 15)));
        assert_eq!(next("@hourly", now), Some(n(2026, 9, 26, 11, 0, 0)));
        assert_eq!(next("@daily", now), Some(n(2026, 9, 27, 0, 0, 0)));
    }

    #[test]
    fn next_is_strictly_after() {
        assert_eq!(
            next("0 * * * *", n(2026, 1, 1, 10, 0, 0)),
            Some(n(2026, 1, 1, 11, 0, 0))
        );
        assert_eq!(
            next("* * * * * *", n(2026, 1, 1, 10, 0, 0)),
            Some(n(2026, 1, 1, 10, 0, 1))
        );
        // Sub-second part of the base is ignored.
        let base = n(2026, 1, 1, 10, 0, 0) + Duration::milliseconds(900);
        assert_eq!(next("* * * * * *", base), Some(n(2026, 1, 1, 10, 0, 1)));
    }

    #[test]
    fn next_steps_ranges_lists() {
        assert_eq!(
            next("*/15 * * * *", n(2026, 1, 1, 10, 7, 0)),
            Some(n(2026, 1, 1, 10, 15, 0))
        );
        assert_eq!(
            next("*/15 * * * *", n(2026, 1, 1, 10, 45, 0)),
            Some(n(2026, 1, 1, 11, 0, 0))
        );
        assert_eq!(
            next("0-30/10 * * * *", n(2026, 1, 1, 10, 25, 0)),
            Some(n(2026, 1, 1, 10, 30, 0))
        );
        assert_eq!(
            next("5/20 * * * *", n(2026, 1, 1, 10, 6, 0)),
            Some(n(2026, 1, 1, 10, 25, 0))
        );
        assert_eq!(
            next("0 8,12,18 * * *", n(2026, 1, 1, 13, 0, 0)),
            Some(n(2026, 1, 1, 18, 0, 0))
        );
        assert_eq!(
            next("0 8,12,18 * * *", n(2026, 1, 1, 19, 0, 0)),
            Some(n(2026, 1, 2, 8, 0, 0))
        );
        assert_eq!(
            next("0 0 1 JAN-MAR *", n(2026, 4, 1, 0, 0, 0)),
            Some(n(2027, 1, 1, 0, 0, 0))
        );
    }

    #[test]
    fn next_month_and_year_rollover() {
        assert_eq!(
            next("0 0 1 * *", n(2026, 12, 31, 23, 59, 59)),
            Some(n(2027, 1, 1, 0, 0, 0))
        );
        assert_eq!(
            next("0 0 1 3 *", n(2026, 4, 1, 0, 0, 0)),
            Some(n(2027, 3, 1, 0, 0, 0))
        );
        assert_eq!(
            next("0 0 31 * *", n(2026, 1, 31, 0, 0, 0)),
            Some(n(2026, 3, 31, 0, 0, 0))
        );
        assert_eq!(
            next("@monthly", n(2026, 1, 15, 0, 0, 0)),
            Some(n(2026, 2, 1, 0, 0, 0))
        );
        assert_eq!(
            next("@yearly", n(2026, 1, 15, 0, 0, 0)),
            Some(n(2027, 1, 1, 0, 0, 0))
        );
        assert_eq!(
            next("59 59 23 31 12 *", n(2026, 12, 31, 23, 59, 58)),
            Some(n(2026, 12, 31, 23, 59, 59))
        );
        assert_eq!(
            next("59 59 23 31 12 *", n(2026, 12, 31, 23, 59, 59)),
            Some(n(2027, 12, 31, 23, 59, 59))
        );
    }

    #[test]
    fn next_feb_29() {
        assert_eq!(
            next("0 0 29 2 *", n(2025, 3, 1, 0, 0, 0)),
            Some(n(2028, 2, 29, 0, 0, 0))
        );
        assert_eq!(
            next("0 0 29 2 *", n(2028, 2, 29, 0, 0, 0)),
            Some(n(2032, 2, 29, 0, 0, 0))
        );
        // 2100 is not a leap year.
        assert_eq!(
            next("0 0 29 2 *", n(2096, 3, 1, 0, 0, 0)),
            Some(n(2104, 2, 29, 0, 0, 0))
        );
        // Feb 30 never exists.
        assert_eq!(next("0 0 30 2 *", n(2026, 1, 1, 0, 0, 0)), None);
    }

    #[test]
    fn next_dom_dow_or_rule() {
        // 2026-02-07 is a Saturday. Fridays or the 10th: Tue Feb 10 first, then Fri Feb 13.
        assert_eq!(
            next("0 0 10 * 5", n(2026, 2, 7, 0, 0, 0)),
            Some(n(2026, 2, 10, 0, 0, 0))
        );
        assert_eq!(
            next("0 0 10 * 5", n(2026, 2, 10, 0, 0, 0)),
            Some(n(2026, 2, 13, 0, 0, 0))
        );
        // dow only
        assert_eq!(
            next("0 9 * * 1-5", n(2026, 2, 6, 10, 0, 0)),
            Some(n(2026, 2, 9, 9, 0, 0))
        );
        // dom only (dow star): AND with star
        assert_eq!(
            next("0 0 10 * *", n(2026, 2, 7, 0, 0, 0)),
            Some(n(2026, 2, 10, 0, 0, 0))
        );
        // "*/2" in dom counts as star (robfig): odd days AND Monday.
        assert_eq!(
            next("0 0 */2 * 1", n(2026, 2, 1, 0, 0, 0)),
            Some(n(2026, 2, 9, 0, 0, 0))
        );
        // Sunday as 0 and 7 and name.
        for e in ["0 0 * * 0", "0 0 * * 7", "0 0 * * sun"] {
            assert_eq!(
                next(e, n(2026, 2, 1, 0, 0, 0)),
                Some(n(2026, 2, 8, 0, 0, 0)),
                "{e}"
            );
        }
        assert_eq!(
            next("@weekly", n(2026, 2, 1, 0, 0, 0)),
            Some(n(2026, 2, 8, 0, 0, 0))
        );
    }

    #[test]
    fn next_every() {
        let s = CronSchedule::parse("@every 90s").unwrap();
        assert_eq!(
            s.next_naive_after(n(2026, 1, 1, 0, 0, 0)),
            Some(n(2026, 1, 1, 0, 1, 30))
        );
        let s = CronSchedule::parse("@every 1h30m").unwrap();
        assert_eq!(
            s.next_naive_after(n(2026, 1, 1, 0, 0, 0)),
            Some(n(2026, 1, 1, 1, 30, 0))
        );
        // Sub-second intervals round up to one second.
        let s = CronSchedule::parse("@every 0.5s").unwrap();
        assert_eq!(
            s.next_naive_after(n(2026, 1, 1, 0, 0, 0)),
            Some(n(2026, 1, 1, 0, 0, 1))
        );
    }

    #[test]
    fn next_after_local_matches_naive() {
        // Mid-day, mid-month: no DST edge in any zone.
        let base = Local
            .from_local_datetime(&n(2026, 6, 10, 12, 0, 0))
            .unwrap();
        let s = CronSchedule::parse("30 2 * * *").unwrap();
        let got = s.next_after(base).unwrap();
        assert_eq!(got.naive_local(), n(2026, 6, 11, 2, 30, 0));
        assert!(got > base);

        let s = CronSchedule::parse("@every 10s").unwrap();
        assert_eq!(s.next_after(base).unwrap() - base, Duration::seconds(10));

        let utc = s.next_after_utc(base.with_timezone(&Utc)).unwrap();
        assert_eq!(utc, (base + Duration::seconds(10)).with_timezone(&Utc));
        let sys = s.next_after_system(SystemTime::from(base)).unwrap();
        assert_eq!(DateTime::<Local>::from(sys), base + Duration::seconds(10));
    }

    #[test]
    fn next_after_always_strictly_future_across_a_year() {
        // DST-agnostic sanity: every hourly step is strictly increasing in real time.
        let s = CronSchedule::parse("30 2 * * *").unwrap();
        let mut t = Local.from_local_datetime(&n(2026, 1, 1, 0, 0, 0)).unwrap();
        for _ in 0..400 {
            let nx = s.next_after(t).unwrap();
            assert!(nx > t);
            assert!(nx - t <= Duration::hours(49));
            t = nx;
        }
    }

    #[test]
    fn runner_fires_via_tick_at() {
        let runner = CronRunner::new();
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        let t0 = Local
            .from_local_datetime(&n(2026, 6, 10, 12, 0, 0))
            .unwrap();
        let s = CronSchedule::parse("*/5 * * * * *").unwrap();
        assert!(runner.add_at(
            "a",
            s,
            move || {
                h.fetch_add(1, Ordering::SeqCst);
            },
            t0
        ));
        assert_eq!(
            runner.next_fire("a").unwrap().naive_local(),
            n(2026, 6, 10, 12, 0, 5)
        );

        assert_eq!(runner.tick_at(t0 + Duration::seconds(4)), 0);
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        assert_eq!(runner.tick_at(t0 + Duration::seconds(5)), 1);
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        // Same instant again: next fire has advanced.
        assert_eq!(runner.tick_at(t0 + Duration::seconds(5)), 0);
        assert_eq!(runner.tick_at(t0 + Duration::seconds(10)), 1);
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        // A long gap fires once, not once per missed slot.
        assert_eq!(runner.tick_at(t0 + Duration::seconds(100)), 1);
        assert_eq!(
            runner.next_fire("a").unwrap().naive_local(),
            n(2026, 6, 10, 12, 1, 45)
        );
    }

    #[test]
    fn runner_remove_replace_and_stop() {
        let runner = CronRunner::new();
        let a = Arc::new(AtomicUsize::new(0));
        let b = Arc::new(AtomicUsize::new(0));
        let t0 = Local
            .from_local_datetime(&n(2026, 6, 10, 12, 0, 0))
            .unwrap();
        let sched = || CronSchedule::parse("* * * * * *").unwrap();
        let a2 = a.clone();
        runner.add_at(
            "x",
            sched(),
            move || {
                a2.fetch_add(1, Ordering::SeqCst);
            },
            t0,
        );
        // Re-register replaces the handler.
        let b2 = b.clone();
        runner.add_at(
            "x",
            sched(),
            move || {
                b2.fetch_add(1, Ordering::SeqCst);
            },
            t0,
        );
        assert_eq!(runner.len(), 1);
        runner.tick_at(t0 + Duration::seconds(1));
        assert_eq!((a.load(Ordering::SeqCst), b.load(Ordering::SeqCst)), (0, 1));

        runner.remove("x");
        runner.remove("unknown"); // no-op
        assert!(runner.is_empty());
        assert_eq!(runner.tick_at(t0 + Duration::seconds(5)), 0);

        runner.add_at("y", sched(), || {}, t0);
        runner.stop();
        assert!(runner.is_empty());
        runner.remove("y"); // after stop: no-op
    }

    #[test]
    fn runner_start_stop_is_prompt_and_idempotent() {
        let runner = CronRunner::new();
        runner.start();
        runner.start();
        assert!(runner.is_running());
        let t = std::time::Instant::now();
        runner.stop();
        assert!(
            t.elapsed() < std::time::Duration::from_millis(500),
            "stop must not wait a full tick"
        );
        assert!(!runner.is_running());
        runner.stop();
    }

    #[test]
    fn runner_add_returns_false_for_never_firing_schedule() {
        let runner = CronRunner::new();
        let s = CronSchedule::parse("0 0 30 2 *").unwrap();
        assert!(!runner.add("never", s, || {}));
        assert!(runner.is_empty());
    }
}
