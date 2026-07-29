//! mediasoup SFU foundation.
//!
//! Provides `SfuManager` (global), `SfuRoom` (per-room Router + peers),
//! and `SfuPeer` (per-peer transports + producers/consumers).
//!
//! Only compiled when the `sfu-mediasoup` feature is enabled.

// ── Feature-gated imports ───────────────────────────────────────────────

#[cfg(feature = "sfu-mediasoup")]
use dashmap::DashMap;
use omspbase_common::protocol;

#[cfg(feature = "sfu-mediasoup")]
mod imp {
    use super::*;
    use mediasoup::prelude::*;
    use mediasoup::worker_manager::WorkerManager;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::Arc;

    /// Result of a transport creation request.
    pub struct TransportCreated {
        pub transport_id: String,
        pub ice_parameters: protocol::IceParameters,
        pub dtls_parameters: protocol::DtlsParameters,
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

    impl SfuManager {
        /// Create a new SfuManager with a single mediasoup Worker.
        pub async fn new() -> Result<Self, String> {
            let worker_manager = WorkerManager::new();
            let worker = worker_manager
                .create_worker(WorkerSettings::default())
                .await
                .map_err(|e| format!("Failed to create mediasoup worker: {e}"))?;
            tracing::info!("mediasoup Worker created (id: {:?})", worker.id());
            Ok(Self {
                worker_manager,
                worker,
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
                        .create_router(RouterOptions::default())
                        .await
                        .map_err(|e| format!("Failed to create router: {e}"))?;
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

            // Create transport
            let listen_info = ListenInfo {
                protocol: Protocol::Udp,
                ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                announced_address: None,
                expose_internal_ip: false,
                port: None,
                port_range: None,
                flags: None,
                send_buffer_size: None,
                recv_buffer_size: None,
            };
            let options =
                WebRtcTransportOptions::new(WebRtcTransportListenInfos::new(listen_info));
            let transport = router
                .create_webrtc_transport(options)
                .await
                .map_err(|e| format!("Failed to create transport: {e}"))?;

            let transport_id = transport.id().to_string();
            let ice = transport.ice_parameters().clone();
            let dtls = transport.dtls_parameters();

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

            transport.connect(ms_dtls).await
                .map_err(|e| format!("Failed to connect transport: {e}"))?;

            tracing::info!(
                "SFU: transport {transport_id} connected for peer {peer_id} in room {room_id}"
            );
            Ok(())
        }
    }
}

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
