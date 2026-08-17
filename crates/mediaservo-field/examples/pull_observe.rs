//! 观测: PullSession subscribe 后监控 inbound RTP（bytes_received 增长验证）
#![cfg(target_os = "linux")]
use std::time::Duration;

use mediaservo_common::protocol::{PeerRole, SignalingMessage};
use mediaservo_field::{PullConfig, PullSession, PushConfig, PushSession, PublishOptions, SessionEvent};
use mediaservo_webrtc::stats::RTCStats;
use mediaservo_webrtc::traits::PeerConnectionApi;

#[tokio::main]
async fn main() {
    let url = std::env::var("SFU_E2E_WS_URL").expect("SFU_E2E_WS_URL");
    let psk = std::env::var("SFU_E2E_PSK").expect("SFU_E2E_PSK");
    let room = format!("pull-obs-{}", std::process::id());

    // Pull 先入房
    let pull_cfg = PullConfig {
        url: url.clone(), psk: psk.clone(), room: room.clone(),
        role: PeerRole::Consumer, auto_subscribe: true,
    };
    let (mut pull, mut pull_events) = PullSession::connect(pull_cfg.clone()).await.expect("pull connect");

    // Push publish + 帧
    let push_cfg = PushConfig::new(url, psk, room);
    let (mut push, _pe) = PushSession::connect(push_cfg.clone()).await.expect("push connect");
    let _ = push.publish_video(&push_cfg, &PublishOptions::default()).await.expect("publish");
    push.start_video_frames(&push_cfg).expect("frames");

    // 等 NewProducer
    let producer_id = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match pull_events.recv().await {
                Some(SessionEvent::Message(SignalingMessage::NewProducer { producer_id, .. })) => return producer_id,
                _ => continue,
            }
        }
    }).await.expect("NewProducer timeout");

    // subscribe
    let mut _frames = tokio::time::timeout(Duration::from_secs(30), pull.subscribe(&pull_cfg, &producer_id))
        .await.expect("subscribe timeout").expect("subscribe failed");
    println!("subscribed to {producer_id}");

    // 监控 inbound 10s — 用 pc 内部统计（get_receivers 可能不含 on_track 新建的 receiver）
    let pc = pull.peer_connection().expect("pc");
    for i in 0..10 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        // 遍历 pc 已知 track_id（on_track 回调里的 receiver 未入 tracks）
        let mut saw = false;
        for tid in pc.track_ids() {
            let stats = pc.sender_get_stats(&tid);
            for st in &stats {
                if let RTCStats::InboundRtp(o) = st {
                    println!("t={i}s inbound(track={tid}): bytes_received={} packets_received={}",
                        o.bytes_received, o.packets_received);
                    saw = true;
                }
            }
        }
        if !saw {
            // 尝试 backend 原生 get_stats 全量
            let stats = pc.sender_get_stats("video");
            let kinds: Vec<String> = stats.iter().map(|s| format!("{s:?}").chars().take(40).collect()).collect();
            println!("t={i}s no-inbound, stats={kinds:?}");
        }
    }
    push.stop_video_frames();
    push.close().await.unwrap();
    pull.close().await.unwrap();
}
