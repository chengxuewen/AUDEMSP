//! E2E integration tests for field::PushSession → mediasoup SFU pipeline.
//!
//! Runs on Linux only — connects to an external mediasoup server (C21).
//! 纯外部模式：仅通过 WS 信令协议交互，不 import server 内部类型。
//!
//! Tests:
//! - D1-field: PushSession connect → publish_video 全链路（transport→produce）
//! - D2-field: 信令事件桥（SignalEvent → SessionEvent::Message）

#![cfg(target_os = "linux")]

use std::time::Duration;

// 诊断: 测试内初始化 tracing（否则日志不可见）
fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
}

use mediaservo_common::protocol::PeerRole;
use mediaservo_field::{
    FieldError, PublishOptions, PullConfig, PullSession, PushConfig, PushSession, SessionEvent,
};
use mediaservo_webrtc::traits::PeerConnectionApi;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

fn ws_url() -> String {
    std::env::var("SFU_E2E_WS_URL").unwrap_or_else(|_| {
        panic!(
            "SFU_E2E_WS_URL 未设置 — field e2e 需连外部 mediasoup server (C21);
            例: SFU_E2E_WS_URL=ws://127.0.0.1:9800/ws"
        )
    })
}

fn psk() -> String {
    std::env::var("SFU_E2E_PSK").unwrap_or_else(|_| "mediaservo-dev".to_string())
}

fn test_config() -> PushConfig {
    // 每次调用独立 room（测试并行 + 每次运行唯一，防 server 端残留 producer 冲突）
    let room = format!(
        "field-push-test-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut cfg = PushConfig::new(ws_url(), psk(), room);
    cfg.role = PeerRole::Host;
    cfg
}

/// D1-field: PushSession 全链路 — connect → publish_video 成功。
///
/// 流程（对齐 host e2e_sfu D1/D2）：
/// 1. PushSession::connect(cfg) — 信令连接 + 加入房间
/// 2. publish_video — transport 创建 → answer 协商 → Connect → Produce
/// 3. 断言返回 track id + TrackPublished 事件
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn field_push_session_publish_video() {
    let cfg = test_config();
    let (mut session, mut events) = PushSession::connect(cfg.clone())
        .await
        .expect("PushSession connect failed");

    let opts = PublishOptions::default(); // VP8 / auto backend
    let track = tokio::time::timeout(CONNECT_TIMEOUT, session.publish_video(&cfg, &opts))
        .await
        .expect("publish_video timeout")
        .expect("publish_video failed");
    assert!(!track.is_empty(), "track id empty");

    // 事件流应出现 TrackPublished
    tokio::time::timeout(CONNECT_TIMEOUT, async {
        while let Some(ev) = events.recv().await {
            match ev {
                SessionEvent::TrackPublished {
                    track: published,
                } => {
                    assert_eq!(published, track, "published track id mismatch");
                    return;
                }
                SessionEvent::Error(e) => panic!("session error during publish: {e:?}"),
                _ => continue, // Message/Connected/Disconnected 忽略
            }
        }
        panic!("event stream closed before TrackPublished");
    })
    .await
    .expect("TrackPublished event timeout");

    session.close().await.expect("close failed");
}

/// D2-field: 连接失败路径 — 无 server 时 connect 应回 LinkError 而非挂起。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn field_push_session_connect_failure() {
    let mut cfg = test_config();
    cfg.url = "ws://127.0.0.1:1/ws".to_string(); // 必然不可达端口
    let err = PushSession::connect(cfg).await;
    match err {
        Err(FieldError::Link(_)) => {}
        other => panic!("expected LinkError, got {other:?}"),
    }
}

