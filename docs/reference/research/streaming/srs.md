# SRS 参考分析
> 生成日期：2026-07-16 | 分类：流媒体

## 1. 产品画像
- **名称**：SRS (Simple Realtime Server)，品牌名为 SRS/8.0（2026年5月起）
- **开发者**：OSSRS 开源社区。核心维护者 winlinvip（杨成立，全职投入 SRS 开发超过 10 年）及 120+ 贡献者。社区包括大量的中文互联网开发者
- **首次发布**：2013年（v1.0），持续开发超过 13 年。SRS/8.0 为当前主版本（2026-05-17 发布），是自 2013 年来第 8 个大版本
- **产品定位**：简单高效的实时视频服务器。从一个二进制同时提供 RTMP/WebRTC/HLS/HTTP-FLV/HTTP-TS/SRT/MPEG-DASH/GB28181 的多协议支持。"简单"是核心哲学 — 无论是部署（一条命令）、配置（INI 文件）、还是使用（HTTP API + 内建播放器）。同时支持高性能（协程并发、transmux-only、零转码）和低延迟（WebRTC 80ms、SRT 0.5s）
- **目标用户群体**：直播平台开发者（中文互联网直播公司大量使用 — 斗鱼/虎牙/快手/抖音等），需要 RTMP 兼容性的传统流媒体架构师，自建小型直播方案（教育/企业/安防），视频监控系统集成商（GB28181 设备接入）
- **许可 / 商业模式**：MIT 许可（SRS/8.0 "code Free" 版本）。核心代码完全免费开源。通过 Oryx (SRS Stack) 管理平台提供服务盈利（Docker 一体化部署 + Web UI），提供商业技术支持。赞助商包括声网 (Agora) 等

## 2. 技术特性
### 整体架构
```
┌──────────────────────────────────────────────────────────────────┐
│                         SRS (C/C++ 协程架构)                       │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐     │
│  │               ST (State Threads) 协程调度器                │     │
│  │                                                           │     │
│  │  单进程，多协程。每个网络连接绑定一个 ST 协程              │     │
│  │  同步编程模型（无回调嵌套）。用户态线程，百万级并发        │     │
│  │  Linux/macOS/ARM/RISCV/LOONGARCH/MIPS 全架构支持          │     │
│  └──────────────────────────────────────────────────────────┘     │
│                                                                   │
│  ┌───────────────────────────────────────────────────────┐        │
│  │              内部统一数据路径：RTMP 流 ← 核心设计       │        │
│  │                                                       │        │
│  │  所有输入协议 ──→ 内部 RTMP 流 ──→ 所有输出协议        │        │
│  │                                                       │        │
│  │  RTMP (推流) ──┐                                      │        │
│  │  WebRTC (推流) ─┤        ┌───────────┐               │        │
│  │  SRT (推流)    ─┼───────▶│ 内部 RTMP  ├───────────┐  │        │
│  │  GB28181 (推)  ─┘        └───────────┘           │  │        │
│  │                                                  ▼  │        │
│  │                             ┌──────────────────────┐ │        │
│  │                             │  transmux (零转码)    │ │        │
│  │                             │                      │ │        │
│  │                             │ RTMP → HLS           │ │        │
│  │                             │ (解 FLV → 封 TS)     │ │        │
│  │                             │ RTMP → HTTP-FLV      │ │        │
│  │                             │ (零开销，直接转发)    │ │        │
│  │                             │ RTMP → DASH          │ │        │
│  │                             │ (解 FLV → 封 fMP4)   │ │        │
│  │                             │ RTMP → SRT           │ │        │
│  │                             │ (解 FLV → 封 MPEG-TS)│ │        │
│  │                             │ WebRTC → RTMP         │ │        │
│  │                             │ (解 RTP → 封 FLV)    │ │        │
│  │                             └──────────────────────┘ │        │
│  └───────────────────────────────────────────────────────┘        │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐     │
│  │  协议输入层                                               │     │
│  │  ┌────────┐ ┌──────────┐ ┌───────┐ ┌─────────┐          │     │
│  │  │  RTMP  │ │  WebRTC  │ │  SRT  │ │ GB28181 │          │     │
│  │  │ TCP    │ │WHIP/WHEP │ │  UDP  │ │SIP+RTP  │          │     │
│  │  │ 1935   │ │DTLS-SRTP │ │       │ │         │          │     │
│  │  │        │ │ICE-Lite  │ │Go独立  │ │         │          │     │
│  │  │        │ │需外部TURN│ │进程    │ │         │          │     │
│  │  └────────┘ └──────────┘ └───────┘ └─────────┘          │     │
│  └──────────────────────────────────────────────────────────┘     │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐     │
│  │  协议输出层（全部从内部 RTMP 流 transmux 生成）            │     │
│  │  ┌──────┐ ┌──────┐ ┌────-┐ ┌──────┐ ┌────-┐ ┌────┐     │     │
│  │  │ RTMP │ │ HLS  │ │HTTP-│ │WebRTC│ │DASH │ │SRT │     │     │
│  │  │ 播放 │ │.m3u8│ │ FLV │ │WHEP  │ │.mpd │ │播放│     │     │
│  │  │      │ │ +.ts │ │HTTP │ │RTP   │ │     │ │    │     │     │
│  │  │ TCP  │ │      │ │长连接│ │UDP   │ │HTTP │ │UDP │     │     │
│  │  └──────┘ └──────┘ └────┘ └──────┘ └────┘ └────┘     │     │
│  └──────────────────────────────────────────────────────────┘     │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐     │
│  │  Origin-Edge 集群                                          │     │
│  │                                                           │     │
│  │  推流 → Origin ──┬──► Edge 1 ──► 观众 (RTMP/FLV/WebRTC)  │     │
│  │                 ├──► Edge 2 ──► 观众                       │     │
│  │                 └──► Edge N ──► 观众                       │     │
│  │                                                           │     │
│  │  多级边缘节点（Edge-of-Edge）。RTMP/HTTP-FLV/WebRTC 分发 │     │
│  └──────────────────────────────────────────────────────────┘     │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐     │
│  │  HTTP Callback 事件系统                                    │     │
│  │  on_publish · on_unpublish · on_play · on_stop           │     │
│  │  on_dvr · on_hls · on_rtc_play                            │     │
│  │  → HTTP POST 到外部服务 → 认证/计费/通知/自动转码触发      │     │
│  └──────────────────────────────────────────────────────────┘     │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐     │
│  │  可观测性                                                   │     │
│  │  Prometheus metrics exporter · Grafana Dashboard          │     │
│  │  HTTP API (运行时控制) · 内建 WebRTC/HLS 播放器控制台     │     │
│  │  Docker Hub · Helm Chart (Kubernetes)                     │     │
│  └──────────────────────────────────────────────────────────┘     │
│                                                                   │
│  SRS/8.0 特殊说明：MIT 许可，code Free，无额外收费特性             │
└──────────────────────────────────────────────────────────────────┘
```

