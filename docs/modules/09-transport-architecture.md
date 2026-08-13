# 传输架构与 trait 设计



> Phase 0 架构定义 — MediaTransport trait、双后端 + 测试桩分发、sans-I/O 运行循环。参考 webrtc-kit 的 trait 抽象 + str0m 的 sans-I/O 设计。默认后端 webrtc-sys（libwebrtc C++ FFI 包装）。str0m 推迟至 Phase 2+（Embed 场景）。

**命名约定**: 本文档描述 sans-I/O 内部传输层，方法使用 Rust snake_case 惯例（如 create_offer）。对外 mediaservo-webrtc W3C API 层使用 camelCase（如 createOffer），RTC 前缀类型名（如 RTCPeerConnection）。详见 docs/modules/17-webrtc-crate.md。


---

## 一、三后端架构（D11）

```
┌─────────────────────────────────────────────────────────────┐
│                  MediaTransport trait (sans-I/O)             │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  backend-webrtc-sys  (默认)   backend-webrtc-rs   stub      │
│                                                             │
│  C++ libwebrtc FFI             tokio async          测试桩   │
│  ~1M+ LOC                      ~80K LOC             无依赖   │
│  完整 W3C                      完整 W3C (D27)       no-op    │
│  cmake + Corrosion             纯 Rust              编译用   │
│                                                             │
│  str0m → Phase 2+ (Embed/LAN, sans-I/O 原生, ~30K LOC, 零外部依赖) │
└─────────────────────────────────────────────────────────────┘
```

| 后端 | 编译特性 | 场景 | Phase |
|------|---------|------|-------|
| libwebrtc | `backend-webrtc-sys` (默认) | 公网遥控, 弱网穿透 | Phase 0 |
| webrtc-rs | `backend-webrtc-rs` | 未来 Embed 升级 | Phase 0 (struct 骨架) |
| stub | 无 feature 时 | 开发/测试/编译检查 | Phase 0 |
| str0m | `backend-str0m` | AUDESYS Embed, 局域网 P2P | Phase 2+ |

---

## 二、MediaTransport trait（D31）

### 2.1 工厂 trait

```rust
/// 传输工厂（webrtc-kit 的 RTCPeerConnectionFactory 模式）。

pub trait MediaTransportFactory: Send + Sync {
    fn backend_name(&self) -> &str;
    fn create_transport(
        &self,
        config: &TransportConfig,
        callbacks: Box<dyn TransportCallbacks + Send>,
    ) -> Result<Box<dyn MediaTransport>>;
}

pub trait TransportCallbacks {
    fn on_ice_candidate(&self, candidate: &RTCIceCandidate);
    fn on_connection_state_change(&self, state: ConnectionState);
    fn on_track_added(&self, track: &MediaTrackInfo);
    fn on_data_channel_open(&self, label: &str);
    fn on_data_channel_message(&self, label: &str, data: &[u8]);
}
```

### 2.2 传输 trait（sans-I/O 优先）

```rust
/// 传输实例。参考 str0m 的 Rtc 状态机 + webrtc-rs 的 RTCPeerConnection。
/// sans-I/O 优先 — Embed 场景无需 tokio。

pub trait MediaTransport: Send {
    // ── 网络 I/O (sans-I/O 模式) ──

    /// 喂入网络数据包。
    fn handle_input(&mut self, data: &[u8]) -> Result<()>;

    /// 排空输出。可能返回待发送数据、事件、超时或空。
    fn poll_output(&mut self) -> Result<TransportOutput>;

    // ── SDP 协商 ──

    fn create_offer(&self) -> Result<RTCSessionDescription>;
    fn create_answer(&self) -> Result<RTCSessionDescription>;
    fn set_local_description(&self, sd: &RTCSessionDescription) -> Result<()>;
    fn set_remote_description(&self, sd: &RTCSessionDescription) -> Result<()>;

    // ── ICE ──

    fn add_ice_candidate(&self, candidate: &RTCIceCandidate) -> Result<()>;

    // ── 媒体轨道 ──

    fn create_video_track(&self, config: &VideoTrackConfig)
        -> Result<Box<dyn MediaTrack>>;
    fn create_data_channel(&self, label: &str)
        -> Result<Box<dyn DataChannelHandle>>;
    fn get_video_track(&self, id: &str)
        -> Option<Box<dyn MediaTrack>>;

    // ── 生命周期 ──

    fn close(&self);
    fn is_connected(&self) -> bool;
}
```

