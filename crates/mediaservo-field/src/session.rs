//! field 会话门面：推流（PushSession）/ 拉流（PullSession）。
//!
//! 契约 §4：`PushSession`（采集→编码→推流）/ `PullSession`（订阅→解码→出帧）。
//! push 链路复用 host SFU 推流序列（CreateWebRtcTransport → answer 协商 →
//! Connect → Produce → WebRtcTrackSink 帧注入），经 mediaservo-webrtc 抽象层（C12）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mediaservo_common::protocol::{
    DtlsParameters, Fingerprint, IceCandidate, IceParameters, MediaKind, PeerRole,
    SignalingMessage, TransportDirection,
};
use mediaservo_deck::DeckError;
use mediaservo_link::{SignalClient, SignalEvent, SignalSession};
use mediaservo_webrtc::traits::PeerConnectionApi;
use mediaservo_media::pipeline::generator::{
    Anchor, BitmapFont, ColorStrategy, PatternMode, SquaresConfig, TextBurner, TimestampFormat,
    TimestampOverlay, VideoFrameGenerator,
};
use mediaservo_media::pipeline::sink::VideoSinkWants;
use mediaservo_media::pipeline::source::VideoSource;
use mediaservo_webrtc::{
    RTCPeerConnection, RTCPeerConnectionFactory, RTCConfiguration, RTCIceServer,
    RTCIceTransportPolicy, RTCSessionDescription, RTCSdpType, TrackKind, TrackRef,
    TrackSender, WebRtcTrackSink,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::config::{PublishOptions, PullConfig, PushConfig};
use crate::error::FieldError;
use crate::sfu;

/// 会话事件流（契约 §4）。
#[derive(Debug)]
#[non_exhaustive]
pub enum SessionEvent {
    /// 已连接并加入房间。
    Connected,
    /// 收到一条信令消息（透传，供上层做 SFU/设备发现等）。
    Message(SignalingMessage),
    /// 推流会话：track 发布成功。
    TrackPublished { track: String },
    /// 连接断开。
    Disconnected { reason: String },
    /// 错误。
    Error(FieldError),
}

/// 会话事件接收端。
pub type SessionEvents = UnboundedReceiver<SessionEvent>;

/// 会话事件发送端（内部）。
pub(crate) type EventSender = UnboundedSender<SessionEvent>;

/// 推流会话（采集→编码→webrtc 推流）。
pub struct PushSession {
    signal: SignalSession,
    /// 已建立的 PeerConnection（首个 track 发布时惰性初始化）。
    pc: Option<RTCPeerConnection>,
    /// 已发布的 video sender（当前 MVP 单视频轨）。
    video_sender: Option<TrackSender>,
    /// 帧生成器（PIT-81: 必须 owned 存活——Drop 即停线程；stop_video_frames 显式停）。
    frame_generator: Option<VideoFrameGenerator>,
    events: EventSender,
    closed: Arc<AtomicBool>,
}

impl PushSession {
    /// 连接信令、加入房间；媒体（transport/produce）在首次 `publish_video` 时建立。
    pub async fn connect(cfg: PushConfig) -> Result<(Self, SessionEvents), FieldError> {
        let client = SignalClient::new(&cfg.url, &cfg.psk, &cfg.room, cfg.role.clone());
        let signal = client.connect().await.map_err(FieldError::Link)?;
        tracing::info!(room = %cfg.room, "PushSession connected to room");

        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut signal_events = signal.events();
        let events_tx_bridge = events_tx.clone();
        tokio::spawn(async move {
            while let Ok(ev) = signal_events.recv().await {
                let bridge = match ev {
                    SignalEvent::Message(msg) => SessionEvent::Message(msg),
                    SignalEvent::Disconnected { reason } => SessionEvent::Disconnected { reason },
                    SignalEvent::Error(e) => {
                        SessionEvent::Error(FieldError::Link(mediaservo_link::LinkError::Signal(e)))
                    }
                    SignalEvent::Connected { .. } => continue, // connect 已返回 Connected
                    _ => continue, // 其余信号事件忽略（non_exhaustive 兜底）
                };
                if events_tx_bridge.send(bridge).is_err() {
                    break;
                }
            }
        });

        let session = Self {
            signal,
            pc: None,
            video_sender: None,
            frame_generator: None,
            events: events_tx,
            closed: Arc::new(AtomicBool::new(false)),
        };
        Ok((session, events_rx))
    }

    /// 发布一路视频：SFU transport 建立 + answer 协商 + produce + 编码器配置。
    /// MVP 支持单视频轨；返回 track id（与 mediaservo-webrtc `add_track` 的 track id 一致）。
    pub async fn publish_video(
        &mut self,
        cfg: &PushConfig,
        opts: &PublishOptions,
    ) -> Result<String, FieldError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(FieldError::Closed);
        }
        if self.pc.is_some() {
            return Err(FieldError::InvalidState(
                "MVP PushSession 支持单路视频 — 已存在 active track".into(),
            ));
        }

        // 1. 先订阅信令事件（发送 CreateWebRtcTransport 前 — 避免 server 响应快于
        // subscribe 导致 WebRtcTransportCreated 丢失；broadcast 无历史重放）
        let mut transport_events = self.signal.events();

        // 2. CreateWebRtcTransport (Send)
        let create_transport = SignalingMessage::CreateWebRtcTransport {
            room_id: cfg.room.clone(),
            peer_id: peer_id(&cfg.role),
            direction: TransportDirection::Send,
        };
        self.signal.send(create_transport).await.map_err(FieldError::Link)?;

        // 3. 等待 WebRtcTransportCreated（其余消息忽略/透传）
        let (transport_id, ice_parameters, dtls_parameters, ice_candidates) =
            self.await_transport_created(&mut transport_events).await?;

        // 4. 建立 RTCPeerConnection（mediaservo-webrtc 抽象，C12）
        let rtc_config = RTCConfiguration {
            ice_servers: Vec::<RTCIceServer>::new(), // SFU: mediasoup ICE-Lite，无需 STUN
            ice_transport_type: RTCIceTransportPolicy::All,
        };
        let factory = RTCPeerConnectionFactory::new();
        let pc = factory
            .create_peer_connection(rtc_config)
            .await
            .map_err(|e| FieldError::WebRtc(format!("create peer connection: {e}")))?;

        // 5. 标准 answerer 协商：remote SDP → add_track → create_answer (P3 v2, C18)
        let sfu::CodecSpec {
            payload_type,
            name,
            clock_rate,
            fmtp,
        } = sfu::codec_spec(&opts.codec);
        let remote_sdp = sfu::build_remote_sdp(
            &ice_parameters,
            &dtls_parameters,
            ice_candidates.as_ref(),
            payload_type,
            name,
            clock_rate,
            fmtp,
        );
        let remote_desc = RTCSessionDescription::new(RTCSdpType::Offer, remote_sdp);
        pc.set_remote_description(&remote_desc)
            .await
            .map_err(|e| FieldError::WebRtc(format!("set remote description: {e}")))?;

        let track_id = pc
            .add_track("video", TrackKind::Video)
            .map_err(|e| FieldError::WebRtc(format!("add_track: {e}")))?;
        let track_ref = pc
            .get_track(&track_id)
            .ok_or_else(|| FieldError::WebRtc("track not found after add_track".into()))?;
        let sender = match track_ref {
            TrackRef::Sender(s) => s,
            _ => return Err(FieldError::WebRtc("expected sender track".into())),
        };

        // 编码器后端（软/硬）偏好 — 协商前设置（对齐 host: 经 get_senders 的 RTCRtpSender）
        if let Some(backend) = mediaservo_webrtc::rtp::RTCVideoEncoderBackend::from_config(
            &opts.encoder_backend,
        ) {
            if backend != mediaservo_webrtc::rtp::RTCVideoEncoderBackend::Auto {
                match pc.get_senders().iter().find(|s| s.track_id == track_id) {
                    Some(rtp_sender) => {
                        if let Err(e) = rtp_sender.set_video_encoder_backend(backend) {
                            tracing::warn!("set_video_encoder_backend({backend:?}): {e}");
                        }
                    }
                    None => tracing::warn!("sender not found for backend config: {track_id}"),
                }
            }
        }

        let answer = pc
            .create_answer(&mediaservo_webrtc::RTCAnswerOptions::default())
            .await
            .map_err(|e| FieldError::WebRtc(format!("create answer: {e}")))?;
        pc.set_local_description(&answer)
            .await
            .map_err(|e| FieldError::WebRtc(format!("set local description: {e}")))?;

        // 编码码率区间（协商后、produce 前）
        let max_bps = u64::from(cfg.bitrate_kbps) * 1000;
        match pc.get_senders().iter().find(|s| s.track_id == track_id) {
            Some(rtp_sender) => {
                if let Err(e) = rtp_sender.set_encoding_bitrate(None, Some(max_bps)) {
                    tracing::warn!("set_encoding_bitrate: {e}");
                }
            }
            None => tracing::warn!("sender not found for bitrate config: {track_id}"),
        }

        // 6. ConnectWebRtcTransport（DTLS fingerprint 经 API，非解析 SDP）
        let fp_hex = pc
            .local_dtls_fingerprint()
            .ok_or_else(|| FieldError::WebRtc("no DTLS fingerprint".into()))?;
        let connect = SignalingMessage::ConnectWebRtcTransport {
            room_id: cfg.room.clone(),
            peer_id: peer_id(&cfg.role),
            transport_id: transport_id.clone(),
            dtls_parameters: DtlsParameters {
                fingerprints: vec![Fingerprint {
                    algorithm: "sha-256".to_string(),
                    value: fp_hex,
                }],
                role: "client".to_string(),
            },
        };
        self.signal.send(connect).await.map_err(FieldError::Link)?;

        // 7. Produce（从协商结果推导 rtp_parameters，P3 v2 官方路径）
        let rtp_params = pc
            .get_sending_rtp_parameters("video")
            .map_err(|e| FieldError::WebRtc(format!("get_sending_rtp_parameters: {e}")))?;
        let produce = SignalingMessage::Produce {
            room_id: cfg.room.clone(),
            peer_id: peer_id(&cfg.role),
            transport_direction: TransportDirection::Send,
            kind: MediaKind::Video,
            rtp_parameters: sfu::build_produce_rtp_parameters_from_rtp(&rtp_params),
        };
        self.signal.send(produce).await.map_err(FieldError::Link)?;

        self.pc = Some(pc);
        self.video_sender = Some(sender);
        let _ = self.events.send(SessionEvent::TrackPublished {
            track: track_id.clone(),
        });
        tracing::info!(track = %track_id, "PushSession video published (codec={})", opts.codec);
        Ok(track_id)
    }

    /// 消费信令直到 `WebRtcTransportCreated`；错误/断开则返回 Err。
    /// `events` 必须在发送请求前已订阅（broadcast 无历史重放，先订阅防丢）。
    async fn await_transport_created(
        &self,
        events: &mut tokio::sync::broadcast::Receiver<SignalEvent>,
    ) -> Result<(String, IceParameters, DtlsParameters, Option<Vec<IceCandidate>>), FieldError>
    {
        loop {
            match events.recv().await {
                Ok(SignalEvent::Message(SignalingMessage::WebRtcTransportCreated {
                    transport_id,
                    ice_parameters,
                    dtls_parameters,
                    ice_candidates,
                    ..
                })) => {
                    return Ok((
                        transport_id,
                        ice_parameters,
                        dtls_parameters,
                        ice_candidates,
                    ));
                }
                Ok(SignalEvent::Message(SignalingMessage::Error { code, message })) => {
                    return Err(FieldError::WebRtc(format!(
                        "SFU error [{code}]: {message}"
                    )));
                }
                Ok(SignalEvent::Disconnected { reason }) => {
                    return Err(FieldError::InvalidState(format!(
                        "signal disconnected during transport create: {reason}"
                    )));
                }
                Ok(SignalEvent::Connected { .. }) => {}
                Ok(_) => {} // 其余消息忽略（NewProducer/RoomLeave 等，PIT-59/65 同类）
                Err(_) => {
                    return Err(FieldError::InvalidState(
                        "signal event stream closed during transport create".into(),
                    ));
                }
            }
        }
    }

    /// 获取底层 PeerConnection（escape hatch 用途；None 表示尚未 publish）。
    pub fn peer_connection(&self) -> Option<&RTCPeerConnection> {
        self.pc.as_ref()
    }

    /// 启动视频帧生成（Squares 彩条 + 时间戳水印）→ WebRtcTrackSink → TrackSender。
    /// 对齐 host B5 链路（C17: 锚定单调时间戳 + 绝对时间轴帧循环由 generator 内建）。
    /// 需先 `publish_video`；重复调用返回 InvalidState。
    pub fn start_video_frames(&mut self, cfg: &PushConfig) -> Result<(), FieldError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(FieldError::Closed);
        }
        if self.frame_generator.is_some() {
            return Err(FieldError::InvalidState("frames already running".into()));
        }
        let sender = self.video_sender.clone().ok_or_else(|| {
            FieldError::InvalidState("publish_video first (no video track)".into())
        })?;

        let fps = cfg.framerate.max(1);
        let (width, height) = (cfg.width, cfg.height);

        // 时间戳水印（对齐 host: TopLeft + DateTime+FrameCount）
        let burner = TextBurner::new(BitmapFont::new(), false, Anchor::TopLeft);
        let overlay = TimestampOverlay::new(burner, TimestampFormat::Combined);
        let squares = SquaresConfig {
            count: 40,
            min_size: 20,
            max_size: 300,
            motion_speed: 10,
            color_strategy: ColorStrategy::default(),
        };

        let generator = VideoFrameGenerator::new();
        let sink = WebRtcTrackSink::new(sender)
            .map_err(|e| FieldError::WebRtc(format!("WebRtcTrackSink: {e}")))?;
        generator.add_or_update_sink(Box::new(sink), VideoSinkWants::default());
        generator.start(fps, PatternMode::Squares(squares), Some(overlay), width, height);

        // PIT-81: generator 必须 owned 存活（Drop 停线程）
        self.frame_generator = Some(generator);
        tracing::info!("PushSession frames started {width}x{height}@{fps}fps");
        Ok(())
    }

    /// 停止帧生成（幂等；多次调用无副作用）。
    pub fn stop_video_frames(&mut self) {
        if let Some(g) = self.frame_generator.take() {
            g.stop();
            tracing::info!("PushSession frames stopped");
        }
    }

    /// 关闭会话：关闭信令 + 标记 closed（媒体发送由帧任务负责停止）。
    pub async fn close(self) -> Result<(), FieldError> {
        self.closed.store(true, Ordering::Relaxed);
        self.signal.close().await.map_err(FieldError::Link)
    }
}

