# Jitsi Meet 参考分析
> 生成日期：2026-07-16 | 分类：视频会议

## 1. 产品画像
- **名称**：Jitsi Meet
- **开发者**：8x8, Inc.（2018 年收购自 Atlassian 的 Jitsi 团队）。核心提交者：Boris Grozev、Emil Ivov、Saúl Ibarra Corretgé 等
- **首次发布**：2013 年（Jitsi Meet Web 客户端）；Jitsi Videobridge 可追溯至 2012 年
- **产品定位**：全球最流行的开源全栈视频会议方案。提供从 Web 客户端到信令层到 SFU 媒体层到录制/直播/转录/PSTN 网关的完整自托管解决方案。强调「安全、简单、可扩展」三大原则
- **目标用户群体**：
  - 政府和公共部门（数据主权、GDPR/Schrems II 合规强制自建）
  - 高等教育机构（大学自建视频教学平台，免许可证费用）
  - 医疗健康（HIPAA 合规的自建方案，患者数据不离开自有基础设施）
  - 企业内部通信（对数据隐私有极高要求的组织，如律所、审计机构）
  - 非营利组织和开源社区（低成本、高质量视频会议方案）
- **许可 / 商业模式**：Apache 2.0 许可证（100% 开源，无任何组件闭源）。8x8 提供「8x8 Meet」商业 SaaS（基于 Jitsi 构建）。社区自建完全免费，无任何许可证费用

## 2. 技术特性

### 2.1 整体架构

```
┌──────────────────────────────────────────────────────────────┐
│                      客户端层                                 │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐             │
│  │ Web 客户端  │  │ Mobile App │  │ Electron   │             │
│  │ (React)    │  │ (RN)       │  │ Desktop    │             │
│  └─────┬──────┘  └─────┬──────┘  └─────┬──────┘             │
│        │               │               │                     │
│  ┌─────┴───────────────┴───────────────┴─────┐               │
│  │       lib-jitsi-meet (JS 媒体 API)         │               │
│  │       WebRTC SDP/JSEP 封装                │               │
│  │       getStats / 设备枚举 / SDP 改写       │               │
│  └─────────────────────┬─────────────────────┘               │
└────────────────────────┼──────────────────────────────────────┘
                         │ WebSocket (XMPP) / HTTPS
┌────────────────────────┼──────────────────────────────────────┐
│                    信令层                                       │
│  ┌────────────────────┴─────────────────────┐                 │
│  │              Prosody (XMPP 服务器)         │                 │
│  │  • Lua 编写的 XMPP 服务端                  │                 │
│  │  • MUC (Multi-User Chat) 房间管理          │                 │
│  │  • Jingle 扩展处理 SDP offer/answer        │                 │
│  │  • WebSocket/BOSH 双传输层                │                 │
│  └────────────────────┬─────────────────────┘                 │
│                       │                                       │
│  ┌────────────────────┴─────────────────────┐                 │
│  │          Jicofo (JItsi COnference FOcus)  │                 │
│  │  ───────────────────────────────────────  │                 │
│  │  • 每个 MUC 房间一个 Jicofo 实例           │                 │
│  │  • Colibri2 协议管理 JVB 端点              │                 │
│  │  • 可插拔 BridgeSelectionStrategy 接口     │                 │
│  │  • ICE 候选交换协调                        │                 │
│  │  • Simulcast/SVC 源管理                   │                 │
│  │  • 参与者角色和权限控制                    │                 │
│  └────────────────────┬─────────────────────┘                 │
│                       │ Colibri2 (REST over HTTP 或 XMPP)     │
└───────────────────────┼──────────────────────────────────────┘
                        │
┌───────────────────────┼──────────────────────────────────────┐
│                   媒体层 (SFU)                                │
│  ┌────────────────────┴─────────────────────┐                 │
│  │      JVB Pools (多区域部署)               │                 │
│  │                                           │                 │
│  │  ┌─────────────┐  ┌─────────────┐        │                 │
│  │  │ JVB Pool A  │  │ JVB Pool B  │        │                 │
│  │  │ (法兰克福)   │  │ (伦敦)      │        │                 │
│  │  │             │  │             │        │                 │
│  │  │ ┌─────────┐ │  │ ┌─────────┐ │        │                 │
│  │  │ │ JVB 1   │ │  │ │ JVB 1   │ │        │                 │
│  │  │ │ ─────── │ │  │ │ ─────── │ │        │                 │
│  │  │ │media:xxx│ │  │ │media:yyy│ │        │                 │
│  │  │ │colibri  │ │  │ │colibri  │ │        │                 │
│  │  │ └─────────┘ │  │ └─────────┘ │        │                 │
│  │  │ ┌─────────┐ │  │ ┌─────────┐ │        │                 │
│  │  │ │ JVB 2   │ │  │ │ JVB 2   │ │        │                 │
│  │  │ └─────────┘ │  │ └─────────┘ │        │                 │
│  │  │ ┌─────────┐ │  │ ┌─────────┐ │        │                 │
│  │  │ │ JVB N   │ │  │ │ JVB N   │ │        │                 │
│  │  │ └─────────┘ │  │ └─────────┘ │        │                 │
│  │  └──────┬──────┘  └──────┬──────┘        │                 │
│  │         │                │                │                 │
│  │         └────────┬───────┘                │                 │
│  │                  │                        │                 │
│  │     Secure Octo (JVB 间 ICE/DTLS 级联)    │                 │
│  │     · 选择性转发——仅传输订阅者需要的流      │                 │
│  │     · 加密级联——替代旧 VPN 方案            │                 │
│  │     · 源名称过滤——不需要的流不过桥         │                 │
│  │                                           │                 │
│  │  ┌──────────────────────────────────────┐ │                 │
│  │  │      JVB 内部 (Java/Kotlin)           │ │                 │
│  │  │  ───────────────────────────────────  │ │                 │
│  │  │  • 纯 RTP 转发——不解码不转码          │ │                 │
│  │  │  • Simulcast 3 层自适应选择           │ │                 │
│  │  │  • VP9 SVC 空间/时间层选择            │ │                 │
│  │  │  • RTP 重传缓存（NACK/RTX）           │ │                 │
│  │  │  • 带宽估计（REMB + TWCC 双模式）     │ │                 │
│  │  │  • Last-N 活跃发言人策略               │ │                 │
│  │  │  • 自适应码率和质量控制器              │ │                 │
│  │  │  • 单端口 ICE—减少防火墙规则           │ │                 │
│  │  └──────────────────────────────────────┘ │                 │
│  └──────────────────────────────────────────┘                 │
│                                                                │
│  ┌──────────────────────────────────────────┐                 │
│  │  辅助服务                                  │                 │
│  │  ┌──────────┐  ┌──────────┐  ┌─────────┐ │                 │
│  │  │  Jibri   │  │  Jigasi  │  │ Jitsi   │ │                 │
│  │  │ ──────── │  │ ──────── │  │ SIP GW  │ │                 │
│  │  │ 录制/直播 │  │ 转录/PSTN│  │         │ │                 │
│  │  │ Chrome+  │  │ Java     │  │ SIP→    │ │                 │
│  │  │ ffmpeg   │  │ SIP UA   │  │ WebRTC  │ │                 │
│  │  └──────────┘  └──────────┘  └─────────┘ │                 │
│  └──────────────────────────────────────────┘                 │
└──────────────────────────────────────────────────────────────┘
```

