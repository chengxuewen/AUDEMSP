# mediasoup Rust API 参考 (v0.24.2)

> **来源**: [versatica/mediasoup v3/rust](https://github.com/versatica/mediasoup)  
> **生成日期**: 2026-07-31  
> **适用版本**: mediasoup 0.24.x (Rust crate)

---

## 1. 架构概览

```
WorkerManager
  └── Worker (C++ thread, 单 CPU 核, 处理 Router 实例)
       ├── WebRtcServer (单端口承载多个 WebRtcTransport, ICE Lite)
       └── Router (媒体路由)
            ├── WebRtcTransport (ICE/DTLS 网络路径, 收发媒体)
            │    ├── Producer (注入音视频源)
            │    └── Consumer (转发到终端)
            ├── PlainTransport (RTP/RTCP plain)
            ├── PipeTransport (同 Host Router 间通信)
            └── DirectTransport (进程内直传)
```

mediasoup Rust API 是 **ICE Lite** (服务端), 不主动发起 ICE 连接, 等待客户端 Binding Request.

---

## 2. Worker 创建

### 2.1 WorkerSettings

```rust
use mediasoup::worker::{WorkerSettings, WorkerLogTag, WorkerLogLevel};
use mediasoup::worker_manager::WorkerManager;
use std::net::{IpAddr, Ipv4Addr};
use std::ops::RangeInclusive;

let worker_manager = WorkerManager::new();

let worker_settings = WorkerSettings {
    log_level: WorkerLogLevel::Debug,  // Debug | Warn | Error | None
    log_tags: vec![
        WorkerLogTag::Info,
        WorkerLogTag::Ice,
        WorkerLogTag::Dtls,
        WorkerLogTag::Rtp,
        WorkerLogTag::Rtcp,
        // ...
    ],
    rtc_port_range: 10000..=59999,  // ICE/DTLS/RTP 端口范围
    dtls_files: None,  // None = 自动生成证书
    libwebrtc_field_trials: None,
    thread_initializer: None,
    app_data: AppData::default(),
};

let worker = worker_manager
    .create_worker(worker_settings)
    .await
    .expect("Failed to create worker");
```

### 2.2 Worker 生命周期回调

```rust
worker.on_new_router(|router| { /* Router 创建 */ });
worker.on_new_webrtc_server(|server| { /* WebRtcServer 创建 */ });
worker.on_dead(|result| { /* Worker 线程意外退出 */ });
worker.on_close(|| { /* Worker 关闭 */ });
```

---

## 3. Router 创建 & RouterOptions

### 3.1 RouterOptions + media_codecs

```rust
use mediasoup::router::RouterOptions;
use mediasoup_types::rtp_parameters::{
    RtpCodecCapability, MimeTypeAudio, MimeTypeVideo,
    RtpCodecParametersParameters,
};
use std::num::{NonZeroU32, NonZeroU8};

fn media_codecs() -> Vec<RtpCodecCapability> {
    vec![
        // Opus 音频
        RtpCodecCapability::Audio {
            mime_type: MimeTypeAudio::Opus,
            preferred_payload_type: None,  // None = 自动分配
            clock_rate: NonZeroU32::new(48000).unwrap(),
            channels: NonZeroU8::new(2).unwrap(),
            parameters: RtpCodecParametersParameters::from([
                ("useinbandfec", 1_u32.into()),
            ]),
            rtcp_feedback: vec![],
        },
        // VP8 视频
        RtpCodecCapability::Video {
            mime_type: MimeTypeVideo::Vp8,
            preferred_payload_type: None,
            clock_rate: NonZeroU32::new(90000).unwrap(),
            parameters: RtpCodecParametersParameters::default(),
            rtcp_feedback: vec![],
        },
        // H264 视频
        RtpCodecCapability::Video {
            mime_type: MimeTypeVideo::H264,
            preferred_payload_type: None,
            clock_rate: NonZeroU32::new(90000).unwrap(),
            parameters: RtpCodecParametersParameters::from([
                ("level-asymmetry-allowed", 1_u32.into()),
                ("packetization-mode", 1_u32.into()),
                ("profile-level-id", "4d0032".into()),
            ]),
            rtcp_feedback: vec![],
        },
    ]
}

let router = worker
    .create_router(RouterOptions::new(media_codecs()))
    .await
    .expect("Failed to create router");
```

**关键**: `RouterOptions::default()` 创建 `media_codecs: vec![]` → **空 codec 列表**, produce 会报 "Unsupported codec"。必须显式传入 `RouterOptions::new(media_codecs)`。

### 3.2 Router rtp_capabilities (返回客户端)

```rust
let rtp_capabilities = router.rtp_capabilities();
// 将 rtp_capabilities 序列化发送给客户端, 客户端用于 computeSendingRtpParameters
```

### 3.3 Router::can_consume

```rust
let can_consume = router.can_consume(&producer.id(), &client_rtp_capabilities);
```

---

## 4. WebRtcTransport

### 4.1 方式 A: Individual (独立端口)

```rust
use mediasoup::webrtc_transport::{
    WebRtcTransportOptions, WebRtcTransportListenInfos,
};
use mediasoup_types::data_structures::{ListenInfo, Protocol};
use std::net::{IpAddr, Ipv4Addr};

let transport = router
    .create_webrtc_transport(WebRtcTransportOptions::new(
        WebRtcTransportListenInfos::new(ListenInfo {
            protocol: Protocol::Udp,
            ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            announced_address: Some("公网IP或域名".to_string()),
            expose_internal_ip: false,
            port: None,               // None = 自动分配
            port_range: None,         // Some(40000..=40100)
            flags: None,
            send_buffer_size: None,
            recv_buffer_size: None,
        }),
    ))
    .await?;
```

### 4.2 方式 B: WebRtcServer (共享单端口) — 推荐

```rust
use mediasoup::webrtc_server::{WebRtcServerOptions, WebRtcServerListenInfos};

// 1. 创建 WebRtcServer (Worker 级别)
let webrtc_server = worker
    .create_webrtc_server(WebRtcServerOptions::new(
        WebRtcServerListenInfos::new(ListenInfo {
            protocol: Protocol::Udp,
            ip: IpAddr::V4(Ipv4Addr::UNSPECIFIED),  // 0.0.0.0
            announced_address: None,
            expose_internal_ip: false,
            port: Some(20000),  // 固定单端口!
            port_range: None,
            flags: None,
            send_buffer_size: None,
            recv_buffer_size: None,
        }),
    ))
    .await?;

// 2. 使用 WebRtcServer 创建 Transport
let transport = router
    .create_webrtc_transport(
        WebRtcTransportOptions::new_with_server(webrtc_server.clone())
    )
    .await?;
```

`new_with_server()` vs `new()`:
- `new_with_server`: `enable_tcp: true` (默认), `enable_udp: true`
- `new`: `enable_tcp: false` (默认), `enable_udp: true`

### 4.3 ICE/DTLS 参数获取 (发送给客户端)

```rust
// Transport 创建后立即获取参数发给浏览器
let id = transport.id();
let ice_parameters = transport.ice_parameters();  // usernameFragment + password
let ice_candidates = transport.ice_candidates();  // Vec<IceCandidate>
let dtls_parameters = transport.dtls_parameters(); // role + fingerprints
```

### 4.4 ICE State 变化回调

```rust
transport.on_ice_state_change(|ice_state| {
    match ice_state {
        IceState::New => {},
        IceState::Connected => { /* ICE 连通! */ },
        IceState::Completed => {},
        IceState::Disconnected => {},
        IceState::Failed => {},
        _ => {},
    }
});

transport.on_dtls_state_change(|dtls_state| {
    match dtls_state {
        DtlsState::Connected => { /* DTLS 握手完成! */ },
        _ => {},
    }
});
```

### 4.5 客户端 DTLS 参数连接 (connect)

```rust
use mediasoup::webrtc_transport::WebRtcTransportRemoteParameters;
use mediasoup_types::data_structures::{DtlsParameters, DtlsRole, DtlsFingerprint};

// 浏览器发送其 DTLS 参数后, 调用 connect:
transport
    .connect(WebRtcTransportRemoteParameters {
        dtls_parameters: DtlsParameters {
            role: DtlsRole::Client,  // 浏览器端
            fingerprints: vec![
                DtlsFingerprint::Sha256 {
                    value: [/* 32 bytes of SHA-256 hash */],
                }
            ],
        },
    })
    .await
    .expect("Failed to connect WebRTC transport");

// connect 只能调用一次, 重复调用返回 Err(RequestError::Response{..})
```

### 4.6 带宽控制

```rust
transport.set_max_incoming_bitrate(1_000_000).await?;  // bps
transport.set_max_outgoing_bitrate(2_000_000).await?;
transport.set_min_outgoing_bitrate(100_000).await?;
// 传入 0 移除限制
```

### 4.7 ICE Restart

```rust
let new_ice_params = transport.restart_ice().await?;
// 返回新的 IceParameters, 需转发给客户端
```

---

## 5. Producer — RtpParameters 完整格式

### 5.1 音频 Producer (Opus)

```rust
use mediasoup::producer::ProducerOptions;
use mediasoup_types::rtp_parameters::{
    MediaKind, MimeTypeAudio, RtcpParameters, RtpCodecParameters,
    RtpCodecParametersParameters, RtpEncodingParameters,
    RtpHeaderExtensionParameters, RtpHeaderExtensionUri, RtpParameters,
};

let audio_producer = transport.produce(ProducerOptions::new(
    MediaKind::Audio,
    RtpParameters {
        mid: Some("AUDIO".to_string()),
        codecs: vec![
            RtpCodecParameters::Audio {
                mime_type: MimeTypeAudio::Opus,
                payload_type: 0,  // 必须匹配 Router 中 Opus payloadType
                clock_rate: NonZeroU32::new(48000).unwrap(),
                channels: NonZeroU8::new(2).unwrap(),
                parameters: RtpCodecParametersParameters::from([
                    ("useinbandfec", 1_u32.into()),
                    ("usedtx", 1_u32.into()),
                ]),
                rtcp_feedback: vec![],
            }
        ],
        header_extensions: vec![
            RtpHeaderExtensionParameters {
                uri: RtpHeaderExtensionUri::Mid,
                id: 10,
                encrypt: false,
            },
            RtpHeaderExtensionParameters {
                uri: RtpHeaderExtensionUri::SsrcAudioLevel,
                id: 12,
                encrypt: false,
            },
        ],
        encodings: vec![
            RtpEncodingParameters {
                ssrc: Some(11111111),
                ..RtpEncodingParameters::default()
            }
        ],
        rtcp: RtcpParameters {
            cname: Some("audio-1".to_string()),
            ..RtcpParameters::default()
        },
        msid: None,
    },
)).await?;
```

### 5.2 视频 Producer (H264 + Simulcast)

```rust
let video_producer = transport.produce(ProducerOptions::new(
    MediaKind::Video,
    RtpParameters {
        mid: Some("VIDEO".to_string()),
        codecs: vec![
            RtpCodecParameters::Video {
                mime_type: MimeTypeVideo::H264,
                payload_type: 112,
                clock_rate: NonZeroU32::new(90000).unwrap(),
                parameters: RtpCodecParametersParameters::from([
                    ("packetization-mode", 1_u32.into()),
                    ("profile-level-id", "4d0032".into()),
                ]),
                rtcp_feedback: vec![
                    RtcpFeedback::Nack,
                    RtcpFeedback::NackPli,
                    RtcpFeedback::GoogRemb,
                ],
            },
            RtpCodecParameters::Video {
                mime_type: MimeTypeVideo::Rtx,
                payload_type: 113,
                clock_rate: NonZeroU32::new(90000).unwrap(),
                parameters: RtpCodecParametersParameters::from([("apt", 112u32.into())]),
                rtcp_feedback: vec![],
            },
        ],
        header_extensions: vec![
            RtpHeaderExtensionParameters {
                uri: RtpHeaderExtensionUri::Mid,
                id: 10,
                encrypt: false,
            },
            RtpHeaderExtensionParameters {
                uri: RtpHeaderExtensionUri::VideoOrientation,
                id: 13,
                encrypt: false,
            },
        ],
        encodings: vec![
            RtpEncodingParameters {
                ssrc: Some(22222222),
                rtx: Some(RtpEncodingParametersRtx { ssrc: 22222223 }),
                scalability_mode: "L1T5".parse().unwrap(),
                ..RtpEncodingParameters::default()
            },
            RtpEncodingParameters {
                ssrc: Some(22222224),
                rtx: Some(RtpEncodingParametersRtx { ssrc: 22222225 }),
                scalability_mode: "L1T5".parse().unwrap(),
                ..RtpEncodingParameters::default()
            },
            RtpEncodingParameters {
                ssrc: Some(22222226),
                rtx: Some(RtpEncodingParametersRtx { ssrc: 22222227 }),
                scalability_mode: "L1T5".parse().unwrap(),
                ..RtpEncodingParameters::default()
            },
        ],
        rtcp: RtcpParameters {
            cname: Some("video-1".to_string()),
            ..RtcpParameters::default()
        },
        msid: None,
    },
)).await?;
```

### 5.3 Producer Type 判定

| encodings 数 | scalability_mode | ProducerType |
|-------------|-----------------|--------------|
| 1 | None | `Simple` |
| 1 | `"L1T3"` 等 | `Svc` |
| ≥2 | 任意 | `Simulcast` |

### 5.4 Producer 事件

```rust
producer.on_pause(|| {});
producer.on_resume(|| {});
producer.on_score(|scores: &[ProducerScore]| {
    for s in scores {
        println!("encoding={} ssrc={} score={}", s.encoding_idx, s.ssrc, s.score);
    }
});
producer.on_close(|| {});
```

---

## 6. Consumer — RtpCapabilities 完整格式

### 6.1 RtpCapabilities (客户端设备能力)

```rust
use mediasoup_types::rtp_parameters::{
    MediaKind, MimeTypeAudio, MimeTypeVideo, RtcpFeedback,
    RtpCapabilities, RtpCodecCapability, RtpCodecParametersParameters,
    RtpHeaderExtension, RtpHeaderExtensionDirection, RtpHeaderExtensionUri,
};

let device_capabilities = RtpCapabilities {
    codecs: vec![
        RtpCodecCapability::Audio {
            mime_type: MimeTypeAudio::Opus,
            preferred_payload_type: Some(100),  // 客户端偏好 payloadType
            clock_rate: NonZeroU32::new(48000).unwrap(),
            channels: NonZeroU8::new(2).unwrap(),
            parameters: RtpCodecParametersParameters::default(),
            rtcp_feedback: vec![RtcpFeedback::Nack],
        },
        RtpCodecCapability::Video {
            mime_type: MimeTypeVideo::H264,
            preferred_payload_type: Some(101),
            clock_rate: NonZeroU32::new(90000).unwrap(),
            parameters: RtpCodecParametersParameters::from([
                ("level-asymmetry-allowed", 1_u32.into()),
                ("packetization-mode", 1_u32.into()),
                ("profile-level-id", "4d0032".into()),
            ]),
            rtcp_feedback: vec![
                RtcpFeedback::Nack,
                RtcpFeedback::NackPli,
                RtcpFeedback::CcmFir,
                RtcpFeedback::GoogRemb,
            ],
        },
        RtpCodecCapability::Video {
            mime_type: MimeTypeVideo::Rtx,
            preferred_payload_type: Some(102),
            clock_rate: NonZeroU32::new(90000).unwrap(),
            parameters: RtpCodecParametersParameters::from([("apt", 101_u32.into())]),
            rtcp_feedback: vec![],
        },
    ],
    header_extensions: vec![
        RtpHeaderExtension {
            kind: MediaKind::Audio,
            uri: RtpHeaderExtensionUri::Mid,
            preferred_id: 1,
            preferred_encrypt: false,
            direction: RtpHeaderExtensionDirection::default(),
        },
        RtpHeaderExtension {
            kind: MediaKind::Video,
            uri: RtpHeaderExtensionUri::Mid,
            preferred_id: 1,
            preferred_encrypt: false,
            direction: RtpHeaderExtensionDirection::default(),
        },
        RtpHeaderExtension {
            kind: MediaKind::Audio,
            uri: RtpHeaderExtensionUri::SsrcAudioLevel,
            preferred_id: 6,
            preferred_encrypt: false,
            direction: RtpHeaderExtensionDirection::default(),
        },
        RtpHeaderExtension {
            kind: MediaKind::Video,
            uri: RtpHeaderExtensionUri::VideoOrientation,
            preferred_id: 8,
            preferred_encrypt: false,
            direction: RtpHeaderExtensionDirection::default(),
        },
    ],
};
```

### 6.2 Consumer 创建

```rust
use mediasoup::consumer::{ConsumerOptions, ConsumerLayers};

let consumer = transport.consume(ConsumerOptions::new(
    producer.id(),                    // 要消费的 Producer ID
    device_capabilities,              // 客户端 RTP 能力
)).await?;

// 带选项:
let consumer = transport.consume({
    let mut options = ConsumerOptions::new(producer.id(), device_capabilities);
    options.paused = true;                           // 先暂停, 等客户端就绪
    options.preferred_layers = Some(ConsumerLayers {
        spatial_layer: 0,
        temporal_layer: Some(1),
    });
    options.enable_rtx = Some(true);                 // 启用 NACK 重传
    options.mid = Some("custom-mid".to_owned());     // 自定义 MID
    options.pipe = false;                            // PipeTransport 模式
    options.ignore_dtx = false;                      // DTX 包处理
    options
}).await?;
```

### 6.3 Consumer RtpParameters (转发给客户端)

```rust
let consumer_rtp_params = consumer.rtp_parameters();
// 包含: mid, codecs, header_extensions, encodings, rtcp
// 浏览器用此创建本地 RTCRtpReceiver
```

### 6.4 Consumer 控制

```rust
consumer.pause().await?;
consumer.resume().await?;
consumer.set_preferred_layers(ConsumerLayers {
    spatial_layer: 2, temporal_layer: Some(3),
}).await?;
consumer.set_priority(2).await?;   // 1-255, 默认 1
consumer.request_key_frame().await?;
consumer.get_stats().await?;       // ConsumerStat
```

---

## 7. DtlsParameters 结构

```rust
pub struct DtlsParameters {
    pub role: DtlsRole,  // Auto | Client | Server
    pub fingerprints: Vec<DtlsFingerprint>,
}

pub enum DtlsFingerprint {
    Sha1 { value: [u8; 20] },
    Sha224 { value: [u8; 28] },
    Sha256 { value: [u8; 32] },
    Sha384 { value: [u8; 48] },
    Sha512 { value: [u8; 64] },
}

// 客户端发送用于 connect:
// {
//   "role": "client",
//   "fingerprints": [{ "algorithm": "sha-256", "value": "82:5A:68:..." }]
// }
```

---

## 8. WebRtcServer 单端口模式 (完整流程)

```rust
use mediasoup::webrtc_server::{WebRtcServerOptions, WebRtcServerListenInfos};
use mediasoup::webrtc_transport::WebRtcTransportOptions;

// Step 1: Worker 创建 WebRtcServer (端口 20000)
let webrtc_server = worker
    .create_webrtc_server(WebRtcServerOptions::new(
        WebRtcServerListenInfos::new(ListenInfo {
            protocol: Protocol::Udp,
            ip: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            announced_address: None,
            expose_internal_ip: false,
            port: Some(20000),
            port_range: None,
            flags: None,
            send_buffer_size: None,
            recv_buffer_size: None,
        }),
    ))
    .await?;

// Step 2: 每个 Peer 创建 Transport 复用同一端口
let transport_1 = router
    .create_webrtc_transport(WebRtcTransportOptions::new_with_server(webrtc_server.clone()))
    .await?;

let transport_2 = router
    .create_webrtc_transport(WebRtcTransportOptions::new_with_server(webrtc_server))
    .await?;

// WebRtcServer 通过 local_ice_username_fragment 区分不同 Transport
```

---

## 9. 完整信令流程

```
Browser                          Server (Rust/mediasoup)
  │                                    │
  │──── WS: getRouterRtpCapabilities ──→│
  │←── router.rtp_capabilities() ──────│
  │                                    │
  │──── WS: createWebRtcTransport ─────→│
  │       (clientRtpCapabilities)      │ router.create_webrtc_transport(...)
  │←── transport.id() +                │
  │     iceParameters +                │
  │     iceCandidates +                │
  │     dtlsParameters ────────────────│
  │                                    │
  │─── WebRTC setRemoteDescription ───→│ (browser 本地)
  │←── createAnswer ──────────────────│
  │─── WebRTC setLocalDescription ────│
  │                                    │
  │──── WS: connectWebRtcTransport ────→│
  │       {dtlsParameters: browser}    │ transport.connect(remote_dtls_params)
  │←── "transport connected" ──────── │ (DTLS + ICE 完成)
  │                                    │
  │──── WS: produce ──────────────────→│
  │       {kind, rtpParameters}        │ transport.produce(producer_options)
  │←── producer.id() ──────────────────│
  │                                    │
  │──── WS: consume ───────────────────→│ (另一 Peer)
  │       {producerId, rtpCapabilities}│ transport.consume(consumer_options)
  │←── consumer.rtp_parameters() ─────│ → 用于创建 RTCRtpReceiver
```

---

## 10. 常见错误 & 解决方案

### E1: `RouterOptions::default()` 创建空 codec 列表

**症状**: `produce()` 返回 "Unsupported codec"

**根因**: `RouterOptions::default()` 产生 `media_codecs: vec![]`

**解决**: 必须使用 `RouterOptions::new(media_codecs)` 显式传入 codec 列表

### E2: `RtpCodecParameters` untagged enum 反序列化失败

**症状**: "data did not match any variant of untagged enum RtpCodecParameters"

**根因**: `RtpCodecParameters` 是 `#[serde(untagged)]` enum, 有 `Audio` 和 `Video` 变体。每个变体需要特定字段。Video 不需要 `channels`, Audio 需要。字段缺失导致匹配失败。

**解决**: 确保构造时 type-safe 使用 `RtpCodecParameters::Audio{...}` / `RtpCodecParameters::Video{...}` 而非 JSON 反序列化。

### E3: payloadType 不匹配

**症状**: produce 成功但客户端无法解码

**解决**: Producer RtpParameters 中的 payloadType 必须匹配 Router media_codecs 中该 codec 分配的值。可以从 Consumer rtp_parameters 中获取实际使用的 payloadType。

### E4: RTX codec 的 apt 参数错误

**症状**: `ProduceError::FailedRtpParametersMapping`

**根因**: RTX codec 的 `apt` 参数必须指向关联的主 codec payloadType

**解决**: 
```rust
RtpCodecParameters::Video {
    mime_type: MimeTypeVideo::Rtx,
    payload_type: 113,
    // ...
    parameters: RtpCodecParametersParameters::from([("apt", 112u32.into())]),
    // apt 必须 = 主 H264 codec 的 payloadType
}
```

### E5: Router 间 pipe 必须不同 Router

**症状**: `PipeProducerToRouterError::SameRouter`

**解决**: 目标 Router 必须与源 Router 不同

### E6: WebRtcTransport 二次 connect 失败

**症状**: `RequestError::Response { reason: "..." }`

**解决**: `transport.connect()` 只能调用一次, 重复调用会报错

### E7: encodings 为空导致 produce 失败 (视频)

**症状**: `ProduceError::Request`

**解决**: 视频 Producer 必须提供至少 1 个 encoding, 且每个 encoding 必须有 ssrc 或 rid

### E8: MID 冲突

**症状**: MID 重复导致 produce 失败

**解决**: 每个 Producer 的 `mid` 在同一 Transport 内必须唯一

---

## 11. Serde JSON 映射

mediasoup Rust types 使用 `#[serde(rename_all = "camelCase")]`, 序列化为 JSON 时字段名自动转 camelCase:

| Rust 字段 | JSON 字段 |
|-----------|----------|
| `payload_type` | `payloadType` |
| `clock_rate` | `clockRate` |
| `mime_type` | `mimeType` |
| `header_extensions` | `headerExtensions` |
| `rtcp_feedback` | `rtcpFeedback` |
| `ice_parameters` | `iceParameters` |
| `media_codecs` | `mediaCodecs` |
| `preferred_payload_type` | `preferredPayloadType` |
| `scalability_mode` | `scalabilityMode` |

**注意**: `RtpCodecParameters` 的 `Audio`/`Video` 变体使用 `#[serde(untagged)]`, 不做 tagging — 序列化时不包含 `"type": "audio"` 字段, mediasoup-worker 通过 `mimeType` 和是否存在 `channels` 字段区分。

---

## 12. 关键类型路径速查

| 类型 | 导入路径 |
|------|---------|
| `WorkerManager` | `mediasoup::worker_manager::WorkerManager` |
| `Worker` | `mediasoup::worker::Worker` |
| `WorkerSettings` | `mediasoup::worker::WorkerSettings` |
| `Router` | `mediasoup::router::Router` |
| `RouterOptions` | `mediasoup::router::RouterOptions` |
| `WebRtcServer` | `mediasoup::webrtc_server::WebRtcServer` |
| `WebRtcServerOptions` | `mediasoup::webrtc_server::WebRtcServerOptions` |
| `WebRtcTransport` | `mediasoup::webrtc_transport::WebRtcTransport` |
| `WebRtcTransportOptions` | `mediasoup::webrtc_transport::WebRtcTransportOptions` |
| `ProducerOptions` | `mediasoup::producer::ProducerOptions` |
| `ConsumerOptions` | `mediasoup::consumer::ConsumerOptions` |
| `RtpParameters` | `mediasoup_types::rtp_parameters::RtpParameters` |
| `RtpCapabilities` | `mediasoup_types::rtp_parameters::RtpCapabilities` |
| `RtpCodecCapability` | `mediasoup_types::rtp_parameters::RtpCodecCapability` |
| `RtpCodecParameters` | `mediasoup_types::rtp_parameters::RtpCodecParameters` |
| `MediaKind` | `mediasoup_types::rtp_parameters::MediaKind` |
| `DtlsParameters` | `mediasoup_types::data_structures::DtlsParameters` |
| `DtlsFingerprint` | `mediasoup_types::data_structures::DtlsFingerprint` |
| `ListenInfo` | `mediasoup_types::data_structures::ListenInfo` |
| `Protocol` | `mediasoup_types::data_structures::Protocol` |
| `AppData` | `mediasoup_types::data_structures::AppData` |

---

## 13. prelude 快捷导入

```rust
use mediasoup::prelude::*;
```

覆盖: `WorkerManager`, `Worker`, `Router`, `Transport`, `Producer`, `Consumer`, `WebRtcTransport`, `WebRtcServer`, 以及相关 types 的常用导出。

## 14. mediasoup-sys 构建要求

- **Linux x86_64 only** — mediasoup C++ Worker 不构建于 macOS/Windows
- `cargo check --features sfu-mediasoup` 在 macOS 可通过 (仅类型检查)
- `cargo build` / `cargo test` 需要 Linux 环境
- 需要 meson, ninja-build, libuv1-dev, libssl-dev
- `rtc_port_range` 默认 10000-59999, Docker 需映射相应 UDP 端口范围

---

**参见**:
- [mediasoup 官方文档](https://mediasoup.org/documentation/v3/)
- [RTP Parameters and Capabilities](https://mediasoup.org/documentation/v3/mediasoup/rtp-parameters-and-capabilities/)
- [GitHub 集成测试](https://github.com/versatica/mediasoup/tree/v3/rust/tests/integration)
