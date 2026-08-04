//! W3C PeerConnectionApi trait — the public W3C WebRTC API contract.
//!
//! This trait defines the standard W3C PeerConnection interface.
//! Currently implemented by `crate::peer_connection::RTCPeerConnection` struct.
//!
//! # Backend dispatch
//!
//! The trait is backend-agnostic. Backend selection happens at compile time
//! via `ActivePc` type alias in `crate::backend`.
//!
//! # Usage
//!
//! ```ignore
//! use audemsp_webrtc::traits::PeerConnectionApi;
//!
//! fn use_pc(pc: &impl PeerConnectionApi) {
//!     pc.create_offer(&Default::default()).await.unwrap();
//! }
//! ```

use crate::peer_connection::{
    RTCAnswerOptions, RTCIceCandidate, RTCOfferOptions,
    RTCIceConnectionState, RTCIceGatheringState, RTCPeerConnectionState, RTCSignalingState,
};
use crate::sdp::RTCSessionDescription;
use crate::data_channel::{RTCDataChannel, RTCDataChannelInit};
use crate::track::{TrackKind, TrackRef};
use crate::rtp::{RTCRtpSender, RTCRtpReceiver};
use crate::RTCError;

/// W3C WebRTC RTCPeerConnection interface (D146).
///
/// Provides the standard W3C methods: SDP negotiation, ICE management,
/// DataChannel creation, track management, and event callbacks.
///
/// Each backend (webrtc-sys, webrtc-rs, stub) provides an implementation
/// via compile-time `ActivePc` type alias dispatch.
pub trait PeerConnectionApi: Send + Sync + 'static {
    /// Create an SDP offer for initiating a new connection.
    async fn create_offer(&self, options: &RTCOfferOptions) -> Result<RTCSessionDescription, RTCError>;

    /// Create an SDP answer in response to an offer.
    async fn create_answer(&self, options: &RTCAnswerOptions) -> Result<RTCSessionDescription, RTCError>;

    /// Set the local session description (offer or answer).
    async fn set_local_description(&self, desc: &RTCSessionDescription) -> Result<(), RTCError>;

    /// Set the remote session description (offer or answer from peer).
    async fn set_remote_description(&self, desc: &RTCSessionDescription) -> Result<(), RTCError>;

    /// Add a remote ICE candidate received from the signaling channel.
    async fn add_ice_candidate(&self, candidate: &RTCIceCandidate) -> Result<(), RTCError>;

    /// Create a data channel for sending/receiving arbitrary data.
    async fn create_data_channel(&self, label: &str, init: RTCDataChannelInit) -> Result<RTCDataChannel, RTCError>;

    /// Current state of the peer connection.
    fn connection_state(&self) -> RTCPeerConnectionState;

    /// Current state of the ICE connection.
    fn ice_connection_state(&self) -> RTCIceConnectionState;

    /// Current state of ICE gathering.
    fn ice_gathering_state(&self) -> RTCIceGatheringState;

    /// Current signaling state.
    fn signaling_state(&self) -> RTCSignalingState;

    /// Close the peer connection.
    async fn close(&self);

    /// Register a local media track for RTP transmission.
    /// Returns the track ID on success (max 8 tracks per connection).
    fn add_track(&self, track_id: &str, kind: TrackKind) -> Result<String, RTCError>;

    /// Remove a previously registered track.
    fn remove_track(&self, track_id: &str) -> Result<(), RTCError>;

    /// Get a track reference by ID.
    fn get_track(&self, track_id: &str) -> Option<TrackRef>;

    /// Number of registered tracks.
    fn track_count(&self) -> usize;

    /// IDs of all registered tracks.
    fn track_ids(&self) -> Vec<String>;

    /// Get all sender (outgoing) tracks as RTCRtpSender objects.
    fn get_senders(&self) -> Vec<RTCRtpSender>;

    /// Get all receiver (incoming) tracks as RTCRtpReceiver objects.
    fn get_receivers(&self) -> Vec<RTCRtpReceiver>;

    /// Register a callback for incoming remote tracks.
    /// The callback receives an RTCRtpReceiver when a remote track is added.
    fn on_track<F>(&self, callback: F) where F: Fn(RTCRtpReceiver) + Send + Sync + 'static;
}
