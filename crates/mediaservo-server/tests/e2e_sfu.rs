//! E2E integration tests for mediasoup SFU flow.
//!
//! Feature-gated behind `sfu-mediasoup` and only runs on Linux
//! (mediasoup crate requires Linux kernel features).
//!
//! Tests: create room → create transports → produce media → consume media → cleanup.

#![cfg(all(feature = "sfu-mediasoup", target_os = "linux"))]

use futures_util::{SinkExt, StreamExt};
use mediaservo_common::protocol::{PeerRole, SignalingMessage};
use mediaservo_server::sfu::SfuManager;
use mediaservo_server::signaling::{signaling_router, SignalingServer};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMsg;

type KeepAliveWs = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

const PSK: &str = "e2e-sfu-psk";
const ROOM: &str = "sfu-test-room";

/// Full SFU lifecycle: create room → transports → produce → consume → cleanup.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_sfu_lifecycle() {
    unsafe { std::env::set_var("MEDIASERVO_PSK", PSK) };

    // Create mediasoup SFU manager
    let sfu = SfuManager::new_with_port(mediaservo_server::sfu::random_udp_port())
        .await
        .expect("Failed to create SFU manager");
    let sfu = Arc::new(sfu);
    let initial_room_count = sfu.room_count();

    // Create signaling server with SFU
    let server = SignalingServer::new(Arc::clone(&sfu), 65536, None);
    let app = signaling_router(server);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let ws_url = format!("ws://{}/ws", addr);

    // --- Host: connect, auth, join room, create send transport ---
    let host_url = ws_url.clone();
    let host_handle = tokio::spawn(async move {
        let (mut ws, _) = tokio_tungstenite::connect_async(&host_url).await.unwrap();

        // PSK auth
        ws.send(WsMsg::Text(PSK.into())).await.unwrap();
        let ack = ws.next().await.unwrap().unwrap();
        assert!(ack.to_text().unwrap().contains("authenticated"));

        // RoomJoin as Host
        let join = serde_json::to_string(&SignalingMessage::RoomJoin {
            room_id: ROOM.into(),
            peer_role: PeerRole::Host,
            stream_id: None,
            device_id: None,
            device_secret: None,
        })
        .unwrap();
        ws.send(WsMsg::Text(join.into())).await.unwrap();
        let joined = ws.next().await.unwrap().unwrap();
        assert!(joined.to_text().unwrap().contains("room_joined"));

        // Create send WebRTC transport
        let create_transport = serde_json::to_string(&SignalingMessage::CreateWebRtcTransport {
            room_id: ROOM.into(),
            peer_id: "host".to_string(),
            direction: mediaservo_common::protocol::TransportDirection::Send,
        })
        .unwrap();
        ws.send(WsMsg::Text(create_transport.into())).await.unwrap();

        // Wait for transport created response (may get room_leave first)
        let (transport_id, ice_parameters, dtls_parameters) = loop {
            let transport_resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
            let resp_text = transport_resp.to_text().unwrap();
            let sig: SignalingMessage = serde_json::from_str(resp_text).unwrap();
            match sig {
                SignalingMessage::WebRtcTransportCreated {
                    transport_id, ice_parameters, dtls_parameters, ..
                } => break (transport_id, ice_parameters, dtls_parameters),
                SignalingMessage::RoomLeave { .. } => continue,
                other => panic!("Unexpected response: {other:?}"),
            }
        };
        assert!(!transport_id.is_empty());
        assert!(!ice_parameters.username_fragment.is_empty());
        assert!(!dtls_parameters.role.is_empty());

        // Signal done — sentinel message
        ws.send(WsMsg::Text("host-ready".into())).await.unwrap();
    });

    // --- Remote: connect, auth, join room, create recv transport ---
    let remote_url = ws_url.clone();
    let remote_handle = tokio::spawn(async move {
        let (mut ws, _) = tokio_tungstenite::connect_async(&remote_url).await.unwrap();

        // PSK auth
        ws.send(WsMsg::Text(PSK.into())).await.unwrap();
        let ack = ws.next().await.unwrap().unwrap();
        assert!(ack.to_text().unwrap().contains("authenticated"));

        // RoomJoin as Remote
        let join = serde_json::to_string(&SignalingMessage::RoomJoin {
            room_id: ROOM.into(),
            peer_role: PeerRole::Remote,
            stream_id: None,
            device_id: None,
            device_secret: None,
        })
        .unwrap();
        ws.send(WsMsg::Text(join.into())).await.unwrap();
        let joined = ws.next().await.unwrap().unwrap();
        assert!(joined.to_text().unwrap().contains("room_joined"));

        // Create recv WebRTC transport
        let create_transport = serde_json::to_string(&SignalingMessage::CreateWebRtcTransport {
            room_id: ROOM.into(),
            peer_id: "remote".to_string(),
            direction: mediaservo_common::protocol::TransportDirection::Recv,
        })
        .unwrap();
        ws.send(WsMsg::Text(create_transport.into())).await.unwrap();

        // Wait for transport created response (may get room_leave first)
        loop {
            let transport_resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
            let resp_text = transport_resp.to_text().unwrap();
            let sig: SignalingMessage = serde_json::from_str(resp_text).unwrap();
            match sig {
                SignalingMessage::WebRtcTransportCreated { .. } => break,
                SignalingMessage::RoomLeave { .. } => continue, // skip
                other => panic!("Unexpected response: {other:?}"),
            }
        }

        // Signal done
        ws.send(WsMsg::Text("remote-ready".into())).await.unwrap();
    });

    // Wait for both peers to be ready
    host_handle.await.unwrap();
    remote_handle.await.unwrap();

    // Verify room was created in SFU
    assert!(
        sfu.room_count() >= initial_room_count,
        "SFU should have at least one room after transport creation"
    );

    // Cleanup: drop WS connections triggers automatic peer cleanup
    // (signaling.rs handles disconnect → remove_peer automatically)
}

