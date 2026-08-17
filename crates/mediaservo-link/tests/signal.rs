//! Phase 1b: SignalClient 测试（本地 mock WS server）。

use futures_util::{SinkExt, StreamExt};
use mediaservo_common::protocol::{PeerRole, SignalingMessage};
use mediaservo_link::{SignalClient, SignalEvent};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn connect_auth_join_and_roundtrip() {
    // 本地 mock WS server：PSK 认证 → RoomJoin → RoomJoined → echo
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

        // 1) 收 PSK（首条文本）
        let psk_msg = ws.next().await.unwrap().unwrap();
        assert!(matches!(psk_msg, Message::Text(_)), "首条应为 PSK 文本");

        // 2) 发认证确认 Error{code:0}
        let ack = SignalingMessage::Error { code: 0, message: String::new() };
        ws.send(Message::Text(serde_json::to_string(&ack).unwrap().into()))
            .await
            .unwrap();

        // 3) 收 RoomJoin → 发 RoomJoined
        let join_msg = ws.next().await.unwrap().unwrap();
        let join: SignalingMessage =
            serde_json::from_str(join_msg.to_text().unwrap()).unwrap();
        let room_id = match join {
            SignalingMessage::RoomJoin { room_id, .. } => room_id,
            _ => panic!("expected RoomJoin"),
        };
        let joined = SignalingMessage::RoomJoined { room_id, peer_id: "peer-1".to_string() };
        ws.send(Message::Text(serde_json::to_string(&joined).unwrap().into()))
            .await
            .unwrap();

        // 4) echo loop：收一条回一条
        while let Some(Ok(msg)) = ws.next().await {
            if ws.send(msg).await.is_err() {
                break;
            }
        }
    });

    // 客户端
    let client = SignalClient::new(
        &format!("ws://{addr}/ws"),
        "test-psk",
        "test-room",
        PeerRole::Host,
    );
    let session = client.connect().await.expect("connect");
    assert_eq!(session.room_id(), "test-room");
    let mut events = session.events();

    // 发一条 Sdp，期待 server echo 回来
    session
        .send(SignalingMessage::Sdp {
            room_id: "test-room".to_string(),
            target: None,
            sdp: "v=0".to_string(),
        })
        .await
        .expect("send");

    // 收事件（先 Connected，后 echo 的 Message）
    let echoed = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            match events.recv().await.unwrap() {
                SignalEvent::Message(SignalingMessage::Sdp { sdp, .. }) => return sdp,
                _ => continue, // Connected 等事件跳过
            }
        }
    })
    .await
    .expect("echo timeout");
    assert_eq!(echoed, "v=0");

    session.close().await.expect("close");
    server.await.unwrap();
}

#[tokio::test]
async fn auth_denied_returns_error() {
    // mock server：认证拒绝
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _psk = ws.next().await.unwrap().unwrap();
        let deny = SignalingMessage::Error { code: 4003, message: "PSK authentication failed".to_string() };
        ws.send(Message::Text(serde_json::to_string(&deny).unwrap().into()))
            .await
            .unwrap();
    });

    let client = SignalClient::new(&format!("ws://{addr}/ws"), "wrong-psk", "r", PeerRole::Host);
    let err = client.connect().await.unwrap_err();
    assert!(err.to_string().contains("auth denied [4003]"), "应报认证拒绝，got: {err}");

    server.await.unwrap();
}
