# MediaMTX 参考分析
> 生成日期：2026-07-16 | 分类：流媒体

## 1. 产品画像
- **名称**：MediaMTX（原名 rtsp-simple-server，2023年更名）
- **开发者**：Bluenviron（独立开发者/组织，主要作者为单个核心开发者）。各协议库由同一组织维护：gortsplib (RTSP), gortmplib (RTMP), gohlslib (HLS), mediacommon (通用媒体工具)
- **首次发布**：2019年12月（rtsp-simple-server v0.0.0），2023年更名为 MediaMTX v1.0.0。持续开发超过 6 年，123+ releases
- **产品定位**：即用型零依赖实时媒体服务器和媒体代理。定位为"媒体路由器"（media router），而非"流媒体平台"。核心理念是协议间的自动转换 — publisher 用任意协议推流，viewer 用任意协议观看，无需配置。不内置转码，纯路由
- **目标用户群体**：IoT/IP 摄像头部署者、视频监控系统集成商、需要多协议转换的流媒体架构师、家庭安防 DIY 用户（Raspberry Pi 摄像头）、小型直播平台
- **许可 / 商业模式**：MIT 许可，完全免费开源。无商业版。所有依赖库均为 MIT 或 Apache 2.0 许可

## 2. 技术特性
### 整体架构
```
┌──────────────────────────────────────────────────────────────────┐
│                      MediaMTX (Go 单二进制)                        │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐     │
│  │                    Path Manager                            │     │
│  │  path → { source, publisher, readers[], auth, config }    │     │
│  │                                                           │     │
│  │  每个 path 对应一个媒体流。生命周期由以下驱动：             │     │
│  │  - publisher 连接/断开 → path 创建/销毁                    │     │
│  │  - static source 配置 → path 持久存活                      │     │
│  │  - sourceOnDemand → 有 reader 时拉取，无 reader 时释放      │     │
│  └────────────┬──────────────────────┬───────────────────────┘     │
│               │                      │                             │
│  ┌────────────▼──────────┐  ┌────────▼──────────────────────┐     │
│  │   Publisher / Source   │  │        Reader / Consumer       │     │
│  │                        │  │                                │     │
│  │  ┌──────────────────┐  │  │  ┌──────────────────────────┐ │     │
│  │  │ Static Source     │  │  │  │ Protocol Auto-Conversion │ │     │
│  │  │ 持续拉取外部流     │  │  │  │ 零配置，自动转换          │ │     │
│  │  │ RTSP/RTMP/SRT     │  │  │  │                          │ │     │
│  │  │ 断线重连          │  │  │  │ 推 RTSP → 看 WebRTC     │ │     │
│  │  └──────────────────┘  │  │  │ 推 RTMP → 看 LL-HLS      │ │     │
│  │                        │  │  │ 推 SRT → 看 HLS          │ │     │
│  │  ┌──────────────────┐  │  │  │ 推 WebRTC → 看 RTSP      │ │     │
│  │  │ sourceOnDemand   │  │  │  │ 推 MoQ → 看 WebRTC       │ │     │
│  │  │ 惰性拉流模式      │  │  │  └──────────────────────────┘ │     │
│  │  │ 有 viewer 时拉取  │  │  │                                │     │
│  │  │ 无 viewer 时释放  │  │  │  ┌──────────────────────────┐ │     │
│  │  │ 省电/省带宽       │  │  │  │ Recording & Playback    │ │     │
│  │  └──────────────────┘  │  │  │ fMP4 / MPEG-TS 分段录制  │ │     │
│  │                        │  │  │ REST API 按时间段检索     │ │     │
│  │  ┌──────────────────┐  │  │  │ 回放模拟实时流            │ │     │
│  │  │ RPI Camera        │  │  │  └──────────────────────────┘ │     │
│  │  │ 硬件H.264/MJPEG   │  │  │                                │     │
│  │  │ 时间戳文字叠加    │  │  │                                │     │
│  │  └──────────────────┘  │  │                                │     │
│  └────────────────────────┘  └────────────────────────────────     │
│                                                                   │
│  内部数据流模型：                                                  │
│  path 中的流由 reader track 管理。每个 track 有一个 ring buffer    │
│  缓冲最近 N 个数据单元。新 reader 连接时从 ring buffer 最近     │
│  关键帧开始发送，保证秒开                                          │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐     │
│  │ 协议层（每个协议有独立的服务端/客户端实现）                  │     │
│  │                                                           │     │
│  │  ┌──────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐ │     │
│  │  │  RTSP    │ │   RTMP    │ │   SRT     │ │  WebRTC   │ │     │
│  │  │gortsplib │ │ gortmplib │ │  libsrt   │ │ pion/     │ │     │
│  │  │[推/播]   │ │ [推/播]   │ │ [推/播]   │ │ webrtc    │ │     │
│  │  │TCP/UDP   │ │  TCP      │ │  UDP      │ │WHIP/WHEP  │ │     │
│  │  └──────────┘ └───────────┘ └───────────┘ └───────────┘ │     │
│  │  ┌──────────┐ ┌───────────┐ ┌───────────┐               │     │
│  │  │ LL-HLS   │ │ MPEG-TS   │ │Media-over- │               │     │
│  │  │gohlslib  │ │   UDP     │ │   QUIC    │               │     │
│  │  │ [播]     │ │ [推/播]   │ │WebTransport│              │     │
│  │  │HTTP      │ │           │ │[推/播]     │               │     │
│  │  └──────────┘ └───────────┘ └───────────┘               │     │
│  │  ┌──────────┐                                            │     │
│  │  │   RTP    │  UDP 推流 (原始 RTP)                      │     │
│  │  └──────────┘                                            │     │
│  └──────────────────────────────────────────────────────────┘     │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐     │
│  │ 扩展层                                                     │     │
│  │  ┌────────────────┐  ┌──────────────┐  ┌──────────────┐ │     │
│  │  │ runOn*/exec钩子 │  │ 外部认证      │  │ Read Replica │ │     │
│  │  │ FFmpeg子进程   │  │ HTTP回调     │  │ Origin→Edge  │ │     │
│  │  └────────────────┘  └──────────────┘  └──────────────┘ │     │
│  └──────────────────────────────────────────────────────────┘     │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐     │
│  │ Runtime API + Observability                                │     │
│  │ REST API: /v3/config/paths/*, /v3/recordings/*, config   │     │
│  │ Prometheus metrics, pprof, structured logging, healthz    │     │
│  └──────────────────────────────────────────────────────────┘     │
└──────────────────────────────────────────────────────────────────┘
```

