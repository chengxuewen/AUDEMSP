# 管道模型与 Trait 设计

> Phase 0 架构定义 — 详细 trait 设计和管道模型。参考 LVQR Unified Fragment Model + GStreamer 状态机 + OBS 回调模式。

---

## 一、核心类型（native-core — 零依赖）

### 1.1 公共 API 类型（SDK 层 — 类型安全）

```rust
// ── 原始视频帧（屏幕捕获产出，渲染器消费）────────────────

pub struct RawFrame {
    pub track_id: String,
    pub timing: FrameTiming,
    pub format: RawPixelFormat,        // NV12, I420, BGRA, P010
    pub planes: Vec<Plane>,
    pub texture: Option<TextureHandle>, // GPU 路径句柄
}

// ── 编码片段（编码器产出，协议适配器/录制器消费）──────────

pub struct EncodedFragment {
    pub track_id: String,
    pub timing: FrameTiming,
    pub flags: FragmentFlags,
    pub codec: CodecId,                // RFC 6381 字符串
    pub init_data: Option<Bytes>,      // SPS/PPS, vorbis 头
    pub payload: Bytes,                // NAL, fMP4 moof+mdat, Opus 包
}

pub struct FrameTiming {
    pub dts: u64,                      // 轨道 timebase 单位
    pub pts: u64,
    pub duration: u64,
    pub wall_clock: Option<Instant>,   // NTP 挂钟时间 (可选)
}

pub struct FragmentFlags {
    pub keyframe: bool,
    pub independent: bool,
    pub discardable: bool,             // D22：SFU 帧级 QoS + HLS 部分段
}

pub enum RawPixelFormat { Nv12, I420, Bgra, P010 }
```

### 1.2 内部管道类型（引擎层 — 统一路径）

```rust
/// 内部统一枚举 — 管道引擎的路由单位。
/// GStreamer 的 GstBuffer + GstMapInfo 模型。

pub enum InternalPacket {
    Encoded(EncodedFragment),
    Raw(RawFrame),
    Metadata(PacketMetadata),
}

pub struct PacketMetadata {
    pub track_id: String,
    pub event: MetadataEvent,
}

pub enum MetadataEvent {
    TrackStarted { codec: CodecId, timescale: u32 },
    TrackEnded,
    QualityChanged { target_bitrate: u32 },
}
```

### 1.3 TextureHandle 借用模型（D20）

```rust
pub struct TextureHandle { /* 内部 Arc<InnerTexture> */ }

impl TextureHandle {
    pub fn map_readable(&self) -> Result<TextureView>;
    pub fn map_writable(&mut self) -> Result<TextureView>;
}
// 多个消费者可并发 map_readable；map_writable 为独占
// 参考 GStreamer 的 GstBuffer + gst_buffer_ref
```

### 1.4 设计依据

| 类型 | 参考项目 | 原因 |
|------|---------|------|
| RawFrame + EncodedFragment | OBS (obs_source_frame + encoder_packet) | 类型安全：SDK 使用者不会混淆编码帧与原始帧 |
| InternalPacket | GStreamer (GstBuffer), LVQR (Fragment) | 统一管道路径：引擎内部路由不再重复代码 |
| FrameTiming (混合 NTP) | MediaMTX + GStreamer | 推送流场景精确唇音同步，屏幕捕获场景优雅回退 |
| TextureHandle (借用) | GStreamer (gst_buffer_ref) | 支持分叉管道（渲染+编码同时），支持写入路径 |

---

## 二、管道节点层次

### 2.1 节点基础 trait（D23）

```rust
/// 所有管道节点的基础。声明元数据和能力。

trait NodeInfo: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> NodeCapability;
}

/// 可运行节点的生命周期（GStreamer 的 change_state 模型，Phase 1 简化为两态）。

trait PipelineNode: NodeInfo {
    fn on_start(&mut self) -> Result<()>;   // 分配资源，激活 I/O
    fn on_stop(&mut self) -> Result<()>;    // 释放资源，停用 I/O
}

pub struct NodeCapability {
    pub input: FormatSpec,     // 输入格式需求
    pub output: FormatSpec,    // 输出格式声明
}

pub struct FormatSpec {
    pub media_type: MediaType,         // Encoded, Raw, Both
    pub codecs: Option<Vec<CodecId>>,   // None = 通配符
    pub pixel_formats: Vec<RawPixelFormat>,
}

pub enum MediaType { Encoded, Raw, Both }
```

