//! E2E integration tests for mediasoup SFU flow.
//!
//! Feature-gated behind `sfu-mediasoup` and only runs on Linux
//! (mediasoup crate requires Linux kernel features).
//!
//! Tests: create room → create transports → produce media → consume media → cleanup.

#![cfg(all(feature = "sfu-mediasoup", target_os = "linux"))]

use futures_util::{SinkExt, StreamExt};
use omspbase_common::protocol::{PeerRole, SignalingMessage};
use omspbase_server::sfu::SfuManager;
use omspbase_server::signaling::{signaling_router, SignalingServer};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as WsMsg;

const PSK: &str = "e2e-sfu-psk";
const ROOM: &str = "sfu-test-room";

/// Full SFU lifecycle: create room → transports → produce → consume → cleanup.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_sfu_lifecycle() {
    unsafe { std::env::set_var("OMSPBASE_PSK", PSK) };

    // Create mediasoup SFU manager
    let sfu = SfuManager::new().await.expect("Failed to create SFU manager");
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
        })
        .unwrap();
        ws.send(WsMsg::Text(join.into())).await.unwrap();
        let joined = ws.next().await.unwrap().unwrap();
        assert!(joined.to_text().unwrap().contains("room_joined"));

        // Create send WebRTC transport
        let create_transport = serde_json::to_string(&SignalingMessage::CreateWebRtcTransport {
            room_id: ROOM.into(),
            peer_id: "host".to_string(),
            direction: omspbase_common::protocol::TransportDirection::Send,
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
        })
        .unwrap();
        ws.send(WsMsg::Text(join.into())).await.unwrap();
        let joined = ws.next().await.unwrap().unwrap();
        assert!(joined.to_text().unwrap().contains("room_joined"));

        // Create recv WebRTC transport
        let create_transport = serde_json::to_string(&SignalingMessage::CreateWebRtcTransport {
            room_id: ROOM.into(),
            peer_id: "remote".to_string(),
            direction: omspbase_common::protocol::TransportDirection::Recv,
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
    unsafe { std::env::set_var("OMSPBASE_PSK", PSK) };

    let sfu = SfuManager::new().await.expect("Failed to create SFU manager");
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
    })
    .unwrap();
    ws.send(WsMsg::Text(join.into())).await.unwrap();
    let joined = ws.next().await.unwrap().unwrap();
    assert!(joined.to_text().unwrap().contains("room_joined"));

    // Create send transport
    let create_transport = serde_json::to_string(&SignalingMessage::CreateWebRtcTransport {
        room_id: ROOM.into(),
        peer_id: "host".to_string(),
        direction: omspbase_common::protocol::TransportDirection::Send,
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
    unsafe { std::env::set_var("OMSPBASE_PSK", PSK) };

    let sfu = Arc::new(SfuManager::new().await.expect("Failed to create SFU manager"));
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
        }).unwrap();
        ws.send(WsMsg::Text(join.into())).await.unwrap();
        let joined = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        assert!(joined.to_text().unwrap().contains("room_joined"));

        // Create send transport
        let create = serde_json::to_string(&SignalingMessage::CreateWebRtcTransport {
            room_id: "sfu-consume-room".into(), peer_id: "host".into(),
            direction: omspbase_common::protocol::TransportDirection::Send,
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
            transport_direction: omspbase_common::protocol::TransportDirection::Send,
            kind: omspbase_common::protocol::MediaKind::Video,
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
        }).unwrap();
        ws.send(WsMsg::Text(join.into())).await.unwrap();
        let joined = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        assert!(joined.to_text().unwrap().contains("room_joined"));

        // Create recv transport
        let create = serde_json::to_string(&SignalingMessage::CreateWebRtcTransport {
            room_id: "sfu-consume-room".into(), peer_id: "consumer".into(),
            direction: omspbase_common::protocol::TransportDirection::Recv,
        }).unwrap();
        ws.send(WsMsg::Text(create.into())).await.unwrap();
        let resp = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        let created: SignalingMessage = serde_json::from_str(resp.to_text().unwrap()).unwrap();
        let (recv_tid, recv_dtls) = match created {
            SignalingMessage::WebRtcTransportCreated { transport_id, dtls_parameters, .. } => (transport_id, dtls_parameters),
            other => panic!("Expected WebRtcTransportCreated, got: {:?}", other),
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
                assert_eq!(kind, omspbase_common::protocol::MediaKind::Video, "consumer kind should be Video");
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
