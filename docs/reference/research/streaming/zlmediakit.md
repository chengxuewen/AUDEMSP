# ZLMediaKit 参考分析
> 生成日期：2026-07-19 | 分类：流媒体

## 1. 产品画像
- **名称**：ZLMediaKit
- **开发者**：夏楚 (xia-chu / ZeroLogic) 主导开发，社区贡献者约 30+ 人。核心维护者 xia-chu 贡献超过 3600+ commits，另有 xiongguangjie（200+ commits）、wasphin（63+ commits）等活跃贡献者
- **首次发布**：2017-04-01（GitHub 仓库创建）。持续开发超过 9 年，2017 年 11 月已有生产环境使用案例
- **当前版本**：持续滚动更新（master 分支），无固定大版本号策略。同时提供闭源专业版 zlmediakit-pro（WebRTC 集群、AI 推理等增强功能）
- **Star 数**：~17.3K（2026-07）
- **许可**：MIT + 补充协议（必须保留 "ZLMediaKit" 品牌信息，不得去除 Server/User-Agent 等字段中的版权声明）
- **产品定位**：高性能运营级流媒体服务框架。定位为「视频监控协议栈与直播协议栈之间的桥梁」，从底层 C++11 网络库 (ZLToolKit) 到上层 MediaServer 完整覆盖。既可「开箱即用」作为独立流媒体服务器部署，也可作为 C/C++ SDK 嵌入其他应用二次开发
- **目标用户群体**：安防监控系统集成商（GB28181/RTP 摄像头接入与协议转换），直播平台与 CDN 供应商（高并发 RTMP/HLS/HTTP-FLV 分发），物联网视频应用开发者（多平台嵌入 SDK），WebRTC 实时通信应用（SFU 场景），C++ 流媒体二次开发团队
- **官网**：https://docs.zlmediakit.com | **仓库**：https://github.com/ZLMediaKit/ZLMediaKit

## 2. 技术特性

### 整体架构

```
┌──────────────────────────────────────────────────────────────────────┐
│                    ZLMediaKit (C++11 多线程架构)                        │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐      │
│  │          EventPollerPool (事件循环线程池)                      │      │
│  │                                                              │      │
│  │  启动时按 CPU 核心数创建 N 个 EventPoller 实例                 │      │
│  │  每个 EventPoller = 1 个 epoll 实例 + 1 个线程运行 epoll_wait │      │
│  │  每个 TcpSession 只绑定一个 EventPoller（准单线程、无锁操作）   │      │
│  │  Linux: epoll | 其他平台: select                              │      │
│  └─────────────────────────────────────────────────────────────┘      │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐      │
│  │          WorkThreadPool (后台工作线程池)                       │      │
│  │                                                              │      │
│  │  CPU 密集型任务投递（编解码、截图、录制等）                      │      │
│  │  与 EventPoller 分离，避免阻塞 IO 事件循环                      │      │
│  └─────────────────────────────────────────────────────────────┘      │
│                                                                       │
│  ┌──────────┬──────────┬──────────┬──────────┬──────────────┐         │
│  │ RtspSess │ RtmpSess │ HttpSess │ RtcTrans │ GB28181Sess  │         │
│  │ ion      │ ion      │ ion      │ port     │              │         │
│  ├──────────┴──────────┴──────────┴──────────┴──────────────┤         │
│  │                   Protocol Normalization                   │         │
│  │         内部 RTP ←→ 内部 Frame (H.264/H.265/AAC/OPUS)      │         │
│  │         RTSP/WebRTC 共享底层 RTP 路径，免转协议开销          │         │
│  └───────────────────────────────────────────────────────────────┘      │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐      │
│  │          TcpServer (多线程 Accept 负载均衡)                   │      │
│  │                                                              │      │
│  │  listen fd 加入所有 EventPoller → 内核自动选择最空闲线程      │      │
│  │  accept 后 TcpSession 绑定到该 EventPoller 直至连接关闭          │      │
│  └─────────────────────────────────────────────────────────────┘      │
└──────────────────────────────────────────────────────────────────────┘
```

### 线程模型