### Path 模型详解

MediaMTX 的核心抽象是 Path，每个 path 是一个媒体流的生命周期管理单元：

- **Path 状态机**：Idle（无 publisher 无 source）→ Ready（有 publisher 或 source 提供数据）→ OnDemandWaiting（sourceOnDemand = true，有 viewer 等待拉流）→ OnDemandPulling（正在拉取上游流）→ Idle（publisher/source 断开，所有 viewer 未连接）
- **Publisher 模式**：外部客户端推流到 path（RTSP ANNOUNCE/SETUP, RTMP publish, SRT caller, WHIP POST, MoQ ANNOUNCE）
- **Static Source 模式**：服务器主动从外部拉流（RTSP DESCRIBE/SETUP/PLAY, RTMP play, SRT listener），持续重连
- **sourceOnDemand 模式**：仅在 viewer 存在时才拉取上游流。viewer 断开后等待 onDemandCloseAfter 时间后释放连接
- **认证与授权**：每个 path 可单独配置 publisher 和 reader 的认证。支持 external（HTTP 回调认证）、externalStart（仅在 path 启动时回调）、rtsp（RTSP Digest）、jwt、internal（配置文件中的明文凭据）
- **IP 白名单**：按发布和读取分别配置 IP/CIDR 列表

### 关键技术能力
| 能力 | 详情 |
|------|------|
| 协议支持 | 输入(推流)：RTSP (TCP/UDP, TLS), RTMP/RTMPS (TCP), SRT (UDP, AES-128/256 加密), WebRTC/WHIP (UDP/TCP, DTLS-SRTP), Media-over-QUIC (QUIC/HTTP3 WebTransport), MPEG-TS over UDP, RTP over UDP。输出(播放)：RTSP, RTMP, SRT, WebRTC/WHEP, LL-HLS, Media-over-QUIC, MPEG-TS |
| 编码 | 视频：H.264, H.265(HEVC), AV1, VP9, VP8, MJPEG。音频：Opus, MPEG-4 Audio (AAC), G.711 (PCMA/PCMU), LPCM, FLAC (v1.19.0+), G.726。**纯 passthrough 模式，不解码不重新编码** |
| 传输 | TCP/UDP（各协议原生），QUIC/HTTP3/WebTransport (Media-over-QUIC)。RTSP 支持 TCP/UDP 自动切换（interleaved TCP/UDP transport fallback）。RTP over UDP 支持单播和组播 |
| 录制 | fMP4 (fragmented MP4) 或 MPEG-TS 分段录制到磁盘。按时间和文件大小分段。REST API 按起止时间检索段列表。录制路径和段大小可配置 |
| sourceOnDemand | 仅在 viewer 存在时从上游拉流。viewer 断开后等待可配置的延时后释放。适用电池供电 IP 摄像头或带宽受限场景 |
| 回放 | 将录制的片段重新作为实时流播放。REST API 按时间段选择回放范围。支持加速/减速播放和暂停 |
| API & 热重载 | REST API 运行时管理 path 和配置。热重载配置文件不中断已有连接（except 端口变更、加密密钥变更等特殊情况） |
| 安全 | Argon2 密码哈希。TLS 1.3（HTTPS/RTMPS/RTSPS）。Basic/Digest 认证。JWT。HTTP 外部认证 |
| RPI Camera | mediamtx-rpicamera 模块：树莓派摄像头硬件 H.264 编码和 MJPEG 编码。统一软硬件编码参数 (v2.8.0)。时间戳自定义文字叠加 |

### 协议转换矩阵
MediaMTX 的协议自动转换支持任意协议组合，以下是已验证的组合：

| Publisher ↓ / Reader → | RTSP | RTMP | SRT | WebRTC | LL-HLS | MoQ | MPEG-TS |
|------------------------|:----:|:----:|:---:|:------:|:------:|:---:|:-------:|
| RTSP | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| RTMP | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| SRT | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| WebRTC (WHIP) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Media-over-QUIC | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| MPEG-TS (UDP) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| RTP (UDP) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |


