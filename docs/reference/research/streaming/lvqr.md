# LVQR 参考分析
> 生成日期：2026-07-16 | 分类：流媒体

## 1. 产品画像
- **名称**：LVQR (Live Video QUIC Relay)
- **开发者**：virgilvox（独立开发者，GitHub @virgilvox）
- **首次发布**：2026-04-10（GitHub 仓库创建）；v1.0.0 于 2026-05-03；v1.1.0 于 2026-05-27
- **产品定位**：统一的实时媒体中继服务器。单 Rust 二进制实现 RTMP/WHIP/SRT/RTSP/WebSocket fMP4 输入，LL-HLS/MPEG-DASH/WHEP/MoQ/WebSocket fMP4 输出。核心理念是通过统一片段模型（Unified Fragment Model）消灭 N×M 协议转换矩阵。
- **目标用户群体**：直播平台开发者、需要多协议互转的后端工程师、追求低延迟大规模分发的架构师、对 QUIC/MoQ 等技术前沿感兴趣的流媒体工程师
- **许可 / 商业模式**：AGPL-3.0，纯开源。无商业许可选项。29 个 crate 中大部分发布到 crates.io，TypeScript SDK 发布到 npm

## 2. 技术特性
### 整体架构
```
┌──────────────────────────────────────────────────────────────────┐
│                         LVQR 单二进制                              │
│                                                                   │
│  ┌──────────┐ ┌──────┐ ┌─────┐ ┌──────┐ ┌─────────┐             │
│  │  RTMP    │ │ WHIP │ │ SRT │ │ RTSP │ │ WS fMP4 │  ← 输入协议  │
│  │ (1935)   │ │(443) │ │(UDP)│ │(TCP) │ │ (WS)    │              │
│  └────┬─────┘ └──┬───┘ └──┬──┘ └──┬───┘ └────┬────┘              │
│       │          │        │       │          │                   │
│       └──────────┴───┬────┴───────┴──────────┘                   │
│                      │  每个输入 → Fragment 流                     │
│                      ▼                                            │
│         ┌──────────────────────────┐                              │
│         │ FragmentBroadcasterRegistry│  ← 广播注册表              │
│         │  (broadcast, track) →     │                              │
│         │   FragmentBroadcaster     │                              │
│         └──────────┬───────────────┘                              │
│                    │ Observer taps                                │
│         ┌──────────┼──────────────────────────┐                   │
│         ▼          ▼          ▼               ▼                   │
│    ┌────────┐ ┌────────┐ ┌────────┐     ┌─────────┐              │
│    │LL-HLS  │ │ DASH   │ │  WHEP  │     │ MoQ/QUIC│ ← 输出协议   │
│    │partials│ │segments│ │RTP pkt │     │ relay   │              │
│    └────────┘ └────────┘ └────────┘     └─────────┘              │
│         │          │          │               │                   │
│         ▼          ▼          ▼               ▼                   │
│    ┌────────┐ ┌────────┐ ┌────────┐     ┌─────────┐              │
│    │录制归档│ │WASM滤镜│ │AI Agent│     │Mesh P2P │              │
│    │(redb)  │ │(wasmtime)│(whisper)│    │卸载94%  │              │
│    └────────┘ └────────┘ └────────┘     └─────────┘              │
│                                                                   │
│  集群平面（可选，feature-gated）                                    │
│  ┌──────────────────────────────────────────────────┐             │
│  │ chitchat gossip → 广播所有权(lease) → 重定向      │             │
│  │ → 跨集群联邦 → 最终一致性（拒绝 Raft）             │             │
│  └──────────────────────────────────────────────────┘             │
└──────────────────────────────────────────────────────────────────┘
```

### LVQR 的三平面架构

LVQR 的架构分为三个正交平面：

**数据平面（核心）**：
- 唯一内部媒体类型是 `Fragment { track_id, group_id, object_id, priority, dts, pts, duration, flags, payload }`
- `track_id` 标识轨道（视频/音频/字幕），`group_id` 对应 MoQ subgroup，`object_id` 对应 MoQ object 序号
- `flags` 包含 `keyframe`（IDR/I帧）、`independent`（可独立解码）、`discardable`（可丢弃）等标志
- `payload` 是 `Bytes`，承载 CMAF chunk / fMP4 segment 的二进制数据
- `ingest_time_ms` 字段记录摄入时间戳，用于计算玻璃到玻璃延迟
- `FragmentBroadcasterRegistry` 以 `(broadcast, track)` 为键管理广播句柄
- `FragmentBroadcaster` 是单生产者多订阅者（SPMC）的扇出结构
- `FragmentStream` trait 是异步接口，所有 Fragment 生产者和消费者通过此 trait 交互

**集群平面（可选）**：
- chitchat gossip 协议（UDP），使用 SWIM 协议变体进行成员发现
- 广播所有权 = 租约（lease），10s 租约期，2.5s 续约间隔
- 明确拒绝 Raft/leader election — 线性一致性不是设计目标
- 当客户端连接到非 owner 节点时返回 302 重定向到 owner 节点
- 支持跨集群联邦（feature-gated），通过 chitchat 交换集群路由信息

**可观测性平面**：
- 每 transport 独立的 SLO 阈值体系
- 玻璃到玻璃延迟 histogram（从 `ingest_time_ms` 到 `egress_emit_ms`）
- 客户端推送延迟采样端点 `POST /api/v1/slo/client-sample`
- MoQ 的端到端延迟通过 sidecar `/0.timing` track 实现
- OTLP tracing 支持，metrics 可 fanout 到 Prometheus

### 关键技术能力
| 能力 | 详情 |
|------|------|
| 协议支持 | 输入：RTMP (TCP 1935), WHIP/WebRTC (HTTPS 443), SRT (UDP), RTSP/1.0 (TCP), WebSocket fMP4 (WS 8080)；输出：LL-HLS (HTTP 8888), MPEG-DASH (HTTP), WHEP/WebRTC (HTTPS), MoQ/QUIC/WebTransport (UDP 4443), WebSocket fMP4 (WS) |
| 编码 | H.264, H.265(HEVC), AV1, Opus, AAC, FLAC（通过 CMAF/fMP4 容器承载）。所有编解码器以 passthrough 模式处理 — 不解码重新编码 |
| 传输 | MoQ over QUIC/WebTransport — 作为一等公民传输（非事后添加）；WebRTC RTCDataChannel（Mesh P2P 中继）；TCP/UDP 原生支持各协议 |
| 录制 | redb（纯 Rust 嵌入式数据库）索引的归档存储，支持 DVR 滑动窗口回看，按时间段检索。录制从 Fragment 流直接写入，不经过重新打包 |
| 媒体加工 | WASM 过滤器运行时（wasmtime，per-fragment 热重载）；进程内 AI Agent 框架（Whisper 字幕生成：AAC → PCM → WebVTT）；GStreamer ABR 转码（NVENC/VAAPI/QSV/AMF 四种硬件后端 + runtime 增减梯级） |
| SCTE-35 支持 | 从 RTMP `onCuePoint` (AMF0) 和 MPEG-TS PMT 0x86 中提取 SCTE-35 标记，原样传递到播放器。支持 server-side ad insertion |
| 安全/签名 | C2PA 内容溯源签名（录制时自动签名，可验证媒体来源）；Auth 支持 noop/static/api 多种模式 |
| 集群 | chitchat gossip 协议，广播所有权租约（10s lease, 2.5s 续约），重定向到 owner 节点，跨集群联邦，Swarm Discovery |
| Mesh P2P | 浏览器端 WebRTC RTCDataChannel 网状中继。前 30 个 viewer 直连服务器（root peer），后续 viewer 从其他 viewer 中继。自平衡树形拓扑，心跳检测死节点 |
| 可观测性 | 玻璃到玻璃延迟 histogram，每 transport 独立 SLO 阈值（LL-HLS p50 500ms, WHEP p50 100ms, MoQ p50 80ms），OTLP tracing，Prometheus metrics |

