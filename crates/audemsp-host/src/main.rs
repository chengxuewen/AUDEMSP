//! AUDEMSP Host — headless capture + encode + WebRTC push.
//!
//! # Startup flow
//! 1. Parse config (host.conf YAML)
//! 2. Load or create session state
//! 3. Start GStreamer pipeline
//! 4. Initialize WebRTC transport
//! 5. Create control handler
//! 6. Build axum router (metrics + signaling)
//! 7. Start emergency UDP listener
//! 8. Serve until shutdown signal


use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower_http::timeout::TimeoutLayer;

use futures_util::{SinkExt, StreamExt};
use audemsp_media::engine::PipelineEngine;
use audemsp_common::protocol::{DtlsParameters, Fingerprint, IceCandidate, IceParameters, MediaKind, SignalingMessage, TransportDirection};
use tokio_tungstenite::tungstenite::Message;
use signaling::SignalingClient;
use audemsp_webrtc::{RTCPeerConnectionFactory, RTCConfiguration, RTCIceServer, RTCIceTransportPolicy, RTCSessionDescription, RTCSdpType, RTCAnswerOptions, RTCOfferOptions, TrackKind, TrackSender, TrackRef, RTCPeerConnectionState};
use audemsp_webrtc::rtp::{RTCRtpTransceiverInit, RTCRtpTransceiverDirection, RTCRtpParameters};
use audemsp_webrtc::traits::PeerConnectionApi;
mod config;
mod control;
mod sfu_media;
mod emergency;
mod metrics;
mod pipeline;
mod engine_adapters;
mod session;
mod signaling;
mod transport;
#[cfg(feature = "webrtc-p2p")]
mod webrtc_transport;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
        )
        .init();

    tracing::info!("AUDEMSP Host v{} starting", env!("CARGO_PKG_VERSION"));

    // Parse config — collect args once for bounds-safe access
    let config_path = {
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 2 && args[1] == "--config" {
            args[2].clone()
        } else if std::path::Path::new("crates/audemsp-host/config/host.conf").exists() {
            // dev 默认（仓库根 cwd）— 不带 --config 时优先项目内配置
            "crates/audemsp-host/config/host.conf".to_string()
        } else {
            // 部署默认
            "/opt/audemsp/etc/host.conf".to_string()
        }
    };
    let config = match config::load(&config_path) {
        Ok(c) => {
            tracing::info!("Config loaded from {config_path}");
            c
        }
        Err(e) => {
            tracing::warn!("Config {config_path}: {e}, using defaults");
            serde_yaml::from_str(&default_host_config()).unwrap()
        }
    };// ponytail: fallback to defaults when config file missing, add config wizard when needed

    // Parse resolution "WIDTHxHEIGHT"
    let (width, height) = parse_resolution(&config.capture.resolution);
    let framerate = config.capture.framerate;
    let bitrate = config.encoder.bitrate_kbps;
    let encoder = &config.encoder.backend;

    // Phase 1: Load or create session state
    let session_state = session::Session::load(None);
    let persist_handle = session_state.start_persist();

    // Collect background task handles for clean shutdown
    let mut background_tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    // PIT-81: SFU 帧生成器必须存活到 main 结束（if 分支作用域内绑定 → 分支结束即 Drop → 线程被 stop）。
    let mut frame_generator: Option<audemsp_media::pipeline::generator::VideoFrameGenerator> = None;

    // Phase 2: Start GStreamer pipeline
    let pipeline = std::sync::Arc::new(pipeline::Pipeline::new(
        &config.capture,
        width,
        height,
        framerate,
        bitrate,
        encoder,
    )
    .unwrap_or_else(|e| {
        tracing::warn!("Pipeline init failed: {e}, running headless");
        // ponytail: return dummy pipeline for headless mode (E2E testing)
        pipeline::Pipeline::dummy()
    }));
    // ponytail: pipeline start may fail without GStreamer; non-fatal for E2E
    if let Err(e) = pipeline.start() {
        tracing::warn!("Pipeline start failed: {e}, continuing headless");
    }

    // Phase 3: Create control handler (shared with metrics)
    // Phase 4: Create control handler (shared with metrics)
    let control_handler = control::ControlHandler::new();
    let frames_dropped = control_handler.frames_dropped.clone();
    let control_handler = Arc::new(Mutex::new(control_handler));

    // Phase 5: Build axum router (metrics)
    let core_metrics = audemsp_common::metrics::CoreMetrics::new();
    let shared_metrics = std::sync::Arc::new(std::sync::RwLock::new(core_metrics));
    let metrics_router = metrics::metrics_router(shared_metrics.clone());

