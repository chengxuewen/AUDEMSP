# 11. NAPI Binding — Node.js API

> 版本: 0.1 | 日期: 2026-07-16 | Phase: 0

## 11.1 设计原则

- **Session 抽象**：仅导出高级 Session API，不暴露 Rust trait 层次
- **EventEmitter**：Node.js 原生模式，零额外依赖
- **解码后传递**：Rust 侧 GStreamer decode → RGBA Buffer → napi-rs 拷贝
- **零代码生成**：手动编写 napi-rs 绑定，不依赖宏

## 11.2 架构

```
宿主平台 (Node.js)
│
├─ @mediaservo/napi
│   ├─ Session          ← napi-rs 导出类
│   │   ├─ EventEmitter
│   │   ├─ control.send()
│   │   └─ getStats()
│   │
│   └─ types.d.ts       ← TypeScript 类型声明
│
│  napi-rs FFI
│
▼
mediaservo-common (Rust)
├─ SessionManager        ← 新建 thin wrapper
├─ MediaTransport        ← 现有
├─ PluginManager         ← 现有
└─ pipeline              ← 现有
```

## 11.3 Session API

### 创建

```typescript
import { createSession, SessionType, SessionRole } from '@mediaservo/napi';

const session = createSession({
  type: 'teleop_cockpit',      // SessionType
  role: 'cockpit',             // SessionRole
  transport: 'libwebrtc',      // TransportBackend: 'str0m' | 'libwebrtc'

  signaling: {
    url: 'wss://signal.example.com/ws',
    token: '<JWT>',
    reconnect: {
      maxAttempts: 5,
      intervalMs: 2000,
    },
  },

  video: {
    codec: 'vp8',              // 'h264' | 'vp8' | 'vp9' | 'av1'
    width: 1280,
    height: 720,
    fps: 30,
    bitrateKbps: 2000,
  },

  audio: {
    codec: 'opus',             // Phase 1 only opus
    sampleRate: 48000,
    channels: 2,
    bitrateKbps: 64,
  },

  dataChannel: {
    labels: ['control', 'telemetry'],
    ordered: {
      'control': true,         // 控制指令确保顺序
      'telemetry': false,      // 遥测允许丢包
    },
  },
});
```

### 类型

```typescript
type SessionType =
  | 'remote_desktop'    // 远程桌面
  | 'teleop_cockpit'    // 遥控座舱
  | 'teleop_vehicle';   // 遥控车辆

> Phase 1 SessionType range: remote_desktop, teleop_cockpit, teleop_vehicle。剩余 4 种能力（视频会议、推拉流、监控相机、video-conference）为 Phase 2+ 扩展。
type SessionRole =
  | 'host'              // 被控端（桌面共享方）
  | 'client'            // 控制端（远程桌面客户端）
  | 'cockpit'           // 座舱（接收视频+发送控制）
  | 'vehicle';          // 车辆端（发送视频+接收控制）

type SessionState =
  | 'idle'              // 未连接
  | 'connecting'        // 正在建立连接
  | 'connected'         // 已连接
  | 'reconnecting'      // 断线重连中
  | 'disconnected'      // 已断开
  | 'failed';           // 连接失败
```

### 生命周期

```typescript
// 启动连接（阻塞至 ICE completed 或失败）
await session.connect();

// 等待特定状态（用于测试）
await session.waitFor('connected', { timeoutMs: 30000 });

// 优雅关闭
session.disconnect();
```

### 事件

```typescript
interface VideoFrame {
  timestamp: bigint;            // 微秒
  width: number;
  height: number;
  format: 'rgba';              // Phase 1 only RGBA
  data: Buffer;                 // width × height × 4 bytes
}

interface AudioFrame {
  timestamp: bigint;
  sampleRate: number;          // 48000
  channels: number;            // 1 | 2
  format: 'pcm_s16le';        // PCM 16-bit signed little-endian
  data: Buffer;
}

interface DataChannelMessage {
  label: string;               // 'control' | 'telemetry'
  data: Buffer;
}

interface SessionStats {
  rttMs: number;
  packetLossPercent: number;
  bitrateKbps: number;
  framerate: number;
  encodeMs: number;            // 编码延迟
  decodeMs: number;            // 解码延迟
  jitterMs: number;
}

interface SessionError {
  code: string;                // 'SIGNALING_FAILED' | 'ICE_FAILED' | 'DATACHANNEL_ERROR'
  message: string;
  recoverable: boolean;
}

// 注册回调
session.on('state_change', (state: SessionState) => {});
session.on('remote_video', (frame: VideoFrame) => {});
session.on('remote_audio', (frame: AudioFrame) => {});
session.on('data_channel', (msg: DataChannelMessage) => {});
session.on('stats', (stats: SessionStats) => {});      // 每秒推送
session.on('error', (err: SessionError) => {});
```

### 控制

