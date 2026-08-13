# 推拉流产品调研报告

> 调研日期：2026-07-16 | 目标：为 MediaServo 推拉流模块提供技术选型参考

---

## 目录

1. [生产工具（编码器/推流客户端）](#1-生产工具)
   - OBS Studio
   - vMix
   - Wirecast
2. [媒体服务器](#2-媒体服务器)
   - SRS (Simple Realtime Server)
   - MediaMTX
   - nginx-rtmp-module
   - Xiu
   - LiveGo
   - Node-Media-Server
3. [协议转换器 / 流处理引擎](#3-协议转换器)
   - FFmpeg
   - GStreamer
4. [新兴架构参考](#4-新兴架构参考)
   - LVQR — Unified Fragment Model
   - Muxshed (shed)
5. [GStreamer Pipeline 在推拉流中的实践](#5-gstreamer-pipeline)
6. [总结合成](#6-总结合成)

---

## 1. 生产工具（编码器/推流客户端）

### 1.1 OBS Studio

**概况**：全球最流行的开源直播编码器，C/C++ 编写，GPLv2 许可。Windows/macOS/Linux 全平台，免费。定位为通用直播和录屏工具。

**架构模式**：
- **插件化管线**：核心引擎 `libobs` 管理场景图（Scene Graph），插件通过 `obs_output_t` / `obs_source_t` 接口插入，形成 source → filter → encoder → output 的管线
- **多协议输出模型**：每种输出协议是独立的插件（`obs-outputs`），通过 `OBS_OUTPUT_AV/VIDEO/AUDIO` 标志区分
- **两线程 RTMP 架构**：连接线程（`connect_thread`）+ 发送线程（`send_thread`），通过信号量协调；Windows 上还有独立的 socket 线程优化
- **WebRTC 输出取消了 interleaver**：PR #13270 发现 FLV/RTMP 需要的音视频包交错排序对 WebRTC 是多余的（音视频走独立 RTP 轨道），跳过 interleaver 可消除管线延迟

**协议支持矩阵**：

| 协议 | 方向 | 编码 | 备注 |
|------|------|------|------|
| RTMP/RTMPS | 推流 | H.264 + AAC (FLV) | 通过 librtmp，支持 Enhanced RTMP (HEVC/AV1/多轨) |
| SRT | 推流 | H.264 + AAC (MPEG-TS) | 通过 libsrt |
| RIST | 推流 | H.264 + AAC | 通过 librist |
| WHIP (WebRTC) | 推流 | H.264/HEVC/AV1 + Opus | v32.1 新增，支持 Simulcast (1-4 层) |
| HLS | 推流 | H.264 + AAC | 自定义 muxer |

**技术栈**：
- C/C++ 核心，Qt 界面
- GPU 编码：NVENC, AMF, VAAPI, Apple VideoToolbox, QSV
- 网络库：librtmp (自维护 fork，支持 mbedTLS/OpenSSL/GnuTLS)、libdatachannel (WebRTC)
- 脚本扩展：Lua + Python

**性能指标**：
- CPU 占用（1080p60 推流）：约 22%（OBS 30，3 摄像头场景）
- GPU 编码开销：约 12%
- 启动时间：约 4.2s
- 场景切换延迟：约 120ms
- 6 小时稳定性测试：零丢帧

**优点**：
- 免费开源，社区最大
- 插件生态丰富（Lua/Python 脚本）
- 全平台支持（Windows/macOS/Linux）
- Simulcast 支持 WebRTC 推流编码自适应

**缺点**：
- 单输出流原生设计，多平台同时推流需第三方插件（Multiple RTMP Outputs）。OBS v32.1+ 已支持 WHIP Simulcast 1-4 层编码（单一 endpoint，非多 endpoint）。
- 音频路由复杂，不如 vMix 的 bus 系统直观
- 没有内置即时回放（需 Replay Buffer 插件）
- 没有内置虚拟演播室

**可借鉴的设计**：
1. **插件化输出模型**：每种协议是独立的 output 插件，通过统一接口对接编码器输出。MediaServo 的协议适配层可参考这种「统一编码产出 → 各协议分叉」的模式
2. **WHIP Simulcast 分层策略**：1 层=100%, 2 层=50%, 3 层=33%, 4 层=25% — 简单实用，无需复杂协商
3. **跳过 interleaver 优化**：协议感知的管线优化 — WebRTC 不需要 FLV 的音视频交错，跳过此环节减少延迟

**教训**：
- 插件依赖风险：Teams 集成需要 StreamFX 插件，但 Teams 更新频繁导致兼容性断裂
- `librtmp` 需要自维护 fork：社区标准的 librtmp 不支持 Enhanced RTMP，需要自己打补丁
- **不适用于多平台推流场景** — 单输出架构是高并发推流的瓶颈

---

### 1.2 vMix

**概况**：Windows 专用专业级直播制作软件，定位为中大型活动直播。vMix 27（2026年1月发布）。付费软件，Basic $60 ~ Pro $1200。

**架构模式**：
- 全功能直播制作台：将 PC 变为完整的广播工作室
- 支持 HDMI/SDI/NDI/IP 多源混合输入
- 原生多流输出：Basic HD 支持 3 路，Pro 支持 5 路同时推送
- 内置即时回放引擎（体育直播核心功能）
- PTZ 摄像机 IP 控制（VISCA, ONVIF, NDI）

**协议支持**：
- RTMP (带 AES 加密)，NDI
- 不直接支持 SRT/WebRTC 原生（通过外部工具）

**技术栈**：
- Windows 专用（C# 脚本扩展）
- GPU 编码：NVENC, QuickSync, AMF
- 无跨平台支持（Mac 用户只能通过 Parallels）

**性能指标**：
- CPU 占用（1080p60，3 摄像头）：约 28%
- 4K CPU 占用：约 20%（AMD Threadripper）
- GPU 编码开销：约 15%
- 场景切换延迟：约 80ms（业界最优）
- 8K 稳定性超过 Wirecast 50%

**优点**：
- 即时回放引擎业界最佳，一键慢动作+标记+返回直播
- 原生多流推送到 5 个目标，无需插件
- 无限 NDI 输入，适合大型多机位制作
- 内置虚拟演播室、PTZ 控制

**缺点**：
- **Windows Only** — 这是最大的限制
- 成本高（Pro 版 $1200）
- Mac/Linux 不支持
- 脚本扩展仅 C#，生态不如 OBS 的 Lua/Python
- 学习曲线比 OBS 陡峭

**可借鉴的设计**：
1. **Bus 音频系统**：比 OBS 更直观的多声道/多 bus 音频路由
2. **PTZ 摄像机集成**：VISCA/ONVIF 协议与直播制作的深度整合，对 MediaServo 监控相机接入有参考价值
3. **NDI 零配置网络视频**：局域网内免配置的视频共享，适合企业内网场景

**教训**：
- Windows-only 限制受众和部署场景
- 闭源依赖 — MediaServo 需要开源策略

---

### 1.3 Wirecast

**概况**：Telestream 出品，Windows/Mac 双平台。专业级直播制作，定位企业级直播和广播。Wirecast 16（2026年2月发布）。

**架构模式**：
- 专业切换台模式：无限输入源、实时切换、多画面监控
- **ISO 录制**：核心差异化功能 — 所有输入源并行录制，直播后可按独立摄像机源编辑
- 远程制作：Rendezvous 功能处理远程嘉宾（延迟约 120ms，比 OBS WebRTC 插件快 25%）
- 多流输出：原生支持同时推送到多个平台

**协议支持**：
- RTMP/RTMPS, NDI, SRT
- 远程嘉宾通过 RTMP/NDI/浏览器接入
- AI 实时字幕（97% 准确率，4 说话人测试）

**技术栈**：
- C++ 核心，Windows/macOS
- GPU 编码：NVENC, QuickSync, Apple VideoToolbox
- JavaScript 脚本扩展

**性能指标**：
- CPU 占用（1080p60，3 摄像头）：约 35%（三者中最重）
- RAM 占用：约 3.1 GB
- Mac 稳定性问题：M3 Max 上 3 小时直播 crash 2 次
- 启动时间：约 12.5s（最慢）
- GPU 编码开销：约 18%

**优点**：
- ISO 录制是差异化武器
- AI 字幕准确度高
- 企业级 SLA 支持
- 内置专业图形模板

**缺点**：
- 价格高（$299 Studio / $799 Pro）
- Mac 上性能不稳定
- CPU/RAM 占用最高
- 音频混音器功能基础
- 无 Linux 支持

**可借鉴的设计**：
1. **ISO 录制**：所有输入源独立录制，MediaServo 的录像模块可参考这种"多轨并行录制"模式
2. **远程嘉宾管理**：Rendezvous 功能展示了如何将 RTMP/NDI/浏览器多源混合接入

---

## 2. 媒体服务器

### 2.1 SRS (Simple Realtime Server)

**概况**：C/C++ 编写，MIT 许可，28,646 GitHub Stars。中文社区活跃。定位为简单高效的实时媒体服务器，支持从单个二进制同时提供 RTMP/WebRTC/HLS/HTTP-FLV/SRT/MPEG-DASH/GB28181。

**架构模式**：
- **协程架构**：基于 ST（State Threads）协程，无异步回调地狱，单进程多协程处理并发连接
- **单一数据路径**：所有输入协议最终统一为内部 RTMP 流，所有输出协议都从内部 RTMP 流转换
- **多协议输出**：一条 RTMP 输入流可同时输出为 HLS/HTTP-FLV/WebRTC/DASH，无需转码（仅 transmux）
- **Hybrid 模型**：SRT 独立进程模块，通过内部 RTMP 与主进程通信
- **Origin-Edge 集群**：支持多级边缘节点

**协议转换矩阵**：

| 输入 ↓ / 输出 → | RTMP | WebRTC | HLS | HTTP-FLV | SRT | DASH |
|-----------------|------|--------|-----|----------|-----|------|
| RTMP | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| WebRTC | ✅ | ✅ | - | - | - | - |
| SRT | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

**技术栈**：
- C/C++，协程（State Threads）
- 编解码支持：H.264, H.265, AV1, VP9, AAC, Opus, G.711
- 配置格式：INI
- 监控：Prometheus + Grafana
- 部署：Docker + Kubernetes 原生

**性能指标**：

| 测试项 | 数据 |
|--------|------|
| RTMP 延迟 | 0.8s ~ 3s |
| WebRTC 延迟 | 80ms ~ 400ms（优化后 80ms） |
| HLS 延迟 | 3s ~ 10s 标准，2s ~ 3s LL-HLS |
| RTMP 播放并发 | 数千路（单机） |
| WebRTC 播放并发 | 500~1000+（2 vCPU） |
| SRS 4.0.87 WebRTC 延迟 | 80ms |
| SRS 2.0.72 RTMP 延迟 | H.264: 0.4s, H.264+AAC: 0.6s |

**优点**：
- 协议覆盖最广的单一二进制服务器
- 协议转换无需转码（transmux only），CPU 开销极低
- 协程架构避免了异步回调的复杂性
- 社区庞大，中文文档完善
- MIT 许可，商业友好

**缺点**：
- 内部数据模型固定为 RTMP 流，扩展新协议需要适配 RTMP 中间表示
- 不支持 QUIC/Media-over-QUIC（MediaMTX 已支持）
- TURN 服务器需单独部署（WebRTC NAT 穿透靠外部）
- 集群管理没有 GUI（Wowza/Ant Media 有）
- C 代码基础在 2013 年，部分模块维护成本高

**可借鉴的设计**：
1. **单一内部表示的协议转换模式**：所有输入协议归一化到内部流，所有输出内部分派生 — 这是 MediaServo Unified Fragment Model 的核心灵感来源之一
2. **协程架构**：高并发场景下比线程池更轻量，比 async/await 更简单
3. **RTMP-to-* transmux**：零转码开销的协议转换

**教训**：
- 内部 RTMP 流作为唯一中间表示限制了数据模型的扩展性 — LVQR 的 Fragment Model 解决了这个问题
- SRT 独立进程模块（hybrid model）说明多语言/多进程混合架构的边界沟通成本
- WebRTC 的 TURN 是独立的运维负担

---

### 2.2 MediaMTX

**概况**：Go 编写，MIT 许可。前身 `rtsp-simple-server`。定位为"媒体路由器"（media router），零外部依赖的单一二进制。支持 Media-over-QUIC（新兴协议）。

**架构模式**：
- **Path 为中心的数据模型**：每个 path 对应一个流，由一个 publisher 或外部 source 提供，广播给所有 reader
- **Path Manager**：管理路径、认证、客户端绑定
- **协议自动转换**：流进入服务器后，自动以所有启用的协议提供 — 无需额外配置
- **on-demand 拉流**：`sourceOnDemand` 模式，仅在有观众时才拉取上游流，节省带宽
- **Read Replica 模式**：读副本从 origin 实例拉流分发，L4/L7 LB 均衡

**协议支持**：

| 协议 | 推流 | 播放 | 加密 |
|------|:----:|:----:|:----:|
| Media-over-QUIC | ✅ | ✅ | TLS 1.3 |
| SRT | ✅ | ✅ | AES-128/256 |
| WebRTC (WHIP/WHEP) | ✅ | ✅ | DTLS-SRTP |
| RTSP | ✅ | ✅ | RTSPS |
| RTMP | ✅ | ✅ | RTMPS |
| LL-HLS | - | ✅ | - |
| MPEG-TS | ✅ | ✅ | - |
| RTP | ✅ | - | - |

**技术栈**：
- Go 单二进制，零外部依赖
- 编解码：H.264, H.265, AV1, VP9, Opus, MPEG-4 Audio, G.711, LPCM
- 记录格式：fMP4 (fragmented MP4) 或 MPEG-TS
- REST API 运行时管理
- Prometheus metrics, pprof

**优点**：
- **零依赖单二进制** — 部署极其简单（Docker 一行命令）
- Media-over-QUIC 支持领先业界
- on-demand 拉流节省带宽（IoT/IP 摄像头场景）
- 热重载配置，不断开已有连接
- RTSP 隧道（HTTP/WebSocket）解决防火墙穿越

**缺点**：
- Go GC 在极高并发下有停顿风险
- 不内置转码（需配合 FFmpeg）
- 无内置集群管理系统（read replica 需手动配置 LB）
- 纯 Go 生态，无法利用 GPU 硬件编码

**可借鉴的设计**：
1. **"Path" 抽象**：以路径为中心的解耦模型，publisher/source 与 reader 完全隔离，适合 MediaServo 的频道/房间模型
2. **sourceOnDemand**：按需拉流，摄像头/IPC 休眠场景的节能模式
3. **协议自动转换**：publisher 推一种协议，观众用任意协议观看，零配置
4. **Read Replica 水平扩展模式**：简单的 origin-replica 架构，比 SRS 集群更轻量
5. **Media-over-QUIC**：MediaServo 应考虑 QUIC 作为统一传输层

**教训**：
- 不内置转码限制了独立使用场景（必须配 FFmpeg）
- CDN 集成时 LL-HLS 的缓存问题 — 部分 segment 需绕过 CDN 缓存

---

### 2.3 nginx-rtmp-module

**概况**：C 编写，BSD-2-Clause 许可。最广泛使用的开源 RTMP 模块。将 nginx 变为完整的流媒体服务器，是小型自建直播方案的标准选型。

**架构模式**：
- **nginx 模块内嵌**：作为 nginx 的模块运行，共享 nginx 的 worker 进程模型
- **RTMP → HLS/DASH 转换**：将 RTMP 输入实时分片为 HLS `.ts` 段和 `.m3u8` 播放列表，写入磁盘后由 nginx HTTP 模块服务
- **push/pull relay**：构建 origin-edge 分布式网络
- **exec 钩子**：通过外部 FFmpeg 进程实现转码（每个流 fork 一个 FFmpeg 进程）
- **HTTP 回调认证**：`on_publish`、`on_play`、`on_record_done` 等事件回调

**协议支持**：
- 输入：RTMP, RTMPS (通过 stunnel)
- 输出：RTMP, HLS, MPEG-DASH, FLV
- 编解码：H.264, H.265/HEVC, VP6, VP9, Sorenson H.263 + AAC, MP3, Speex, Opus

**性能指标**：

| 场景 | 数据 |
|------|------|
| RTMP 延迟 | 1.56s（平均，学术测试） |
| HLS 延迟（1s segment） | ~4s end-to-end |
| HLS 延迟（6s segment） | ~15-20s |
| 无转码并发 | 30-50 路（4 核 VPS） |
| 带转码并发 | 3-5 路/核（FFmpeg per-stream） |
| HLS 最优 fragment | 3s（延迟与稳定性的平衡点） |
| LL-HLS 下限 | ~5-10s（无 WebRTC 支持） |

**优点**：
- 部署最简单：nginx 用户几乎零学习成本
- 资源占用最低：无转码场景下内存和 CPU 开销极小
- nginx 生态集成：可利用 nginx 的 access control、限速、SSL termination
- 社区成熟，资料丰富

**缺点**：
- **不支持 WebRTC/SRT** — 这是最致命的缺点
- 无内置 ABR 转码（需手动配 FFmpeg exec + 手动拼 master playlist）
- 不支持 RTMPS 原生（需 stunnel/HAProxy 前端）
- 多 worker 需要 `rtmp_auto_push` 且 Windows 上不可用
- 上游 repo 更新缓慢，bug 修复靠社区 fork
- 无 DRM、无高级分析

**可借鉴的设计**：
1. **HTTP 回调事件系统**：`on_publish`/`on_play`/`on_done` — 简单的 stream lifecycle hook 模式。MediaServo 也需要类似的事件系统做认证和计费
2. **push/pull relay**：简单的 origin-edge 拓扑，适合 MediaServo 初期部署
3. **segment-based HLS 生成**：直接写入文件系统，HTTP 静态服务 — 最简模式

**教训**：
- 不支持 WebRTC 意味着它只能作为 RTMP 时代的遗产
- per-stream FFmpeg fork 转码是性能黑洞（4 核机器只能跑 3-5 路）
- **没有内部统一数据模型** — 每个协议各自处理，HLS 写磁盘、RTMP 走 TCP、DASH 又是独立路径 — MediaServo 必须避免这种碎片化

---

### 2.4 Xiu

**概况**：Rust 编写，MIT 许可，2,293 GitHub Stars。纯 Rust 实现的轻量级直播服务器，支持 RTMP/RTSP/WebRTC(WHIP/WHEP)/HTTP-FLV/HLS。

**架构模式**：
- **模块化 Cargo workspace**：各协议作为独立 crate（`rtmp`, `xrtsp`, `httpflv`, `hls`, `webrtc`）
- **StreamHub 中心**：类似 SRS 的内部流总线，协议间转换通过 StreamHub 完成
- **TOML 配置文件**：支持组合启用协议（rtmp only / rtmp+hls / rtmp+httpflv+hls）
- **命令行快速启动**：`xiu -r 1935 -t 5544 -w 8900 -f 8080 -s 8081`

**协议支持**：

| 协议 | 推流 | 播放 | 备注 |
|------|:----:|:----:|------|
| RTMP | ✅ | ✅ | H.264+AAC, GOP cache, cluster |
| RTSP | ✅ | ✅ | H.265/H.264+AAC, TCP/UDP |
| WebRTC (WHIP/WHEP) | ✅ | ✅ | 浏览器播放器内置 |
| HTTP-FLV | - | ✅ | 从 RTMP/RTSP remux |
| HLS | - | ✅ | 从 RTMP/RTSP remux, 支持录制 |

**技术栈**：
- Rust, Tokio 异步
- 编解码：H.264, H.265, AAC
- HTTP 框架：从 hyper 迁移到 axum
- 依赖库：`audiopus`（跨平台编译）, `reqwest`

**优点**：
- Rust 内存安全，适合长期运行的服务端
- 模块化清晰，协议间解耦
- 支持 RTSP over TCP/UDP（Interleaved）
- WHIP/WHEP 内置 Web 播放器（开箱即用）
- CLI 一行命令启动

**缺点**：
- 社区较小，生产部署案例少
- 不支持 SRT（虽然 Rust SRT 库存在）
- HLS 录制有 bug 历史（v0.13.0 panics on HLS URL path）
- 不内置转码
- 性能数据不透明（无公开 benchmark）

**可借鉴的设计**：
1. **Rust workspace 模块化**：协议作为独立 crate — 与 MediaServo 的 crate 架构高度一致
2. **StreamHub 模式**：类似 SRS 的内部 RTMP 流但更模块化
3. **CLI 友好**：命令行快速切换协议端口 — 开发/测试效率高

**教训**：
- HLS URL 路径解析的数组越界 panic（`index out of bounds: len is 3 but index is 3`）— 生产级服务器对 URL 解析必须有防御性编程
- 社区规模影响 bug 修复速度

---

### 2.5 LiveGo

**概况**：Go 编写，MIT 许可，由 `gwuhaolin` 开发。极简 Go 直播服务器，仅支持 RTMP 推流 + HLS/HTTP-FLV/RTMP 播放。

**架构特征**：
- 纯 Go，单一二进制
- 支持协议：RTMP (推流+播放), AMF, HLS, HTTP-FLV
- GOP 缓存（`gop_num` 参数控制）
- 静态推流（`static_push`）
- JWT 认证（fork 版本）
- YAML/JSON 配置文件

**优点**：极简部署，Go 跨平台，适合学习和小型项目
**缺点**：功能有限，无 WebRTC/SRT/RTSP，社区不活跃

**可借鉴的设计**：GOP cache 实现模式 — 缓存最近的 GOP 使新观众立即看到画面

---

### 2.6 Node-Media-Server

**概况**：Node.js 编写，MIT 许可，6K GitHub Stars。npm 全局安装即可使用。

**架构模式**：
- 事件驱动、单线程异步
- v4 支持 Enhanced RTMP (HEVC/VP9/AV1)
- 客户端 SDK 全套（NodePlayer.js / NodeMediaClient iOS/Android）
- REST API + JWT 认证（v4.2）

**协议支持**：RTMP/RTMPS, HTTP/HTTP2-FLV, WS/WSS-FLV, HLS, DASH

**优点**：npm 生态，客户端 SDK 齐全，Node.js 开发者友好
**缺点**：单线程限制并发，不支持 WebRTC/SRT，v2→v4 不兼容升级

---

## 3. 协议转换器 / 流处理引擎

### 3.1 FFmpeg

**概况**：最通用的音视频处理命令行工具和库。`libavcodec`/`libavformat`/`libavfilter`/`libswscale` 构成完整的编解码+转封装+滤镜栈。

**在推拉流场景中的角色**：
- **编码器**：从摄像头/文件/网络流采集 → 编码为 H.264/H.265 → 推送到 RTMP/SRT 服务器
- **转码器**：接收 RTMP 流 → 解码 → 多码率编码 → 输出 HLS ABR ladder
- **协议转换器**：RTMP → HLS, SRT → RTMP, RTSP → HLS 等
- **录制**：`-f flv` 保存推流，`-f segment` 分片保存

**关键参数（低延迟直播）**：
```bash
ffmpeg -re -i input \
  -c:v libx264 -preset veryfast -tune zerolatency \
  -g 60 -keyint_min 60 -sc_threshold 0 \
  -b:v 4000k -maxrate 4000k -bufsize 8000k \
  -f flv rtmp://server/live/key
```
- `-tune zerolatency`：禁用帧重排序（关键！）
- `-g 60`：每 2 秒一个关键帧（HLS 分片对齐）
- `-sc_threshold 0`：禁用场景检测（保证 IDR 对齐）

**ABR 转码的固有问题**（Twitch 工程师分析）：
- 单实例 `1-in-N-out` FFmpeg 产生 N 个独立编码器，N 个编码器的 IDR 帧不对齐
- 源码流 transmux + 转码流混合时，IDR PTS 不对齐导致 Chromecast 播放暂停
- 解决方案：1 个解码器 → N 个缩放器/编码器 + 帧率自适应下采样 — 这需要自定义转码器

**性能数据**：
- NVENC T4：单 GPU 4-6 路 1080p60 同时编码
- NVENC A10：4K 同等工作量
- CPU-only：720p 单码率可行，多码率 ABR 需 GPU
- 4 核 VPS + FFmpeg 转码：3-5 路并发

**优点**：通用性无敌，几乎支持所有格式和协议
**缺点**：命令行工具，不是服务器，无多观众分发能力

**可借鉴的设计**：
1. **管线参数模式**：`preset`/`tune`/`GOP` 的配置模式可作为 MediaServo 编码配置的参考
2. **零延迟调优策略**：`zerolatency` + `sc_threshold 0` + `bf=0`

---

### 3.2 GStreamer

> 详见 [§5. GStreamer Pipeline 深入分析](#5-gstreamer-pipeline)

**概况**：pipeline-based 多媒体框架，由元素（element）通过 pad 链接成管道。GStreamer 1.x 系列，C 核心 + Rust 绑定，LGPL 许可。

**在推拉流中的角色**：
- 采集（`v4l2src`, `ximagesrc`, `videotestsrc`）→ 编码（`x264enc`, `nvh264enc`）→ 打包（`flvmux`, `mpegtsmux`）→ 传输（`rtmpsink`, `srtsink`）
- `webrtcbin` + `webrtcsink`/`webrtcsrc` 实现 WebRTC

**协议支持**：RTMP, SRT, RTSP, HLS/DASH, WebRTC (via `webrtcbin`)

**优点**：极灵活的管道组合，硬件加速集成好
**缺点**：学习曲线极陡，caps 协商复杂，WebRTC 不是原生支持

---

## 4. 新兴架构参考

### 4.1 LVQR — Unified Fragment Model（核心参考）

**概况**：Rust 编写，29-crate workspace。单二进制实现 RTMP/WHIP/SRT/RTSP/WebSocket fMP4 输入，LL-HLS/MPEG-DASH/WHEP/MoQ/WebSocket fMP4 输出。MIT 许可。**这是 MediaServo 管线模型最重要的参考项目**。

**核心架构 — Unified Fragment Model**：

```rust
// crates/lvqr-fragment/src/lib.rs
pub struct Fragment {
    pub track_id: TrackId,    // 轨道标识
    pub group_id: u64,        // 组标识（MoQ group）
    pub object_id: u64,       // 对象标识（MoQ object）
    pub priority: u8,         // 优先级
    pub dts: i64,             // 解码时间戳
    pub pts: i64,             // 展示时间戳
    pub duration: u32,        // 时长
    pub flags: FragmentFlags, // keyframe / independent / discardable
    pub payload: Bytes,       // 负载数据（CMAF chunk / fMP4 segment）
}
```

**设计原则**：
1. **所有输入协议产生 Fragment**，所有输出协议消费 Fragment
2. **一种内部媒体类型**：每个 wire format 都是 Fragment 的一个投影
3. **添加新协议是 ~50 行桥接代码**（安装一个 Observer 或产生 Fragment）
4. **控制平面用 `async-trait`**（每连接一次分配），**数据平面用具体类型**（每 Fragment 零堆分配）

**数据平面架构**：
```
RTMP (1935)  ─┐
WHIP  (HTTPS) ├┐
SRT   (UDP)   ├┼─► FragmentBroadcaster ─► FragmentObserver taps
RTSP  (TCP)   ├┘   per (broadcast, track)  ├─► MoQ relay
WS fMP4       ┘                            ├─► LL-HLS playlist + segments
                                           ├─► DASH MPD + segments
                                           ├─► WHEP RTP packetizer
                                           ├─► WebSocket fMP4 forwarder
                                           ├─► lvqr-record (disk)
                                           └─► lvqr-archive (redb index)
```

**集群平面**（可选，feature-gated）：
- chitchat gossip 协议（UDP），最终一致性
- 广播所有权 = 租约（非锁），10s lease, 2.5s 续约
- 订阅者重定向：302 跳转到 owner 节点
- **明确拒绝 Raft/leader election**：线性一致性不是设计目标

**可观测性**：
- 服务端玻璃到玻璃延迟 histogram（ingest_time_ms → egress_emit_ms）
- 客户端推送延迟样本（`POST /api/v1/slo/client-sample`）
- 每 transport 独立 SLO 阈值
- MoQ 的玻璃到玻璃延迟通过 sidecar ` /0.timing` track 实现

**Mesh 带宽卸载**：
- 前 30 个 viewer 直接连服务器（root peer）
- 后续 viewer 通过 WebRTC DataChannel 从其他 viewer 中继
- 500 viewer 时服务器仅服务 120 Mbps（94% 卸载）
- 树形拓扑，自平衡，心跳检测死节点

**性能 SLO**：

| Transport | p50 | p95 warning | p99 critical |
|-----------|-----|-------------|--------------|
| LL-HLS | 500ms | 1500ms | 2000ms → 4000ms |
| DASH | 1000ms | 3000ms | 4000ms → 8000ms |
| WHEP (WebRTC) | 100ms | 250ms | 500ms → 1000ms |
| MoQ | 80ms | 200ms | 400ms → 800ms |
| WS (fMP4) | 300ms | 800ms | 1200ms → 2500ms |

**对 MediaServo 的启示**：
1. **Fragment 就是 MediaServo 应该追求的 Unified Fragment Model** — 它是协议无关的中间表示，避免了 SRS 以 RTMP 为中心的限制
2. **数据平面零虚函数分发** — Rust 的 enum dispatch 和具体类型对性能至关重要
3. **chitchat gossip 集群** — 比 Raft/Paxos 简单但足够用，MediaServo 可以借鉴
4. **MoQ 作为一等公民** — Media over QUIC 是低延迟传输的未来方向
5. **玻璃到玻璃延迟的 SLO 体系** — 每种协议不同阈值，完整的可观测性
6. **Mesh 卸载** — 带宽成本降低 94%+ 的大规模分发方案

---

### 4.2 Muxshed (shed)

**概况**：Rust (Axum) + SvelteKit 前端，AGPL 许可。自托管的 multistream studio。RTMP + SRT 输入，fan-out 到 YouTube/Twitch/Kick/自定义 RTMP。可视为 self-hosted 的 Restream/StreamYard 替代。

**架构特征**：
- 浏览器内生产切换台（SvelteKit UI）
- 自定义 Rust RTMP/FLV relay（`crates/api/src/rtmp/`）
- FFmpeg 子进程做转码和 HLS
- Program failover：主播断流自动切到 fallback 源
- Elgato Stream Deck 插件

**启示**：展示了 Rust + FFmpeg 组合的实用模式，failover 机制值得借鉴

---

## 5. GStreamer Pipeline 在推拉流中的实践

### 5.1 Pipeline 模式总结

GStreamer 的 pipeline 模型是推拉流场景中最灵活的流处理方式：

```
[Source] → [Decoder] → [Filter/Scale] → [Encoder] → [Muxer] → [Sink]
 v4l2src → decodebin  → videoscale    → x264enc  → flvmux  → rtmpsink
filesrc  → avidemux   → deinterlace   → nvh264enc→ mpegtsmux→ srtsink
```

### 5.2 常见协议 Pipeline

**RTMP 推流**：
```bash
gst-launch-1.0 v4l2src ! videoconvert ! x264enc tune=zerolatency \
  speed-preset=veryfast bitrate=4000 ! h264parse ! flvmux \
  ! rtmpsink location='rtmp://server/live/stream live=1'
```

**SRT 推流**（屏幕共享）：
```bash
gst-launch-1.0 ximagesrc ! videoconvert \
  ! x264enc bitrate=32000 tune=zerolatency speed-preset=veryfast \
  byte-stream=true threads=1 key-int-max=15 intra-refresh=true \
  ! video/x-h264,profile=baseline ! mpegtsmux \
  ! srtserversink uri=srt://0.0.0.0:8888/ latency=100
```

**WebRTC (webrtcsink)**：
```bash
gst-launch-1.0 webrtcsink name=ws meta="meta,name=gst-stream" \
  videotestsrc ! ws. audiotestsrc ! ws.
```
- `webrtcsink` 内置：编解码器选择、TWCC 拥塞控制（GCC 算法重新实现）、自定义 signaling
- `webrtcsrc` 支持解码输出或编码数据直通

### 5.3 生产级使用案例

**ShareChat 直播平台**（印度，百万级用户）：
- GStreamer pipeline 接收 WebRTC 原始帧（YUV/PCM）→ compositor 合成多参与者画面 → x264enc 编码 → RTMP 输出
- 多参与者画面合成使用 GStreamer compositor + 延迟缓冲同步
- 头像/静态画面使用 uridecodebin + imagefreeze
- 同时支持合成流录制和独立流录制
- Temporal workflow 保证高可用

### 5.4 性能参数调优

| 参数 | 低延迟值 | 标准值 | 说明 |
|------|---------|--------|------|
| `tune=zerolatency` | ✅ | - | 禁用帧重排序 |
| `speed-preset` | veryfast/ultrafast | medium | 编码速度 vs 压缩率 |
| `key-int-max` | 15（0.5s@30fps） | 60（2s@30fps） | 关键帧间隔 |
| `intra-refresh` | true | false | 渐进式刷新 vs IDR |
| `threads` | 1（避免延迟） | auto | 编码线程数 |
| `latency` (SRT) | 80-200ms | 125ms | SRT 延迟 |
| `bitrate` | 按链路带宽 70% | - | 避免拥塞 |

### 5.5 GStreamer vs FFmpeg

| 维度 | GStreamer | FFmpeg |
|------|-----------|--------|
| 模型 | 管道元素链接 | 命令行工具 + libav* |
| 实时控制 | 强（运行时管道修改） | 弱（启动后无法改变） |
| 动态管道 | 支持（pad 动态添加/移除） | 不支持 |
| 学习曲线 | 陡峭 | 平缓 |
| 适用场景 | 长时间运行应用、嵌入式 | 批处理、一次性转码 |
| WebRTC | webrtcbin（半原生） | 不支持（需 libdatachannel） |

### 5.6 对 MediaServo 的启示

GStreamer 的 pipeline 模型与 LVQR 的 Unified Fragment Model 有本质区别：
- GStreamer 是**元素级 pipeline**：数据在元素间以 buffer 形式流动，caps 协商复杂
- LVQR 是**Fragment 级管道**：所有输入归一化到 Fragment，所有输出从 Fragment 派生

**MediaServo 应走 LVQR 路线而非 GStreamer 路线**：
1. LVQR 的 Fragment 模型更简单：只有一种内部格式
2. 添加新协议只需桥接代码（~50 行）
3. GStreamer 的 caps 协商在生产中是一个故障源
4. 但 GStreamer 的 compositor + 硬件编解码集成能力值得在 GPU 编码路径中参考

---

## 6. 总结合成

### 6.1 多协议适配最佳模式

| 模式 | 代表产品 | 优点 | 缺点 | 适合 MediaServo？ |
|------|---------|------|------|:---:|
| 内部 RTMP 归一化 | SRS | 简单、成熟 | 受限于 RTMP 语义 | ❌ |
| Path + 协议自动转换 | MediaMTX | 零配置、灵活 | 无统一数据模型 | ⚠️ |
| 管道式链接 | GStreamer | 极灵活 | 学习曲线极陡 | ❌ |
| **Unified Fragment Model** | **LVQR** | 最优雅、协议无关 | 较新、社区小 | ✅ **首选** |
| 无内部模型 | nginx-rtmp | 极简 | 协议间割裂 | ❌ |
| StreamHub 消息总线 | Xiu | Rust 友好 | 功能较少 | ⚠️ |

### 6.2 协议选择建议矩阵

| 协议 | 推流接入 | 播放分发 | 低延迟 | 穿透 NAT | 移动端 | 推荐度 |
|------|:---:|:---:|:---:|:---:|:---:|:---:|
| RTMP | ✅ | ⚠️ | 1-5s | ❌ | ❌ | 必须支持（生态兼容） |
| SRT | ✅ | - | 0.5-2s | ⚠️ | ❌ | 高（可靠推流） |
| WebRTC/WHIP | ✅ | ✅ | <500ms | 需 TURN | ✅ | 高（低延迟互动） |
| HLS/LL-HLS | - | ✅ | 2-10s | ✅ | ✅ | 必须（大规模分发） |
| MPEG-DASH | - | ✅ | 2-10s | ✅ | ✅ | 建议支持 |
| HTTP-FLV | - | ✅ | 1-3s | ✅ | ⚠️ | 可选（中文生态） |
| RTSP | ✅ | ✅ | 0.5-2s | ❌ | ❌ | 建议（IPC 接入） |
| Media-over-QUIC | ✅ | ✅ | <100ms | ✅ | - | 未来方向 |
| MoQ (QUIC) | ✅ | ✅ | <100ms | ✅ | - | 未来方向 |

### 6.3 MediaServo 技术选型建议

**核心架构**：采用 LVQR 的 **Unified Fragment Model** 作为内部数据统一表示

**理由**：
1. SRS 的 RTMP 中心模型在扩展到 MoQ/QUIC 等新协议时隔靴搔痒
2. MediaMTX 的 path 模型缺乏统一的内部媒体类型
3. LVQR 证明了一个 Fragment 类型可以同时服务 MoQ, HLS, DASH, WHEP — 10 种协议，一种内部表示
4. 数据平面零虚函数分发的性能优势（每 Fragment 调用走具体类型，零堆分配）

**建议的 crate 拆分**（参考 LVQR 29-crate 结构）：
- `mediaservo-fragment` — Fragment 类型 + FragmentBroadcaster + FragmentObserver trait
- `mediaservo-ingest-rtmp` — RTMP 输入桥接
- `mediaservo-ingest-srt` — SRT 输入桥接
- `mediaservo-ingest-whip` — WHIP/WebRTC 输入桥接
- `mediaservo-ingest-rtsp` — RTSP 输入桥接
- `mediaservo-egress-hls` — LL-HLS 输出
- `mediaservo-egress-dash` — MPEG-DASH 输出
- `mediaservo-egress-whep` — WHEP/WebRTC 输出
- `mediaservo-egress-moq` — MoQ over QUIC 输出
- `mediaservo-segmenter` — CMAF segmenter（fMP4 打包）
- `mediaservo-record` — 录制引擎
- `mediaservo-cluster` — 集群 gossip（可选）

**分阶段实施建议**：

| 阶段 | 内容 | 参考 |
|------|------|------|
| Phase 1 | Fragment Model + RTMP ingest + HLS egress | LVQR RTMP bridge + hls crate |
| Phase 2 | SRT ingest + WHIP/WHEP | MediaMTX SRT 配置模式 |
| Phase 3 | RTSP ingest + MPEG-DASH egress | Xiu RTSP 协议实现 |
| Phase 4 | MoQ egress + cluster + mesh | LVQR relay + cluster + mesh |
| Phase 5 | QUIC 统一传输 + GPU 硬件编码 | LVQR moq + GStreamer NVENC pipeline |

**关键决策**：
- [ ] 内部表示用 CMAF/fMP4 作为 Fragment payload（LVQR 已验证可行）
- [ ] 控制平面 async-trait，数据平面 concrete type dispatch（LVQR 原则）
- [ ] 集群用 chitchat gossip 而非 Raft（LVQR 已验证可行，且足够）
- [ ] SLO 体系按 transport 独立阈值（LVQR SLO 文档提供模板）

### 6.4 避坑指南

1. **不要用 RTMP 作为内部统一表示** — SRS 的教训：添加新协议需要扭曲适配 RTMP 语义
2. **不要 per-stream fork FFmpeg** — nginx-rtmp 的教训：并发上限极低
3. **不要在数据平面用 dyn trait** — LVQR 原则：每 Fragment 零堆分配是关键性能要求
4. **HLS segment IDR 对齐** — Twitch 教训：transmux 源码流 + 转码流混合时必须保证 IDR PTS 对齐
5. **TURN 是独立的运维负担** — SRS/MediaMTX 都验证：WebRTC 的生产部署绕不开 TURN
6. **MoQ wire 不做私有的 per-frame header** — LVQR 教训：保持 wire 格式的互操作性，用 sidecar track 传递额外数据
7. **HLS URL 路径解析必须防御式** — Xiu panic 教训：`index out of bounds` 在生产中不可接受

## 对应的决策

| 研究发现 | 对应决策 |
|---------|---------|
| RTMP/HLS/SRT 协议栈 (GStreamer) | D6, D19 |
| Origin-Edge 集群拓扑 (SRS/MediaMTX/nginx-rtmp) | D-STREAM-TOPOLOGY |
| sourceOnDemand 按需拉流 (MediaMTX) | MediaSource trait 预留 |
| GOP Cache 即时加入 (SRS/MediaMTX) | D-GOP-CACHE |
| RTMP 录制 hooks (nginx-rtmp on_publish/on_done) | D152 |
| RTP interceptor 扩展点 (Pion) | D153 |