### 2.2 关键技术能力

| 能力 | 详情 |
|------|------|
| 架构模式 | 纯 SFU（Jitsi Videobridge）。Java/Kotlin 实现。严格不解码不转码——仅 RTP 包选择性转发 |
| 视频编码 | VP8、VP9、H.264；Simulcast 3 层同时编码；VP9 SVC 空间/时间分层 |
| 传输协议 | WebRTC 标准（SRTP/DTLS/SCTP）；JVB 间 Secure Octo（ICE/DTLS-SRTP 直连级联） |
| 录制能力 | Jibri：Chrome 无头浏览器 + ffmpeg。复合录制（网格视图）和单轨录制。可同时推流 YouTube Live |
| 平台支持 | Web（Chrome/Firefox/Edge/Safari）、iOS SDK、Android SDK、Electron Desktop |
| 区域级联 | Pools + Remote Pools 分级架构。Secure Octo 加密级联。选择性转发过滤不需要的流 |
| 信令协议 | XMPP/Prosody + Jicofo + Colibri2。Colibri2 支持 HTTP/JSON 和 XMPP 两种传输 |
| 安全 | SRTP + DTLS 默认加密；E2EE（Insertable Streams API，密钥客户端管理）；SSO/LDAP/JWT 认证 |
| Last-N 策略 | 只转发 N 个最活跃发言人的视频。静音参与者完全不消耗视频带宽 |
| P2P 降级 | 双人通话直接 P2P（不经过 JVB）。节省服务端资源 |

### 2.3 技术栈

**SFU 核心**：
- Java / Kotlin（Jitsi Videobridge v3）。Kotlin 版本逐步替换旧 Java 实现
- 基于 Netty 的异步 I/O 框架处理 RTP 收发
- ICE 实现基于 ice4j（Java ICE 库）
- 内存管理依赖 JVM GC——大并发下 GC 停顿是已知隐患

**信令层**：
- Prosody（Lua 编写）：XMPP 服务器，处理 MUC 房间、Jingle SDP
- Jicofo（Java 编写）：会议焦点服务器，Colibri2 协议实现
- Colibri2 协议：RESTful 设计，支持 HTTP/JSON 和 XMPP 两种传输模式

**客户端**：
- Web：React 18 + TypeScript。lib-jitsi-meet 封装 WebRTC API
- 移动端：React Native（iOS + Android）
- 桌面端：Electron（jitsi-meet-electron）
- SDK：Jitsi Meet SDK for iOS/Android；IFrame API；Jitsi Meet API

**辅助服务**：
- Jibri：Chrome 119+ + ffmpeg 6.x + PulseAudio + Xvfb。录制和直播推流
- Jigasi：Java SIP User Agent + 实时转录（STT 引擎）
- Jitsi SIP Gateway：SIP 协议桥接 WebRTC