```
TaskExecutor (ThreadPool 基类)
    └─ EventPoller (事件轮询器)
           ├─ _epoll_fd: epoll 实例 fd
           ├─ _pipe_fd[2]: pipe 用于唤醒 epoll_wait
           ├─ _event_map: fd → 事件回调映射
           ├─ _list_task: 异步任务队列（批量线程切换）
           ├─ _delay_task_map: 延时任务（按执行时间排序）
           └─ runLoop(): epoll_wait + 事件分发 + 任务执行循环
    └─ EventPollerPool (单例，管理 N 个 EventPoller，N=CPU核心数)
```

ZLMediaKit 采用**单进程多线程 + IO 多路复用 + 非阻塞**模型：

- **准单线程无锁**：每个 TcpSession 由固定一个 EventPoller 掌管生命周期，其他线程不得直接操作。因此 TcpSession 内的网络 IO 操作无需互斥锁保护，锁粒度消减至极致
- **批量线程切换**：媒体数据分发时，先在 _dispatcherMap 中按目标线程聚合，然后一次性向每个目标线程投递批处理任务，N 个目标线程只需要 N 次线程切换（而非每个客户端一次切换），大幅减少上下文切换开销
- **零拷贝转发**：利用 C++11 shared_ptr 引用计数，多个目标连接共享同一份数据缓冲区，常规多线程编程中的内存拷贝完全消除
- **对象循环池**：RTP 包等高频分配的对象使用循环池复用，减少 new/delete 调用（全局互斥的内存分配是性能瓶颈）
- **sendmsg 批量发送**：使用 sendmsg/writev 系统调用合并发送多块数据，减少系统调用次数
- **Socket 优化**：握手期间开启 TCP_NODELAY 提高延时，握手后关闭 TCP_NODELAY 开启 MSG_MORE 减少 ACK 包数量、提高带宽利用率
- **与 SRS 对比**：SRS 为单线程多协程模型 (State Threads)，同一个 CPU 核心上运行，无法充分利用多核。ZLMediaKit 为多线程模型，每个 CPU 核心一个线程，可以榨干多核性能。低负载下 ZLMediaKit 单线程性能约为 SRS 的 50%（SRS 有合并写特性缓存 300ms 后批量发送），但真实网络环境 + 高负载下差距缩小，多核综合性能远超 SRS

### 协议归一化

RTSP 和 WebRTC 底层都基于 RTP，ZLMediaKit 内部可直接互联，无需复杂的解复用/封装逻辑。其他协议（RTMP、HLS、HTTP-FLV 等）通过内部 Frame 数据结构转换，支持 H.264/H.265/AAC/G711/OPUS/MP3/VP8/VP9/AV1/MP2 编码的转协议，不支持的编码仅做透传转发。

### 模块组织

| 模块 | 说明 |
|------|------|
| ZLToolKit (基础库) | Thread/Poller/Network/Util 四大部分，提供 EventPoller 事件引擎、TcpServer/TcpSession 网络抽象、异步任务框架 |
| server/ | MediaServer 主程序，开箱即用的流媒体服务器 |
| src/ | 核心 API：Rtsp/Rtmp/Hls/Http/WebRTC/GB28181/Record 等模块 |
| api/ | C API SDK，供其他语言调用 |
| tests/ | 测试工具：test_benchmark（性能压测）、test_bench_pull（拉流压测）、test_bench_push（推流压测）、test_server（测试服务器） |
| webrtc/ | WebRTC 实现（独立子模块） |
| srt/ | SRT 协议支持（独立子模块） |

### 关键设计决策