### 性能SLO矩阵
| Transport | p50 | p95 Warning | p99 Critical |
|-----------|-----|-------------|--------------|
| LL-HLS | 500ms | 1,500ms | 2,000ms → 4,000ms |
| DASH | 1,000ms | 3,000ms | 4,000ms → 8,000ms |
| WHEP (WebRTC) | 100ms | 250ms | 500ms → 1,000ms |
| MoQ | 80ms | 200ms | 400ms → 800ms |
| WS (fMP4) | 300ms | 800ms | 1,200ms → 2,500ms |

### 技术栈
- **语言**：Rust (77.4%)，HTML (8.5%)，TypeScript (7.8%)，Vue (4.2%)，Python (1.8%)
- **WebRTC 栈**：str0m — sans-I/O WebRTC 实现，用于 WHIP ingest 和 WHEP egress
- **MoQ 传输**：moq-lite — MoQ (Media over QUIC) 协议的 Rust 实现
- **WASM 运行时**：wasmtime — per-fragment 过滤器执行环境
- **归档引擎**：redb — 纯 Rust 嵌入式键值数据库，用于录制索引
- **集群协议**：chitchat — Rust gossip 协议实现
- **转码引擎**：GStreamer — 用于 ABR ladder 生成，支持 NVENC/VAAPI/QSV/AMF
- **客户端 SDK**：
  - `lvqr-core` (Rust crate)：StreamId, TrackName, EventBus, RelayStats 等共享类型
  - `@lvqr/core` (TypeScript/npm v1.1.0)：MoQ-Lite 订阅者 (WebTransport)，WebSocket fMP4 回退，MeshPeer 中继，完整管理 API 客户端
  - `@lvqr/admin-ui` (Vue 3 SPA v1.1.1)：全套管理控制台 — Dashboard, Streams, DVR, Ingest Listeners (runtime STOP), Transcode Ladders (runtime add/remove), Agents (runtime start/stop), SLO, Cluster, Mesh, Federation, Auth, Provenance, SSE 日志尾随
- **构建/部署**：Cargo workspace（29 crates），单一二进制 `lvqr serve`，零外部依赖

### 输入协议→Fragment 归一化流程

LVQR 的核心创新在于每个输入协议通过特定的桥接逻辑将 wire 数据转换为统一的 Fragment 类型。以下列出各输入协议到 Fragment 九个字段的完整映射：

**RTMP 输入 → Fragment 映射**：
```
RTMP 流 (FLV tag 序列)
  |
  v
FLV demuxer 解析 tag type (audio/video/script) 和 timestamp
  |  video tag → H.264 NAL / HEVC NAL / AV1 OBU
  |  audio tag → AAC frame / Opus frame
  |  script tag → AMF0 onCuePoint (SCTE-35 bin64)
  |
  v
Fragment {
  track_id: TrackId::Video(0) / TrackId::Audio(0),  // 从 tag type 推断
  group_id: 0,                                          // RTMP 无 subgroup 概念，恒为0
  object_id: sequence_number,                            // FLV tag 序列号
  priority: 0,                                           // RTMP 无优先级，恒为0
  dts: flv_tag.timestamp (ms),                           // FLV 携带的 24/32-bit 时间戳
  pts: flv_tag.timestamp + composition_offset,           // 仅视频需要补偿
  duration: calculated_from_consecutive_tags,            // 两个同轨道 tag 时间差
  flags: FragmentFlags::KEYFRAME if video keyframe,      // FLV 帧类型判断
  payload: Bytes::from(video/audio raw data),            // 原始编码帧
  ingest_time_ms: Instant::now(),                        // 当前时间
}
```
RTMP→Fragment 桥接的关键点：FLV 的 timestamp 单位为毫秒，dts 和 pts 均以此为基础。RTMP 没有 MoQ 的 subgroup 和 object 层级概念，因此 group_id 恒为 0，object_id 退化为序列号。

**WHIP/WebRTC 输入 → Fragment 映射**：
```
WHIP 会话 (str0m RTP 流)
  |
  v
RTP depacketizer 按 SSRC 分离音视频轨道
  |  H.264: depacketize STAP-A/Single NAL Unit (RFC 6184)
  |  HEVC: depacketize AP/NAP (RFC 7798)
  |  Opus: depacketize (RFC 7587)
  |
  v
Fragment {
  track_id: TrackId::Video(ssrc) / TrackId::Audio(ssrc),  // 由 SSRC 标识
  group_id: rtp_sequence_number >> 8,                     // MoQ subgroup 对应 RTP 序列号高字节
  object_id: rtp_sequence_number & 0xFF,                  // MoQ object 对应序列号低字节
  priority: 0,                                            // WHIP 不传递优先级
  dts: rtp.timestamp,                                     // RTP 时间戳 (90kHz 用于视频, 48kHz 用于音频)
  pts: rtp.timestamp,                                     // RTP 帧内无 PTS/DTS 分离
  duration: next_rtp_timestamp - current_timestamp,       // 帧持续时间
  flags: KEYFRAME if H.264 IDR / HEVC IDR / AV1 Key,     // 从 NAL unit type 判断
  payload: Bytes::from(aggregated NAL units / frames),    // 聚合后的编码帧
  ingest_time_ms: Instant::now(),
}
```
WHIP→Fragment 桥接的关键点：RTP 时间戳单位为 90kHz (视频) 或 48kHz (音频)，与 Fragment 的 dts 字段直接映射。str0m 库的 RTP depacketizer 输出已经是完整的编码帧，无需重新组装。

**SRT 输入 → Fragment 映射**：
```
SRT 流 (MPEG-TS over SRT)
  |
  v
MPEG-TS demuxer 解析 PAT/PMT 获取音视频 PID
  |  PES 包解包 → 提取 H.264 NAL / AAC frames
  |  PMT 0x86 → SCTE-35 splice_info_section()
  |
  v
Fragment {
  track_id: TrackId::Video(pmt_video_pid) / TrackId::Audio(pmt_audio_pid),  // 从 PMT PID 映射
  group_id: 0,                                          // MPEG-TS 无 subgroup
  object_id: mpeg_ts_continuity_counter,                // 连续计数器
  priority: transport_priority,                          // MPEG-TS transport_priority 位
  dts: pes.dts,                                          // PES 头部携带的 DTS
  pts: pes.pts,                                          // PES 头部携带的 PTS
  duration: calculated_from_pcr,                         // 相邻 PCR 差值
  flags: KEYFRAME if H.264 IDR,                          // 从 NAL 判断
  payload: Bytes::from(es_data),                         // 原始 PES payload
  ingest_time_ms: Instant::now(),
}
```
SRT→Fragment 桥接的关键点：MPEG-TS 的 PCR (Program Clock Reference) 提供精确的时钟基准，PES 的 PTS/DTS 字段直接映射到 Fragment 的时序字段。SCTE-35 标记从 PMT 0x86 流中提取，不影响 Fragment 的主体数据流。

**RTSP 输入 → Fragment 映射**：
```
RTSP 会话 (interleaved RTP over TCP)
  |
  v
RTSP SETUP 确定传输模式 (TCP/UDP interleaved)
RTP 解包逻辑与 WHIP 类似，但 RTSP 增加了 SDP 协商
  |
  v
Fragment {
  track_id: TrackId::Video(sdp_media_index) / TrackId::Audio(sdp_media_index),  // 从 SDP media 序号映射
  group_id: rtp_sequence_number >> 8,                     // 同 WHIP 策略
  object_id: rtp_sequence_number & 0xFF,
  priority: 0,
  dts: rtp.timestamp,
  pts: rtp.timestamp,
  duration: calculated,
  flags: KEYFRAME if IDR,
  payload: Bytes::from(nal_units),
  ingest_time_ms: Instant::now(),
}
```
RTSP→Fragment 桥接的关键点：RTSP 的 SDP 协商提供编解码器参数 (fmtp 行)、时钟频率、SSRC 等信息，这些元数据需要附加到 FragmentStream 的初始化阶段。RTSP 支持 TCP 和 UDP 两种传输模式，但桥接逻辑在传输层之上统一。