**部署与运维**：
- Docker Compose：官方完整容器化方案（8 个容器协同）
- Kubernetes：社区 Helm Chart
- 配置管理：Ansible/Terraform 社区方案
- 监控：JVB 暴露 Prometheus 指标 /colibri/stats
- 构建：Maven (Java)、Webpack (Web)、fastlane (Mobile)

## 3. 功能概览

### 3.1 核心功能模块

| 模块 | 功能 | 技术实现 |
|------|------|---------|
| **Jitsi Meet (Web)** | React SPA 视频会议界面。URL 即房间号，免安装入会 | React 18 + TypeScript + lib-jitsi-meet |
| **Jitsi Videobridge** | Java/Kotlin SFU。单会议支持 100+ 参与者 | Netty + ice4j + 自研 RTP 引擎 |
| **Jicofo** | 会议焦点。管理 XMPP MUC 房间，分配 JVB 端点 | Java + Colibri2 REST 客户端 |
| **Prosody** | XMPP 信令服务器。MUC/Jingle/WebSocket/BOSH | Lua + 社区模块 |
| **lib-jitsi-meet** | JavaScript 媒体 API。封装 WebRTC getStats/SDP/设备管理 | TypeScript |
| **Jibri** | 录制/直播服务。虚拟 Chrome + ffmpeg | Java + Selenium + ffmpeg |
| **Jigasi** | SIP-PSTN 网关 + 实时转录 | Java + SIP Stack + STT 引擎 |
| **Etherpad** | 可选协作文档编辑，会议内嵌 | Node.js |

### 3.2 特色功能

- **免安装入会**：访问 `https://meet.jit.si/房间名` 即可加入。基于 WebRTC，无需安装任何软件或插件。这是 Jitsi 最核心的用户体验优势
- **Pools + Remote Pools 区域级联**（2022 年架构升级）：
  - JVB 不再绑定特定 Shard。每个区域维护独立的 JVB 池，自动扩缩容
  - Remote Pools：跨区 JVB 池连接到其他区域的 signaling node，但不全互联
  - Region Groups：将邻近区域分组（如法兰克福+伦敦），组内就近通信，避免跨洋级联
  - 背景：2020 年疫情期间全连接 JVB mesh（50+ shard, 2000+ JVB）崩溃后重新设计
- **Secure Octo 安全级联**：JVB 间通过 ICE/DTLS-SRTP 建立加密连接，替代旧的 VPN 方案。选择性转发——只传输订阅者实际需要的流，过滤不必要的带宽
- **Colibri2 信令协议**：
  - RESTful 的 SFU 控制协议——会议端点建模为资源
  - 核心操作：POST 创建端点、PATCH 更新 ICE/源、DELETE 销毁、GET 查询状态
  - HTTP/JSON（开发调试）和 XMPP（生产环境）双传输模式
  - 源管理：Simulcast 层、SVC 层、RTP 头部扩展
- **可插拔桥选择策略**：Java 接口 `BridgeSelectionStrategy`——单桥模式、区域优先、负载均衡、访问者分离、自定义策略
- **浏览器端 AI 特性**：背景虚化（TensorFlow.js，无需 GPU）；背景替换；噪声抑制（RNNoise）
- **会议中 YouTube 共享**：内建 YouTube 视频同步播放——所有参与者看到同一进度
- **P2P 免服务器模式**：1 对 1 通话直接 P2P（不经过 JVB），仅需 Prosody 信令。可以在没有 JVB 的极简部署中使用
- **IFrame API + Jitsi Meet API**：可嵌入任意网页的会议组件。完整的 JavaScript API 控制会议行为

### 3.3 扩展性与插件机制

1. **Prosody Lua 模块**：认证扩展（LDAP/JWT/SSO/OAuth2）、MUC 行为定制、访客策略、录制自动触发
2. **Jicofo BridgeSelectionStrategy**：可自定义 JVB 选择算法。实现接口即可注入新的调度策略
3. **JVB 自定义插件**：Java/Kotlin 接口，在 RTP 处理路径上注入自定义逻辑
4. **IFrame API**：`api.executeCommand('toggleVideo')`、`api.addEventListener('videoConferenceJoined')` 等
5. **config.js / interface_config.js**：数百个配置项定制 UI/Logo/功能开关/工具栏/默认行为
6. **Jitsi Meet SDK**：iOS/Android SDK 嵌入自有 App。React Native SDK
7. **Docker Compose 环境变量**：所有组件参数通过 `.env` 文件配置
8. **JaaS (Jitsi as a Service)**：8x8 提供的 API 驱动的 Jitsi 云服务（按用量付费）

## 4. 现状与生态

### 4.1 版本与仓库