- **智能指针管理内存**：全面使用 C++11 shared_ptr/weak_ptr，避免裸指针，在线程切换时完美管理多线程下内存共享与生命周期
- **按需解复用**：仅在存在观看者时才开启转协议/解复用，无人观看自动关闭，降低 CPU 占用。通过 hook 事件 (on_stream_none_reader) 配合业务逻辑实现智能关流
- **先播后推 (play before publish)**：播放器请求不存在的流时，服务器不立即返回错误，而是挂起等待推流。推流上线后立即返回成功——降低视频打开延迟，改善用户体验
- **断连续推**：推流端异常断开后，服务器延迟回收媒体源资源（可配置超时时间），若推流端在超时内重连则复用资源，播放器无感知
- **HLS 长连接模拟**：通过 HTTP Cookie 追踪技术将 HLS 的无状态分段请求模拟为「长连接」，实现 HLS 按需拉流、播放鉴权（无需重复鉴权）、播放流量统计
- **编码格式对称性**：WebRTC 支持的编码格式与 RTSP 协议一致（H.264/H.265/AAC/G711/OPUS/MJPEG/MP3/VP8/VP9/AV1），简化协议互转
- **enhanced-rtmp 支持**：不仅支持传统 RTMP-H265/OPUS，还支持 veovera enhanced-rtmp 标准（H.265/VP8/VP9/AV1/OPUS）

## 3. 关键能力

### 协议支持总览

| 协议 | 角色 | H264 | H265 | AAC | G711 | OPUS | VP8 | VP9 | AV1 | 备注 |
|------|------|------|------|-----|------|------|-----|-----|-----|------|
| RTSP[S] | Server/Pusher/Player | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 4 种 RTP 传输模式；Basic/Digest 鉴权 |
| RTMP[S] | Server/Pusher/Player | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | enhanced-rtmp 支持 |
| HLS (mpegts/fmp4) | Server/Player | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 多轨道模式；Cookie 长连接追踪 |
| HTTP-FLV | Server/Player | ✅ | ✅ | ✅ | ✅ | ✅ | — | — | — | WebSocket-FLV 也支持 |
| HTTP-TS | Server | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | WebSocket-TS 也支持 |
| HTTP-fMP4 | Server | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | WebSocket-fMP4 也支持 |
| MP4 | 点播/录制 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 支持 seek；多轨道录制 |
| WebRTC | SFU/Client | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | WHIP/WHEP；Simulcast；RTCDataChannel |
| GB28181/RTP | Server/Client | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 双向语音对讲；PS/TS/ES/EHOME |
| SRT | Server/Client | ✅ | ✅ | ✅ | ✅ | ✅ | — | — | — | TS 透传模式 |
| STUN/TURN | Server | — | — | — | — | — | — | — | — | 内建 STUN/TURN 服务 |

### WebRTC SFU 特性

ZLMediaKit 的 WebRTC 实现是其最具竞争力的模块之一，在开源界有多个独有特性：

- **单端口 + 多线程 + 连接迁移**：通过重复 bind/connect 操作，为每个客户端分配唯一的 fd 并均匀分配到各线程。单 UDP 端口承载所有 WebRTC 客户端，突破传统多端口方案的 6 万端口限制（理论上可承载百万级客户端）。当用户网络切换（WiFi→4G）时通过 STUN 包锁定，支持无缝连接迁移——开源界唯一同时具备这三项能力的方案
- **ICE-Full 支持**：可作为 WebRTC 客户端主动拉流/推流，支持 P2P 模式
- **Simulcast + RTX/NACK**：上下行丢包重传，配合优秀的 jitter buffer 算法，抗丢包能力卓越
- **TWCC 动态码率控制**：基于 Transport-Wide CC 的自适应码率调节
- **GOP 缓冲秒开**：缓存最近的 GOP，实现 WebRTC 播放秒开
- **WHIP/WHEP 协议**：支持标准化的 WebRTC 推拉流信令协议

### 录制能力

- 支持录制为 FLV、HLS (mpegts)、MP4 三种格式
- 支持通过 HTTP API 触发录制，支持实时截图并返回
- 支持多轨道录制（多路音视频流）

### 集群部署

- **溯源模式**：源站 (Origin) + 边沿站 (Edge) 架构，通过配置文件启用
- **溯源协议**：支持 RTSP、RTMP、HLS、HTTP-TS、HTTP-FLV 多种溯源方式
- **负载均衡**：源站支持多地址，采用 Round Robin 方式选择
- **边沿站 HLS**：ZLMediaKit 独有的 HLS 边沿支持，SRS 边沿不支持 HLS
- **专业版增强**：WebRTC 集群支持 RTC 流量代理，解决 K8s 部署时信令与媒体流无法命中同一 Pod 的问题

