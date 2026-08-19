//! D1 host-agent 信令网关测试（本地 mock WS server，无 Docker 依赖）。
//!
//! 覆盖契约（Momus HIGH-1 协议语义）:
//! ① 多本地客户端 → 单远端 WS，双向转发 + 房间重写（整车房间 ↔ 子进程房间）
//! ② RoomJoin 拦截：子进程 RoomJoin 不上行（agent 单次 join）；子进程本地
//!    合成 RoomJoined（携带整车 peer_id）
//! ③ 并发协商：两路 CreateWebRtcTransport 在途 → 响应按 FIFO 路由不串
//!    （NewProducer 广播夹在响应之间不消耗 FIFO）
//! ④ 断线重连：远端 WS 断开 → B1 重连 → 转发恢复；断线在途请求清空
//! ⑤ P2P Sdp/ICE 单协商路由：回显去重 + 协商归属切换

use std::net::SocketAddr;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use mediaservo_common::protocol::{
    DtlsParameters, Fingerprint, IceParameters, MediaKind, PeerRole, SignalingMessage,
    TransportDirection,
};
use mediaservo_host::gateway::{run_gateway, GatewayConfig, LocalEnvelope};
use mediaservo_link::RetryConfig;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// mock server 侧（accept_async 原始 TCP 流）。
type WsServer = WebSocketStream<TcpStream>;
/// 本地子进程侧（connect_async 可能 TLS 流）。
type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// 任一 WS 流（client/server 共用 helper 的泛型约束）。
trait WsIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl WsIo for TcpStream {}
impl WsIo for MaybeTlsStream<TcpStream> {}

type Ws = WebSocketStream<TcpStream>;

const VEHICLE_ROOM: &str = "vehicle-1";
const VEHICLE_PEER: &str = "veh-peer";

fn cfg(remote: SocketAddr) -> GatewayConfig {
    GatewayConfig {
        local_port: 0, // 临时端口
        remote_url: format!("ws://{remote}/ws"),
        psk: "test-psk".into(),
        room: VEHICLE_ROOM.into(),
        retry: RetryConfig {
            max_retries: 3,
            base_delay: Duration::from_millis(50),
            max_delay: Duration::from_millis(500),
        },
    }
}

fn env(src: &str, msg: SignalingMessage) -> String {
    serde_json::to_string(&LocalEnvelope { src: src.into(), msg }).unwrap()
}

/// mock server 完整握手：PSK → Error{0} 确认 → RoomJoin → RoomJoined。
/// 返回 (ws, 请求的房间, 角色)。
async fn mock_handshake(listener: &TcpListener) -> (WsServer, String, PeerRole) {
    let (stream, _) = listener.accept().await.expect("mock accept");
    let mut ws = tokio_tungstenite::accept_async(stream).await.expect("mock ws handshake");
    let psk = ws.next().await.unwrap().unwrap();
    assert!(matches!(psk, Message::Text(_)), "首条应为 PSK 文本");
    ws.send(Message::Text(
        serde_json::to_string(&SignalingMessage::Error { code: 0, message: String::new() })
            .unwrap()
            .into(),
    ))
    .await
    .unwrap();
    let join = ws.next().await.unwrap().unwrap();
    let (room, role) = match serde_json::from_str::<SignalingMessage>(join.to_text().unwrap())
        .expect("mock 解析 RoomJoin")
    {
        SignalingMessage::RoomJoin { room_id, peer_role, .. } => (room_id, peer_role),
        other => panic!("expected RoomJoin, got {other:?}"),
    };
    ws.send(Message::Text(
        serde_json::to_string(&SignalingMessage::RoomJoined {
            room_id: room.clone(),
            peer_id: VEHICLE_PEER.into(),
        })
        .unwrap()
        .into(),
    ))
    .await
    .unwrap();
    (ws, room, role)
}

async fn local(port: u16) -> WsClient {
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
        .await
        .expect("local client connect");
    ws
}

async fn read_env<S: WsIo>(ws: &mut WebSocketStream<S>) -> (String, SignalingMessage) {
    let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("读超时")
        .expect("ws 关闭")
        .expect("ws 错误");
    let text = msg.to_text().expect("应为文本").to_string();
    let env: LocalEnvelope = serde_json::from_str(&text).expect("信封解析");
    (env.src, env.msg)
}

