//! `service.health` file writer/reader (port of `HealthWriter.swift`).
//!
//! On disk: snake_case JSON with ISO-8601 (second precision, `Z`) dates.

use chrono::{DateTime, SecondsFormat, Utc};
use crossbeam_channel::{Sender, bounded};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::Duration;

/// Heartbeats older than this are stale.
pub const STALE_AFTER_SECS: i64 = 60;
/// Default heartbeat period.
pub const DEFAULT_PERIOD: Duration = Duration::from_secs(5);

mod iso {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn fmt(d: &DateTime<Utc>) -> String {
        d.to_rfc3339_opts(SecondsFormat::Secs, true)
    }
    pub fn ser<S: Serializer>(d: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&fmt(d))
    }
    pub fn de<'de, D: Deserializer<'de>>(d: D) -> Result<DateTime<Utc>, D::Error> {
        let s = String::deserialize(d)?;
        DateTime::parse_from_rfc3339(&s)
            .map(|d| d.with_timezone(&Utc))
            .map_err(serde::de::Error::custom)
    }
    pub fn ser_opt<S: Serializer>(d: &Option<DateTime<Utc>>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            Some(d) => ser(d, s),
            None => s.serialize_none(),
        }
    }
    pub fn de_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<DateTime<Utc>>, D::Error> {
        match Option::<String>::deserialize(d)? {
            Some(s) => DateTime::parse_from_rfc3339(&s)
                .map(|d| Some(d.with_timezone(&Utc)))
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

fn now() -> DateTime<Utc> {
    // Second precision, like the on-disk format.
    DateTime::from_timestamp(Utc::now().timestamp(), 0).unwrap_or_else(Utc::now)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceHealth {
    pub pid: i64,
    #[serde(rename = "service_name")]
    pub service_name: String,
    pub mode: String,
    #[serde(
        rename = "started_at",
        serialize_with = "iso::ser",
        deserialize_with = "iso::de"
    )]
    pub started_at: DateTime<Utc>,
    #[serde(
        rename = "last_heartbeat",
        serialize_with = "iso::ser",
        deserialize_with = "iso::de"
    )]
    pub last_heartbeat: DateTime<Utc>,
    #[serde(rename = "web_port")]
    pub web_port: i64,
    #[serde(
        rename = "last_sync_at",
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "iso::ser_opt",
        deserialize_with = "iso::de_opt"
    )]
    pub last_sync_at: Option<DateTime<Utc>>,
    #[serde(
        rename = "next_schedule_at",
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "iso::ser_opt",
        deserialize_with = "iso::de_opt"
    )]
    pub next_schedule_at: Option<DateTime<Utc>>,
    #[serde(rename = "last_error")]
    pub last_error: String,
    #[serde(rename = "active_tasks")]
    pub active_tasks: Vec<String>,
}

impl ServiceHealth {
    pub fn new(pid: i64) -> ServiceHealth {
        let t = now();
        ServiceHealth {
            pid,
            service_name: "gn-drive".into(),
            mode: "service".into(),
            started_at: t,
            last_heartbeat: t,
            web_port: 0,
            last_sync_at: None,
            next_schedule_at: None,
            last_error: String::new(),
            active_tasks: Vec::new(),
        }
    }

    pub fn health_path(config_dir: &str) -> PathBuf {
        Path::new(config_dir).join("service.health")
    }

    /// Read the health file; `None` when absent or undecodable.
    pub fn read(config_dir: &str) -> Option<ServiceHealth> {
        let data = std::fs::read(Self::health_path(config_dir)).ok()?;
        serde_json::from_slice(&data).ok()
    }

    pub fn is_stale_at(&self, at: DateTime<Utc>) -> bool {
        (at - self.last_heartbeat).num_milliseconds() > STALE_AFTER_SECS * 1000
    }

    pub fn is_stale(&self) -> bool {
        self.is_stale_at(Utc::now())
    }

    pub fn uptime_secs_at(&self, at: DateTime<Utc>) -> f64 {
        (at - self.started_at).num_milliseconds() as f64 / 1000.0
    }

    pub fn uptime_secs(&self) -> f64 {
        self.uptime_secs_at(Utc::now())
    }

    /// camelCase shape returned by RPC `service.health` (see docs/gpui/rpc.md).
    pub fn to_rpc_json(&self) -> serde_json::Value {
        let d = |d: &Option<DateTime<Utc>>| match d {
            Some(d) => serde_json::Value::String(iso::fmt(d)),
            None => serde_json::Value::Null,
        };
        serde_json::json!({
            "present": true,
            "stale": self.is_stale(),
            "pid": self.pid,
            "serviceName": self.service_name,
            "mode": self.mode,
            "startedAt": iso::fmt(&self.started_at),
            "lastHeartbeat": iso::fmt(&self.last_heartbeat),
            "uptimeSecs": self.uptime_secs().max(0.0) as i64,
            "webPort": self.web_port,
            "lastSyncAt": d(&self.last_sync_at),
            "nextScheduleAt": d(&self.next_schedule_at),
            "lastError": self.last_error,
            "activeTasks": self.active_tasks,
        })
    }
}