let app = axum::Router::new()
    .merge(metrics_router)
    .layer(TimeoutLayer::new(Duration::from_secs(30)));

    // Determine bind address
    let bind_addr = "0.0.0.0:9801"; // ponytail: separate port from server (9800)

    let listener = match tokio::net::TcpListener::bind(bind_addr).await {
        Ok(l) => {
            tracing::info!("Listening on {}", bind_addr);
            l
        }
            Err(e) => {
                tracing::error!("Failed to bind {}: {}", bind_addr, e);
                return Err(anyhow::anyhow!("Failed to bind {bind_addr}: {e}"));
            }
        };

    tracing::info!("Host ready — session id={}", config.capture.source);

    // Phase 4: Connect to signaling and create WebRTC transport
    let signaling_url = config.server.signaling_url.clone();
    let psk = config
        .psk
        .clone()
        .unwrap_or_else(|| "audemsp-dev".to_string());
    let room_id = config.room.id.clone();

    const MAX_RETRIES: u32 = 5;
    let mut last_err = None;
    let client = SignalingClient::new(&signaling_url, &psk, &room_id);
    let (mut ws_sender, mut ws_receiver) = {
        let mut delay = std::time::Duration::from_secs(1);
        let mut result = None;
        for attempt in 1..=MAX_RETRIES {
            match client.connect().await {
                Ok(pair) => { result = Some(pair); break; }
                Err(e) => {
                    tracing::warn!(attempt, max = MAX_RETRIES, "Signaling connect failed: {e}, retrying in {delay:?}");
                    last_err = Some(e);
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(std::time::Duration::from_secs(16));
                }
            }
        }
        match result {
            Some(pair) => pair,
            None => {
                tracing::error!("Signaling connection failed after {MAX_RETRIES} attempts: {last_err:?}");
                return Err(anyhow::anyhow!("Signaling connection failed after {MAX_RETRIES} attempts: {last_err:?}"));
            }
        }
    };
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;


    // PipelineEngine: P2P path only — SFU uses audemsp-webrtc (C12)
    // ponytail: engine declared here so it's visible to cleanup at end of main()
    #[cfg(feature = "webrtc-p2p")]
    #[allow(unused_variables)]
    let engine = PipelineEngine::new(tokio::runtime::Handle::current());
    #[cfg(not(feature = "webrtc-p2p"))]
    let engine = (); // placeholder for cleanup scope

    // SFU produce (mediasoup) — via audemsp-webrtc abstraction
    if config.sfu_produce {
        tracing::info!("SFU produce mode — negotiating mediasoup transport");
        let peer_id = "host"; // ponytail: use config peer identity when available
        let sfu_room = room_id.clone();

        // Step 1: CreateWebRtcTransport (direction: Send)
        let create_transport = SignalingMessage::CreateWebRtcTransport {
            room_id: sfu_room.clone(),
            peer_id: peer_id.to_string(),
            direction: TransportDirection::Send,
        };
        let json = serde_json::to_string(&create_transport)?;
        ws_sender
            .send(Message::Text(json.into()))
            .await
            .map_err(|e| anyhow::anyhow!("SFU CreateWebRtcTransport send: {}", e))?;
        tracing::info!("SFU: CreateWebRtcTransport sent");

        // Step 2: Wait for WebRtcTransportCreated (with real ice_candidates from Part C)
        let (transport_id, ice_parameters, dtls_parameters, ice_candidates) = loop {
            match ws_receiver.next().await {
                Some(Ok(Message::Text(text))) => {
                    let msg: SignalingMessage = serde_json::from_str(&text)
                        .map_err(|e| anyhow::anyhow!("SFU parse: {}", e))?;
                    match msg {
                        SignalingMessage::WebRtcTransportCreated {
                            room_id: _, peer_id: _, transport_id, ice_parameters, dtls_parameters, ice_candidates,
                        } => break (transport_id, ice_parameters, dtls_parameters, ice_candidates),
                        SignalingMessage::Sdp { .. } | SignalingMessage::RTCIceCandidate { .. } => {
                            tracing::debug!("SFU: skipping P2P message");
                        }
                        SignalingMessage::Error { code, message } => {
                            return Err(anyhow::anyhow!("SFU error [{}]: {}", code, message));
                        }
                        // PIT-59: server 重启清理旧 peer 时广播 RoomLeave — 非自己的直接忽略
                        // (旧 Host 的 RoomLeave 会污染新 Host 的 forward loop, 误判退出)
                        SignalingMessage::RoomLeave { peer_id, .. } => {
                            tracing::info!("SFU: RoomLeave for peer {} ignored", peer_id);
                        }
                        // PIT-65: 旧 Host 的 producer 残留 server → 新 Host 连接时广播 stale NewProducer → 忽略
                        // (本 Host 关注视频帧, 不消费 NewProducer; 只等 transport + produce 响应)
                        SignalingMessage::NewProducer { .. } => {
                            tracing::debug!("SFU: NewProducer ignored");
                        }
                        other => {
                            return Err(anyhow::anyhow!("SFU: unexpected {:?}", other));
                        }
                    }
                }
                Some(Ok(other)) => return Err(anyhow::anyhow!("SFU: unexpected WS: {:?}", other)),
                Some(Err(e)) => return Err(anyhow::anyhow!("SFU: WS error: {}", e)),
                None => return Err(anyhow::anyhow!("SFU: WS closed")),
            }
        };
        let candidate_count = ice_candidates.as_ref().map_or(0, |v| v.len());
        tracing::info!("SFU: WebRtcTransportCreated id={}, candidates={}", transport_id, candidate_count);

        // Step 3: Create RTCPeerConnection via audemsp-webrtc (NOT webrtc-rs)
        let ice_servers: Vec<RTCIceServer> = config.server.ice_servers.iter()
            .map(|url| RTCIceServer {
                urls: vec![url.clone()],
                username: String::new(),
                password: String::new(),
            })
            .collect();
        let rtc_config = RTCConfiguration {
            ice_servers,
            ice_transport_type: RTCIceTransportPolicy::All,
        };
        let factory = RTCPeerConnectionFactory::new();
        let pc = factory.create_peer_connection(rtc_config).await
            .map_err(|e| anyhow::anyhow!("PC create: {}", e))?;

        // B4: Register ICE/PC state callbacks
        let produce_trigger = Arc::new(tokio::sync::Notify::new());
        let produce_ready = produce_trigger.clone();
        // ponytail: pc is Clone, so we can capture it in callbacks
        {
            let _pc = pc.clone();
            pc.on_ice_connection_state_change(move |state| {
                let _ = &_pc;
                tracing::info!("SFU ICE state: {:?}", state);
            });
        }
        {
            let _pc = pc.clone();
            let ready = produce_ready.clone();
            pc.on_peer_connection_state_change(move |state| {
                let _ = &_pc;
                tracing::info!("SFU PC state: {:?}", state);
                if state == RTCPeerConnectionState::Connected {
                    ready.notify_one();
                }
            });
        }

        // P3 (v2)+P2: 标准 answerer 协商（对齐 libmediasoupclient Handler.cpp 顺序）:
        //  ① 用 server transport 参数构造 remote SDP → set_remote_description
        //  ② add_track (sendrecv transceiver + 默认 encoding; libwebrtc 自动生成)
        //  ③ create_answer → set_local_description
        // 顺序关键: set_remote_description 必须先于 add_track — 反之 transceiver
        // 不匹配 remote m-line → answer a=inactive → codecs=0 → produce 被拒
        // 不用 add_transceiver_with_track: 空 send_encodings → libwebrtc 生成无 encoding
        // sender → answer inactive (P2 实测); add_track 与 e2e_sfu 验证路径一致
        // PIT-48: a=candidate 行必须位于 m= 行之后（media section 内）——
        // 会话级 candidate 被 libwebrtc 忽略 → remote candidate 丢失 → ICE 不发起 STUN

        // ① remote SDP from real server ICE candidates — codec 由 config.encoder.codec 控制 (v2, T3/T4)
        // 固定 offer codec = 固定协商交集 = 固定实际编码（Oracle 审核: produce 参数裁剪不可行）
        // PT 对齐 router 默认（sfu.rs default_router_options: VP8 96 / H264 101）; VP9/AV1 router 无 → 协商失败负向
        let (sdp_pt, sdp_codec, sdp_clock, sdp_fmtp) = match config.encoder.codec.as_str() {
            "h264" => (101u16, "H264", 90000u32, Some("profile-level-id=42e01f;packetization-mode=1")), // v2 T7: 与 router 42e01f 对齐
            "vp8" => (96u16, "VP8", 90000u32, None),
            "vp9" => (98u16, "VP9", 90000u32, None),
            "av1" => (100u16, "AV1", 90000u32, None),
            _ => (96u16, "VP8", 90000u32, None), // auto 默认: 现状行为（router 序 VP8 优先）
        };
        let remote_sdp = sfu_media::build_remote_sdp(
            &ice_parameters,
            &dtls_parameters,
            ice_candidates.as_ref(),
            sdp_pt,
            sdp_codec,
            sdp_clock,
            sdp_fmtp,
        );
        tracing::info!("SFU: offer codec={sdp_codec} PT={sdp_pt} (encoder.codec={})", config.encoder.codec);
        let remote_desc = RTCSessionDescription::new(RTCSdpType::Offer, remote_sdp);
        pc.set_remote_description(&remote_desc).await
            .map_err(|e| anyhow::anyhow!("set remote: {}", e))?;
        tracing::info!("SFU: remote description set (server ICE-Lite offer)");

        // ② add track (sendrecv; answer 协商后 host 侧 sendonly)
        let track_id = pc.add_track("video", TrackKind::Video)
            .map_err(|e| anyhow::anyhow!("add_track: {}", e))?;
        let track_ref = pc.get_track(&track_id)
            .ok_or_else(|| anyhow::anyhow!("track not found after add_track"))?;
        let video_track = match track_ref {
            TrackRef::Sender(s) => s,
            _ => return Err(anyhow::anyhow!("expected sender track")),
        };

        // v2 (T5): 编码器软/硬后端选择 — 协商前设置（首个编码器创建于首帧, SetEncoderSelector 生效）
        // 语义: 偏好非强制（不可用时 libwebrtc 自动 fallback + warning）
        if let Some(backend) = audemsp_webrtc::rtp::RTCVideoEncoderBackend::from_config(&config.encoder.backend) {
            if backend != audemsp_webrtc::rtp::RTCVideoEncoderBackend::Auto {
                match pc.get_senders().iter().find(|s| s.track_id == track_id) {
                    Some(sender) => {
                        if let Err(e) = sender.set_video_encoder_backend(backend) {
                            tracing::warn!("SFU: set_video_encoder_backend({backend:?}): {e}");
                        }
                    }
                    None => tracing::warn!("SFU: sender not found for backend config: {track_id}"),
                }
            }
        }
        tracing::info!("SFU: video track added (id={})", track_id);

        // ③ answer + set local — PIT-76 v2: x-google 注入已移除（见 build_remote_sdp）
        let answer = pc.create_answer(&RTCAnswerOptions::default()).await
            .map_err(|e| anyhow::anyhow!("create answer: {}", e))?;
        tracing::debug!("SFU local answer SDP:\n{}", answer.sdp);
        pc.set_local_description(&answer).await
            .map_err(|e| anyhow::anyhow!("set local: {}", e))?;
        // B3: Extract DTLS fingerprint via audemsp-webrtc API (not SDP parsing)
        let fp_hex = pc.local_dtls_fingerprint()
            .ok_or_else(|| anyhow::anyhow!("no DTLS fingerprint"))?;
        let connect = SignalingMessage::ConnectWebRtcTransport {
            room_id: sfu_room.clone(),
            peer_id: peer_id.to_string(),
            transport_id: transport_id.clone(),
            dtls_parameters: DtlsParameters {
                fingerprints: vec![Fingerprint {
                    algorithm: "sha-256".to_string(),
                    value: fp_hex,
                }],
                role: "client".to_string(),
            },
        };
        let json = serde_json::to_string(&connect)?;
        ws_sender
            .send(Message::Text(json.into()))
            .await
            .map_err(|e| anyhow::anyhow!("SFU ConnectWebRtcTransport: {}", e))?;
        tracing::info!("SFU: ConnectWebRtcTransport sent");

        // B6: Wait for ICE/DTLS connection (30s timeout)
        match tokio::time::timeout(Duration::from_secs(30), produce_trigger.notified()).await {
            Ok(()) => tracing::info!("SFU: PC Connected, sending Produce"),
            Err(_) => return Err(anyhow::anyhow!("SFU: ICE/DTLS connection timeout after 30s")),
        }

        // Step 4: Produce video (P3 v2: 从 get_sending_rtp_parameters 推导，非手工)
        // PIT-56 替代: 不再手工解析 answer ssrc — 走 transceiver.sender.get_parameters() 官方路径
        let rtp_params: RTCRtpParameters = pc.get_sending_rtp_parameters("video")
            .map_err(|e| anyhow::anyhow!("get_sending_rtp_parameters: {}", e))?;
        tracing::debug!("SFU: negotiated rtp params mid={} codecs={} encodings={}",
            rtp_params.mid, rtp_params.codecs.len(), rtp_params.encodings.len());

        let produce = SignalingMessage::Produce {
            room_id: sfu_room,
            peer_id: peer_id.to_string(), // PIT-65: 与 create/connect 一致 (host)
            transport_direction: TransportDirection::Send,
            kind: MediaKind::Video,
            rtp_parameters: sfu_media::build_produce_rtp_parameters_from_rtp(&rtp_params),
        };
        let json = serde_json::to_string(&produce)?;
        match tokio::time::timeout(Duration::from_secs(10), ws_sender.send(Message::Text(json.into()))).await {
            Ok(Ok(())) => tracing::info!("SFU: Produce (Video) sent"),
            Ok(Err(e)) => return Err(anyhow::anyhow!("SFU Produce send error: {}", e)),
            Err(_) => return Err(anyhow::anyhow!("SFU Produce send timeout after 10s")),
        }

        // B5 (v2): VideoFrameGenerator + WebRtcTrackSink — 统一帧源接口
        // 计划: .sisyphus/plans/video-source-unification T3 (WebRtcTrackSink: c56bd87)。
        // 旧手写循环已删除（原 main.rs:373-406; 历史参考 d24f6e5 SquaresPattern 引入 /
        // 9cf94b8 b=AS 码率预算 / 90ea937 关键帧触发）。C17 语义由 generator 内部保证:
        // 绝对时间轴 + 锚定单调时间戳 + 时间戳水印 (TopLeft, DateTime+FrameCount)。
        // PIT-81: generator 绑定 main 级 frame_generator（if 分支内绑定 → 分支结束 Drop 停线程）。
        {
            use audemsp_media::pipeline::generator::{
                Anchor, BitmapFont, ColorStrategy, PatternMode, SquaresConfig, TextBurner,
                TimestampFormat, TimestampOverlay, VideoFrameGenerator,
            };
            use audemsp_media::pipeline::sink::VideoSinkWants;
            use audemsp_media::pipeline::source::VideoSource;
            use audemsp_webrtc::WebRtcTrackSink;

            // v2: fps/resolution 接 config.capture（修复旧循环硬编码 33ms/640×480）。
            let fps = config.capture.framerate.max(1);
            let (width, height) = parse_resolution(&config.capture.resolution);

            let burner = TextBurner::new(BitmapFont::new(), false, Anchor::TopLeft);
            let overlay = TimestampOverlay::new(burner, TimestampFormat::Combined);
            let squares = SquaresConfig {
                count: 40,
                min_size: 20,
                max_size: 300,
                motion_speed: 10, // 沿用 B5 常量（v2 审核确认）
                color_strategy: ColorStrategy::default(),
            };

            let generator = VideoFrameGenerator::new();
            let sink = WebRtcTrackSink::new(video_track.clone())
                .map_err(|e| anyhow::anyhow!("WebRtcTrackSink: {}", e))?;
            generator.add_or_update_sink(Box::new(sink), VideoSinkWants::default());
            generator.start(fps, PatternMode::Squares(squares), Some(overlay), width, height);
            // Drop guard: main 退出/错误路径 → generator.stop() 停线程 (BLOCKER-4)。
            frame_generator = Some(generator);
        }

        // PIT-76: 周期关键帧触发 — GOP ≤ keyframe_interval 秒（默认 2s）。
        // interval 首 tick 立即 = 协商完成即触发一次（快速首帧）; libwebrtc 每次
        // 消费后清标志, 每次调用恰好一次 GenerateKeyFrame, 无需复位。
        // 独立 task 与帧循环解耦; 同步 cxx 调用（worker Invoke, 毫秒级）不影响帧节奏。
        let pc_kf = pc.clone();
        let track_kf = track_id.clone();
        let kf_interval_secs = config.encoder.keyframe_interval.max(1) as u64;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(kf_interval_secs));
            loop {
                interval.tick().await;
                if let Err(e) = pc_kf.request_key_frame(&track_kf) {
                    tracing::warn!("SFU: request_key_frame: {}", e);
                }
            }
        });