### 协议转换性能对比

MediaMTX 的协议转换不需要编解码操作（pure passthrough），但在不同协议组合下的延迟表现因传输层特性而异：

| 推流协议 | 播放协议 | 端到端延迟（局域网） | 瓶颈环节 | 适用 MediaServo 场景 |
|---------|---------|-------------------|---------|-------------------|
| RTSP (TCP) | RTSP (TCP) | <100ms | RTP 解包/封包 | 监控相机→监控观看 |
| RTSP (TCP) | LL-HLS | 500ms-2s | HLS segment 缓冲 | 监控相机→Web 观看 |
| RTSP (TCP) | WebRTC | 100-200ms | ICE 协商 + RTP 转发 | 监控相机→实时互动 |
| RTMP | LL-HLS | 500ms-2s | FLV→fMP4 转换 | 通用推流→HLS 分发 |
| RTMP | WebRTC | 100-300ms | FLV→RTP 解包 | 推流→低延迟互动 |
| SRT | WebRTC | 80-200ms | 无（最低延迟组合） | 远程推流→Web 播放 |
| WebRTC | WebRTC | <80ms | 无（同级 RTP 转发） | 双向实时通话 |
| Media-over-QUIC | WebRTC | 50-150ms | QUIC→RTP 桥接 | 未来低延迟链路 |
| MPEG-TS (UDP) | RTSP | 100-200ms | UDP 组播→单播转发 | 广电级信号分发 |

**延迟等级划分**：

| 等级 | 延迟范围 | MediaMTX 组合 | MediaServo 协议路由策略 |
|------|---------|--------------|---------------------|
| 实时 (Real-time) | <150ms | RTSP→RTSP, WebRTC→WebRTC, SRT→WebRTC | Fragment→WHEP 直通，零缓冲 |
| 准实时 (Near-real-time) | 150-500ms | RTSP→WebRTC, RTMP→WebRTC, QUIC→WebRTC | Fragment→WHEP，最小 jitter buffer |
| 低延迟 (Low-latency) | 500ms-2s | RTSP→HLS, RTMP→HLS, SRT→HLS | Fragment→CMAF→LL-HLS，0.5s segment |
| 标准延迟 (Standard) | 2-10s | 任意→标准 HLS/DASH | Fragment→CMAF→HLS/DASH，3-6s segment |

### 传输层协议对比

| 传输协议 | 多路复用 | 丢包恢复 | 连接迁移 | TLS 加密 | 防火墙穿越 | MediaServo 优先级 |
|---------|:-------:|:-------:|:-------:|:--------:|:---------:|:--------------:|
| TCP | ❌ (顺序) | 重传 (头部阻塞) | ❌ | ✅ | ✅ | Phase 1 (基础) |
| UDP | ❌ | 应用层处理 | ❌ | DTLS | ⚠️ | Phase 1 (SRT/WebRTC) |
| QUIC (HTTP/3) | ✅ (并行流) | 独立流重传 | ✅ (连接迁移) | ✅ (内建) | ✅ (80/443) | Phase 4 (未来) |
| WebTransport | ✅ (stream+datagram) | 独立流重传 + 丢包忽略 | ✅ | ✅ (内建) | ✅ | Phase 4+ (前沿) |

QUIC/WebTransport 的独立流重传特性对流媒体意义重大：一个视频帧的丢包不会阻塞后续帧的传输（TCP 的头部阻塞问题）。MediaMTX v1.19.0 验证了 QUIC 在流媒体中的可行性。

### 技术栈
- **语言**：Go (100%)。使用 Go 1.22+ 特性
- **核心依赖库**（全部由 Bluenviron 自研维护）：
  - `gortsplib/v5`：RTSP 1.0 客户端和服务端库。支持 TCP/UDP/TLS transport。H.264 SPS/PPS 解析，RTP 解包/封包。v5 是一次重大重构，性能显著提升
  - `gortmplib`：RTMP 客户端和服务端库。支持 RTMP/RTMPS。FLV 解析和生成
  - `gohlslib/v2`：HLS 解析和生成库。支持 LL-HLS、fMP4 segments、主播放列表生成
  - `mediacommon/v2`：通用媒体工具库。Codec 解析 (H.264 NAL/H.265 NAL/AV1 OBU/VP9)，fMP4 写入，Bitrate 计算
  - `mediamtx-rpicamera`：Raspberry Pi 摄像头硬件编码模块。独立仓库
- **WebRTC 栈**：pion 项目（Go 版 WebRTC 标准实现，Google 贡献）。
  - `pion/webrtc/v4` — WebRTC 实现 (WHIP/WHEP)
  - `pion/srtp/v3` — SRTP (Secure RTP) 加密
  - `pion/dtls/v3` — DTLS (Datagram TLS) 握手
  - `pion/ice/v4` — ICE (Interactive Connectivity Establishment) NAT 穿透
  - `pion/sctp` — SCTP (Stream Control Transmission Protocol) RTCDataChannel
  - `pion/turn/v5` — TURN 服务 (仅在配置 webrtcAdditionalHosts 时)
