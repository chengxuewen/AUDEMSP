# Xiu 流媒体服务器参考文档

> 最后更新: 2026-07-19 | 源仓库: [harlanc/xiu](https://github.com/harlanc/xiu) | 最新版本: v0.13.0

## 1. 产品画像

Xiu 是一个用纯 Rust 编写的简单、高性能、安全的直播流媒体服务器，由 HarlanC（@harlanc）于 2020 年 8 月发起并维护。项目定位为"对开发者友好"的轻量级流媒体中间件，强调代码简洁、架构清晰、易于扩展。

| 维度 | 详情 |
|------|------|
| **语言** | Rust 97.7%（其余为 JS/HTML/Shell） |
| **许可证** | MIT |
| **最新版本** | v0.13.0（2024-08-11） |
| **GitHub Stars** | ~2,300 |
| **贡献者** | 30 人，570+ commits |
| **crates.io 下载** | ~43,000 |
| **官方网站** | https://rustxiu.com |
| **支持平台** | Linux / macOS / Windows |
| **运行时依赖** | tokio (async runtime) |

**核心定位：** 面向中小规模直播场景的纯 Rust 流媒体服务器，以"开箱即用"的 RTMP/RTSP/WebRTC/HTTP-FLV/HLS 协议支持和简洁的代码为卖点。

## 2. 技术特性

### 2.1 Rust Async 架构 — tokio

Xiu 基于 tokio 构建其全部异步 I/O 和并发模型：

```rust
#[tokio::main]
async fn main() -> Result<()> { ... }
```

- **默认多线程 runtime**：`#[tokio::main]`（注释掉了 `current_thread` 选项），每个 CPU 核心一个工作线程
- **流处理全异步**：所有协议处理（RTMP 连接、RTSP 会话、HTTP-FLV/HLS 分发）均在 tokio task 中运行
- **signal 处理**：使用 `tokio::signal::ctrl_c()` 优雅关闭
- **发布-订阅总线**：通过 `streamhub` crate 实现跨协议、跨连接的异步数据分发

### 2.2 模块化 Workspace 架构

项目采用 Cargo workspace 管理 15 个 crate，按职责分为 4 层：

```
harlanc/xiu (workspace)
├── application/          # 应用层
│   ├── xiu/              # 主服务入口 (main.rs + config + CLI)
│   ├── http-server/      # HTTP API / 通知服务
│   └── pprtmp/           # RTMP 播放/推流工具
├── protocol/             # 协议层 (独立 crate)
│   ├── rtmp/             # RTMP 协议实现 (0.6.5)
│   ├── rtsp/ (xrtsp)     # RTSP 协议实现 (0.3.0)
│   ├── webrtc/ (xwebrtc) # WebRTC WHIP/WHEP (0.3.5)
│   ├── httpflv/          # HTTP-FLV 分发 (0.4.5)
│   └── hls/              # HLS 分发与录制 (0.5.5)
├── library/              # 基础库层
│   ├── streamhub/        # 发布-订阅路由核心
│   ├── bytesio/          # 字节流读写抽象
│   ├── common/           # 通用类型/工具
│   ├── logger/           # 日志扩展
│   ├── container/
│   │   ├── flv/ (xflv)   # FLV 容器解析 (0.4.4)
│   │   └── mpegts/       # MPEG-TS 容器解析
│   └── codec/
│       └── h264/         # H.264 编解码解析
└── confs/                # 配置示例文件
```

**设计特点**：
- 每个协议是独立的 crate，可单独发布到 crates.io
- 协议 crate 不直接互相依赖，通过 `streamhub` 解耦
- `bytesio` 提供统一的字节流 I/O 抽象（TCP/UDP）

### 2.3 StreamHub — 发布-订阅数据总线

`streamhub` crate（v0.2.4）是 Xiu 的架构核心，实现跨协议的数据路由：

```
Publisher (RTMP/RTSP/WHIP)
         │
         ▼
    StreamsHub
    ├── publish()      — 注册发布者，接收媒体数据
    ├── subscribe()    — 注册订阅者，转发数据
    ├── unsubscribe()  — 取消订阅
    │
    └── StreamHubEventSender → 事件通知 (HTTP API callback)
         │
         ▼
Subscriber (RTMP/RTSP/HTTP-FLV/HLS/WHEP)
```

**核心能力**：
- **多协议路由**：RTMP 推流 → 自动转换成 HTTP-FLV / HLS / RTMP 拉流 / RTSP 拉流 / WHEP
- **GOP 缓存**：可配置 GOP 数量，新订阅者先收到最近的关键帧 GOP，实现快速首帧
- **协议转换（Remux）**：RTMP → HTTP-FLV（直接复用，无需重编码）、RTMP → HLS（m3u8 + ts 分段）
- **统计信息**：通过 `StatisticDataSender` 获取每路流的发布/订阅统计
- **事件通知**：通过 `BroadcastEventReceiver` 监听流的发布/停止事件（HTTP API 回调）

### 2.4 协议处理细节

#### RTMP（protocol/rtmp crate v0.6.5）
- 完整实现 RTMP 握手、chunk 分片/合并、AMF0/AMF3 编解码
- 支持 H.264/AAC 推流和拉流
- GOP 缓存：可配置 `gop_num` 控制缓存的 GOP 数量
- RTMP chunk 压缩
- 集群模式：静态 Push/Pull relay
- 已知问题：[音频流 GOP 缓存泄漏](#issue190)（audio-only stream → OOM）

#### RTSP（protocol/rtsp crate xrtsp v0.3.0）
- 支持 H.265(HEVC)/H.264/AAC
- 支持 TCP Interleaved 和 UDP 传输模式
- 支持推流和拉流
- 协议转换：RTSP → RTMP / HLS / HTTP-FLV

#### WebRTC（protocol/webrtc crate xwebrtc v0.3.5）
- WHIP（推流）：OBS 30.0+ 可直接通过 WHIP 推流到 Xiu
- WHEP（拉流）：内建 HTML/JS Web 播放器，在浏览器中直接播放
- 协议转换：WHIP → RTMP / HLS / HTTP-FLV
- 需要静态 Web 客户端文件（`webrtc/src/clients/` 目录）

#### HTTP-FLV（protocol/httpflv crate v0.4.5）
- 通过 HTTP 长连接分发 FLV 格式流（低延迟）
- 使用 axum（v0.6.10）作为 HTTP 框架
- 端口 8080

#### HLS（protocol/hls crate v0.5.5）
- 从 RTMP/RTSP 流生成 m3u8 + ts 分段
- 支持直播录制（record to HLS）
- 端口 8081

### 2.5 编解码支持

| 协议 | 视频编码 | 音频编码 | 备注 |
|------|---------|---------|------|
| RTMP | H.264 | AAC | 不支持 HEVC/H.265 和 AV1 |
| RTSP | H.264, H.265(HEVC) | AAC | 仅 RTSP 支持 H.265 |
| WebRTC | H.264 | AAC | WHIP/WHEP 模式 |
| HTTP-FLV/HLS | 继承源流编码 | 继承源流编码 | 协议转换，不重编码 |

**不支持**：AV1、VP8/VP9、Opus、SRT、GB28181

### 2.6 关键依赖

| 库 | 用途 |
|---|------|
| tokio ^1.26 | 异步 runtime |
| axum ^0.6.10 | HTTP 服务框架（HTTP API + HTTP-FLV + HLS） |
| clap ^4.1 | CLI 参数解析 |
| serde + serde_json | 序列化 / 配置解析 |
| toml ^0.5.8 | TOML 配置文件解析 |
| log + env_logger_extend | 日志系统（支持日志轮转） |
| anyhow | 应用级错误处理 |
| reqwest ^0.11.24 | HTTP 通知回调客户端 |

### 2.7 编译优化

```toml
[profile.release]
codegen-units = 1
lto = true
```

单 codegen unit + LTO，优化二进制体积和运行时性能。

## 3. 关键能力

### 3.1 协议转换矩阵

| 源 → 目标 | RTMP | RTSP | HTTP-FLV | HLS | WHIP/WHEP |
|-----------|------|------|----------|-----|-----------|
| RTMP 推流 | ✅ | ✅ | ✅ | ✅ | ✅ (WHEP) |
| RTSP 推流 | ✅ | ✅ | ✅ | ✅ | ❌ |
| WHIP 推流 | ✅ | ❌ | ✅ | ✅ | ✅ (WHEP) |

协议转换均为**无转码的 remux/repackaging**，不涉及编解码，CPU 开销低。

### 3.2 集群 / Relay

支持两种静态 relay 模式：

**Static Push（推模式）**：
- 源节点推流 → 自动转发到配置的下游节点
- 配置示例：`[[rtmp.push]] enabled = true, address = "node2", port = 1935`

**Static Pull（拉模式）**：
- 下游节点有播放请求时，从上游节点拉流
- 配置示例：`[rtmp.pull] enabled = true, address = "node1", port = 1935`

**限制**：仅支持静态配置，不支持动态发现/负载均衡/自动故障转移。

### 3.3 HTTP API / 通知

- **查询接口**：查询当前活跃的流列表、流信息
- **通知回调**：流发布/停止时向配置的 HTTP URL 发送事件通知
- **Token 认证**：支持推流/拉流的 token 验证

### 3.4 部署方式

```
# 1. Cargo 安装
cargo install xiu
xiu -r 1935 -f 8080 -s 8081

# 2. Docker
docker run -d --net=host --name xiu harlancn/xiu:latest /app/start.sh /app/config.toml

# 3. 源码编译
git clone https://github.com/harlanc/xiu.git && cd xiu
make local && make build
./application/xiu/target/release/xiu -c config.toml

# 4. Docker Compose 集群
docker compose up -d  # 3 节点集群
```

### 3.5 配置系统

支持两种配置方式（互斥）：

1. **配置文件**（TOML）：`xiu -c config.toml`
2. **命令行参数**：`xiu -r 1935 -t 554 -w 8900 -f 8080 -s 8081 -l info`

提供 4 个预设配置模板：
- `config_rtmp.toml` — 仅 RTMP
- `config_rtmp_hls.toml` — RTMP + HLS
- `config_rtmp_httpflv.toml` — RTMP + HTTP-FLV
- `config_rtmp_httpflv_hls.toml` — 全协议

## 4. 部署与运维

### 4.1 典型部署拓扑

```
┌──────────┐     RTMP/WHIP      ┌──────────┐     HTTP-FLV/HLS     ┌──────────┐
│  OBS /   │ ──────────────────▶│          │ ────────────────────▶│  Web/App │
│  FFmpeg  │                    │   XIU    │                      │  Player  │
└──────────┘                    │  Server  │◀──── RTMP/RTSP ──────└──────────┘
                                │          │
                                └────┬─────┘
                                     │ RTMP push relay
                                ┌────▼─────┐
                                │   XIU    │
                                │  Node 2  │
                                └──────────┘
```

### 4.2 运维要点

| 方面 | 说明 |
|------|------|
| **资源占用** | 单二进制 <20MB，内存取决于活跃流数和 GOP 缓存配置 |
| **日志** | 支持日志级别（trace~error）、文件输出、日志轮转（按小时/天） |
| **监控** | HTTP API 查询流状态；HTTP 回调通知推流/断流事件；无内建 metrics 导出 |
| **优雅关闭** | `Ctrl+C` 触发 `tokio::signal::ctrl_c()`，停止 logger |
| **Docker** | 官方镜像 `harlancn/xiu:latest`，使用 host 网络模式 |

### 4.3 已知运维风险

- **音频流 GOP 内存泄漏**：[issue #190](https://github.com/harlanc/xiu/issues/190) — 纯音频 RTMP 推流时 GOP 缓存无限增长，24小时内可泄漏 ~1.5GB/天。临时规避：设置 `gop_num = 0` 关闭 GOP 缓存。
- **无实时监控**：没有 Prometheus/Grafana 集成，需要自行对接 HTTP API
- **无动态配置**：修改配置需要重启服务

## 5. 生态与市场

### 5.1 社区活跃度

| 指标 | 数据 |
|------|------|
| GitHub Stars | ~2,300 |
| 贡献者 | 30 |
| 最新 release | 2024-08（v0.13.0） |
| 最后推送 | 2026-03（仍有维护） |
| crates.io 下载 | 43,000+ |
| 文档 | rustxiu.com（Docusaurus） |

项目由单一维护者（HarlanC）主导，有 30 位社区贡献者参与。更新频率中等，最近一次代码推送在 2026 年 3 月。Discord 社区存在但规模较小。

### 5.2 市场定位

Xiu 在 Rust 流媒体生态中处于独特位置：

| 对比项 | Xiu (Rust) | SRS (Go) | Monibuca v6 (Rust) | Node-Media-Server (Node.js) | nginx-rtmp (C) |
|--------|-----------|----------|-------------------|---------------------------|----------------|
| 协议覆盖 | RTMP/RTSP/WHIP/FLV/HLS | RTMP/HLS/WebRTC/GB28181 | 全部 8 协议 | RTMP/HTTP-FLV/WS-FLV | RTMP |
| 集群 | 静态 relay | 完善的 origin/edge | 企业级集群 | 无 | relay |
| 运维面板 | 无 | 简单 HTTP | 完整 Admin 面板 | REST API | 无 |
| 性能目标 | 轻量级 | 高性能 CDN 边缘 | 企业级（10K+ 流） | 中低负载 | 高并发 |
| 定位 | 学习友好 / 快速集成 | CDN 源站 | 企业全栈 | 快速原型 | 稳定 relay |

### 5.3 典型适用场景

- ✅ 个人/小团队直播服务
- ✅ 开发测试环境（Rust 技术栈集成）
- ✅ 学习 Rust 流媒体编程的参考实现
- ✅ RTMP → HLS/HTTP-FLV 协议转换网关
- ⚠️ 中小规模生产环境（需要注意音频流内存问题）
- ❌ 大规模 CDN 边缘节点（建议 SRS）
- ❌ 企业级全协议方案（建议 Monibuca v6）

## 6. 亮点与局限

### 6.1 亮点

1. **纯 Rust 实现**：内存安全、无 GC、编译时保证并发安全。对于同样使用 Rust 技术栈的 MediaServo 具有直接参考价值。
2. **代码架构清晰**：15 个 crate 的 workspace 分层明确（应用层/协议层/库层），每个协议独立可复用。
3. **StreamHub 设计优雅**：发布-订阅模式解耦协议实现，新增协议只需对接 StreamHub 接口。
4. **协议转换无重编码**：remux/repackaging 模式，CPU 开销极低。
5. **开箱即用**：cargo install 一键安装，CLI 参数灵活，Docker 支持。
6. **跨平台**：Linux/MacOS/Windows 全支持。
7. **代码简洁**：以教育和集成友好为目标，即使初学者也能理解。

### 6.2 局限

1. **编解码支持有限**：RTMP 侧不支持 H.265/AV1；WebRTC 侧不支持 VP8/VP9/Opus；无 SRT、GB28181。
2. **集群能力弱**：仅静态 relay，无动态发现、负载均衡、故障转移。
3. **运维工具缺失**：无 Prometheus metrics、无内置 Admin UI、无健康检查端点。
4. **单维护者风险**：核心开发依赖一人，长期演进可持续性不确定。
5. **音频流 OOM bug**：生产环境的严重缺陷，已报告但尚未修复（见 issue #190）。
6. **WebRTC 不完整**：WHIP/WHEP 基础实现，无 SFU、无 simulcast、无 TURN/STUN 内建支持。
7. **录制功能简单**：仅支持 HLS 录制，无 MP4/FLV 录制。
8. **没有插件系统**：扩展协议需要修改源码，无动态加载机制。
9. **错误处理使用 anyhow**：应用级代码使用 anyhow，不适合作为库的精细错误处理。

## 7. 对 MediaServo 的参考价值

### 7.1 可直接借鉴的模式

1. **Workspace 分层架构**（★★★★★）
   - Xiu 的 `application/protocol/library` 三层划分与 MediaServo 的 `host/remote/server` 三层有结构上的相似性
   - 建议参考其协议 crate 的独立性和 `streamhub` 的发布-订阅解耦模式

2. **StreamHub 发布-订阅模式**（★★★★★）
   - `publish()` / `subscribe()` / `unsubscribe()` 接口设计简洁高效
   - MediaServo 的 PipelineEngine 可以借鉴此模式，用通道（channel）连接采集→编码→推流节点
   - `streamhub` 的 `StatisticDataSender` 统计接口值得复用

3. **bytesio 字节流抽象**（★★★★）
   - 统一的 TCP/UDP 字节流读写抽象，减少协议层代码重复
   - MediaServo 中采集源（屏幕/摄像头）到编码器再到网络推流，同样需要统一的字节流抽象

4. **协议转换的 Remux 策略**（★★★★）
   - 不重编码的协议转换（RTMP→HTTP-FLV 直接复用 FLV 数据、RTMP→HLS 仅重封装为 TS）
   - MediaServo 可能需要 RTMP 输出 → HTTP-FLV 分发的场景，remux 模式可最小化 CPU 开销

5. **配置系统设计**（★★★）
   - TOML 配置文件 + CLI 参数双模式，互斥处理
   - GOP 缓存、端口、日志等可配置项

6. **tokio 多线程 runtime**（★★★★）
   - 默认多线程 tokio runtime，利用多核
   - `tokio::signal` 优雅关闭模式
   - 日志轮转（`env_logger_extend`）

### 7.2 应当避免的设计

1. **避免使用 anyhow 在库代码中**（★★★）
   - Xiu 大量使用 `anyhow` 和 `failure` crate，不利于库使用者进行精细的错误处理
   - MediaServo 应使用 `thiserror`（库 crate）和 `anyhow`（应用 crate）的分层策略

2. **避免 GOP 缓存的内存泄漏模式**（★★★）
   - 音频流 GOP 无限增长的 bug 说明：缓存必须有基于时间/大小的硬上限，不能仅依赖关键帧触发淘汰
   - MediaServo 的 GOP 缓存设计应包含强制淘汰策略和内存预算

3. **避免静态配置的 relay**（★★）
   - 静态 relay 配置缺乏弹性
   - MediaServo 的 Server 应有动态会话管理和路由能力

4. **避免无内建的运维可观测性**（★★）
   - 缺少 metrics、health check、admin panel
   - MediaServo 应在设计初期就考虑可观测性

### 7.3 作为 Rust 流媒体的参考基线

Xiu 是目前 Rust 流媒体生态中**最简洁、最易读的参考实现**，特别适合：

- 理解 RTMP/RTSP 协议在 Rust 中的实现方式
- 理解 tokio 在流媒体场景中的使用模式
- 作为 MediaServo MVP 阶段的架构对照——Xiu 的代码量适中，可以快速阅读并提取有用模式
- 对比评估：当 MediaServo 需要更高级功能（动态路由、编解码管线、插件系统）时，需要比 Xiu 更深入的架构设计

### 7.4 具体可复用的 crate

| Xiu crate | 用途 | MediaServo 复用价值 |
|-----------|------|-------------------|
| `streamhub` | 发布-订阅路由 | 高 — Pipeline 节点间通信模式 |
| `bytesio` | 字节流 I/O 抽象 | 高 — 统一网络读写接口 |
| `rtmp` | RTMP 协议 | 中 — 如果需要 RTMP 输出 |
| `xflv` | FLV 容器 | 中 — FLV 分装/解封 |
| `xmpegts` | MPEG-TS 容器 | 中 — HLS 分片 |
| `h264` | H.264 解析 | 低 — MediaServo 将使用系统编解码器 |

> **核心建议**: MediaServo 不需要直接 fork 或依赖 Xiu 的 crate，但应认真学习其架构思想，尤其是 StreamHub 的解耦模式和 workspace 分层组织方式。

---
**相关决策**: D5 (Fragment Model), D6, D19

## 参考来源

- GitHub: https://github.com/harlanc/xiu
- 官方文档: https://rustxiu.com
- crates.io: https://crates.io/crates/xiu
- streamhub docs: https://docs.rs/streamhub
- 已知问题: [issue #190 - GOP cache OOM for audio-only streams](https://github.com/harlanc/xiu/issues/190)
