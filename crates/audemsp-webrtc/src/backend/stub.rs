//! Stub backend — all operations are no-ops or return defaults.
//! Used when no WebRTC backend feature is enabled (compilation-only mode).

use super::DcBackend;
use super::PcBackend;
use super::TrackWriteBackend;
use crate::data_channel::{RTCDataChannel, RTCDataChannelInit, RTCDataChannelRx, RTCDataChannelState};
use crate::peer_connection::{
    RTCAnswerOptions, RTCIceCandidate, RTCOfferOptions, RTCConfiguration,
    RTCIceConnectionState, RTCIceGatheringState, RTCPeerConnectionState, RTCSignalingState,
};
use crate::sdp::{RTCSdpType, RTCSessionDescription};
use crate::track::{RTCAudioTrackConfig, TrackKind};
use crate::RTCError;
use std::sync::atomic::{AtomicBool, Ordering};

// ── StubPc ──

#[derive(Debug, Default)]
pub(crate) struct StubPc {
    closed: AtomicBool,
}

impl PcBackend for StubPc {
    async fn create_offer(&self, _: &RTCOfferOptions) -> Result<RTCSessionDescription, RTCError> {
        Ok(RTCSessionDescription::new(RTCSdpType::Offer, String::new()))
    }

    async fn create_answer(&self, _: &RTCAnswerOptions) -> Result<RTCSessionDescription, RTCError> {
        Ok(RTCSessionDescription::new(RTCSdpType::Answer, String::new()))
    }

    async fn set_local_description(&self, _: &RTCSessionDescription) -> Result<(), RTCError> {
        Ok(())
    }

    async fn set_remote_description(&self, _: &RTCSessionDescription) -> Result<(), RTCError> {
        Ok(())
    }

    async fn add_ice_candidate(&self, _: &RTCIceCandidate) -> Result<(), RTCError> {
        Ok(())
    }

    fn connection_state(&self) -> RTCPeerConnectionState {
        if self.closed.load(Ordering::Relaxed) {
            RTCPeerConnectionState::Closed
        } else {
            RTCPeerConnectionState::New
        }
    }

    fn ice_connection_state(&self) -> RTCIceConnectionState {
        if self.closed.load(Ordering::Relaxed) {
            RTCIceConnectionState::Closed
        } else {
            RTCIceConnectionState::New
        }
    }

    fn ice_gathering_state(&self) -> RTCIceGatheringState {
        RTCIceGatheringState::New
    }

    fn signaling_state(&self) -> RTCSignalingState {
        if self.closed.load(Ordering::Relaxed) {
            RTCSignalingState::Closed
        } else {
            RTCSignalingState::Stable
        }
    }

    async fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}

// ponytail: manual Clone for AtomicBool-backed struct
impl Clone for StubPc {
    fn clone(&self) -> Self {
        Self {
            closed: AtomicBool::new(self.closed.load(Ordering::Relaxed)),
        }
    }
}

#[cfg(not(feature = "backend-webrtc-rs"))]
impl StubPc {
    pub(crate) async fn create_data_channel(
        &self,
        label: &str,
        _init: RTCDataChannelInit,
    ) -> Result<RTCDataChannel, RTCError> {
        Ok(RTCDataChannel {
            label: label.to_string(),
            id: 0,
            backend: StubDc,
        })
    }
}

// ── StubDc ──

#[derive(Debug, Default, Clone)]
pub(crate) struct StubDc;

impl DcBackend for StubDc {
    fn state(&self) -> RTCDataChannelState {
        RTCDataChannelState::Closed
    }

    async fn send(&self, _: &[u8]) -> Result<(), RTCError> {
        Ok(())
    }

    async fn send_text(&self, _: &str) -> Result<(), RTCError> {
        Ok(())
    }

    async fn spool(&self) -> RTCDataChannelRx {
        RTCDataChannelRx::stub()
    }

    async fn close(&mut self) {}
}

// ── StubTrack ──

#[derive(Debug, Default, Clone)]
pub(crate) struct StubTrack;

impl TrackWriteBackend for StubTrack {
    async fn write_frame(
        &self,
        data: &[u8],
        _kind: TrackKind,
        _audio_config: Option<&RTCAudioTrackConfig>,
    ) -> Result<(), RTCError> {
        tracing::debug!("TrackSender::write_frame (stub): {} bytes", data.len());
        Ok(())
    }
}

// ── StubFactory ──

#[derive(Debug, Default, Clone)]
pub(crate) struct StubFactory;

impl StubFactory {
    pub(crate) async fn create_peer_connection(
        &self,
        _config: RTCConfiguration,
    ) -> Result<StubPc, RTCError> {
        tracing::info!("Creating RTCPeerConnection (stub)");
        Ok(StubPc::default())
    }

    /// Create a stub video track — no-op.
    pub(crate) fn create_video_track(
        &self,
    ) -> (StubTrack, ()) {
        (StubTrack, ())
    }
}