### 2.3 sans-I/O 输出枚举

```rust
/// str0m 的 Output 变体。所有后端统一返回此枚举。

pub enum TransportOutput {
    Transmit(Vec<u8>),            // 需发送的数据包
    Event(TransportEvent),        // 状态变更事件
    Timeout(Duration),            // 下次 poll_output 调用的等待时间
    Nothing,                      // 无待处理
}

pub enum TransportEvent {
    Connected,
    Disconnected,
    IceStateChanged(RTCIceConnectionState),
    TrackAdded(MediaTrackInfo),
    TrackData(MediaData),
    ChannelOpen(String),
    ChannelData(String, Vec<u8>),
    Stats(TransportStats),
}
```

### 2.4 轨道和数据通道 trait

```rust
pub trait MediaTrack: Send {
    fn id(&self) -> &str;
    fn kind(&self) -> TrackKind;
    fn write(&mut self, frame: &[u8]) -> Result<()>;  // 写入编码帧 → TrackLocal::write_frame → libwebrtc RTP/SRTP 打包
    fn close(&self);
}

pub trait DataChannelHandle: Send {
    fn label(&self) -> &str;
    fn send_text(&self, text: &str) -> Result<()>;
    fn send_bytes(&self, data: &[u8]) -> Result<()>;
    fn close(&self);
}

pub enum TrackKind { Video, Audio }
```

### 2.5 RTP 轨道管线 (webrtc-sys 后端)

当使用 webrtc-sys (libwebrtc) 后端时，视频帧的推送路径：

```
编码器 (GStreamer/HW encoder)
  │ H.264/H.265 encoded frame (Annex B / AVCC)
  ▼
TrackLocal (MediaTrack::write)
  │ 将编码帧封装为 webrtc::VideoFrame
  ▼
libwebrtc VideoTrackSource::write_frame()
  │ RTP 打包 (packetizer) + SRTP 加密
  ▼
网络 (ICE/DTLS 传输)
```

**关键边界**: GStreamer 产出的字节边界在 `TrackLocal::write()` 处被消费，此后的 RTP 封装和加密完全由 libwebrtc 内部处理。用户代码只需提供编码帧的 `&[u8]`。

### 2.6 RTCDataChannel 降级说明 (D155)

在 webrtc-sys 后端中，RTCDataChannel 降级为**仅控制信令**用途：
- 键盘/鼠标/手柄输入事件
- 剪贴板同步
- 文件传输通过 HTTP multipart（非 DC）

原因是 libwebrtc 的 SCTP RTCDataChannel 在弱网下重传策略不够灵活，对大块数据传输体验不佳。Phase 1 遥控座舱中，DC 仅承载 `<1KB` 的控制命令帧。

### 2.7 GStreamer→WebRTC 字节边界 (D155)

```
┌───────────────────────────────────────────────────┐
│              GStreamer Pipeline                    │
│  appsrc → videoscale → x264enc → appsink          │
│                               ↓ emit-signals=true  │
└───────────────────────────────────────────────────┘
                                │
                H.264 Annex B byte stream (编码帧)
                                │
                                ▼
┌───────────────────────────────────────────────────┐
│           MediaServo Transport Layer                 │
│  MediaTrack::write(&[u8])                         │
│  → webrtc-sys: TrackLocal::write_frame()          │
│  → libwebrtc: RTP packetizer + SRTP + DTLS        │
└───────────────────────────────────────────────────┘
```