### 内部 RTMP 归一化模型

SRS 最核心的设计决策是将所有输入协议归一化到内部的 RTMP 流：

**归一化流程**：
1. **RTMP 推流** → 直接解析 FLV tag → 内部 RTMP 流（零转换）
2. **WebRTC WHIP 推流** → 解 RTP → 解码 H.264/Opus → 重新编码 FLV → 内部 RTMP 流（有编解码开销！）
3. **SRT 推流** → (SRT 独立进程) → FLV → TCP 管道 → 内部 RTMP 流（跨进程桥接）
4. **GB28181 推流** → 解 PS/RTP → 提取 H.264/AAC → 封 FLV → 内部 RTMP 流

**归一化后的 transmux**：
- RTMP → HTTP-FLV：零开销（FLV tag 原样转发），最低延迟 (1-3s)
- RTMP → HLS：解 FLV → 编码为 TS segment + 生成 m3u8 playlist，延迟 3-10s (标准) / 2-3s (LL-HLS)
- RTMP → DASH：解 FLV → 编码为 fMP4 segment + 生成 MPD manifest，延迟 3-10s
- RTMP → SRT：解 FLV → 封 MPEG-TS → SRT 推流，延迟 0.5-2s
- RTMP → WebRTC (WHEP)：解 FLV → 解编码 → 重新 RTP 打包 → WebRTC 播放（有编解码开销）

### 关键技术能力
| 能力 | 详情 |
|------|------|
| 协议支持 | 输入：RTMP/RTMPS (TCP 1935), WebRTC/WHIP (UDP/TCP), SRT (UDP, 独立进程), GB28181 (SIP+RTP)。输出：RTMP, WebRTC/WHEP, HLS (m3u8+ts), HTTP-FLV, HTTP-TS, MPEG-DASH (MPD+fMP4), SRT |
| 编码 | H.264, H.265(HEVC) — Enhanced RTMP, AV1 — Enhanced RTMP, VP9/VP8 — WebRTC。音频：AAC, Opus, G.711 (PCMA/PCMU), MP3 |
| 传输 | RTMP: TCP (1935) + TLS (RTMPS)；SRT: UDP + AES-128/256；WebRTC: UDP/DTLS-SRTP + ICE-Lite + 外部 TURN；HLS: HTTP/HTTPS；HTTP-FLV: HTTP 长连接；DASH: HTTP |
| 转换 | **transmux only（核心设计哲学）**：RTMP → HLS / HTTP-FLV / DASH / SRT。仅协议容器格式转换，不解码/编码。CPU 开销极低 |
| 录制 | FLV 格式录制到磁盘。按流和按时间分段。HTTP 回调 on_dvr 通知录制段创建。DVR 支持滑动窗口 |
| 安全 | RTMP token 认证（`?token=<md5_hash>` URL 参数），HTTP 回调认证，WebRTC DTLS-SRTP 加密，SRT AES-128/256 加密 |
| 转码 | **不内置转码**。依赖外部 FFmpeg 进程（通过 HTTP 回调 on_publish 触发）。接收转码流作为不同的流名 |
| LL-HLS | 低延迟 HLS，segment 时间 1-2s，延迟 2-3s（vs 标准 HLS 3-10s） |

### 性能指标
| 测试项 | 数据 | 条件 |
|--------|------|------|
| RTMP 延迟 | 0.4-3s | H.264 only: 0.4s; H.264+AAC: 0.6s; 标准: 0.8-3s |
| WebRTC 延迟 | 80-400ms | 优化后 80ms (SRS 4.0.87)；SRS 2.0.72 基准测试 80ms |
| HLS 延迟 | 3-10s (标准) / 2-3s (LL-HLS) | Segment 时长 1-6s |
| HTTP-FLV 延迟 | 1-3s | HTTP 长连接，FLV tag 直通 |
| RTMP 播放并发 | 数千路 | 单机 4 核，无转码 |
| WebRTC 播放并发 | 500-1000+ | 2 vCPU，需外部 TURN |
| HLS 最优 fragment | 3s | 延迟与稳定性的平衡点 |
| CPU 占用 | 极低 (transmux only) | 单核可承载数百 RTMP 连接 |
| 代码行数 | 约 170,000 行 | SRS v6.0-r0 |

### 协议对比深度分析 — SRS vs Fragment Model

SRS 的协议转换矩阵以 RTMP 流为中间表示，与非 RTMP 协议的交互存在以下性能特征：