/// RPC `service.health` result for a config dir (`{"present":false}` when absent).
pub fn rpc_health_json(config_dir: &str) -> serde_json::Value {
    match ServiceHealth::read(config_dir) {
        Some(h) => h.to_rpc_json(),
        None => serde_json::json!({ "present": false }),
    }
}

struct Runner {
    stop: Sender<()>,
    handle: JoinHandle<()>,
}

struct Shared {
    path: PathBuf,
    health: Mutex<ServiceHealth>,
}

impl Shared {
    fn write_now(&self) -> std::io::Result<()> {
        let snapshot = {
            let mut h = self.health.lock();
            h.last_heartbeat = now();
            h.clone()
        };
        let data = serde_json::to_vec(&snapshot).map_err(std::io::Error::other)?;
        let tmp = self.path.with_extension("health.tmp");
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, &self.path)
    }
}

/// Periodically writes service health JSON to disk.
pub struct HealthWriter {
    shared: std::sync::Arc<Shared>,
    period: Duration,
    runner: Mutex<Option<Runner>>,
}

impl HealthWriter {
    pub fn new(config_dir: &str) -> HealthWriter {
        Self::with_period(config_dir, DEFAULT_PERIOD)
    }

    /// A zero period falls back to the default (5s).
    pub fn with_period(config_dir: &str, period: Duration) -> HealthWriter {
        HealthWriter {
            shared: std::sync::Arc::new(Shared {
                path: ServiceHealth::health_path(config_dir),
                health: Mutex::new(ServiceHealth::new(std::process::id() as i64)),
            }),
            period: if period.is_zero() {
                DEFAULT_PERIOD
            } else {
                period
            },
            runner: Mutex::new(None),
        }
    }

    /// Write once immediately, then every `period` on a background thread.
    pub fn start(&self) -> std::io::Result<()> {
        {
            let mut h = self.shared.health.lock();
            h.started_at = now();
            h.last_heartbeat = h.started_at;
        }
        self.shared.write_now()?;
        let mut runner = self.runner.lock();
        if runner.is_some() {
            return Ok(());
        }
        let (tx, rx) = bounded::<()>(1);
        let shared = self.shared.clone();
        let period = self.period;
        let handle = std::thread::Builder::new()
            .name("gn-drive.health".into())
            .spawn(move || {
                // Wakes on timeout (tick) or when the sender is dropped/signalled (stop).
                while let Err(crossbeam_channel::RecvTimeoutError::Timeout) =
                    rx.recv_timeout(period)
                {
                    let _ = shared.write_now();
                }
            })?;
        *runner = Some(Runner { stop: tx, handle });
        Ok(())
    }

    /// Stop the heartbeat thread (prompt; joins). Safe to call repeatedly.
    pub fn stop(&self) {
        if let Some(r) = self.runner.lock().take() {
            let _ = r.stop.send(());
            drop(r.stop);
            let _ = r.handle.join();
        }
    }

    pub fn set_web_port(&self, port: i64) {
        self.shared.health.lock().web_port = port;
    }
    pub fn set_active_tasks(&self, tasks: Vec<String>) {
        self.shared.health.lock().active_tasks = tasks;
    }
    pub fn set_last_error(&self, e: &str) {
        self.shared.health.lock().last_error = e.to_string();
    }
    pub fn mark_sync(&self) {
        self.shared.health.lock().last_sync_at = Some(now());
    }
    pub fn mark_next_schedule(&self, d: DateTime<Utc>) {
        self.shared.health.lock().next_schedule_at = Some(d);
    }

    /// Snapshot of the in-memory state.
    pub fn snapshot(&self) -> ServiceHealth {
        self.shared.health.lock().clone()
    }

    /// Force a write now (tests / on-demand flush).
    pub fn write_now(&self) -> std::io::Result<()> {
        self.shared.write_now()
    }
}