- **QUIC/WebTransport**：`quic-go/webtransport-go` v0.11.0。支持 QUIC/HTTP3 和 WebTransport API
- **密码学**：`argon2` (密码哈希)，`golang.org/x/crypto` (TLS/加密)
- **构建/部署**：Go 交叉编译为单一静态二进制 (除 libsrt C 绑定外，静态链接)。Docker 镜像：~27MB (amd64), ~26MB (arm64), ~28MB (armv6/v7)。支持 darwin/amd64, darwin/arm64, linux/amd64, linux/arm/v6, linux/arm/v7, linux/arm64, windows/amd64
- **可观测性**：Prometheus metrics（`/metrics` 端点），Go pprof 性能分析（`/debug/pprof/*`），结构化日志（slog），health check (`/healthz`)

## 3. 功能概览
### 核心功能模块
| 模块 | 功能 | 实现 |
|------|------|------|
| Path Manager | 管理所有媒体路径。维护 publisher/source/reader 映射。驱动 path 生命周期状态机。处理认证和配置 | `internal/core/path_manager.go` |
| Static Source | 服务器持续连接外部流源。支持 RTSP/Rtmp/SRT 作为源。断线自动重连（指数退避）。支持源认证 | `internal/core/source_static.go` |
| sourceOnDemand | 惰性拉流。第一个 viewer 触发拉流，最后一个 viewer 断开后释放。可配置释放延迟 | `internal/core/source_on_demand.go` |
| Protocol Auto-Conversion | 自动协议转换。无需配置。publisher 推入的媒体数据自动映射到所有启用的输出协议。基于 track 的 ring buffer | `internal/core/path.go` (readPublisher 方法) |
| Recording | fMP4/MPEG-TS 分段录制。按时间或大小分段。REST API 检索段列表。录制路径可配置 | `internal/record/` |
| Playback | 将录制段作为实时流播放。REST API 按时间范围选择。支持暂停/加速/减速 | `internal/playback/` |
| Read Replica | 从 origin 实例拉流分发。Replica node 通过配置指定 origin 的 source。L4/L7 负载均衡 | `internal/core/source_static.go` (remote 源) |
| Auth & ACL | External/RTSP Digest/JWT/Internal 认证。Per-path 发布/读取凭据。IP/CIDR 白名单 | `internal/auth/` |
| RPI Camera | 树莓派硬件 H.264/MJPEG 编码。统一配置接口 (v2.8.0)。时间戳文字叠加。性能优化 | `mediamtx-rpicamera` (独立模块) |
| API | REST API: path CRUD, 录制段查询, 配置热重载, 运行时状态。Swagger/OpenAPI | `internal/api/` |
| Metrics | Prometheus metrics exporter。Path 数、客户端数、发布者/读取者数、字节数、录制段数 | `internal/metrics/` |

### 特色功能
- **零配置协议自动转换**：MediaMTX 的杀手特性。publisher 推 RTSP → viewer 用 HLS 看。publisher 推 RTMP → viewer 用 WebRTC 看。不需要任何转换规则配置。内部 track ring buffer + codec passthrough 实现了零开销的协议转换
- **Media-over-QUIC 先行者（v1.19.0, 2026-06-02）**：业界首批原生支持 Media-over-QUIC 的开源媒体服务器。基于 QUIC/HTTP3 传输和 WebTransport 浏览器 API。比 WebRTC 稍快（无 SCTP 开销），更好的丢包恢复（QUIC 原生），额外 codec 支持（FLAC），路由复杂度更低（单 UDP 端口，无 ICE/STUN 协商）
- **sourceOnDemand 惰性拉流**：IoT/摄像头场景的节能模式。摄像头仅在有 viewer 时才传输视频流。无 viewer 时完全释放网络连接，摄像头进入低功耗模式。可配置释放延迟（防止频繁上下线）。对 4G/电池供电监控相机价值巨大
- **RPI Camera 硬件编码**：直接在树莓派上使用硬件 H.264/MJPEG 编码器。v2.8.0 统一了软件和硬件编码的配置接口（`rpiCameraH264Profile` 和 `rpiCameraH264Level` 同时适用于软件和硬件路径）。性能计算优化（帧大小计算一次）
- **RTSP 隧道穿越防火墙**：RTSP over HTTP 和 RTSP over WebSocket 隧道。解决企业网络中 RTSP 端口封锁问题。gortsplib 同时支持 TCP (interleaved) 和 UDP transport，自动切换
- **Read Replica 水平扩展**：最简单的 origin-replica 架构。Origin 作为 static source，Replica 通过远程路径拉流。不需要集群管理系统（无 Raft/Paxos/etcd）。通过外部 L4 (TCP) 或 L7 (HTTP) 负载均衡器分散读流量