tracing::info!("SFU produce transport {} ready — Squares b=AS fix", transport_id);
    } else {

    // P2P transport path — gated behind webrtc-p2p feature
    #[cfg(feature = "webrtc-p2p")]
    {
    // ponytail: wait for remote to join room before sending SDP offer
    // ponytail: wait for remote to join room before sending SDP offer

    let (webrtc_transport, dc_events) =
        webrtc_transport::WebrtcTransport::new(ws_sender, room_id)
            .await
            .map_err(|e| {
                tracing::error!("WebRTC transport creation failed: {e}");
                anyhow::anyhow!("WebRTC transport creation failed: {e}")
            })?;

    let webrtc = Arc::new(webrtc_transport);

    // Spawn WS receiver loop — handles incoming SDP answers and ICE candidates
    let ws_webrtc = webrtc.clone();
    let ws_receiver_handle = tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(msg) => {
                    if let Ok(text) = msg.to_text() {
                        tracing::debug!("WS received: {}", text);
                        if let Ok(sig_msg) =
                            serde_json::from_str::<SignalingMessage>(text)
                        {
                            match sig_msg {
                                SignalingMessage::Sdp { sdp, .. } => {
                                    // ponytail: ignore offer echo from server relay; only process answers
                                    let sdp_type = serde_json::from_str::<serde_json::Value>(&sdp)
                                        .ok()
                                        .and_then(|v| v.get("type")?.as_str().map(String::from));
                                    if sdp_type.as_deref() != Some("answer") {
                                        tracing::debug!("Ignoring non-answer SDP (type={sdp_type:?})");
                                        continue;
                                    }
                                    tracing::info!("Received SDP answer, setting remote description");
                                    match ws_webrtc.handle_answer(&sdp).await {
                                        Ok(()) => tracing::info!("Remote description set"),
                                        Err(e) => tracing::error!("Failed to set remote description: {e}"),
                                    }
                                }
                                SignalingMessage::RTCIceCandidate { candidate, sdp_mid, sdp_mline_index, .. } => {
                                    let candidate_json = serde_json::json!({
                                        "candidate": candidate,
                                        "sdpMid": sdp_mid,
                                        "sdpMLineIndex": sdp_mline_index,
                                    }).to_string();
                                    match ws_webrtc.handle_remote_ice(&candidate_json).await {
                                        Ok(()) => tracing::debug!("ICE candidate added"),
                                        // ponytail: ICE-before-SDP race is expected; non-fatal
                                        Err(e) => tracing::debug!("ICE candidate deferred: {e}"),
                                    }
                                }
                                SignalingMessage::RoomLeave { .. } => {
                                    tracing::info!("Peer left room");
                                }
                                SignalingMessage::WebRtcTransportCreated { transport_id, .. } => {
                                    // ponytail: logged for SFU path; SFU skeleton consumes this synchronously,
                                    // this is a fallback for async arrival
                                    tracing::info!("SFU: WebRtcTransportCreated id={} (async)", transport_id);
                                }
                                _ => {} // ponytail: ignore other variants
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!("WS receive error: {e}"),
            }
        }
        tracing::warn!("WS receiver loop ended");
    });
    background_tasks.push(ws_receiver_handle);

    // Spawn DC event loop — logs lifecycle events
    let dc_event_handle = tokio::spawn(async move {
        webrtc_transport::run_dc_event_loop(dc_events, control_handler.clone()).await;
    });
    background_tasks.push(dc_event_handle);

    // Phase 7: Emergency UDP listener (background)
    let emergency_handle = tokio::spawn(async move {
        let listener = match emergency::EmergencyListener::bind(9999).await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("Emergency listener failed to bind: {e}");
                return;
            }
        };
        if let Err(e) = listener.listen().await {
            tracing::error!("Emergency listener error: {e}");
        }
    });
    background_tasks.push(emergency_handle);

    // PipelineEngine: orchestrate capture → encode → WebRTC push
    let push_pipeline = pipeline.clone();
    let push_webrtc = webrtc.clone();
    let shared_m = shared_metrics.clone();

    engine.add_chain(
        "capture".into(),
        Box::new(engine_adapters::GstCaptureSource::new(push_pipeline.clone())),
        vec![],
        vec![Box::new(engine_adapters::WebrtcOutputSink::new(
            push_webrtc.clone(),
        ))],
    ).expect("Failed to add capture chain");

    engine.start().expect("Failed to start engine");

    }  // end P2P cfg
    #[cfg(not(feature = "webrtc-p2p"))]
    {
        tracing::error!("SFU produce disabled and webrtc-p2p not enabled; no transport available");
        return Err(anyhow::anyhow!("No WebRTC transport feature available"));
    }
    }  // end else (P2P path)
    // Start metrics updater: sync dropped frames counter
    let dropped = frames_dropped;
    let metrics_updater = tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            let _ = dropped.load(std::sync::atomic::Ordering::Relaxed);
        }
    });
    background_tasks.push(metrics_updater);

    // Run server (blocks until shutdown signal)
    let server = axum::serve(listener, app).with_graceful_shutdown(async {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Shutting down...");
    });

    if let Err(e) = server.await {
        tracing::error!("Server error: {}", e);
    }
    tracing::info!("Shutdown complete");

    // Stop pipeline
    if let Err(e) = pipeline.stop() {
        tracing::error!("Pipeline stop error: {}", e);
    }

    // Clean up background tasks
    // Stop engine before aborting tasks
    #[cfg(feature = "webrtc-p2p")]
    let _ = engine.stop().await;

    for handle in background_tasks {
        handle.abort();
    }
    // ponytail: brief wait for graceful abort before runtime drops
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // persist_handle runs for session lifetime, abort last
    persist_handle.abort();

Ok(())
}

/// Parse "WIDTHxHEIGHT" into (width, height). Defaults to 1280x720.
fn parse_resolution(res: &str) -> (u32, u32) {
    let parts: Vec<&str> = res.split('x').collect();
    if parts.len() == 2 {
        let w = parts[0].parse().unwrap_or(1280);
        let h = parts[1].parse().unwrap_or(720);
        (w, h)
    } else {
        (1280, 720)
    }
}

/// Generate a default host config YAML for headless/E2E fallback.
fn default_host_config() -> String {
    r#"
server:
  signaling_url: "ws://localhost:9800/ws"
  ice_servers: []
capture:
  source: "test_pattern"
  resolution: "1280x720"
  framerate: 30
  device: null
encoder:
  backend: "auto"
  bitrate_kbps: 2000
  keyframe_interval: 2
room:
  id: "default-room"
psk: "audemsp-dev"
"#.to_string()
}