/// 子进程 RoomJoin（拦截语义）：网关未就绪时 Error 5001 → 重试。
/// 返回合成 RoomJoined 的 peer_id。
async fn join<S: WsIo>(ws: &mut WebSocketStream<S>, room: &str) -> String {
    for _ in 0..100 {
        ws.send(Message::Text(env(
            "child",
            SignalingMessage::RoomJoin {
                room_id: room.into(),
                peer_role: PeerRole::Host,
                stream_id: None,
            },
        )))
        .await
        .unwrap();
        let (_src, msg) = read_env(ws).await;
        match msg {
            SignalingMessage::RoomJoined { room_id, peer_id } => {
                assert_eq!(room_id, room, "合成 RoomJoined 应回显子进程房间");
                return peer_id;
            }
            SignalingMessage::Error { code: 5001, .. } => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            other => panic!("join 意外应答: {other:?}"),
        }
    }
    panic!("join 重试耗尽");
}

/// 读 2 条消息：transport 响应 + NewProducer 广播（任意顺序），返回 transport_id。
async fn collect_transport(ws: &mut WsClient, room: &str) -> String {
    let mut transport: Option<String> = None;
    for _ in 0..2 {
        let (_, m) = read_env(ws).await;
        match m {
            SignalingMessage::WebRtcTransportCreated { transport_id, room_id, .. } => {
                assert_eq!(room_id, room, "transport 响应房间应改写为 {room}");
                transport = Some(transport_id);
            }
            SignalingMessage::NewProducer { room_id, .. } => {
                assert_eq!(room_id, room, "NewProducer 广播房间应改写为 {room}");
            }
            other => panic!("{room} 意外消息: {other:?}"),
        }
    }
    transport.expect("应收到 transport 响应")
}

fn transport_created(transport_id: &str) -> SignalingMessage {
    SignalingMessage::WebRtcTransportCreated {
        room_id: VEHICLE_ROOM.into(),
        peer_id: VEHICLE_PEER.into(),
        transport_id: transport_id.into(),
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
    }
}

// ── ① 多本地客户端转发正确（双向房间重写 + src=server 下发）────────────────

#[tokio::test]
async fn multiple_clients_forward_with_room_rewrite() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut ws, room, role) = mock_handshake(&listener).await;
        assert_eq!(room, VEHICLE_ROOM, "agent 应以整车身份单次 join");
        assert_eq!(role, PeerRole::Host);

        // 两条上行（来自不同子进程）：房间均重写为整车房间
        let m1: SignalingMessage =
            serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert!(
            matches!(&m1, SignalingMessage::Frame { room_id, .. } if room_id == VEHICLE_ROOM),
            "Frame 房间应重写为整车房间, got {m1:?}"
        );
        let m2: SignalingMessage =
            serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert!(
            matches!(&m2, SignalingMessage::EncoderStatus { room_id, .. } if room_id == VEHICLE_ROOM),
            "EncoderStatus 房间应重写为整车房间, got {m2:?}"
        );

        // 下行房间级广播：每个子进程收到自己房间的改写
        ws.send(Message::Text(
            serde_json::to_string(&SignalingMessage::NewProducer {
                room_id: VEHICLE_ROOM.into(),
                producer_id: "p1".into(),
                peer_id: VEHICLE_PEER.into(),
                kind: MediaKind::Video,
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await; // 保持连接至断言完成
    });

    let port = run_gateway(cfg(addr)).await.unwrap();
    let mut a = local(port).await;
    let mut b = local(port).await;
    join(&mut a, "room-a").await;
    join(&mut b, "room-b").await;

    a.send(Message::Text(env(
        "a",
        SignalingMessage::Frame {
            room_id: "room-a".into(),
            codec: "vp8".into(),
            sequence: 1,
            is_keyframe: true,
            data_base64: "AA==".into(),
        },
    )))
    .await
    .unwrap();
    b.send(Message::Text(env(
        "b",
        SignalingMessage::EncoderStatus {
            room_id: "room-b".into(),
            peer_id: "host".into(),
            codec: "vp8".into(),
            encoder_backend: "software".into(),
            encoder_implementation: None,
            frames_per_second: 30.0,
            frame_width: 640,
            frame_height: 360,
            avg_encode_ms: None,
        },
    )))
    .await
    .unwrap();

    // 下行广播：各子进程收到自己的房间改写 + src=server
    let (src, m) = read_env(&mut a).await;
    assert_eq!(src, "server");
    assert!(
        matches!(&m, SignalingMessage::NewProducer { room_id, .. } if room_id == "room-a"),
        "子进程 a 应收到房间 room-a 的改写, got {m:?}"
    );
    let (src, m) = read_env(&mut b).await;
    assert_eq!(src, "server");
    assert!(
        matches!(&m, SignalingMessage::NewProducer { room_id, .. } if room_id == "room-b"),
        "子进程 b 应收到房间 room-b 的改写, got {m:?}"
    );
    server.await.unwrap();
}

// ── ② RoomJoin 拦截：不上行 + 本地合成 RoomJoined ──────────────────────────

#[tokio::test]
async fn room_join_intercepted_not_forwarded() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut ws, room, role) = mock_handshake(&listener).await;
        assert_eq!(room, VEHICLE_ROOM);
        assert_eq!(role, PeerRole::Host);
        // 子进程 RoomJoin 绝不上行：2s 内不应收到任何消息（更不是 RoomJoin）
        let leaked = tokio::time::timeout(Duration::from_secs(2), ws.next()).await;
        assert!(
            leaked.is_err(),
            "子进程 RoomJoin 泄漏到远端: {:?}",
            leaked.ok().flatten().map(|m| format!("{m:?}"))
        );
    });

    let port = run_gateway(cfg(addr)).await.unwrap();
    let mut a = local(port).await;
    let peer_id = join(&mut a, "child-room").await;
    assert_eq!(peer_id, VEHICLE_PEER, "合成 RoomJoined 应携带整车 peer_id");
    server.await.unwrap();
}

