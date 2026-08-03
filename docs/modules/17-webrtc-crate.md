# 17. WebRTC Crate — audemsp-webrtc

> 状态：Phase 3 完成 | 关联决策：D11, D31, D139–D151 | 参考：webrtc-kit trait 抽象模式

## 定位

`audemsp-webrtc` 是 AUDEMSP 的 WebRTC 传输层 crate，对外暴露标准 W3C WebRTC API trait，内部通过编译期 feature gate 支持多后端分发。当前默认后端 `webrtc-sys`（libwebrtc C++ FFI）。

```
┌─────────────────────────────────────────────────────┐
│                 audemsp-webrtc                     │
│                                                     │
│  RTCEngine::create_factory()                        │
│    └→ RTCPeerConnectionFactory                      │
│         └→ RTCPeerConnection (pub trait)            │
│                                                     │
│  ┌─────────────────────────────────────────────┐    │
│  │              内部 trait 层                   │    │
│  │  PcBackend  │  DcBackend  │  TrackWriteBackend │ │
│  └──────────────┼─────────────┼──────────────────┘ │
│                 ▼             ▼                     │
│  ┌─────────────────────────────────────────────┐    │
│  │             编译期 type alias                 │    │
│  │  ActivePc / ActiveDc / ActiveTrack           │    │
│  └──────────────┬──────────────────────────────┘ │
│                 ▼                                  │
│  ┌─────────────────────────────────────────────┐    │
│  │    webrtc-sys    │   webrtc-rs   │   stub    │   │
│  │  (libwebrtc C++) │  (纯 Rust)   │  (测试用)  │   │
│  └──────────────────┴───────────────┴───────────┘   │
└─────────────────────────────────────────────────────┘
```

## 命名规范

所有对外 API 类型统一 `RTC` 前缀，对齐 W3C WebRTC 标准：

| 类型 | 命名 | W3C 标准 |
|------|------|----------|
| 引擎入口 | `RTCEngine` | — |
| 工厂 | `RTCPeerConnectionFactory` | — |
| 连接 | `RTCPeerConnection` | `RTCPeerConnection` |
| 数据通道 | `RTCDataChannel` | `RTCDataChannel` |
| 会话描述 | `RTCSessionDescription` | `RTCSessionDescription` |
| ICE 候选 | `RTCIceCandidate` | `RTCIceCandidate` |
| 配置 | `RTCConfiguration` | `RTCConfiguration` |
| RTP 发送 | `RTCRtpSender` | `RTCRtpSender` |
| RTP 接收 | `RTCRtpReceiver` | `RTCRtpReceiver` |
| 错误 | `RTCError` | — |
| ICE 连接状态 | `RTCIceConnectionState` | `RTCIceConnectionState` |
| ICE 收集状态 | `RTCIceGatheringState` | `RTCIceGatheringState` |
| 信令状态 | `RTCSignalingState` | `RTCSignalingState` |
| Offer 选项 | `RTCOfferOptions` | `RTCOfferOptions` |
| Answer 选项 | `RTCAnswerOptions` | `RTCAnswerOptions` |
| ICE 传输策略 | `RTCIceTransportPolicy` | `RTCIceTransportPolicy` |
| 统计 | `RTCStats` | — |

**内部类型不加前缀**：`TrackSender`、`TrackReceiver`、`PcBackend`、`DcBackend`、`TrackWriteBackend`。`AudioTrackConfig`、`DataChannelRx` 保留原名（无 W3C 对应类型）。

## Trait 层次

两层架构：

```
┌──────────────────────────────────────────┐
│  pub trait RTCPeerConnection             │  ← 对外 W3C API
│    create_offer / create_answer / close  │
│    add_track / on_track / get_senders...│
├──────────────────────────────────────────┤
│  RTCPeerConnectionImpl (struct)          │  ← 共享逻辑
│    tracks: HashMap<TrackRef>             │     track 注册表
│    on_track_callback: Arc<Mutex<...>>    │     回调管理
│    backend: ActivePc                     │     委托后端
├──────────────────────────────────────────┤
│  pub(crate) trait PcBackend              │  ← 内部后端 trait
│    create_offer / set_local_desc / ...   │
│    set_on_track / register_track / ...   │
├──────────────────────────────────────────┤
│  WebrtcSysPc │ WebrtcRsPc │ StubPc      │
└──────────────────────────────────────────┘
```

