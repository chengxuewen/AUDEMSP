//! Backend abstraction layer for multi-backend WebRTC support.
//!
//! Defines traits (PcBackend, DcBackend, TrackWriteBackend)
//! and compile-time type alias dispatch via cfg gates.
//! Zero dyn overhead — all dispatch is monomorphized.

use crate::data_channel::{RTCDataChannel, RTCDataChannelRx, RTCDataChannelState};
use crate::peer_connection::{RTCAnswerOptions, RTCIceCandidate, RTCOfferOptions, RTCIceConnectionState, RTCIceGatheringState, RTCPeerConnectionState, RTCSignalingState};
use crate::sdp::RTCSessionDescription;
use crate::stats::RTCStats;
use crate::track::{RTCAudioTrackConfig, TrackKind, TrackReceiver};
use crate::RTCError;

// ── Traits ──

pub(crate) trait PcBackend: Send + Sync + 'static {
    async fn create_offer(&self, options: &RTCOfferOptions) -> Result<RTCSessionDescription, RTCError>;
    async fn create_answer(&self, options: &RTCAnswerOptions) -> Result<RTCSessionDescription, RTCError>;
    async fn set_local_description(&self, desc: &RTCSessionDescription) -> Result<(), RTCError>;
    async fn set_remote_description(&self, desc: &RTCSessionDescription) -> Result<(), RTCError>;
    async fn add_ice_candidate(&self, candidate: &RTCIceCandidate) -> Result<(), RTCError>;
    fn connection_state(&self) -> RTCPeerConnectionState;
    fn ice_connection_state(&self) -> RTCIceConnectionState;
    fn ice_gathering_state(&self) -> RTCIceGatheringState;
    fn signaling_state(&self) -> RTCSignalingState;
    async fn close(&self);

    // ── Default methods (no-op for backends that skip these) ──

    /// Register callback for incoming data channels (receiver side).
    fn set_on_data_channel(&self, _cb: Box<dyn Fn(RTCDataChannel) + Send + Sync + 'static>) {}

    /// Register callback for incoming remote tracks (receiver side).
    fn set_on_track(&self, _cb: Box<dyn Fn(TrackReceiver) + Send + Sync + 'static>) {}

    /// Wait until ICE gathering is complete.
    fn gather_complete(&self) -> Result<(), RTCError> {
        Ok(())
    }

    /// Get structured statistics.
    fn get_stats(&self) -> Vec<RTCStats> {
        vec![]
    }

    /// Add an RTP transceiver for a media type with a given direction.
    // ponytail: String params until enums are justified.
    fn add_transceiver(&self, _media_type: &str, _direction: &str) -> Result<(), RTCError> {
        Err(RTCError::Internal("not supported".into()))
    }

    /// Register a local track with the RTCPeerConnection for RTP transmission.
    /// Backends that support track registration (webrtc-sys) call into
    /// libwebrtc to activate the track. Other backends store in the wrapper.
    fn register_track(
        &self, _track_id: &str, _kind: TrackKind,
    ) -> Result<(), RTCError> {
        Ok(())
    }

    /// Register callback for trickled local ICE candidates (P2P 需要完整转发).
    /// 默认空实现（webrtc-rs 后端如有需要自行实现）。
    fn set_on_ice_candidate(
        &self,
        _cb: Box<dyn Fn(RTCIceCandidate) + Send + Sync + 'static>,
    ) {
    }
    /// Register callback for ICE connection state changes (monitoring).
    fn set_on_ice_connection_state_change(
        &self,
        _cb: Box<dyn Fn(RTCIceConnectionState) + Send + Sync + 'static>,
    ) {
    }

    /// Register callback for peer connection state changes (monitoring).
    fn set_on_peer_connection_state_change(
        &self,
        _cb: Box<dyn Fn(RTCPeerConnectionState) + Send + Sync + 'static>,
    ) {
    }

    /// Return the local SDP string if available.
    fn local_description_sdp(&self) -> Option<String> {
        None
    }
}

pub(crate) trait DcBackend: Send + Sync + 'static {
    fn state(&self) -> RTCDataChannelState;
    async fn send(&self, data: &[u8]) -> Result<(), RTCError>;
    async fn send_text(&self, text: &str) -> Result<(), RTCError>;
    async fn spool(&self) -> RTCDataChannelRx;
    async fn close(&mut self);
}

pub trait TrackWriteBackend: Send + Sync + 'static {
    async fn write_frame(
        &self,
        data: &[u8],
        kind: TrackKind,
        audio_config: Option<&RTCAudioTrackConfig>,
    ) -> Result<(), RTCError>;

    /// Write a raw I420 (YUV 4:2:0 planar) frame to the video track.
    /// The backend handles encoding (webrtc-sys built-in x264) or no-ops (stub, webrtc-rs).
    /// For webrtc-rs: use audemsp-codec crate to encode I420→H.264, then write_frame().
    ///
    /// `data` layout: Y plane (w*h) + U plane (w*h/4) + V plane (w*h/4).
    async fn write_raw_i420(
        &self, _data: &[u8], _width: u32, _height: u32,
    ) -> Result<(), RTCError> {
        Ok(())
    }
}

// ── Mutual exclusion guard ──

#[cfg(all(feature = "backend-webrtc-rs", feature = "backend-webrtc-sys"))]
compile_error!("Only one backend can be enabled at a time. Choose either backend-webrtc-rs or backend-webrtc-sys.");

// ── Module declarations ──

#[cfg(not(any(feature = "backend-webrtc-rs", feature = "backend-webrtc-sys")))]
pub(crate) mod stub;
#[cfg(feature = "backend-webrtc-rs")]
pub(crate) mod webrtc_rs;
#[cfg(feature = "backend-webrtc-sys")]
pub mod webrtc_sys;
// ── Type alias dispatch (compile-time, monomorphized) ──


#[cfg(feature = "backend-webrtc-rs")]
pub type ActivePc = webrtc_rs::WebrtcRsPc;
#[cfg(feature = "backend-webrtc-sys")]
pub type ActivePc = webrtc_sys::WebrtcSysPc;
#[cfg(not(any(feature = "backend-webrtc-rs", feature = "backend-webrtc-sys")))]
pub type ActivePc = stub::StubPc;

#[cfg(feature = "backend-webrtc-rs")]
pub(crate) type ActiveDc = webrtc_rs::WebrtcRsDc;
#[cfg(feature = "backend-webrtc-sys")]
pub(crate) type ActiveDc = webrtc_sys::WebrtcSysDc;
#[cfg(not(any(feature = "backend-webrtc-rs", feature = "backend-webrtc-sys")))]
pub(crate) type ActiveDc = stub::StubDc;

#[cfg(feature = "backend-webrtc-rs")]
pub type ActiveTrack = webrtc_rs::WebrtcRsTrack;
#[cfg(feature = "backend-webrtc-sys")]
pub type ActiveTrack = webrtc_sys::WebrtcSysTrack;
#[cfg(not(any(feature = "backend-webrtc-rs", feature = "backend-webrtc-sys")))]
pub type ActiveTrack = stub::StubTrack;

#[cfg(feature = "backend-webrtc-rs")]
pub(crate) type ActiveFactory = webrtc_rs::WebrtcRsFactory;
#[cfg(feature = "backend-webrtc-sys")]
pub type ActiveFactory = webrtc_sys::WebrtcSysFactory;
#[cfg(not(any(feature = "backend-webrtc-rs", feature = "backend-webrtc-sys")))]
pub(crate) type ActiveFactory = stub::StubFactory;