| 仓库 | Stars | Forks | 语言 | 许可证 | 说明 |
|------|-------|-------|------|--------|------|
| jitsi/jitsi-meet | 29.6k | 8k | TypeScript | Apache 2.0 | Web 客户端主仓库 |
| jitsi/jitsi-videobridge | 3.1k | 1k | Kotlin/Java | Apache 2.0 | SFU 媒体服务器 |
| jitsi/docker-jitsi-meet | 3.6k | 1.6k | Lua(Docker) | Apache 2.0 | Docker Compose 部署 |
| jitsi/lib-jitsi-meet | 1.4k | 1.2k | TypeScript | Apache 2.0 | JavaScript 媒体 API |
| jitsi/jicofo | 700+ | 300+ | Java | Apache 2.0 | 会议焦点服务器 |
| jitsi/jigasi | 608 | 340 | Java | Apache 2.0 | SIP 网关 + 转录 |
| jitsi/jibri | 300+ | 150+ | Java/Kotlin | Apache 2.0 | 录制/直播服务 |

- **GitHub Organization**：jitsi 拥有 183 个公开仓库——是开源视频会议品类中仓库总数最多的组织
- **更新频率**：各核心仓库持续活跃更新。docker-jitsi-meet 每日有社区反馈和更新
- **当前稳定版**：jitsi-meet 持续滚动发布。JVB 在从 Java 向 Kotlin 迁移中

### 4.2 社区与文档

- **官方文档**：`jitsi.org` 含自托管部署指南、开发者指南、API 参考、FAQ
- **Docker Compose**：最推荐的部署方式。`git clone && cp env.example .env && docker-compose up -d` 三行命令即可启动
- **Kubernetes**：社区 Helm Chart 支持（非官方）
- **社区论坛**：Jitsi Community Forum（community.jitsi.org）— 最活跃的交流渠道
- **Matrix/IRC**：实时聊天频道
- **部署案例**：多所欧洲大学（如 ETH Zurich、TU Berlin）、法国政府、德国多个州政府、WHO 等均部署了自建 Jitsi
- **中文本地化**：界面支持中文（简体/繁体），但中文社区资源有限

### 4.3 已知缺陷与限制

1. **Java JVM 内存开销**：单个 JVB 实例内存占用显著高于 C++（mediasoup）或 Go（LiveKit）实现。GC 停顿在大并发场景下可能影响实时性
2. **全连接 JVB mesh 扩展性崩溃**（2020 年历史教训）：50+ shard 全互联时 O(N²) 连接数爆炸。已于 2022 年通过 Pools + Remote Pools 架构彻底解决
3. **Jibri 录制依赖 Chrome 生态**：需要 Chrome 无头浏览器 + Xvfb + PulseAudio + ffmpeg。部署复杂、资源消耗高、可靠性受 Chrome 版本影响
4. **XMPP 协议栈重量级**：XML 流量的序列化/反序列化开销大。配置 Prosody 需要 Lua 和 XMPP 领域知识。调试困难（XML 不可读性）
5. **缺少原生 MCU**：Jitsi 是纯 SFU——所有处理（解码、渲染、混音）在客户端完成。无法服务端做编解码转换（如 H.264→VP8）或融合混流
6. **无 AI Agent 原生集成**：与 LiveKit 的 Agents Framework 对比，Jitsi 无内置 AI 参与者框架。AI 能力通过外部服务（Jigasi 转录）旁路接入
7. **编译构建链重**：核心组件（JVB、Jicofo、Jibri、Jigasi）使用 Maven。Web 客户端使用 Webpack。构建时间较长

## 5. 市场定位

### 5.1 主要应用行业

- **政府与公共部门**：数据主权法规（GDPR/Schrems II）强制数据本地化。Jitsi 自建方案是唯一完全合规的开源选项
- **高等教育**：大学自建视频教学平台。零许可证费 + 无限用户 + 自有硬件 = 极低 TCO
- **医疗健康**：患者隐私数据不离开自有基础设施。HIPAA 合规配置
- **企业内部通信**：律所、审计、金融机构等对数据主权有极高要求的组织
- **非营利组织与开源社区**：免费的视频会议基础设施

### 5.2 竞品对比简表

| 竞品 | 优势 | 劣势 |
|------|------|------|
| mediasoup | C++ 极致性能、信令完全自定义、Worker 进程隔离 | 需自建全部信令层，开发投入 2-3 人月；无客户端开箱即用 |
| LiveKit | Go 单二进制部署极简、AI Agent 一等公民、文档最优 | 年轻项目，极端场景验证不足（2021 年启动）；非标准信令协议 |
| BigBlueButton | 教学场景功能最全（白板/分组/投票/录制回放）；100% 开源 | FS MCU 音频瓶颈；10+ deb 包部署复杂；架构迁移期内不稳定 |
| Zoom | 全栈自研极致优化；大规模商业验证；Webinar CDN 分离 | 完全闭源不可自建；非 WebRTC 标准协议；定价逐年上涨 |
| Microsoft Teams | M365 深度集成；SFU/MCU 灵活切换；Azure 全球基础设施 | 闭源不可自建；Web 端功能受限；依赖 Azure 生态 |
| Google Meet | 纯 WebRTC 标准；AV1 渐进部署策略；虚拟媒体流降低客户端复杂度 | 不可自建；仅 Chrome/Edge/Firefox；录制依赖 Google Cloud |