| 协议路径 | SRS 处理方式 | 编解码开销 | 延迟范围 | AUDEMSP Fragment 处理方式 | 延迟改善 |
|---------|-------------|:---------:|:-------:|---------------------------|:--------:|
| RTMP→HLS | FLV→TS transmux | 无 | 3-10s | Fragment→CMAF→LL-HLS | 改善50% (1-3s) |
| RTMP→HTTP-FLV | FLV 直通 | 无 | 1-3s | Fragment→FLV tag 生成 | 持平 |
| RTMP→WHEP | FLV→解码→RTP | **有** (丢失质量) | 200-500ms | Fragment→RTP 打包 (passthrough) | **改善**(无质量损失) |
| WHIP→RTMP | RTP→解码→FLV | **有** (重复编码) | 200-500ms | Fragment 直通 (回到 WHIP) | **改善**(无编码) |
| WHIP→WHEP | RTP→FLV→解码→RTP | **有** (2次编解码) | 300-800ms | Fragment→RTP 直通 | **改善90%** |
| SRT→HLS | MPEG-TS→FLV→TS | 无 (双容器转换) | 3-10s | Fragment→CMAF→HLS | 改善 (单步) |
| GB28181→HLS | PS→FLV→TS | 无 | 3-10s | PS→Fragment→CMAF→HLS | 改善 (单步) |

**关键发现**：
1. WHIP→WHEP 在 SRS 中存在 2 次编解码（原始最优质量已丢失），是 SRS 架构最大的性能痛点
2. Fragment Model 中 WHIP→WHEP 是纯 RTP 直通路径，没有任何编解码操作
3. SRT→HLS 在 SRS 中经过 MPEG-TS→FLV→TS 两步容器转换，Fragment Model 只需 Fragment→CMAF 一步

### 并发性能与资源消耗对比

| 场景 | SRS (RTMP 归一化) | LVQR (Fragment Model) | 差异 |
|------|-----------------|----------------------|------|
| 1000 路 RTMP→HLS | 1000 条 transmux 通道 (零 CPU) | 1000 条 CMAF 生成通道 (零 CPU) | 相近 |
| 100 路 WHIP→WHEP | 100 路编解码 (100% CPU+GPU) | 100 路 Fragment 直通 (<1% CPU) | SRS 高出 100x |
| 500 路 RTMP→HTTP-FLV | 500 条 FLV 直通 (零 CPU) | 500 条 FLV tag 生成 (零 CPU) | 相近 |
| 添加 RTMP→MoQ 支持 | 需要 FLV→MoQ Object 适配 (复杂) | 已支持 (Fragment→MoQ) | Fragment 优势 |
| GPU 需求 | WHIP 转 RTMP 必需 GPU | 不需要 (零编解码) | Fragment 优势 |


### 技术栈
- **语言分布**：C/C++ (87.5% — 核心服务器)，Go (5.5% — SRSX 代理模块、部分工具)，JavaScript (3.0% — WebRTC 播放器 srsRTCPlayer.js、HLS/FLV 播放器 srsPlayer.js、管理控制台)，Shell/Python/Lua (运维脚本)
- **协程框架**：ST (State Threads) — 轻量级用户态协程库。2001年 Netscape 开发，单进程多协程模型。每个网络连接一个协程。上下文切换 ≈ 函数调用成本。无需异步编程框架。缺点：非标准库，无原生调试支持，社区较小
- **内部依赖**：自研为主，最小化外部依赖。
  - RTMP 协议栈：完整的自研实现（13 年打磨）。Enhanced RTMP 支持（HEVC/AV1/多轨）。FLV 解析和生成
  - WebRTC 模块：自研 RTC 模块。DTLS-SRTP 加密 (基于 OpenSSL/mbedTLS)。ICE-Lite（服务器端简化 ICE，无需 STUN 协商）。无内置 TURN/STUN 服务
  - SRT：独立 Go 进程模块（hybrid model），通过内部 TCP 管道与主进程通信，传递 FLV 流。使用 libsrt C 库
  - GB28181：自研 SIP 信令 + RTP/PS 解包模块