impl Drop for HealthWriter {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn start_writes_file_with_swift_keys() {
        let d = tempfile::tempdir().unwrap();
        let cfg = d.path().to_str().unwrap();
        let w = HealthWriter::with_period(cfg, Duration::from_secs(3600));
        w.set_active_tasks(vec!["T1".into()]);
        w.set_last_error("boom");
        w.set_web_port(8080);
        w.start().unwrap();
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(d.path().join("service.health")).unwrap())
                .unwrap();
        let obj = raw.as_object().unwrap();
        for k in [
            "pid",
            "service_name",
            "mode",
            "started_at",
            "last_heartbeat",
            "web_port",
            "last_error",
            "active_tasks",
        ] {
            assert!(obj.contains_key(k), "missing {k}");
        }
        // nil optionals are omitted like Swift's synthesized Encodable.
        assert!(!obj.contains_key("last_sync_at") && !obj.contains_key("next_schedule_at"));
        assert_eq!(obj["service_name"], "gn-drive");
        assert_eq!(obj["mode"], "service");
        assert_eq!(obj["pid"], std::process::id());
        assert_eq!(obj["web_port"], 8080);
        let s = obj["started_at"].as_str().unwrap();
        assert!(s.ends_with('Z') && !s.contains('.'), "{s}");
        w.stop();
    }

    #[test]
    fn read_roundtrip_and_optionals() {
        let d = tempfile::tempdir().unwrap();
        let cfg = d.path().to_str().unwrap();
        assert!(ServiceHealth::read(cfg).is_none());
        let w = HealthWriter::with_period(cfg, Duration::from_secs(3600));
        w.mark_sync();
        w.mark_next_schedule(DateTime::from_timestamp(2_000_000_000, 0).unwrap());
        w.start().unwrap();
        let h = ServiceHealth::read(cfg).unwrap();
        assert!(h.last_sync_at.is_some());
        assert_eq!(h.next_schedule_at.unwrap().timestamp(), 2_000_000_000);
        assert!(!h.is_stale());
        w.stop();
    }

    #[test]
    fn read_decodes_swift_file() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("service.health"),
            r#"{"pid":5123,"service_name":"gn-drive","mode":"service","started_at":"2026-10-22T12:00:00Z","last_heartbeat":"2026-10-22T12:05:00Z","web_port":0,"last_error":"","active_tasks":["T1"]}"#,
        )
        .unwrap();
        let h = ServiceHealth::read(d.path().to_str().unwrap()).unwrap();
        assert_eq!(h.pid, 5123);
        assert_eq!(h.active_tasks, vec!["T1"]);
        assert!(h.last_sync_at.is_none());
        let at = DateTime::parse_from_rfc3339("2026-10-22T12:05:30Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(!h.is_stale_at(at));
        assert!(h.is_stale_at(at + chrono::Duration::seconds(31)));
        assert_eq!(h.uptime_secs_at(at), 330.0);
    }

    #[test]
    fn corrupt_file_reads_as_none() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("service.health"), "{nope").unwrap();
        assert!(ServiceHealth::read(d.path().to_str().unwrap()).is_none());
        assert_eq!(
            rpc_health_json(d.path().to_str().unwrap()),
            serde_json::json!({"present": false})
        );
    }

    #[test]
    fn rpc_json_shape() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("service.health"),
            r#"{"pid":1,"service_name":"gn-drive","mode":"service","started_at":"2026-10-22T12:00:00Z","last_heartbeat":"2020-10-22T12:05:00Z","web_port":0,"last_error":"","active_tasks":[]}"#,
        )
        .unwrap();
        let v = rpc_health_json(d.path().to_str().unwrap());
        assert_eq!(v["present"], true);
        assert_eq!(v["stale"], true);
        assert_eq!(v["serviceName"], "gn-drive");
        assert_eq!(v["lastSyncAt"], serde_json::Value::Null);
        assert_eq!(v["startedAt"], "2026-10-22T12:00:00Z");
        assert!(v["uptimeSecs"].is_i64());
    }

    #[test]
    fn heartbeat_advances_and_stop_is_prompt() {
        let d = tempfile::tempdir().unwrap();
        let cfg = d.path().to_str().unwrap();
        let w = HealthWriter::with_period(cfg, Duration::from_millis(20));
        w.start().unwrap();
        w.set_last_error("later");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if ServiceHealth::read(cfg).is_some_and(|h| h.last_error == "later") {
                break;
            }
            assert!(Instant::now() < deadline, "periodic write never happened");
            std::thread::sleep(Duration::from_millis(10));
        }
        let t = Instant::now();
        w.stop();
        assert!(t.elapsed() < Duration::from_millis(500));
        // after stop no further writes
        w.set_last_error("after-stop");
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(ServiceHealth::read(cfg).unwrap().last_error, "later");
        w.stop();
    }

    #[test]
    fn long_period_stop_does_not_wait_for_tick() {
        let d = tempfile::tempdir().unwrap();
        let w = HealthWriter::with_period(d.path().to_str().unwrap(), Duration::from_secs(3600));
        w.start().unwrap();
        let t = Instant::now();
        w.stop();
        assert!(t.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn zero_period_uses_default() {
        let d = tempfile::tempdir().unwrap();
        let w = HealthWriter::with_period(d.path().to_str().unwrap(), Duration::ZERO);
        assert_eq!(w.period, DEFAULT_PERIOD);
    }
}