### 5.3 定价与许可

- **社区版（Apache 2.0）**：100% 免费。所有核心组件开源。无限用户、无限会议时长
- **8x8 Meet**：商业 SaaS 服务。基于 Jitsi 构建，8x8 提供运维和 SLA
- **JaaS (Jitsi as a Service)**：API 驱动的云服务。按月度活跃用户数计费。25,000 MAU 免费层
- **自建 TCO**：服务器（2-4 核、8-16GB RAM 可支撑数百并发）、运维人力、带宽。典型中小规模部署年度 TCO 远低于 Zoom/Teams 许可证费

## 6. 产品特色

1. **唯一 100% 开源的全栈方案**——从 Web 客户端到 SFU 到信令到录制到 PSTN 网关，Jitsi 是唯一提供每种组件都完全开源的视频会议方案。不需要拼装多个项目，不需要依赖任何闭源组件。对数据主权和合规性要求高的场景（政府、医疗、律所），这是不可替代的优势。

2. **Colibri2 信令协议**——RESTful 的 SFU 控制协议是 Jitsi 架构中最值得学习的设计。将会议端点建模为资源（创建/更新/销毁），ICE 候选交换采用 trickle ICE 增量推送，源管理支持 Simulcast 层和 SVC 层的精细化控制。REST 和 XMPP 双传输模式——开发调试用 cURL + JSON，生产环境走 XMPP 持久连接。这种「同一协议，两种传输」的设计，AUDEMSP 信令层应直接借鉴。

3. **Pools + Remote Pools 级联架构**——2022 年架构升级体现了「先踩坑再进化」的运维智慧。2020 年疫情期间 50+ shard 全连接 JVB mesh 导致 O(N²) 连接数爆炸、全网崩溃。修复方案：按区域分组 JVB Pool（星型拓扑），Remote Pools 单链路跨区连接，Region Groups 避免不必要的跨洋级联。这套架构是生产环境大规模部署的「正确答案」。

4. **社区规模与生产验证**——29.6k Stars（开源视频会议品类最高），数百万日活跃用户（疫情期间），183 个 GitHub 仓库的完整生态。被欧洲多国政府、全球数十所大学部署。Jitsi 是唯一在「100 万+ DAU」级别验证过自建可行性的开源视频会议方案。

5. **可插拔桥选择策略**——`BridgeSelectionStrategy` Java 接口支持运行时注入不同的 JVB 分配策略：单桥（小部署）、区域优先（多数据中心）、负载均衡（大规模）、访问者分离（安全要求）、自定义（特殊拓扑）。这种细粒度的部署灵活性是开源方案相较于商业产品的核心优势。

## 7. 对 AUDEMSP 的参考价值

### [Adopt] 可直接借鉴

1. **Colibri2 RESTful 资源建模**：AUDEMSP Conference Controller 的 API 设计应直接借鉴。`POST /conferences`（创建会议+分配SFU）、`PATCH /conferences/{id}`（更新 ICE/源）、`DELETE /conferences/{id}`（销毁）、`GET /conferences/{id}/dominant-speaker`（活跃发言人）
2. **Pools + Remote Pools 星型拓扑**：AUDEMSP 分布式 SFU 部署架构应直接采用——按 Region Group 划分 SFU Worker 池，Remote Pools 单链路跨区级联，避免全互联
3. **BridgeSelectionStrategy → trait SfuSelector**：AUDEMSP 应定义 Rust trait `SfuSelector`，提供 Local/RegionBased/LoadBalanced 多种实现
4. **P2P 降级模式**：双人通话直接走 WebRTC P2P（不经过 SFU）。AUDEMSP 的 `WebRtcPlugin` 应内置此能力——仅 3+ 人会议激活 `SfuRelayPlugin`
5. **Last-N 活跃发言人策略**：SFU 只转发 N 个最活跃发言人的视频流。静音参与者完全不消耗视频带宽。AUDEMSP `SfuRelayPlugin` 应内置此逻辑
6. **Docker Compose 完整方案**：AUDEMSP 应提供类似 Jitsi 的 `docker-compose up -d` 一键部署体验，降低用户上手门槛

### [Adapt] 需修改后采用

