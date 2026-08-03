# 视频会议产品调研报告

> 生成日期: 2026-07-16
> 目的: 为 AUDEMSP 会议模块选型 SFU 架构和信令设计提供参考

---

## 目录

- [架构分类概览](#架构分类概览)
- [SFU 架构产品](#sfu-架构产品)
  - [Zoom](#1-zoom)
  - [Google Meet](#2-google-meet)
  - [Microsoft Teams](#3-microsoft-teams)
  - [Jitsi Meet](#4-jitsi-meet)
  - [LiveKit](#5-livekit)
  - [mediasoup](#6-mediasoup)
  - [Janus](#7-janus-gateway)
- [SFU+MCU 混合架构产品](#sfumcu-混合架构产品)
  - [BigBlueButton](#8-bigbluebutton)
  - [腾讯会议](#9-腾讯会议)
- [SD-RTN 网络层产品](#sd-rtn-网络层产品)
  - [Agora](#10-agora)
- [总结合成](#总结合成)

---

## 架构分类概览

| 架构类型 | 产品 |
|---------|------|
| **纯 SFU** | Zoom、Google Meet、Jitsi、LiveKit、mediasoup、Janus |
| **SFU+MCU 混合** | Microsoft Teams、BigBlueButton、腾讯会议 |
| **SD-RTN (全球专网)** | Agora |

---

# SFU 架构产品

## 1. Zoom

### 概况

Zoom 是全球市场份额最高的视频会议平台之一。2020 年 4 月日会议参与者峰值达 3 亿（含重复计数）。提供 Meetings、Webinars、Phone、Contact Center 等全系列 UCaaS 产品。架构核心为自研 MMR (Multi-Media Router)，本质是 SFU。全球数十个数据中心，可用性 99.9%。

### 架构

**SFU (MMR)** — Zoom 的 MMR 是分布式订阅型 SFU，不做服务端转码或混流。核心架构三层：
- **MMR (Meeting Server)**: 媒体路由节点，负责选择性转发 RTP/SRTP 包
- **Zone Controller**: 管理同区域 MMR 集群的负载均衡和健康状态
- **Global Cloud Controller**: 跨区域调度，根据 geolocation 将用户路由到最近可用 Zone

**级联策略**: 参会者在同一区域时分配到同一 MMR；跨区会议通过 Zoom 全球骨干网级联 MMR。Zoom Node 支持混合部署（本地 MMR + 云 MMR）。自适应传输层：UDP 优先，无缝 fallback 到 TCP/TLS (443)。屏幕共享使用 Reliable UDP。

### 技术栈

- **信令**: 自研协议，通过 HTTPS/WebSocket，Zone Controller 分配最优 MMR
- **编解码**: 自研自适应 codec，多同时流发送，客户端动态选层。包丢失 ~45% 仍可通话
- **带宽适配**: 监控带宽、丢包、延迟、抖动 + CPU/内存/IO，动态调整帧率分辨率
- **安全**: SRTP + DTLS，支持 E2EE（可选）

### 延迟与扩展性

- 2 人 P2P，3+ 人切换到 SFU。Webinar 模式大规模观看者走 HLS/DASH CDN
- 后端全球多区域部署，地理就近接入，跨区骨干网互联
- **教训**: Zoom Web SDK 视频走 DataChannel (TCP)，在中等 WiFi 下有可观测的冻结问题

### 优缺点

**优点**: 全栈自研，端到端优化；大规模验证的 SFU 架构；音频优先策略保障通话连续性
**缺点**: 闭源，不可自建；非 WebRTC 原生协议，与开源生态隔离；Web SDK 性能劣于原生客户端

### 可借鉴设计

- **MMR 三层架构**: Global Controller → Zone Controller → MMR 的分层调度模式
- **自适应传输**: UDP/TCP fallback + 多同时流 + 客户端选层
- **混合部署**: Zoom Node 模式允许本地 + 云 MMR 共存
- **音频优先**: 丢包 45% 仍保证音频，视频降级

---

## 2. Google Meet

### 概况

Google Meet 是 Google Workspace 的核心视频会议产品，前身为 Google Hangouts。基于 WebRTC 标准，完全在浏览器中运行。全球部署，利用 Google 全球网络基础设施。2023 年开始灰度部署 AV1 codec。

### 架构

**纯 SFU + Virtual Media Streams** — Meet 使用独特的「虚拟媒体流」架构：
- 每个客户端维持**恰好 3 个音频 transceiver**和**1~3 个视频 transceiver**（固定数量，生命周期不变）
- SFU 将多个参与者的媒体复用到这少量固定流上，通过 RTP CSRC (Contributing Source) 字段标识真实来源
- 音频：SFU 总是传输当前最响的 3 个发言人的音频，SSRC 不变，CSRC 动态切换
- 这相当于**在 SFU 层做了「逻辑混流」**——客户端只需处理固定数量流，降低客户端负载

**编解码**: 支持 VP8、VP9、AV1。视频使用 VP9 SVC (L3T3_KEY 模式)，根据参会人数动态开关 SVC。AV1 在预加入阶段灰度测试（发送 320×180 预热流获得带宽估计），正式通话时回退到 VP9 SVC。

### 信令

- 信令通过 Google Meet REST API (HTTPS)
- SDP offer/answer 模型：客户端始终发起 offer，Meet 服务端回应 answer
- 需支持 `transport-cc` 扩展、Opus 音频编码
- 数据通道 (SCTP) 用于 `session-control` 和 `media-stats`

### 延迟与扩展性

- 借助 Google 全球网络基础设施，边缘节点就近接入
- AV1 编解码可降低 30-50% 比特率（相比 VP9/H.264），40kbps 可维持视频
- **教训**: VP9 SVC 在硬件解码器上不可用时会 fallback 到软解，增加客户端 CPU 负载

### 优缺点

**优点**: 纯 WebRTC 标准，与浏览器深度整合；虚拟媒体流降低客户端复杂度；AV1 部署策略稳健（渐进灰度）；Google 全球网络基础设施保障
**缺点**: 不可自建；仅支持 Chrome/Edge/Firefox；录制等高级功能依赖 Google Cloud

### 可借鉴设计

- **虚拟媒体流 (Virtual Media Streams)**: 固定数量 SSRC + 动态 CSRC 切换，客户端只需处理 3+3 路流
- **AV1 渐进部署**: 先用 AV1 做预热流量获得带宽估计，不暴露用户视频
- **动态 SVC**: 根据参会人数切换 SVC 开关（人少不用 SVC 避免软解开销）
- **CSRC 活跃发言人切换**: 服务器端决定最响 3 人，客户端无需重协商

---

## 3. Microsoft Teams

### 概况

Microsoft Teams 是 Microsoft 365 生态的核心协作平台，整合聊天、会议、通话、文件协作。会议架构基于 Azure Communication Services 基础设施，与 Skype for Business 演进而来。支持最多 1000 人互动 + 10000 人仅观看。

### 架构

**SFU+MCU 混合** — Teams 媒体处理器 (Media Processor) 可根据场景在 SFU 和 MCU 模式间切换：
- **1:1 通话**: 优先 P2P 直连（通过 ICE/STUN/TURN），媒体不经过服务器
- **多人会议**: SFU 模式，使用 Azure 媒体处理器
- **PSTN 接入**: MCU 模式，做编解码转换和混音
- 媒体处理器同时处理转录、录制、实时字幕

**传输层**: SRTP over UDP 优先，TCP fallback。ICE 完整性检查确定最佳路径。支持 Transport Relay（中继）穿越企业防火墙。

### 信令

- 信令使用 HTTPS REST (TLS 1.2)
- SDP offer/answer 模型
- PSTN 场景使用 SIP 协议对接 SBC
- 会议中视频订阅管理可能通过 SCTP/DataChannel 进行

### 编解码与安全

- 支持 H.264、H.265/HEVC、AV1（新版）
- 音频使用 Opus 和 SILK
- SRTP 加密（SDES 优先于 DTLS，因向前兼容 SBC）
- 企业级合规：数据驻留、eDiscovery、保留策略

### 延迟与扩展性

- Azure 全球数据中心就近接入
- P2P 直连最低延迟；SFU 转发增加少量延迟
- 媒体处理器弹性伸缩，按会议所在区域分配
- **教训**: 1:1 通话和会议使用不同媒体路径（DataChannel 订阅管理），VDI 优化场景下需确保双向 SCTP 通道畅通

### 优缺点

**优点**: 深度集成 M365 生态；SFU/MCU 灵活切换覆盖全场景；企业级安全合规；PSTN 原生支持
**缺点**: 闭源不可自建；架构复杂（会话边界控制器、媒体处理器、传输中继三层）；Web 端功能受限；依赖 Azure 基础设施

### 可借鉴设计

- **SFU/MCU 动态切换**: 根据场景自适应选择媒体路径
- **ICE/STUN/TURN 中继架构**: 完整 NAT 穿透方案
- **DataChannel 订阅管理**: WebRTC DataChannel 用于 SFU 视频订阅的信令控制
- **Direct Routing**: SBC 对接 PSTN 的媒体 bypass 方案

---

## 4. Jitsi Meet

### 概况

Jitsi Meet 是最流行的开源视频会议方案之一，由 8x8 公司维护。完全基于 WebRTC 标准，提供自托管部署能力。由 Jitsi Videobridge (JVB)、Jicofo、Prosody 三大核心组件构成。社区活跃，疫情期间承受了巨大流量增长。

### 架构

**纯 SFU (Jitsi Videobridge)** — JVB 是 Java 实现的 SFU，不做转码混流：
- **JVB (Jitsi Videobridge)**: SFU 媒体服务器，纯 Java 实现。每个 JVB 实例可处理数百场会议。UDP 10000 端口收流，支持 Simulcast、VP8/VP9/H.264
- **Jicofo (JItsi COnference FOcus)**: 信令焦点。管理 XMPP MUC 房间，为每个参会者选择最优 JVB，通过 Colibri2 协议分配媒体端点
- **Prosody**: XMPP 服务器，提供信令通道和 MUC 房间管理

**级联与区域路由** (2022 年新架构):
- **JVB Pools**: JVB 不再绑定特定 shard。每个区域独立 JVB 池，自动扩缩容
- **Remote Pools**: 跨区 JVB 池连接到其他区域的 signaling node，避免全连接爆炸
- **Region Groups**: 将邻近区域分组（如法兰克福+伦敦），避免不必要的级联
- **Secure Octo**: JVB 间通过 ICE/DTLS-SRTP 建连，替代旧的 VPN 方案。可过滤不需要的流

### 信令

- **XMPP + Jingle**: 参会者通过 XMPP MUC 房间协调
- **Colibri2**: Jicofo 与 JVB 之间的 RESTful 信令协议，用于端点分配、ICE 候选交换、源管理
- 支持 HTTP/JSON 和 XMPP 两种 Colibri 传输

### 延迟与扩展性

- 跨区级联增加 80-150ms 延迟
- 区域就近接入降低 next-hop RTT 约 29%（从 223ms 降到 158ms 跨大洲场景）
- 水平扩展：JVB 池独立扩缩，signaling node 按需部署
- **教训**: 2020 年疫情期间全连接 JVB mesh 在 50+ shard、2000+ JVB 时崩溃；改造成 Pools + Remote Pools 架构后才恢复

### 优缺点

**优点**: 完全开源；信令架构优雅（Colibri2 RESTful API）；Bridge Cascading 多区域方案成熟；组件解耦可独立扩缩
**缺点**: Java 实现内存消耗大；XMPP 协议栈重量级；录制 (Jibri) 依赖 Chrome + ffmpeg 虚拟帧缓冲；缺乏原生 MCU

### 可借鉴设计

- **Colibri2 协议**: RESTful 的 SFU 控制协议，支持创建/修改/销毁会议端点，ICE 候选交换，源管理
- **Pools + Remote Pools 级联架构**: 按区域分组 JVB，remote pool 跨区连接，避免全互联
- **Secure Octo**: JVB 间安全的 ICE/DTLS 直连，无需 VPN
- **Bridge Selection Strategy**: 可插拔的桥选择策略（单桥/区域/负载均衡/访问者分离）

---

## 5. LiveKit

### 概况

LiveKit 是新兴的开源 SFU 实现，Go 语言编写。定位为「有观点的 SFU」——内置信令协议、分布式协调、AI Agent 框架。创始团队来自 Twitch/Amazon。提供开源自建和云托管两种模式。已获得大量社区采用（尤其 AI 语音场景）。

### 架构

**Go 实现 SFU** — 分层架构，六层设计：
- **Service Layer**: HTTP/Twirp API，信令入口
- **Routing Layer**: Router 接口抽象（LocalRouter / RedisRouter），房间到节点映射
- **Core RTC Layer**: Room/参与者生命周期管理
- **SFU Pipeline**: WebRTCReceiver → Buffer → Forwarder/DownTrack（三层流水线）
- **Storage Layer**: ObjectStore (RedisStore/LocalStore)
- **Infrastructure**: Wire DI、Telemetry、Config

**分布式协调**: Redis 作为共享状态存储和消息总线。PSRPC (PubSub RPC) 实现节点间 RPC。Node health 通过 Keepalive 心跳监测。

### 媒体处理

- **Ingestion**: WebRTCReceiver 接收 RTP 包，Buffer 缓存并处理重传
- **Processing**: Forwarder 做自适应层选择，VideoAllocation 根据 TWCC/REMB 带宽估计分配
- **Egress**: DownTrack 做 RTP header munging (SSRC/SN/timestamp 重写)，per-subscriber pacing
- **Dynacast**: 监控层订阅数，暂停无人订阅的编码层，节省发布者上行带宽
- 支持 VP8/VP9/H.264/AV1，VP9/AV1 支持 SVC

### 信令

- 自研 WebSocket 信令协议
- Client SDK 封装信令细节，多平台支持
- Token 认证机制

### 延迟与扩展性

- SFU 单跳增加 10-30ms 延迟（vs P2P）
- 单节点可路由数百路同时视频流
- Redis Router 多节点分布式，按区域就近接入
- Prometheus 指标监控：participant_count、packet_loss_rate、forwarded_rtp_total

### 优缺点

**优点**: Go 编写，内存安全、部署简单（单二进制）；信令内置开箱即用；AI Agent 框架前瞻；Dynacast 减少带宽浪费；文档优秀
**缺点**: 相对年轻，生产验证不如 mediasoup/Jitsi；Redis 强依赖；信令协议非标准；社区生态仍在成长

### 可借鉴设计

- **Dynacast**: 按需编码的闭环比特率管理，发布者只编码有订阅者的层
- **PSRPC**: 基于 Redis Pub/Sub 的类型安全 RPC 框架
- **Router 抽象**: LocalRouter/RedisRouter 接口化路由，灵活部署
- **单二进制**: Go 编译产物，运维简单

---

## 6. mediasoup

### 概况

mediasoup 是一个高性能 SFU 库，C++ 实现核心媒体处理，Node.js 封装为服务端模块。定位「极简主义」——只处理媒体层，不绑定任何信令协议。被广泛应用于自主构建的视频会议系统（Topology、BigBlueButton 视频部分等）。v3 是当前主流版本。

### 架构

**C++ Worker + Node.js 应用层**:
- **Worker**: 每个 CPU 核一个 C++ 子进程，实现真正的 SFU (Router)
- **Router**: 行为严格 SFU——纯 RTP 包转发，不解码不转码。支持 Simulcast/SVC 层选择，RTP 重传缓存，PLI/FIR 请求聚合
- **Transport**: WebRTC 或 Plain RTP 连接端点
- **Producer/Consumer**: 发布/订阅模型

**性能模型**: 单个 Worker (~1 CPU) 可处理约 500 Consumer。N 人全互动会议室 Consumer 数量 = N × (N-1) × 2（每人消费其他人的音频+视频），呈二次增长。

### 信令

- **完全不绑定信令协议**——开发者需自己实现信令层
- 服务端通过 Node.js API 调用 mediasoup 方法 (createRouter, createTransport, pipeToRouter 等)
- `pipeToRouter()` API 实现 Router 间级联（同主机或跨主机）

### 扩展性

- **同主机扩展**: 多 Worker 分担负载，Router 按房间分配
- **跨主机级联**: `pipeToRouter()` 连接不同主机 Router。应用层负责跨主机信令中介（如 Redis Pub/Sub 或 REST）
- **大房间策略**: 保守限制 ~16 人/Router。超限创建新 Router 并 pipeToRouter 级联
- **教训**: 全互动大房间 (>50人) 是 mediasoup 的短板，Community 明确建议限制同时发言人数而非无限扩展

### 优缺点

**优点**: 极致性能（C++ SFU 核心）；信令不可知，最大灵活性；Worker 隔离，单 Worker 崩溃不影响其他；Lightweight——CPU 消耗远低于 MCU 方案
**缺点**: 不提供信令层，开发工作量极大；扩容需自建编排系统；大房间能力受限（建议不超过 16 人全程互动）；Node.js 调用 C++ 有 FFI 开销

### 可借鉴设计

- **Router.pipeToRouter()**: 优雅的级联抽象，类似 Unix pipe
- **PLI/FIR 聚合**: 多 Consumer 请求关键帧时聚合成单个上游请求，按 500ms~1s debounce
- **Worker 隔离**: 每个 Worker 独立 C++ 进程，崩溃隔离
- **信令不可知**: 分离媒体层和控制层的设计哲学

---

## 7. Janus Gateway

### 概况

Janus 是 Meetecho 公司开发的开源通用 WebRTC 服务器，C 语言编写。核心理念是「插件架构」——核心只实现 WebRTC 协议栈（JSEP/SDP、ICE、DTLS-SRTP、DataChannel），所有应用逻辑由插件实现。是历史最悠久的 WebRTC SFU 之一，在学术界和中小企业广泛使用。

### 架构

**C 核心 + 插件体系**:
- **Core**: 实现完整 WebRTC 协议栈，内存和 CPU 管理
- **Plugins**: 业务逻辑以动态库形式加载。每个插件提供一种功能 —— EchoTest、VideoRoom (SFU)、AudioBridge (MCU 混音)、Streaming (RTSP/RTMP 注入)、SIP Gateway
- **API Transports**: 多种传输层 —— HTTP/REST、WebSocket、RabbitMQ、MQTT、Nanomsg、Unix Sockets

### VideoRoom 插件 (SFU)

- 发布/订阅模型。参与者发布媒体流，其他参与者订阅
- 支持 multistream (单个 PeerConnection 订阅多个流) 和 legacy 模式 (每个订阅独立 PeerConnection)
- 发布和订阅使用不同 PeerConnection（设计选择，避免重协商复杂性）
- RTP Forwarding：将发布者的 RTP/RTCP 流转发到外部服务（录制、转码、分析）

### 信令

- 多层 API：HTTP REST、WebSocket、RabbitMQ、MQTT 等
- janus.js 客户端库封装交互
- 插件间可以通过组合实现复杂场景（如 VideoRoom + AudioBridge 实现带混音的会议）

### 延迟与扩展性

- C 实现性能优异，单进程开销小
- 注意：VideoRoom 不支持将订阅者和发布者放在同一 PeerConnection（设计限制）
- 可部署在边缘设备（Raspberry Pi、无人机等）用于 IoT 场景
- **教训**: 插件架构虽然灵活，但插件间组合的复杂性容易被低估

### 优缺点

**优点**: C 实现极致轻量；插件架构高度可扩展；丰富的 API 传输层选择；学术研究友好（ACM 论文论证）；可部署在资源受限设备
**缺点**: 开发体验相对落后（C 语言）；插件 API 学习曲线陡峭；VideoRoom 发布/订阅分离 PeerConnection 增加客户端复杂度；文档以 Doxygen 生成，可读性一般

### 可借鉴设计

- **插件架构**: 将应用逻辑与 WebRTC 协议栈完全解耦
- **RTP Forwarding**: SFU 流可转发到外部非 WebRTC 端点（录制服务、AI 分析等）
- **多 API 传输层**: 同一核心支持 HTTP/WS/MQTT/RabbitMQ 多种控制协议
- **功能组合**: 不同插件组合实现复杂场景（VideoRoom + AudioBridge + SIP）

---

# SFU+MCU 混合架构产品

## 8. BigBlueButton

### 概况

BigBlueButton 是全球最流行的开源在线教育/虚拟教室平台。由 Blindside Networks 维护，面向教学场景深度优化——白板、演示、分组讨论、投票、录制回放。当前稳定版 3.0。部署在 Ubuntu 服务器上，通过 deb 包管理组件。

### 架构

**SFU + MCU 双重媒体架构**:
- **音频 (FreeSWITCH — MCU)**: 传统 MCU 架构处理语音会议。所有参与者音频在 FreeSWITCH 服务端混音，每人下载一路混合音频。支持 PSTN 拨入。问题：CPU 密集，50 人会议即使大部分静音也消耗大量资源
- **视频 (mediasoup — SFU)**: 使用 mediasoup 处理摄像头视频和屏幕共享。直接转发不解码。支持 VP8/H.264
- **过渡中**: 正在将音频从 FreeSWITCH MCU 迁移到 mediasoup SFU。「透明听音模式」实验性功能已可用——静音时自动挂断 FreeSWITCH 通道节约 CPU

### 组件体系

- **bbb-webrtc-sfu (Node.js)**: 信令服务器，桥接客户端到 mediasoup/FreeSWITCH
- **bbb-apps-akka (Scala)**: 核心应用服务，维护会议状态
- **Redis Pub/Sub**: 组件间通信总线
- **bbb-webrtc-recorder**: 录制组件，支持 WebM 格式
- **Kurento**: 旧版媒体服务器（已废弃，迁移到 mediasoup）

### 编解码与配置

- 音频：Opus 48kHz, 2ch, 30kbps max, FEC enabled
- 视频：VP8/H.264，1500kbps 默认上限
- Worker 分配：`auto` 策略 = `ceil((min(nproc,32) * 0.8) + (max(0, nproc - 32) / 2))`
- 支持按媒体类型分配专用 Worker（audio/main/content 独立池）

### 延迟与扩展性

- 单服务器可处理数场中等规模会议（取决于 mediasoup Worker 和 FreeSWITCH 容量）
- 水平扩展需要自定义负载均衡（社区提供方案示例）
- **教训**: FreeSWITCH MCU 音频是最大瓶颈——50 人会议可能比 200 人听音模式消耗更多 CPU；正在全力迁移到 mediasoup 全 SFU 架构

### 优缺点

**优点**: 教学场景功能最完整（白板、轮询、分组讨论室、录制播放）；完全开源可自建；mediasoup 视频架构性能优秀；Worker 按媒体类型隔离
**缺点**: FreeSWITCH MCU 音频瓶颈明显；组件多部署复杂（10+ deb 包）；实时通信延迟不如纯 SFU；迁移过程中架构过渡期脆弱

### 可借鉴设计

- **按媒体类型隔离 Worker**: audio/main/content 使用独立 mediasoup Worker 池
- **透明听音模式 (Transparent Listen Only)**: 静音自动释放服务器资源的智能音频管理
- **Recording Adapter 可插拔**: 支持 Kurento/native/bbb-webrtc-recorder 多种录制后端
- **FS Bridge Mode**: RTP/WebRTC 双模式桥接 FreeSWITCH，灵活适配部署环境

---

## 9. 腾讯会议

### 概况

腾讯会议基于腾讯 20 年音视频技术积累，2019 年底上线。自研 xCast 引擎（第三代音视频引擎，前两代为 QQ TRAE 引擎和 OpenSDK 引擎）。支持全平台。中国市场占有率最高之一，万方会议能力。提供本地部署方案满足合规需求。

### 架构

**SFU+MCU 混合，自研 xCast 引擎**:
- **Pere 传输层**: xCast 的网络传输层。非 WebRTC 实现，自研 RTP 协议栈。支持 DataChannel/WebSocket/WebTransport 多种传输
- **媒体服务器**: SFU 做视频转发（分层编码，不解码不转码），MCU 做音频混流（可配置服务端混音或客户端混音）
- **信令**: 自研协议，通过 Spear 云端配置系统动态下发流控参数
- **多点分布式部署**: 就近接入，多服务器协作，动态调整，互相备份。支持万方会议，断线自动迁移

### QoE 优化体系 (ARC)

- **配置层**: Spear 云端参数系统，实时推送编解码/3A/抗丢包参数
- **动态控制**: 流控 (ARC) 根据网络抖动、丢包动态调整 FEC/重传/jitter buffer
- **拥塞控制**: 基于 GCC 改进，延迟优先（多数网络）vs 丢包优先（跨运营商网络）自适应切换
- **优先重传**: 在时延允许时优先使用重传（节省带宽），不足时 FEC 补充

### 编解码

- 视频：H.264/H.265/AV1（自研编解码器集成在 xCast 中）
- SVC 用于下行流控——时域/空域分层选择
- Web 端通过 WebAssembly 运行自研编解码器替代浏览器内置 WebRTC codec

### 安全合规

- 国密加密标准
- 专网会议（媒体数据全程本地化，不出公网）+ 公网会议双模式
- 1+1 集群备份架构，异地容灾自动切换
- SSO 登录限制、内网接入权限、音视频水印、审计留痕

### 优缺点

**优点**: 自研引擎端到端优化；QoE 优化体系深入；SFU/MCU 灵活切换；万方会议能力；本地部署满足合规
**缺点**: 完全闭源；非 WebRTC 标准协议；对出海场景网络条件不如全球 SD-RTN 产品；技术细节公开有限

### 可借鉴设计

- **端到端 QoE 优化**: 把视频会议建模为约束优化问题（时延、带宽），各模块统一决策
- **实时拥塞控制自适应**: 不同网络类型使用不同算法（延迟敏感网络用 delay-based，丢包敏感网络用 loss-based）
- **Spear 云端配置**: 动态下发参数的流控系统，无需客户端升级
- **分级部署**: 专网+公网双轨模式，覆盖合规与灵活需求

---

# SD-RTN 网络层产品

## 10. Agora

### 概况

Agora 是全球领先的实时互动 (RTE) PaaS 平台。不直接做视频会议产品，而是提供底层 SDK 供开发者构建自己的 RTC 应用。月通话分钟数 500 亿+，覆盖 200+ 国家和地区。核心壁垒是自建全球 SD-RTN™ 专网。

### 架构

**SD-RTN™ (Software-Defined Real-Time Network)**:
- 全球 200+ POP (Points of Presence) 节点，全互联 mesh 测量
- 每个 POP 既是接入点又是路由节点
- **智能路由**: 实时测量所有路径延迟，ML 算法选择最优路由。三路冗余发送（数据经三条最优路径同时传输，最先到达的包被使用）
- **Large Channel**: 大规模广播场景，优化路由使尽可能多的参与者共享路径
- 本质是 SFU 理念的网络层实现——转发不解码，但路由优化做在网络层而非应用层

### 性能指标

- 全球中位延迟 <76ms
- 单 channel 支持 100 万+ 同时参与者
- 可用性 99.99%
- 公司历史零全系统停机
- 同区域延迟 32ms (50th percentile), 33ms (95th)
- 跨北美-欧洲: 62ms (50th), 83ms (95th)

### 信令与 SDK

- 信令 SDK 独立于媒体 SDK，支持 pub/sub、1:1、stream 三种 channel 类型
- 基于 Token 的认证系统 (App ID + App Certificate)
- SDK 覆盖所有主流平台 (iOS, Android, Web, Windows, macOS, Unity, Flutter, React Native, Electron)
- Web SDK 自研替代 WebRTC 方案——WASM 运行自研编解码，绕过浏览器限制

### 延迟与扩展性

- 相比公网 CDN (1-30 秒延迟)，SD-RTN 提供 400ms 以下延迟
- 动态扩缩容：可快速在本地数据中心或大陆级别增加节点
- 智能监测+自动路由：数据中心故障时自动绕过
- **教训**: 公网 BGP 路由存在老化问题（路由表更新跟不上实际网络变化），SD-RTN 通过全互联 mesh 实时测量绕过此问题

### 优缺点

**优点**: 全球网络延迟最低；SDK 覆盖最广；可用性极高 (99.99%)；按用量付费，无前期投入；Web SDK 自研方案绕过 WebRTC 限制
**缺点**: PaaS 模式不可自建；费用随用量线性增长；数据流经第三方网络；中国境内外版本有合规差异

### 可借鉴设计

- **三路冗余传输**: 同时推三条路径，最快的包到达即用，有效降低尾延迟
- **SD-RTN 作为 overlay**: 在公网上构建低延迟 overlay 网络
- **全互联 mesh + 实时测量**: 每个 POP 测量到其他所有 POP 的路径质量
- **Large Channel**: 大规模时共享路由路径优化

---

# 总结合成

## SFU 架构对比表

| 维度 | mediasoup | Janus | Jitsi JVB | LiveKit | Zoom MMR | Google Meet |
|------|-----------|-------|-----------|---------|----------|-------------|
| **语言** | C++/Node.js | C | Java | Go | 闭源(C/C++推测) | 闭源(C/C++) |
| **开源** | MIT | GPLv3 | Apache 2.0 | Apache 2.0 | 否 | 否 |
| **信令** | 无(自实现) | HTTP/WS/RabbitMQ/MQTT | Colibri2 (REST/XMPP) | WebSocket (自研) | 自研 | REST (SDP) |
| **Simulcast** | ✅ | ✅ VideoRoom | ✅ | ✅ | ✅ (自研多流) | ❌ (用 SVC) |
| **SVC** | ✅ | ❌ | ✅ | ✅ (VP9/AV1) | ❌ | ✅ (VP9/AV1) |
| **级联** | pipeToRouter | ❌ | Secure Octo | Redis Router | 骨干网级联 | 网络层 |
| **录制** | 需自建 | RTP Forward | Jibri (Chrome) | Egress | 原生 | 原生 |
| **E2EE** | 可选 | 可选 | 可选 | ❌ | 可选(Insertable Streams) | ❌ |
| **Worker隔离** | ✅ (进程) | ❌ | ❌ | ❌ | ✅ (节点) | ✅ (节点) |
| **AI Agent** | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ |
| **部署复杂度** | 高(需自建信令) | 中 | 中 | 低(单二进制) | 不可自建 | 不可自建 |

## AUDEMSP 技术选型建议

### SFU 选型

**推荐**: **mediasoup** 作为核心 SFU 引擎

理由：
1. **性能最优**: C++ 实现，Worker 进程隔离，极致 CPU 效率
2. **最大灵活性**: 信令不可知，可完全自定义控制面
3. **级联生态**: `pipeToRouter()` 支持同机/跨机级联，适合 AUDEMSP 多场景需求
4. **生产验证**: BigBlueButton 等大规模部署验证
5. **Rust FFI 友好**: 可通过 napi-rs 或 FFI 封装为 Rust crate，融入 AUDEMSP native-core

### 信令设计

**推荐自研信令协议，借鉴 Colibri2 + PSRPC 模式**：

```
信令层设计建议:
├── 传输: WebSocket (实时) + gRPC (服务间)
├── 协议: JSON/Protobuf 双模 (Web 用 JSON, native 用 Protobuf)
├── 状态同步: Redis Pub/Sub (借鉴 LiveKit PSRPC)
├── 房间管理: 借鉴 Jicofo 的 Bridge Selection Strategy 可插拔模式
└── 端点管理: 借鉴 Colibri2 RESTful API 设计
    - POST /conferences → 创建会议+分配 SFU
    - PATCH /conferences/{id} → 更新端点/ICE/源
    - DELETE /conferences/{id} → 销毁
    - GET /conferences/{id}/dominant-speaker → 活跃发言人
```

### 音频方案

**推荐**: SFU 转发 + 客户端混音为主，MCU 混音为补充（PSTN/录制桥接）

借鉴 BigBlueButton 教训：MCU 音频是扩展性瓶颈。优先使用 SFU 转发音频，客户端解码多路并混音。仅在 PSTN 桥接和录制场景使用 MCU。

### 录制方案

**推荐双层架构**:
- **实时录制**: 类似 Janus RTP Forwarding——SFU 将流复制一份推送到录制服务（参考 mediasoup Producer → 外部 Consumer 模式）
- **异步合成**: 类似 Zoom 的 post-meeting 批量合成——各轨独立存储，会议结束后 GPU/CPU 合成最终视频

### 关键教训汇总

1. **不要用 TCP 传媒体**: Zoom Web SDK 的 DataChannel 视频是反例
2. **PLI/FIR 聚合必不可少**: 100 个订阅者同时请求关键帧 = 编码器灾难
3. **信令层要分离**: mediasoup 的成功经验——媒体层和控制层解耦是长期可维护性的关键
4. **Worker 隔离**: 进程隔离 > 线程隔离 > 无隔离
5. **全互动大房间是伪需求**: mediasoup 社区经验——限制同时发言人数比追求无限扩展更务实
6. **全连接 mesh 不可扩展**: Jitsi 2020 年教训——50+ shard 全连接崩溃，必须用 Pools 模式
7. **AV1 渐进部署**: Google Meet 策略——预热流量测试新编解码，不暴露用户数据
8. **模拟发送预估带宽**: Google Meet 预热阶段发送 320×180 流获得准确带宽估计
9. **E2EE 与 SFU 转发兼容**: 只需要 RTP header 可读写 (SRTP 解包后重加密)，payload 不碰
10. **拥塞控制要自适应**: 腾讯会议经验——不同网络类型用不同算法，延迟敏感 vs 丢包敏感

### 架构蓝图: AUDEMSP 会议模块

```
┌─────────────────────────────────────────────────────┐
│                    Client (Web/Native)               │
│  ┌─────────┐  ┌──────────┐  ┌────────────────────┐  │
│  │ Camera  │  │ Microphone│  │ Screen Capture     │  │
│  └────┬────┘  └────┬─────┘  └─────────┬──────────┘  │
│       │ VP8/AV1    │ Opus           │ VP8/AV1       │
│       ▼            ▼                ▼               │
│  ┌─────────────────────────────────────────────┐    │
│  │         WebRTC Encoder (Simulcast + SVC)    │    │
│  └────────────────────┬────────────────────────┘    │
│                       │ SRTP/UDP                     │
└───────────────────────┼─────────────────────────────┘
                        │
┌───────────────────────┼─────────────────────────────┐
│                 Signaling (WebSocket)                │
│  ┌────────────────────┴────────────────────────┐    │
│  │         Conference Controller                │    │
│  │  (Rust: room mgmt, SFU selection, ICE relay) │    │
│  └────────────────────┬────────────────────────┘    │
│                       │                              │
│  ┌────────────────────┴────────────────────────┐    │
│  │         mediasoup SFU Engine                 │    │
│  │  (C++ workers, napi-rs binding)              │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  │    │
│  │  │ Worker 1 │  │ Worker 2 │  │ Worker N │  │    │
│  │  │ Router A │  │ Router B │  │ Router C │  │    │
│  │  └──────────┘  └──────────┘  └──────────┘  │    │
│  └─────────────────────────────────────────────┘    │
│                       │                              │
│  ┌────────────────────┴────────────────────────┐    │
│  │    Recording / ASR / Analytics Pipeline      │    │
│  │    (RTP Forwarding → external consumers)     │    │
│  └─────────────────────────────────────────────┘    │
│                       │                              │
│  ┌────────────────────┴────────────────────────┐    │
│  │    Redis: state sync, pub/sub, room routing  │    │
│  └─────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────┘
```

---

> **下一步**: 基于本调研结论，细化会议模块的组件选型和接口设计。

## 对应的决策

| 研究发现 | 对应决策 |
|---------|---------|
| SFU 转发最响 N 路 (Google Meet) | D50 |
| mediasoup Worker-per-core (Jitsi/mediasoup) | D-SFU-WORKER |
| Audio-first QoS 降级链 (Zoom/Jitsi) | D-QOS-AUDIO |
| Colibri2 RESTful 资源建模 (Jitsi) | Phase 2 设计参考 |
| LiveKit 纯 SFU 插件化 | D14, D97 |
| GOP Cache (SRS/MediaMTX) | D-GOP-CACHE |
| 场景感知 Simulcast/Dynacast | D-SIMULCAST |
| 统一 Room 模型 (LiveKit) | D53 |
