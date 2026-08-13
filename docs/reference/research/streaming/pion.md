# Pion — Pure Go WebRTC 生态参考文档

> 最后更新: 2026-07-19 | 版本: pion/webrtc v4.2.16 | GitHub: https://github.com/pion/webrtc

---

## 1. 产品画像

| 维度 | 信息 |
|------|------|
| **项目名称** | Pion — Pure Go implementation of the WebRTC API |
| **核心仓库** | `pion/webrtc` (16,633 ★, 1,867 forks, 240+ contributors) |
| **许可证** | MIT License |
| **语言** | Go (99.8%), 零 Cgo、零 C 依赖 |
| **当前版本** | v4.2.16 (2026-06-29) |
| **作者/维护者** | Sean DuBois (@Sean-Der), 现任职 OpenAI |
| **定位** | 纯 Go 实现的 W3C WebRTC 规范——服务器端、嵌入式、IoT、媒体服务器的 WebRTC 基础设施 |
| **上级生态** | `pion/org` 下属 50+ 子仓库, `ionorg/` 下属 ion 集群和 ion-sfu |

### Pion 生态核心仓库矩阵

```
┌─────────────────────────────────────────────────────────────────┐
│                    Pion WebRTC 生态全景                          │
├─────────────────────────────────────────────────────────────────┤
│  传输层        pion/ice (RFC 8445)    pion/turn (RFC 5766/8656) │
│  安全层        pion/dtls (DTLS 1.2)   pion/srtp (SRTP)          │
│  数据层        pion/sctp (SCTP)       pion/datachannel           │
│  媒体层        pion/rtp, pion/rtcp    pion/interceptor (插件管道)│
│  应用层        pion/webrtc (RTCPeerConnection API)                  │
│  SFU           pion/ion-sfu (纯 Go SFU)                          │
│  分布式集群    pion/ion (biz + ISLB + NATS/etcd/redis)           │
│  音视频处理    pion/ion-avp (write-to-disk, ffmpeg, OpenCV)      │
│  客户端 SDK    ion-sdk-js, ion-sdk-flutter, ion-app-web          │
│  上层产品      LiveKit (19.8K ★) ← OpenAI ChatGPT Advanced Voice │
│  测试工具      pion/webrtc-bench, pion/rtsp-bench                │
│  编码器桥接    libgowebrtc (Pion-compatible libwebrtc wrapper)   │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. 技术特性

### 2.1 纯 Go 无 Cgo — 核心架构差异

Pion 与 libwebrtc (Google Chrome 的 C++ 实现) 的根本区别:

| 对比维度 | Pion (Go) | libwebrtc (C++) |
|----------|-----------|-----------------|
| **语言** | 纯 Go | C++ |
| **构建** | `go build`, <1 秒 | 复杂 CMake/GN, 分钟级 |
| **依赖** | 零外部 C 库 | OpenSSL, libvpx, 多个系统库 |
| **跨平台** | Go 能跑就能跑 (含 WASM, iOS, Android) | 各平台需单独编译 |
| **并发模型** | Goroutine (轻量, 数千连接无压力) | 多线程 (线程开销大) |
| **部署** | 单二进制 | 动态库依赖 |
| **包体积** | ~10MB | 数十 MB 起 |
| **性能** | 服务器场景 15-25x 优于 libwebrtc SFU | 浏览器内编码优化更好 |

### 2.2 WebRTC 规范实现

Pion 严格遵循 W3C 规范:

- **webrtc-pc**: RTCPeerConnection API 完整实现
- **webrtc-stats**: 统计 API
- **Plan-B 和 Unified Plan**: 双 SDP 语义支持
- **Simulcast**: 多层视频编码
- **SVC**: VP9 可伸缩视频编码
- **数据通道**: 有序/无序, 可靠/不可靠, 基于 SCTP
- **WASM 支持**: 可在浏览器中运行

### 2.3 编解码器支持

| 类型 | 编解码器 | 说明 |
|------|----------|------|
| 音频 | Opus, PCM (PCMU/PCMA) | 默认 Opus |
| 视频 | H.264, VP8, VP9, AV1 | 所有主流编解码器 |
| FEC | FlexFEC | v4.2.0 新增前向纠错 |
| 容器 | IVF, Ogg, H.264 Annex-B, Matroska | 读写磁盘 |

### 2.4 网络传输栈

```
应用层 RTP/RTCP
  ↕ Interceptor Pipeline (可插拔包处理器)