### 扩展性 / 插件机制
MediaMTX 没有传统插件系统。扩展方式：
- **配置驱动**：所有行为通过 YAML 配置文件控制。支持热重载（`SIGHUP` 信号或 API 触发）。配置覆盖：全局 → 协议默认 → per-path
- **REST API**：完整的运行时管理 API。路径 CRUD（POST/GET/PATCH/DELETE `/v3/config/paths/{name}`），录制段查询（GET `/v3/recordings/list`），录制段详情（GET `/v3/recordings/get/{name}`），配置重载（POST `/v3/config/reload`）
- **外部认证钩子**：`externalAuthenticationURL` — HTTP POST 回调认证。服务器将 client 的凭据和 IP 发送到外部服务，由外部服务决定是否允许连接。Callback 超时/失败默认拒绝
- **runOn*/exec 钩子**：在特定事件时执行外部命令。`runOnInit` (服务启动后)，`runOnReady` (path 就绪)，`runOnRead` (有 viewer)，`runOnRecordSegmentCreate` (录制段创建)。常用作外部 FFmpeg 转码的触发器
- **API 事件（WebSocket）**：通过 WebSocket 接收服务器事件（path 创建/删除、publisher 连接/断开、reader 连接/断开）。适合构建自定义管理界面
- **Go build tags**：编译时通过 build tags 控制特性开关。RPI Camera 模块仅在 ARM Linux 上编译
- **静态编译**：所有 Go 依赖编译进单二进制。只有一个外部 C 依赖 (libsrt)，但也在 linker 中静态链接

## 4. 现状与生态
- **当前版本**：v1.19.2 (2026-06-28，最新稳定版)，v1.19.0 (2026-06-02，新增 Media-over-QUIC 和 FLAC 支持)。活跃维护，约每月一个 release。6 年来共 123+ releases
- **GitHub Stars / 活跃度**：约 20,000 stars，1,500+ forks。高频提交，Issue 响应快速（通常 1-3 天内回复）。代码质量高（测试覆盖好，CI/CD 自动化完善）。Release 工作流完全公开（GitHub Actions 编译，区块链校验和验证）
- **社区规模**：中等规模社区。GitHub Issues/Discussions 活跃。主要用例集中在 IP 摄像头 / IoT / 监控 / 家庭安防。Discord 有用户社区
- **文档 / SDK / API 生态**：
  - 官方网站：mediamtx.org（完整文档，包括所有协议的配置指南、快速开始、部署指南、API 参考、FAQ）
  - REST API 文档：完整的 OpenAPI/Swagger 描述。所有端点有 curl 和编程语言示例
  - 配置文档：每个参数都有详细说明和默认值。YAML 配置注释完善
  - Docker Hub 官方镜像：bluenviron/mediamtx，多架构 (amd64/arm64/armv6/armv7)。自动构建，标签对应版本号
  - 客户端集成示例：提供 OBS Studio、FFmpeg、GStreamer、VLC、WebRTC (pion/browser)、RTSP (OpenCV/ffplay) 等客户端对接 MediaMTX 的配置示例
  - 无官方 SDK，但所有协议都是标准的。使用各协议的标准客户端库即可
  - 安全与校验：Release checksums.sha256 + GitHub Attestations 区块链校验
- **已知缺陷或限制**：
  - **Go GC 延迟风险**：在极高并发（万级以上连接）下 Go GC 可能产生毫秒级停顿。对于 <50ms 低延迟场景有风险
  - **不内置转码**：纯媒体路由器定位。无内部编解码引擎。需要 ABR 自适应码率时必须外部 FFmpeg/GStreamer。runOnReady 钩子 fork 的子进程管理是运维负担
  - **无内置集群管理**：Read Replica 依赖手动配置。无自动服务发现、故障转移或节点编排
  - **纯 Go 生态限制**：无法调用 GPU 硬件编码 API（NVENC/VAAPI/VT）。不适合嵌入式 C FFI 场景
  - **LL-HLS + CDN 缓存冲突**：低延迟 HLS 的部分 segment 需要绕过 CDN 缓存。对不同 CDN 的配置需要个案处理
  - **录制不支持云存储**：仅支持本地文件系统录制。无 S3/OSS/MinIO 等对象存储的直接集成
  - **MJPEG 仅 RPI Camera 支持**：通用 MJPEG 流的处理和分发未实现。仅在 RPI Camera 模块中通过硬件编码器支持
  - **RTMP Enhanced (HEVC/AV1) 支持不完善**：gortmplib 是轻量级实现，功能不如 SRS/librtmp 完整

## 5. 市场定位
- **主要应用行业**：视频监控 — IP 摄像头流转发和多协议分发（最核心场景），IoT 视频设备管理 — 将边缘设备的视频流接入云端，家庭安防 DIY — 树莓派摄像头 + Home Assistant 集成，小型直播平台 — 依赖外部转码的直播分发，多协议转换网关 — 将遗留 RTSP 系统升级为 WebRTC/HLS
- **竞品对比简表**：
| 维度 | MediaMTX | SRS | nginx-rtmp | Ant Media | Wowza | LiveKit |
|------|----------|-----|------------|-----------|-------|---------|
| 部署复杂度 | ⭐ (单二进制) | ⭐⭐ (需TURN) | ⭐⭐ (需nginx) | ⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐ |
| Docker 体积 | ~27MB | ~100MB+ | ~50MB+ | ~200MB+ | ~500MB+ | ~150MB+ |
| 内置转码 | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ |
| 集群 | Read Replica | Origin-Edge | push/pull | 内置 | 内置 | 内置 |
| Media-over-QUIC | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| RTSP 支持 | ⭐⭐⭐⭐⭐ | ⚠️ | ❌ | ⭐⭐⭐ | ⭐⭐⭐⭐ | ❌ |
| on-demand 拉流 | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| RPI Camera 优化 | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| IP Camera 场景 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐ | ⭐⭐ | ⭐⭐⭐ | ❌ |
| ABR 转码 | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ |
| 语言 | Go | C/C++ | C | Java | Java | Go/TS |
| Stars | 20K | 29K | 14K | 3K | N/A | 30K+ |
| 许可 | MIT | MIT | BSD-2 | 商业 | 商业 | Apache 2.0 |
| 发布时间 | 2019 | 2013 | 2012 | 2018 | 2007 | 2021 |
- **定价 / 许可**：MIT 免费开源。无商业版。Release 二进制免费下载

