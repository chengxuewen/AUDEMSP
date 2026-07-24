//! OMSPBase Server library.
//!
//! Re-exports all server modules for binary and integration test use.

pub mod audit;
pub mod config;
pub mod monitor;
pub mod relay;
pub mod room;
pub mod sfu;
pub mod health;
pub mod signaling;
pub mod tls;

// Re-export key dependencies for integration tests
pub use axum;
pub use tokio;