### 2.2 三层管道角色（D24）

```rust
/// ── Producer: 产出媒体数据（摄像头、屏幕、网络源）──

trait MediaSource: PipelineNode {
    type Output;

    /// 轮询下一个数据单元。None = 暂无数据（非阻塞）。
    /// LVQR 的 FragmentStream::next_fragment 模式，适配 str0m 的 poll_output。

    fn poll_fragment(&mut self) -> Result<Option<Self::Output>>;
}

/// ── Processor: 变换媒体数据（编码、解码、色彩转换）──

trait MediaProcessor: PipelineNode {
    type Input;
    type Output;

    /// 同步变换。CPU/GPU 工作 — 可内部 spawn 到独立线程。
    /// GStreamer 的 BaseTransformImpl::transform 模式。

    fn process(&mut self, input: Self::Input) -> Result<Self::Output>;
}

/// ── Sink: 消费媒体数据（渲染、推流、录制）──

trait MediaSink: PipelineNode {
    type Input;

    /// 推送式消费。OBS 的 filter_video / encoder_packet 回调模式。

    fn on_fragment(&mut self, fragment: Self::Input) -> Result<()>;
}
```

### 2.3 async 模型（D27）

| 角色 | 模式 | 原因 |
|------|------|------|
| `poll_fragment` | 异步 | 等待外部数据（网络、摄像头）—— 使用 futures-based |
| `process` | 同步 | CPU/GPU 变换 —— 避免 async overhead 和 Pin<Box<Future>> 复杂性 |
| `on_fragment` | 同步 | 推送式消费 —— 无需 futures |

对于 CPU 密集型处的执行，节点内部可选使用 `tokio::spawn` 卸载到独立线程。

---

## 三、扇出机制（D25）

### 3.1 FragmentBroadcaster

```rust
/// 单生产者、多订阅者的广播器。
/// 基于 tokio::sync::broadcast（LVQR 模式）。

pub struct FragmentBroadcaster<P> {
    tx: broadcast::Sender<P>,
    meta: BroadcasterMeta,
}

impl<P: Clone + Send + 'static> FragmentBroadcaster<P> {
    /// 发布数据包。返回当前订阅者数量。生产者永不阻塞。
    pub fn emit(&self, packet: P) -> usize;

    /// 创建新订阅者。返回的流可作为 MediaSource 链入管道。
    pub fn subscribe(&self) -> BroadcastStream<P>;

    /// 获取广播器元数据。
    pub fn meta(&self) -> &BroadcasterMeta;
}

/// 订阅者流 — 实现 MediaSource trait。

impl<P> MediaSource for BroadcastStream<P> {
    type Output = P;

    fn poll_fragment(&mut self) -> Result<Option<P>> {
        // tokio::sync::broadcast::Receiver::recv()
        // RecvError::Lagged → 跳过 + 记录（LVQR 反压策略）
    }
}
```

**关键约束**：
- 默认缓冲区：1024 个数据包（~1-2 秒 60fps）
- 慢消费者收到 `RecvError::Lagged` 并跳过 — 发布者永不阻塞
- 订阅者是 MediaSource — 可链入管道链

**备注 — 反压策略**：Phase 1 采用简单 FIFO skip on overflow（广播通道满时跳过旧包）。Phase 2 引入从 encoder 到 capture source 的反压机制（D-OPS-03），当下游编码或传输速率不足时通知上游降帧或减少质量。
### 3.2 PipelineRegistry（D26）

```rust
/// 管道的中央注册表。管理动态源的添加/移除。

pub struct PipelineRegistry {
    broadcasters: RwLock<HashMap<(String, String), Arc<dyn AnyBroadcaster>>>,
    on_created: RwLock<Vec<Box<dyn Fn(&RegistryEvent) + Send + Sync>>>,
    on_removed: RwLock<Vec<Box<dyn Fn(&RegistryEvent) + Send + Sync>>>,
}

impl PipelineRegistry {
    /// 获取或创建广播器。生产者（RTMP 源、摄像头）调用。
    pub fn get_or_create(
        &self, source_id: &str, track_id: &str, meta: BroadcasterMeta,
    ) -> Arc<FragmentBroadcaster>;

    /// 移除广播器。生产者断开时调用。
    pub fn remove(&self, source_id: &str, track_id: &str);

    /// 生命周期钩子——在锁外触发（关键：消费者可在回调中安全订阅而不会死锁）。
    /// LVQR 的 FragmentBroadcasterRegistry::on_entry_created/removed 模式。

    pub fn on_source_added<F>(&self, cb: F)
        where F: Fn(&RegistryEvent) + Send + Sync + 'static;

    pub fn on_source_removed<F>(&self, cb: F)
        where F: Fn(&RegistryEvent) + Send + Sync + 'static;
}
```