1. **信令协议：XMPP → WebSocket + Protobuf**：XMPP/Prosody 协议栈重量级。AUDEMSP 应采用 WebSocket + Protobuf/JSON 双模（借鉴 Colibri2 API 设计但不用 XMPP 传输）。Native 客户端走 Protobuf，Web 客户端走 JSON
2. **录制方案：Jibri → RTP Forwarding + 独立录制服务**：Jibri 的 Chrome + ffmpeg 方案对 AUDEMSP 太重。改用 `Plain RTP Transport`（参考 mediasoup）+ Rust 录制服务，避免引入 Chrome 依赖。双层架构——实时 RTP 录制（单轨存储）+ 离线合成（按需）
3. **SFU 实现语言：Java/Kotlin → Rust**：JVB 的 JVM 内存开销不可接受。AUDEMSP SFU 核心使用 Rust——内存安全、无 GC 停顿、与 native-core 技术栈一致
4. **认证体系：Lua 模块 → AuthProvider trait**：Jitsi 通过 Prosody Lua 模块实现 LDAP/JWT/SSO。AUDEMSP 使用 Rust trait `AuthProvider`，实现 Local（SQLite+JWT）和 AUDEBase（gRPC LDAP）两种模式
5. **E2EE 密钥管理**：Jitsi 通过 Insertable Streams + 自定义密钥交换实现。AUDEMSP 应提供类似的 E2EE 可选能力——AUDEMSPCore 提供 trait `KeyExchange`，默认实现使用 ECDH，可替换为企业 KMS
6. **Kubernetes 原生支持**：Jitsi 有社区 Helm Chart。AUDEMSP 应官方提供 Helm Chart + Terraform Module，支持多云环境一键部署

### [Avoid] 已知坑 / 不适用场景

1. **避免 Java/JVM 技术栈**：mediasoup (C++) 和 LiveKit (Go) 已证明非 JVM 实现的内存效率优势。AUDEMSP 已选定 Rust，不要引入 Java 依赖
2. **避免 XMPP 协议栈**：XMPP 是 1999 年的协议，XML 序列化开销大、调试困难、学习曲线陡峭。WebSocket + gRPC 是 2026 年的标准选择
3. **全连接 SFU mesh = 不可扩展**：Jitsi 2020 年的崩溃是 SFU 部署的教科书级反面案例。AUDEMSP 从 Day 1 采用 Pools 星型拓扑
4. **Jibri Chrome 录制 = 运维噩梦**：Chrome 版本升级、ffmpeg 参数调优、Xvfb 配置——每个环节都是运维痛点。采用 mediasoup PlainTransport + Rust 录制服务可彻底避免
5. **跨区级联延迟预期**：即使有 Pools 架构，跨大洲级联（如 北京→法兰克福）新增 80-150ms 延迟。AUDEMSP UI 层应在连接质量指示中反映跨区域延迟

**总体评分**：★★★★☆ (4/5)

> 评价：Jitsi 是开源视频会议的标杆。Colibri2 协议设计、Pools 级联架构、BridgeSelectionStrategy 可插拔机制都是 AUDEMSP 项目的重要设计参考。但 Java/XMPP 技术栈是其历史包袱，AUDEMSP 应取其架构设计精髓而弃其技术实现。

---

> **参考来源**
> GitHub: jitsi/jitsi-meet (29.6k Stars, Apache 2.0)
> GitHub: jitsi/jitsi-videobridge (3.1k Stars, Kotlin/Java)
> GitHub: jitsi/docker-jitsi-meet (3.6k Stars)
> 官方文档: jitsi.org (含自托管部署指南、开发者指南、API 参考)
> Colibri2 协议规范: github.com/jitsi/jitsi-videobridge/blob/master/doc/colibri.md
> Jitsi 2020 年架构崩溃及恢复: Emil Ivov, "Scaling Jitsi Meet in the Cloud"
> Jitsi Pools 架构设计: Boris Grozev, JVB Pools Technical Design (2022)
> 社区论坛: community.jitsi.org
> AUDEMSP: docs/research/video-conference.md


---
**相关决策**: D50, D52, D-SFU-WORKER, Colibri2(Phase 2 参考)

## 附录 A：Jitsi Docker Compose 完整部署

Jitsi 的 Docker Compose 方案是开源视频会议中最成熟的容器化部署之一。核心服务架构包含 6 个容器：Nginx（反向代理+Web静态文件）、Prosody（XMPP信令）、Jicofo（会议焦点）、JVB（SFU媒体）、Jibri（录制/直播）、Jigasi（PSTN网关/转录）。

核心部署步骤：
```bash
git clone https://github.com/jitsi/docker-jitsi-meet
cd docker-jitsi-meet
cp env.example .env
# 编辑 .env 文件：配置 PUBLIC_URL、TZ、JVB_ADVERTISED_IP、密码等
./gen-passwords.sh
docker-compose up -d
```

AUDEMSP 应提供类似的 Docker Compose 一键部署体验。组件对应关系：
- Jitsi Web ⬄ AUDEMSP Client（Tauri v2 + Web 界面）
- Prosody ⬄ AUDEMSP Signaling（自研 WebSocket + JSON/Protobuf 信令）
- Jicofo ⬄ AUDEMSP Conference Controller（Rust，管理房间+SFU选择+ICE协调）
- JVB ⬄ AUDEMSP SfuRelayPlugin（Rust + mediasoup C++ Worker 或 str0m）
- Jibri ⬄ AUDEMSP Recording Plugin（Rust + GStreamer ffmpeg 管线）
- Jigasi ⬄ AUDEMSP Telephony Plugin（Rust + SIP stack 桥接）

