# SDK 接口契约（link / field / client / deck）

> **状态**: 设计定稿（未实现）| **日期**: 2026-08-14
> **关联**: D233（API 形态=单层会话型）、D222-D232（四 SDK 架构）、04-sdk-layers.md、api-interface-design 技能
> **边界**: 本文是四 SDK 的**公开 Rust 接口契约** + C ABI 绑定形态。内部协商遵循 mediasoup 标准 offer/answer（C18，反 PIT-65）。

---

## 0. 已定决策（契约依据）

| 决策 | 内容 | 出处 |
|---|---|---|
| **API 形态** | 单层会话型（`PushSession`/`PullSession`），不暴露 mediasoup 细粒度对象；高级需求走富 Options；底层控制走 mediaservo-webrtc 逃生舱 | D233 |
| **事件模型** | Rust = enum + channel/Stream；C/C++ = 回调函数指针 | D234 |
| **品牌化 ID** | `RoomId/PeerId/TrackId/SessionId/StreamId/NodeId/DeviceId` 包装类型 | D234 |
| **错误模型** | 每 SDK 一个 thiserror enum + `#[non_exhaustive]` | D234 |
| **选项风格** | 纯 struct + `Default`（`..Default::default()`），不用 builder | D234 |
| **帧注入** | 保留 `write_raw_i420_with_ts` 为注入点；deck source 对象产帧喂入 | D234 |
| **async** | Rust 核心全 async（tokio），返回 `Result<T, XxxError>` | D234 |

## 1. 统一调用约定

| 约定 | 规则 |
|---|---|
| async | 所有 I/O/协商方法 `async fn`；本地状态切换（pause/resume）同步 |
| 错误 | `Result<T, XxxError>`；库用 thiserror；错误 enum `#[non_exhaustive]` |
| 参数 | `&str` 优于 `String`；`&[T]` 优于 `Vec<T>`；ID 用品牌化类型 |
| 选项 | 纯 struct + `Default`；可选字段 `Option<T>` |
| 事件 | Rust 返回 `UnboundedReceiver<Event>`（或 `Stream`）；C/C++ 注册回调 |
| 所有权 | Rust `Arc` 内部共享；会话/源对象 owned；C++ RAII handle |
| 兼容 | **只加法**：新 variant 追加 enum 末尾、新字段 `Option<T>`、新方法默认实现；禁改/删既有签名 |

## 2. 共享类型（mediaservo-common / 约定）

```rust
// 品牌化 ID（防串参）—— #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomId(String);
pub struct PeerId(String);
pub struct TrackId(String);
pub struct SessionId(String);
pub struct StreamId(String);
pub struct NodeId(String);
pub struct DeviceId(String);

// 帧（复用 mediaservo-media）
pub struct FrameRef;                 // I420/NV12/RGBA + 宽高 + 时间戳（monotonic/epoch 双时钟）
pub type FrameStream = UnboundedReceiver<FrameRef>;

// 认证（复用 common auth）
#[non_exhaustive]
pub enum AuthCredential { Psk(PskCredential), Jwt(JwtCredential) }

// 信令消息（复用 common protocol.rs 的 SignalingMessage，serde tag=type）
```

---

## 3. link — 连接面

> **设备侧 IPC 专题**（FrameBus 总线 / Registry 注册 / ACL 权限 / 能力令牌）**已实现**（iceoryx2），见 [21-link-ipc.md](/docs/modules/21-link-ipc.md)。本节为 link 对 server 的信令（SignalClient）与共享类型。

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LinkError {
    #[error("attach failed: {0}")]          Attach(String),
    #[error("acl denied: {topic}")]         AclDenied { topic: String },   // D237
    #[error("topic has publisher: {topic}")] TopicConflict { topic: String }, // D239
    #[error("token invalid: {0}")]          Token(String),                  // D238
    #[error("registry error: {0}")]         Registry(String),
    #[error("bus error: {0}")]              Bus(String),
    #[error("closed")]                      Closed,
}

// ── 信令客户端（WS，复用 common SignalingMessage）────────────
pub struct SignalClient;
impl SignalClient {
    pub async fn connect(url: &str, auth: AuthCredential) -> Result<SignalSession, LinkError>;
}

pub struct SignalSession;                       // Arc 内部
impl SignalSession {
    pub fn events(&self) -> UnboundedReceiver<SignalEvent>;
    pub async fn send(&self, msg: SignalingMessage) -> Result<(), LinkError>;
    pub async fn request(&self, msg: SignalingMessage) -> Result<SignalingMessage, LinkError>; // req/resp
    pub fn room_id(&self) -> RoomId;
    pub async fn close(self) -> Result<(), LinkError>;
}