传输层 SRTP (加密媒体) / SCTP (数据通道)
  ↕ DTLS 1.2 (加密握手)
连接层 ICE Agent (RFC 8445)
  ├── STUN (NAT 穿透)
  ├── TURN (UDP, TCP, DTLS, TLS 中继)
  ├── mDNS candidates (局域网发现)
  ├── Trickle ICE (渐进式候选)
  └── ICE Restart / ICE Renomination (网络漫游)
```

**加密套件**:
- DTLS: `TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256`
- SRTP: `SRTP_AEAD_AES_256_GCM`, `SRTP_AES128_CM_HMAC_SHA1_80`
- GCM 套件支持硬件加速

### 2.5 SettingEngine — 高级配置

Pion 独有的 `SettingEngine` 提供浏览器 API 之外的细粒度控制:

```go
se := webrtc.SettingEngine{}
// 单端口多路复用 (SFU 必备)
se.SetICEUDPMux(webrtc.NewICEUDPMux(nil, udpListener))
// 自定义缓冲工厂
se.BufferFactory = customBufferFactory
// ICE 超时配置
se.SetICETimeouts(disconnectedTimeout, failedTimeout, keepaliveInterval)
// NAT 1:1 IP 映射
se.SetNAT1To1IPs(publicIPs, webrtc.ICECandidateTypeHost)
// 禁用 mDNS
se.SetICEMulticastDNSMode(ice.MulticastDNSModeDisabled)
// ICE Lite 模式
se.SetLite(true)
```

### 2.6 Interceptor 管道 — 可插拔 RTP/RTCP 处理器

Interceptor 是 Pion 最核心的架构创新之一，允许以插件方式组合媒体处理逻辑:

**内置 Interceptor**:
| Interceptor | 功能 |
|-------------|------|
| `nack` | NACK 生成器和响应器, 丢包重传 |
| `twcc` | Transport-Wide Congestion Control |
| `gcc` | Google Congestion Control |
| `report` | Sender/Receiver Reports |
| `stats` | webrtc-stats 兼容统计 |
| `jitterbuffer` | 乱序重排和缓冲 |
| `flexfec` | FlexFEC-03 前向纠错 |
| `rfc8888` | RFC 8888 拥塞控制反馈 |
| `intervalpli` | 定时 PLI 生成 |
| `packetdump` | 包转储调试 |

**管道执行顺序** (出站 → 入站):

```
出站: App → [FlexFEC→NACK→TWCC→Stats→Reports→NACKGen] → Network
入站: Network → [NACKGen→Reports→Stats→TWCC→NACK→FlexFEC] → App
```

**链式组合**:

```go
chain := interceptor.NewChain([]interceptor.Interceptor{
    &StatsInterceptor{},
    &LoggingInterceptor{},
    &RecordingInterceptor{},
})
// 可与 webrtc.NewAPI(webrtc.WithInterceptorRegistry(registry)) 集成
```

---

## 3. 关键能力

### 3.1 RTCPeerConnection 完整功能

- **SDP 生成/解析**: CreateOffer/CreateAnswer/SetLocalDescription/SetRemoteDescription
- **Track 管理**: AddTrack/RemoveTrack 动态操作
- **数据通道**: CreateDataChannel, 支持 ondatachannel 事件
- **ICE 管理**: 完整的 ICE Agent, Trickle ICE, 候选收集和添加
- **重新协商**: 运行时 AddTrack/RemoveTrack
- **状态机**: RTCSignalingState, ICEConnectionState, RTCPeerConnectionState 完整状态回调
- **单端口部署**: ICETransport 单端口多路复用, 适用于 Docker/K8s

### 3.2 ion-sfu (Selective Forwarding Unit)

`ionorg/ion-sfu` (v1.11.0) — 纯 Go 实现的高性能 SFU:

**核心能力**:
- Audio/Video/RTCDataChannel 选择性转发
- 拥塞控制: TWCC (Transport-Wide CC), REMB, RR/SR
- Unified Plan 语义
- Pub/Sub 模式 RTCPeerConnection (O(n) 端口使用)
- RFC 6464 音频级别指示 ("X is speaking")
- gRPC 和 JSON-RPC 双信令接口

**架构设计**:

```
SFU
 ├── Session (房间概念)
 │    ├── Publisher RTCPeerConnection (上行)
 │    │    └── Router (按 trackID 路由)
 │    │         ├── Receiver (接收端包装)
 │    │         └── DownTrack[] (下行轨)
 │    ├── Subscriber RTCPeerConnection[] (下行)
 │    │    └── DownTrack (WriteRTP)
 │    └── RTCDataChannel (中间件管道, HTTP 风格)
 │         └── KeepAlive, SubscriberAPI 等中间件
 ├── WebRTCTransportConfig (全局配置)
 └── TURN Server (可选内置)