---

## 四、管道组合示例

### 4.1 遥控座舱推流（Phase 1 里程碑）

```
┌─────────────────────────────────────────────────────────────┐
│  车辆端                                                      │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐  │
│  │ CameraCapture│───▶│ NvencEncoder │───▶│ WebRtcPush  │  │
│  │ MediaSource  │    │ MediaProc    │    │ MediaSink    │  │
│  │ <RawFrame>   │    │ Raw→Encoded  │    │ <EncodedFrag>│  │
│  └──────────────┘    └──────────────┘    └──────────────┘  │
│                                                             │
│  InternalPacket::Raw    InternalPacket::Encoded              │
│                                                             │
│  座舱端                                                      │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐  │
│  │ RtmpSubscribe│───▶│ VtDecoder    │───▶│ ScreenRender │  │
│  │ MediaSource  │    │ MediaProc    │    │ MediaSink    │  │
│  │<EncodedFrag> │    │ Encoded→Raw  │    │ <RawFrame>   │  │
│  └──────────────┘    └──────────────┘    └──────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 扇出示例：同时渲染 + 编码 + 录制

```
ScreenCapture ─── FragmentBroadcaster<RawFrame>
                      ├── ScreenRender (MediaSink<RawFrame>)
                      ├── NvencEncoder (MediaProcessor<RawFrame, EncodedFragment>)
                      │       └── RtmpPublisher (MediaSink<EncodedFragment>)
                      └── Recorder (MediaSink<RawFrame>)
```

### 4.3 类型边界

编码器和解码器是 InternalPacket 类型的转换节点：

| 节点 | 输入 | 输出 | 说明 |
|------|------|------|------|
| NvencEncoder | RawFrame | EncodedFragment | GPU 零拷贝编码 |
| VaapiEncoder | RawFrame | EncodedFragment | Linux 硬件编码 |
| VtDecoder | EncodedFragment | RawFrame | macOS 硬件解码 |
| SoftwareH264 | RawFrame | EncodedFragment | 软件编码（兼容性 fallback） |
| Passthrough | EncodedFragment | EncodedFragment | SFU 透传（零拷贝） |
| ColorConvert | RawFrame | RawFrame | 色彩空间转换 |

---

## 五、GStreamer + Rust 混合管线（D6, D19）

| 层次 | 技术 | 场景 |
|------|------|------|
| **协议适配器** | GStreamer (RTMP/RTSP/SRT 解析) | Phase 1 |
| **编解码** | GStreamer + Rust NVENC/VAAPI 桥接 | Phase 1 |
| **HLS/DASH 打包** | GStreamer + Rust 改进 | Phase 2+ |
| **WebRTC 核心** | str0m / libwebrtc / webrtc-rs | Phase 1 |
| **GPU 编码桥接** | libloading 直调 NVENC/VAAPI/VT | Phase 1 |
| **输入注入管道** | 纯 Rust | Phase 1 |

Phase 2+ 目标：将 GStreamer 降级为可插拔协议适配器，核心管线全 Rust。

---

## 六、参考项目映射

| AUDEMSP 概念 | LVQR | GStreamer | OBS | MediaMTX |
|---------------|------|-----------|-----|----------|
| InternalPacket | Fragment | GstBuffer | — | unit.Unit |
| FragmentBroadcaster | FragmentBroadcaster (broadcast::Sender) | tee element | — | — |
| PipelineRegistry | FragmentBroadcasterRegistry | — | obs_load_all_modules2 | path struct |
| MediaSource | FragmentStream | BaseSrc | obs_source_info (activated) | Source interface |
| MediaProcessor | Transcoder, Agent | BaseTransform | obs_source_info (filter_video) | — |
| MediaSink | — (observer pattern) | BaseSink | obs_output_info | Reader interface |
| PluginCapability | — (header fields) | PadTemplate + Caps | output_flags | — |