#[non_exhaustive]
pub enum SignalEvent {
    Connected { room_id: RoomId },
    Message(SignalingMessage),
    Disconnected { reason: DisconnectReason },
    Error(LinkError),
}

// ── 帧总线（跨进程 SHM，iceoryx2，D242/D239/D235）────────
// buffer_size=1 + enable_safe_overflow → latest-frame；max_publishers(1) → 单发布者
pub struct FrameBus;
impl FrameBus {
    pub fn attach(endpoint: &str, token: &CapabilityToken, vk: &Ed25519VerifyingKey)
        -> Result<FrameBus, LinkError>;                       // 验签→载ACL→register
    pub fn publish(&self, topic: &FrameTopic, payload: &[u8], meta: &FrameMeta)
        -> Result<(), LinkError>;                              // ACL+单发布者→send
    pub fn subscribe(&self, topic: &FrameTopic) -> Result<FrameStream, LinkError>;
    pub fn close(self) -> Result<(), LinkError>;
}
pub struct FrameTopic(String);
pub struct FrameMeta { seq, width, height, format, version, is_keyframe, ts_mono_ns, ts_epoch_ns } // 定长 LE, D243

// ── 节点注册/发现（iceoryx2 内建活性, 无 daemon）──────────
pub struct Registry;
impl Registry {
    pub fn register(info: &NodeInfo) -> Result<(), LinkError>;
    pub fn discover_topics(prefix: &str) -> Result<Vec<TopicInfo>, LinkError>;
    pub fn discover_nodes(role: Role) -> Result<Vec<NodeInfo>, LinkError>;
    pub fn topic_publisher(topic: &FrameTopic) -> Result<Option<NodeId>, LinkError>;
}

// ── DataChannel（Phase 2，webrtc-rs）─────────────────────
// pub struct ControlChannel;  // Phase2 定稿
```

---

## 4. field — 组合 SDK（会话 facade，唯一公开层）

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FieldError {
    #[error("link: {0}")]      Link(#[from] LinkError),
    #[error("webrtc: {0}")]    WebRtc(String),
    #[error("codec: {0}")]     Codec(String),
    #[error("track {0}: {1}")] Track(TrackId, String),
    #[error("closed")]         Closed,
}

// ── 推流会话（采集→编码→推流，信令内建）────────────────────
pub struct PushSession;
impl PushSession {
    pub async fn connect(cfg: PushConfig) -> Result<(PushSession, SessionEvents), FieldError>;
    pub async fn publish_video(&self, src: VideoSourceRef, opts: PublishOptions) -> Result<TrackId, FieldError>;
    pub async fn publish_audio(&self, src: AudioSourceRef, opts: PublishOptions) -> Result<TrackId, FieldError>;
    pub async fn unpublish(&self, track: TrackId) -> Result<(), FieldError>;
    pub fn control_channel(&self) -> Result<ControlChannel, FieldError>;  // 控制/遥测（DC 或 relay）
    pub async fn stats(&self) -> Result<SessionStats, FieldError>;
    pub async fn close(self) -> Result<(), FieldError>;
}

// ── 拉流会话（订阅→解码→出帧）────────────────────────────
pub struct PullSession;
impl PullSession {
    pub async fn connect(cfg: PullConfig) -> Result<(PullSession, SessionEvents), FieldError>;
    pub async fn subscribe(&self, producer: ProducerRef, opts: SubscribeOptions) -> Result<FrameStream, FieldError>;
    pub async fn unsubscribe(&self, track: TrackId) -> Result<(), FieldError>;
    pub fn control_channel(&self) -> Result<ControlChannel, FieldError>;  // 发送控制
    pub async fn close(self) -> Result<(), FieldError>;
}

pub type SessionEvents = UnboundedReceiver<SessionEvent>;
#[non_exhaustive]
pub enum SessionEvent {
    Connected,
    StateChanged(SessionState),
    TrackPublished { track: TrackId },
    TrackSubscribed { track: TrackId },
    ProducerAvailable { producer: ProducerRef },     // 供 subscribe
    Disconnected { reason: DisconnectReason },
    Error(FieldError),
}

// ── 配置 / 富 Options（承接高级需求，D233）────────────────
pub struct PushConfig {
    pub url: String,
    pub auth: AuthCredential,
    pub room: RoomId,
    pub role: PeerRole,                              // Host/Pusher
}
pub struct PullConfig {
    pub url: String,
    pub auth: AuthCredential,
    pub room: RoomId,
    pub role: PeerRole,                              // Client/Puller
    pub auto_subscribe: bool,
}
impl Default for PullConfig { /* auto_subscribe = true */ }

pub struct PublishOptions {
    pub codec: VideoCodec,                           // VP8/H264/VP9/AV1
    pub encoder_backend: EncoderBackend,             // Auto/Software/Hardware
    pub encoding: Option<VideoEncoding>,             // bitrate/max_framerate/resolution
    pub simulcast: bool,
    pub degradation: Option<DegradationPreference>,
}
impl Default for PublishOptions { /* codec=VP8, backend=Auto, simulcast=false */ }

pub struct SubscribeOptions { pub priority: u8, pub max_resolution: Option<Resolution> }
impl Default for SubscribeOptions { /* ... */ }

// ── 组合 re-export（一行依赖闭环）────────────────────────
pub use mediaservo_link::{SignalClient, FrameBus, AuthCredential};
pub use mediaservo_deck::{MediaDevices, CameraSource, AudioSource, ScreenSource, VideoSource};
```