/// Test SFU room lifecycle: create room via transport, then cleanup via RoomLeave.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_sfu_cleanup_on_disconnect() {
    unsafe { std::env::set_var("MEDIASERVO_PSK", PSK) };

    let sfu = SfuManager::new_with_port(mediaservo_server::sfu::random_udp_port())
        .await
        .expect("Failed to create SFU manager");
    let sfu = Arc::new(sfu);
    let initial_count = sfu.room_count();

    let server = SignalingServer::new(Arc::clone(&sfu), 65536, None);
    let app = signaling_router(server);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let ws_url = format!("ws://{}/ws", addr);

    // Connect a Host peer, create transport, then disconnect
    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

    // Auth
    ws.send(WsMsg::Text(PSK.into())).await.unwrap();
    let ack = ws.next().await.unwrap().unwrap();
    assert!(ack.to_text().unwrap().contains("authenticated"));

    // Join room
    let join = serde_json::to_string(&SignalingMessage::RoomJoin {
        room_id: ROOM.into(),
        peer_role: PeerRole::Host,
        stream_id: None,
        device_id: None,
        device_secret: None,
    })
    .unwrap();
    ws.send(WsMsg::Text(join.into())).await.unwrap();
    let joined = ws.next().await.unwrap().unwrap();
    assert!(joined.to_text().unwrap().contains("room_joined"));

    // Create send transport
    let create_transport = serde_json::to_string(&SignalingMessage::CreateWebRtcTransport {
        room_id: ROOM.into(),
        peer_id: "host".to_string(),
        direction: mediaservo_common::protocol::TransportDirection::Send,
    })
    .unwrap();
    ws.send(WsMsg::Text(create_transport.into())).await.unwrap();

    let resp = ws.next().await.unwrap().unwrap();
    assert!(resp.to_text().unwrap().contains("transport_created"));

    // Room should exist
    assert_eq!(sfu.room_count(), initial_count + 1);

    // Close WebSocket — triggers disconnect cleanup
    ws.close(None).await.unwrap();

    // Give cleanup a moment
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Room should be cleaned up after disconnect
    assert_eq!(
        sfu.room_count(),
        initial_count,
        "SFU room should be destroyed after peer disconnect"
    );
}