**合约**: GStreamer pipeline 产生完整的编码帧 (NAL unit 边界正确)。Transport 层不重新分片编码帧 — 编码帧的 NAL unit 边界由 GStreamer 保证，libwebrtc 的 RTP packetizer 处理 MTU 分片。

---

---

## 三、后端编译期分发（D32）

### 3.1 cfg dispatch（webrtc-kit 模式）

```rust
// mediaservo-transport/src/engine.rs

pub fn create_factory(
    config: &TransportConfig,
) -> Option<Box<dyn MediaTransportFactory>> {
    // str0m: Phase 2+ — see section 一 for backend table
    #[cfg(feature = "backend-str0m")]
    { return Some(Box::new(str0m_backend::Str0mFactory::new())); }

    #[cfg(feature = "backend-webrtc-sys")
    { return Some(Box::new(libwebrtc_backend::LibWebRtcFactory::new(config)?)); }

    #[cfg(feature = "backend-webrtc-rs")]
    { return Some(Box::new(webrtc_rs_backend::WebRtcRsFactory::new())); }

    None
}
```

### 3.2 互斥 guard

```rust
// 参考 webrtc-kit 的 compile_error! 模式

#[cfg(any(
    // str0m guards (Phase 2+):
    // all(feature = "backend-str0m", feature = "backend-webrtc-sys"),
    // all(feature = "backend-str0m", feature = "backend-webrtc-rs"),
    all(feature = "backend-webrtc-sys", feature = "backend-webrtc-rs"),
))]
compile_error!("Only one WebRTC backend can be enabled at a time");
```

### 3.3 后端对比

| 维度 | str0m | libwebrtc (via webrtc-sys) | webrtc-rs |
|------|-------|---------------------------|-----------|
| 设计哲学 | sans-I/O 状态机 | C++ FFI 包装 (webrtc-sys crate) | tokio async |
| W3C API 兼容 | 不兼容（故意） | 完全兼容 | 兼容 (v0.20+) |
| 运行时需求 | 无 | libwebrtc.so + webrtc-sys FFI | tokio |
| 编译复杂度 | cargo build | cmake + Corrosion 交叉编译 | cargo build |
| 二进制大小 | ~100KB | ~30MB (.so) | ~2MB |
| GCC 拥塞控制 | ❌ 自建 | ✅ 完整的 Google GCC | ❌ 自建 |
| FEC + NetEQ | ❌ 自建 | ✅ 完整的 | ❌ 自建 |
| 适用场景 | LAN P2P, AUDESYS Embed | 公网, 弱网, 遥控 (默认) | 未来 AUDESYS Embed |

---

## 四、sans-I/O 运行循环（D33）

### 4.1 合约（str0m 强制）

```
每次 mutation（handle_input, write, set_local_description, add_ice_candidate）后，
在下次 mutation 之前，必须完全 drain poll_output 直到返回 Nothing 或 Timeout。
```

### 4.2 标准循环模式

```rust
impl App {
    fn run_loop(&mut self, transport: &mut dyn MediaTransport, socket: &UdpSocket) {
        loop {
            // Step 1: 从输入启动
            if let Some(data) = socket.try_recv()? {
                transport.handle_input(&data)?;
                // mutation 发生 → 必须 drain
            }

            // Step 2: 排空输出
            self.drain_output(transport, socket)?;

            // Step 3: 写入媒体（如可用）
            if let Some(frame) = self.camera.next_frame() {
                if let Some(track) = transport.get_video_track("video-0") {
                    track.write(&frame)?;
                    // mutation 发生 → 必须 drain
                    self.drain_output(transport, socket)?;
                }
            }

            // Step 4: 等待下一个事件
            // TransportOutput::Timeout → sleep 对应时间
            // TransportOutput::Nothing → 继续循环
        }
    }

    fn drain_output(
        &mut self, transport: &mut dyn MediaTransport, socket: &UdpSocket,
    ) -> Result<()> {
        loop {
            match transport.poll_output()? {
                TransportOutput::Transmit(data) => socket.send(&data)?,
                TransportOutput::Event(e) => self.handle_event(e),
                TransportOutput::Timeout(t) => {
                    self.next_wake = Instant::now() + t;
                    break;
                }
                TransportOutput::Nothing => break,
            }
        }
        Ok(())
    }
}
```

