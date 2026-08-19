//! host-streamer: 推流进程（Task C2）— FrameBus 订阅 → WebRTC 推流。
//!
//! 用法: `host-streamer --stream <id> --config <host.toml 路径> --token <令牌文件路径>`
//!
//! 流程: 读 host.toml `[[streams]]`（camera/codec 缺省 id/vp8，
//! [`mediaservo_host::translate::stream_config`]）→ 相机配置（fps）→ FrameBus
//! 订阅 `camera/<camera-id>`（FrameMeta + 紧凑 I420，C1 capturer 线格式）→
//! field `PushSession`（connect → publish_video：SFU transport + answer 协商 +
//! Connect + Produce，复用 field 推流链路）→ `TrackSender::write_raw_i420_with_ts`
//! 写帧（时间戳来自 FrameMeta.ts_mono_ns，C17 透传）。P2P 模式：信令直连 Server
//! （Phase D 网关前，MUST NOT 引入总线信令）。
//!
//! 信令目标 = 本地网关 host-agent（D2）：`--gateway <url>`（缺省
//! `ws://127.0.0.1:17980/ws`）。网关本地侧无 PSK 挑战（信任边界
//! 127.0.0.1）；整车 PSK 在 host-agent 的远端连接。房间 = `stream-<id>`
//! （网关拦袪 RoomJoin 并重写为整车房间，多流集合一车会话）。

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use mediaservo_field::{PublishOptions, PushConfig, PushSession, SessionEvent};
use mediaservo_host::monitor::flow::StreamerStats;
use mediaservo_link::{FrameBus, FrameMeta, FrameTopic, TokenFile};
use mediaservo_webrtc::stats::RTCStats;
use mediaservo_webrtc::traits::PeerConnectionApi;

/// FrameMeta 像素格式: 1 = I420（D243 枚举，与 C1 capturer 一致）。
const FORMAT_I420: u8 = 1;
/// 无帧看门狗：capturer 未启动/已退出时 10s 无帧即退出（C15，失败可见非挂起；
/// 部署侧 restart_policy=always 拉起，对齐 PIT-87 ICE-failed 自愈模式）。
const NO_FRAME_TIMEOUT: Duration = Duration::from_secs(10);
/// 出站统计日志间隔（e2e 证据 + 可观测性）。
const STATS_INTERVAL: Duration = Duration::from_secs(2);

const USAGE: &str = "用法: host-streamer --stream <id> --config <host.toml> --token <令牌文件>";

/// 出站统计消息序号（stats topic FrameMeta.seq，单调）。
static STATS_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
struct Args {
    stream: String,
    config: PathBuf,
    token: PathBuf,
    /// 本地网关 WS 地址（D2）；缺省 `ws://127.0.0.1:17980/ws`。
    gateway: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mut stream: Option<String> = None;
    let mut config: Option<PathBuf> = None;
    let mut token: Option<PathBuf> = None;
    let mut gateway: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stream" => stream = Some(args.next().ok_or("--stream 缺值")?),
            "--config" => config = Some(PathBuf::from(args.next().ok_or("--config 缺值")?)),
            "--token" => token = Some(PathBuf::from(args.next().ok_or("--token 缺值")?)),
            "--gateway" => gateway = Some(args.next().ok_or("--gateway 缺值")?),
            _ => return Err(format!("未知参数: {arg}")),
        }
    }
    Ok(Args {
        stream: stream.ok_or("缺少 --stream")?,
        config: config.ok_or("缺少 --config")?,
        token: token.ok_or("缺少 --token")?,
        gateway,
    })
}

/// 网关 WS 地址（D2）：`--gateway` 参数 > 缺省本地网关。
fn gateway_url(gateway_arg: Option<&str>) -> String {
    gateway_arg
        .map(str::to_string)
        .unwrap_or_else(|| "ws://127.0.0.1:17980/ws".to_string())
}

#[cfg(test)]
mod tests {
    use super::gateway_url;

    #[test]
    fn gateway_url_defaults_to_local_gateway() {
        assert_eq!(gateway_url(None), "ws://127.0.0.1:17980/ws");
    }

    #[test]
    fn gateway_url_override_wins() {
        assert_eq!(
            gateway_url(Some("ws://127.0.0.1:18888/ws")),
            "ws://127.0.0.1:18888/ws"
        );
    }
}

/// 紧凑 I420 payload 校验（线格式假设: tight strides Y + U + V）。
fn valid_i420(meta: &FrameMeta, payload_len: usize) -> bool {
    meta.format == FORMAT_I420
        && meta.width.is_multiple_of(2)
        && meta.height.is_multiple_of(2)
        && payload_len == (meta.width * meta.height * 3 / 2) as usize
}

/// 等待 SIGINT/SIGTERM（unix 主路径；其他平台仅 ctrl_c）。
async fn shutdown_signal() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    Ok(())
}

