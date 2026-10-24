//! JSON-RPC style protocol v1 (port of `Sources/GNDriveCore/RPC`).
//!
//! [`dispatcher::Dispatcher::handle`] is the transport-independent contract;
//! `stdio` wraps it over NDJSON, the GPUI window calls it in-process.

pub mod bridge;
pub mod dispatcher;
pub mod error;
pub mod handlers_remotes;
pub mod json;
pub mod log_buffer;
pub mod oauth;
pub mod params;
pub mod stdio;

use crate::logger::LogLevel;
use serde_json::Value;

pub use dispatcher::Dispatcher;
pub use error::{RpcError, map_error, redact};
pub use log_buffer::LogBuffer;
pub use params::Params;

pub const PROTOCOL_VERSION: i64 = 1;

/// Pushed event frame: `{"event":..,"ts":..,"data":..}`.
#[derive(Debug, Clone)]
pub struct Event {
    pub name: String,
    /// Unix milliseconds.
    pub ts: i64,
    pub data: Value,
}

impl Event {
    pub fn new(name: &str, data: Value) -> Event {
        Event {
            name: name.to_string(),
            ts: json::millis(chrono::Utc::now()),
            data,
        }
    }

    /// Compact JSON frame, no trailing newline.
    pub fn to_frame(&self) -> String {
        let mut m = serde_json::Map::new();
        m.insert("event".into(), Value::String(self.name.clone()));
        m.insert("ts".into(), Value::from(self.ts));
        m.insert("data".into(), self.data.clone());
        Value::Object(m).to_string()
    }
}

/// Port of `RPCService.Options`.
#[derive(Debug, Clone)]
pub struct Options {
    pub log_level: LogLevel,
    pub log_buffer_capacity: usize,
    pub exit_on_shutdown: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            log_level: LogLevel::Info,
            log_buffer_capacity: 2000,
            exit_on_shutdown: true,
        }
    }
}
