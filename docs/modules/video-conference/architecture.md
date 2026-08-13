# 视频会议架构

## 1. 组件架构

```
┌──────────────────────────────────────────────────────────────────┐
│                        Client (Web/Native)                        │
│  ┌──────────┐  ┌──────────┐  ┌──────────────┐  ┌─────────────┐  │
│  │ Camera   │  │ Mic      │  │ Screen Share │  │ Audio       │  │
│  │ Capture  │  │ Capture  │  │ Capture      │  │ Playback    │  │
│  └────┬─────┘  └────┬─────┘  └──────┬───────┘  └──────┬──────┘  │
│       │ VP8/AV1     │ Opus          │ VP8/AV1          │ Opus     │
│       ▼              ▼               ▼                  ▼        │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  WebRTC Stack (Simulcast + SVC, transport-cc, TWCC)     │    │
│  └──────────────────────────┬───────────────────────────────┘    │
│                             │ SRTP/UDP                           │
└─────────────────────────────┼───────────────────────────────────┘
                              │
┌─────────────────────────────┼───────────────────────────────────┐
│                     MediaServo Backend                            │
│                              │                                    │
│  ┌──────────────────────────┴──────────────────────────────┐    │
│  │               Signaling (WebSocket + gRPC)               │    │
│  │  ┌─────────────┐  ┌──────────────┐  ┌────────────────┐  │    │
│  │  │ Room Mgr    │  │ Participant  │  │ SFU Selector   │  │    │
│  │  │ (create/    │  │ Mgr (join/   │  │ (worker alloc, │  │    │
│  │  │  destroy)   │  │  leave)      │  │  load balance) │  │    │
│  │  └─────────────┘  └──────────────┘  └────────────────┘  │    │
│  └──────────────────────────┬───────────────────────────────┘    │
│                             │                                     │
│  ┌──────────────────────────┴──────────────────────────────┐    │
│  │              Conference Controller (Rust)                │    │
│  │  ┌─────────────┐  ┌──────────────┐  ┌────────────────┐  │    │
│  │  │ Auth Guard  │  │ Permission   │  │ ICE/TURN Relay │  │    │
│  │  │ (token)     │  │ Check        │  │ (NAT traversal)│  │    │
│  │  └─────────────┘  └──────────────┘  └────────────────┘  │    │
│  └──────────────────────────┬───────────────────────────────┘    │
│                             │                                     │
│  ┌──────────────────────────┴──────────────────────────────┐    │
│  │              mediasoup SFU Engine                         │    │
│  │  ┌───────────┐  ┌───────────┐  ┌───────────┐           │    │
│  │  │ Worker 1  │  │ Worker 2  │  │ Worker N  │           │    │
│  │  │ Router A  │  │ Router B  │  │ Router C  │           │    │
│  │  │           │  │           │  │           │           │    │
│  │  │ Producer  │  │ Producer  │  │ Producer  │           │    │
│  │  │ Consumer  │  │ Consumer  │  │ Consumer  │           │    │
│  │  └───────────┘  └───────────┘  └───────────┘           │    │
│  └─────────────────────────────────────────────────────────┘    │
│                             │                                     │
│  ┌──────────────────────────┴──────────────────────────────┐    │
│  │              Recording / ASR Pipeline                    │    │
│  │  (RTP Forwarding: SFU -> external consumers via pipe)   │    │
│  └─────────────────────────────────────────────────────────┘    │
│                             │                                     │
│  ┌──────────────────────────┴──────────────────────────────┐    │
│  │  Redis: Room state, Pub/Sub, Node health, PSRPC        │    │
│  └─────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘
```

## 2. SFU 引擎

### 2.1 选型: mediasoup

mediasoup 作为核心 SFU 引擎，通过 napi-rs 绑定到 MediaServo native-core。

理由：
- C++ 实现，性能最优，进程级 Worker 隔离
- 信令不可知，可完全自定义控制面
- `pipeToRouter()` 级联支持同机/跨机分布式部署
- Rust FFI 友好，通过 napi-rs 或 FFI 封装为 Rust crate

### 2.2 Worker 模型

```
┌───────────────┐
│ Conference    │
│ Controller    │
│ (Rust)        │
│               │
│  ┌─────────┐  │
│  │ Worker  │  │  ← 每个 Worker 是独立 C++ 进程
│  │ Pool    │  │     进程隔离，崩溃不影响其他 Worker
│  │ Mgr     │  │
│  └────┬────┘  │
└───────┼───────┘
        │
  ┌─────┼─────┐
  │     │     │
  ▼     ▼     ▼
┌────┐ ┌────┐ ┌────┐
│ W1 │ │ W2 │ │ WN │  ← 独立进程，1 Worker ≈ 1 CPU 核
│ Rtr│ │ Rtr│ │ Rtr│
└───┬┘ └───┬┘ └───┬┘
    │      │      │
    └──────┴──────┘
            │ pipeToRouter() 跨 Worker / 跨主机
            ▼
      ┌──────────┐
      │ Remote   │
      │ Router   │
      └──────────┘
```

Worker 分配策略：
- 每个 Worker 绑定一个 CPU 核
- 新会议分配到当前负载最低的 Worker
- 大房间独占 Worker，小房间共享 Worker
- 默认 `auto` 策略: `ceil((nproc * 0.8) + (max(0, nproc - 32) / 2))`

### 2.3 Producer/Consumer 模型

mediasoup 使用发布/订阅模型。MediaServo 有两种 Producer 来源：

**Browser Producer（浏览器端发布者）**: 通过 getUserMedia 采集摄像头/麦克风，经 mediasoup-client (JS) 创建 Producer。

