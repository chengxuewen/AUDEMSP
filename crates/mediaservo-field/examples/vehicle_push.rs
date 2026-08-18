//! 车端推流示例 — PushSession 完整流程（连接→发布→帧生成→停止）。
//!
//! 前置: mediaservo server 运行中（`./mediaservo.sh start server`）。
//!
//! ```bash
//! SFU_E2E_WS_URL=ws://127.0.0.1:9800/ws SFU_E2E_PSK=mediaservo-dev \
//!   pixi run cargo run -p mediaservo-field --example vehicle_push
//! ```

use std::time::Duration;

use mediaservo_field::{PublishOptions, PushConfig, PushSession, SessionEvent};

#[tokio::main]
async fn main() {
    // 1. 配置（车端: 相机分辨率/帧率/码率）
    let url = std::env::var("SFU_E2E_WS_URL").unwrap_or_else(|_| "ws://127.0.0.1:9800/ws".into());
    let psk = std::env::var("SFU_E2E_PSK").unwrap_or_else(|_| "mediaservo-dev".into());
    let room = format!("vehicle-{}", std::process::id());

    let mut cfg = PushConfig::new(url, psk, room);
    cfg.width = 1280;
    cfg.height = 720;
    cfg.framerate = 30;
    cfg.bitrate_kbps = 2000;
    cfg.keyframe_interval = 2;

    // 2. 连接信令 + 加入房间
    let (mut session, mut events) = PushSession::connect(cfg.clone())
        .await
        .expect("connect failed");
    println!("connected to room {}", cfg.room);

    // 3. 发布视频轨（SFU 协商: transport→answer→Connect→Produce）
    let opts = PublishOptions::default(); // VP8 / auto backend
    let track = session
        .publish_video(&cfg, &opts)
        .await
        .expect("publish failed");
    println!("video published: track={track}");

    // 4. 启动帧生成（Squares 彩条 + 时间戳水印, C17 时间戳语义）
    session.start_video_frames(&cfg).expect("start frames failed");
    println!("frames running ({}x{}@{}fps)", cfg.width, cfg.height, cfg.framerate);

    // 5. 事件监控 + 运行 30s
    let run = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(ev) = events.recv().await {
            match ev {
                SessionEvent::TrackPublished { track } => {
                    println!("event: TrackPublished({track})");
                }
                SessionEvent::Disconnected { reason } => {
                    eprintln!("event: Disconnected({reason})");
                    break;
                }
                SessionEvent::Error(e) => {
                    eprintln!("event: Error({e:?})");
                    break;
                }
                _ => {}
            }
        }
    });
    let _ = run.await;

    // 6. 停止 + 关闭
    session.stop_video_frames();
    session.close().await.expect("close failed");
    println!("done");
}
