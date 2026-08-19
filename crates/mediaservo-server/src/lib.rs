//! MediaServo Server library.
//!
//! Re-exports all server modules for binary and integration test use.

pub mod admin;
pub mod audit;
pub mod config;
pub mod devices;
pub mod monitor;
pub mod relay;
pub mod room;
pub mod status;
pub mod sfu;
pub mod health;
pub mod signaling;
pub mod static_files;
pub mod tls;

// Re-export key dependencies for integration tests
pub use axum;
pub use tokio;