```typescript
// 发送控制指令（RTCDataChannel）
session.control.send(Buffer.from(JSON.stringify({ steering: 15 })));

// 主动拉取统计
const stats: SessionStats = session.getStats();

// 请求关键帧（弱网重传后）
session.control.requestKeyframe();
```

## 11.4 Rust 侧实现

```rust
// mediaservo-common/src/session.rs (新建)

use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi(object)]
pub struct SessionConfig {
  pub session_type: String,
  pub role: String,
  pub transport: String,
  pub signaling_url: String,
  pub token: String,
  pub video_codec: String,
  pub video_width: u32,
  pub video_height: u32,
  pub video_fps: u32,
  pub video_bitrate_kbps: u32,
  pub data_channel_labels: Vec<String>,
}

#[napi]
pub struct Session {
  inner: Mutex<SessionInner>,
}

#[napi]
impl Session {
  #[napi(constructor)]
  pub fn new(config: SessionConfig) -> Result<Self> {
    // 1. 根据 transport 选择 MediaTransport 后端
    // 2. 创建 SignalingTransport（WebSocket）
    // 3. 创建 pipeline (decode → format convert → callback)
    Ok(Self { inner: Mutex::new(SessionInner::new(config)?) })
  }

  #[napi]
  pub async fn connect(&self) -> Result<()> {
    // 1. WS connect
    // 2. SDP offer/answer via SignalHandler
    // 3. ICE negotiation
    // 4. 启动 pipeline
    // 5. emit('state_change', 'connected')
    Ok(())
  }

  #[napi]
  pub fn disconnect(&self) -> Result<()> { /* ... */ }
}
```

## 11.5 视频帧管线（napi-rs 路径）

```
Rust: GStreamer pipeline
  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
  │ appsrc   │ →  │ h264parse│ →  │ decodebin│ →  │ appsink  │
  │ (RTP)    │    │          │    │ (HW dec) │    │ (RGBA)   │
  └──────────┘    └──────────┘    └──────────┘    └────┬─────┘
                                                        │
  appsink callback: GstBuffer → RGBA bytes              │
  napi-rs: Buffer::from(rgba_bytes)                     │
                                                        ▼
  Node.js: emit('remote_video', { data: Buffer })
```

Phase 2 优化：DMA-BUF 共享（Linux only）
- Rust 输出 DMA-BUF fd → napi-rs 传递 fd → Node.js 侧通过 addon 导入 GPU 纹理
- 避免 ~8MB/frame RGBA 拷贝

## 11.6 内存与性能

| 场景 | 分辨率 | FPS | RGBA 带宽 | 拷贝开销 |
|------|--------|-----|----------|---------|
| 远程桌面 | 1920×1080 | 30 | ~250 MB/s | 5-10ms (memcpy) |
| 遥控座舱 | 1280×720 | 30 | ~110 MB/s | 2-4ms |
| 遥控座舱 | 1280×720 | 15 | ~55 MB/s | 1-2ms |
| 视频会议 | 640×480 | 15 | ~18 MB/s | <1ms |

Phase 1 遥控座舱场景 15fps，55MB/s 拷贝开销可接受。
`memcpy` 在现代 CPU 上 8MB RGBA 帧约 2-4ms（DDR4 带宽 ~20GB/s）。

## 11.7 与宿主平台集成

```typescript
// 宿主平台插件内部使用 MediaServo Session

// plugins/teleop-cockpit/src/index.ts
import { createSession } from '@mediaservo/napi';

export class TeleopCockpitPlugin implements Plugin {
  private session: Session;

  async onLoad(ctx: PluginContext) {
    this.session = createSession({
      type: 'teleop_cockpit',
      role: 'cockpit',
      transport: 'libwebrtc',
      signaling: { url: ctx.config.signalingUrl, token: ctx.auth.token },
      video: { codec: 'vp8', width: 1280, height: 720, fps: 30, bitrateKbps: 2000 },
    });

    this.session.on('remote_video', (frame) => {
      // 渲染到浏览器 Canvas / WebGL
      ctx.ui.renderVideo(this.videoElementId, frame);
    });

    this.session.on('data_channel', (msg) => {
      if (msg.label === 'telemetry') {
        ctx.emit('telemetry_update', parseTelemetry(msg.data));
      }
    });

    await this.session.connect();
  }

  // 宿主平台 UI 可通过 PluginHost 调用
  sendControl(steering: number, throttle: number) {
    this.session.control.send(Buffer.from(JSON.stringify({ steering, throttle })));
  }
}
```

## 11.8 Phase 1 vs Phase 2

| | Phase 1 | Phase 2 |
|---|---------|---------|
| SessionType | remote_desktop, teleop_cockpit, teleop_vehicle | + video_conference, live_streaming, surveillance_viewer |
| 视频格式 | RGBA Buffer | + DMA-BUF fd (Linux) |
| 音频格式 | PCM s16le | + Opus pass-through |
| 事件 | 6 种基础事件 | + track_added/removed (SFU) |
| 控制 | control.send() | + subscribeTrack() (SFU) |