## 6. 产品特色
1. **零依赖单二进制**：Go 编译出一个静态二进制，无运行时依赖。Docker 镜像 27MB (amd64)。部署就是下载/启动。没有外部 Java/JRE (Ant Media/Wowza)，没有外部 nginx (nginx-rtmp)。这是"简单"的终极体现
2. **协议自动转换**："推一种，所有协议自动可用"。不需要配置转换规则或输入输出对。Publisher 推 RTSP，viewer 用 HLS/WebRTC/SRT/RTMP 都能播放。内部 ring buffer + codec passthrough 实现的零开销转换
3. **sourceOnDemand 惰性拉流**：IoT/电池供电场景的节能优化。摄像头仅在被人看的时候工作。无 viewer 时完全释放资源。这是监控领域特有的需求，其他通用流媒体服务器都没有实现
4. **Media-over-QUIC 先行者**：2026年6月 v1.19.0 即支持。领先 SRS (13 年项目)、nginx-rtmp (14 年项目)、Ant Media (8 年项目) 等所有竞品。QUIC 传输层在丢包恢复、连接迁移、多路复用方面优于 TCP
5. **IP 摄像头场景的一站式方案**：RPI Camera 硬件编码 + RTSP 主协议 + on-demand 拉流 + 多协议分发。从摄像头到 viewer 的完整链路，且几乎零配置

## 7. 对 MediaServo 的参考价值
### [Adopt] 可直接借鉴
1. **Path 抽象模型**：`path → { publisher/source, readers[], auth, config }` 的流生命周期管理模型直接映射到 MediaServo 的 Stream/Channel 概念。Path 作为 publisher 和 reader 的 meeting point，与 MediaServo 的 Stream 概念高度契合。状态机设计（Idle → Ready → OnDemand*）也值得直接复用
2. **sourceOnDemand 按需拉流模式**：MediaServo 监控相机接入的核心能力之一。`MediaSource` trait 的 `start()`/`stop()` 生命周期方法天然支持此模式。当没有 `StreamSubscriber` 连接时释放对摄像头的 RTSP 连接
3. **协议自动转换的零配置哲学**：MediaMTX 证明了不需要显式配置"RTSP → HLS"转换规则。MediaServo 的 Unified Fragment Model 天然支持这种模式：所有输入产生 Fragment → 所有输出消费 Fragment → 所有协议组合自动可用
4. **Read Replica 水平扩展**：origin-replica 是最简单的水平扩展模式。不需要 Raft/etcd，通过外部 LB 实现。MediaServo Phase 1-2 可以先用此模式，后期再升级到更完善的集群方案
5. **热重载配置**：不中断已有连接更新配置是生产服务器的基本要求。MediaServo 的 PluginManager 应该设计运行时配置重载能力（通过 `ConfigUpdate` trait 方法或信号处理）
- **版本演进**：
  - 2019-12：rtsp-simple-server v0.0.0 — 仅 RTSP 推流+拉流，Go 编写
  - 2020-02：v0.2.0 — 加入 RTMP 播放支持
  - 2021-05：v0.16.0 — 加入 HLS 播放
  - 2022-03：v0.18.0 — 加入 WebRTC 播放 (pion/webrtc)
  - 2023-06：v1.0.0 — 更名为 MediaMTX，架构全面重构
  - 2024-09：v1.15.0 — 加入录制功能 (fMP4/MPEG-TS)
  - 2025-11：v1.18.0 — 加入 WHIP 推流，Read Replica 模式
  - 2026-06：v1.19.0 — Media-over-QUIC 支持，FLAC 支持 (里程碑版本)
  - 2026-06：v1.19.2 — 最新稳定版，QUIC 稳定性改进
- **Docker 下载量**：超过 2 亿次拉取（来自 Docker Hub 公开数据），是流媒体领域部署最广泛的容器之一
- **赞助/依赖关系**：核心作者受雇于 Bluenviron 从事 MediaMTX 全职开发。所有协议库 (gortsplib/gortmplib/gohlslib/mediacommon) 由同一作者维护