> **逃生舱**：需底层控制（SDP/transceiver/RTP 细粒度）者直接依赖 `mediaservo-webrtc`（RTCPeerConnection/TrackSender/PeerConnectionApi），field 不 re-expose。

---

## 5. client — 舱端消费编排

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ClientError {
    #[error("field: {0}")]   Field(#[from] FieldError),
    #[error("render: {0}")]  Render(String),
    #[error("playback: {0}")] Playback(String),
}

// ── 渲染器（D47: CPU buffer → GPU interop）────────────────
pub struct Renderer;
impl Renderer {
    pub fn new(surface: Surface, opts: RenderOptions) -> Result<Renderer, ClientError>;
    pub fn render(&self, frame: FrameRef) -> Result<(), ClientError>;
    pub fn set_surface(&self, surface: Surface) -> Result<(), ClientError>;
}
pub struct RenderOptions { pub backend: RenderBackend /* Auto/Cpu/GpuDirect */, pub zero_copy: bool }
impl Default for RenderOptions { /* backend=Auto */ }

// ── 多路编排：复用 field::PullSession（每路一个）+ Renderer ──
// 舱端 App = N × PullSession.subscribe() → FrameStream → Renderer.render()

// ── 回放 / 快放（deck playback 集成）─────────────────────
pub struct Playback;
impl Playback {
    pub fn open(path: &str) -> Result<Playback, ClientError>;
    pub fn frames(&self) -> Result<FrameStream, ClientError>;
    pub fn play(&self) -> Result<(), ClientError>;
    pub fn pause(&self) -> Result<(), ClientError>;
    pub fn seek(&self, ts_us: i64) -> Result<(), ClientError>;
    pub fn set_rate(&self, rate: f32) -> Result<(), ClientError>;   // 快放/慢放
}
```

---

## 6. deck — 媒体数据面

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DeckError {
    #[error("device: {0}")]   Device(String),
    #[error("codec: {0}")]    Codec(String),
    #[error("io: {0}")]       Io(#[from] std::io::Error),
    #[error("not_found: {0}")] NotFound(String),
}

// ── 采集（source 域，GStreamer 后端）────────────────────
pub struct MediaDevices;
impl MediaDevices {
    pub fn enumerate(kind: DeviceKind) -> Vec<DeviceId>;   // Camera/Audio/Screen
}

pub struct CameraSource;
impl CameraSource {
    pub fn open(dev: DeviceId, opts: CaptureOptions) -> Result<CameraSource, DeckError>;
    pub fn frames(&self) -> Result<FrameStream, DeckError>;
    pub fn start(&self) -> Result<(), DeckError>;
    pub fn stop(&self) -> Result<(), DeckError>;
}
pub struct AudioSource;   // 同型：open/frames/start/stop
pub struct ScreenSource;  // 同型

pub struct CaptureOptions { pub resolution: Option<Resolution>, pub framerate: Option<u32>, pub format: Option<PixelFormat> }
impl Default for CaptureOptions { /* 设备默认 */ }

// VideoSource：帧注入抽象（field publish_video 的入参类型）
pub struct VideoSource;   // CameraSource/文件/自定义帧源 的统一句柄
// 产帧内部经 mediaservo-webrtc TrackSender::write_raw_i420_with_ts 注入

// ── 录制（record 域，FFmpeg mux）────────────────────────
pub struct Recorder;
impl Recorder {
    pub fn new(path: &str, opts: RecordOptions) -> Result<Recorder, DeckError>;
    pub fn record(&self, stream: FrameStream) -> Result<(), DeckError>;   // 或 record_topic(FrameTopic)
    pub fn stop(self) -> Result<(), DeckError>;
}
pub struct RecordOptions { pub codec: VideoCodec, pub container: Container /* Mp4/Mkv */, pub keyframe_align: bool }

// ── 回放（playback 域，demux + decode）─────────────────
pub struct Player;
impl Player {
    pub fn open(path: &str) -> Result<Player, DeckError>;
    pub fn frames(&self) -> Result<FrameStream, DeckError>;
    pub fn seek(&self, ts_us: i64) -> Result<(), DeckError>;
    pub fn set_rate(&self, rate: f32) -> Result<(), DeckError>;
}

// ── codec（复用 mediaservo-codec 引擎，facade 不吞并）──
pub use mediaservo_codec::{Encoder, Decoder};   // D229: deck 依赖 codec，不合并
```