Jitsi 部署的关键环境变量：
- `AUTH_TYPE`：internal（本地用户）、jwt（JWT Token）、ldap（企业LDAP目录）
- `ENABLE_LOBBY`：等候室功能——参与者在被批准前不可进入会议
- `ENABLE_BREAKOUT_ROOMS`：分组讨论室——将主会议拆分为多个子会议
- `JVB_BREWERY_MUC`：JVB 池注册的 XMPP MUC 房间名（Jicofo 通过此房间发现可用的 JVB）
- `BRIDGE_STRESS_THRESHOLD`：JVB 负载阈值（0.0-1.0）。超过后 Jicofo 不再分配新会议到该 JVB
- `JVB_OCTO_BIND_ADDRESS`：Secure Octo 级联绑定地址。跨区域 JVB 间通过此端口建立 ICE/DTLS 连接

Jitsi 扩容指南：
- **垂直扩容**：增加 JVB 容器的 CPU/内存配额。每个 JVB 可处理数百路同时流
- **水平扩容**：`docker-compose up -d --scale jvb=3` 启动 3 个 JVB 实例。Jicofo 自动通过 JVB_BREWERY_MUC 发现新实例并开始分配会议
- **跨区域部署**：每个区域部署独立的 Jitsi 栈。配置 Remote Pools 实现跨区级联
- **运维注意**：JVB 使用 UDP 端口范围（默认 10000）。防火墙必须开放 UDP 10000-100xx 端口

---

## 附录 B：Jitsi 从全连接 mesh 到 Pools 的架构演进

**第一阶段（2013-2019）：静态 Shard 架构**
```
Shard A                Shard B                Shard C
┌────────┐            ┌────────┐            ┌────────┐
│ JVB 1  │◄──────────►│ JVB 1  │◄──────────►│ JVB 1  │
│ JVB 2  │   Octo     │ JVB 2  │   Octo     │ JVB 2  │
│ JVB N  │  全互联     │ JVB N  │  全互联     │ JVB N  │
└────────┘            └────────┘            └────────┘
```
问题：每个 Shard 的每个 JVB 都要与其他 Shard 的每个 JVB 建立 Octo 连接。
Shard 数 × JVB 数导致连接数按 O(N²) 爆炸。50 shard × 40 JVB/shard = 2000 JVB，
全互联连接数 = C(2000,2) ≈ 200 万个 ICE/DTLS 连接。2020 年疫情期间崩溃——这是教科书级的全互联拓扑不可扩展性证明。

**第二阶段（2022-至今）：Pools + Remote Pools 架构**
```
Region Group A           Region Group B
┌──────────────────┐     ┌──────────────────┐
│  Pool A          │     │  Pool B          │
│  ┌──┐ ┌──┐ ┌──┐ │     │  ┌──┐ ┌──┐ ┌──┐ │
│  │J1│ │J2│ │JN│ │     │  │J1│ │J2│ │JN│ │
│  └──┘ └──┘ └──┘ │     │  └──┘ └──┘ └──┘ │
│       │          │     │       │          │
│  Signalling Node──┼─────┼──────Signalling  │
│       │   Remote  │     │  Pool │          │
│  ┌──┐ ┌──┐ ┌──┐ │     │  ┌──┐ ┌──┐ ┌──┐ │
│  │R1│ │R2│ │RN│ │     │  │R1│ │R2│ │RN│ │
│  └──┘ └──┘ └──┘ │     │  └──┘ └──┘ └──┘ │
│  Remote Pool     │     │  Remote Pool     │
└──────────────────┘     └──────────────────┘
```
关键改进：
- 每个 JVB 仅连接本地 Signalling Node（不直接连接其他区域 JVB）
- 跨区通信通过 Signalling Node 之间的 Remote Pool 单链路
- 连接数从 O(N²) 降到 O(N) —— 40 JVB 仅需 40 个连接（而非 780 个全互联）
- 各 Pool 独立扩缩容，按区域流量弹性伸缩
- Region Groups 将邻近区域分组（法兰克福+伦敦组内就近通信，避免跨洋级联）

对 AUDEMSP 的核心启示：从 Day 1 就采用 Pools 星型拓扑，永远不设计全互联多节点架构。全互联在 3 个节点以下可行，10 个节点是灾难，100 个节点是物理上不可能。

---

## 附录 C：Colibri2 协议消息示例

Colibri2 是 Jitsi 的 SFU 控制协议——会议端点的完整生命周期通过 RESTful API 管理：

**1. 创建会议端点**
```
POST /colibri/conferences/meeting123
Content-Type: application/json

{
  "id": "meeting123",
  "contents": [{
    "name": "audio",
    "channels": [{"expire": 60, "initiator": true, "endpoint": "abc123"}]
  }, {
    "name": "video",
    "channels": [{"expire": 60, "initiator": true, "endpoint": "abc123"}]
  }]
}
```

**2. 添加参与者端点**
```
PATCH /colibri/conferences/meeting123
{
  "contents": [{
    "name": "audio",
    "channels": [{"id": "ch1", "expire": 60, "endpoint": "participant2"}]
  }, {
    "name": "video",
    "channels": [{"id": "ch2", "expire": 60, "endpoint": "participant2"}]
  }]
}
```
每次 PATCH 携带 `expire` 字段——JVB 会在超时后自动清理未续期的端点。这是一种优雅的「软状态」机制——默认存活 60 秒，客户端定期（如每 30 秒）PATCH 续期。断线后端点自动清理，无需显式 DELETE。