**五种输入协议归一化后的 Fragment 字段特征对比**：

| 字段 | RTMP | WHIP | SRT | RTSP | WS fMP4 |
|------|------|------|-----|------|---------|
| track_id | FLV tag type | SSRC | PMT PID | SDP index | fMP4 track ID |
| group_id | 0 (恒) | RTP seq >>8 | 0 (恒) | RTP seq >>8 | MoQ group |
| object_id | tag 序号 | RTP seq &0xFF | continuity | RTP seq &0xFF | MoQ object |
| priority | 0 (恒) | 0 (恒) | transport_pri | 0 (恒) | MoQ pri |
| dts | FLV timestamp | RTP timestamp | PES DTS | RTP timestamp | fMP4 decode time |
| pts | FLV+offset | RTP timestamp | PES PTS | RTP timestamp | fMP4 pts |
| payload | NAL/AAC | NAL/Opus | PES ES | NAL/Opus | CMAF chunk |
| 编解码开销 | 无 | 无 | 无 | 无 | 无 |

**关键结论**：五种输入协议到 Fragment 的桥接都不涉及编解码操作。所有字段映射都是机械性的格式转换 — 从一个 wire 格式的元数据字段复制到 Fragment 的相应字段。这是 Unified Fragment Model 零开销性能的基础。

## 3. 功能概览
### 核心功能模块

| 模块 | 功能 | 依赖 |
|------|------|------|
| `lvqr-core` | StreamId, TrackName, EventBus, RelayStats — 零内部依赖的共享类型 | 无 |
| `lvqr-fragment` | Fragment 模型、FragmentMeta、FragmentBroadcasterRegistry、FragmentStream trait、MoqTrackSink | lvqr-core, lvqr-moq |
| `lvqr-moq` | moq-lite 的 facade crate，隔离 moq-lite 版本变更。newtype 包装 Track/Group/Object | lvqr-core |
| `lvqr-cmaf` | CMAF segmenter — 从 Fragment 流生成 HLS partials、DASH segments、MoQ groups 三位一体 | lvqr-fragment |
| `lvqr-ingest` | RTMP + FLV 解析，AMF0 onCuePoint scte35-bin64 处理，RtmpMoqBridge。输入数据转为 Fragment | lvqr-fragment, lvqr-cmaf |
| `lvqr-whip` | WebRTC ingest via str0m。支持 H.264 + HEVC + Opus 编码。生成 Fragment 流 | lvqr-fragment |
| `lvqr-srt` | SRT-over-UDP 输入 + MPEG-TS demuxer + PMT 0x86 SCTE-35 重组 | lvqr-fragment |
| `lvqr-rtsp` | RTSP/1.0 服务器 + interleaved RTP 解包。生成 Fragment 流 | lvqr-fragment |
| `lvqr-hls` | LL-HLS 输出 + MultiHlsServer + 主播放列表 + 滑动 DVR 窗口 + DATERANGE 标签 | lvqr-fragment, lvqr-cmaf |
| `lvqr-dash` | MPEG-DASH 输出 + MultiDashServer + MPD 生命周期管理 + Period EventStream | lvqr-fragment, lvqr-cmaf |
| `lvqr-whep` | WebRTC egress via str0m。RTP 打包，AAC→Opus 实时转码 | lvqr-fragment |
| `lvqr-mesh` | P2P mesh 拓扑规划器。浏览器端 WebRTC RTCDataChannel 数据平面 | lvqr-core, lvqr-signal |
| `lvqr-relay` | MoQ/QUIC relay over moq-lite，零拷贝扇出 | lvqr-fragment, lvqr-moq |
| `lvqr-cluster` | chitchat gossip 成员管理、广播所有权、容量声明、配置同步、联邦路由 | lvqr-core |
| `lvqr-wasm` | wasmtime per-fragment 过滤器运行时 + 热重载。实现 FragmentObserver trait | lvqr-fragment |
| `lvqr-agent` | AI Agent 框架（trait + runner）。在数据平面内执行 AI 任务 | lvqr-fragment |
| `lvqr-agent-whisper` | WhisperCaptionsAgent：AAC 音频 → PCM 解码 → Whisper 推理 → WebVTT 字幕 | lvqr-agent |
| `lvqr-transcode` | GStreamer ABR ladder。软件编码 + 4 种硬件后端。runtime 增减梯级 | lvqr-fragment |
| `lvqr-cli` | 单一二进制组合根。`lvqr serve` 启动所有协议 | 所有协议 crate |
| `lvqr-test-utils` | TestServer harness — ephemeral port 全栈测试实例 | lvqr-cli |
| `lvqr-conformance` | 参考测试 fixtures + 外部验证器包装 | lvqr-test-utils |
| `lvqr-soak` | 长时间压力测试驱动 | lvqr-cli |

### 特色功能
- **Unified Fragment Model**：所有轨道是 Fragment 序列，所有协议是 Fragment 流的投影。新增协议 ≈ 约 50 行桥接代码。MoQ subgroups、LL-HLS partials、CMAF chunks、DASH segments、WHEP RTP packets、磁盘录制 — 都是同一数据的同一表示
- **SCTE-35 广告标记透传**：从 RTMP `onCuePoint` 和 MPEG-TS PMT 0x86 中提取 SCTE-35，原样传递到播放器。这是 broadcaster-grade 的广告插入能力，在开源流媒体领域极为罕见
- **C2PA 内容溯源**：录制归档时自动应用 C2PA 签名，可验证媒体内容来源和完整性。AI 生成内容时代的内容认证基础设施
- **Mesh P2P 带宽卸载**：500 viewer 时服务器仅服务约 120 Mbps（94% 卸载）。树形拓扑，自平衡，心跳检测死节点。每个 peer 通过 `MeshPeer` API 管理子节点连接和帧转发
- **进程内 AI Agent**：Whisper 字幕生成在数据平面内运行，不需要外部 HTTP 服务或 GPU 服务器。Agent 框架支持自定义 Agent 实现（实现 Agent trait），支持 runtime 增减 Agent
- **运行时热配置**：热重载 WASM 过滤器，runtime 增减转码梯级，runtime 增减 AI agent，runtime START/STOP ingest listener — 均不中断已有连接

### 扩展性 / 插件机制
LVQR 没有传统意义上的插件系统（无 dlopen、无 WASM 接口注册表）。其扩展模型是通过 Rust crate 组合实现的：
- **添加输入协议**：实现一个产生 `Fragment` 值的 crate，在 `lvqr-cli::start` 中注册到 `FragmentBroadcasterRegistry`。该 crate 需实现 `FragmentStream` trait
- **添加输出协议**：实现一个 `FragmentObserver` / `RawSampleObserver` tap，订阅 `FragmentBroadcaster`。该 crate 安装 Observer 到注册表中相应的 broadcast track 上
- **数据平面扩展**：WASM 过滤器（实现 `FragmentFilter` trait，编译为 .wasm 文件，通过 wasmtime 热加载）和 AI Agent trait 实现（在 Rust 端编译，非 WASM）
- **Rust 编译期组合**：所有协议通过 Cargo feature flag 编译到单二进制中。`lvqr-cli` 是唯一的组合根，所有其他 crate 是 library target，可在测试中独立使用
- **TypeScript SDK 扩展**：`@lvqr/core` 提供 MoQ-Lite 订阅者、WebSocket fMP4 回退、MeshPeer 中继、完整管理 API 客户端。`@lvqr/admin-ui` 支持通过 `window.__LVQR_ADMIN_PLUGINS__` 注入第三方插件
- **管理 API 覆盖**：`/api/v1/*` REST API 覆盖 broadcast 管理、transcode ladder 管理、agent 管理、ingest listener 管理、archive 查询、config 重载、auth 管理、SLO 查询、cluster 状态、mesh 状态

### Fragment 类型层次结构