- **配置系统**：INI 格式配置文件 (`conf/srs.conf`)。Per-vhost 和 per-app 配置隔离。支持 include 指令引用多个配置文件
- **监控与运维**：Prometheus metrics exporter (HTTP `/metrics`)，Grafana JSON 仪表板模板，HTTP API 运行时状态查询和控制，内建 WebRTC/HLS/HTTP-FLV 播放器（测试和演示），容器化部署（Docker Hub + Helm Chart）
- **多架构支持**：x86_64, ARMv7 (32-bit ARM), AARCH64 (ARM64/Apple M1/M2), RISCV (RISC-V 64-bit), LOONGARCH (龙芯), MIPS (MIPS64)。国产 CPU 适配是其中国市场的重要竞争力
- **客户端生态**：srsRTCPlayer.js (WebRTC 播放器，GitHub releases 提供)，srsPlayer.js (HTTP-FLV/HLS 播放器)，社区 SDK (Java/C#/Python/Go)，Oryx/SRS Stack (Docker 一体化管理平台 + Web UI)

## 3. 功能概览
### 核心功能模块
| 模块 | 功能 | 实现语言 |
|------|------|----------|
| RTMP Core | 完整 RTMP/RTMPS 实现。推流 + 播放。Enhanced RTMP：H.265/AV1/多轨。GOP cache：缓存最近 GOP，新观众秒开画面。FLV tag 直通（HTTP-FLV 输出） | C/C++ |
| WebRTC | WHIP 推流 + WHEP 播放。DTLS-SRTP。ICE-Lite（简化 ICE）。需外部 TURN 做 NAT 穿透。延迟 80-400ms | C/C++ |
| HLS | 实时 HLS 生成：FLV → TS segment + m3u8 playlist。标准 HLS (3-10s) + LL-HLS (2-3s)。HLS over HTTPS | C/C++ |
| HTTP-FLV | HTTP 长连接 FLV 流分发。延迟 1-3s。零开销（FLV tag 直通）。不支持 iOS Safari | C/C++ |
| DASH | FLV → fMP4 segment + MPD manifest 生成。ABR 多码率（手动配置多流 MPD）。延迟接近 HLS | C/C++ |
| SRT | 独立 Go 进程。内部 TCP 管道连接主进程。支持推流和播放。AES 加密。延迟 0.5-2s | Go (独立进程) |
| GB28181 | 国标 GB/T 28181 协议。SIP 信令（注册/心跳/Invite）+ RTP/PS 推流。安防摄像头接入。唯一开源实现 | C/C++ |
| Origin-Edge Cluster | Origin 推流 → Edge 分发。多级 Edge。RTMP/HTTP-FLV/WebRTC 均支持。配置简单 | C/C++ |
| HTTP Callback | 流生命周期事件通知。on_publish/unpublish/play/stop/dvr/hls。HTTP POST 调用外部服务。认证/计费/转码触发 | C/C++ |
| Transmux | 纯容器格式转换。不解码/编码。RTMP → HLS/DASH/HTTP-FLV/SRT。极低 CPU 开销 | C/C++ |
| Security | RTMP token 认证。HTTP 回调认证。DTLS-SRTP (WebRTC)。AES 加密 (SRT)。Per-vhost/app 认证规则 | C/C++ |
| Monitoring | Prometheus metrics。HTTP API。内建 WebRTC/HLS 播放器调试工具。Grafana Dashboard | C/C++ + JS |

### 特色功能
- **RTMP 协议的深度实现**：13 年持续打磨的 RTMP 协议栈。Enhanced RTMP 支持 HEVC/AV1 编码和多音视频轨道。OBS Studio 将 SRS 作为 RTMP 兼容性测试基准。这个深厚积累是长期投入的结果
- **ST 协程架构的简洁性**：单进程多协程。同步编程模型（无 async/await 关键字，无 Future/Promise）。每个连接一个协程，代码像写同步一样简单，但底层是高效的协程调度。在 C/C++ 生态中独树一帜
- **Transmux 零转码哲学**：不解码/编码，仅做容器格式转换。CPU 开销极低（单机 4 核可承载数千 RTMP 并发）。这一决策在不需要 ABR 自适应码率的场景下是最优架构
- **GB28181 国标唯一开源支持**：中国安防监控行业标准协议的原生开源实现。SIP 信令 + RTP/PS 媒体传输。对 GB28181 设备（中国安防摄像头）的开箱即用支持。这是 SRS 在中国安防监控市场的独特竞争力
- **中文社区生态全面**：完整的中文文档、教程、Wiki、GitHub Issues（中文友好）、微信群。对于中文用户，SRS 是上手门槛最低的开源流媒体服务器。Oryx (SRS Stack) 提供了一键部署的 Web 管理界面
- **多架构 CPU 支持**：x86_64, ARMv7, AARCH64 (Apple M1/M2), RISCV, LOONGARCH (龙芯), MIPS。国产 CPU 适配使其在中国信创市场有独特优势

### 扩展性 / 插件机制
SRS 没有官方插件系统。扩展方式：
- **HTTP Callback 事件系统**：定义了一组流生命周期事件（on_publish, on_unpublish, on_play, on_stop, on_dvr, on_hls, on_rtc_play）。事件发生时 HTTP POST 调用配置 URL。用于：
  - 认证：on_publish/on_play 回调中验证 token 和权限，返回 HTTP 状态码控制允许/拒绝
  - 计费：on_stop 回调中记录观看时长
  - 通知：on_dvr 回调中通知录制完成
  - 自动转码：on_publish 回调触发 FFmpeg 子进程做 ABR 转码
- **外部 FFmpeg 转码**：通过 HTTP 回调 on_publish 自动启动 FFmpeg 进程。FFmpeg 输出新的 RTMP 流（如 `stream_720p`, `stream_480p`）。SRS 接收这些流作为独立的 path。然后手动配置 HLS master playlist 或 DASH MPD 包含多码率
- **Origin-Edge 集群**：通过配置文件指定 Edge 节点的上游 Origin。Edge 节点启动时连接 Origin 并拉流。动态流分配（观众请求时 Edge 才从 Origin 拉取该流）
- **HTTP API**：运行时查询（流列表、客户端列表、服务器状态、统计信息），运行时控制（踢出发布者/播放者），配置查询
- **SRT 混合模块（hybrid model）**：SRT 作为独立 Go 进程运行。通过内部 TCP 连接与 C/C++ 主进程通信。这是历史遗留设计，不是推荐的扩展模式

## 4. 现状与生态
- **当前版本**：SRS/8.0（2026-05-17，code Free）。此前的最新 tagged release 是 v6.0-r0（2025-12-03，170,962 行代码）。自 v6.0 后不再使用语义化版本号，改为品牌版本
- **GitHub Stars / 活跃度**：约 29,000 stars，5,600+ forks。持续高活跃度，120+ 贡献者。Open Issues 约 45 个。核心维护者 winlinvip 全职投入 SRS 开发超过 10 年。Release 周期：v5.x 约每季度一个 release，v6.x 约每月一个 alpha/beta
- **社区规模**：可能是中文互联网最活跃的开源流媒体社区。中文文档完善（Wiki + 官方文档站 ossrs.io/ossrs.net）。英文文档也持续完善。Reddit、GitHub Issues/Discussions 活跃。微信社群。Oryx 开发者社区
- **文档 / SDK / API 生态**：
  - 官方网站：ossrs.io (国际站), ossrs.net (中国站)。完整的中英文文档
  - Wiki: GitHub Wiki 包含详细的架构文档、配置指南、性能测试报告、FAQ
  - HTTP API 文档：完整的 REST API 参考
  - 配置文档：每个参数有详细说明。示例配置文件丰富
  - 客户端 SDK：srsRTCPlayer.js (WebRTC 播放器)，srsPlayer.js (HTTP-FLV/HLS 播放器)。社区贡献 Java/C#/Python/Go SDK
  - 播放器兼容：FFmpeg (推流/拉流)，OBS Studio (推流)，VLC (RTMP/HLS 播放)，browser WebRTC (WHEP 播放)
  - 运维工具：Prometheus + Grafana 监控模板，Docker Compose (docker-compose.yml)，Helm Chart (Kubernetes)
  - Oryx (SRS Stack)：一体化管理平台。Docker 一键部署。Web UI 管理界面。支持 docker run 一行命令启动完整直播平台
- **已知缺陷或限制**：
  - **内部模型固定为 RTMP 流（最大架构限制）**：所有输入归一化到 RTMP，所有输出从 RTMP 派生。添加新协议（如 MoQ/QUIC 的 subgroup 和 object 概念）需要扭曲适配 RTMP 语义。这是 SRS 的阿喀琉斯之踵
  - **不支持 QUIC/Media-over-QUIC**：MediaMTX v1.19.0 已支持，SRS 无计划。RTMP 中心架构难以适配 QUIC 的多路复用和对象模型
  - **无内置 TURN/STUN**：WebRTC 的 NAT 穿透完全依赖外部 coturn 服务器。增加了部署复杂度和故障点
  - **SRT 混合进程模型**：独立 Go 进程 + TCP 管道桥接增加了延迟、维护复杂度和故障点
  - **集群管理无 GUI**：Origin-Edge 配置全部在配置文件中手动指定。无图形化管理或动态编排
  - **C 代码基础老化**：核心代码始于 2013 年，17 万行 C/C++。技术债务积累。新人入手难度高
  - **HLS 延迟较高**：LL-HLS 的 2-3s 延迟在 WebRTC 时代（<100ms）显得较慢
  - **WebRTC 推流转 RTMP 有编解码开销**：WHIP 推流（H.264/Opus）→ 解码 → 重新编码为 FLV（H.264/AAC）→ RTMP。破坏了纯 transmux 的性能优势
  - **ABR 配置复杂**：多码率分发需要手动配置 FFmpeg 转码 + 手动编辑 HLS master playlist。无自动化 ABR ladder

## 5. 市场定位
- **主要应用行业**：中文互联网直播 — 斗鱼/虎牙/快手/抖音/B站等平台大量使用 SRS 作为边缘/中转服务器（最大用户群体），在线教育直播 — 网课平台实时互动，企业直播/培训 — 内部培训直播，安防监控 — GB28181 摄像头接入和流分发，海外中小型直播平台 — RTMP/HLS/LHLS 分发
- **竞品对比简表**：
| 维度 | SRS | MediaMTX | nginx-rtmp | Ant Media | Wowza | LiveKit |
|------|-----|----------|------------|-----------|-------|---------|
| 核心语言 | C/C++ | Go | C | Java | Java | Go/TS |
| 协议数 | 8 | 7 | 3 | 6+ | 10+ | 1 (WebRTC) |
| 内部模型 | **RTMP 归一化** | Path + 无统一模型 | 无统一模型 | RTMP 归一化 | 多格式 | Room/Track |
| 内置转码 | ❌ | ❌ | ❌ | ✅ (H.264) | ✅ (多格式) | ✅ |
| MoQ/QUIC | ❌ | ✅ v1.19 | ❌ | ❌ | ❌ | ❌ |
| 集群 | Origin-Edge | Read Replica | push/pull | 内置集群 | 内置集群 | 内置 SFU |
| TURN 内置 | ❌ (需 coturn) | ❌ | ❌ | ✅ | ✅ | ✅ |
| GB28181 | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| RTMP 深度 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ❌ |
| HLS 优化 | ⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐ | ❌ |
| Stars | 29K | 20K | 14K | 3K | N/A (闭源) | 30K+ |
| 中文支持 | ⭐⭐⭐⭐⭐ | ⭐ | ⭐ | ⭐ | ⭐ | ⭐ |
| 许可 | MIT | MIT | BSD-2 | 商业 | 商业 | Apache 2.0 |
| 发布年份 | 2013 | 2019 | 2012 | 2018 | 2007 | 2021 |
| 代码规模 | 17万行 | ~5万行 | ~3万行 | ~20万行 | ~50万行 | ~10万行 |
- **定价 / 许可**：MIT 免费开源（SRS/8.0 code Free）。Oryx (SRS Stack) 平台有免费版和付费版（高级特性如多用户管理、录制管理、统计面板）。企业可购买商业技术支持

## 6. 产品特色
1. **开源 RTMP 协议实现的巅峰**：13 年持续打磨。Enhanced RTMP (HEVC/AV1/多轨) 领先业界。OBS Studio 作为兼容性基准。17 万行 C/C++ 代码承载了 RTMP 时代的所有经验教训
2. **ST 协程架构的工程实践**：单进程多协程、同步编程模型避免了 C/C++ 异步编程的复杂性。2013 年就实现了如今 async/await 的用户体验，但用的是 2001 年的协程库
3. **Transmux 零转码架构**：不解码/编码，仅容器格式转换。单机 4 核可承载数千 RTMP 并发。CPU 占用极低。不需要 GPU。这是简单场景下的最优架构
4. **GB28181 国标唯一开源支持**：中国安防监控行业的国家标准协议。SIP 信令 + RTP/PS 媒体传输。中国安防摄像头接入的标准开源方案
5. **中文社区生态全面**：可能是全球中文文档最完善的开源流媒体服务器。完整的中文 Wiki、博客、微信群。Oryx 一键部署的 Web UI。零门槛上手
- **版本演进**：
  - 2013 年：SRS v1.0 — 仅 RTMP，C 语言，ST 协程架构确立
  - 2015 年：SRS v2.0 — HLS 支持，HTTP Callback，DVR 录制
  - 2017 年：SRS v3.0 — HTTP-FLV，多进程架构，DASH 支持
  - 2020 年：SRS v4.0 — WebRTC (WHIP/WHEP) 支持 (重大里程碑)，SRT 支持 (Go 独立进程)
  - 2021 年：SRS v5.0 — H.265/AV1 Enhanced RTMP，LL-HLS，GB28181 (安防)
  - 2023 年：SRS v6.0 — Centrifuge 级联集群，SRT 推流，WebTransport 初步
  - 2025 年：SRS v6.0-r0 — 170,962 行代码，最后语义化版本
  - 2026-05：SRS/8.0 — 品牌更名，MIT 许可 "code Free" 版本
- **贡献者结构**：120+ 贡献者，核心维护者 winlinvip 全职开发超 10 年。公司赞助包括声网 (Agora) 等 Chinese 实时音视频公司
- **中国市场占有率**：中文互联网直播场景中 RTMP 服务器市场的约 30-40% (预估)，Oryx (SRS Stack) Docker 镜像下载量数百万次


## 7. 对 AUDEMSP 的参考价值
### [Adopt] 可直接借鉴
1. **Transmux 零转码快速路径**：AUDEMSP 应该支持"快速 transmux 路径"：当 Fragment payload 是 fMP4/CMAF 时，输出 HLS/DASH/HTTP-FLV 直接复用 payload 或仅做容器格式转换，不解码/编码。这是性能最优的路径。当需要 ABR 时才启动编码管线
2. **HTTP Callback 事件系统**：`on_publish`/`on_play`/`on_unpublish`/`on_stop`/`on_dvr` 的流生命周期回调模式简洁实用。AUDEMSP 的 `StreamingSDK` 需要相同的钩子系统用于认证、计费、通知、自动化转码触发
3. **Origin-Edge 集群拓扑**：多级边缘节点经大规模验证的集群模型。适合 CDN 边缘分发场景。AUDEMSP 的 `ClusterPlugin` 在初期可以采用相似拓扑，简单有效
4. **GOP Cache 机制**：缓存最近 GOP 使新观众秒开。这是在线直播的基本要求。AUDEMSP 的 HLS/HTTP-FLV 输出层必须实现。参考 SRS 的 ring buffer + keyframe boundary 策略
5. **Prometheus + Grafana 可观测性参考**：SRS 的 metrics 设计（流数/客户端数/码率/延迟分布）是基础参考。AUDEMSP 应该在此基础上增加 Fragment 级指标（Fragment 吞吐量、Observer 延迟、SLO 违反次数）
6. **Docker + Helm 部署模式**：Docker 镜像 + Helm Chart 的部署方式是 K8s 时代的标配。AUDEMSP 应该参考 SRS 的部署配置（多架构 Dockerfile + K8s values.yaml）

### [Adapt] 需修改后采用
1. **RTMP 归一化 → Unified Fragment Model**：SRS 所有输入归一化到 RTMP。AUDEMSP 应该借鉴归一化思想，但用 Fragment 替代 RTMP。Fragment 没有 RTMP 的语义限制（无固定音视频交错、有无序 object ID、有 subgroup 概念），可以自然地映射到 MoQ/QUIC 等新协议
2. **协程模型 → Tokio async/await**：ST 协程是非标准技术。AUDEMSP 用 Rust 的 Tokio（业界标准），功能相同但生态更完整。同步风格的 async/await 代码可读性同样好
3. **RTMP 协议实现的参考**：SRS 的 RTMP 实现可以作为 `audemsp-ingest-rtmp` 的功能规范和兼容性基准。但不能直接移植代码（语言和许可不兼容）。需用 Rust 重新实现，重点关注 Enhanced RTMP 兼容性
4. **GB28181 协议的参考实现**：AUDEMSP 的 `SurveillanceSDK` 在 Phase 3+ 应考虑 GB28181。SRS 的 SIP 信令 + RTP/PS 解包是唯一可参考的开源实现。需要关注 SIP 注册、心跳、catalog 查询、invite 信令流程
5. **HTTP-FLV 输出**：中文直播生态特有的 HTTP-FLV 协议。AUDEMSP 的 `HlsPlugin` 可以附加 HTTP-FLV 输出能力（从 Fragment payload 生成 FLV tag 序列）。这是轻量级的格式转换
6. **Oryx 管理平台的理念**：Docker 一键部署 + Web UI 的模式对 AUDEMSP 的部署形态有参考价值。但 AUDEMSP 的 Client 是桌面应用，Host 的嵌入式配置页是 axum + 静态 HTML，不是 SRS 的 Web SPA

### [Avoid] 已知坑 / 不适用场景
1. **不要以 RTMP 作为内部统一表示（核心教训）**：RTMP 的语义限制（固定音视频交错、无 subgroup、无 object ID）使得添加新协议困难。AUDEMSP 必须用 Fragment Model 替代 RTMP 归一化。这是从 SRS 吸取的最重要教训
2. **不要用 ST 协程或非标准并发技术**：ST 协程调试工具不完善、社区小、招聘难。AUDEMSP 应该使用 Tokio async/await（Rust 标准异步运行时）
3. **不要完全依赖外部转码**：SRS 不内置转码，依赖外部 FFmpeg 进程。AUDEMSP 需要内置 transmux（必须）和可选编码管线（ABR、格式转码等场景）。`Transcoder` trait 支持多种后端
4. **不要使用多语言混合进程架构**：SRS 的 SRT 独立 Go 进程 + TCP 管道桥接是历史包袱。跨进程通信增加了延迟和故障点。AUDEMSP 应将所有关键路径保持在 Rust 单进程中
5. **17 万行 C 代码的技术债务教训**：单仓库大代码库的维护挑战。AUDEMSP 应该从初期就保持清晰的 crate 边界（参考 LVQR 的 29 crate），避免单体代码库
6. **TURN 服务器独立部署的运维负担**：SRS 依赖外部 coturn 是持续的痛点。AUDEMSP 应将 TURN/STUN 作为一个可选的内置模块（`StunTurnPlugin`），简化部署
7. **WebRTC 推流转 RTMP 的编解码开销**：SRS WHIP 推流转 RTMP 需要解码→重新编码，破坏 transmux 性能。AUDEMSP 如果用 Fragment Model，WHIP 输入直接产生 Fragment，无需重新编码

**总体评分**：★★★☆☆ (3/5)

SRS 是 RTMP 时代的标杆 — 协议兼容性、生产稳定性、传输性能、中文社区支持均无可匹敌。但以 RTMP 为中心的归一化模型是它的根本性架构缺陷，限制了向 MoQ/QUIC 等现代协议的演进。AUDEMSP 应从 SRS 学习 transmux 哲学、HTTP Callback 事件系统、Origin-Edge 集群拓扑和可观测性设计，但必须用 Fragment Model 替代 RTMP 归一化，从根本上解决 13 年前的架构遗留问题。

---
**相关决策**: D5 (Unified Fragment), D6, D19, D-STREAM-TOPOLOGY, D-GOP-CACHE

## 附录 A: ST 协程架构详解

### A.1 ST (State Threads) 协程原理

ST 是 2001 年 Netscape 开发的用户态线程库。SRS 自 2013 年起使用 ST
作为唯一并发模型。

**核心特性**：
- 用户态协程：调度在内核之外完成，无需系统调用切换上下文
- M:1 模型：多个 ST 协程映射到 1 个内核线程，单进程运行
- 协作式调度：协程主动 yield (通常在 I/O 操作时)
- 上下文切换成本近似于函数调用 (保存/恢复寄存器)
- 每个连接一个协程：新 TCP 连接到达则创建 ST 协程处理此连接所有 I/O

**调度流程**：
```
                   ST Scheduler
                   
  Ready Queue: [coro_1, coro_3, coro_7]
  Wait Queue:  [coro_2 (waiting for fd=5)]
               [coro_4 (waiting for timer)]
               [coro_5 (waiting for fd=9)]

  Loop:
  1. 从 Ready Queue 取出协程，恢复执行
  2. 协程执行到 I/O 操作 -> yield -> 加入 Wait Queue
  3. select() 等待 I/O 事件 -> 就绪协程移入 Ready
  4. 回到步骤 1
```

### A.2 与 Tokio async/await 的比较

| 维度 | ST 协程 | Tokio async/await |
|------|---------|-------------------|
| 语言 | C (非标准) | Rust (语言级支持) |
| 调度模型 | 协作式 (手动 yield) | 协作式 (await point 自动 yield) |
| 上下文切换 | 近乎函数调用 | 近乎函数调用 |


### [Adopt] 补充 — HTTP Callback 事件系统详细设计

**10. HTTP Callback 的事件类型与 AUDEMSP 映射**：

| SRS Callback | 触发时机 | 返回控制 | AUDEMSP 等效 |
|-------------|---------|---------|---------------|
| on_publish | publisher 推流请求 | HTTP 200=允许, 其他=拒绝 | MediaSource::on_publish |
| on_unpublish | publisher 断开连接 | 无（通知） | MediaSource::on_unpublish |
| on_play | viewer 开始播放 | HTTP 200=允许, 其他=拒绝 | MediaSink::on_subscribe |
| on_stop | viewer 停止播放 | 无（通知） | MediaSink::on_unsubscribe |
| on_dvr | 录制段完成 | 无（通知） | RecordSink::on_segment_closed |
| on_hls | HLS segment 生成 | 无（通知） | HlsSink::on_partial_ready |
| on_rtc_play | WebRTC 播放开始 | HTTP 200=允许, 其他=拒绝 | WhepSink::on_subscribe |

AUDEMSP 的 callback 系统应设计为 `EventHook` trait，支持 HTTP 回调（远程服务）和本地回调（内建认证/计费插件）两种模式。

### [Adapt] 补充 — Origin-Edge 集群的 AUDEMSP 实现

**8. Origin-Edge 拓扑在 ClusterPlugin 中的实现策略**：

AUDEMSP Phase 1-2 的集群模型可以直接采用 Origin-Edge 拓扑（与 SRS 一致）：

```
Origin 节点                          Edge 节点
(PipelineEngine)                     (PipelineEngine)
├── MediaSource (RTMP)               ├── MediaSource (从 Origin 拉流)
├── MediaSink (HLS)                  ├── MediaSink (HLS, 本地观众)
├── MediaSink (WHEP)                 ├── MediaSink (WHEP, 本地观众)
└── ClusterPlugin (Origin 角色)      └── ClusterPlugin (Edge 角色)
     ├── 接收推流                           ├── 从 Origin 的 FragmentBroadcaster
     └── FragmentBroadcaster                拉取 Fragment 流 (gRPC/QUIC)
          └── 非本地 edge 通过远程拉流       └── 广播到本地 Sink
```

关键差异：SRS 的 Edge 从 Origin 拉取 RTMP 流（需要编解码器感知），AUDEMSP 的 Edge 从 Origin 拉取 Fragment 流（协议无关）。这意味着 Edge 可以缓存和分发的数据格式不受协议限制 — 同一个 Fragment 流可以被 Edge 的 HLS、DASH、WHEP 三个 Sink 独立消费。

### [Avoid] 补充 — 更多不适用场景

**8. RTMP 作为推流协议的生命周期**：RTMP 于 2009 年由 Adobe 公开规范，至今超过 15 年。虽然 RTMP 兼容性在直播生态中不可替代（OBS→RTMP→SRS 是目前最成熟的推流链路），但 Adobe Flash Player 已于 2020 年退役。RTMP 在传输层面逐渐被 SRT/WebRTC/MoQ 替代。AUDEMSP 必须支持 RTMP（Phase 1，因为 OBS 和推流工具链依赖它），但不应将其作为架构核心。Fragment Model 使 RTMP 成为平等的协议之一，而非架构中心。

**9. 17 万行 C 代码的维护代价**：SRS 的单体代码库 (170K 行 C/C++) 使得新人贡献门槛极高。AUDEMSP 从 Phase 0 就要避免这个问题 — 每个 crate 保持在 2000-5000 行 Rust 代码，加上清晰的 crate 边界和 trait 隔离，让新人可以在单个 crate 范围内完成工作而不需要理解整个代码库。

| 多核利用 | 单线程 (M:1) | 多线程 work-stealing (M:N) |
| 调试支持 | 无原生调试器 | Rust IDE/tracing |
| 生态成熟度 | 极小 | 极大 (整个 Rust 异步生态) |
| I/O 唤醒 | select() | epoll/kqueue/iocp (mio) |
| 代码风格 | 同步写 (无关键字) | async/await 显式标记 |
| 招聘难度 | 几乎不可能 | 容易 (Rust 开发者都会) |

SRS 使用 ST 的历史原因：2013 年 C/C++ 没有标准异步模型。
现在 Rust 的 async/await 是更好的选择。AUDEMSP 使用 Tokio 正确。

---

## 附录 B: RTMP 归一化 vs Fragment Model 比较

### B.1 SRS RTMP 归一化的编解码开销

SRS 将 WebRTC WHIP 推流归一化到 RTMP 时存在重复编码：

```
WebRTC WHIP (H.264+Opus)
  |
  v
RTP depacketizer -> H.264 NAL + Opus frames
  |
  v
H.264 decoder -> YUV -> H.264 encoder -> H.264 NAL (re-encode!)
Opus decoder -> PCM -> AAC encoder -> AAC frames (re-encode!)
  |
  v
FLV muxer -> RTMP stream (内部表示)
  |
  v
从 RTMP transmux 到各输出协议 (HLS/DASH/HTTP-FLV/SRT/WHEP)
```

问题：
1. WHIP 输入的 H.264 已编码完成，但 SRS 需解码再编码，浪费 CPU 且损失质量
2. WHIP 输入的 Opus 音频质量高于 AAC，归一化到 RTMP 后变成 AAC，信息丢失
3. 每个 WHIP 推流都有编解码开销，transmux 零转码优势丧失

### B.2 Fragment Model 解决此问题

```
WebRTC WHIP (H.264+Opus)
  |
  v
RTP depacketizer -> H.264 NAL + Opus frames
  |
  v
直接打包为 Fragment { payload: H.264 NAL + Opus frames }
  |
  v
FragmentBroadcasterRegistry -> Observer taps
  |
  +---> HLS: 从 Fragment 生成 CMAF chunk (不解码!)
  +---> DASH: 从 Fragment 生成 fMP4 segment (不解码!)
  +---> WHEP: 从 Fragment 生成 RTP packets (不解码!)
  +---> MoQ: 直接推送 Fragment (不解码!)
  +---> Recording: 直接写入 fMP4 (不解码!)
```

优势：
1. 零编解码：Fragment 承载原始编码数据，所有输出直接复用
2. 无质量损失：H.264 passthrough
3. 保留原编解码器信息，输出协议可实现最优分包策略

### B.3 迁移路径对比

| 组件 | RTMP 归一化 (SRS) | Fragment Model (AUDEMSP) |
|------|-------------------|---------------------------|
| 内部表示 | FLV tag stream | Fragment stream |
| RTMP 输入 | 零转换 (FLV-FLV) | 小转换 (FLV-Fragment) |
| WHIP 输入 | 解码-编码-FLV | RTP depacket-Fragment |
| SRT 输入 | (独立进程) FLV 桥接 | MPEG-TS-Fragment |
| HLS 输出 | FLV-TS segment | Fragment-CMAF chunk |
| DASH 输出 | FLV-fMP4 segment | Fragment-fMP4 segment |
| WHEP 输出 | FLV-解码-RTP | Fragment-RTP (不解码!) |
| HTTP-FLV 输出 | FLV 直通 | Fragment-FLV tag |

---

## 附录 C: SRS 配置核心参数

SRS 的 INI 配置文件结构：

```ini
listen              1935;
max_connections     1000;
daemon              off;

vhost __defaultVhost__ {
    # HTTP Callback
    http_hooks {
        enabled         on;
        on_publish      http://127.0.0.1:8085/api/v1/callback/on_publish;
        on_play         http://127.0.0.1:8085/api/v1/callback/on_play;
        on_stop         http://127.0.0.1:8085/api/v1/callback/on_stop;
        on_dvr          http://127.0.0.1:8085/api/v1/callback/on_dvr;
    }

    # HLS 配置
    hls {
        enabled         on;
        hls_fragment    3;      # 分片时长 (秒)
        hls_window      12;     # 保留分片数
        hls_path        ./objs/nginx/html;
    }

    # DVR 录制
    dvr {
        enabled         on;
        dvr_path        ./objs/nginx/html/dvr/[app]/[stream]/[timestamp].flv;
        dvr_plan        segment;
        dvr_duration    600;    # 每段10分钟
    }

    # HTTP-FLV
    http_remux {
        enabled     on;
        mount       [vhost]/[app]/[stream].flv;
    }
}
```

对 AUDEMSP 的启示：
- INI 格式虽然传统，但配置参数设计简洁 (hls_fragment 仅需一个数字)
- HTTP Callback 的 URL 模式用参数化路径如 [app]/[stream]
- dvr_plan: segment 按时间分段是标准录制策略
- max_connections 等全局限制是生产级的必要保护措施