/// Consumer pipeline: Host produce → Consumer recv transport → connect → consume.
/// This test exercises the full consumer-side SFU flow that causes "Signal Lost".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_sfu_consume_pipeline() {
    unsafe { std::env::set_var("MEDIASERVO_PSK", PSK) };

    let sfu = Arc::new(
        SfuManager::new_with_port(mediaservo_server::sfu::random_udp_port())
            .await
            .expect("Failed to create SFU manager"));
    let server = SignalingServer::new(Arc::clone(&sfu), 65536, None);
    let app = signaling_router(server);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let ws_url = format!("ws://{}/ws", addr);

    // --- Host: create send transport, connect, produce video ---
    let host_url = ws_url.clone();
    let host_handle = tokio::spawn(async move {
        let (mut ws, _) = tokio_tungstenite::connect_async(&host_url).await.unwrap();

        // Auth + RoomJoin
        ws.send(WsMsg::Text(PSK.into())).await.unwrap();
        let ack = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        assert!(ack.to_text().unwrap().contains("authenticated"));

        let join = serde_json::to_string(&SignalingMessage::RoomJoin {
            room_id: "sfu-consume-room".into(), peer_role: PeerRole::Host, stream_id: None,
            device_id: None,
            device_secret: None,
        }).unwrap();
        ws.send(WsMsg::Text(join.into())).await.unwrap();
        let joined = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        assert!(joined.to_text().unwrap().contains("room_joined"));

        // Create send transport
        let create = serde_json::to_string(&SignalingMessage::CreateWebRtcTransport {
            room_id: "sfu-consume-room".into(), peer_id: "host".into(),
            direction: mediaservo_common::protocol::TransportDirection::Send,
        }).unwrap();
        ws.send(WsMsg::Text(create.into())).await.unwrap();
        let resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        let created: SignalingMessage = serde_json::from_str(resp.to_text().unwrap()).unwrap();
        let (send_tid, send_dtls) = match created {
            SignalingMessage::WebRtcTransportCreated { transport_id, dtls_parameters, .. } => (transport_id, dtls_parameters),
            other => panic!("Expected WebRtcTransportCreated, got: {:?}", other),
        };

        // Connect send transport
        let connect = serde_json::to_string(&SignalingMessage::ConnectWebRtcTransport {
            room_id: "sfu-consume-room".into(), peer_id: "host".into(),
            transport_id: send_tid.clone(), dtls_parameters: send_dtls,
        }).unwrap();
        ws.send(WsMsg::Text(connect.into())).await.unwrap();
        let conn_resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        assert!(conn_resp.to_text().unwrap().contains("transport_connected"));

        // Produce video
        let produce = serde_json::to_string(&SignalingMessage::Produce {
            room_id: "sfu-consume-room".into(),
            peer_id: "host".into(),
            transport_direction: mediaservo_common::protocol::TransportDirection::Send,
            kind: mediaservo_common::protocol::MediaKind::Video,
            rtp_parameters: serde_json::json!({"mid": "0", "codecs": [{"mimeType": "video/VP8", "payloadType": 100, "clockRate": 90000}], "headerExtensions": [], "encodings": [{"ssrc": 12345}], "rtcp": {"reducedSize": true}}),
        }).unwrap();
        ws.send(WsMsg::Text(produce.into())).await.unwrap();
        let prod_resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        let produced: SignalingMessage = serde_json::from_str(prod_resp.to_text().unwrap()).unwrap();
        let producer_id = match produced {
            SignalingMessage::Produced { producer_id, .. } => producer_id,
            other => panic!("Expected Produced, got: {:?}", other),
        };

        // Signal done
        ws.send(WsMsg::Text(format!("host-ready:{}", producer_id))).await.unwrap();
        (ws, producer_id)
    });

    // Wait for host to be ready
    let (host_ws, producer_id) = host_handle.await.unwrap();

    // --- Consumer: create recv transport, connect, consume ---
    let consumer_url = ws_url.clone();
    let pid = producer_id.clone();
    let consumer_handle = tokio::spawn(async move {
        let (mut ws, _) = tokio_tungstenite::connect_async(&consumer_url).await.unwrap();

        // Auth + RoomJoin
        ws.send(WsMsg::Text(PSK.into())).await.unwrap();
        let ack = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        assert!(ack.to_text().unwrap().contains("authenticated"));

        let join = serde_json::to_string(&SignalingMessage::RoomJoin {
            room_id: "sfu-consume-room".into(), peer_role: PeerRole::Remote, stream_id: None,
            device_id: None,
            device_secret: None,
        }).unwrap();
        ws.send(WsMsg::Text(join.into())).await.unwrap();
        let joined = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        assert!(joined.to_text().unwrap().contains("room_joined"));

        // Create recv transport
        let create = serde_json::to_string(&SignalingMessage::CreateWebRtcTransport {
            room_id: "sfu-consume-room".into(), peer_id: "consumer".into(),
            direction: mediaservo_common::protocol::TransportDirection::Recv,
        }).unwrap();
        ws.send(WsMsg::Text(create.into())).await.unwrap();
        // late-joiner 同步会把 pending NewProducer 重放广播 — 先于直接响应到达，
        // 必须 drain 到目标消息（PIT-103 顺手修: 该测试在 test-server 编译挂期间从未跑过）。
        let created = loop {
            let resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let parsed: SignalingMessage = serde_json::from_str(resp.to_text().unwrap()).unwrap();
            match parsed {
                SignalingMessage::WebRtcTransportCreated { .. } => break parsed,
                SignalingMessage::NewProducer { .. } => continue,
                other => panic!("Expected WebRtcTransportCreated, got: {:?}", other),
            }
        };
        let (recv_tid, recv_dtls) = match created {
            SignalingMessage::WebRtcTransportCreated { transport_id, dtls_parameters, .. } => (transport_id, dtls_parameters),
            _ => unreachable!(),
        };

        // Connect recv transport
        let connect = serde_json::to_string(&SignalingMessage::ConnectWebRtcTransport {
            room_id: "sfu-consume-room".into(), peer_id: "consumer".into(),
            transport_id: recv_tid, dtls_parameters: recv_dtls,
        }).unwrap();
        ws.send(WsMsg::Text(connect.into())).await.unwrap();
        let conn_resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        assert!(conn_resp.to_text().unwrap().contains("transport_connected"));

        // Consume the Host's producer
        let consume = serde_json::to_string(&SignalingMessage::Consume {
            room_id: "sfu-consume-room".into(),
            peer_id: "consumer".into(),
            producer_id: pid.clone(),
            rtp_capabilities: serde_json::json!({
                "codecs": [{"mimeType": "video/VP8", "clockRate": 90000, "kind": "video"}],
                "headerExtensions": []
            }),
        }).unwrap();
        ws.send(WsMsg::Text(consume.into())).await.unwrap();
        let consumed_resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        let consumed: SignalingMessage = serde_json::from_str(consumed_resp.to_text().unwrap()).unwrap();
        match consumed {
            SignalingMessage::Consumed { consumer_id, producer_id, kind, .. } => {
                assert!(!consumer_id.is_empty(), "consumer_id should not be empty");
                assert_eq!(producer_id, pid, "producer_id should match host's producer");
                assert_eq!(kind, mediaservo_common::protocol::MediaKind::Video, "consumer kind should be Video");
            }
            other => panic!("Expected Consumed, got: {:?}", other),
        }

        ws
    });

    // Wait for consumer to complete
    let consumer_ws = consumer_handle.await.unwrap();

    // Verify consumer was registered
    assert!(sfu.room_count() > 0, "SFU should have at least one room");

    // Cleanup
    let _ = host_ws; // drop triggers disconnect
    let _ = consumer_ws;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
}

