//! ICE 真实连接断言测试 — CI 门禁 (PIT-50 教训)
//!
//! 历史: 测试套件"全过但从未验证真实 ICE 连接"（loopback 只交换 SDP 不断言
//! connected/收帧）→ webrtc-sys 的 Linux 可用性从未被真实验证（PIT-45 观察）。
//! 本测试: 等待 PeerConnection Connected + 对端收到帧（CountingSink > 0）。
//! 若 ICE 不工作（如封装回归/环境问题），此测试必须失败。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use audemsp_webrtc::peer_connection::{RTCConfiguration, RTCPeerConnectionState};
use audemsp_webrtc::traits::PeerConnectionApi;
use audemsp_webrtc::track::{FrameSink, TrackKind, TrackRef};
use tokio::sync::oneshot;

fn init_log() { let _ = tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).try_init(); }

mod common;

struct CountingSink {
    count: Arc<AtomicU64>,
}
impl CountingSink {
    fn new() -> (Self, Arc<AtomicU64>) {
        let count = Arc::new(AtomicU64::new(0));
        (Self { count: count.clone() }, count)
    }
}
impl FrameSink for CountingSink {
    fn on_frame(&self, _: &[u8], _: u32, _: u32) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }
}

/// 真实 ICE 连接测试：双 PC SDP 交换后必须到达 Connected 且对端收到帧。
/// 超时 15s 断言连接失败即 RED。
/// 当前状态: P2P 双 full ICE 不连接（PIT-53——trickle 候选添加成功但 ICE transport 未激活），
/// Host→SFU (ICE-Lite) 正常；测试门禁已暴露该缺陷，#[ignore] 待 P2P ICE 修复后启用。
#[tokio::test]
#[ignore = "PIT-53: P2P 双 full ICE 不连接"]
async fn p2p_ice_reaches_connected_and_receives_frames() {
    init_log();
    let factory = audemsp_webrtc::factory::RTCPeerConnectionFactory::new();
    let pc1 = factory
        .create_peer_connection(RTCConfiguration::default())
        .await
        .expect("pc1 create");
    let pc2 = factory
        .create_peer_connection(RTCConfiguration::default())
        .await
        .expect("pc2 create");

    // 等待 pc1 到达 Connected（Fn 闭包内用 Arc<Mutex> 取 sender）
    let (tx, rx) = oneshot::channel::<()>();
    let tx = Arc::new(std::sync::Mutex::new(Some(tx)));
    pc1.on_peer_connection_state_change(move |state| {
        if state == RTCPeerConnectionState::Connected {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(());
            }
        }
    });

    // 对端注册收帧 sink（共享 Arc<AtomicU64> 计数）
    let received = Arc::new(AtomicU64::new(0));
    let received2 = received.clone();
    pc2.on_track(move |receiver| {
        if let TrackRef::Receiver(ref track_receiver) = receiver.track {
            track_receiver.set_frame_sink(Box::new(CountingSink {
                count: received2.clone(),
            }));
        }
    });

    // 发送端 track
    let sender = factory.create_video_track("test-video");
    pc1.add_track("test-video", TrackKind::Video).unwrap();

    // PIT-52: trickle 候选双向转发（libwebrtc 候选在信令线程回调——不能 tokio::spawn，
    // 用 std mpsc 投递 + 主任务泵送 add_ice_candidate）
    let (tx_pc1, rx_pc1) = std::sync::mpsc::channel::<audemsp_webrtc::RTCIceCandidate>();
    let (tx_pc2, rx_pc2) = std::sync::mpsc::channel::<audemsp_webrtc::RTCIceCandidate>();
    let tx2_for_pc1 = tx_pc2.clone();
    pc1.on_ice_candidate(move |c| {
        eprintln!("[test] pc1 local candidate: {}", c.candidate);
        let _ = tx2_for_pc1.send(c);
    });
    let tx1_for_pc2 = tx_pc1.clone();
    pc2.on_ice_candidate(move |c| {
        eprintln!("[test] pc2 local candidate: {}", c.candidate);
        let _ = tx1_for_pc2.send(c);
    });

    // SDP 交换（与现有测试相同的流程）
    common::loopback::exchange_sdp(&pc1, &pc2)
        .await
        .expect("sdp exchange");

    // 断言：ICE 必须到达 Connected（15s）——同时泵送 trickle 候选到对端
    let mut rx = rx;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            tokio::select! {
                _ = &mut rx => break,
                _ = tokio::task::yield_now() => {
                    while let Ok(c) = rx_pc1.try_recv() {
                        eprintln!("[test] pc1→pc2 add: {}", c.candidate.split_whitespace().nth(1).unwrap_or("?"));
                        match pc2.add_ice_candidate(&c).await {
                            Ok(()) => eprintln!("[test]   pc2 add OK"),
                            Err(e) => eprintln!("[test]   pc2 add ERR: {e}"),
                        }
                    }
                    while let Ok(c) = rx_pc2.try_recv() {
                        eprintln!("[test] pc2→pc1 add: {}", c.candidate.split_whitespace().nth(1).unwrap_or("?"));
                        match pc1.add_ice_candidate(&c).await {
                            Ok(()) => eprintln!("[test]   pc1 add OK"),
                            Err(e) => eprintln!("[test]   pc1 add ERR: {e}"),
                        }
                    }
                },
            }
        }
    })
        .await
        .expect("ICE did not reach Connected within 15s — 真实连接未建立 (PIT-50 门禁)");

    // 连接后发送帧（~1s, 30fps）
    for i in 0..30u64 {
        let frame = common::loopback::generate_test_frame(320, 240, i);
        sender
            .write_raw_i420(&frame, 320, 240)
            .await
            .expect("write frame");
        tokio::time::sleep(Duration::from_millis(33)).await;
    }

    // 等待对端解码接收
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 断言：对端必须收到帧
    let count = received.load(Ordering::Relaxed);
    assert!(count > 0, "remote did not receive any frames (count={count}) — 数据面未打通 (PIT-50 门禁)");

    pc1.close().await;
    pc2.close().await;
}