impl std::fmt::Debug for PushSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PushSession")
            .field("signal", &self.signal)
            .field("pc", &self.pc)
            .field("video_sender", &self.video_sender)
            .field("frames_running", &self.frame_generator.is_some())
            .field("closed", &self.closed)
            .finish()
    }
}

/// 拉流会话（订阅→解码→出帧）。
#[derive(Debug)]
pub struct PullSession {
    signal: SignalSession,
    events: EventSender,
    _closed: Arc<AtomicBool>,
}

impl PullSession {
    /// 连接信令、加入房间（消费/解码链路 Phase 2+）。
    pub async fn connect(cfg: PullConfig) -> Result<(Self, SessionEvents), FieldError> {
        let client = SignalClient::new(&cfg.url, &cfg.psk, &cfg.room, cfg.role.clone());
        let signal = client.connect().await.map_err(FieldError::Link)?;
        tracing::info!(room = %cfg.room, "PullSession connected to room");

        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut signal_events = signal.events();
        let events_tx_bridge = events_tx.clone();
        tokio::spawn(async move {
            while let Ok(ev) = signal_events.recv().await {
                let bridge = match ev {
                    SignalEvent::Message(msg) => SessionEvent::Message(msg),
                    SignalEvent::Disconnected { reason } => SessionEvent::Disconnected { reason },
                    SignalEvent::Error(e) => {
                        SessionEvent::Error(FieldError::Link(mediaservo_link::LinkError::Signal(e)))
                    }
                    SignalEvent::Connected { .. } => continue,
                    _ => continue, // 其余信号事件忽略（non_exhaustive 兜底）
                };
                if events_tx_bridge.send(bridge).is_err() {
                    break;
                }
            }
        });

        let session = Self {
            signal,
            events: events_tx,
            _closed: Arc::new(AtomicBool::new(false)),
        };
        Ok((session, events_rx))
    }

    /// 关闭会话。
    pub async fn close(self) -> Result<(), FieldError> {
        self.signal.close().await.map_err(FieldError::Link)
    }
}

/// peer_id：复用 SignalClient 的 role 语义（host 为 "host"）。
fn peer_id(role: &PeerRole) -> String {
    match role {
        PeerRole::Host => "host".to_string(),
        PeerRole::Remote => "remote".to_string(),
        PeerRole::Consumer => "consumer".to_string(),
    }
}

/// 从 deck/link 错误便捷转换为 FieldError（供后续 slice 使用）。
pub(crate) fn deck_err(e: DeckError) -> FieldError {
    FieldError::Deck(e)
}