---

## 7. C ABI 绑定形态（c / cxx / py）

延续 D109：**opaque handle + int 错误码 + 回调**。每 SDK 一套 `ms_<sdk>_*` 前缀。

```c
/* 示例：link 信令 */
typedef struct ms_link_signal_t ms_link_signal_t;   /* opaque */
typedef int ms_err_t;                                /* 0 = ok, <0 = error code */

ms_err_t ms_link_signal_connect(const char* url, const ms_auth_t* auth, ms_link_signal_t** out);
ms_err_t ms_link_signal_send(ms_link_signal_t* s, const uint8_t* msg, size_t len);
void     ms_link_signal_on_event(ms_link_signal_t* s, ms_signal_event_cb cb, void* user);  /* 事件回调 */
ms_err_t ms_link_signal_close(ms_link_signal_t* s);
ms_err_t ms_last_error(char* buf, size_t len);       /* 最近错误详情 */

/* field 推流（async → handle + 完成回调，或阻塞式，绑定层二选一） */
typedef struct ms_field_push_t ms_field_push_t;
ms_err_t ms_field_push_connect(const ms_push_config_t* cfg, ms_field_push_t** out);
ms_err_t ms_field_push_publish_video(ms_field_push_t* s, ms_video_source_t* src,
                                     const ms_publish_options_t* opts, ms_track_id_t* out_track);
```

- **C++（`-cxx`）**：header-only RAII 包装 C ABI（`FfiHandle` 式析构 + `Result<T,E>`）
- **Python（`-py`）**：首版 ctypes 加载 cdylib（LiveKit 模式）；瓶颈时 pyo3 加速后端（D227 两步走）
- **事件**：C 侧回调函数指针 + user data；async 操作用 handle + 完成回调
- **帧**：跨 FFI 传帧用指针 + 元数据 struct（零拷贝优先），避免序列化

### 7.1 交付形态（D240）
**单动态库（.so/cdylib）为主**，不预建静态 .a（嵌入式自包含需求出现再加，additive）。
- 交付物：`link.so` / `field.so`（打包 link+deck）/ `client.so` / `deck.so`（+ `deck-full.so` OTA 插件）
- C++（-cxx）：header-only RAII over `.so`；Python（-py）：ctypes 加载 `.so`；Rust 内部走 rlib

### 7.2 版本与 C ABI 稳定（D241）
- **soname**：`libmediaservo_<sdk>.so.<MAJOR>`（如 `libmediaservo_field.so.1`）；实体 `.so.<MAJOR>.<MINOR>.<PATCH>`；开发 symlink 无版本号
- **MAJOR = C ABI 版本**：破坏性 ABI 变更才 bump MAJOR；MINOR/PATCH = additive/fix
- **within MAJOR 稳定承诺**：只加法（既有签名/语义不变）+ opaque handle 隐藏内部布局 + 仅 C 兼容类型；需演进的结构用 version/size 字段或 `_v2` 函数
- **兼容规则**：同 MAJOR 二进制兼容（换 .so 免重链）；跨 MAJOR 需重链 + 迁移指南
- **cbindgen 纪律**：只导出稳定 C ABI；Rust 内部 API 可 semver 演进，C ABI 面 within MAJOR 稳定

## 8. 向后兼容纪律（Hyrum's Law）

| 规则 | 落实 |
|---|---|
| 公开 enum 全 `#[non_exhaustive]` | `SignalEvent/SessionEvent/*Error` |
| 新 variant 追加末尾、新字段 `Option<T>` | 禁改/删既有 variant/字段 |
| 新方法优先默认实现 | trait 演进不破坏实现方 |
| ID 品牌化 | 防 `RoomId`↔`TrackId` 串参 |
| E2E 固化行为 | 消息顺序/重试间隔等可观察行为进 E2E 脚本 |
| 检查 | `cargo doc --no-deps`、serde roundtrip 测试、`cargo tree -d` 空 |
