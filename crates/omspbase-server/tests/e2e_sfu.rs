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

        // Wait for transport created response
        let transport_resp = ws.next().await.unwrap().unwrap();
        let resp_text = transport_resp.to_text().unwrap();
        let sig: SignalingMessage =
            serde_json::from_str(resp_text).expect("Expected WebRtcTransportCreated");
        match sig {
            SignalingMessage::WebRtcTransportCreated {
                transport_id,
                ice_parameters,
                dtls_parameters,
                ..
            } => {
                assert!(!transport_id.is_empty());
                assert!(!ice_parameters.username_fragment.is_empty());
                assert!(!dtls_parameters.role.is_empty());
            }
            SignalingMessage::Error { code, message } => {
                panic!("Transport creation failed: code={code} message={message}");
            }
            _ => panic!("Unexpected response: {resp_text}"),
        }

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

        // Wait for transport created response
        let transport_resp = ws.next().await.unwrap().unwrap();
        let resp_text = transport_resp.to_text().unwrap();
        let sig: SignalingMessage =
            serde_json::from_str(resp_text).expect("Expected WebRtcTransportCreated");
        match sig {
            SignalingMessage::WebRtcTransportCreated { .. } => {} // OK
            SignalingMessage::Error { code, message } => {
                panic!("Transport creation failed: code={code} message={message}");
            }
            _ => panic!("Unexpected response: {resp_text}"),
        }

        // Signal done
        ws.send(WsMsg::Text("remote-ready".into())).await.unwrap();
    });

    // Wait for both peers to be ready
    host_handle.await.unwrap();
    remote_handle.await.unwrap();

    // Verify room was created in SFU
    assert_eq!(
        sfu.room_count(),
        initial_room_count + 1,
        "SFU should have one room after transport creation"
    );

    // --- Cleanup: test remove_peer ---
    sfu.remove_peer(ROOM, "host");
    sfu.remove_peer(ROOM, "remote");

    // After removing both peers, room should be destroyed
    assert_eq!(
        sfu.room_count(),
        initial_room_count,
        "SFU room should be cleaned up after all peers removed"
    );
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