### 4.3 为什么此合约如此重要

| 违反时的后果 | 示例 |
|-------------|------|
| ICE state 事件丢失 | 连接建立但应用不知道 → 静默失败 |
| 输出数据包在缓冲区中积累 | 视频的 DTLS 握手完成，但客户端未发送 |
| 重入问题 | handle_input 产生事件，poll_output 未调用，下一个 handle_input 造成状态不一致 |
| 所有后端的统一行为 | 即使 libwebrtc 内部使用回调线程，poll_output 也按合约 drain |

---

## 五、配置结构

```rust
pub struct TransportConfig {
    pub ice_servers: Vec<RTCIceServer>,
    pub ice_transport_policy: IcePolicy,     // All, Relay
    pub bundle_policy: BundlePolicy,         // Balanced, MaxBundle
    pub rtcp_mux_policy: RtcpMuxPolicy,
    pub ice_candidate_pool_size: u8,
    pub cert_type: CertificateType,          // ECDSA, RSA

    // 后端特定配置
    pub backend_config: BackendConfig,
}

pub enum BackendConfig {
    Str0m {
        mtu: usize,
        ice_lite: bool,               // 信令服务器可以设置此选项
        rtp_mode: bool,               // RTP 级访问而非帧级
    },
    LibWebRtc {
        field_trials: String,          // WebRTC field trials string
        threads: usize,                // libwebrtc 工作线程数
        disable_encryption: bool,      // 仅用于测试
    },
    WebRtcRs {
        ice_lite: bool,
        ice_udp_mux_port: Option<u16>,
    },
}

pub struct VideoTrackConfig {
    pub id: String,
    pub direction: RtpDirection,         // SendOnly, RecvOnly, SendRecv
    pub codecs: Vec<CodecConfig>,        // 优先级排序
}

pub struct CodecConfig {
    pub mime_type: String,               // "video/H264", "video/VP9"
    pub clock_rate: u32,                 // 90000
    pub fmtp_params: HashMap<String, String>,  // "packetization-mode=1"
}
```

---

## 六、场景矩阵

| 场景 | 后端 | 传输模式 | TURN | 运行时 |
|------|------|---------|------|--------|
| AUDESYS Embed (LAN) | str0m | P2P | 无需 | 无 (sans-I/O) |
| AUDESYS Embed (WAN) | libwebrtc (Phase 2) | P2P + relay | coturn | libwebrtc.so |
| AUDEBase Sidecar | libwebrtc (默认) | P2P + relay | coturn | libwebrtc.so + gstreamer |
| Standalone 服务器 | libwebrtc + mediasoup SFU | SFU | coturn/srt | libwebrtc.so + tokio |
| Web Viewer | 浏览器 RTCPeerConnection | P2P/SFU (native) | coturn | 浏览器内置 |

---

## 七、参考项目映射

| MediaServo 概念 | webrtc-kit | str0m | mediasoup-rust |
|---------------|-----------|-------|----------------|
| MediaTransportFactory | RTCPeerConnectionFactory | — | Router |
| MediaTransport | RTCPeerConnection | Rtc | Transport trait |
| handle_input | — (callback-based) | handle_input | — (channel-based) |
| poll_output | — | poll_output | — (async) |
| TransportOutput | — (event callbacks) | Output enum | — (async) |
| MediaTrack | VideoTrack trait | Writer handle | Producer/Consumer |
| 后端分发 | RTCEngine::create_factory (cfg) | 无 (单一实现) | 无 (C++ worker) |
| sans-I/O 合约 | 无 (回调驱动) | 核心设计原则 | 无 (channel IPC) |