- **RTCPeerConnection trait**：对外 W3C API，方法使用 Rust 惯用 snake_case（`create_offer` / `on_track`），对齐 webrtc-rs/str0m 生态
- **RTCPeerConnectionImpl**：track 注册表、`MAX_TRACKS` 校验、回调管理——三个后端共享
- **PcBackend**：`pub(crate)` 可见，SDP/ICE 生成与解析、observer 注册——每个后端独立实现

D145 最初规定 Phase 0 RTCPeerConnection struct 而非 trait。该决策的范围是 PcBackend 内部后端层——后端实现用 concrete struct 直接对接 webrtc-sys，不引入后端的 trait 抽象。对外 API 层 `RTCPeerConnection` 定义为 pub trait 是为了提供稳定 W3C 契约，与后端抽象是不同层次的设计考量（2026-07-22 修订）。

### 方法命名原则

所有对外方法使用 Rust snake_case（D167），napi-rs 自动转换为 JS camelCase：

```rust
pub trait RTCPeerConnection: Send + Sync {
    async fn create_offer(&self, options: &RTCOfferOptions) -> Result<RTCSessionDescription>;
    async fn create_answer(&self, options: &RTCAnswerOptions) -> Result<RTCSessionDescription>;
    async fn set_local_description(&self, desc: &RTCSessionDescription) -> Result<()>;
    async fn set_remote_description(&self, desc: &RTCSessionDescription) -> Result<()>;
    async fn add_ice_candidate(&self, candidate: &RTCIceCandidate) -> Result<()>;
    async fn create_data_channel(&self, label: &str, init: RTCDataChannelInit) -> Result<RTCDataChannel>;

    fn connection_state(&self) -> RTCPeerConnectionState;
    fn ice_connection_state(&self) -> RTCIceConnectionState;
    fn ice_gathering_state(&self) -> RTCIceGatheringState;
    fn signaling_state(&self) -> RTCSignalingState;

    async fn close(&self);

    // Track management
    fn add_track(&self, track_id: &str, kind: TrackKind) -> Result<RTCRtpSender>;
    fn remove_track(&self, track_id: &str) -> Result<()>;
    fn get_senders(&self) -> Vec<RTCRtpSender>;
    fn get_receivers(&self) -> Vec<RTCRtpReceiver>;
    fn on_track<F>(&self, callback: F) where F: Fn(RTCRtpReceiver) + Send + Sync + 'static;
}
```

## 后端编译期分发

```rust
// backend/mod.rs — 一次只编译一个后端，零运行时开销
#[cfg(feature = "backend-webrtc-sys")]
pub type ActivePc = webrtc_sys::WebrtcSysPc;

#[cfg(feature = "backend-webrtc-rs")]
pub type ActivePc = webrtc_rs::WebrtcRsPc;

#[cfg(not(any(feature = "backend-webrtc-rs", feature = "backend-webrtc-sys")))]
pub type ActivePc = stub::StubPc;

// 互斥守卫 — 同时启用多个后端报编译错误
#[cfg(all(feature = "backend-webrtc-rs", feature = "backend-webrtc-sys"))]
compile_error!("Only one backend can be enabled at a time.");
```

| 后端 | Feature | 依赖 | 场景 |
|------|---------|------|------|
| `webrtc-sys` | `backend-webrtc-sys`（默认） | libwebrtc C++ (cmake) | 公网遥控、弱网穿透 |
| `webrtc-rs` | `backend-webrtc-rs` | 纯 Rust webrtc crate | 未来 Embed 升级 |
| `stub` | 无 feature 时 | 无 | 开发/测试/编译检查 |

`ActivePc` / `ActiveDc` / `ActiveTrack` / `ActiveFactory` 同理——四个 type alias 覆盖所有后端分发点。

## PcBackend trait

`pub(crate)` 可见的内部 trait，定义每个后端必须实现的 SDP/ICE 操作：