**Native Host Producer（原生推流端）**: mediaservo-host 通过 mediaservo-webrtc (webrtc-sys/libwebrtc) 建立 ICE/DTLS 连接，
经由 WebRtcTransport 创建 Producer 注入视频流。Host 不依赖浏览器，适用于车端/边缘设备原生推流。

```
Producer (发布者)            Consumer (订阅者)
┌──────────────────┐       ┌──────────────────┐
│ Router           │       │ Router           │
│  │               │       │  │               │
│  ▼               │       │  ▼               │
│ Producer         │       │ Consumer         │
│ • Browser (JS)   │       │ • Browser (JS)   │
│ • Host (native)  │       │ • RTP stream out │
│ - RTP stream in  │       │ - 层选择         │
│ - Simulcast/SVC  │       │ - 重传缓存       │
│ - 编码信息       │       │                  │
└──────────────────┘       └──────────────────┘

### 2.4 级联 (Cascading)

跨区域会议通过 `pipeToRouter()` 级联：

```
Zone A (Beijing)              Zone B (Singapore)
┌──────────────────┐         ┌──────────────────┐
│ Worker 1         │         │ Worker 2         │
│ Router A1        │◄────────│ Router B1        │
│ Participants     │ pipe    │ Participants     │
│  A, B, C         │         │  D, E, F         │
└──────────────────┘         └──────────────────┘
        │                            │
    ┌───────┐                   ┌───────┐
    │ Local │                   │ Local │
    │ SFU   │                   │ SFU   │
    └───────┘                   └───────┘
```

级联决策：
- 同区域参会者分配到同一 Router
- 跨区域通过 `pipeToRouter()` 级联
- 级联只转发当前活跃流（PLI/FIR 聚合控制）
- 避免全连接 mesh 模式（Jitsi 2020 教训）

## 3. 音频策略

### 3.1 设计选择

**SFU 转发 + 客户端混音为主，MCU 混音为补充。**

```
正常会议:
  SFU 转发所有参与者的 Opus 音频
  → 客户端解码多路音频并本地混音
  → 服务端不混音，不消耗 CPU

PSTN 桥接:
  SFU 转发 + FreeSWITCH MCU 混音
  → PSTN 用户只收一路混合音频
  → 录制时服务端可选混音

透明听音模式 (借鉴 BBB):
  → 静音用户自动挂断音频通道
  → 只保留活跃发言人的音频转发
  → 节约 SFU 资源
```

### 3.2 活跃发言人检测

借鉴 Google Meet 的虚拟媒体流模式：

- 服务端检测当前最响的 3 个发言人的音频
- 通过 RTP CSRC 字段标识活跃发言人变化
- 客户端持续监听固定数量音频流，CSRC 动态切换
- 无需客户端重协商

## 4. 编解码策略

### 4.1 视频编解码

| 编解码 | 角色 | 场景 |
|--------|------|------|
| VP8 | 基线 | 全平台兼容 |
| VP9 | SVC 主选 | 浏览器优先，VP9 SVC L3T3_KEY |
| H.264 | 硬件兼容 | 移动端、硬件编码器 |
| AV1 | 渐进部署 | 预热流量 + 可选主编码 |

### 4.2 Simulcast + SVC

- 默认使用 Simulcast (VP8/H.264)：3 层 (低/中/高)
- VP9 开启 SVC 模式：时域 + 空域分层
- 动态切换：人少 (<16) 用 SVC，人多 (>16) 切 Simulcast
- Dynacast (按需编码)：只编码有订阅者的层

### 4.3 带宽管理

- 客户端通过 transport-cc 和 TWCC 反馈带宽估计
- SFU 根据带宽自适应选择转发层
- 拥塞控制自适应：延迟优先网络用 delay-based，丢包优先网络用 loss-based
- 音频优先：丢包严重时保音频降视频

## 5. 录制方案

### 5.1 双层架构

```
实时录制:
  Producer → pipeToRouter() → Recording Consumer → 各轨独立存储

异步合成:
  会议结束后批量合成各轨为最终视频文件
  GPU/CPU 合成，支持多种布局
```

### 5.2 RTP Forwarding

借鉴 Janus RTP Forwarding 模式：

- SFU 将发布者的 RTP/RTCP 流复制一份
- 通过 pipe 推送到外部录制服务
- 录制服务非 WebRTC 端点，接收裸 RTP
- 支持多轨独立录制和合成录制

## 6. 安全

- SRTP + DTLS 加密媒体传输
- Token 认证机制 (JWT)
- 可选的端到端加密 (Insertable Streams)
- 权限控制：谁可以发言、共享屏幕、录制
- 国密加密支持（中国部署场景）

## 7. 参考架构对比

| 维度 | MediaServo VC | Zoom | Jitsi | LiveKit |
|------|-------------|------|-------|---------|
| SFU 引擎 | mediasoup (C++) | MMR (自研) | JVB (Java) | LiveKit (Go) |
| 信令 | 自研 WebSocket+gRPC | 自研 | XMPP+Colibri2 | WebSocket 自研 |
| 级联 | pipeToRouter | 骨干网级联 | Secure Octo | Redis Router |
| 音频 | SFU+客户端混音 | SFU | SFU+客户端混音 | SFU+客户端混音 |
| 录制 | RTP Forwarding | 原生 | Jibri | Egress |
| 部署 | 多种形态 | 不可自建 | 自托管 | 自托管+云 |

详见 [research doc](../../research/video-conference.md) 了解完整调研分析。