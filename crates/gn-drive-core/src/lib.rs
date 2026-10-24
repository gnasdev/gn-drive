//! GN Drive engine. Rust port of `Sources/GNDriveCore` (Swift).
//!
//! See `docs/rust-port.md` for the module-by-module mapping.

// `json!` objects in the flow-engine tests nest deep enough to pass the
// default macro recursion limit.
#![recursion_limit = "256"]

pub mod argon2id;
pub mod auth;
pub mod board_engine;
pub mod build_version;
pub mod container;
pub mod cron;
pub mod crypto;
pub mod database;
pub mod event_bus;
pub mod flow_engine;
pub mod health;
pub mod instance_lock;
pub mod keychain;
pub mod launchd;
pub mod logger;
pub mod models;
pub mod paths;
pub mod rclone;
pub mod rpc;
pub mod runtime_hub;
pub mod self_update;
pub mod store;
pub mod sync_engine;
