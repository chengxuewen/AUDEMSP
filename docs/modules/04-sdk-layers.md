# SDK 分层与 API 参考

> AUDEMSP SDK 采用分层架构，基于 audemsp-common 微内核构建领域特定 SDK。
> 关联决策: D65-D69 (SDK命名+Facade), D82 (crate清单)

---

## 架构总览

Phase 1 MVP 核心：双 SDK facade 模型

```
                    ┌─────────────────┐
                    │  audemsp-common │  ← 微内核（所有场景共享）
                    │  PluginManager  │
                    │  LicenseManager │
                    │  PipelineEngine │
                    └────────┬────────┘
                             │
        ┌────────────────────┼────────────────────┐
        ▼                                         ▼
┌───────────────────────┐            ┌───────────────────────┐
│ audemsp-field        │            │ audemsp-client       │
│ (车端 SDK)             │◄─ WebRTC ─►│ (座舱 SDK)             │
├───────────────────────┤            ├───────────────────────┤
│ CameraCapture (D64)    │            │ VideoDecode (D46)      │
│ HardwareEncode (D43)   │            │ VideoRender (D47)      │
│ WebRTC Push (D11)      │            │ WebRTC Pull (D11)      │
│ RTCDataChannel (D65)      │            │ RTCDataChannel (D66)      │
│ MQTT Telemetry (D74)   │            │ Input Forward (D18)    │
└───────────────────────┘            └───────────────────────┘
        │                                         │
        ▼                                         ▼
┌───────────────┐                      ┌───────────────┐
│ field-c (C FFI)│                    │ remote-c (.a) │
│ .a + .so (D83)│                    │ FFmpeg (D70)  │
└───────────────┘                      └───────────────┘
```

Phase 2+: audemsp-streaming, audemsp-conference, audemsp-surveillance

---

## 一、audemsp-common — 微内核

所有场景共享的核心引擎，提供：

| 组件 | 职责 |
|------|------|
| **PluginManager** | 插件注册、生命周期管理 |
| **LicenseManager** | 权限校验、配额控制 |
| **PipelineEngine** | 媒体管线调度 |
| **ProtocolBroker** | 内部协议路由（FlatBuffers） |

### 核心 Trait

```rust
#[async_trait]
trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> (u16, u16, u16);
    fn category(&self) -> PluginCategory;
    async fn init(&mut self, ctx: &PluginContext) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
}

#[async_trait]
trait MediaSource: Plugin {
    fn output_port(&self) -> &MediaPort;
    async fn start(&mut self) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
}

#[async_trait]
trait MediaProcessor: Plugin {
    fn input_port(&self) -> &MediaPort;
    fn output_port(&self) -> &MediaPort;
    async fn process(&mut self, frame: MediaFrame) -> Result<MediaFrame>;
}

#[async_trait]
trait MediaSink: Plugin {
    fn input_port(&self) -> &MediaPort;
    async fn consume(&mut self, frame: MediaFrame) -> Result<()>;
}
```

---

## 二、Phase 1 MVP SDK

### 2.1 audemsp-field（车端 SDK）

facade crate，单一入口 re-export 所有车端能力 (D69)

| 能力 | 说明 |
|------|------|
| **CameraCapture** | V4L2/AVFoundation/Jetson CSI (D64) |
| **WebRTC Push** | libwebrtc 弱网编码推流 (D11) |
| **RTCDataChannel** | 控制指令双向通道 (D65) |
| **MQTT Telemetry** | 车端状态上报 (D74) |
| **C FFI** | audemsp-field-c: .a + .so 静态+动态库 (D79, D83) |

### 2.2 audemsp-client（座舱 SDK）

facade crate，单一入口 re-export 所有座舱能力 (D69)

| 能力 | 说明 |
|------|------|
| **WebRTC Pull** | libwebrtc 拉流解码 (D72) |
| **FFmpeg Decode** | str0m 后端备选 (D70-D71) |
| **RTCDataChannel** | 控制指令发送 (D66) |
| **VideoRender** | Phase 1 CPU buffer 渲染 (D47) |
| **C FFI** | audemsp-client-c: .a 静态链接 FFmpeg (D70) |

### 2.3 audemsp-client（GUI 应用）

Tauri v2 + React 桌面应用 (D76)

| 能力 | 说明 |
|------|------|
| **远程桌面** | 屏幕查看 + 输入注入 |
| **遥控座舱** | 视频拉流 + 控制发送 |
| **视频会议** | 信令 + 媒体流 |
| **权限管理** | 动态模块加载 |

---

## 三、Cargo Workspace 结构 (D82)

**Phase 2+ 目标结构 (D82)**: 当前 Phase 3 完成, workspace 为 7 crate (audemsp-host/client/server/common/webrtc/media/codec)，详见 `Cargo.toml`。

```
crates/
├── audemsp-common/          微内核 (PluginManager, PipelineEngine)
├── audemsp-transport/     传输层 (RTP/SRT/WebRTC, D31-D33)
├── audemsp-signaling/     信令客户端 (D51-D54)
├── audemsp-codec/         编解码 (D43-D48)
├── audemsp-auth/          认证 (D4, D57)
├── audemsp-pipeline/      管线 (D23-D27)
├── audemsp-field/          车端 SDK facade (D65, D67, D69)
├── audemsp-field-c/        车端 C 绑定 .a+.so (D64, D79, D83)
├── audemsp-client/         座舱 SDK facade (D66, D68-D69)
├── audemsp-client-c/       座舱 C 绑定 .a+FFmpeg (D70-D72, D79, D83)
└── audemsp-napi/           Node.js 绑定 (D55-D56)

binaries/
├── audemsp-host/           Host 应用 (D62-D63, D73)
├── audemsp-client/         Client 应用 (D76)
└── audemsp-server/         后台管理服务 (D86-D91)
```
