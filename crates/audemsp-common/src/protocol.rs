//! Signaling protocol message types.
//!
//! All signaling messages flow through the Server's WebSocket /ws endpoint
//! as JSON. Server relays messages between Host and Remote without modification
//! (except for room management messages).

use serde::{Deserialize, Serialize};

/// A signaling message exchanged via WebSocket.
///
/// # Flow
/// ```text
/// Host ──WS──▶ Server ──WS──▶ Remote
/// Remote ──WS──▶ Server ──WS──▶ Host
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalingMessage {
    /// Request to join a room. Sent by Host or Remote to Server.
    RoomJoin {
        room_id: String,
        peer_role: PeerRole,
        #[serde(skip_serializing_if = "Option::is_none")]
        stream_id: Option<String>,
    },

    /// Room join acknowledged by Server.
    RoomJoined {
        room_id: String,
        peer_id: String,
    },

    /// A peer has left the room. Broadcast by Server.
    RoomLeave {
        room_id: String,
        peer_id: String,
    },

    /// SDP offer/answer relayed through Server.
    Sdp {
        room_id: String,
        target: Option<String>,
        sdp: String,
    },

    /// ICE candidate relayed through Server.
    RTCIceCandidate {
        room_id: String,
        target: Option<String>,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u16>,
    },

    // ── SFU transport negotiation (mediasoup) ────────────────────

    /// Request Server to create a WebRTC transport for this peer.
    /// The Server (SFU) creates the transport and returns parameters.
    CreateWebRtcTransport {
        room_id: String,
        peer_id: String,
        direction: TransportDirection,
    },

    /// Server responds with transport parameters needed by the client.
    WebRtcTransportCreated {
        room_id: String,
        peer_id: String,
        transport_id: String,
        ice_parameters: IceParameters,
        dtls_parameters: DtlsParameters,
        /// ICE candidates for the transport (None for backward compat).
        #[serde(skip_serializing_if = "Option::is_none")]
        ice_candidates: Option<Vec<IceCandidate>>,
    },

    /// Client sends back DTLS parameters to connect the transport.
    ConnectWebRtcTransport {
        room_id: String,
        peer_id: String,
        transport_id: String,
        dtls_parameters: DtlsParameters,
    },

    /// Error response from Server.
    Error {
        code: u16,
        message: String,
    },

    /// Encoded media frame relayed through Server.
    /// data_base64 is encoded as base64 (JSON-safe).
    Frame {
        room_id: String,
        codec: String,
        sequence: u64,
        is_keyframe: bool,
        data_base64: String,
    },

    // ── SFU produce/consume (mediasoup) ─────────────────────────

    /// Peer asks to produce media on its send transport.
    /// rtp_parameters is opaque JSON — server passes it through to mediasoup.
    Produce {
        room_id: String,
        peer_id: String,
        transport_direction: TransportDirection,
        kind: MediaKind,
        rtp_parameters: serde_json::Value,
    },

    /// Server confirms producer created.
    Produced {
        room_id: String,
        producer_id: String,
    },

    /// Server broadcasts a new producer to all peers in the room.
    NewProducer {
        room_id: String,
        producer_id: String,
        peer_id: String,
        kind: MediaKind,
    },

    /// Peer asks to consume a producer on its recv transport.
    /// rtp_capabilities is opaque JSON — server passes it through to mediasoup.
    Consume {
        room_id: String,
        peer_id: String,
        producer_id: String,
        rtp_capabilities: serde_json::Value,
    },

    /// Server confirms consumer created.
    Consumed {
        room_id: String,
        consumer_id: String,
        producer_id: String,
        kind: MediaKind,
        /// RTP parameters needed by the consumer to decode the stream.
        rtp_parameters: serde_json::Value,
    },
    // ponytail: add frame ack/retransmit when reliability matters
}

/// Direction of a WebRTC transport (send-only or recv-only).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportDirection {
    Send,
    Recv,
}

/// ICE parameters returned after WebRTC transport creation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IceParameters {
    pub username_fragment: String,
    pub password: String,
}

/// DTLS parameters for transport connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DtlsParameters {
    pub fingerprints: Vec<Fingerprint>,
    /// "auto" | "client" | "server"
    pub role: String,
}

/// A DTLS fingerprint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Fingerprint {
    /// e.g. "sha-256"
    pub algorithm: String,
    /// hex-encoded fingerprint value
    pub value: String,
}

/// An ICE candidate for WebRTC transport connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IceCandidate {
    /// IP address of the candidate.
    pub ip: String,
    /// Port of the candidate.
    pub port: u16,
    /// Transport protocol ("udp" or "tcp").
    pub protocol: String,
    /// Unique identifier for the candidate.
    pub foundation: String,
    /// Assigned priority of the candidate.
    pub priority: u32,
    /// Type of candidate ("host", "srflx", "prflx", "relay").
    pub candidate_type: String,
}