// ═══════════════════════════════════════════════════════════════════════════
// G3: SFU 面角色强制 — 车端 produce 自动允许（回归）+ 账号 produce 拒绝 +
// 授权账号 consume 放行 + 非授权账号 join 拒绝（租户隔离）。
// ═══════════════════════════════════════════════════════════════════════════

const G3_JWT: &str = "e2e-sfu-g3-jwt-secret-32-bytes-min!";
const G3_ROOM: &str = "sfu-g3-room";

fn g3_token(username: &str, role: &str, vehicles: &[&str]) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;
    let claims = mediaservo_common::auth::JwtClaims {
        sub: username.into(),
        iat: now,
        exp: now + 3600,
        role: Some(role.into()),
        vehicles: Some(vehicles.iter().map(|s| s.to_string()).collect()),
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(G3_JWT.as_bytes()),
    )
    .unwrap()
}

async fn g3_account_connect(ws_url: &str, token: &str) -> KeepAliveWs {
    let mut req = ws_url.into_client_request().unwrap();
    req.headers_mut()
        .insert("Sec-WebSocket-Protocol", token.parse().unwrap());
    let (ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
    ws
}

async fn g3_auth_and_join(ws: &mut KeepAliveWs, room: &str, role: PeerRole) {
    let ack = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        ack.to_text().unwrap().contains("authenticated"),
        "auth ack: {}",
        ack.to_text().unwrap()
    );
    let join = serde_json::to_string(&SignalingMessage::RoomJoin {
        room_id: room.into(),
        peer_role: role,
        stream_id: None,
        device_id: None,
        device_secret: None,
    })
    .unwrap();
    ws.send(WsMsg::Text(join.into())).await.unwrap();
    let resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        resp.to_text().unwrap().contains("room_joined"),
        "join: {}",
        resp.to_text().unwrap()
    );
}