```
pub(crate) trait PcBackend: Send + Sync + 'static {
    // ── 必需 (10 methods) ──
    async fn create_offer(&self, &RTCOfferOptions) -> Result<RTCSessionDescription>;
    async fn create_answer(&self, &RTCAnswerOptions) -> Result<RTCSessionDescription>;
    async fn set_local_description(&self, &RTCSessionDescription) -> Result<()>;
    async fn set_remote_description(&self, &RTCSessionDescription) -> Result<()>;
    async fn add_ice_candidate(&self, &RTCIceCandidate) -> Result<()>;
    async fn close(&self);
    fn connection_state(&self) -> RTCPeerConnectionState;
    fn ice_connection_state(&self) -> RTCIceConnectionState;
    fn ice_gathering_state(&self) -> RTCIceGatheringState;
    fn signaling_state(&self) -> RTCSignalingState;

    // ── 默认 / 可选 (8 methods) ──
    fn set_on_data_channel(&self, cb) {}     // 默认 no-op
    fn set_on_track(&self, cb) {}            // 默认 no-op — RealObserver 覆盖
    fn set_on_ice_candidate(&self, cb) {}    // 默认 no-op
    fn gather_complete(&self) -> Result { Ok(()) }
    fn get_stats(&self) -> Vec<RTCRtcStats> { vec![] }
    fn add_transceiver(&self, ...) -> Result { Err("not supported") }
    fn register_track(&self, id, kind) -> Result { Ok(()) }
}
```

### 当前覆盖状态

| 方法 | WebrtcSysPc | WebrtcRsPc | StubPc |
|------|:-----------:|:----------:|:------:|
| `create_offer` / `create_answer` | ✅ FFI | ✅ webrtc-rs | ✅ 空 SDP |
| `set_local_description` / `set_remote_description` | ✅ FFI | ✅ webrtc-rs | ✅ no-op |
| `add_ice_candidate` | ✅ FFI | ✅ webrtc-rs | ✅ no-op |
| `connection_state` 等状态方法 | ✅ | ✅ | ✅ |
| `close` | ✅ FFI | ✅ | ✅ |
| `set_on_track` | ✅ RealObserver (D173) | ❌ 默认 no-op | ❌ 默认 no-op |
| `set_on_data_channel` | ✅ RealObserver | ❌ 默认 no-op | ❌ 默认 no-op |
| `set_on_ice_candidate` | ✅ RealObserver | ❌ 默认 no-op | ❌ 默认 no-op |

更新: 2026-07-31 — webrtc-sys 后端通过 RealObserver 已实现三个回调 (D173)，覆盖率表修正。

## 帧发送链路

```
TrackSender::write_raw_i420(data, w, h)
  → TrackWriteBackend::write_raw_i420()
    → WebrtcSysTrack (webrtc-sys)
      → I420Buffer::data_y/data_u/data_v (raw pointer fill)
      → VideoFrameBuilder → set_video_frame_buffer → build()
      → VideoTrackSource::on_captured_frame(&frame, &metadata)
    → libwebrtc 内部编码 (VP8/H.264)
```

## 帧接收链路（Phase 0-1 实施中）

```
libwebrtc on_track 事件
  → RealObserver::on_track(transceiver)
    → transceiver.receiver().track()  → MediaStreamTrack
    → media_to_video(media_track)     → VideoTrack
    → new_native_video_sink(FrameSinkAdapter)
    → video_track.add_sink(&native_sink)
    → VideoSink::on_frame(VideoFrame) → FrameSinkAdapter
    → 用户 FrameSink::on_frame(&VideoFrame)
```

### FrameSink trait

对标 `audemsp-media::VideoSink<F>` 的回调模式：

```rust
pub trait FrameSink: Send + Sync {
    fn on_frame(&self, frame: &VideoFrame) -> Result<(), RTCError>;
    fn is_active(&self) -> bool { true }
}
```

用户可自行封装为 channel / stream：

```rust
struct ChannelSink { tx: mpsc::UnboundedSender<VideoFrame> }
impl FrameSink for ChannelSink {
    fn on_frame(&self, f: &VideoFrame) -> Result<(), RTCError> {
        self.tx.send(f.clone()).ok();
        Ok(())
    }
}
```