/// D3-field: peer_connection escape hatch — publish 后应暴露底层 PC。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn field_push_session_peer_connection_available() {
    let cfg = test_config();
    let (mut session, _events) = PushSession::connect(cfg.clone())
        .await
        .expect("connect failed");

    // publish 前无 PC
    assert!(session.peer_connection().is_none(), "PC should be None before publish");

    let opts = PublishOptions::default();
    tokio::time::timeout(CONNECT_TIMEOUT, session.publish_video(&cfg, &opts))
        .await
        .expect("publish timeout")
        .expect("publish failed");

    // publish 后 PC 可用（escape hatch 可访问底层状态）
    let pc = session.peer_connection().expect("PC available after publish");
    assert_eq!(pc.track_count(), 1, "one video track");

    session.close().await.expect("close failed");
}
/// D4-field: 帧发布 — start_video_frames 后 sender 应产生编码帧（bytes_sent/frames_encoded 增长）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn field_push_session_video_frames_flow() {
    let cfg = test_config();
    let (mut session, _events) = PushSession::connect(cfg.clone())
        .await
        .expect("connect failed");

    let opts = PublishOptions::default();
    tokio::time::timeout(CONNECT_TIMEOUT, session.publish_video(&cfg, &opts))
        .await
        .expect("publish timeout")
        .expect("publish failed");

    // 帧发布前: 无生成器
    assert!(session.peer_connection().is_some(), "PC after publish");

    // 启动帧生成
    session
        .start_video_frames(&cfg)
        .expect("start_video_frames failed");
    // 重复启动应报 InvalidState
    let dup = session.start_video_frames(&cfg).unwrap_err();
    assert!(matches!(dup, FieldError::InvalidState(_)), "got {dup:?}");

    // 轮询 sender stats: 帧编码应启动（等待 ≤10s 出帧）
    let pc = session.peer_connection().expect("pc");
    let mut observed_frames = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let stats = pc.sender_get_stats("video");
        if let Some(o) = stats.iter().find_map(|s| match s {
            mediaservo_webrtc::stats::RTCStats::OutboundRtp(o) => Some(o),
            _ => None,
        }) {
            if o.bytes_sent > 0 && o.frames_encoded > 0 {
                tracing::info!(
                    "frames flowing: bytes_sent={} frames_encoded={}",
                    o.bytes_sent,
                    o.frames_encoded
                );
                observed_frames = true;
                break;
            }
        }
    }
    assert!(observed_frames, "no outbound frames observed within 10s");

    // 停止帧生成（幂等）
    session.stop_video_frames();
    session.stop_video_frames(); // 二次调用无副作用

    session.close().await.expect("close failed");
}

/// D5-field: PullSession 消费 — Push 推帧 → Pull 订阅 → 解码帧流出。
///
/// 同一房间内: PushSession publish + start_video_frames → PullSession subscribe
/// （producer_id 来自 NewProducer 广播）→ on_track → FrameSink → PullFrame 接收。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn field_pull_session_consumes_video() {
    init_tracing();
    use mediaservo_common::protocol::SignalingMessage;

    let room = format!(
        "field-pull-test-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let push_cfg = PushConfig::new(ws_url(), psk(), room.clone());
    let pull_cfg = PullConfig {
        url: ws_url(),
        psk: psk(),
        room,
        role: PeerRole::Consumer,
        auto_subscribe: true,
    };

    // 1. Pull 先入房（Consumer 角色）— 保证 push publish 的 NewProducer 广播必达
    let (mut pull, mut pull_events) = PullSession::connect(pull_cfg.clone())
        .await
        .expect("pull connect");

    // 2. Push 侧: 连接 + publish + 帧生成（广播给房间内已有 peer = Pull）
    let (mut push, mut push_events) = PushSession::connect(push_cfg.clone())
        .await
        .expect("push connect");
    let opts = PublishOptions::default();
    let track = tokio::time::timeout(CONNECT_TIMEOUT, push.publish_video(&push_cfg, &opts))
        .await
        .expect("push publish timeout")
        .expect("push publish failed");
    push.start_video_frames(&push_cfg).expect("start frames");

    // 3. 等待 NewProducer 广播（Push 的 producer）
    let producer_id = tokio::time::timeout(CONNECT_TIMEOUT, async {
        loop {
            match pull_events.recv().await {
                Some(SessionEvent::Message(SignalingMessage::NewProducer { producer_id, .. })) => {
                    return producer_id;
                }
                Some(SessionEvent::Error(e)) => panic!("pull session error: {e:?}"),
                _ => continue,
            }
        }
    })
    .await
    .expect("NewProducer timeout");

    // 4. 订阅 + 收帧
    let mut frames = tokio::time::timeout(CONNECT_TIMEOUT, pull.subscribe(&pull_cfg, &producer_id))
        .await
        .expect("subscribe timeout")
        .expect("subscribe failed");

    // 5. 等待解码帧流出（≤15s; WebRtcTrackSink 推帧 → SFU relay → 解码 → FrameSink）
    let mut got_frame = false;
    for _ in 0..30 {
        tokio::time::timeout(Duration::from_secs(1), frames.recv())
            .await
            .map(|opt| {
                if let Some(f) = opt {
                    assert!(f.width > 0 && f.height > 0, "frame dims");
                    assert!(!f.data.is_empty(), "frame data");
                    tracing::info!("pull frame: {}x{} ({} bytes)", f.width, f.height, f.data.len());
                    got_frame = true;
                }
            });
        if got_frame {
            break;
        }
    }
    assert!(got_frame, "no decoded frame within 15s");

    // 清理
    push.stop_video_frames();
    push.close().await.expect("push close");
    pull.close().await.expect("pull close");
    let _ = track;
    let _ = push_events;
}