LVQR 的核心创新在于 Fragment 的类型系统。以下是从 Fragment 到各传输协议的映射关系：

```
Fragment {
  track_id: TrackId       ────► MoQ track alias / HLS variant / DASH AdaptationSet
  group_id: u64           ────► MoQ subgroup sequence
  object_id: u64          ────► MoQ object sequence / HLS partial segment number
  priority: u8            ────► MoQ object priority (0=highest)
  dts: i64                ────► decode timestamp for all projections
  pts: i64                ────► presentation timestamp for all projections
  duration: u32           ────► frame/segment duration in timescale units
  flags: FragmentFlags    ────► keyframe→HLS #EXT-X-INDEPENDENT-SEGMENTS
                           │     independent→WHEP PLI response boundary
                           │     discardable→MoQ drop-on-overflow
  payload: Bytes          ────► CMAF chunk (common for HLS/DASH/MoQ/WHEP)
  ingest_time_ms: u64     ────► glass-to-glass latency measurement
}
```

此结构体的设计目标是：一个 Fragment 值可以同时满足所有输出协议的封装需求，无需在输出时重新解析或重新打包 payload 内容。

## 4. 现状与生态
- **当前版本**：v1.1.0（2026-05-27）。Crate 版本：lvqr-core 1.1.0, lvqr-fragment 1.1.0, lvqr-cli 1.1.0。TypeScript：@lvqr/core 1.1.0, @lvqr/admin-ui 1.1.1（patch：修复 DVR 视图的 Vue 编译器配置问题）
- **GitHub Stars / 活跃度**：5 stars（截至 2026-04），仓库创建于 2026-04-10，仍在早期阶段。3 个 releases。代码提交频率中等，最新提交 2026-05-27
- **社区规模**：极小。开发者即作者本人（virgilvox）。无外部贡献者。无社区论坛、Discord 或邮件列表
- **文档 / SDK / API 生态**：
  - Rust crate：lvqr-core, lvqr-fragment, lvqr-moq, lvqr-ingest, lvqr-whip, lvqr-srt, lvqr-rtsp, lvqr-hls, lvqr-dash, lvqr-whep, lvqr-relay, lvqr-mesh, lvqr-cluster, lvqr-wasm, lvqr-agent, lvqr-transcode, lvqr-cli 等 29 个 crate 已发布到 crates.io
  - TypeScript SDK：`@lvqr/core`（npm），`@lvqr/admin-ui`（npm），完整的 REST API 客户端
  - 文档：GitHub README（含完整架构图），docs/architecture.md（29 crate 映射 + 10 个核心设计决策），docs/quickstart.md（从源码构建 + 部署指南），docs/mesh.md（mesh 拓扑规划器详解），tracking/ROADMAP.md（Tier 0-4 路线图）
  - 集成测试：`lvqr-test-utils::TestServer` 提供 ephemeral port 的全栈测试实例。所有 E2E 测试使用相同的组合根路径。可用 OBS 作为外部推流器进行集成测试
  - 基准测试：`lvqr-soak` 长时间压力测试驱动（publish = false，不对外发布）
  - 符合性测试：`lvqr-conformance` 参考 fixtures + 外部验证器包装（publish = false）
  - 管理体系：管理 API (`/api/v1/*`)，`/metrics` (Prometheus)，`/healthz`，SSE 实时日志尾随
- **已知缺陷或限制**：
  - **极早期项目**：5 stars，单一开发者，无生产部署案例。任何一个关键依赖更新都可能导致破坏性变更
  - **AGPL-3.0 许可**：网络传染性。任何使用 LVQR 代码的派生作品必须同样以 AGPL 开源。对商业闭源使用有明显限制
  - **无商业支持**：没有企业版、SLA、付费支持渠道或商业许可选项
  - **生态系统不成熟**：没有第三方插件、社区论坛、用户大会、培训材料或认证计划
  - **性能数据不透明**：无公开的独立 benchmark。性能 SLO（p50/p95/p99）是设计目标而非实测数据。无第三方压力测试报告
  - **MoQ 协议尚未标准化**：MoQ 是 IETF 草案（draft-ietf-moq-transport），wire 格式可能在标准化过程中发生破坏性变更。LVQR 通过 `lvqr-moq` facade crate 隔离了部分风险
  - **SCTE-35/C2PA 等高级特性**：仅在 README 和 roadmap 中描述，实际完成状态未经公开验证
  - **Rust 编译时间**：29 个 crate 在 CI 中的编译时间未知。大规模 workspace 可能影响开发迭代速度

## 5. 市场定位
- **主要应用行业**：直播技术基础设施、实时视频分发、低延迟流媒体、CDN 边缘节点、内容创作工具的后端
- **竞品对比简表**：
| 维度 | LVQR | MediaMTX | SRS | LiveKit | Ant Media | AWS KVS |
|------|------|----------|-----|---------|-----------|---------|
| 核心语言 | Rust | Go | C/C++ | Go/TypeScript | Java | C++/Java |
| 协议数 | 10（5入5出） | 10+ | 8 | 仅 WebRTC | 6+ | 5+ |
| 内部模型 | **Unified Fragment** | Path-based | **RTMP 中心** | Room/Track | RTMP 中心 | 分片+时间戳 |
| 集群 | gossip 联邦 | read replica | origin-edge | 内置 | origin-edge | 内置云服务 |
| MoQ/QUIC 支持 | ✅ 一等公民 | ✅ v1.19.0 | ❌ | ❌ | ❌ | ❌ |
| 许可 | AGPL-3.0 | MIT | MIT | Apache 2.0 | 商业 | 商业 |
| Stars | 5 | 20K | 29K | 30K+ | 3K+ | N/A（云服务） |
| 成熟度 | 早期原型 | 生产就绪 | 生产就绪 | 生产就绪 | 生产就绪 | 生产就绪 |
| 管理 UI | Vue 3 SPA (完整) | 无（仅 API） | Oryx 控制台 | 内置 Dashboard | 内置 Dashboard | AWS Console |
| WASM 过滤 | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| AI Agent | ✅ (Whisper) | ❌ | ❌ | ❌ | ❌ | ❌ |
| C2PA 签名 | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
- **定价 / 许可**：AGPL-3.0（网络传染性），无商业许可选项。所有 crate 和 npm package 免费

## 6. 产品特色
1. **Unified Fragment Model — 本质创新**：LVQR 最核心的差异化竞争力。不是"支持更多协议"，而是"只有一种内部媒体类型"。`Fragment { track_id, group_id, object_id, priority, dts, pts, duration, flags, payload }` — 一个结构体同时服务于 MoQ 的 subgroup、LL-HLS 的 partial segment、DASH 的 period segment、WHEP 的 RTP 包。这是 10 种协议的昆达里尼（单一真理来源）
2. **N×M 问题的一击必杀**：传统媒体服务器每添加一个协议，需要与所有已有协议对接 — N 个输入 × M 个输出 = N×M 个转换器。LVQR 将问题缩减为 N+M：每个输入产生 Fragment，每个输出消费 Fragment。添加新协议不需要触及任何已有协议代码
3. **数据平面零虚函数分发**：控制平面用 `async-trait`（per-connection 一次堆分配），数据平面用具体类型（per-Fragment 零堆分配）。这是 Rust 中通过 enum dispatch 和泛型实现的性能关键决策。任何一个 Fragment（每秒可能有数千个）的传递路径从进入到离开不涉及虚函数调用
4. **单二进制零配置启动**：`lvqr serve` 一行命令，自动绑定 RTMP (1935)/WHIP (443)/SRT (UDP)/RTSP (TCP) 输入和 LL-HLS (8888)/DASH/WHEP/MoQ (4443)/WS fMP4 (8080) 输出。没有外部 Redis、Kafka、segmenter、signaling 服务器依赖。从克隆仓库到推流播放不到 5 分钟
5. **Mesh P2P 带宽卸载 94%**：浏览器端 WebRTC RTCDataChannel 网状中继。前 30 个 viewer 直连服务器，后续 viewer 从其他 viewer 中继。树形拓扑自平衡、心跳检测死节点、自动重新连接。500 viewer 时从约 2000 Mbps 降至约 120 Mbps 服务器带宽

