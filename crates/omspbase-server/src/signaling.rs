use crate::audit::{self, AuditEvent};
use crate::room::RoomManager;
use crate::health::{HealthChecker, HealthStatus};
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::get;
#[cfg(feature = "sfu-mediasoup")]
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use omspbase_common::auth::{JwtAuth, SimplePskAuth};
use omspbase_common::error::CoreError;
use omspbase_common::protocol::SignalingMessage;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::broadcast;
use tokio::sync::watch;

struct RoomChannel {
    tx: broadcast::Sender<String>,
}

impl RoomChannel {
    fn new() -> Self {
        let (tx, _) = broadcast::channel::<String>(4096); // ponytail: 4096 frames ~= 4s at 1k fps
        Self { tx }
    }
}

#[derive(Clone)]
pub struct SignalingServer {
    channels: Arc<dashmap::DashMap<String, RoomChannel>>,
    pub room_manager: RoomManager,
    /// SFU manager for mediasoup transport negotiation.
    #[cfg(feature = "sfu-mediasoup")]
    pub sfu_manager: Arc<crate::sfu::SfuManager>,
    /// Shutdown signal — send `true` to request draining.
    shutdown_tx: watch::Sender<bool>,
    /// Number of currently active WebSocket connections.
    active_connections: Arc<AtomicUsize>,
    pub ws_max_message_size: usize,
    /// Pending messages cache — stores SDP offer + ICE candidates per room for late-joiner replay.
    pub pending_messages: Arc<dashmap::DashMap<String, Vec<String>>>,
    /// JWT authenticator (optional; PSK used as fallback).
    pub jwt_auth: Option<JwtAuth>,
}

impl SignalingServer {
    #[cfg(feature = "sfu-mediasoup")]
    pub fn new(_sfu: Arc<crate::sfu::SfuManager>, ws_max_message_size: usize, jwt_auth: Option<JwtAuth>) -> Self {
        let (shutdown_tx, _) = watch::channel(false);
        Self {
            channels: Arc::new(dashmap::DashMap::new()),
            room_manager: RoomManager::new(),
            sfu_manager: _sfu,
            shutdown_tx,
            active_connections: Arc::new(AtomicUsize::new(0)),
            ws_max_message_size,
            pending_messages: Arc::new(dashmap::DashMap::new()),
            jwt_auth,
        }
    }

    #[cfg(not(feature = "sfu-mediasoup"))]
    pub fn new(ws_max_message_size: usize, jwt_auth: Option<JwtAuth>) -> Self {
        let (shutdown_tx, _) = watch::channel(false);
        Self {
            channels: Arc::new(dashmap::DashMap::new()),
            room_manager: RoomManager::new(),
            shutdown_tx,
            active_connections: Arc::new(AtomicUsize::new(0)),
            ws_max_message_size,
            pending_messages: Arc::new(dashmap::DashMap::new()),
            jwt_auth,
        }
    }

    /// Subscribe to the shutdown signal — cloned receivers are given to each
    /// WebSocket handler so they can detect when draining has been requested.
    pub fn subscribe_shutdown(&self) -> watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }

    /// Request graceful shutdown: all active connections should close.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
        tracing::info!(
            "Shutdown signal broadcast to {} active connections",
            self.active_connections.load(Ordering::Relaxed)
        );
    }

    /// Number of currently active WebSocket connections.
    pub fn active_connections(&self) -> usize {
        self.active_connections.load(Ordering::Relaxed)
    }

    fn get_or_create_channel(&self, room_id: &str) -> broadcast::Sender<String> {
        self.channels
            .entry(room_id.to_string())
            .or_insert_with(RoomChannel::new)
            .tx
            .clone()
    }
}

// ── HealthChecker impl ────────────────────────────────────────────────────