### RealObserver 替代 NoOpObserver

当前 `NoOpObserver`（webrtc_sys.rs:517-602）实现 `PeerConnectionObserver` 但 **全部 20+ 回调为空**。Phase 0-1 替换为 `RealObserver`：

```rust
struct RealObserver {
    on_track_cb: Mutex<Option<Box<dyn Fn(TrackReceiver) + Send + Sync + 'static>>>,
    on_data_channel_cb: Mutex<Option<Box<dyn Fn(RTCDataChannel) + Send + Sync + 'static>>>,
    on_ice_candidate_cb: Mutex<Option<Box<dyn Fn(String, i32, String) + Send + Sync + 'static>>>,
}
```

## 工厂入口

```rust
pub struct RTCEngine;
impl RTCEngine {
    pub fn create_factory() -> impl RTCPeerConnectionFactory;
}

pub trait RTCPeerConnectionFactory: Send + Sync {
    async fn create_peer_connection(&self, config: RTCConfiguration)
        -> Result<impl RTCPeerConnection>;
    fn create_video_track(&self, id: &str) -> TrackSender;
    fn create_audio_track(&self, id: &str, config: RTCAudioTrackConfig) -> TrackSender;
}
```

使用示例：

```rust
use audemsp_webrtc::{RTCEngine, RTCConfiguration};

let factory = RTCEngine::create_factory();
let pc = factory.create_peer_connection(RTCConfiguration::default()).await?;

pc.onTrack(|receiver| {
    println!("remote track: {:?}", receiver.kind);
});
```

## 文件结构

```
crates/audemsp-webrtc/
├── src/
│   ├── lib.rs              re-exports
│   ├── engine.rs           RTCEngine::create_factory()
│   ├── peer.rs             RTCPeerConnection trait + impl
│   ├── channel.rs          RTCDataChannel
│   ├── track.rs            TrackSender / TrackReceiver / FrameSink
│   ├── rtp.rs              RTCRtpSender / RTCRtpReceiver
│   ├── sdp.rs              RTCSessionDescription
│   ├── stats.rs            RTCRtcStats
│   ├── rtp_params.rs       RTP 参数类型
│   └── backend/
│       ├── mod.rs          PcBackend / DcBackend trait + type alias
│       ├── webrtc_sys.rs   WebrtcSysPc + RealObserver
│       ├── webrtc_rs.rs    WebrtcRsPc
│       └── stub.rs         StubPc
├── examples/
│   └── webrtc_loopback_egui.rs  P2P loopback demo
└── tests/
    └── w3c_api_tests.rs    W3C API 测试
```

## 关联文档

- [09. 传输架构与 trait 设计](09-transport-architecture.md) — `MediaTransport` trait（上层抽象）
- [10. 信令架构](10-signaling-architecture.md) — 信令层，调用本 crate 创建连接
- [14. Remote 架构](14-remote-architecture.md) — Remote Client/Host 使用本 crate
- [15. Component 架构](15-component-architecture.md) — Component 通过本 crate 通信
- [决策记录 D144–D151](../.agents/memorys/decisions.md) — 架构决策链

## 当前状态

| 能力 | 状态 |
|------|------|
| 发送端 I420 帧 → libwebrtc | ✅ 已实现 |
| SDP 协商 (createOffer/createAnswer) | ✅ 已实现 |
| ICE candidate 管理 | ✅ 已实现 |
| RTCDataChannel 创建与发送 | ✅ 已实现 |
| RTCDataChannel 事件接收 (spool) | ⚠️ webrtc-sys 为 stub |
| 接收端 onTrack 回调 | ✅ 已实现 (RealObserver, D173) |
| 接收端 FrameSink 帧接收 | ✅ 已实现 (VideoSink bridge, D173) |
| webrtc-rs 后端 | ⚠️ struct 已有，待适配 trait |
| str0m 后端 | 🔲 Phase 2+ |
| RTC 前缀重命名 | ✅ 已完成 (D170) |
| backends/ → backend/ 目录统一 | ✅ 已完成 (D171) |
| core → media 模块迁移 | ✅ 已完成 (D169) |
| RawPixelFormat → PixelFormat 合并 | ✅ 已完成 |
