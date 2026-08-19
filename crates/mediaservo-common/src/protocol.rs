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

    /// v2 (web-stream-stats T1): Host 周期上报编码状态（room 广播 relay 到浏览器）。
    /// encoder_implementation: libwebrtc outbound-rtp 实际编码器名（软编/硬编识别）; None = 不可用。
    EncoderStatus {
        room_id: String,
        peer_id: String,
        codec: String,
        encoder_backend: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        encoder_implementation: Option<String>,
        frames_per_second: f64,
        frame_width: u32,
        frame_height: u32,
        /// v3 (encode-time-stats T1): 平均每帧编码耗时（ms/帧, Host 增量计算）—
        /// ΔtotalEncodeTime / ΔframesEncoded × 1000; None = 不可用（旧 host / 首周期）。
        #[serde(skip_serializing_if = "Option::is_none")]
        avg_encode_ms: Option<f64>,
    },


    /// v4 (E3 host-multiprocess): host-agent 整车状态上报 — 拓扑 + 数据流 + 信令
    /// 三快照聚合，经网关远端 WS 周期上报（默认 5s）。Server 直接消费存储
    /// （非 relay 消息，不广播房间；旧 Server 解析失败静默丢弃 = 可容忍，
    /// 周期性上报下一周期自愈）。
    StatusReport {
        room_id: String,
        /// 数据面: 各 camera topic 数据流统计（E2 快照）。
        topics: Vec<TopicFlowJson>,
        /// 数据面: 各 streamer 推流状态（E2 快照）。
        streams: Vec<StreamFlowJson>,
        /// 拓扑面: 期望 + 实际进程并集（E1 快照）。
        processes: Vec<ProcessStateJson>,
        /// 信令面: 网关视角连接状态（E3 快照）。
        signal: SignalStatusJson,
        /// 上报时刻（unix 秒）。
        ts: u64,
        /// host.toml 配置版本（E4 ConfigPush 关联；当前恒 0）。
        config_version: u64,
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
    /// v5 (E4 云端配置闭环): Server → host-agent 整车配置下发。
    /// config 为 host.toml 全文；target = 整车 peer_id（房间内其他 peer 忽略）；
    /// version 与 StatusReport.config_version 关联（agent 应用成功后回报）。
    ConfigPush {
        room_id: String,
        target: String,
        config: String,
        version: u64,
    },

    // ponytail: add frame ack/retransmit when reliability matters
}


/// E3 状态上报: 单 topic 数据流统计（wire 版，镜像 host monitor::flow::TopicFlow）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TopicFlowJson {
    pub topic: String,
    /// 窗口内帧率（<2 帧或窗口为零 → 0）。
    pub fps: f64,
    /// 窗口内字节率。
    pub bps: u64,
    /// 最近一帧发布端单调时间戳（ns；从未收到 → 0）。
    pub last_ts_mono_ns: u64,
    /// 窗口内收到帧数。
    pub frames: u64,
    /// 停滞（距最近到达超阈值；从未收到帧也视为停滞）。
    pub stalled: bool,
}

/// E3 状态上报: 单流推流状态（wire 版，镜像 host monitor::flow::StreamFlow）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamFlowJson {
    pub id: String,
    /// 最近一次 stats 的 bytes_sent（webrtc OutboundRtp，累计）。
    pub bytes_sent: u64,
    /// 最近一次 stats 的 frames_encoded（libwebrtc u32，累计）。
    pub frames_encoded: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    /// 最近 stats 是否在新鲜窗口内。
    pub connected: bool,
}

/// E3 状态上报: 单进程拓扑状态（期望 + 实际并集；running = oxmgr running）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcessStateJson {
    pub name: String,
    pub running: bool,
    /// host.toml 期望进程（实际发现的非期望进程 = false）。
    pub expected: bool,
}

/// E3 状态上报: 信令平面（网关视角）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalStatusJson {
    /// 远端 server WS 是否已连接并入房。
    pub remote_connected: bool,
    /// 本次远端会话建立至今秒数（未连接 = None）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_since_secs: Option<u64>,
    /// 整车 peer_id（未连接 = 空串）。
    pub remote_peer_id: String,
    /// 本地子进程 WS 连接列表。
    pub children: Vec<ChildSignalJson>,
    /// host-agent 启动至今秒数。
    pub agent_uptime_secs: u64,
}

/// E3 状态上报: 单子进程 WS 连接。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChildSignalJson {
    /// 子进程标识（LocalEnvelope.src）。
    pub src: String,
    /// 连接中（快照仅含在途连接，恒 true；字段保留供 H 阶段渲染）。
    pub connected: bool,
    /// 距最近一条上行消息的秒数（0 = 刚收到；u64::MAX = 未发过消息）。
    pub last_msg_secs: u64,
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

    #[test]
    fn roundtrip_config_push() {
        let msg = SignalingMessage::ConfigPush {
            room_id: "vehicle-1".into(),
            target: "veh-peer".into(),
            config: "[[cameras]]\nid = \"cam0\"\n".into(),
            version: 7,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"config_push""#));
        assert!(json.contains(r#""version":7"#));
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            SignalingMessage::ConfigPush { room_id, target, config, version } => {
                assert_eq!(room_id, "vehicle-1");
                assert_eq!(target, "veh-peer");
                assert!(config.contains("cam0"));
                assert_eq!(version, 7);
            }
            other => panic!("expected ConfigPush, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_status_report() {
        let msg = SignalingMessage::StatusReport {
            room_id: "vehicle-1".into(),
            topics: vec![TopicFlowJson {
                topic: "camera/cam0".into(),
                fps: 29.7,
                bps: 800_000,
                last_ts_mono_ns: 1_234_567_890,
                frames: 148,
                stalled: false,
            }],
            streams: vec![StreamFlowJson {
                id: "cam0".into(),
                bytes_sent: 42_000_000,
                frames_encoded: 21_000,
                frame_width: 1280,
                frame_height: 720,
                connected: true,
            }],
            processes: vec![
                ProcessStateJson { name: "host-agent".into(), running: true, expected: true },
                ProcessStateJson { name: "host-capturer-cam0".into(), running: false, expected: true },
            ],
            signal: SignalStatusJson {
                remote_connected: true,
                remote_since_secs: Some(120),
                remote_peer_id: "veh-peer".into(),
                children: vec![ChildSignalJson {
                    src: "host-streamer".into(),
                    connected: true,
                    last_msg_secs: 1,
                }],
                agent_uptime_secs: 3600,
            },
            ts: 1_700_000_000,
            config_version: 0,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"status_report""#));
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            SignalingMessage::StatusReport {
                room_id,
                topics,
                streams,
                processes,
                signal,
                ts,
                config_version,
            } => {
                assert_eq!(room_id, "vehicle-1");
                assert_eq!(topics[0].topic, "camera/cam0");
                assert_eq!(topics[0].frames, 148);
                assert_eq!(streams[0].frames_encoded, 21_000);
                assert_eq!(processes[1].running, false);
                assert_eq!(processes[1].expected, true);
                assert_eq!(signal.remote_peer_id, "veh-peer");
                assert_eq!(signal.children[0].src, "host-streamer");
                assert_eq!(ts, 1_700_000_000);
                assert_eq!(config_version, 0);
            }
            _ => panic!("expected StatusReport"),
        }
    }

}