impl HealthChecker for SignalingServer {
    fn name(&self) -> &'static str {
        "signaling"
    }

    fn check_health(&self) -> HealthStatus {
        let connections = self.active_connections.load(Ordering::Relaxed);
        let rooms = self.room_manager.active_rooms();
        tracing::debug!("Health: {connections} connections, {rooms} rooms");
        // ponytail: always healthy while alive; add degraded thresholds if needed
        HealthStatus::Healthy
    }
}

pub fn signaling_router(server: SignalingServer) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .with_state(server)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(server): State<SignalingServer>,
) -> impl IntoResponse {
    // Extract JWT from sec-websocket-protocol header (format: "Bearer <token>")
    let jwt_token = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());
    let max_size = server.ws_max_message_size;
    ws.max_message_size(max_size).on_upgrade(move |socket| handle_socket(socket, server, jwt_token))
}

/// Send a signaling message to this peer directly (not broadcast).
fn send_msg(msg: &SignalingMessage) -> Result<String, String> {
    serde_json::to_string(msg).map_err(|e| format!("serialize error: {e}"))
}

async fn handle_socket(socket: WebSocket, server: SignalingServer, jwt_token: Option<String>) {
    // Track active connection count
    server.active_connections.fetch_add(1, Ordering::Relaxed);

    // Decrement on every exit path
    struct Guard(Arc<AtomicUsize>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::Relaxed);
        }
    }
    let _guard = Guard(Arc::clone(&server.active_connections));

    // Subscribe to shutdown signal
    let shutdown_rx = server.subscribe_shutdown();

    let (ws_sender, mut receiver) = socket.split();
    let ws_sender = Arc::new(tokio::sync::Mutex::new(ws_sender));

    let mut peer_id = uuid::Uuid::new_v4().to_string();
    tracing::info!("New connection: peer={}", peer_id);

    // PSK auth — from env var for Phase 1
    let psk = std::env::var("OMSPBASE_PSK").ok();
    let psk_auth = psk.as_ref().map(|k| SimplePskAuth::new(k.as_bytes()));
    let mut authenticated = psk_auth.is_none();
    tracing::info!("Auth: psk_set={}, authenticated={}", psk.is_some(), authenticated);

    // ── JWT auth (tried first; PSK is fallback) ───────────────────────
    if !authenticated {
        if let (Some(jwt_auth), Some(token)) = (&server.jwt_auth, &jwt_token) {
            match jwt_auth.verify(token) {
                Ok(claims) => {
                    peer_id = claims.sub.clone();
                    authenticated = true;
                    tracing::info!("JWT authenticated: peer={}", peer_id);
                    audit::log_event(AuditEvent::AuthSuccess {
                        peer_id: peer_id.clone(),
                    });
                }
                Err(e) => {
                    tracing::warn!("JWT verification failed: {}, falling back to PSK", e);
                }
            }
        }
    }

    // ── PSK auth (fallback) ───────────────────────────────────────────
    if !authenticated {
        tracing::info!("Auth: waiting for PSK...");
        match receiver.next().await {
            Some(Ok(Message::Text(text))) => {
                if let Some(ref a) = psk_auth
                    && (a.sign(peer_id.as_bytes()) == a.sign(text.as_bytes())
                        || text == psk.as_deref().unwrap_or(""))
                {
                    authenticated = true;
                    tracing::info!("Peer {} authenticated via PSK", peer_id);
                    audit::log_event(AuditEvent::AuthSuccess {
                        peer_id: peer_id.clone(),
                    });
                }
                if !authenticated {
                    let error = SignalingMessage::Error {
                        code: 4003,
                        message: "PSK authentication failed".into(),
                    };
                    let _ = ws_sender
                        .lock()
                        .await
                        .send(Message::Text(send_msg(&error).unwrap()))
                        .await;
                    audit::log_event(AuditEvent::AuthFailure {
                        peer_id: peer_id.clone(),
                        reason: "PSK authentication failed".into(),
                    });
                    return;
                }
            }
            _ => {
                let error = SignalingMessage::Error {
                    code: 4003,
                    message: "Authentication required".into(),
                };
                let _ = ws_sender
                    .lock()
                    .await
                    .send(Message::Text(send_msg(&error).unwrap()))
                    .await;
                audit::log_event(AuditEvent::AuthFailure {
                    peer_id: peer_id.clone(),
                    reason: "Authentication required".into(),
                });
                return;
            }
        }
    }

    // Always send auth ack (or skip if no auth required)
    let ack = SignalingMessage::Error {
        code: 0,
        message: "authenticated".into(),
    };
    let _ = ws_sender
        .lock()
        .await
        .send(Message::Text(send_msg(&ack).unwrap()))
        .await;
    tracing::info!("Auth ack sent, entering RoomJoin phase");

    // Phase 2: RoomJoin
    let (room_id, role) = loop {
        // Check for shutdown during RoomJoin
        if *shutdown_rx.borrow() {
            tracing::info!("Shutdown requested during RoomJoin for peer {}", peer_id);
            return;
        }

        tracing::debug!("RoomJoin: waiting for message...");
        match receiver.next().await {
            Some(Ok(Message::Text(text))) => {
                let text_str = text.to_string();
                if let Ok(SignalingMessage::RoomJoin { room_id, peer_role, .. }) =
                    serde_json::from_str(&text_str)
                {
                    break (room_id, peer_role);
                }
            }
            Some(Ok(Message::Close(_))) | None => return,
            _ => continue,
        }
    };

    // Join the room
    match server.room_manager.join_room(&room_id, &peer_id, &role) {
        Ok(()) => {
            audit::log_event(AuditEvent::PeerJoin {
                peer_id: peer_id.clone(),
                room_id: room_id.clone(),
                role: format!("{:?}", role),
            });
        }
        Err(CoreError::RoomFull) => {
            let error = SignalingMessage::Error {
                code: 4002,
                message: "Room is full".into(),
            };
            let _ = ws_sender
                .lock()
                .await
                .send(Message::Text(send_msg(&error).unwrap()))
                .await;
            return;
        }
        Err(e) => {
            tracing::error!("Room join error: {}", e);
            let error = SignalingMessage::Error {
                code: 4001,
                message: format!("Failed to join room: {}", e),
            };
            let _ = ws_sender
                .lock()
                .await
                .send(Message::Text(send_msg(&error).unwrap()))
                .await;
            return;
        }
    }

    // Send RoomJoined ack
    let ack = SignalingMessage::RoomJoined {
        room_id: room_id.clone(),
        peer_id: peer_id.clone(),
    };
    let _ = ws_sender
        .lock()
        .await
        .send(Message::Text(send_msg(&ack).unwrap()))
        .await;

    // ── Replay cached SDP offer + ICE candidates for late joiners ────────
    if let Some(cached) = server.pending_messages.get(&room_id) {
        let sender = Arc::clone(&ws_sender);
        let count = cached.len();
        for msg in cached.iter() {
            let _ = sender.lock().await.send(Message::Text(msg.clone())).await;
        }
        tracing::info!("Replayed {} cached messages for room {}", count, room_id);
    }

    let tx = server.get_or_create_channel(&room_id);
    let mut rx = tx.subscribe();

    // Phase 3: Message relay
    let relay_peer_id = peer_id.clone();
    let relay_room = room_id.clone();

    // Clone ws_sender for SFU direct responses and relay
    #[cfg(feature = "sfu-mediasoup")]
    let direct_sender = Arc::clone(&ws_sender);
    let relay_sender = ws_sender;

    let relay_handle = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    tracing::info!("Relay: forwarding to peer ({} bytes)", msg.len());
                    if relay_sender
                        .lock()
                        .await
                        .send(Message::Text(msg))
                        .await
                        .is_err()
                    {
                        tracing::warn!("Relay: send failed, peer disconnected");
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Relay: lagged behind by {} messages, continuing", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("Relay: broadcast channel closed");
                    break;
                }
            }
        }
    });

    // Forward: this peer's receiver → broadcast
    tracing::info!("Entering forward loop for peer {}", relay_peer_id);
    while let Some(Ok(msg)) = receiver.next().await {
        // Check shutdown signal before processing each message
        if *shutdown_rx.borrow() {
            tracing::info!("Shutdown requested, disconnecting peer {}", relay_peer_id);
            // Notify room peers
            let leave_msg = SignalingMessage::RoomLeave {
                room_id: relay_room.clone(),
                peer_id: relay_peer_id.clone(),
            };
            let _ = tx.send(serde_json::to_string(&leave_msg).unwrap());
            break;
        }

        match msg {
            Message::Text(text) => {
                let text_str = text.to_string();

                // Handle RoomLeave — relay to peers then disconnect (cleanup in disconnect path)
                if let Ok(sig) = serde_json::from_str::<SignalingMessage>(&text_str)
                    && matches!(sig, SignalingMessage::RoomLeave { .. })
                {
                    let _ = tx.send(text_str);
                    break;
                }

                // Check for SFU transport messages (server-side handling)
                #[cfg(feature = "sfu-mediasoup")]
                {
                    tracing::debug!("SFU check: parsing message");
                    if let Ok(sig_msg) = serde_json::from_str::<SignalingMessage>(&text_str) {
                        tracing::debug!("SFU check: parsed OK, calling handle_sfu_message");
                        if handle_sfu_message(
                            &sig_msg,
                            &server.sfu_manager,
                            &direct_sender,
                            &tx,
                            &relay_peer_id,
                        )
                        .await
                        {
                            continue; // Handled by SFU, don't relay
                        }
                    } else {
                        tracing::debug!("SFU check: parse FAILED for: {}", &text_str[..text_str.len().min(100)]);
                    }
                }

                // Try SignalingMessage first, then raw JSON for Frame
                let should_relay = match serde_json::from_str::<SignalingMessage>(&text_str) {
                    Ok(sig_msg) => matches!(
                        sig_msg,
                        SignalingMessage::Sdp { .. } | SignalingMessage::RTCIceCandidate { .. }
                            | SignalingMessage::Frame { .. }
                    ),
                    Err(_) => {
                        if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&text_str) {
                            raw.get("type").and_then(|v| v.as_str()) == Some("frame")
                        } else {
                            false
                        }
                    }
                };
                if should_relay {
                    // ── DeviceStream filter: only relay Frame, skip SDP/ICE from consumers
                    if server
                        .room_manager
                        .is_device_stream(&relay_room)
                    {
                        let is_frame = if let Ok(sig_msg) =
                            serde_json::from_str::<SignalingMessage>(&text_str)
                        {
                            matches!(sig_msg, SignalingMessage::Frame { .. })
                        } else if let Ok(raw) =
                            serde_json::from_str::<serde_json::Value>(&text_str)
                        {
                            raw.get("type").and_then(|v| v.as_str()) == Some("frame")
                        } else {
                            false
                        };
                        if !is_frame {
                            tracing::debug!("DeviceStream: dropping non-Frame message");
                            continue;
                        }
                    }

                    match tx.send(text_str.clone()) {
                        Ok(n) => {
                            tracing::debug!("Forward: broadcast to {} receivers", n);
                            // ── Cache SDP + ICE for late-joiner replay
                            if let Ok(sig_msg) = serde_json::from_str::<SignalingMessage>(&text_str) {
                                if matches!(sig_msg, SignalingMessage::Sdp { .. } | SignalingMessage::RTCIceCandidate { .. }) {
                                    let mut msgs = server.pending_messages
                                        .entry(relay_room.clone())
                                        .or_default();
                                    msgs.push(text_str);
                                    // ponytail: cap at 64 messages; real ring-buffer if this overflows
                                    if msgs.len() > 64 {
                                        msgs.remove(0);
                                    }
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::SendError(_)) => {
                            tracing::warn!("Forward: no receivers, message dropped");
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    relay_handle.abort();

    // Clean up SFU resources for the disconnecting peer
    #[cfg(feature = "sfu-mediasoup")]
    {
        server.sfu_manager.remove_peer(&relay_room, &relay_peer_id);
        tracing::info!("SFU: cleaned up peer {} in room {}", relay_peer_id, relay_room);
    }


    // ── Check if leaving peer is a DeviceStream host (before leave_room removes it)
    let is_device_stream_host = server.room_manager.is_device_stream(&relay_room)
        && server
            .room_manager
            .list_rooms()
            .iter()
            .find(|r| r.id == relay_room)
            .and_then(|r| r.host.as_deref())
            == Some(&relay_peer_id);

    #[allow(unused_variables)]
    let room_removed = server.room_manager.leave_room(&relay_room, &relay_peer_id);
    audit::log_event(AuditEvent::PeerLeave {
        peer_id: relay_peer_id.clone(),
        room_id: relay_room.clone(),
    });

    // ── DeviceStream host disconnect: drop all consumers
    if is_device_stream_host {
        tracing::info!("DeviceStream host {} disconnected, disconnecting consumers", relay_peer_id);
        let consumers = server
            .room_manager
            .disconnect_consumers(&relay_room);
        for consumer_id in &consumers {
            audit::log_event(AuditEvent::PeerLeave {
                peer_id: consumer_id.clone(),
                room_id: relay_room.clone(),
            });
            tracing::info!("DeviceStream consumer {} removed (host left)", consumer_id);
        }
        if !consumers.is_empty() {
            tracing::info!("Disconnected {} consumers from room {}",
                consumers.len(), relay_room);
        }
    }

    // If room became empty, also remove it from SFU
    #[cfg(feature = "sfu-mediasoup")]
    if room_removed {
        server.sfu_manager.remove_room(&relay_room);
    }

    let leave_msg = SignalingMessage::RoomLeave {
        room_id: relay_room.clone(),
        peer_id: relay_peer_id.clone(),
    };
    let _ = tx.send(serde_json::to_string(&leave_msg).unwrap());

    tracing::info!(
        "Peer {} disconnected from room {}",
        relay_peer_id,
        relay_room
    );
}

/// Handle SFU transport negotiation and produce/consume messages.
/// Returns `true` if the message was handled (should not be relayed).
#[cfg(feature = "sfu-mediasoup")]
pub(crate) async fn handle_sfu_message(
    msg: &SignalingMessage,
    sfu: &crate::sfu::SfuManager,
    sender: &Arc<tokio::sync::Mutex<SplitSink<WebSocket, Message>>>,
    broadcast_tx: &tokio::sync::broadcast::Sender<String>,
    peer_id: &str,
) -> bool {
    match msg {
        SignalingMessage::CreateWebRtcTransport {
            room_id,
            peer_id,
            direction,
        } => {
            tracing::info!(
                "SFU: creating {} transport for peer {} in room {}",
                serde_json::to_string(direction).unwrap_or_default(),
                peer_id,
                room_id
            );
            let dir_str = match direction {
                omspbase_common::protocol::TransportDirection::Send => "send",
                omspbase_common::protocol::TransportDirection::Recv => "recv",
            };
            match sfu.create_webrtc_transport(room_id, peer_id, dir_str).await {
                Ok(created) => {
                    let response = SignalingMessage::WebRtcTransportCreated {
                        room_id: room_id.clone(),
                        peer_id: peer_id.clone(),
                        transport_id: created.transport_id,
                        ice_parameters: created.ice_parameters,
                        dtls_parameters: created.dtls_parameters,
                    };
                    let _ = sender
                        .lock()
                        .await
                        .send(Message::Text(send_msg(&response).unwrap()))
                        .await;
                }
                Err(e) => {
                    let error = SignalingMessage::Error {
                        code: 5000,
                        message: format!("Transport creation failed: {e}"),
                    };
                    let _ = sender
                        .lock()
                        .await
                        .send(Message::Text(send_msg(&error).unwrap()))
                        .await;
                }
            }
            true
        }
        SignalingMessage::ConnectWebRtcTransport {
            room_id,
            peer_id,
            transport_id,
            dtls_parameters,
        } => {
            match sfu.connect_transport(&room_id, &peer_id, &transport_id, dtls_parameters.clone()).await {
                Ok(()) => {
                    tracing::info!(
                        "SFU: transport {transport_id} connected for peer {peer_id}"
                    );
                    let response = SignalingMessage::Error {
                        code: 0,
                        message: "transport_connected".into(),
                    };
                    let _ = sender
                        .lock()
                        .await
                        .send(Message::Text(send_msg(&response).unwrap()))
                        .await;
                }
                Err(e) => {
                    tracing::error!("SFU: connect transport failed: {e}");
                    let response = SignalingMessage::Error {
                        code: 5000,
                        message: format!("Connect failed: {e}"),
                    };
                    let _ = sender
                        .lock()
                        .await
                        .send(Message::Text(send_msg(&response).unwrap()))
                        .await;
                }
            }
            true
        }

        SignalingMessage::Produce {
            room_id,
            transport_direction,
            kind,
            rtp_parameters,
        } => {
            // ponytail: only process "send" direction; recv produce is a protocol error
            if !matches!(transport_direction, omspbase_common::protocol::TransportDirection::Send) {
                let error = SignalingMessage::Error {
                    code: 4000,
                    message: "Produce requires send transport".into(),
                };
                let _ = sender
                    .lock()
                    .await
                    .send(Message::Text(send_msg(&error).unwrap()))
                    .await;
                return true;
            }

            match sfu
                .create_producer(room_id, peer_id, kind, rtp_parameters.clone())
                .await
            {
                Ok(result) => {
                    // Respond to producer
                    let response = SignalingMessage::Produced {
                        room_id: room_id.clone(),
                        producer_id: result.producer_id.clone(),
                    };
                    let _ = sender
                        .lock()
                        .await
                        .send(Message::Text(send_msg(&response).unwrap()))
                        .await;

                    // Broadcast NewProducer to all peers in room
                    let broadcast = SignalingMessage::NewProducer {
                        room_id: room_id.clone(),
                        producer_id: result.producer_id,
                        peer_id: peer_id.to_string(),
                        kind: result.kind,
                    };
                    let _ = broadcast_tx.send(serde_json::to_string(&broadcast).unwrap());
                    tracing::info!(
                        "SFU: broadcast NewProducer for peer {} in room {}",
                        peer_id, room_id
                    );
                }
                Err(e) => {
                    let error = SignalingMessage::Error {
                        code: 5000,
                        message: format!("Producer creation failed: {e}"),
                    };
                    let _ = sender
                        .lock()
                        .await
                        .send(Message::Text(send_msg(&error).unwrap()))
                        .await;
                }
            }
            true
        }

        SignalingMessage::Consume {
            room_id,
            producer_id,
            rtp_capabilities,
        } => {
            match sfu
                .create_consumer(room_id, peer_id, producer_id, rtp_capabilities.clone())
                .await
            {
                Ok(result) => {
                    let response = SignalingMessage::Consumed {
                        room_id: room_id.clone(),
                        consumer_id: result.consumer_id,
                        producer_id: result.producer_id,
                        kind: result.kind,
                        rtp_parameters: result.rtp_parameters_json,
                    };
                    let _ = sender
                        .lock()
                        .await
                        .send(Message::Text(send_msg(&response).unwrap()))
                        .await;
                }
                Err(e) => {
                    let error = SignalingMessage::Error {
                        code: 5000,
                        message: format!("Consumer creation failed: {e}"),
                    };
                    let _ = sender
                        .lock()
                        .await
                        .send(Message::Text(send_msg(&error).unwrap()))
                        .await;
                }
            }
            true
        }

        _ => false,
    }
}
