
use crate::track::{TrackKind, TrackRef};

/// W3C RTCRtpSender — wraps a TrackRef::Sender with sender metadata (D146).
#[derive(Debug, Clone)]
pub struct RTCRtpSender {
    pub track: TrackRef,
    pub track_id: String,
    pub kind: TrackKind,
}

impl RTCRtpSender {
    pub fn new(track: TrackRef) -> Self {
        let track_id = track.id().to_string();
        let kind = track.kind();
        Self {
            track,
            track_id,
            kind,
        }
    }
}

/// W3C RTCRtpReceiver — wraps a TrackRef::Receiver with receiver metadata (D146).
#[derive(Debug, Clone)]
pub struct RTCRtpReceiver {
    pub track: TrackRef,
    pub track_id: String,
    pub kind: TrackKind,
}

impl RTCRtpReceiver {
    pub fn new(track: TrackRef) -> Self {
        let track_id = track.id().to_string();
        let kind = track.kind();
        Self {
            track,
            track_id,
            kind,
        }
    }
}


/// W3C RTCRtpCodecParameters
#[derive(Debug, Clone)]
pub struct RTCRtpCodecParameters {
    pub mime_type: String, // "video/H264", "video/VP8", etc.
    pub payload_type: u8,
    pub clock_rate: u32,
    pub channels: Option<u16>, // for audio
    pub sdp_fmtp_line: Option<String>,
}

/// W3C RTCRtpEncodingParameters
#[derive(Debug, Clone)]
pub struct RTCRtpEncodingParameters {
    pub ssrc: Option<u64>,
    pub active: bool,
    pub max_bitrate: Option<u64>,
    pub max_framerate: Option<f64>,
    pub scale_resolution_down_by: Option<f64>,
    pub rid: Option<String>,
}

impl Default for RTCRtpEncodingParameters {
    fn default() -> Self {
        Self {
            ssrc: None,
            active: true,
            max_bitrate: None,
            max_framerate: None,
            scale_resolution_down_by: None,
            rid: None,
        }
    }
}

/// W3C RTCRtpHeaderExtensionParameters
#[derive(Debug, Clone)]
pub struct RTCRtpHeaderExtensionParameters {
    pub uri: String,
    pub id: u16,
    pub encrypted: bool,
}

/// W3C RTCRtcpParameters
#[derive(Debug, Clone, Default)]
pub struct RTCRtcpParameters {
    pub cname: Option<String>,
    /// When true, indicates reduced-size RTCP.
    #[allow(dead_code)]
    pub reduced_size: bool,
}

/// W3C RTCRtpParameters
#[derive(Debug, Clone, Default)]
pub struct RTCRtpParameters {
    pub transaction_id: String,
    pub codecs: Vec<RTCRtpCodecParameters>,
    pub encodings: Vec<RTCRtpEncodingParameters>,
    pub header_extensions: Vec<RTCRtpHeaderExtensionParameters>,
    pub rtcp: RTCRtcpParameters,
}
