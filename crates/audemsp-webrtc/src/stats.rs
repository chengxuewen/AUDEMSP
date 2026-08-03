//! W3C RTCStats types — structured getStats return type.
//!
//! Ported from webrtc-kit rtc/core.rs.
//! Provides 5 core stat types matching the W3C WebRTC Stats API.

/// W3C RTCStats with 5 core stat types.
#[derive(Debug, Clone, serde::Serialize)]
pub enum RTCStats {
    RTCPeerConnection(RTCPeerConnectionStats),
    Transport(RTCTransportStats),
    Codec(RTCCodecStats),
    InboundRtp(RTCInboundRtpStreamStats),
    OutboundRtp(RTCOutboundRtpStreamStats),
}

/// Peer connection statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RTCPeerConnectionStats {
    pub id: String,
    pub timestamp: f64,
    pub data_channels_opened: u32,
    pub data_channels_closed: u32,
}

/// Transport-level statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RTCTransportStats {
    pub id: String,
    pub timestamp: f64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub dtls_state: Option<String>,
    pub selected_candidate_pair_id: Option<String>,
}

/// Codec statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RTCCodecStats {
    pub id: String,
    pub timestamp: f64,
    pub payload_type: u8,
    pub mime_type: String,
    pub clock_rate: u32,
    pub channels: Option<u16>,
}

/// Inbound RTP statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RTCInboundRtpStreamStats {
    pub id: String,
    pub timestamp: f64,
    pub ssrc: u32,
    pub kind: String,
    pub packets_received: u64,
    pub packets_lost: u64,
    pub bytes_received: u64,
    pub frames_decoded: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    pub frames_per_second: f64,
}

/// Outbound RTP statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RTCOutboundRtpStreamStats {
    pub id: String,
    pub timestamp: f64,
    pub ssrc: u32,
    pub kind: String,
    pub packets_sent: u64,
    pub bytes_sent: u64,
    pub frames_encoded: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    pub frames_per_second: f64,
}