### API 与监控

- **RESTful API**：完善的 HTTP API，涵盖流管理、录制控制、截图、线程负载查询、配置热加载等
- **Web Hook**：事件驱动的回调机制，支持推流鉴权 (on_publish)、播放鉴权 (on_play)、流注册/注销 (on_stream_changed)、无人观看 (on_stream_none_reader)、流未找到 (on_stream_not_found)、流量统计 (on_flow_report) 等事件
- **Telnet 调试**：支持简单的 Telnet 命令行调试接口
- **热加载**：配置文件和 SSL 证书支持热加载，无需重启服务

## 4. 部署与运维

### 部署模式

- **MediaServer 二进制**：直接编译或下载预编译的可执行文件，一条命令启动
- **C/C++ SDK 嵌入**：将 ZLMediaKit 作为库链接到自己的 C++ 应用中，自定义业务逻辑
- **C API 封装**：通过 C API SDK 供 Go、C#、Python、Java 等语言调用
- **Docker**：官方提供 `zlmediakit/zlmediakit` Docker 镜像，Docker Hub 可直接拉取

```bash
# Docker 部署示例
docker run -d \
  -p 1935:1935 -p 8080:80 -p 8554:554 \
  -p 10000:10000/udp -p 8000:8000/udp \
  zlmediakit/zlmediakit
```

### 配置格式

使用 INI 配置文件格式（基于 mINI 解析器）。配置文件路径默认为执行目录下的 `config.ini`。主要配置区块：

| 配置区块 | 说明 |
|---------|------|
| [http] | HTTP 服务器端口、跨域、虚拟主机等 |
| [rtmp] | RTMP 服务器端口 |
| [rtsp] | RTSP 服务器端口、超时设置 |
| [rtc] | WebRTC 配置、STUN/TURN 地址、端口范围 |
| [rtp] | RTP 代理、GB28181 相关配置 |
| [rtp_proxy] | RTP 推流代理配置 |
| [hls] | HLS 分片时长、文件保留策略 |
| [record] | 录制路径、分片大小、文件类型 |
| [general] | 通用配置：流超时、合并写开关、虚拟主机、溯源集群 |
| [srt] | SRT 协议配置 |
| [api] | API 密钥、调试开关 |
| [hook] | Web Hook 回调地址、超时、重试 |
| [ffmpeg] | FFmpeg 可执行文件路径（用于拉流代理） |
| [cluster] | 集群模式配置、溯源地址、超时 |

### 监控与观测

- **线程负载 API**：`/index/api/getThreadsLoad` 获取 EventPoller 线程负载；`/index/api/getWorkThreadsLoad` 获取后台工作线程负载
- **性能指标**：每个线程返回 load（负载百分比）、delay（延迟毫秒）、fd_count（监听的文件描述符数量）
- **流量统计 Hook**：通过 `on_flow_report` 事件获取每路流的流量数据
- **播放统计**：通过 `on_stream_none_reader` 事件和观众统计功能实现播放量监测

### 平台支持

- **操作系统**：Linux、macOS、Windows、iOS、Android（全平台）
- **CPU 架构**：x86、ARM、RISC-V、MIPS、龙芯 (LoongArch)、申威 (SW)
- **编译要求**：CMake 3.1+、支持 C++11 的编译器 (GCC 4.8+ / Clang 3.3+ / MSVC 2015+)

## 5. 生态与市场

### 社区活跃度

- GitHub Stars：~17.3K，Forks：~4.1K
- Open Issues：~130+（活跃维护中）
- 贡献者：核心 30+ 人，主要由中国开发者社区驱动
- 文档：docs.zlmediakit.com 在线文档（中英文）；Wiki 包含性能测试、API 参考、常见问题

### 第三方生态项目

| 项目 | 说明 |
|------|------|
| wvp-GB28181-pro | Go 实现的 GB28181 信令服务器，与 ZLMediaKit 深度集成 |
| ZLMRTCClient | WebRTC 客户端 SDK (iOS/Android) |
| ZLMediaKit-CSharp-API | C SDK 的完整 C# 包装库 |
| node-zlmediakit | Node.js 版本的 HTTP API 客户端 |
| metaRTC | 国产 WebRTC SDK，与 ZLMediaKit 配合使用 |
| ZLMDataView | ZLMediaKit 管理控制台 (Vue.js) |

