//! mediasoup SFU foundation.
//!
//! Provides `SfuManager` (global), `SfuRoom` (per-room Router + peers),
//! and `SfuPeer` (per-peer transports + producers/consumers).
//!
//! Only compiled when the `sfu-mediasoup` feature is enabled.

// ── Feature-gated imports ───────────────────────────────────────────────

#[cfg(feature = "sfu-mediasoup")]
use dashmap::DashMap;
use mediaservo_common::protocol;

#[cfg(feature = "sfu-mediasoup")]
mod imp {
    use super::*;
    use mediasoup::prelude::*;
    use mediasoup::worker_manager::WorkerManager;
    use mediasoup::webrtc_server::{WebRtcServer, WebRtcServerOptions, WebRtcServerListenInfos};
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::Arc;
    use std::num::{NonZeroU32, NonZeroU8};

    /// Detect the container's primary IP (zero-dependency UDP connect trick;
    /// connect() on UDP only sets the default route target, no packet is sent).
    fn detect_local_ip() -> String {
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if socket.connect("8.8.8.8:80").is_ok() {
                if let Ok(addr) = socket.local_addr() {
                    return addr.ip().to_string();
                }
            }
        }
        "0.0.0.0".to_string()
    }

    /// PIT-58: announced_address 解析 — 环境变量 MEDIASERVO_SFU_ANNOUNCED_IP，
    /// 支持逗号分隔多 IP（宿主多网卡）。fallback: 容器内探测（172.18.0.2 仅本机可用，
    /// 部署方应经 CLI/外部注入真实宿主 IP）。
    fn announced_ips_from_env() -> Vec<String> {
        let raw = std::env::var("MEDIASERVO_SFU_ANNOUNCED_IP").unwrap_or_else(|_| detect_local_ip());
        raw.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Create RouterOptions with sensible default codecs (Opus + VP8 + H264).
    fn default_router_options() -> RouterOptions {
        RouterOptions::new(vec![
            RtpCodecCapability::Audio {
                mime_type: MimeTypeAudio::Opus,
                preferred_payload_type: Some(111),  // Opus 显式（防与视频冲突）
                clock_rate: NonZeroU32::new(48000).unwrap(),
                channels: NonZeroU8::new(2).unwrap(),
                parameters: RtpCodecParametersParameters::default(),
                rtcp_feedback: vec![],
            },
            RtpCodecCapability::Video {
                mime_type: MimeTypeVideo::Vp8,
                preferred_payload_type: Some(96),  // VP8 显式（防与 H264 101 冲突）
                clock_rate: NonZeroU32::new(90000).unwrap(),
                parameters: RtpCodecParametersParameters::default(),
                rtcp_feedback: vec![],
            },
            RtpCodecCapability::Video {
                mime_type: MimeTypeVideo::H264,
                preferred_payload_type: Some(101),  // PIT-51: 与 Host produce 的 payloadType 101 匹配（None=自动分配≠101 → produce 失败）
                clock_rate: NonZeroU32::new(90000).unwrap(),
                parameters: RtpCodecParametersParameters::from([
                    ("level-asymmetry-allowed", 1_u32.into()),
                    ("packetization-mode", 1_u32.into()),
                    ("profile-level-id", "42e01f".into()), // v2: 4d0032→42e01f（浏览器解码兼容性, encoder-backend-codec-config T7 实证）
                ]),
                rtcp_feedback: vec![],
            },
            // v2 (2026-08-11): 用户要求 264/265/vp8/vp9/av1 — VP9/AV1 启用;
            // H265 不可用: mediasoup-rs 0.24 MimeTypeVideo 无 H265 绑定（worker 不支持, 需新版/自定义）
            RtpCodecCapability::Video {
                mime_type: MimeTypeVideo::Vp9,
                preferred_payload_type: Some(99),  // 防冲突: 96/101 已用, 100 被 mediasoup 自动分配占用
                clock_rate: NonZeroU32::new(90000).unwrap(),
                parameters: RtpCodecParametersParameters::default(),
                rtcp_feedback: vec![],
            },
            RtpCodecCapability::Video {
                mime_type: MimeTypeVideo::AV1,
                preferred_payload_type: Some(97),  // 池尾动态 PT（96/99/101 已用; 100-102 池首与 mediasoup 分配冲突, 实测）
                clock_rate: NonZeroU32::new(90000).unwrap(),
                parameters: RtpCodecParametersParameters::default(),
                rtcp_feedback: vec![],
            },
        ])
    }
    /// Result of a transport creation request.
    pub struct TransportCreated {
        pub transport_id: String,
        pub ice_parameters: protocol::IceParameters,
        pub dtls_parameters: protocol::DtlsParameters,
        pub ice_candidates: Vec<protocol::IceCandidate>,
    }

    /// Result of a producer creation request.
    pub struct ProduceResult {
        pub producer_id: String,
        pub kind: protocol::MediaKind,
    }

    /// Result of a consumer creation request.
    pub struct ConsumeResult {
        pub consumer_id: String,
        pub producer_id: String,
        pub kind: protocol::MediaKind,
        pub rtp_parameters_json: serde_json::Value,
    }

    /// Per-peer state: send/recv transports and active producers/consumers.
    pub struct SfuPeer {
        pub send_transport: Option<WebRtcTransport>,
        pub recv_transport: Option<WebRtcTransport>,
        pub producers: Vec<Producer>,
        pub consumers: Vec<Consumer>,
    }

    /// Per-room SFU state: one Router, all connected peers.
    pub struct SfuRoom {
        pub router: Arc<Router>,
        pub peers: DashMap<String, SfuPeer>,
    }

    /// Global SFU manager — owns WorkerManager, maps room_id → SfuRoom.
    #[allow(dead_code)]
    pub struct SfuManager {
        worker_manager: WorkerManager,
        worker: Worker,
        webrtc_server: Arc<WebRtcServer>,
        rooms: DashMap<String, SfuRoom>,
    }

    /// Convert mediasoup DtlsParameters → protocol DtlsParameters via serde.
    fn convert_dtls_parameters(dtls: &mediasoup::prelude::DtlsParameters) -> protocol::DtlsParameters {
        // DtlsParameters derives Serialize; DtlsFingerprint has a custom Serialize
        // that produces {"algorithm": "sha-256", "value": "AA:BB:..."}.
        // Serialize to JSON, then deserialize into our protocol types.
        // ponytail: serde round-trip for type conversion; hand-write converters if perf matters.
        let json = serde_json::to_value(dtls).unwrap_or_default();
        protocol::DtlsParameters {
            fingerprints: json
                .get("fingerprints")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|f| protocol::Fingerprint {
                            algorithm: f["algorithm"].as_str().unwrap_or("unknown").to_string(),
                            value: f["value"].as_str().unwrap_or("").to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            role: json["role"].as_str().unwrap_or("auto").to_string(),
        }
    }

    fn convert_ice_parameters(ice: &IceParameters) -> protocol::IceParameters {
        protocol::IceParameters {
            username_fragment: ice.username_fragment.clone(),
            password: ice.password.clone(),
        }
    }

    fn convert_ice_candidates(candidates: &[IceCandidate]) -> Vec<protocol::IceCandidate> {
        candidates
            .iter()
            .map(|c| protocol::IceCandidate {
                ip: c.address.clone(),
                port: c.port,
                protocol: format!("{:?}", c.protocol).to_lowercase(),
                foundation: c.foundation.clone(),
                priority: c.priority,
                candidate_type: format!("{:?}", c.r#type).to_lowercase(),
            })
            .collect()
    }

    impl SfuManager {
        /// Create a new SfuManager with a single mediasoup Worker and WebRtcServer.
        /// SFU 端口取自 `MEDIASERVO_SFU_PORT`（缺省 20000）— 测试用 `new_with_port` 传随机端口。
        pub async fn new() -> Result<Self, String> {
            let sfu_port = std::env::var("MEDIASERVO_SFU_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(20000);
            Self::new_with_port(sfu_port).await
        }

        /// 指定 WebRtcServer 监听端口（测试隔离: 多测试并行各绑独立端口）。
        pub async fn new_with_port(sfu_port: u16) -> Result<Self, String> {
            let worker_manager = WorkerManager::new();
            let worker = worker_manager
                .create_worker(WorkerSettings::default())
                .await
                .map_err(|e| format!("Failed to create mediasoup worker: {e}"))?;
            tracing::info!("mediasoup Worker created (id: {:?})", worker.id());

            // Create WebRtcServer with single port (port 20000)
            // PIT-44: listen 0.0.0.0 必须设 announced_address（mediasoup 官方要求），
            // 否则 candidate=0.0.0.0 对端无法 ICE；容器内探测本机 IP。
            // PIT-58: 容器内探测 = 172.18.0.2 (内网地址, 其他主机不可达 → Signal Lost);
            // 必须用宿主可达 IP — 环境变量 MEDIASERVO_SFU_ANNOUNCED_IP 配置 (宿主机网卡 IP)
            // 多 announced IP（宿主多网卡）：每个 IP 一个 ListenInfo（listen 0.0.0.0:20000）
            let announced_ips = announced_ips_from_env();
            let mut listen_infos = WebRtcServerListenInfos::new(ListenInfo {
                protocol: Protocol::Udp,
                ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                announced_address: announced_ips.first().cloned(),
                expose_internal_ip: false,
                port: Some(sfu_port),  // Fixed ICE port
                port_range: None,
                flags: None,
                send_buffer_size: None,
                recv_buffer_size: None,
            });
            for ip in announced_ips.iter().skip(1) {
                listen_infos = listen_infos.insert(ListenInfo {
                    protocol: Protocol::Udp,
                    ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                    announced_address: Some(ip.clone()),
                    expose_internal_ip: false,
                    port: Some(sfu_port),
                    port_range: None,
                    flags: None,
                    send_buffer_size: None,
                    recv_buffer_size: None,
                });
            }
            tracing::info!("WebRtcServer created on port {sfu_port} (announced: {announced_ips:?})");
            let webrtc_server = worker
                .create_webrtc_server(WebRtcServerOptions::new(listen_infos))
                .await
                .map_err(|e| format!("Failed to create WebRtcServer: {e}"))?;

            Ok(Self {
                worker_manager,
                worker,
                webrtc_server: Arc::new(webrtc_server),
                rooms: DashMap::new(),
            })
        }

        /// Create a WebRTC transport for a peer in a room.
        pub async fn create_webrtc_transport(
            &self,
            room_id: &str,
            peer_id: &str,
            direction: &str,
        ) -> Result<TransportCreated, String> {
            // Get or create room
            let router = {
                if let Some(room) = self.rooms.get(room_id) {
                    Arc::clone(&room.router)
                } else {
                    // No room yet — create one
                    let router = self
                        .worker
                        .create_router(default_router_options())
                        .await
                        .map_err(|e| {
                            // v2 诊断: 打印 codec 列表定位 PT 冲突
                            tracing::error!("Router create failed; media_codecs={:?}", default_router_options());
                            format!("Failed to create router: {e}")
                        })?;
                    let router = Arc::new(router);
                    tracing::info!("Router created for room {}", room_id);

                    self.rooms.insert(
                        room_id.to_string(),
                        SfuRoom {
                            router: Arc::clone(&router),
                            peers: DashMap::new(),
                        },
                    );
                    router
                }
            };

            // Create transport using shared WebRtcServer (single port)
            let options = WebRtcTransportOptions::new_with_server(self.webrtc_server.as_ref().clone());
            let transport = router
                .create_webrtc_transport(options)
                .await
                .map_err(|e| format!("Failed to create transport: {e}"))?;

            let transport_id = transport.id().to_string();
            let ice = transport.ice_parameters().clone();
            let dtls = transport.dtls_parameters();
            let ice_candidates = convert_ice_candidates(transport.ice_candidates());

            // Store transport on peer
            if let Some(room) = self.rooms.get_mut(room_id) {
                let mut peer = room.peers.entry(peer_id.to_string()).or_insert_with(|| {
                    SfuPeer {
                        send_transport: None,
                        recv_transport: None,
                        producers: Vec::new(),
                        consumers: Vec::new(),
                    }
                });

                match direction {
                    "send" => {
                        peer.send_transport = Some(transport);
                    }
                    "recv" => {
                        peer.recv_transport = Some(transport);
                    }
                    _ => return Err(format!("Invalid direction: {direction}")),
                }
            }

            Ok(TransportCreated {
                transport_id,
                ice_parameters: convert_ice_parameters(&ice),
                dtls_parameters: convert_dtls_parameters(&dtls),
                ice_candidates,
            })
        }

        /// Remove a peer from a room, cleaning up transports, producers, and consumers.
        /// Returns true if the peer was found and removed.
        /// If the room becomes empty after removal, the Router is destroyed.
        pub fn remove_peer(&self, room_id: &str, peer_id: &str) -> bool {
            if let Some(mut room) = self.rooms.get_mut(room_id) {
                let removed = room.peers.remove(peer_id).is_some();
                if removed {
                    tracing::info!("Peer {} removed from SFU room {}", peer_id, room_id);
                    if room.peers.is_empty() {
                        drop(room);
                        self.remove_room(room_id);
                    }
                }
                removed
            } else {
                false
            }
        }

        /// Remove a room and its Router (stops forwarding for all peers).
        pub fn remove_room(&self, room_id: &str) -> bool {
            let existed = self.rooms.remove(room_id).is_some();
            if existed {
                tracing::info!("SFU room {} destroyed", room_id);
            }
            existed
        }
        /// Create a producer for a peer on its send transport.
        pub async fn create_producer(
            &self,
            room_id: &str,
            peer_id: &str,
            kind: &protocol::MediaKind,
            rtp_parameters_json: serde_json::Value,
        ) -> Result<ProduceResult, String> {
            // Convert JSON RTP parameters to mediasoup type
            let rtp_parameters: RtpParameters = serde_json::from_value(rtp_parameters_json)
                .map_err(|e| format!("Invalid RTP parameters: {e}"))?;

            let ms_kind = match kind {
                protocol::MediaKind::Audio => MediaKind::Audio,
                protocol::MediaKind::Video => MediaKind::Video,
            };

            let room = self.rooms.get_mut(room_id)
                .ok_or_else(|| format!("Room {} not found for produce", room_id))?;
            let mut peer = room.peers.get_mut(peer_id)
                .ok_or_else(|| format!("Peer {} not found in room {}", peer_id, room_id))?;

            let transport = peer.send_transport.as_ref()
                .ok_or_else(|| format!("No send transport for peer {}", peer_id))?;

            // ponytail: construct ProducerOptions; let compiler validate the exact constructor
            let producer_options = ProducerOptions::new(ms_kind, rtp_parameters);
            let producer = transport.produce(producer_options).await
                .map_err(|e| format!("Failed to create producer: {e}"))?;

            let producer_id = producer.id().to_string();
            tracing::info!(
                "Producer {} ({:?}) created for peer {} in room {}",
                producer_id, kind, peer_id, room_id
            );

            // PIT 观测: Producer RTP trace（每包处理/丢弃原因）— 验证 push 帧是否到达 server
            {
                let pid = producer_id.clone();
                producer.on_trace(move |trace: &mediasoup::producer::ProducerTraceEventData| {
                    tracing::info!("PROD-TRACE {}: {:?}", pid, trace);
                });
                let _ = producer
                    .enable_trace_event(vec![mediasoup::producer::ProducerTraceEventType::Rtp])
                    .await;
            }
            // PIT 观测: Producer dump（rtp_streams score — 收包质量）
            {
                let pid = producer_id.clone();
                if let Ok(dump) = producer.dump().await {
                    let scores: Vec<u8> = dump.rtp_streams.iter().map(|s| s.score).collect();
                    tracing::info!("PROD-DUMP {}: paused={} scores={:?}", pid, dump.paused, scores);
                }
            }

            peer.producers.push(producer);

            Ok(ProduceResult {
                producer_id,
                kind: kind.clone(),
            })
        }

        /// Create a consumer for a peer on its recv transport,
        /// subscribing to an existing producer in the room.
        pub async fn create_consumer(
            &self,
            room_id: &str,
            peer_id: &str,
            producer_id: &str,
            rtp_capabilities_json: serde_json::Value,
        ) -> Result<ConsumeResult, String> {
            // Convert JSON RTP capabilities to mediasoup type
            let rtp_capabilities: RtpCapabilities = serde_json::from_value(rtp_capabilities_json)
                .map_err(|e| format!("Invalid RTP capabilities: {e}"))?;

            // Find the producer and extract its id + kind
            // ponytail: read-lock first to get producer info, then write-lock for consumer insert
            let (producer_id_ms, producer_kind) = {
                let room = self.rooms.get(room_id)
                    .ok_or_else(|| format!("Room {} not found for consume", room_id))?;
                room.peers.iter()
                    .find_map(|entry| {
                        entry.producers.iter()
                            .find(|p| p.id().to_string() == producer_id)
                            .map(|p| (p.id(), p.kind()))
                    })
                    .ok_or_else(|| {
                        format!("Producer {} not found in room {}", producer_id, room_id)
                    })?
            };

            // Now get the consumer peer's recv transport
            let room = self.rooms.get_mut(room_id)
                .ok_or_else(|| format!("Room {} not found", room_id))?;
            let mut peer = room.peers.get_mut(peer_id)
                .ok_or_else(|| format!("Peer {} not found in room {}", peer_id, room_id))?;
            let transport = peer.recv_transport.as_ref()
                .ok_or_else(|| format!("No recv transport for peer {}", peer_id))?;

            let consumer_options = ConsumerOptions::new(producer_id_ms, rtp_capabilities);
            let consumer = transport.consume(consumer_options).await
                .map_err(|e| format!("Failed to create consumer: {e}"))?;

            let consumer_id = consumer.id().to_string();
            let protocol_kind = match producer_kind {
                MediaKind::Audio => protocol::MediaKind::Audio,
                MediaKind::Video => protocol::MediaKind::Video,
            };
            let rtp_parameters_json = serde_json::to_value(consumer.rtp_parameters())
                .unwrap_or_default();

            tracing::info!(
                "Consumer {} created for peer {} (producer: {}, kind: {:?})",
                consumer_id, peer_id, producer_id, protocol_kind
            );

            // PIT-65 观测: Consumer RTP trace (每包处理/丢弃原因)
            {
                let cid = consumer_id.clone();
                consumer.on_trace(move |trace: &mediasoup::consumer::ConsumerTraceEventData| {
                    tracing::info!("CONS-TRACE {}: {:?}", cid, trace);
                });
                let _ = consumer
                    .enable_trace_event(vec![mediasoup::consumer::ConsumerTraceEventType::Rtp])
                    .await;
            }

            // PIT-65 观测: Consumer dump (rtp_streams score — IsActive 判定依据)
            {
                let cid = consumer_id.clone();
                if let Ok(dump) = consumer.dump().await {
                    let scores: Vec<u8> = dump.rtp_streams.iter().map(|s| s.score).collect();
                    tracing::info!("CONS-DUMP {}: paused={} scores={:?}", cid, dump.paused, scores);
                }
            }

            // PIT 观测: Consumer 周期 dump（每 2s 打 score — 验证 producer→consumer 转发）
            {
                let cid = consumer_id.clone();
                let consumer_for_dump = consumer.clone();
                tokio::spawn(async move {
                    for _ in 0..15 {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        if let Ok(dump) = consumer_for_dump.dump().await {
                            let scores: Vec<u8> = dump.rtp_streams.iter().map(|s| s.score).collect();
                            tracing::info!(
                                "CONS-DUMP-PERIODIC {}: paused={} scores={:?}",
                                cid, dump.paused, scores
                            );
                        }
                    }
                });
            }

            // PIT-76: consume 后立即请求关键帧 — 绕过 libwebrtc 99s GOP（x-google-
            // max-keyframe-interval 注入对软件 VP8 编码器不生效，实测仍 99s）。
            // mediasoup request_key_frame → producer 侧发送关键帧请求 → 编码器立即出 IDR
            if protocol_kind == protocol::MediaKind::Video {
                match consumer.request_key_frame().await {
                    Ok(()) => tracing::info!("Consumer {consumer_id}: requested key frame (instant first-frame)"),
                    Err(e) => tracing::warn!("Consumer {consumer_id}: request_key_frame failed: {e}"),
                }
            }

            peer.consumers.push(consumer);

            Ok(ConsumeResult {
                consumer_id,
                producer_id: producer_id.to_string(),
                kind: protocol_kind,
                rtp_parameters_json,
            })
        }

        /// Connect a WebRTC transport with DTLS parameters from the client.
        pub async fn connect_transport(
            &self,
            room_id: &str,
            peer_id: &str,
            transport_id: &str,
            dtls_parameters: protocol::DtlsParameters,
        ) -> Result<(), String> {
            // Convert protocol::DtlsParameters → mediasoup DtlsParameters via serde round-trip
            // ponytail: serde round-trip for type conversion; hand-write converters if perf matters.
            let ms_dtls: mediasoup::prelude::DtlsParameters = {
                let json = serde_json::to_value(&dtls_parameters)
                    .map_err(|e| format!("serialize DtlsParameters: {e}"))?;
                serde_json::from_value(json)
                    .map_err(|e| format!("deserialize DtlsParameters: {e}"))?
            };

            let room = self.rooms.get_mut(room_id)
                .ok_or_else(|| format!("Room {room_id} not found for connect"))?;
            let peer = room.peers.get_mut(peer_id)
                .ok_or_else(|| format!("Peer {peer_id} not found in room {room_id}"))?;

            // Find the transport by ID in send or recv transport
            let transport = peer.send_transport.as_ref()
                .filter(|t| t.id().to_string() == transport_id)
                .or_else(|| {
                    peer.recv_transport.as_ref()
                        .filter(|t| t.id().to_string() == transport_id)
                })
                .ok_or_else(|| {
                    format!("Transport {transport_id} not found for peer {peer_id}")
                })?;

            transport.connect(mediasoup::prelude::WebRtcTransportRemoteParameters { dtls_parameters: ms_dtls }).await
                .map_err(|e| format!("Failed to connect transport: {e}"))?;

            // PIT 观测: dump transport selected tuple（server 学到的客户端地址 — RTP 发送目标）
            if let Ok(dump) = transport.dump().await {
                tracing::info!(
                    "SFU: transport {transport_id} dump: ice={:?} dtls={:?} selected_tuple={:?}",
                    dump.ice_state, dump.dtls_state, dump.ice_selected_tuple
                );
            }

            tracing::info!(
                "SFU: transport {transport_id} connected for peer {peer_id} in room {room_id}"
            );
            Ok(())
        }

        /// List all producers in a room. Returns (producer_id, kind, peer_id) tuples.
        /// Used for late-joiner sync to send existing producers to new consumers.
        pub fn list_producers(&self, room_id: &str) -> Option<Vec<(String, protocol::MediaKind, String)>> {
            let room = self.rooms.get(room_id)?;
            let mut result = Vec::new();
            for entry in room.peers.iter() {
                let peer_id = entry.key().clone();
                for producer in &entry.producers {
                    let kind = match producer.kind() {
                        MediaKind::Audio => protocol::MediaKind::Audio,
                        MediaKind::Video => protocol::MediaKind::Video,
                    };
                    result.push((producer.id().to_string(), kind, peer_id.clone()));
                }
            }
            Some(result)
        }

        /// Send raw RTP data through the first video producer in the room.
        /// Used for WS→SFU frame relay (avoids Host-side ICE/DTLS).
        ///
        /// Note: Requires a DirectProducer. Regular producers (WebRtcTransport-based)
        /// receive RTP from the client-side peer connection, not server-side injection.
        pub fn send_frame(&self, _room_id: &str, _rtp_data: &[u8]) -> Result<(), String> {
            Err("send_frame requires DirectProducer; WebRtcTransport producers receive RTP from client-side ICE/DTLS".into())
        }
        /// Number of active rooms.
        pub fn room_count(&self) -> usize {
            self.rooms.len()
        }
    }

#[cfg(test)]
mod tests {
    use super::*;

    /// PIT-58: announced_address 必须优先环境变量 (宿主可达 IP) —
    /// 容器内探测 (172.18.0.2) 仅本机可用, 其他主机 ICE 不可达 → Signal Lost。
    #[test]
    fn announced_ips_prefers_env_and_falls_back() {
        // env 优先 (宿主 IP 场景)
        // SAFETY: 测试内串行设置/恢复, 无并发读
        unsafe { std::env::set_var("MEDIASERVO_SFU_ANNOUNCED_IP", "192.168.2.127"); }
        assert_eq!(announced_ips_from_env(), vec!["192.168.2.127".to_string()]);

        // 多 IP 逗号分隔
        // SAFETY: 同上
        unsafe { std::env::set_var("MEDIASERVO_SFU_ANNOUNCED_IP", "192.168.2.127,10.0.0.5"); }
        assert_eq!(
            announced_ips_from_env(),
            vec!["192.168.2.127".to_string(), "10.0.0.5".to_string()]
        );

        // 逗号 + 空格容错
        // SAFETY: 同上
        unsafe { std::env::set_var("MEDIASERVO_SFU_ANNOUNCED_IP", " 192.168.2.127 , 10.0.0.5 "); }
        assert_eq!(announced_ips_from_env().len(), 2);

        // fallback 探测 (未配置场景)
        // SAFETY: 同上
        unsafe { std::env::remove_var("MEDIASERVO_SFU_ANNOUNCED_IP"); }
        let fallback = announced_ips_from_env();
        assert!(!fallback.is_empty(), "fallback 探测应返回非空 IP");

        // 恢复环境, 避免污染其他测试
        // SAFETY: 同上
        unsafe { std::env::remove_var("MEDIASERVO_SFU_ANNOUNCED_IP"); }
    }
}}




// ── Stub when sfu-mediasoup is not enabled ──────────────────────────────

#[cfg(not(feature = "sfu-mediasoup"))]
mod imp {
    use super::protocol;

    /// Stub SfuManager — SFU not available.
    pub struct SfuManager;

    impl SfuManager {
        /// Returns an error in non-SFU builds.
        pub async fn new() -> Result<Self, String> {
            Err("sfu-mediasoup feature not enabled".into())
        }

        /// Stub — returns error in non-SFU builds.
        pub async fn create_webrtc_transport(
            &self,
            _room_id: &str,
            _peer_id: &str,
            _direction: &str,
        ) -> Result<TransportCreated, String> {
            Err("sfu-mediasoup feature not enabled".into())
        }

        /// Stub — returns error in non-SFU builds.
        pub async fn create_producer(
            &self,
            _room_id: &str,
            _peer_id: &str,
            _kind: &protocol::MediaKind,
            _rtp_parameters_json: serde_json::Value,
        ) -> Result<ProduceResult, String> {
            Err("sfu-mediasoup feature not enabled".into())
        }

        /// Stub — returns error in non-SFU builds.
        pub async fn create_consumer(
            &self,
            _room_id: &str,
            _peer_id: &str,
            _producer_id: &str,
            _rtp_capabilities_json: serde_json::Value,
        ) -> Result<ConsumeResult, String> {
            Err("sfu-mediasoup feature not enabled".into())
        }

        /// Stub — returns error in non-SFU builds.
        pub async fn connect_transport(&self, _room_id: &str, _peer_id: &str, _transport_id: &str, _dtls_params: protocol::DtlsParameters) -> Result<(), String> {
            Err("sfu-mediasoup feature not enabled".into())
        }

        /// Stub — no-op in non-SFU builds.
        pub fn remove_peer(&self, _room_id: &str, _peer_id: &str) -> bool {
            false
        }

        /// Stub — no-op in non-SFU builds.
        pub fn remove_room(&self, _room_id: &str) -> bool {
            false
        }

        /// Stub — returns 0.
        pub fn room_count(&self) -> usize {
            0
        }
    }

    /// Stub TransportCreated — SFU not available.
    pub struct TransportCreated;

    /// Stub SfuRoom — SFU not available.
    pub struct SfuRoom;

    /// Stub SfuPeer — SFU not available.
    pub struct SfuPeer;

    /// Stub ProduceResult — SFU not available.
    pub struct ProduceResult;

    /// Stub ConsumeResult — SFU not available.
    pub struct ConsumeResult;
}

pub use imp::{SfuManager, SfuPeer, SfuRoom, TransportCreated, ProduceResult, ConsumeResult};

/// 测试用: 进程内唯一的 SFU 测试端口（原子计数器，每次调用 +1）。
/// 曾用 bind :0 探空闲端口 — 并行测试 TOCTOU 竞态会拿到同一端口（PIT-103 实证），
/// 计数器保证单进程内绝不重复；不同测试二进制=不同进程，cargo 串行跑 → 无冲突。
pub fn random_udp_port() -> u16 {
    static NEXT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(31000);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