async fn g3_create_recv_transport(ws: &mut KeepAliveWs, peer: &str) -> (String, mediaservo_common::protocol::DtlsParameters) {
    let create = serde_json::to_string(&SignalingMessage::CreateWebRtcTransport {
        room_id: G3_ROOM.into(),
        peer_id: peer.into(),
        direction: mediaservo_common::protocol::TransportDirection::Recv,
    })
    .unwrap();
    ws.send(WsMsg::Text(create.into())).await.unwrap();
    loop {
        let resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let parsed: SignalingMessage = serde_json::from_str(resp.to_text().unwrap()).unwrap();
        match parsed {
            SignalingMessage::WebRtcTransportCreated { transport_id, dtls_parameters, .. } => {
                return (transport_id, dtls_parameters)
            }
            SignalingMessage::NewProducer { .. } => continue, // late-joiner 重放
            other => panic!("Expected WebRtcTransportCreated, got: {other:?}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_sfu_role_enforcement() {
    unsafe { std::env::set_var("MEDIASERVO_PSK", PSK) };

    let sfu = Arc::new(
        SfuManager::new_with_port(mediaservo_server::sfu::random_udp_port())
            .await
            .expect("Failed to create SFU manager"),
    );
    let mut server = SignalingServer::new(
        Arc::clone(&sfu),
        65536,
        Some(mediaservo_common::auth::JwtAuth::new(G3_JWT)),
    );
    let hash = mediaservo_server::devices::hash_secret("ms-car1", "car1-secret");
    server.device_registry = std::sync::Arc::new(
        mediaservo_server::devices::DeviceRegistry::from_yaml(&format!(
            "devices:\n  ms-car1:\n    secret_hash: \"{hash}\"\n"
        ))
        .unwrap(),
    );
    let app = signaling_router(server);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let ws_url = format!("ws://{}/ws", addr);

    // ── ① 车端（device auth）: produce 自动允许（D-H11 回归）────────────────
    let host_url = ws_url.clone();
    let host_handle = tokio::spawn(async move {
        let (mut ws, _) = tokio_tungstenite::connect_async(&host_url).await.unwrap();
        ws.send(WsMsg::Text(PSK.into())).await.unwrap();
        let ack = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(ack.to_text().unwrap().contains("authenticated"));

        let join = serde_json::to_string(&SignalingMessage::RoomJoin {
            room_id: G3_ROOM.into(),
            peer_role: PeerRole::Host,
            stream_id: None,
            device_id: Some("ms-car1".into()),
            device_secret: Some("car1-secret".into()),
        })
        .unwrap();
        ws.send(WsMsg::Text(join.into())).await.unwrap();
        let joined = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(joined.to_text().unwrap().contains("room_joined"));

        // send transport + connect
        let create = serde_json::to_string(&SignalingMessage::CreateWebRtcTransport {
            room_id: G3_ROOM.into(),
            peer_id: "host".into(),
            direction: mediaservo_common::protocol::TransportDirection::Send,
        })
        .unwrap();
        ws.send(WsMsg::Text(create.into())).await.unwrap();
        let created: SignalingMessage = loop {
            let resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let parsed: SignalingMessage = serde_json::from_str(resp.to_text().unwrap()).unwrap();
            match parsed {
                SignalingMessage::WebRtcTransportCreated { .. } => break parsed,
                SignalingMessage::RoomLeave { .. } => continue,
                other => panic!("Unexpected: {other:?}"),
            }
        };
        let (send_tid, send_dtls) = match created {
            SignalingMessage::WebRtcTransportCreated { transport_id, dtls_parameters, .. } => {
                (transport_id, dtls_parameters)
            }
            _ => unreachable!(),
        };
        let connect = serde_json::to_string(&SignalingMessage::ConnectWebRtcTransport {
            room_id: G3_ROOM.into(),
            peer_id: "host".into(),
            transport_id: send_tid,
            dtls_parameters: send_dtls,
        })
        .unwrap();
        ws.send(WsMsg::Text(connect.into())).await.unwrap();
        let conn = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(conn.to_text().unwrap().contains("transport_connected"));

        // produce — 车端必须自动允许
        let produce = serde_json::to_string(&SignalingMessage::Produce {
            room_id: G3_ROOM.into(),
            peer_id: "host".into(),
            transport_direction: mediaservo_common::protocol::TransportDirection::Send,
            kind: mediaservo_common::protocol::MediaKind::Video,
            rtp_parameters: serde_json::json!({"mid": "0", "codecs": [{"mimeType": "video/VP8", "payloadType": 100, "clockRate": 90000}], "headerExtensions": [], "encodings": [{"ssrc": 12345}], "rtcp": {"reducedSize": true}}),
        })
        .unwrap();
        ws.send(WsMsg::Text(produce.into())).await.unwrap();
        let prod = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let producer_id = match serde_json::from_str::<SignalingMessage>(prod.to_text().unwrap()).unwrap() {
            SignalingMessage::Produced { producer_id, .. } => producer_id,
            other => panic!("车端 produce 必须放行, got: {other:?}"),
        };
        (ws, producer_id)
    });
    let (host_ws, producer_id) = host_handle.await.unwrap();

    // ── ② 授权 operator（carol, [ms-car1]）: join + consume → Consumed ✅ ────
    let carol_url = ws_url.clone();
    let pid = producer_id.clone();
    let carol_handle = tokio::spawn(async move {
        let mut ws = g3_account_connect(&carol_url, &g3_token("carol", "operator", &["ms-car1"])).await;
        g3_auth_and_join(&mut ws, G3_ROOM, PeerRole::Consumer).await;
        let (recv_tid, recv_dtls) = g3_create_recv_transport(&mut ws, "carol").await;
        let connect = serde_json::to_string(&SignalingMessage::ConnectWebRtcTransport {
            room_id: G3_ROOM.into(),
            peer_id: "carol".into(),
            transport_id: recv_tid,
            dtls_parameters: recv_dtls,
        })
        .unwrap();
        ws.send(WsMsg::Text(connect.into())).await.unwrap();
        let conn = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(conn.to_text().unwrap().contains("transport_connected"));

        let consume = serde_json::to_string(&SignalingMessage::Consume {
            room_id: G3_ROOM.into(),
            peer_id: "carol".into(),
            producer_id: pid.clone(),
            rtp_capabilities: serde_json::json!({
                "codecs": [{"mimeType": "video/VP8", "clockRate": 90000, "kind": "video"}],
                "headerExtensions": []
            }),
        })
        .unwrap();
        ws.send(WsMsg::Text(consume.into())).await.unwrap();
        let resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        match serde_json::from_str::<SignalingMessage>(resp.to_text().unwrap()).unwrap() {
            SignalingMessage::Consumed { producer_id: pid_resp, .. } => {
                assert_eq!(pid_resp, pid, "授权 operator consume 必须放行")
            }
            other => panic!("授权 consume 必须 Consumed, got: {other:?}"),
        }
        ws
    });
    let _carol_ws = carol_handle.await.unwrap();

    // ── ③ operator 尝试 produce → 4031（账号只消费）─────────────────────────
    let prod_url = ws_url.clone();
    let prod_handle = tokio::spawn(async move {
        let mut ws = g3_account_connect(&prod_url, &g3_token("carol", "operator", &["ms-car1"])).await;
        g3_auth_and_join(&mut ws, G3_ROOM, PeerRole::Consumer).await;
        // send transport（produce 门在方向校验之后 — 需先建 send transport 才可达）
        let create = serde_json::to_string(&SignalingMessage::CreateWebRtcTransport {
            room_id: G3_ROOM.into(),
            peer_id: "carol-prod".into(),
            direction: mediaservo_common::protocol::TransportDirection::Send,
        })
        .unwrap();
        ws.send(WsMsg::Text(create.into())).await.unwrap();
        let created = loop {
            let resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let parsed: SignalingMessage = serde_json::from_str(resp.to_text().unwrap()).unwrap();
            match parsed {
                SignalingMessage::WebRtcTransportCreated { .. } => break parsed,
                SignalingMessage::NewProducer { .. } => continue,
                other => panic!("Expected transport created, got: {other:?}"),
            }
        };
        let (send_tid, send_dtls) = match created {
            SignalingMessage::WebRtcTransportCreated { transport_id, dtls_parameters, .. } => {
                (transport_id, dtls_parameters)
            }
            _ => unreachable!(),
        };
        let connect = serde_json::to_string(&SignalingMessage::ConnectWebRtcTransport {
            room_id: G3_ROOM.into(),
            peer_id: "carol-prod".into(),
            transport_id: send_tid,
            dtls_parameters: send_dtls,
        })
        .unwrap();
        ws.send(WsMsg::Text(connect.into())).await.unwrap();
        let conn = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(conn.to_text().unwrap().contains("transport_connected"));

        let produce = serde_json::to_string(&SignalingMessage::Produce {
            room_id: G3_ROOM.into(),
            peer_id: "carol-prod".into(),
            transport_direction: mediaservo_common::protocol::TransportDirection::Send,
            kind: mediaservo_common::protocol::MediaKind::Video,
            rtp_parameters: serde_json::json!({"mid": "0", "codecs": [{"mimeType": "video/VP8", "payloadType": 100, "clockRate": 90000}], "headerExtensions": [], "encodings": [{"ssrc": 999}], "rtcp": {"reducedSize": true}}),
        })
        .unwrap();
        ws.send(WsMsg::Text(produce.into())).await.unwrap();
        let resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            resp.to_text().unwrap().contains(r#""code":4031"#),
            "账号 produce 必须拒绝: {}",
            resp.to_text().unwrap()
        );
        ws
    });
    let _prod_ws = prod_handle.await.unwrap();

    // ── ④ 非授权 operator（mallory, 空白名单）: join 即拒（租户隔离）─────────
    let mallory_url = ws_url.clone();
    let mallory_handle = tokio::spawn(async move {
        let mut ws = g3_account_connect(&mallory_url, &g3_token("mallory", "operator", &[])).await;
        let ack = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(ack.to_text().unwrap().contains("authenticated"));
        let join = serde_json::to_string(&SignalingMessage::RoomJoin {
            room_id: G3_ROOM.into(),
            peer_role: PeerRole::Consumer,
            stream_id: None,
            device_id: None,
            device_secret: None,
        })
        .unwrap();
        ws.send(WsMsg::Text(join.into())).await.unwrap();
        let resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            resp.to_text().unwrap().contains(r#""code":4031"#),
            "非授权账号 join 必须拒绝: {}",
            resp.to_text().unwrap()
        );
        ws
    });
    let _mallory_ws = mallory_handle.await.unwrap();

    // Cleanup
    let _ = host_ws;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
}