### 采纳案例

- **安防监控**：大量 GB28181 摄像头接入场景，通过 wvp-GB28181-pro + ZLMediaKit 组合方案
- **大疆无人机视频流**：在 4K/30fps 视频场景下有实测数据，WebRTC 延迟 112±18ms（局域网）远优于 SRS 的 238±32ms
- **智能车载**：车载摄像头 RTMP 推流 + 先播后推机制，解决车载设备上线延迟问题
- **IoT 视频**：ARM 嵌入式平台部署，RISC-V/龙芯/申威等国产化平台支持
- **企业直播**：高并发 RTMP/HLS/HTTP-FLV 分发，支持万级并发

### 竞品对比

| 维度 | ZLMediaKit | SRS | Janus | mediasoup | Live555 |
|------|-----------|-----|-------|-----------|---------|
| 协议丰富度 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐ | ⭐⭐ |
| 单机并发 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐ |
| WebRTC SFU | ⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ❌ |
| 跨平台 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐ |
| 安防协议 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | ❌ | ❌ | ⭐⭐⭐ |
| 二次开发 | ⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐ |
| 社区规模 | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐ |
| 学习曲线 | 中等 | 低 | 中高 | 高 | 高 |

## 6. 亮点与局限

### 亮点 (Strengths)

| 亮点 | 说明 |
|------|------|
| **多核并发性能** | 多线程模型可充分利用多核 CPU，单机支持 10 万级播放器、100Gb/s 带宽，RTMP 拉流可达 3 万路 7Gb/s |
| **WebRTC 单端口多线程** | 开源界唯一同时支持单 UDP 端口 + 多线程 + 客户端连接迁移的方案，突破 6 万端口限制 |
| **零拷贝 + 无锁设计** | 基于智能指针引用计数的零拷贝转发 + TcpSession 准单线程无锁操作，极致性能 |
| **全协议覆盖** | 同时覆盖直播协议栈 (RTMP/HLS/HTTP-FLV) 和安防协议栈 (RTSP/GB28181/RTP)，打通监控与直播 |
| **跨平台全架构** | Linux/macOS/Windows/iOS/Android + x86/ARM/RISC-V/MIPS/龙芯/申威，适配国产化要求 |
| **生产级可靠性** | 多年商用验证，valgrind 长期内存安全测试，完善的热加载、断连续推、集群功能 |
| **先播后推 (Play before Publish)** | 独创的播放挂起等待机制，显著降低视频打开延迟，提升 IoT/车载场景体验 |
| **HLS 长连接模拟** | 通过 Cookie 追踪将无状态 HLS 分段请求模拟为长连接，实现播放鉴权、流量统计、按需拉流 |
| **RTSP + WebRTC 内部 RTP 互通** | 共享底层 RTP 路径，免协议转换开销，适合大规模低延迟 WebRTC 直播 |

### 局限 (Limitations)

| 局限 | 说明 |
|------|------|
| **无转码能力** | 不支持服务端视频转码，要求端侧提供目标编码格式。若需码率自适应或格式转换需外部 FFmpeg 配合（但支持 FFmpeg 拉流代理） |
| **单线程低负载性能略低** | 低负载 + 理想网络下，单线程性能约为 SRS 的 50%（因无合并写优化），但高负载多核场景优势明显 |
| **中文社区为主** | 文档和社区讨论以中文为主，国际化程度不及 SRS/Janus |
| **闭源专业版** | 高级功能（WebRTC 集群、AI 推理等）需付费使用闭源专业版 |
| **配置复杂度** | 功能模块繁多，INI 配置文件选项超过 100+ 项，完整掌握需要较陡学习曲线 |
| **无官方管理 UI** | 不提供 Web 管理后台，需依赖第三方项目 (ZLMDataView) 或自行开发 |
| **编解码器支持有限** | 不支持 AV1 在 RTMP 下的透传（仅 enhanced-rtmp 支持），部分协议组合不支持某些编码转换 |
| **许可补充条款** | MIT 补充协议要求保留 ZLMediaKit 品牌信息，对白标/OEM 场景有限制 |