/// 出站统计：日志（e2e 证据: bytes_sent/frames_encoded > 0，对齐 field D4 模式）
/// + FrameBus 发布 [`StreamerStats`] JSON 到 `stats/stream-<id>`（E2 数据面监控，
/// additive；监控订阅者才消费，无消费者时发布零开销级）。
fn log_stats(session: &PushSession, bus: &FrameBus, topic: &FrameTopic, started: Instant) {
    let Some(pc) = session.peer_connection() else {
        return;
    };
    let stats = pc.sender_get_stats("video");
    let Some(o) = stats.iter().find_map(|s| match s {
        RTCStats::OutboundRtp(o) => Some(o),
        _ => None,
    }) else {
        return;
    };
    tracing::info!(
        "streamer stats: bytes_sent={} frames_encoded={} frame={}x{}",
        o.bytes_sent,
        o.frames_encoded,
        o.frame_width,
        o.frame_height
    );
    // E2 additive: 发布推流状态（FrameMeta::FORMAT_JSON 标记；ts_mono = 进程启动
    // 单调时钟（C17 锚定语义；监控侧不消费 stats ts_mono，仅作为单调标记））
    let meta = FrameMeta {
        seq: STATS_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        format: FrameMeta::FORMAT_JSON,
        ts_mono_ns: started.elapsed().as_nanos() as u64,
        ..Default::default()
    };
    let payload = match serde_json::to_vec(&StreamerStats {
        bytes_sent: o.bytes_sent,
        frames_encoded: o.frames_encoded,
        frame_width: o.frame_width,
        frame_height: o.frame_height,
    }) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("stats 序列化失败: {e}");
            return;
        }
    };
    if let Err(e) = bus.publish(topic, &payload, &meta) {
        tracing::warn!(topic = %topic.as_str(), "stats 发布失败: {e}");
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    mediaservo_host::init_logging("streamer");
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    // 流配置（camera/codec 缺省 id/vp8）+ 相机配置（fps）
    let cfg_text = match std::fs::read_to_string(&args.config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("streamer: 读取配置 {} 失败: {e}", args.config.display());
            return ExitCode::from(1);
        }
    };
    let stream = match mediaservo_host::translate::stream_config(&cfg_text, &args.stream) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("streamer: 配置中无流 {}", args.stream);
            return ExitCode::from(1);
        }
        Err(e) => {
            eprintln!("streamer: {e}");
            return ExitCode::from(1);
        }
    };
    let cam = match mediaservo_host::translate::camera_config(&cfg_text, &stream.camera) {
        Ok(Some(c)) => c,
        Ok(None) => {
            eprintln!("streamer: 流 {} 引用的相机 {} 不存在", stream.id, stream.camera);
            return ExitCode::from(1);
        }
        Err(e) => {
            eprintln!("streamer: {e}");
            return ExitCode::from(1);
        }
    };

    // C17 fps 对齐守卫（I1 审查）: 推流路径的编码帧率由 libwebrtc 内置 30fps
    // 决定 — cfg.framerate 仅语义传递，无下游消费（field publish_video 不读它），
    // 非 30 会在编码器 rate control 产生 PIT-64 类失配。任意 fps 的接线点在
    // webrtc_sys.rs:133 的 max_framerate codec prefs 面（TODO: 接线后移除守卫）。
    if cam.fps != 30 {
        eprintln!(
            "streamer: 相机 {} fps={} 不支持 — 推流编码器内置 30fps \
             （TODO: webrtc_sys.rs:133 max_framerate 接线后放开）",
            cam.id, cam.fps
        );
        return ExitCode::from(1);
    }

    // 令牌 → FrameBus attach → 订阅 camera/<camera-id>
    let token_bytes = match std::fs::read(&args.token) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("streamer: 读取令牌 {} 失败: {e}", args.token.display());
            return ExitCode::from(1);
        }
    };
    let (verifying_key, token) = match TokenFile::decode(&token_bytes) {
        Ok(kv) => kv,
        Err(e) => {
            eprintln!("streamer: 令牌 {} 无效: {e}", args.token.display());
            return ExitCode::from(1);
        }
    };
    let bus = match FrameBus::attach("", &token, &verifying_key) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("streamer: FrameBus attach 失败: {e}");
            return ExitCode::from(1);
        }
    };
    let topic = FrameTopic::new(format!("camera/{}", cam.id));
    let frames = match bus.subscribe(&topic) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("streamer: 订阅 {} 失败: {e}", topic.as_str());
            return ExitCode::from(1);
        }
    };
    tracing::info!(topic = %topic.as_str(), "FrameBus subscribed");

    // 推流会话（field PushSession 复用；D2: 经本地网关，无 PSK）
    let mut cfg = PushConfig::via_gateway(
        gateway_url(args.gateway.as_deref()),
        format!("host-streamer-{}", stream.id),
        format!("stream-{}", stream.id),
    );
    // framerate 仅语义传递（供未来编码器配置消费）— field publish_video 与
    // libwebrtc 编码器均不读此字段（编码帧率 = 内置 30fps）；对齐由上方 fps 守卫保证（I1）
    cfg.framerate = cam.fps;
    let (mut session, mut events) = match PushSession::connect(cfg.clone()).await {
        Ok(se) => se,
        Err(e) => {
            eprintln!("streamer: PushSession connect 失败: {e}");
            return ExitCode::from(1);
        }
    };

    // 首帧决定分辨率（capturer 固定 1280x720，按 meta 自适应更稳）→ publish
    let first = match tokio::time::timeout(NO_FRAME_TIMEOUT, frames.recv()).await {
        Ok(Some(f)) => f,
        Ok(None) => {
            eprintln!("streamer: 帧流关闭（capturer 未运行?）");
            return ExitCode::from(1);
        }
        Err(_) => {
            eprintln!("streamer: {NO_FRAME_TIMEOUT:?} 无帧 — capturer 未启动或已退出");
            return ExitCode::from(1);
        }
    };
    if !valid_i420(first.meta(), first.payload().len()) {
        eprintln!(
            "streamer: 首帧无效（format={} {}x{} payload={}）",
            first.meta().format,
            first.meta().width,
            first.meta().height,
            first.payload().len()
        );
        return ExitCode::from(1);
    }
    cfg.width = first.meta().width;
    cfg.height = first.meta().height;
    let opts = PublishOptions {
        codec: stream.codec.clone(),
        encoder_backend: "auto".into(),
    };
    if let Err(e) = session.publish_video(&cfg, &opts).await {
        eprintln!("streamer: publish_video 失败: {e}");
        return ExitCode::from(1);
    }
    let sender = match session.video_sender() {
        Some(s) => s,
        None => {
            eprintln!("streamer: publish 后无 video sender");
            return ExitCode::from(1);
        }
    };
    println!(
        "streamer ready: stream={} topic={} {}x{}@{} codec={} room={}",
        stream.id, topic.as_str(), cfg.width, cfg.height, cam.fps, stream.codec, cfg.room
    );

    // E2 additive: 推流状态 topic + 单调时钟起点（stats 发布用）
    let stats_topic = FrameTopic::new(format!("stats/stream-{}", stream.id));
    let started = Instant::now();
    let mut exit_code: u8 = 0;
    let mut last_stats = Instant::now();
    'run: loop {
        tokio::select! {
            sig = shutdown_signal() => match sig {
                Ok(()) => break 'run,
                Err(e) => {
                    eprintln!("streamer: 信号处理失败: {e}");
                    exit_code = 1;
                    break 'run;
                }
            },
            ev = events.recv() => match ev {
                Some(SessionEvent::Error(e)) => {
                    tracing::warn!("session error: {e}");
                }
                Some(SessionEvent::Disconnected { reason }) => {
                    tracing::error!("signal disconnected: {reason}");
                    exit_code = 1;
                    break 'run;
                }
                None => {
                    tracing::error!("session event stream closed");
                    exit_code = 1;
                    break 'run;
                }
                _ => {} // Message/Connected/TrackPublished 忽略
            },
            frame = tokio::time::timeout(NO_FRAME_TIMEOUT, frames.recv()) => match frame {
                Ok(Some(f)) => {
                    let meta = f.meta();
                    if !valid_i420(meta, f.payload().len()) {
                        tracing::warn!(
                            seq = meta.seq,
                            "invalid frame (format={} {}x{} payload={})",
                            meta.format, meta.width, meta.height, f.payload().len()
                        );
                        continue;
                    }
                    // C17: 时间戳透传 — capturer 已锚定单调时钟（ts_mono_ns）
                    let ts_us = (meta.ts_mono_ns / 1000) as i64;
                    if let Err(e) = sender
                        .write_raw_i420_with_ts(
                            f.payload(),
                            meta.width,
                            meta.height,
                            Some(ts_us),
                        )
                        .await
                    {
                        tracing::warn!(seq = meta.seq, "write frame: {e}");
                    }
                    if last_stats.elapsed() >= STATS_INTERVAL {
                        log_stats(&session, &bus, &stats_topic, started);
                        last_stats = Instant::now();
                    }
                }
                Ok(None) => {
                    tracing::error!("帧流关闭（capturer 退出?）");
                    exit_code = 1;
                    break 'run;
                }
                Err(_) => {
                    tracing::error!("{NO_FRAME_TIMEOUT:?} 无帧 — capturer 停止，退出待重启");
                    exit_code = 1;
                    break 'run;
                }
            },
        }
    }

    if let Err(e) = session.close().await {
        tracing::warn!("close: {e}");
    }
    tracing::info!("streamer stopped (exit={exit_code})");
    ExitCode::from(exit_code)
}
