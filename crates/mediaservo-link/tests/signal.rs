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
    assert_eq!(session.peer_id(), "peer-1", "RoomJoined 的 peer_id 应可访问");
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

// ---- Phase B (B1): 重连（指数退避 + jitter）与断线通知 ----

/// mock server：前 `refuse` 次连接 TCP 立断（WS 握手失败），之后完整认证+入房；
/// 累计接受 `total` 次连接后退出（防客户端放弃后 accept 永久阻塞）。
async fn refuse_then_serve(refuse: usize, total: usize) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let server = tokio::spawn({
        let attempts = attempts.clone();
        async move {
            let mut conn = 0usize;
            while conn < total {
                let (stream, _) = listener.accept().await.unwrap();
                conn += 1;
                attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if conn <= refuse {
                    drop(stream); // 拒绝：TCP 立即关闭 → connect_async 报错
                    continue;
                }
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let psk_msg = ws.next().await.unwrap().unwrap();
                assert!(matches!(psk_msg, Message::Text(_)));
                let ack = SignalingMessage::Error { code: 0, message: String::new() };
                ws.send(Message::Text(serde_json::to_string(&ack).unwrap().into())).await.unwrap();
                let join_msg = ws.next().await.unwrap().unwrap();
                let join: SignalingMessage = serde_json::from_str(join_msg.to_text().unwrap()).unwrap();
                let room_id = match join {
                    SignalingMessage::RoomJoin { room_id, .. } => room_id,
                    _ => panic!("expected RoomJoin"),
                };
                let joined = SignalingMessage::RoomJoined { room_id, peer_id: "peer-1".to_string() };
                ws.send(Message::Text(serde_json::to_string(&joined).unwrap().into())).await.unwrap();
                return; // 本轮服务完成，剩余连接不再处理
            }
        }
    });
    (addr, server, attempts)
}

#[tokio::test]
async fn connect_with_retry_refuses_then_succeeds() {
    // 先拒 2 次（重试 2 轮），第 3 次连接成功 → 会话可用
    let (addr, server, attempts) = refuse_then_serve(2, 3).await;
    let client = SignalClient::new(&format!("ws://{addr}/ws"), "test-psk", "test-room", PeerRole::Host);
    let session = client
        .connect_with_retry(mediaservo_link::RetryConfig {
            max_retries: 3,
            base_delay: std::time::Duration::from_millis(50),
            max_delay: std::time::Duration::from_secs(1),
        })
        .await
        .expect("retry should succeed");
    assert_eq!(session.room_id(), "test-room");
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3, "应尝试 3 次（2 拒 + 1 收）");
    session.close().await.expect("close");
    server.await.unwrap();
}

#[tokio::test]
async fn connect_with_retry_exhausts_max_retries() {
    // 永远拒绝 → max_retries 次重试后返回错误，且每次等待指数退避
    let (addr, server, attempts) = refuse_then_serve(3, 3).await;
    let client = SignalClient::new(&format!("ws://{addr}/ws"), "test-psk", "test-room", PeerRole::Host);
    let started = std::time::Instant::now();
    let err = client
        .connect_with_retry(mediaservo_link::RetryConfig {
            max_retries: 2,
            base_delay: std::time::Duration::from_millis(50),
            max_delay: std::time::Duration::from_secs(1),
        })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("after 2 retries"), "应报告重试次数，got: {err}");
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3, "应尝试 3 次（1 初 + 2 重试）");
    // ±25% jitter：两次退避 50ms/100ms 的 75% 下限合计 ≥ 100ms
    assert!(started.elapsed() >= std::time::Duration::from_millis(100), "应等待退避，elapsed={:?}", started.elapsed());
    server.await.unwrap();
}

#[tokio::test]
async fn on_disconnect_fires_when_server_closes() {
    // mock server：完整握手后，等客户端 ready 消息再主动 Close
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _psk = ws.next().await.unwrap().unwrap();
        let ack = SignalingMessage::Error { code: 0, message: String::new() };
        ws.send(Message::Text(serde_json::to_string(&ack).unwrap().into())).await.unwrap();
        let _join = ws.next().await.unwrap().unwrap();
        let joined = SignalingMessage::RoomJoined { room_id: "r".to_string(), peer_id: "peer-1".to_string() };
        ws.send(Message::Text(serde_json::to_string(&joined).unwrap().into())).await.unwrap();
        let _ready = ws.next().await.unwrap().unwrap(); // 等客户端就绪
        ws.close(None).await.unwrap();
    });

    let client = SignalClient::new(&format!("ws://{addr}/ws"), "test-psk", "r", PeerRole::Host);
    let session = client.connect().await.expect("connect");
    let (tx, mut rx) = tokio::sync::watch::channel(());
    session.on_disconnect(Box::new(move || {
        let _ = tx.send(());
    }));
    // 通知 server 关闭
    session
        .send(SignalingMessage::Sdp {
            room_id: "r".to_string(),
            target: None,
            sdp: "v=0".to_string(),
        })
        .await
        .expect("send ready");
    tokio::time::timeout(std::time::Duration::from_secs(3), rx.changed())
        .await
        .expect("on_disconnect 应在 server 关闭时触发（3s 超时）")
        .expect("watch channel 不应关闭");
    server.await.unwrap();
}