```

**关键配置**:

```go
type Config struct {
    SFU    struct {
        Ballast   int64  // 内存镇流器 (MB)
        WithStats bool   // 启用统计
    }
    WebRTC WebRTCConfig   // ICE, NAT, 端口范围
    Router RouterConfig    // 最大带宽, 包缓冲, 音频级别, Simulcast
    Turn   TurnConfig      // 可选内置 TURN
}
```

**Router 核心逻辑**:
1. `AddReceiver` — 接收发布者 track, 创建 buffer, 设置 TWCC/音频级别
2. `AddDownTrack` — 为订阅者创建下行轨, 调用 `pc.AddTransceiverFromTrack`
3. `sendRTCP` — goroutine 循环转发 RTCP 包

### 3.3 ion (分布式 RTC 系统)

`ionorg/ion` — 全功能视频会议系统:

```
┌──────────────────────────────────────────┐
│              ion 集群架构                  │
├──────────────────────────────────────────┤
│  biz (业务服务)  ←→ ISLB (负载均衡)       │
│    ↕ NatsRPC          ↕                   │
│  NATS + etcd + redis (消息/状态/缓存)     │
│    ↕                                      │
│  ion-sfu × N (SFU 节点, 每节点一个房间)    │
│    ↕                                      │
│  ion-avp (音视频处理 sidecar)              │
│    ↕                                      │
│  Web App / Flutter App (多端客户端)        │
└──────────────────────────────────────────┘
```

**组件**:
- **biz**: 处理房间成员、文字聊天、JWT 认证
- **ISLB**: 将客户端分配到合适的 SFU 节点 (多数据中心)
- **ion-sfu**: 核心 SFU, 通过 NatsRPC 与 biz/ISLB 通信
- **ion-avp**: 音视频处理 sidecar, 支持 write-to-disk, ffmpeg, OpenCV

**多端 SDK**: JS SDK (`ion-sdk-js`), Flutter SDK (`ion-sdk-flutter`)

> ⚠️ **项目状态**: ion 于 2023 年后停止活跃更新 (NOV24 状态更新建议直接使用 ion-sfu), 当前推荐使用 LiveKit 作为"开箱即用"方案。

### 3.4 WHIP/WHEP 标准化信令

Pion v4 原生支持 WHIP (WebRTC-HTTP Ingestion Protocol) 和 WHEP (WebRTC-HTTP Egress Protocol):

- OBS 可通过 WHIP 直接推流到 Pion 服务
- 浏览器可通过 WHEP 拉流
- 无需手动 SDP 交换

### 3.5 与 libgowebrtc 的编解码桥接

`libgowebrtc` 为 Pion 提供 libwebrtc 原生编码器性能:

| 编解码器 | 编码耗时 (720p, M2 Pro) | 模式 |
|----------|------------------------|------|
| H.264 | ~1.14ms/frame (OpenH264) | purego (无需 CGO) |
| VP8 | ~3.08ms/frame | libvpx |
| VP9 | ~3.21ms/frame | libvpx |
| AV1 | ~1.88ms/frame | libaom |

- 实现 `webrtc.TrackLocal` 接口, 无缝集成 Pion
- purego 模式 FFI 开销 ~200ns/call; 可选 CGO 模式 ~44ns/call
- 支持硬件加速 (macOS VideoToolbox for H.264)
- SVC/Simulcast 完整支持

---

## 4. 部署与运维

### 4.1 基准性能数据

**构建性能** (Intel i5-2520M @ 2.50GHz):
- 构建 `examples/play-from-disk`: **0.28 秒**
- 完整测试套件: **77 秒**

**并发连接** (已验证):

| 场景 | 硬件 | 连接数 | CPU 占用 |
|------|------|--------|----------|
| RTSP 重分发 | m4.2xlarge (8 vCPU) | 15,000 | ~25% |
| 1:1 SFU 场景 | 未公开 | ~30,000 PeerConnections | 未公开 |
| AI Voice Gateway | 8 vCPU | ~1,200 并发 (p95) | 未公开 |
| ion-sfu 单房间 | n1-standard-2 | 20-30 发布者 | ~90% |
| ion-sfu 单房间 | n2-standard-4 | 50 发布者 | ~70% |

**连接建立延迟** (同 VPC):

| 阶段 | 服务端延迟 | 客户端延迟 |
|------|-----------|-----------|
| Signaling Processing | ~13ms | — |
| SDP Offer Processing | ~6.6ms | — |
| SDP Answer Creation | ~0.37ms | — |
| ICE Gathering | ~0.26ms | ~5.0s |
| ICE Connection | ~123ms | ~5.2s |
| DTLS Handshake | ~154ms | ~104ms |
| Media Ready | — | ~5.3s |
| Signaling RTT | — | ~72ms |

### 4.2 部署模式

**单二进制部署**:
```bash
go build -o my-webrtc-app .
./my-webrtc-app
```

**Docker 部署**:
- Pion 应用做成的 Docker 镜像体积小 (FROM scratch 可行)
- 建议暴露 UDP 端口范围 (如 5000-5200)
- 使用 SettingEngine 单端口模式更利于容器化

**Kubernetes**:
- ion 提供完整 K8s 部署配置
- LiveKit 推荐 Helm Chart 或 Docker 部署

### 4.3 运维监控

**内置指标能力**:
- `pion/interceptor/pkg/stats` — webrtc-stats 兼容统计
- `pion/webrtc-bench` — CPU 使用率、连接数、延迟指标 CSV 导出
- `ion-sfu` 内置 stats 模块 (WithStats 配置启用)
- 推荐集成 Prometheus + Grafana

**关键监控指标**:
- ICE Connection State (new/checking/connected/disconnected/failed)
- RTT (Round-Trip Time)
- Jitter (抖动)
- Lost Packets (丢包率)
- Per-call bitrate

### 4.4 运维注意事项

- **文件描述符限制**: 大量连接需调高 `ulimit -n` (Pion #481 报告 10 连接即触发 `too many open files`)
- **端口管理**: 多房间建议使用单端口 ICE mux 模式
- **TURN 服务器**: 对称 NAT 环境必备, Pion 内置 TURN 服务器
- **内存管理**: `ion-sfu` 使用 Ballast (内存镇流器) 减少 GC 压力
- **CPU 优化**: 每核心约可承载 400-500 条媒体流; Simulcast + 360p 可有效降低负载

### 4.5 生产级用户

| 用户 | 场景 | 规模 |
|------|------|------|
| **LiveKit** | 实时通信平台 | 19.8K ★, 300K+ 开发者, 5,000+ 企业 |
| **OpenAI** | ChatGPT Advanced Voice | 数百万日活用户 |
| **CallSphere** | AI Voice Gateway | 37 个生产 Agent, 6 条垂直线 |
| **各种私有 SFU** | 视频会议、监控、IoT | 数千连接/实例 |

---

## 5. 生态与市场

### 5.1 社区活跃度

| 指标 | 数据 |
|------|------|
| `pion/webrtc` Stars | 16,633 |
| Contributors | 240+ |
| Releases | 162 (自 2018 年) |
| 最新发布 | v4.2.16 (2026-06-29) |
| Go Module 引用 | pkg.go.dev 广泛引用 |
| 社区渠道 | Discord, GitHub Issues, webrtcHacks |
| 文档 | pion-webrtc.mintlify.app (Mintlify 文档站) |

### 5.2 与竞品对比

| SFU/框架 | 语言 | 纯 Go | Cgo | 信令 | 拥塞控制 | 生产成熟度 |
|----------|------|-------|-----|------|----------|-----------|
| **Pion + ion-sfu** | Go | ✅ | ❌ | gRPC/JSON-RPC | TWCC, REMB | 高 |
| **LiveKit** (基于 Pion) | Go | ✅ | ❌ | 内置 | TWCC | 极高 |
| **Janus** | C | ❌ | N/A | REST/WS | REMB, TWCC | 极高 |
| **mediasoup** | C++/Node | ❌ | N/A | 自定义 | TWCC | 极高 |
| **Jitsi** | Java | ❌ | N/A | XMPP | REMB | 极高 |
| **Medooze** | C++/Node | ❌ | N/A | 自定义 | TWCC | 高 |

### 5.3 为什么选 Pion (vs libwebrtc)

1. **25x 性能提升** — 服务器 SFU 场景, libwebrtc 的线程模型是巨大开销
2. **构建速度** — 0.28 秒 vs libwebrtc 数分钟
3. **部署简单** — 单二进制, 无共享库依赖
4. **可控性** — SettingEngine + Interceptor 管道提供细粒度控制
5. **Go 并发** — Goroutine 天然映射到长连接模型, 数千连接无压力
6. **跨平台** — 同一份代码编译到 Linux/macOS/Windows/Android/iOS/WASM

### 5.4 典型应用场景

- **AI Voice Gateway**: CallSphere 的 Pion 网关直接终止 WebRTC, 解析 Opus, 对接 STT/LLM/TTS
- **视频会议**: ion 集群 (SFU 水平扩展)
- **IoT/监控**: RTSP → WebRTC 桥接 (deepch/RTSPtoWebRTC)
- **直播推流**: OBS WHIP 推入 → 多观众 WHEP 拉出
- **远程桌面/游戏串流**: 1:1 低延迟 WebRTC
- **文件传输**: WebTorrent / IPFS 基于 RTCDataChannel

---

## 6. 亮点与局限

### 6.1 核心亮点

| 亮点 | 详情 |
|------|------|
| **真·零依赖** | 纯 Go, 无 Cgo, 无 C 库, `go build` 即构建 |
| **社区验证** | 16.6K ★, OpenAI ChatGPT Advanced Voice 底层 |
| **Interceptor 管道** | 业界最优雅的 RTP/RTCP 插件系统, 链式组合 |
| **服务器性能** | 15K+ PeerConnections 单机, 25% CPU |
| **API 熟悉度** | 浏览器 WebRTC API 1:1 映射, 零学习曲线 |
| **构建速度** | <1 秒完整构建, 77 秒测试套件 |
| **文档质量** | W3C 规范实现 + GoDoc + Mintlify 文档站 + 30+ 示例 |
| **活跃维护** | 2026 年仍活跃, 162 个版本, 作者现任职 OpenAI |

### 6.2 已知局限

| 局限 | 详情 | 缓解方案 |
|------|------|----------|
| **纯 Go 编码器性能** | 无硬件加速编码 (VP8/VP9 软件编码速度不及 Chrome) | libgowebrtc 桥接, 或使用外部编码器 |
| **低 FPS 问题** | pion/webrtc #1281 报告 Chrome→Pion 仅 10fps (Chrome→Chrome 60fps) | 正确配置 RTCP Reports/REMB, 浏览器带宽自适应需要反馈 |
| **RTCP 处理需手动** | SR/RR 需要手动调用 ReadRTCP(), 非自动 (#3138) | v5 计划自动处理 |
| **ion 停更** | ion 集群 2023 年后不活跃, 推荐 LiveKit | 使用 ion-sfu 直接嵌入或用 LiveKit |
| **不内置拥塞控制** | Pion 不内置 GCC (Google Congestion Control), 依赖浏览器端 | `pion/interceptor/pkg/gcc` 可注册 |
| **ion-sfu 性能** | 单房间 10 人以上 CPU 显著上升 (#481) | Simulcast + 低码率层 + 水平扩展 |
| **SDP 复杂性** | WHIP/WHEP bug (#2922) 等问题表明 SDP 边缘情况仍有坑 | 使用 WHIP/WHEP 标准化信令可规避 |

### 6.3 不是银弹的场景

- **浏览器内编码优化**: 如果你需要 H.264 硬件编码器即时响应 (如游戏串流), libwebrtc 适合
- **全功能媒体服务器**: 需要录制、转码、混流等, 考虑 Janus 或 mediasoup
- **需要 SIP/H.323 互通**: Pion 不原生支持, 需额外桥接
- **C++ 生态绑定**: 如果现有系统是 C++ 且有复杂的 libwebrtc 定制, 迁移成本高

---

## 7. 对 MediaServo 的参考价值

### 7.1 架构理念借鉴

1. **Interceptor 管道模式**: Pion 的 Interceptor 架构是 MediaServo PipelineEngine 的最佳参考。链式 RTP/RTCP 处理器, 支持自定义插件, 出站/入站对称——直接映射到 MediaServo 的数据管道设计。

2. **SettingEngine 设计**: Pion 将平台特定配置抽象为 `SettingEngine`, 而非污染核心 API。MediaServo 的配置系统可采用相同模式: 默认值 + 可注入的引擎配置。

3. **纯 Rust 优势**: Pion 的 "纯 Go 零 Cgo" 是其主要卖点。MediaServo 同理: 纯 Rust 意味着 `cargo build` 即可, 无 C 链接地狱, 交叉编译简易。

4. **SFU 的 Router 模式**: `ion-sfu` 的 Router→Receiver→DownTrack 设计简洁高效。MediaServo 的媒体路由可参考: 按 trackID 索引, Buffer 工厂模式, RTCP 中央通道。

### 7.2 可直接复用的技术决策

| Pion 决策 | MediaServo 对应 |
|-----------|---------------|
| Interceptor 管道 → 插件化 RTP/RTCP | PipelineEngine + PluginManager 的核心思路 |
| SettingEngine → 平台适配器 | MediaServo 的 `config` 模块设计 |
| BufferFactory → 可替换缓冲策略 | 类似的 Buffer 抽象层 |
| IceUDPMux → 单端口多路复用 | Host 应用的网络层设计 |
| Ballast → GC 优化 | Rust 不需要, 但可用 `jemalloc` 等 |
| webrtc-stats → 标准统计 API | MediaServo 的监控指标 |
| WHIP/WHEP → 标准化信令 | OMSPServer 的信令协议参考 |

### 7.3 潜在集成点

- **信令协议**: MediaServo Server 的信令层可兼容 WHIP/WHEP, 实现与 OBS/GStreamer 等工具的互通
- **TURN 服务**: Pion 内置 TURN 可作为 MediaServo 中继方案的参考实现
- **监控对标**: Pion 的 `webrtc-bench` 和 `rtsp-bench` 的测试方法可直接复用为 MediaServo 的性能基准框架
- **LiveKit 的 SDK 生态**: LiveKit 提供 8 种语言 SDK (JS, Swift, Kotlin, Flutter, React, Python, Rust, Unity), MediaServo 可对标的客户端覆盖

### 7.4 不会采用的部分

- Pion 的 Go 并发模型 (Goroutine + Channel), MediaServo 使用 Rust 的 async/await + Tokio
- ion 集群的 etcd + NATS 依赖栈, MediaServo 可用更轻量的方案
- Go 内存管理策略 (Ballast), Rust 的所有权模型天然无 GC 压力

### 7.5 一句话总结

**Pion 证明了 "语言原生 WebRTC 栈" 的可行性** — 纯 Go 实现性能可达 libwebrtc 的 15-25x (服务器场景), 且构建/部署/跨平台体验远超 C++ 方案。MediaServo 以 Rust 构建相同愿景, Pion 的架构决策 (Interceptor 管道, SettingEngine, Router 模式) 是经过生产验证的最佳实践, 值得深度参考。

---
**相关决策**: D144-D145 (多后端trait), D153 (RTP interceptor)

## 参考资源

| 资源 | 链接 |
|------|------|
| Pion WebRTC GitHub | https://github.com/pion/webrtc |
| Pion 文档站 | https://pion-webrtc.mintlify.app |
| GoDoc API | https://pkg.go.dev/github.com/pion/webrtc/v4 |
| ion-sfu GitHub | https://github.com/ionorg/ion-sfu |
| ion 集群 GitHub | https://github.com/ionorg/ion |
| LiveKit | https://github.com/livekit/livekit |
| Interceptor 框架 | https://github.com/pion/interceptor |
| webrtc-bench | https://github.com/pion/webrtc-bench |
| webrtcHacks 访谈 | https://webrtchacks.com/how-go-based-pion-attracted-webrtc-mass-qa-with-sean-dubois/ |
| Pion 架构 Wiki | https://github.com/pion/webrtc/wiki/Architecture-WebRTC |
| CallSphere Pion 实践 | https://callsphere.ai/blog/vw1e-pion-go-sfu-ai-gateway |
| libgowebrtc | https://github.com/thesyncim/libgowebrtc |