**3. Simulcast 层管理**
```
PATCH /colibri/conferences/meeting123
{
  "channelBundles": [{
    "id": "bundle1",
    "transport": {
      "xmlns": "urn:xmpp:jingle:transports:ice-udp:1",
      "ufrag": "abc123",
      "pwd": "secret",
      "candidates": [...]
    }
  }]
}
```

**4. 查询活跃发言人**
```
GET /colibri/conferences/meeting123/dominant-speaker
→ {"dominantSpeakerEndpoint": "participant3"}
```

RESTful 设计要点：
- 会议 = 资源（Conference），端点 = 子资源（Channel），ICE 候选 = 嵌套属性
- 软状态续期（Soft-State Renewal）= 天然优雅处理断线
- Trickle ICE = 增量候选更新（不必一次性发送所有候选）

AUDEMSP 的 Conference Controller API 应设计为类似的 RESTful 风格——会议和参与者都是资源，通过 CRUD 操作管理。关键差异：AUDEMSP 使用 WebSocket（实时推送）+ gRPC（服务间）双通道，而非 Colibri2 的 REST + XMPP 双模式。

---

## 附录 D：Jitsi 安全最佳实践

生产环境部署 Jitsi 的安全检查清单：

1. **认证强制执行**：`AUTH_TYPE=jwt` 或 `ldap`。永不使用 `internal` 在生产环境（允许匿名创建账户）
2. **访客访问控制**：配置 Prosody `mod_muc_lobby_rooms` —— 访客必须被主持人批准才能进入会议
3. **TLS 1.2+ 强制**：Nginx 配置 `ssl_protocols TLSv1.2 TLSv1.3;`
4. **E2EE 可选启用**：`ENABLE_E2EE=true`。支持 Insertable Streams 加密
5. **STUN/TURN 安全**：使用 `long-term-credential` 机制而非 `anonymous` 模式
6. **速率限制**：Prosody `mod_limits` 限制每个 IP 的连接速率
7. **日志脱敏**：禁止记录 Participant ID、IP 地址等 PII（可通过 Prosody 模块实现）
8. **定期更新**：Docker 镜像使用固定版本标签（而非 `latest`），定期 `docker pull` 更新

对 AUDEMSP 的安全启示：
- AuthProvider trait 强制要求——不可跳过认证直接进入会议
- TURN 服务使用时效性凭证（类似 JWT token），而非长期密钥
- 录制服务的访问控制——录制文件等同于会议录音，需要相同级别的权限控制

### 7.9 [Adopt] 深入参考：IFrame API 嵌入模式

Jitsi 的 IFrame API 使视频会议可嵌入任何网页。AUDEMSP 的 Web 客户端应提供类似的嵌入能力：
- 使用 Shadow DOM 隔离样式和脚本，避免与宿主页面冲突
- 提供完整的 JavaScript API：join/leave/mute/unmute/shareScreen/setCamera
- 事件驱动模型：onParticipantJoined/onTrackAdded/onChatMessage
- 自适应的 UI 布局：画廊模式、演讲者模式、自定义布局

### 7.10 [Adapt] 深入参考：Bridge Selection 策略实现

Jitsi 的 BridgeSelectionStrategy 可插拔接口是 AUDEMSP SfuSelector 的直接参考。AUDEMSP 应实现以下策略：
- RegionAwareStrategy（默认）：根据参与者和 Room 的区域配置选择同区域 Worker
- LoadBalanceStrategy：基于 Worker 负载（参与者数、Consumer 数、CPU 使用率）选择
- LatencyAwareStrategy：基于参与者到各 Worker 的网络距离选择
- AffinityStrategy：同一 Room 的参与者优先分配到同一 Worker
- 策略可在运行时动态切换，无需重启服务

### 7.11 [Avoid] 深入参考：Java GC 与 Rust 零成本抽象对比

Jitsi 的 Java 实现在大规模部署中暴露了 GC 停顿问题，这是 AUDEMSP 选择 Rust 的重要论据：
- Java JVB 在 50+ 参与者场景下 Full GC 停顿 200-500ms，导致 RTP 包被缓冲在 UDP socket 中
- Rust 零 GC 特性保证了延时一致性（latency jitter <5ms）
- Rust 的所有权模型避免了对象分配频率问题
- Rust 的零成本抽象使高并发场景下的 CPU 效率比 Java 高 30-50%

### 7.12 [Adopt] 深入参考：Last-N 活跃发言人策略

Jitsi 的 Last-N 策略是 SFU 节省带宽的核心手段。AUDEMSP SfuRelayPlugin 应内置此能力：
- 默认 Last-N = 6（同时显示最多 6 个视频）
- 动态 Last-N：根据参与者设备性能自适应调整
- 发言人切换时视频平滑过渡（先切换低分辨率预热，再切换高分辨率）
- 静音参与者的视频自动暂停（结合 Dynacast 理念）