### [Adapt] 需修改后采用
1. **Path 模型 + Fragment 模型融合**：MediaMTX 的 path 模型缺乏统一的内部媒体表示（内部只是 codec-aware ring buffer）。MediaServo 应该在 path 模型上叠加 Fragment 模型 — path 管理流生命周期和权限，Fragment 管理媒体数据表示和跨协议映射
2. **录制策略**：MediaMTX 的 fMP4/MPEG-TS 分段录制到本地磁盘。MediaServo 应该统一使用 CMAF/fMP4（与 Fragment payload 一致），录制目标支持多种后端（本地、S3/OSS、NFS），且直接从 FragmentObserver 写入
3. **外部转码的集成方式**：MediaMTX 的 `runOnReady` 钩子 + FFmpeg 子进程模式的功能性正确但运维复杂。MediaServo 应该设计 `Transcoder` trait，支持内建转码（首选）和外部转码（备选）两种后端，统一生命周期管理
4. **RPI Camera 配置统一的思路**：MediaMTX v2.8.0 统一软硬件编码参数的作法值得借鉴。MediaServo 的 `HardwareEncoder` trait 应该提供统一的配置接口，隐藏 NVENC/VAAPI/VT/QSV 的差异
5. **RTSP 隧道**：RTSP over HTTP/WebSocket 的防火墙穿透对监控场景有价值。MediaServo 的 `RtspPlugin` 应该支持多种 transport（TCP interleaved, UDP, HTTP tunnel）
6. **QUIC/WebTransport 传输层**：MediaMTX v1.19.0 验证了 QUIC 在流媒体中的可行性。MediaServo Phase 4+ 应该将 QUIC/WebTransport 作为统一传输层，减少协议数量

### [Avoid] 已知坑 / 不适用场景
1. **Go GC 低延迟风险**：毫秒级 GC 停顿对 <50ms 场景不可接受。MediaServo 的 Rust 核心没有 GC，这是正确的选择
2. **不内置转码的架构限制**：MediaMTX 完全依赖外部进程转码。MediaServo 需要内置 transmux 路径（Fragment 容器格式转换）和可选编码管线。不应依赖外部进程
3. **无 GPU 硬件编码能力**：纯 Go 无法调用 GPU API。MediaServo 的屏幕捕获和编码路径必须通过 Rust FFI 或 `libloading` 直接调用 NVENC/VAAPI/VT
4. **Read Replica 无故障转移**：无自动 failover。MediaServo 如果需要高可用，需要设计更完善的集群方案（参考 LVQR 的 gossip 集群）
5. **单 publisher 限制**：一个 path 仅一个 publisher。MediaServo 的视频会议需要多 publisher → audiomixer/compositor 后输出。需要在 path 模型上增加 room/conference 抽象
6. **gortsplib 等依赖库的 bus factor**：MediaMTX 的所有协议库都由一人维护。如果作者停止维护，整个生态崩溃

**总体评分**：★★★☆☆ (3/5)

MediaMTX 是协议路由 reference — 在零配置协议转换、IoT/监控边缘场景、QUIC 传输方面表现卓越。Path 抽象和 sourceOnDemand 模式对 MediaServo 有直接参考价值。但 Go GC 限制、无转码能力、无 GPU 编码能力使其实现方式不适合 MediaServo 的直接需求。取其设计思想（Path 管理 + 自动转换 + on-demand 拉流），用 Rust + Fragment Model 重新实现。

---
**相关决策**: D5 (Unified Fragment), D21 (时间戳), D-STREAM-TOPOLOGY, D152 (nginx hooks)

## 附录 A: sourceOnDemand 模式详解

### A.1 状态机

sourceOnDemand 模式的核心是一个带超时的状态机：

```
  set sourceOnDemand = yes
                          
    Idle  viewer连接  Connecting  拉流中  Active
   (释放) ----------->  (连接上游) -------> (推流中)
       A                                      |
       |    viewer 断开 + 超时后              |
       +--------------------------------------+
```

**状态说明**：
- Idle：无 viewer，无上游连接。摄像头可进入低功耗模式
- Connecting：第一个 viewer 到达，正在连接上游源
- Active：viewer 存在，上游连接活跃，正在推送媒体数据
- 返回 Idle：viewer 全部断开，等待 onDemandCloseAfter 超时后释放

**防抖机制**：
- onDemandCloseAfter 默认 10s。避免 viewer 短暂断开立即重连
- viewer 在超时前重连则回到 Active 状态，不释放连接
- 超时后释放，摄像头可进入休眠

### A.2 对摄像头的影响

电池供电的 IP 摄像头场景：
- Idle 状态：摄像头完全停止编码和传输，功耗降至待机模式
- Active 状态：摄像头正常编码和传输，正常工作功耗
- 如果监控场景是偶尔有人看，sourceOnDemand 可节省 95%+ 电力

### A.3 配置示例

```yaml
paths:
  front_door:
    source: rtsp://192.168.1.100:554/stream
    sourceOnDemand: yes
    sourceOnDemandCloseAfter: 30s  # 最后 viewer 离开后 30s 释放
    sourceOnDemandRetryInterval: 5s  # 重连间隔
```


### [Adopt] 补充 — 热重载配置设计

**6. 运行时配置热重载**：MediaMTX 支持通过 SIGHUP 信号或 POST API 热重载 YAML 配置文件，不中断已有连接。MediaServo 的 `PluginManager` 需要类似的机制：

```rust
pub trait ConfigurablePlugin: Send + Sync {
    /// 应用新的配置。返回变更的配置差异
    async fn apply_config(&self, new: &serde_json::Value) -> Result<ConfigDiff>;
    /// 当前配置快照
    async fn current_config(&self) -> Result<serde_json::Value>;
}
```

热重载的边界的处理：端口变更需要重启 listener、编解码器变更不会影响已有编码会话、path 配置变更立即生效但不会踢出已有 viewer。MediaServo 的 PipelineEngine 应在 reload_config() 时遍历所有插件的 apply_config() 方法。

### [Adapt] 补充 — 认证与授权模型