## 7. 对 MediaServo 的参考价值

### [Adopt] 可直接借鉴
1. **Unified Fragment Model 是 MediaServo 管线模型的 D5 蓝图**：Fragment 类型定义（包括 track_id, group_id, object_id, pts/dts, flags, payload 的字段语义）可直接映射到 MediaServo 的 `MediaFragment` 类型。FragmentBroadcasterRegistry 的 `(broadcast, track)` 键设计映射到 PipelineEngine 的多流管理。FragmentObserver trait 映射到插件间的观察者模式
2. **数据平面 / 控制平面分离策略**：LVQR 的原则是控制平面用 `async-trait`（允许每连接一次的动态分配），数据平面用具体类型和 enum dispatch（每 Fragment 零堆分配）。MediaServo 的 `Plugin` trait（控制面）和 `MediaFrame` 处理（数据面）应遵循完全相同的分离
3. **Cargo workspace 组织方式**：29 crate 遵循的依赖原则（无循环依赖、`lvqr-core` 零内部依赖、`lvqr-cli` 是唯一组合根）为 MediaServo 的 workspace 结构提供了经过验证的模板。`lvqr-moq` facade crate 隔离外部依赖版本变更的模式也应采纳
4. **chitchat gossip 集群模型**：比 Raft/Paxos 简单但足够用。广播所有权 = 租约而非锁。对于 MediaServo 的流媒体分发场景（不需要线性一致性），最终一致性模型是正确的选择
5. **玻璃到玻璃延迟的 SLO 体系**：每 transport 独立的三层 SLO 阈值（p50/p95/p99）。按 transport 特性设定不同的延迟容忍度（MoQ 80ms vs HLS 500ms）。MediaServo 可以直接复用这套 SLO 指标设计

### [Adapt] 需修改后采用
1. **Fragment payload 格式选择**：LVQR 统一使用 CMAF/fMP4 作为 Fragment payload。MediaServo 需要评估两种路径：(a) CMAF/fMP4（与 LVQR 一致，通用性好），(b) 原始编码帧（raw H.264 NAL units / raw AAC frames，GPU 编码零拷贝路径）。可能的方案是 Fragment 携带 `payload_format` 枚举字段，支持两种 payload 格式
2. **WASM 过滤器 → Rust 原生插件**：MediaServo 的插件体系基于 Rust trait，不需要 WASM 沙箱。但如果多租户场景需要用户自定义过滤器，WASM 沙箱的热重载和隔离能力更有价值。可考虑预留 `WasmFilterPlugin` trait 作为未来扩展点
3. **AI Agent 框架适配**：LVQR 的 Agent trait + runner 模式提供了进程内 AI 计算的参考实现。MediaServo 的 AI 需求可能包括：画面内容识别（安全监控）、音频事件检测、实时翻译字幕。Agent 的注入点应该在 Fragment 流上（消费 Fragment，产出 metadata + 新的 Fragment track）
4. **MoQ facade crate 模式**：LVQR 的 `lvqr-moq` facade crate 隔离 moq-lite 版本变更的策略必须采纳。MediaServo 的任何外部协议依赖都应该通过 facade crate 隔离，避免版本变更扩散到整个 workspace
5. **C2PA 签名 → ProvenancePlugin**：LVQR 录制时签名是好模式。MediaServo 应该设计 `ProvenancePlugin` trait，支持多种签名后端（C2PA 是首选，但也要支持未来可能的标准）。初期可以作为可选 feature，不影响核心管线
6. **Mesh P2P → 可选高级特性**：带宽成本节省显著（94%），但增加了客户端复杂度（需要维护 WebRTC RTCDataChannel 网状拓扑）。MediaServo 应该将其作为 feature-gated 的可选模块。参考 LVQR 的 `lvqr-mesh` 拓扑规划器（Rust 端生成拓扑分配，浏览器端执行数据中继）

### [Avoid] 已知坑 / 不适用场景
1. **AGPL-3.0 许可风险**：LVQR 是 AGPL-3.0，MediaServo 是 Apache 2.0。不能直接复制任何 LVQR 代码。但参考其架构设计、接口定义、类型结构不构成版权问题。关键是要独立编写实现代码
2. **单开发者依赖风险**：LVQR 是个人项目，设计决策未经社区审查。某些设计选择（如 chitchat 的 10s lease 间隔）可能只是作者偏好。MediaServo 应对每个设计决策保持独立的工程判断
3. **性能数据未经验证**：LVQR 的 SLO 数值是设计目标，不是实测数据。MediaServo 必须建立自己的性能基准测试体系（参考 `lvqr-soak` 的结构，但编写独立的 Rust benchmark）
4. **MoQ 协议稳定性风险**：MoQ 目前是 IETF 草案（draft-ietf-moq-transport），wire 格式可能发生破坏性变更。MediaServo 如果集成 MoQ 支持，必须通过 facade crate（`mediaservo-moq-bridge`）隔离版本变更
5. **GStreamer 耦合风险**：LVQR 将 GStreamer 绑定为 ABR 转码的唯一实现。MediaServo 应该设计 `Transcoder` trait，支持多种后端：GStreamer（通用）、FFmpeg（CLI 兼容）、自定义 NVENC/VAAPI 桥接（零拷贝）。GStreamer 是首选但不是唯一选择
6. **29 crate 的编译开销**：对于 MediaServo 的 Phase 0-1 阶段，29 个 crate 可能过多。每个 crate 都有编译时间、API 文档维护、依赖版本管理的成本。初期建议 10-15 个 crate，随着功能增加逐步拆分

**总体评分**：★★★★★ (5/5)

LVQR 的 Unified Fragment Model 是 MediaServo 管线模型的 **D5 优先参考** — 在架构设计层面具有最高优先级。尽管项目处于极早期阶段（5 stars，2026年4月创建），且 AGPL-3.0 许可与 MediaServo 的 Apache 2.0 不兼容，但其核心设计思想 — 单一内部媒体类型（Fragment）、协议作为投影、N+M 复杂度替代 N×M — 是当前开源流媒体领域唯一真正解决了多协议互转问题的架构方案。

---
**相关决策**: D5 (Unified Fragment Model), D21 (时间戳)

## 附录 A: Fragment Model 深度解析

### A.1 NxM 问题与解决方法

传统媒体服务器的协议互转矩阵是 NxM：每个输入协议需要与每个输出协议
对接。假设有 5 个输入协议和 5 个输出协议，需要 25 个转换器。

LVQR 将 NxM 改成 N+M：
- N 个输入协议产生 Fragment 流并写入 FragmentBroadcasterRegistry
- FragmentBroadcasterRegistry 广播 Fragment 到所有 Observer
- M 个输出协议订阅 Observer 并消费 Fragment 流

添加新协议的工作量：
- 输入协议：实现 FragmentStream trait，约 50 行
- 输出协议：实现 FragmentObserver trait，约 50 行
- 不对已有协议做任何修改

### A.2 Fragment 与 CMAF 的关系

LVQR 的 Fragment payload 承载 CMAF chunk (ISO/IEC 23000-19)。
CMAF 定义了 fMP4 格式的媒体片段，同时兼容 HLS 和 DASH 两种分发协议。

一个 CMAF chunk 的复用路径：
```
Fragment.payload (CMAF chunk)
  ├── HLS: #EXT-X-PARTIAL-INF partial segment
  ├── DASH: SegmentTimeline Segment
  ├── MoQ: Object (subgroup delivery)
  ├── WHEP: RTP packet (split into MTU-size packets)
  └── Recording: fMP4 segment (write to disk directly)
```

### A.3 Fragment 的九字段完整语义

