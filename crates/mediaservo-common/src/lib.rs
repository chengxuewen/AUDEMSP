//! MediaServo shared abstractions.
//!
//! # Modules
//! - `config` — Host/Server/Remote YAML config schemas (serde)
//! - `error` — Unified error codes (1xxx–9xxx)
//! - `metrics` — Prometheus metrics helpers
//! - `protocol` — Signaling message types (WebSocket JSON)
//! - `auth` — PSK HMAC-SHA256 authentication trait
//! - `logging` — JSON structured logging with trace_id propagation
//! - `backup` — Atomic state backup/restore

pub mod config;
pub mod error;
pub mod metrics;
pub mod protocol;
pub mod auth;
pub mod logging;
pub mod backup;
pub mod brand;
