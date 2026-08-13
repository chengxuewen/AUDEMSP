//! RTCEngine — entry point for WebRTC backend selection.

#![allow(unexpected_cfgs)]

// ── Mutual exclusion guard ──
// Only one backend feature may be enabled at a time.
#[allow(clippy::non_minimal_cfg)]
#[cfg(any(
    all(feature = "backend-webrtc-rs", feature = "backend-str0m"),
    all(feature = "backend-webrtc-rs", feature = "backend-webrtc-sys"),
    all(feature = "backend-webrtc-sys", feature = "backend-str0m"),
))]
compile_error!(
    "More than one WebRTC backend feature is enabled. \
     Enable exactly one of: backend-webrtc-rs, backend-webrtc-sys, backend-str0m"
);

use crate::factory::RTCPeerConnectionFactory;

/// Reference: webrtc-kit create_factory() pattern. D151.
pub struct RTCEngine;

impl RTCEngine {
    /// Create a RTCPeerConnectionFactory for the selected backend.
    ///
    /// Dispatches to the correct factory constructor based on the
    /// active backend feature flag via compile-time type alias.
    pub fn create_factory() -> RTCPeerConnectionFactory {
        RTCPeerConnectionFactory::new()
    }
}