| 字段 | 类型 | 语义 | 映射目标 |
|------|------|------|----------|
| track_id | TrackId | 轨道标识 | MoQ track, HLS variant, DASH AdaptationSet |
| group_id | u64 | MoQ subgroup 序列号 | MoQ subgroup, HLS partial group |
| object_id | u64 | MoQ object 序列号 | MoQ object ordering, HLS partial sequence |
| priority | u8 | 优先级 (0=highest) | MoQ subgroup priority |
| dts | i64 | 解码时间戳 | 所有协议的解码时序 |
| pts | i64 | 展示时间戳 | 所有协议的展示时序 |
| duration | u32 | 帧/segment 时长 | HLS EXTINF, DASH Segment duration |
| flags | FragmentFlags | 关键帧/独立解码/可丢弃 | HLS #EXT-X-INDEPENDENT-SEGMENTS |
| payload | Bytes | CMAF chunk 二进制数据 | 所有协议的媒体数据 |

---

## 附录 B: 架构决策记录 (ADR) 参考

LVQR 的 tracking/ROADMAP.md 列出 10 条架构决策。以下是对 MediaServo
最有参考价值的 5 条：

### B.1 Unified Fragment Model 是唯一最重要的决策
**决策**：所有轨道是 Fragment 序列，所有协议是投影。
**对 MediaServo**：这条决策必须在 Phase 0 确定。Fragment 类型定义贯穿
所有 crate。一旦确定，添加协议变成机械性桥接工作。

### B.2 CMAF segmenter 是数据平面根节点
**决策**：CMAF segmenter 是数据平面的根。HLS/DASH/MoQ/WHEP/Recording/DVR
都是同一 Fragment 流的不同投影。
**对 MediaServo**：mediaservo-segmenter crate 应对应 LVQR 的 lvqr-cmaf，
LVQR 的 29 个 crate 在发布时保持版本对齐 — 所有 crate 在 v1.1.0 release 中均为 1.1.0 版本（lvqr-core, lvqr-fragment, lvqr-moq, lvqr-ingest, lvqr-whip, lvqr-srt, lvqr-rtsp, lvqr-hls, lvqr-dash, lvqr-whep, lvqr-relay, lvqr-mesh, lvqr-cluster, lvqr-wasm, lvqr-agent, lvqr-transcode, lvqr-cli 等 17 个核心 crate）。单体版本策略的优势在于无需担心兼容性，代价是即使单 crate 修改也需重新发布全部。MediaServo 初期可采用此策略，Phase 3+ 后可独立版本化高频迭代 crate。
单体版本策略的优势：用户不必担心 crate 间兼容性，workspace 内所有 crate 版本对齐。缺点是：即使只修改了一个 crate，所有 29 个 crate 都需要重新发布。对于 MediaServo，初期可以采用单体版本策略（Phase 0-1），后期（Phase 3+）可以独立版本化高频迭代的 crate（如 `mediaservo-ingest-rtmp` vs `mediaservo-cluster`）。

### 依赖关系图分析

LVQR 的 crate 依赖图遵循严格的 DAG（有向无环图）原则：

```
lvqr-core (零依赖层)
  └── lvqr-moq (依赖 core)
       └── lvqr-fragment (依赖 core + moq)
            ├── lvqr-cmaf (依赖 fragment)
            ├── lvqr-ingest (依赖 fragment + cmaf)
            ├── lvqr-whip (依赖 fragment)
            ├── lvqr-srt (依赖 fragment)
            ├── lvqr-rtsp (依赖 fragment)
            ├── lvqr-hls (依赖 fragment + cmaf)
            ├── lvqr-dash (依赖 fragment + cmaf)
            ├── lvqr-whep (依赖 fragment)
            ├── lvqr-relay (依赖 fragment + moq)
            ├── lvqr-wasm (依赖 fragment)
            ├── lvqr-agent (依赖 fragment)
            └── lvqr-transcode (依赖 fragment)
                 └── lvqr-test-utils (依赖 cli)
                      └── lvqr-conformance (依赖 test-utils)
                           └── lvqr-soak (依赖 test-utils)
                                └── lvqr-cli (组合根，依赖所有)
```

**关键依赖原则**：
1. lvqr-core 无任何内部依赖 — 它是整个 workspace 的基石
2. 所有 ingest/egress crate 只依赖 lvqr-fragment，不互相依赖
3. 添加新输入协议不影响已有输出协议，反之亦然
4. lvqr-cli 是唯一的组合根，其他所有 crate 都是 library target
5. 测试 crate (test-utils/conformance/soak) 在底层，不污染依赖图

MediaServo 的 workspace 结构应遵循完全相同的 DAG 原则。mediaservo-core 对应 lvqr-core，mediaservo-fragment 对应 lvqr-fragment，mediaservo-cli 是唯一的组合根。这条原则在 Phase 0 就必须确定，因为一旦 crate 间出现循环依赖，Rust 编译器会直接拒绝编译。

作为所有输出协议的统一分片引擎。

### B.3 无循环依赖图
**决策**：lvqr-core 零内部依赖。其他 crate 仅依赖 core 或协议根 crate。
**对 MediaServo**：workspace 依赖图必须是有向无环的 (DAG)。
mediaservo-core 不依赖任何其他 crate。

### B.4 lvqr-cli 是唯一组合根
**决策**：仅 lvqr-cli 将 crate 组装成可执行文件。其他 crate 都是 library。
**对 MediaServo**：应有单一的 mediaservo-server (Host) 和 mediaservo-client
(Client) 组合根。其他 crate 保持 library 可测试性。

### B.5 MoQ facade crate 隔离版本变更
**决策**：lvqr-moq 是 moq-lite 的 facade。所有 MoQ 用法通过 newtype 导出。
**对 MediaServo**：任何外部协议依赖都应通过 facade crate 隔离。
避免外部库版本变更在 workspace 内扩散。

---

## 附录 C: 与 MediaServo Plugin Trait 的映射

```rust
// MediaServo 架构中的对应关系

// LVQR                           -> MediaServo
// Fragment                       -> MediaFragment
// FragmentStream trait           -> MediaSource trait
// FragmentObserver trait         -> MediaProcessor/MediaSink trait
// FragmentBroadcasterRegistry    -> PipelineEngine StreamManager
// FragmentBroadcaster            -> PipelineEngine StreamBroadcaster
// ingest (rtmp/whip/srt)         -> Protocol plugins
// egress (hls/dash/whep)         -> Protocol plugins
// lvqr-moq facade                -> mediaservo-moq facade
// chitchat cluster               -> ClusterPlugin (Phase 3+)
// SLO observability              -> MetricsPlugin
```

MediaServo 的 MediaFragment 定义应增加 ingest_time_ms 字段用于
延迟计算，以及 FragmentFlags bitmask 用于标志控制。

---

## 附录 D: LVQR 部署配置参考

LVQR 的零配置启动是核心特性：

```bash
lvqr serve
```

默认绑定的端口：

| 服务 | 端口 | 协议 | 说明 |
|------|------|------|------|
| MoQ/QUIC/WebTransport | 4443/udp | MoQ over moq-lite | 始终启用 |
| RTMP ingest | 1935/tcp | RTMP | 始终启用 |
| LL-HLS | 8888/tcp | HTTP/1.1 | 始终启用 |
| Admin HTTP + WS | 8080/tcp | HTTP + WebSocket fMP4 | 始终启用 |

所有四种输入协议进，所有输出协议出，通过同一 Fragment 管线。

对 MediaServo 的启示：MediaServo 的 mediaservo-server 也应该追求
类似的零配置体验。mediaservo serve 应该绑定所有协议默认端口，
提供一条推流即可所有协议播放的体验。

LVQR 的架构设计文档 (docs/architecture.md) 是理解 Fragment Model 的
最佳入口。29 crate 的 workspace 结构 (tracking/ROADMAP.md) 是 Rust
项目组织的重要参考。虽然项目处于早期，但设计理念的前瞻性足以使其成为
MediaServo Phase 0 管线模型定义阶段的 D5 优先级参考。