**7. 多级认证机制**：MediaMTX 支持 per-path 的 external/RTSP Digest/JWT/internal 认证，以及 IP 白名单。MediaServo 的认证模型应更简洁：

| 认证级别 | MediaMTX 方式 | MediaServo 方式 | 差异说明 |
|---------|-------------|---------------|---------|
| Level 0 | 无认证 (noauth) | `AuthMode::None` | 内部测试、开发环境 |
| Level 1 | 静态密码 (internal) | `AuthMode::Static` | 单 Token/密码，简单部署 |
| Level 2 | HTTP 回调 (external) | `AuthMode::Webhook` | 外部认证服务，支持 RBAC |
| Level 3 | JWT | `AuthMode::Jwt` | 无状态令牌，适合微服务 |
| Level 4 | 多因子 + IP ACL | `AuthMode::Composite` | 安全敏感场景 |

MediaServo 应采用 `AuthMode` 枚举 + `AuthBackend` trait，允许插件实现自定义认证后端（LDAP/OAuth/OIDC）。

### [Avoid] 补充 — 更多已知坑

**7. QUIC 传输的可达性问题**：MediaMTX 的 Media-over-QUIC 需要客户端支持 HTTP/3 (WebTransport API)。在企业网络中，QUIC (UDP 443) 可能被防火墙封锁（允许 TLS/TCP 443 但不允许 UDP 443）。MediaServo 如果采用 QUIC 传输，必须保留 TCP/Long-Polling/WebSocket 作为回退传输层。

**8. 单 path 单 publisher 的局限性**：MediaMTX 一个 path 只允许一个 publisher。这适合监控摄像头和通用推流，但不适合视频会议（需要多方合成）。MediaServo 需要在 Path 模型上叠加 Room/Conference 模型 — Room 管理多 publisher 的音频混音和视频合成后成为单一的 Fragment 流输出。


对 MediaServo 的启示：
- mediaservo-surveillance SDK 的 CameraSource trait 应支持 onDemand: bool
- start() 方法是拉流动作，stop() 是释放动作
- PipelineEngine 需要感知 viewer 连接数，触发 start/stop
- 防抖超时需要可配置

---

## 附录 B: Read Replica 架构详解

### B.1 拓扑结构

```
               Load Balancer
              (L4 TCP / L7 HTTP)
                    |
          +---------+---------+
          |                   |
      Replica 1           Replica 2
       (只读)              (只读)
      从Origin拉流        从Origin拉流
          |                   |
          +---------+---------+
                    |
                  Origin
            (接收推流 + 录制 + API)
```

### B.2 配置示例

**Origin (接收推流)**：
```yaml
paths:
  live_stream:
    # Publisher 推到这里
```

**Replica (分发观看)**：
```yaml
paths:
  live_stream:
    source: rtsp://origin-ip:8554/live_stream  # 从Origin拉流
    sourceOnDemand: yes  # 可选
```

### B.3 限制与适用场景

**限制**：
- 手动配置所有 path (无自动同步)
- 无自动故障转移：Origin 宕机则所有 Replica 断流
- 无自动服务发现：需手动更新 LB 配置
- Replica 仅分发阅读流量 (无推流/录制功能)

对 MediaServo 的启示：
- Phase 1-2 可采用 Read Replica 作为简单水平扩展方案
- Phase 3+ 升级到 LVQR gossip 集群 (自动发现 + 故障转移)
- mediaservo-cluster 应实现两个后端 trait 支持渐进式升级

---

## 附录 C: MediaMTX 配置核心参数

MediaMTX 的配置文件结构：

```yaml
logLevel: info
rtspAddress: :8554
rtmpAddress: :1935
hlsAddress: :8888
webrtcAddress: :8889
srtAddress: :8890

paths:
  # Publisher 模式
  live:
    runOnReady: ffmpeg -i rtsp://localhost:$RTSP_PORT/$MTX_PATH ...
    runOnReadyRestart: yes

  # Static Source 模式
  ip_camera:
    source: rtsp://admin:password@192.168.1.64:554/stream
    sourceProtocol: tcp
    sourceOnDemand: no

  # SourceOnDemand 模式
  battery_camera:
    source: rtsp://192.168.1.65:554/stream
    sourceOnDemand: yes
    sourceOnDemandCloseAfter: 1m

  # 录制配置
  recorded_stream:
    record: yes
    recordPath: /recordings/$MTX_PATH/%Y-%m-%d_%H-%M-%S
    recordFormat: fmp4
    recordSegmentDuration: 1h
```

核心参数说明：
- sourceProtocol: tcp 强制 TCP 传输，兼容某些摄像头的 UDP 问题
- runOnReady 钩子在 path 就绪时执行 shell 命令，常用于触发转码
- recordSegmentDuration 录制分段时间，避免单文件过大
- sourceOnDemand + sourceOnDemandCloseAfter 组合实现节能模式

对 MediaServo 的启示：
- 配置格式应考虑 YAML (人类友好) 而非 INI (SRS) 或 TOML (Xiu)
- runOnReady 钩子模式是简单场景实用方案，生产应用 trait 抽象
- 基础录制配置 (路径/格式/分段时间) 是 MVP 的一部分
- 协议端口应可禁用 (仅启用需要的协议)，减少攻击面