// ── ③ 并发协商：两路 Create 在途 → FIFO 响应路由不串 ───────────────────────

#[tokio::test]
async fn concurrent_sfu_negotiation_routes_responses() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut ws, _room, _role) = mock_handshake(&listener).await;

        // 两路 Create（顺序与响应一致），房间均为整车房间
        for _ in 0..2 {
            let c: SignalingMessage =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            assert!(
                matches!(&c, SignalingMessage::CreateWebRtcTransport { room_id, direction, .. }
                    if room_id == VEHICLE_ROOM && *direction == TransportDirection::Send),
                "Create 房间应重写为整车房间, got {c:?}"
            );
        }
        // 响应序列：t1 → NewProducer 广播（夹在响应之间）→ t2
        ws.send(Message::Text(serde_json::to_string(&transport_created("t1")).unwrap().into()))
            .await
            .unwrap();
        ws.send(Message::Text(
            serde_json::to_string(&SignalingMessage::NewProducer {
                room_id: VEHICLE_ROOM.into(),
                producer_id: "p9".into(),
                peer_id: VEHICLE_PEER.into(),
                kind: MediaKind::Video,
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
        ws.send(Message::Text(serde_json::to_string(&transport_created("t2")).unwrap().into()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    let port = run_gateway(cfg(addr)).await.unwrap();
    let mut a = local(port).await;
    let mut b = local(port).await;
    join(&mut a, "room-a").await;
    join(&mut b, "room-b").await;

    // 两路同时发送 Create（在途）
    for (ws, room) in [(&mut a, "room-a"), (&mut b, "room-b")] {
        ws.send(Message::Text(env(
            "child",
            SignalingMessage::CreateWebRtcTransport {
                room_id: room.into(),
                peer_id: "host".into(),
                direction: TransportDirection::Send,
            },
        )))
        .await
        .unwrap();
    }

    // 各读 2 条：自己的 transport 响应 + NewProducer 广播（顺序不定——server 的
    // 广播通道与直连响应并发，广播可能先于第二个响应到达）
    let ta = collect_transport(&mut a, "room-a").await;
    let tb = collect_transport(&mut b, "room-b").await;
    assert_ne!(ta, tb, "两路 transport_id 不得串线");
    assert!(ta == "t1" || ta == "t2");
    assert!(tb == "t1" || tb == "t2");
    server.await.unwrap();
}

// ── ④ 断线重连：转发恢复 + 在途请求清空 ────────────────────────────────────

#[tokio::test]
async fn reconnect_resumes_forwarding() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        // 连接 1：完整握手 → 收一条消息 → 关闭（模拟远端断线）
        let (mut ws1, _r, _role) = mock_handshake(&listener).await;
        let m1 = ws1.next().await.unwrap().unwrap();
        assert!(
            m1.to_text().unwrap().contains("\"type\":\"sdp\""),
            "连接 1 应收到子进程 Sdp"
        );
        drop(ws1);

        // 连接 2：握手 → 收 Create → 响应
        let (mut ws2, _r2, _role2) = mock_handshake(&listener).await;
        let c = ws2.next().await.unwrap().unwrap();
        let c: SignalingMessage = serde_json::from_str(c.to_text().unwrap()).unwrap();
        assert!(
            matches!(&c, SignalingMessage::CreateWebRtcTransport { room_id, .. }
                if room_id == VEHICLE_ROOM),
            "重连后 Create 房间应为整车房间, got {c:?}"
        );
        ws2.send(Message::Text(serde_json::to_string(&transport_created("t-after")).unwrap().into()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    let port = run_gateway(cfg(addr)).await.unwrap();
    let mut a = local(port).await;
    join(&mut a, "room-a").await;

    // 断线前：Sdp 转发（mock 连接 1 收后关闭）
    a.send(Message::Text(env(
        "a",
        SignalingMessage::Sdp {
            room_id: "room-a".into(),
            target: None,
            sdp: "v=0 offer".into(),
        },
    )))
    .await
    .unwrap();

    // 重连后：Create 在途（5001 或断线窗口无应答 → 重试）→ 响应路由回本连接
    let response = loop {
        a.send(Message::Text(env(
            "a",
            SignalingMessage::CreateWebRtcTransport {
                room_id: "room-a".into(),
                peer_id: "host".into(),
                direction: TransportDirection::Send,
            },
        )))
        .await
        .unwrap();
        match tokio::time::timeout(Duration::from_millis(300), read_env(&mut a)).await {
            Ok((_, SignalingMessage::Error { code: 5001, .. })) => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok((_, m)) => break m,
            Err(_) => {} // 断线窗口：请求无应答，重发
        }
    };
    assert!(
        matches!(&response, SignalingMessage::WebRtcTransportCreated { transport_id, room_id, .. }
            if transport_id == "t-after" && room_id == "room-a"),
        "重连后响应应路由到本连接且房间改写, got {response:?}"
    );
    server.await.unwrap();
}

// ── ⑤ P2P Sdp/ICE 单协商路由：回显去重 + 归属切换 ──────────────────────────

#[tokio::test]
async fn p2p_sdp_ice_single_negotiation_routing() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut ws, _r, _role) = mock_handshake(&listener).await;

        // 收 A 的 offer → 原样回显（模拟 server 房间广播）+ 远端应答
        let offer = ws.next().await.unwrap().unwrap();
        assert!(offer.to_text().unwrap().contains("\"type\":\"sdp\""));
        ws.send(offer).await.unwrap();
        ws.send(Message::Text(
            serde_json::to_string(&SignalingMessage::Sdp {
                room_id: VEHICLE_ROOM.into(),
                target: Some("remote-peer".into()),
                sdp: "b-answer".into(),
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

        // 收 B 的 ICE → 原样回显 + 远端候选
        let ice = ws.next().await.unwrap().unwrap();
        assert!(ice.to_text().unwrap().contains("\"type\":\"r_t_c_ice_candidate\""));
        ws.send(ice).await.unwrap();
        ws.send(Message::Text(
            serde_json::to_string(&SignalingMessage::RTCIceCandidate {
                room_id: VEHICLE_ROOM.into(),
                target: None,
                candidate: "peer-cand".into(),
                sdp_mid: Some("0".into()),
                sdp_mline_index: Some(0),
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    let port = run_gateway(cfg(addr)).await.unwrap();
    let mut a = local(port).await;
    let mut b = local(port).await;
    join(&mut a, "room-a").await;
    join(&mut b, "room-b").await;

    // A 发 offer（协商归属 = A）
    a.send(Message::Text(env(
        "a",
        SignalingMessage::Sdp {
            room_id: "room-a".into(),
            target: None,
            sdp: "a-offer".into(),
        },
    )))
    .await
    .unwrap();
    // A 收到的第一条必须是远端应答（自己的回显已被去重）
    let (_, m) = read_env(&mut a).await;
    assert!(
        matches!(&m, SignalingMessage::Sdp { sdp, room_id, .. }
            if sdp == "b-answer" && room_id == "room-a"),
        "A 应收到远端应答（非自身回显），房间改写, got {m:?}"
    );

    // B 发 ICE（归属切到 B）→ 收到远端候选（非自身回显）
    b.send(Message::Text(env(
        "b",
        SignalingMessage::RTCIceCandidate {
            room_id: "room-b".into(),
            target: None,
            candidate: "b-cand".into(),
            sdp_mid: Some("0".into()),
            sdp_mline_index: Some(0),
        },
    )))
    .await
    .unwrap();
    let (_, m) = read_env(&mut b).await;
    assert!(
        matches!(&m, SignalingMessage::RTCIceCandidate { candidate, room_id, .. }
            if candidate == "peer-cand" && room_id == "room-b"),
        "B 应收到远端候选（非自身回显），房间改写, got {m:?}"
    );
    server.await.unwrap();
}