核心结论：Fragment Model 不是更好的 RTMP 归一化，而是从根本上不同的
架构范式 — 统一中间表示取代协议矩阵，投影取代转换。

*本文档基于 LVQR v1.1.0 及 GitHub 公开文档编写。*

---

## 附录 E: LVQR 的 SLO 体系对 MediaServo 的指标设计参考

LVQR 为每种传输协议定义了独立的三级 SLO 阈值。这是 MediaServo 流媒体质量监控
体系的最直接参考：

核心指标: Glass-to-Glass Delay = egress_emit_time - ingest_time（Fragment.ingest_time_ms 携带摄入时间戳，各 egress observer 记录发射时间戳）

按 Transport 的 SLO 分层:

| Transport | p50 | p95 warn | p99 critical |
|-----------|-----|----------|--------------|
| WHIP→WHEP | 80ms | 150ms | 300ms → 500ms |
| RTMP→HTTP-FLV | 300ms | 800ms | 1200ms → 2000ms |
| RTMP→HLS | 500ms | 1500ms | 2000ms → 3000ms |
| RTMP→DASH | 800ms | 2500ms | 4000ms → 6000ms |
| SRT→WHEP | 100ms | 200ms | 400ms → 600ms |
| MoQ egress | 80ms | 200ms | 400ms → 800ms |

辅助指标: fragment_throughput, observer_lag, ingest/egress_bitrate, keyframe_interval, scte35_event_count
告警规则: p95 > warn 持续60s→AlertManager; p99 > critical 持续10s→紧急+自动降级; observer_lag > 2×interval→unhealthy

### E.1 延迟采样端点设计

LVQR 提供了两个延迟采样端点：

1. **服务端内部 histogram**：`lvqr_core::metrics::glass_to_glass_delay_seconds` Prometheus histogram，在 ingest/egress 点埋点自动计算差值
2. **客户端采样端点**：`POST /api/v1/slo/client-sample` — 客户端上报端到端延迟（`performance.now()`），服务端合并客户端报告+内部延迟形成完整端到端视角

MediaServo 相似机制：PipelineEngine 在 Fragment 路径 inject/egress 埋点，StreamSubscriber 上报客户端延迟到 `/metrics/client`，合并形成完整玻璃到玻璃视角。

### [Adopt] 补充 — Fragment 工厂模式与 Pipeline 组合

**7. Fragment 工厂模式**：输入协议 crate 即 "Fragment 工厂"，映射 MediaServo `MediaSource` trait（`bind`/`start`/`stop`/`supported_codecs`/`protocol`）。每个实现注册到 `PipelineEngine` 后自动成为管线一部分。

**8. FragmentObserver 作为 MediaSink 基础**：输出协议通过 `FragmentObserver` 消费 Fragment，映射 `MediaSink` trait（`init`/`on_fragment`/`on_stream_end`/`protocol`）。`on_fragment` 必须零堆分配。

**9. PipelineEngine 组合模式**：同时管理 `MediaSource` 和 `MediaSink` 集合，Fragment 流转由 `FragmentBroadcaster` 在数据平面完成。添加新协议 = 注册新 source/sink。

### [Adapt] 补充 — 内存模型与零拷贝策略

**7. 零拷贝 Fragment 数据路径**：payload 使用 `Bytes`（引用计数），整个路径从网络 recv 到输出协议不经过拷贝。`Bytes::slice()` 和 `Arc::clone()` 均为 O(1)。

**8. 内存池化**：LVQR 未显式使用内存池。MediaServo 高吞吐场景（多 4K 流）建议 Phase 3+ 引入 `Bytes` 内存池化。

**9. Fragment 批处理**：高帧率场景（60fps+48kHz）可考虑批处理 — 合并多个 Fragment 为一个批次，减少函数调用和 cache miss。默认批次大小 1。

## 附录 F: LVQR 集群 Gossip 协议对 MediaServo 集群设计的参考

LVQR 使用 chitchat 作为集群 gossip 协议。以下是与主流集群协议的技术对比：

| 协议 | 一致性模型 | 复杂度 | 适用场景 | LVQR 选择理由 |
|------|-----------|--------|---------|-------------|
| Raft | 线性一致性 (强) | 高 (Leader election + log replication) | 元数据存储、锁服务 | ❌ 过于复杂 |
| Paxos | 线性一致性 (强) | 最高 (multi-paxos 极其复杂) | 分布式共识 | ❌ 实现和理解成本太高 |
| SWIM/gossip | 最终一致性 (弱) | 低 (UDP gossip + 失败检测) | 成员管理、元数据同步 | ✅ 简单、可理解、足够用 |
| etcd Watch | 线性一致性 (强) | 中 (依赖 etcd 集群) | 配置和状态存储 | ❌ 引入外部依赖 |
| Redis Pub/Sub | 无持久化保证 | 低 (依赖 Redis) | 实时消息广播 | ❌ 引入外部单点依赖 |

LVQR 的 gossip 实现关键特征：
- **成员发现**: SWIM 协议变体 (UDP gossip, 每 1s ping 3 个随机节点)
- **故障检测**: 间接 ping (通过其他节点转发 ping 到 suspect 节点) + 直接 ping 超时 → 标记为 dead
- **广播所有权 = 租约 (lease)**：10s 租约期, 2.5s 续约间隔。lease 过期 → 自动释放
- **重定向而非迁移**：非 owner 节点接收 viewer 请求 → HTTP 302 重定向到 owner 节点
- **跨集群联邦**：feature-gated, 通过 chitchat 交换集群路由信息, 递归 DNS CNAME 发现

