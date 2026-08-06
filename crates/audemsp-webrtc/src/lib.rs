//! audemsp-webrtc — multi-backend W3C WebRTC API wrapper.
//!
//! Provides RTCPeerConnection and RTCDataChannel types through
//! [`RTCEngine::create_factory`] as the unified entry point.
//! Two backends:
//! - `backend-webrtc-rs` feature: real webrtc-rs implementation
//! - default (no feature): stub for compilation without WebRTC

pub mod data_channel;
pub mod peer_connection;
pub mod sdp;
pub mod track;
pub mod engine;
pub mod rtp;
pub mod stats;
pub mod backend;
pub mod factory;
pub mod traits;

// Re-export backend-specific types for examples/tests
pub use backend::TrackWriteBackend;
pub use peer_connection::*;
pub use sdp::*;
pub use track::*;
pub use engine::*;
pub use rtp::*;
pub use stats::*;
pub use factory::*;
pub use data_channel::*;

/// Error type for all WebRTC operations.
#[derive(Debug, thiserror::Error)]
pub enum RTCError {
    #[error("RTCPeerConnection error: {0}")]
    RTCPeerConnection(String),
    #[error("RTCDataChannel error: {0}")]
    RTCDataChannel(String),
    #[error("SDP error: {0}")]
    Sdp(String),
    #[error("Track error: {0}")]
    Track(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Not supported by backend: {0}")]
    NotSupported(String),
}

#[cfg(feature = "backend-webrtc-rs")]
impl From<webrtc::error::Error> for RTCError {
    fn from(e: webrtc::error::Error) -> Self {
        RTCError::Internal(e.to_string())
    }
}

impl RTCError {
    /// Return a stable, context-free identifier for this error.
    /// Used by future i18n layers to look up locale-specific text.
    pub fn locale_key(&self) -> &'static str {
        match self {
            RTCError::RTCPeerConnection(_) => "RTCPC",
            RTCError::RTCDataChannel(_) => "RTCDC",
            RTCError::Sdp(_) => "RTCSD",
            RTCError::Track(_) => "RTCTK",
            RTCError::Internal(_) => "RTCIN",
            RTCError::NotSupported(_) => "RTCNS",
        }
    }
}

/// Re-export webrtc-rs for callback types used by consumers.
#[cfg(feature = "backend-webrtc-rs")]
pub use webrtc;