/// Role of a peer in a room.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PeerRole {
    Host,
    Remote,
    Consumer,
}

/// Media kind for produce/consume.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Audio,
    Video,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_room_join() {
        let msg = SignalingMessage::RoomJoin {
            room_id: "room-1".into(),
            peer_role: PeerRole::Host,
            stream_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"room_join""#));
        assert!(json.contains(r#""room_id":"room-1""#));
        assert!(json.contains(r#""peer_role":"host""#));
        // stream_id: None is absent from JSON (serde skips None by default)
        assert!(!json.contains("stream_id"));
    }

    #[test]
    fn roundtrip_room_join_with_stream_id() {
        let msg = SignalingMessage::RoomJoin {
            room_id: "room-1".into(),
            peer_role: PeerRole::Consumer,
            stream_id: Some("stream-42".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"room_join""#));
        assert!(json.contains(r#""peer_role":"consumer""#));
        assert!(json.contains(r#""stream_id":"stream-42""#));

        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            SignalingMessage::RoomJoin { room_id, peer_role, stream_id } => {
                assert_eq!(room_id, "room-1");
                assert_eq!(peer_role, PeerRole::Consumer);
                assert_eq!(stream_id.as_deref(), Some("stream-42"));
            }
            _ => panic!("expected RoomJoin"),
        }
    }

    #[test]
    fn serialize_error() {
        let msg = SignalingMessage::Error {
            code: 4003,
            message: "PSK authentication failed".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"error""#));
        assert!(json.contains("4003"));
    }

    #[test]
    fn roundtrip_ice_candidate() {
        let msg = SignalingMessage::RTCIceCandidate {
            room_id: "r1".into(),
            target: None,
            candidate: "candidate:1 1 UDP 2130706431 10.0.0.1 8000 typ host".into(),
            sdp_mid: Some("0".into()),
            sdp_mline_index: Some(0),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SignalingMessage::RTCIceCandidate { .. }));
    }

    #[test]
    fn roundtrip_room_joined() {
        let msg = SignalingMessage::RoomJoined {
            room_id: "room-42".into(),
            peer_id: "peer-7".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"room_joined""#));
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            SignalingMessage::RoomJoined { room_id, peer_id } => {
                assert_eq!(room_id, "room-42");
                assert_eq!(peer_id, "peer-7");
            }
            _ => panic!("expected RoomJoined"),
        }
    }

    #[test]
    fn roundtrip_room_leave() {
        let msg = SignalingMessage::RoomLeave {
            room_id: "room-99".into(),
            peer_id: "peer-3".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"room_leave""#));
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            SignalingMessage::RoomLeave { room_id, peer_id } => {
                assert_eq!(room_id, "room-99");
                assert_eq!(peer_id, "peer-3");
            }
            _ => panic!("expected RoomLeave"),
        }
    }

    #[test]
    fn roundtrip_sdp() {
        let msg = SignalingMessage::Sdp {
            room_id: "room-1".into(),
            target: Some("peer-a".into()),
            sdp: "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"sdp""#));
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            SignalingMessage::Sdp { room_id, target, sdp } => {
                assert_eq!(room_id, "room-1");
                assert_eq!(target.as_deref(), Some("peer-a"));
                assert!(sdp.starts_with("v=0"));
            }
            _ => panic!("expected Sdp"),
        }
    }

    #[test]
    fn roundtrip_sdp_without_target() {
        let msg = SignalingMessage::Sdp {
            room_id: "room-x".into(),
            target: None,
            sdp: "v=0".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            SignalingMessage::Sdp { target, .. } => {
                assert!(target.is_none());
            }
            _ => panic!("expected Sdp"),
        }
    }

    #[test]
    fn peer_role_host_serde() {
        let json = serde_json::to_string(&PeerRole::Host).unwrap();
        assert_eq!(json, r#""host""#);
        let parsed: PeerRole = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, PeerRole::Host);
    }

    #[test]
    fn peer_role_remote_serde() {
        let json = serde_json::to_string(&PeerRole::Remote).unwrap();
        assert_eq!(json, r#""remote""#);
        let parsed: PeerRole = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, PeerRole::Remote);
    }

    #[test]
    fn peer_role_consumer_serde() {
        let json = serde_json::to_string(&PeerRole::Consumer).unwrap();
        assert_eq!(json, r#""consumer""#);
        let parsed: PeerRole = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, PeerRole::Consumer);
    }

    #[test]
    fn deserialize_unknown_type() {
        let json = r#"{"type":"unknown_kind","room_id":"x"}"#;
        let result: Result<SignalingMessage, _> = serde_json::from_str(json);
        assert!(result.is_err(), "unknown type should fail deserialization");
    }

    #[test]
    fn deserialize_missing_required_field() {
        let json = r#"{"type":"error","message":"oops"}"#;
        // Error variant requires both code and message
        let result: Result<SignalingMessage, _> = serde_json::from_str(json);
        assert!(result.is_err(), "missing 'code' field should fail");
    }

    #[test]
    fn deserialize_bad_peer_role() {
        let json = r#""invalid_role""#;
        let result: Result<PeerRole, _> = serde_json::from_str(json);
        assert!(result.is_err(), "invalid role should fail deserialization");
    }

    #[test]
    fn roundtrip_create_webrtc_transport() {
        let msg = SignalingMessage::CreateWebRtcTransport {
            room_id: "room-1".into(),
            peer_id: "peer-a".into(),
            direction: TransportDirection::Send,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"create_web_rtc_transport""#));
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SignalingMessage::CreateWebRtcTransport { .. }));
    }

    #[test]
    fn roundtrip_webrtc_transport_created() {
        let msg = SignalingMessage::WebRtcTransportCreated {
            room_id: "room-1".into(),
            peer_id: "peer-a".into(),
            transport_id: "transport-1".into(),
            ice_parameters: IceParameters {
                username_fragment: "ufrag".into(),
                password: "pwd".into(),
            },
            dtls_parameters: DtlsParameters {
                fingerprints: vec![Fingerprint {
                    algorithm: "sha-256".into(),
                    value: "AA:BB:CC".into(),
                }],
                role: "auto".into(),
            },
            ice_candidates: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"web_rtc_transport_created""#));
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SignalingMessage::WebRtcTransportCreated { .. }));
    }

    #[test]
    fn roundtrip_connect_webrtc_transport() {
        let msg = SignalingMessage::ConnectWebRtcTransport {
            room_id: "room-1".into(),
            peer_id: "peer-a".into(),
            transport_id: "transport-1".into(),
            dtls_parameters: DtlsParameters {
                fingerprints: vec![Fingerprint {
                    algorithm: "sha-256".into(),
                    value: "DD:EE:FF".into(),
                }],
                role: "client".into(),
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"connect_web_rtc_transport""#));
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SignalingMessage::ConnectWebRtcTransport { .. }));
    }

    #[test]
    fn roundtrip_media_kind() {
        assert_eq!(serde_json::to_string(&MediaKind::Audio).unwrap(), r#""audio""#);
        assert_eq!(serde_json::to_string(&MediaKind::Video).unwrap(), r#""video""#);
        let kind: MediaKind = serde_json::from_str(r#""audio""#).unwrap();
        assert_eq!(kind, MediaKind::Audio);
    }

    #[test]
    fn roundtrip_produce() {
        let msg = SignalingMessage::Produce {
            room_id: "room-1".into(),
            peer_id: "peer-1".into(),
            transport_direction: TransportDirection::Send,
            kind: MediaKind::Video,
            rtp_parameters: serde_json::json!({"codecs": [{"mimeType": "video/VP8"}]}),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"produce""#));
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            SignalingMessage::Produce { room_id, kind, .. } => {
                assert_eq!(room_id, "room-1");
                assert_eq!(kind, MediaKind::Video);
            }
            _ => panic!("expected Produce"),
        }
    }

    #[test]
    fn roundtrip_produced() {
        let msg = SignalingMessage::Produced {
            room_id: "room-1".into(),
            producer_id: "prod-1".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"produced""#));
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SignalingMessage::Produced { .. }));
    }

    #[test]
    fn roundtrip_new_producer() {
        let msg = SignalingMessage::NewProducer {
            room_id: "room-1".into(),
            producer_id: "prod-1".into(),
            peer_id: "peer-a".into(),
            kind: MediaKind::Audio,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"new_producer""#));
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SignalingMessage::NewProducer { .. }));
    }

    #[test]
    fn roundtrip_consume() {
        let msg = SignalingMessage::Consume {
            room_id: "room-1".into(),
            peer_id: "peer-1".into(),
            producer_id: "prod-1".into(),
            rtp_capabilities: serde_json::json!({"codecs": [{"mimeType": "video/VP8"}]}),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"consume""#));
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SignalingMessage::Consume { .. }));
    }

    #[test]
    fn roundtrip_consumed() {
        let msg = SignalingMessage::Consumed {
            room_id: "room-1".into(),
            consumer_id: "cons-1".into(),
            producer_id: "prod-1".into(),
            kind: MediaKind::Video,
            rtp_parameters: serde_json::json!({"codecs": [{"mimeType": "video/VP8"}]}),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"consumed""#));
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, SignalingMessage::Consumed { .. }));
    }
}