## 7. 对 MediaServo 的参考价值

### 值得采纳 (What to Adopt)

- **EventPoller 多线程模型**：MediaServo 的 Host/Server 端可以考虑类似的多线程事件循环架构。每个 CPU 核心一个 EventPoller，TcpSession 绑定到固定线程的无锁模式是经过验证的高并发方案。Rust 中可用 tokio 的 `tokio::spawn` + `Runtime::new()` 模拟类似效果
- **RTSP + WebRTC 共享 RTP 路径**：MediaServo 同时需要支持监控摄像头接入 (RTSP/ONVIF) 和 WebRTC 遥操作，可借鉴 ZLMediaKit 的 RTP 归一化设计，避免 RTSP→WebRTC 转换时重复解封装/封装
- **先播后推机制**：MediaServo 的车端推流场景（车辆摄像头推流到云端）正好需要「播放请求先到、推流后到」的能力。ZLMediaKit 的 play-before-publish 挂起等待机制可直接参考
- **断连续推**：边缘设备网络不稳定是常态。推流端断开后延迟回收资源并支持无感知重连的能力对 MediaServo 的车端/移动端场景至关重要
- **批量线程切换 + 零拷贝**：媒体数据分发到多个 Peer 时，按线程聚合再批量投递任务 + shared_ptr 共享缓冲区的模式，在 Rust 中可用 Arc<[u8]> + `tokio::spawn` 实现类似效果

### 需要适配 (What to Adapt)

- **协议归一化层**：ZLMediaKit 内部以 RTP/Frame 为核心归一化格式。MediaServo 可根据自身需求选择归一化格式（建议以 RTP 为核心，因为 MediaServo 的 WebRTC 遥操作和 RTSP 摄像机接入都基于 RTP）
- **录制与回放**：MediaServo 的远程桌面录制可以借鉴 ZLMediaKit 的多格式录制 (FLV/MP4/HLS) 设计，结合 MediaServo 自身的 GPU 编码能力
- **WebRTC 单端口多线程**：如果 MediaServo 的 WebRTC 服务需要承载大量客户端，ZLMediaKit 的单端口多线程方案是最好的参考实现。Rust 端可使用 `SO_REUSEPORT` + `connect` 绑定实现类似效果
- **集群溯源模式**：MediaServo Server 作为信令+relay 中心，可参考 ZLMediaKit 的溯源集群模式设计边沿-源站架构。溯源协议可扩展为 MediaServo 的内部协议

### 应该避免 (What to Avoid)

- **裸 C++ 开发**：MediaServo 选用 Rust 是合理决策。ZLMediaKit 虽然是 C++ 中代码质量极高的项目（智能指针、无裸指针），但 C++ 的内存安全仍需开发者高度自律。Rust 的编译期内存安全保证适合 MediaServo 的安全敏感场景（远程桌面、车辆控制）
- **单一 INI 配置文件**：ZLMediaKit 的百项 INI 配置文件在新手部署和自动化管理时体验不佳。MediaServo 应考虑结构化配置（YAML/TOML）配合环境变量覆盖，并通过 OMO 管理面统一配置
- **无官方管理 UI**：MediaServo 作为 AUDE 生态组件，应从 Phase 1 就提供管理接口（gRPC API + Web UI），避免依赖第三方管理工具
- **无内置转码**：MediaServo 应评估是否需要服务端转码能力。对于远程桌面场景，终端能力差异大（不同分辨率、不同解码器支持），转码可能是刚需。可考虑 GPU 硬件编码的转码路径
- **C++ 依赖链**：ZLMediaKit 深度依赖 C++ 工具链（CMake、GCC/Clang），交叉编译到 ARM/MIPS 需要完整工具链。MediaServo 的 Rust + cargo 工具链在交叉编译和依赖管理上更加现代化
**相关决策**: D-STREAM-TOPOLOGY, D-GOP-CACHE