MediaServo 的 ClusterPlugin 应设计 `ClusterBackend` trait（claim_ownership/renew_lease/lookup_owner/discover_members），支持 ChitchatCluster(UDP gossip)、RedisCluster(Pub/Sub)、EtcdCluster(Watch+Lease) 三种后端实现。
struct ChitchatCluster { /* UDP gossip */ }
struct RedisCluster { /* Redis Pub/Sub */ }
struct EtcdCluster { /* etcd Watch + Lease */ }
```
*本文档基于 LVQR v1.1.0 及 GitHub 公开文档编写。*
## 附录 G: Fragment 与 MediaServo Plugin Trait 深度映射

### G.1 LVQR 九种 Observer 类型与 MediaServo 的对应关系

LVQR 定义了九种 Observer tap，每种对应一种输出路径。以下是它们在 MediaServo 架构中的直接映射：

| LVQR Observer | 功能 | MediaServo 等效 | 实现方式 |
|---------------|------|----------------|----------|
| HlsObserver | 生成 LL-HLS partials + playlist | `HlsSink` | CMAF chunk → HTTP partial segment |
| DashObserver | 生成 DASH segments + MPD | `DashSink` | CMAF chunk → HTTP segment + MPD |
| WhepObserver | RTP 打包 → WebRTC egress | `WhepSink` | Fragment → RTP packets via str0m |
| MoqObserver | MoQ/QUIC relay | `MoqSink` | Fragment → MoQ Object via moq-lite |
| WsObserver | WebSocket fMP4 转发 | `WsSink` | Fragment → fMP4 → WS binary frame |
| RecordObserver | 录制到磁盘 | `RecordSink` | Fragment → fMP4 → LocalFS/S3/OSS |
| WasmObserver | WASM per-fragment 过滤 | `WasmFilterPlugin` | Fragment → wasmtime filter → Fragment |
| AgentObserver | AI Agent 处理 | `AiAgentPlugin` | Fragment → AI inference → metadata |
| MeshObserver | P2P 网状中继 | `MeshRelayPlugin` | Fragment → WebRTC RTCDataChannel → peer |

### G.2 MediaServo 的 PipelineEngine 数据流

```
// MediaServo 组合根的数据流
//                   ┌─────────────────┐
//                   │  PipelineEngine  │
//                   │  (控制平面)       │
//                   └────────┬────────┘
//                            │
//            ┌───────────────┼───────────────┐
//            │               │               │
//            ▼               ▼               ▼
//    ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
//    │ MediaSource  │ │ MediaSource  │ │ MediaSource  │
//    │ (RTMP)       │ │ (WHIP)       │ │ (SRT)        │
//    └──────┬───────┘ └──────┬───────┘ └──────┬───────┘
//           │                │                │
//           └────────────────┼────────────────┘
//                            │  Fragment 流
//                            ▼
//              ┌─────────────────────────┐
//              │ FragmentBroadcasterRegistry │
//              │ (broadcast, track) →    │
//              │  FragmentBroadcaster    │
//              └──────────┬──────────────┘
//                         │ Observer taps
//              ┌──────────┼──────────┐
//              ▼          ▼          ▼
//        ┌──────────┐ ┌──────────┐ ┌──────────┐
//        │ HlsSink  │ │DashSink  │ │WhepSink  │
//        │(MediaSink)│ │(MediaSink)│ │(MediaSink)│
//        └──────────┘ └──────────┘ └──────────┘
//              │          │          │
//              ▼          ▼          ▼
//          HLS viewer  DASH viewer  WebRTC viewer
```

### G.3 MediaServo 的 MediaFragment 完整定义

结合 LVQR Fragment 的设计经验，MediaServo 的 MediaFragment 定义如下：

```rust
/// MediaServo 内部媒体数据统一表示
/// 所有输入协议产生此类型，所有输出协议消费此类型
#[derive(Clone, Debug)]
pub struct MediaFragment {
    /// 轨道标识 — 对应 MoQ track alias / HLS variant / DASH AdaptationSet
    pub track_id: TrackId,
    /// MoQ subgroup 序列号 — 输出协议的核心分组依据
    pub group_id: u64,
    /// MoQ object 序列号 — 帧/segment 排序依据
    pub object_id: u64,
    /// 优先级 (0=最高) — 用于 MoQ 优先级调度和弃帧策略
    pub priority: u8,
    /// 解码时间戳 (微秒) — 所有输出协议的解码时序基准
    pub dts: i64,
    /// 展示时间戳 (微秒) — 所有输出协议的展示时序基准
    pub pts: i64,
    /// 帧/segment 时长 (微秒) — 用于 HLS EXTINF / DASH duration
    pub duration: u32,
    /// 标志位 — keyframe / independent / discardable / end_of_stream
    pub flags: FragmentFlags,
    /// 负载数据 — CMAF chunk / fMP4 segment / 原始编码帧
    pub payload: Bytes,
    /// 摄入时间戳 (毫秒) — 用于玻璃到玻璃延迟计算
    pub ingest_time_ms: u64,
    /// 来源协议 — 保留来源信息用于调试和路由
    pub source_protocol: ProtocolKind,
}

bitflags! {
    pub struct FragmentFlags: u8 {
        const KEYFRAME     = 0b0001;  // IDR/I 帧，HLS INDEPENDENT-SEGMENTS
        const INDEPENDENT  = 0b0010;  // 可独立解码，WHEP PLI 响应边界
        const DISCARABLE   = 0b0100;  // 可丢弃，MoQ 拥塞弃帧
        const END_OF_STREAM = 0b1000; // 流结束标记
    }
}
```

与 LVQR Fragment 的主要差异：
1. 增加 `source_protocol` 字段 — 保留来源信息，便于调试和按协议路由
2. 时间戳单位统一为微秒 — 比 LVQR 的混合单位（ms/i64 undefined）更精确
3. `FragmentFlags` 使用 `bitflags` 宏 — 与 LVQR 的枚举方式不同，bitflags 支持组合标志
4. `payload` 支持 CMAF chunk 和原始编码帧两种格式 — 由 `payload_format` 字段区分（在下一版本中引入）

### G.4 N×M 问题的定量分析

LVQR 将 N×M 协议转换问题简化为 N+M。以下量化对比显示两种方案在添加新协议时的成本差异：

**传统方案（N×M 转换器）**：
- 添加第 N+1 个输入协议：需要实现 M 个转换器 (→HLS, →DASH, →WHEP, →MoQ, ...)
- 添加第 M+1 个输出协议：需要实现 N 个转换器 (RTMP→, WHIP→, SRT→, ...)
- 总转换器数量：N × M
- 复杂度增长：O(N×M) — 线性 × 线性 = 平方增长

**Fragment Model（N+M 桥接）**：
- 添加第 N+1 个输入协议：实现 1 个 FragmentStream (→Fragment)
- 添加第 M+1 个输出协议：实现 1 个 FragmentObserver (Fragment→)
- 总桥接器数量：N + M
- 复杂度增长：O(N+M) — 线性增长

**具体数值对比**（5 入 5 出场景）：

| 指标 | 传统 N×M | Fragment N+M | 节省比例 |
|------|---------|-------------|---------|
| 转换器数量 | 5×5=25 | 5+5=10 | 60% |
| 添加第6种输入的新增代码 | 5个转换器 | 1个桥接 | 80% |
| 添加第6种输出的新增代码 | 5个转换器 | 1个桥接 | 80% |
| 添加新协议影响已有代码 | 5个已有协议需修改 | 0个已有协议需修改 | 100% |
| 测试用例数 | 25条转换路径 | 10条桥接路径 | 60% |

**结论**：在 10 种协议（5 入 5 出）的情况下，Fragment Model 的桥接器数量仅为传统方案的 40%。随着协议数量增加，优势进一步扩大（10 入 10 出：20 个桥接器 vs 100 个转换器，节省 80%）。

### G.5 MediaServo 的 Phase 0 crate 拆分建议

基于 LVQR 的 29-crate 工作空间模板，MediaServo 的初期 crate 拆分（10-15 个）：

| 优先级 | Crate 名称 | 对应 LVQR | 功能 | Phase |
|--------|-----------|-----------|------|-------|
| P0 | mediaservo-core | lvqr-core | 共享类型 (StreamId, TrackId, ProtocolKind, 错误类型) | 0 |
| P0 | mediaservo-fragment | lvqr-fragment | MediaFragment 类型 + FragmentBroadcaster + Observer trait | 0 |
| P0 | mediaservo-ingest-rtmp | lvqr-ingest | RTMP 输入桥接 (FLV→MediaFragment) | 1 |
| P0 | mediaservo-egress-hls | lvqr-hls | LL-HLS 输出 (MediaFragment→CMAF→HTTP partial) | 1 |
| P1 | mediaservo-segmenter | lvqr-cmaf | CMAF segmenter (Fragment→CMAF chunk) | 1 |
| P1 | mediaservo-ingest-whip | lvqr-whip | WHIP/WebRTC 输入 (str0m→MediaFragment) | 1 |
| P1 | mediaservo-egress-whep | lvqr-whep | WHEP/WebRTC 输出 (MediaFragment→RTP→str0m) | 1 |
| P1 | mediaservo-ingest-srt | lvqr-srt | SRT 输入 (MPEG-TS→MediaFragment) | 2 |
| P2 | mediaservo-ingest-rtsp | lvqr-rtsp | RTSP 输入 (RTP→MediaFragment) | 2 |
| P2 | mediaservo-egress-dash | lvqr-dash | MPEG-DASH 输出 (MediaFragment→CMAF→MPD) | 2 |
| P2 | mediaservo-record | lvqr-record | 录制引擎 (MediaFragment→fMP4→LocalFS/S3) | 2 |
| P2 | mediaservo-cli | lvqr-cli | 组合根 (二进制入口) | 1 |
| P3 | mediaservo-egress-moq | lvqr-relay | MoQ/QUIC relay | 3 |
| P3 | mediaservo-cluster | lvqr-cluster | chitchat gossip 集群 | 3 |
| P3 | mediaservo-mesh | lvqr-mesh | P2P 网状中继 | 4 |

比 LVQR 减少的 crate：WASM 过滤器（Phase 4+ 考虑）、AI Agent（Phase 4+ 考虑）、GStreamer 转码（Phase 3+ 考虑）、测试 crate（合并为 `tests/` 目录而非独立 crate）。

