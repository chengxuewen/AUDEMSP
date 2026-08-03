# Kurento Media Server & OpenVidu 参考研究报告

> **生成日期:** 2026-07-19
> **研究范围:** Kurento Media Server v7.x, OpenVidu v2/v3
> **仓库:**
> - [Kurento/kurento](https://github.com/Kurento/kurento) — 主仓库 (monorepo, Apache 2.0, C/C++/Java)
> - [Kurento/kurento-media-server](https://github.com/Kurento/kurento-media-server) — 历史仓库 (已归档)
> - [OpenVidu/openvidu](https://github.com/OpenVidu/openvidu) — OpenVidu 平台主仓库

---

## 1. 产品画像

### 定位
Kurento Media Server (KMS) 是一个开源的 WebRTC 媒体服务器，提供媒体传输、处理、录制和回放的全栈能力。它是整个 Kurento 生态的核心——应用开发者通过客户端 API 远程控制 KMS，构建媒体流水线来完成复杂的实时音视频处理。

OpenVidu 是构建于 Kurento 之上的高层框架，封装了 Kurento 的低级能力，提供简化的 REST API + 客户端 SDK 用于快速开发视频会议应用。

### 核心理念

| 原则 | 含义 |
|------|------|
| **媒体/信令分离** | Signaling Plane（应用服务器）与 Media Plane（KMS）完全解耦，通过 WebSocket + JSON-RPC 通信 |
| **流水线架构** | 媒体处理通过 MediaElement 链式组合实现，支持运行时动态增删 |
| **模块化黑盒** | 每个 MediaElement 是自包含的功能单元，开发者无需了解内部实现 |
| **端到端透明** | 自动媒体适配层（agnostic media adapter）在连接不兼容的元素时自动插入转码 |
| **云原生** | 支持 PaaS 部署，单应用可调用多 KMS 实例，单 KMS 亦可服务多应用 |

### 技术栈
- **核心语言:** C/C++（媒体服务器本体）、Java/Node.js（客户端 SDK）
- **媒体引擎:** GStreamer（底层多媒体框架，提供编解码、复用、滤镜等）
- **通信协议:** WebSocket + JSON-RPC（Kurento Protocol）
- **许可证:** Apache 2.0
- **架构模型:** MCU + SFU 双模式（既可混流也可转发）

### 版本演进关键节点
- **2013:** Kurento 项目启动（Universidad Rey Juan Carlos 主导）
- **2016:** OpenVidu 创建，作为 Kurento 的简化 API 层
- **~2023:** Kurento 代码迁移至 monorepo (Kurento/kurento)
- **OpenVidu v2:** 基于 Kurento 的经典架构（CE/Pro/Enterprise 三版本，CE 免费）
- **OpenVidu v3 (2024-2025):** 完全重构——底层从 Kurento 迁移至 LiveKit + mediasoup，保持 API 兼容（通过 v2compatibility 模块）

---

## 2. 技术特性

### 2.1 核心架构：三层模型

```
┌─────────────────────────────────────────────────────────────┐
│  Client Application (浏览器/移动端)                         │
│  ├── Presentation Layer: <video>, WebRTC RTCPeerConnection     │
│  └── Client-side App Logic                                 │
├─────────────────────────────────────────────────────────────┤
│  Application Server (信令层)                                │
│  ├── 业务逻辑 (Java/Node.js/Python/...)                     │
│  ├── Kurento Client SDK → WebSocket/JSON-RPC                │
│  └── Session/Connection 管理                                │
├─────────────────────────────────────────────────────────────┤
│  Kurento Media Server (媒体层)                              │
│  ├── JSON-RPC → MediaPipeline 管理                          │
│  ├── GStreamer Pipeline Engine                             │
│  └── MediaElement 实例化与编排                              │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 MediaElement 类型

Kurento 将媒体处理单元抽象为 **MediaElement**，分为四大类：

#### (1) Input Endpoints（输入端）
| Element | 功能 |
|---------|------|
| **WebRtcEndpoint** | WebRTC 双向通信端点，SDP Offer/Answer 协商 |
| **RtpEndpoint** | RTP/RTCP 双向媒体传输 |
| **HttpPostEndpoint** | 接受 HTTP POST 上传的媒体流 |
| **PlayerEndpoint** | 从文件系统/HTTP URL/RTSP URL 读取媒体注入管线 |

#### (2) Filters（过滤器）
| Element | 功能 |
|---------|------|
| **GStreamerFilter** | 通用过滤器——可将任意 GStreamer element 注入管线（仅限单个 element，不可注入 bin） |
| **FaceOverlayFilter** | 人脸检测叠加（OpenCV） |
| **ZBarFilter** | 二维码/条形码检测 |
| **OpenCVFilter** | 通用 OpenCV 过滤器基类 |
| **ChromaFilter** | 色度键抠图 |

#### (3) Hubs（集线器——管理多路媒体流）
| Hub 类型 | 功能 |
|----------|------|
| **Composite** | MCU 混流：混合所有输入音频，视频排列为网格输出 |
| **Dispatcher** | SFU 路由：任意输入端口到任意输出端口的灵活路由 |
| **DispatcherOneToMany** | 一对多广播：一个输入 → 所有输出 |

#### (4) Output Endpoints（输出端）
| Element | 功能 |
|---------|------|
| **RecorderEndpoint** | 录制媒体到文件系统（WebM/MP4） |
| **HttpEndpoint** | 通过 HTTP 对外提供媒体流 |

### 2.3 MediaPipeline 设计

```javascript
// 典型 WebRTC 录制+滤镜管线 (伪代码)
const pipeline    = await kurento.create('MediaPipeline');
const webRtcEp    = await pipeline.create('WebRtcEndpoint');
const recorder    = await pipeline.create('RecorderEndpoint', {uri: 'file:///tmp/rec.webm'});
const faceOverlay = await pipeline.create('FaceOverlayFilter');
const gstFilter   = await pipeline.create('GStreamerFilter', {command: 'videoscale ! video/x-raw,width=640'});

// 连接管线
await webRtcEp.connect(recorder);       // 录制原始流
await webRtcEp.connect(faceOverlay);    // 人脸检测
await faceOverlay.connect(gstFilter);   // 缩放
await gstFilter.connect(webRtcEp);      // 回传处理后的流
```

**关键特性：**
- **动态组合:** 元素可在媒体流动时随时插入、激活和停用
- **自动适配:** Agnostic Media Adapter 自动处理元素间媒体格式不兼容问题（如 VP8→RAW RGB 转码）
- **无环约束:** 管线拓扑无环，但可形成反馈回路（如 WebRtcEndpoint 输出回连到自己）

### 2.4 GStreamer 集成机制

Kurento 的核心媒体处理完全委托给 GStreamer：

1. **每个 KMS 进程内部维护一个或多个 GStreamer Pipeline**
2. **MediaElement 映射为 GStreamer element/bin**（如 WebRtcEndpoint 映射为 `webrtcbin`）
3. **MediaElement.connect() 映射为 GStreamer pad linking**（`gst_pad_link()`）
4. **GStreamerFilter 提供扩展入口:** 允许开发者通过 `gst_parse_launch()` 注入任意 GStreamer element

```cpp
// kms-filters/GStreamerFilterImpl.cpp 核心逻辑
GStreamerFilterImpl::GStreamerFilterImpl(...) {
    // 将用户提供的 GStreamer 命令字符串解析为 element
    filter = gst_parse_launch(command.c_str(), &error);
    // 检查是否为单个 element（拒绝 bin）
    if (GST_IS_BIN(filter)) throw ...;
    // 设置到内部 element 上
    g_object_set(element, "filter", filter, NULL);
}

// 运行时属性操作
void GStreamerFilterImpl::setElementProperty(const string &name, const string &value) {
    GParamSpec *pspec = g_object_class_find_property(G_OBJECT_GET_CLASS(gstElement), name);
    // 根据类型 (int/float/double/string/enum) 转换并设置
    g_object_set_property(G_OBJECT(gstElement), name, &value);
}
```

### 2.5 插件系统 (Kurento Modules)

Kurento 提供了完整的模块开发体系：

#### 模块类型
| 类型 | 基础 | 适用场景 |
|------|------|---------|
| **GStreamer 模块** | GstVideoFilter/GstAudioFilter | 通用媒体处理（需要 GStreamer 知识） |
| **OpenCV 模块** | OpenCVFilter | 计算机视觉、增强现实 |

#### 开发流程

```bash
# 1. 安装开发工具包
sudo apt-get install kurento-media-server-dev

# 2. 脚手架生成
kurento-module-scaffold my_filter        # GStreamer 模块
kurento-module-scaffold my_vision true   # OpenCV 模块
```

生成的文件树结构：
```
my_filter/
├── CMakeLists.txt
├── src/
│   ├── gst-plugins/          # GStreamer element 实现
│   │   ├── gstmyfilter.cpp   # 核心处理逻辑 (transform_frame_ip)
│   │   ├── gstmyfilter.h     # element 头文件
│   │   └── myfilter.c        # plugin 注册
│   ├── server/
│   │   ├── implementation/objects/
│   │   │   ├── MyFilterImpl.cpp   # JSON-RPC API 实现
│   │   │   └── MyFilterImpl.hpp
│   │   └── CMakeLists.txt
│   └── MyFilter.kmd.json     # Kurento Module Descriptor (接口定义)
```

#### KMD (Kurento Module Descriptor)
`.kmd.json` 文件定义模块的完整 API 接口：
- 构造函数参数
- 方法签名（含 JSON-RPC 映射）
- 属性（读写控制）
- 事件
- 自定义复杂类型

CMake 编译时会调用 `kurento-module-creator` 从 KMD 生成：
- C++ 服务端桩代码
- JavaScript client 绑定
- Java client 绑定

#### 部署
```bash
# 编译后安装到 KMS 所在机器
cmake .. && make && make install
# 验证安装
kurento-media-server --version   # 列出所有已安装模块
kurento-media-server --list      # 列出所有可用的 MediaObject Factory
```

> ⚠️ **ABI 兼容性约束:** 模块必须在使用相同编译器版本和系统版本（如 Ubuntu 20.04）的机器上编译，不能跨系统版本安装。

### 2.6 通信协议：Kurento Protocol

- **传输层:** WebSocket
- **RPC 框架:** JSON-RPC 2.0
- **生命周期管理:**
  - `create` — 实例化 MediaPipeline/MediaElement
  - `invoke` — 调用方法
  - `subscribe` — 订阅事件
  - `release` — 释放资源（引用计数 + GC）
  - `transaction` — 批量原子操作

### 2.7 会话建立流程（WebRTC）

```
Client                App Server              Kurento MS
  │                       │                      │
  │  SDP Offer + 业务请求  │                      │
  │──────────────────────►│                      │
  │                       │  create pipeline     │
  │                       │  + WebRtcEndpoint    │
  │                       │─────────────────────►│
  │                       │  processOffer(SDP)   │
  │                       │─────────────────────►│
  │                       │  ◄── SDP Answer ──── │
  │  SDP Answer           │                      │
  │◄──────────────────────│                      │
  │                       │                      │
  │══════════ ICE/DTLS/SRTP 媒体通道 ═══════════│
  │                       │                      │
```

---

## 3. 关键能力

### 3.1 媒体处理能力

| 能力 | 支持情况 | 说明 |
|------|---------|------|
| **WebRTC SFU** | ✅ | 基于 Hub(Dispatcher) 实现路由转发 |
| **WebRTC MCU** | ✅ | 基于 Hub(Composite) 实现混流+网格布局 |
| **录制** | ✅ | RecorderEndpoint → WebM/MP4 文件 |
| **回放** | ✅ | PlayerEndpoint ← 文件/HTTP/RTSP |
| **转码** | ✅ | 自动适配层 + 显式 GStreamerFilter 控制 |
| **视频滤镜** | ✅ | FaceOverlay, Chroma, ZBar, GStreamerFilter |
| **计算机视觉** | ✅ | OpenCVFilter 基类 + 自定义 OpenCV 模块 |
| **RTSP 推拉流** | ✅ | PlayerEndpoint(RTSP), RtpEndpoint |
| **RTP 传输** | ✅ | RtpEndpoint |
| **广播 (直播)** | ✅ | 通过 OpenVidu 推送到 RTMP（YouTube/Twitch） |
| **屏幕共享** | ✅ | WebRTC getDisplayMedia |

### 3.2 编解码器支持
- **视频:** VP8, VP9, H.264, H.263（取决于 GStreamer 安装的插件）
- **音频:** OPUS, G.711, Speex, AMR

### 3.3 网络协议
- **信令:** WebSocket (JSON-RPC) + REST (OpenVidu)
- **媒体传输:** SRTP/SRTCP over ICE/DTLS（WebRTC）、RTP/RTCP、HTTP、RTSP

### 3.4 OpenVidu v2 附加能力
- Session/Connection/Token 安全管理模型
- REST API 全栈控制
- Dashboard 监控
- Docker Compose 一键部署（含 Nginx + Coturn + Redis + KMS）
- 录制上传 S3/Azure
- 弹性集群 (Pro)
- 高可用部署 (Enterprise)

---

## 4. 部署与运维

### 4.1 Kurento 独立部署

```bash
# Ubuntu 安装
sudo apt-get install kurento-media-server
sudo service kurento-media-server start
# WebSocket 监听 ws://localhost:8888/kurento
```

- **端口需求:** 8888 (WebSocket), 大量 UDP/TCP 端口用于媒体传输
- **日志:** GStreamer debug 级别通过 `GST_DEBUG` 环境变量控制
- **模块管理:** 安装 `.so` 到 KMS modules 目录，通过 `--version`/`--list` 验证

### 4.2 OpenVidu 部署

```
┌────────────────────────────────────────────────────┐
│  OpenVidu 部署套件 (Docker Compose)                 │
│                                                    │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐        │
│  │  Nginx   │  │ openvidu │  │   KMS    │        │
│  │ (HTTPS)  │  │ -server  │  │ (Kurento)│        │
│  └──────────┘  └──────────┘  └──────────┘        │
│  ┌──────────┐  ┌──────────┐                        │
│  │  Coturn  │  │  Redis   │                        │
│  │(STUN/TURN)│  │          │                        │
│  └──────────┘  └──────────┘                        │
└────────────────────────────────────────────────────┘
```

- **OpenVidu CE:** 免费单节点
- **OpenVidu Pro:** 弹性集群 + 高级监控 (Elastic Stack)
- **OpenVidu Enterprise:** 高可用 + mediasoup 选项
- **端口:** 443 TCP (HTTPS), 3478 TCP+UDP (TURN), 40000-65535 TCP+UDP (媒体)

### 4.3 OpenVidu v3 架构变更

```
OpenVidu v2                           OpenVidu v3
─────────────                         ─────────────
Kurento (SFU engine)         →        LiveKit + mediasoup (SFU engine)
Elastic Stack (监控)          →        Prometheus + Loki + Grafana
无 E2EE                      →        E2EE (Insertable Streams)
Kurento 过滤器               →        解耦过滤器架构
手动 HA 配置                  →        内置 HA
```

**v3 关键变化:**
- LiveKit 作为主要 WebRTC 协议栈
- mediasoup 作为高性能 SFU 引擎替代 Kurento
- v2 兼容模块保证旧应用可平滑迁移（仅需更新依赖）
- 社区版获得大量原 Pro 功能（S3 录制、广播、虚拟背景、仪表盘）
- 性能提升: mediasoup 相比 Kurento 在同等硬件上可达 **2x 容量**

### 4.4 Kurento vs mediasoup 对比

| 维度 | Kurento | mediasoup |
|------|---------|-----------|
| **媒体处理** | 低级处理（解码→处理→编码） | 无低级处理（仅路由转发） |
| **性能** | CPU 密集，吞吐量较低 | 轻量级，高吞吐量 |
| **功能** | 转码、滤镜、CV、录制 | 基本路由 + simulcast + SVC |
| **适用场景** | 需要媒体处理的复杂应用 | 标准视频会议 SFU |
| **E2EE** | 不支持（需解密处理） | 支持（Insertable Streams） |
| **视频编解码** | VP8/H.264 通吃 | VP8/VP9/H.264 simulcast |

---

## 5. 生态与市场

### 5.1 开源生态

| 组件 | 仓库 | 活跃度 |
|------|------|--------|
| Kurento Media Server | Kurento/kurento | ⚠️ 维护模式（v7.3-dev，社区主要关注已转移到 OpenVidu） |
| OpenVidu Platform | OpenVidu/openvidu | 🟢 极活跃（v3.x 持续迭代，支持 LiveKit） |
| OpenVidu Call | OpenVidu/openvidu-call | 🟢 生产就绪的视频会议参考实现 |
| OpenVidu Meet | OpenVidu/openvidu-meet | 🟢 v3 旗舰产品 |
| kms-filters | Kurento/kms-filters | ⚠️ 维护模式 |
| openvidu-loadtest | OpenVidu/openvidu-loadtest | 🟢 压力测试框架 |

### 5.2 客户端 SDK 支持

| 语言 | Kurento | OpenVidu v2 | OpenVidu v3 |
|------|---------|-------------|-------------|
| JavaScript/TypeScript | ✅ | ✅ openvidu-browser.js | ✅ LiveKit JS |
| Java | ✅ (服务器端) | ✅ openvidu-java-client | ✅ |
| Node.js | ✅ (服务器端) | ✅ openvidu-node-client | ✅ |
| Android | — | ✅ | ✅ LiveKit Android |
| iOS | — | ✅ | ✅ LiveKit iOS |
| React | — | ✅ Components | ✅ React Components |
| Angular | — | ✅ Components | ✅ |
| Flutter | — | — | ✅ LiveKit Flutter |
| Unity | — | — | ✅ |
| Python/Ruby/PHP/Go/.NET | — | ✅ (社区客户端) | ✅ |

### 5.3 学术与产业影响力
- **19 篇**同行评审论文，**500+** 引用
- URJC (Universidad Rey Juan Carlos) 主导研发，FIWARE 生态核心组件
- 广泛应用于在线教育、远程医疗、视频监控、AR/VR 领域
- OpenVidu 被全球数千开发者和组织采用

### 5.4 商业模式
- **OpenVidu Community (CE):** 免费开源 (Apache 2.0)，单节点部署
- **OpenVidu PRO:** 付费，弹性集群 + 高可用 + 商业支持
- **OpenVidu Enterprise:** 付费，最高级别安全 + mediasoup 引擎 + 专属支持

---

## 6. 亮点与局限

### 6.1 亮点

1. **流水线架构极简优雅**
   - `connect()` 一个方法表达所有拓扑关系
   - MediaElement 黑盒抽象屏蔽 GStreamer 复杂性的 90%
   - 运行时动态修改管线，无需重建

2. **GStreamer 集成深度**
   - GStreamerFilter 提供任意 GStreamer element 注入的能力
   - 等同于将整个 GStreamer 生态系统（数百个插件）引入 WebRTC
   - 自动适配层消除格式不兼容

3. **插件系统完整**
   - 脚手架工具一键生成 → kmd.json 声明式接口 → 代码生成 → 编译安装 → 自动发现
   - 客户端 SDK 自动生成，无需手动维护

4. **信令/媒体严格分离**
   - 应用服务器无状态，可按需水平扩展
   - KMS 可独立部署、监控、升级

5. **MCU + SFU 双模式**
   - 同一架构下灵活选择（Composite Hub vs Dispatcher Hub）
   - 满足录制/混流/转发不同场景

### 6.2 局限

1. **性能上限**
   - 低级媒体处理（解码→处理→编码）消耗大量 CPU
   - 对于纯 SFU 场景，mediasoup 等轻量方案效率高得多（官方确认 2x+ 差距）
   - 这是 OpenVidu v3 选择迁移的根本原因

2. **部署权重**
   - GStreamer + OpenCV 依赖链令镜像体积巨大
   - 大量 UDP 端口需求增加网络配置复杂度
   - 仅原生支持 Linux (Ubuntu)，无官方 Windows/macOS 支持

3. **插件开发门槛**
   - GStreamer 模块需要 C/C++ + GStreamer 开发经验
   - ABI 兼容性约束（必须同系统版本编译）
   - 模块调试困难（日志、内存泄漏、线程安全）

4. **架构老化**
   - C++ + Boost 依赖，编译链繁重
   - 单进程模型限制 CPU 核心利用率
   - 无原生 simulcast/SVC 支持（需额外实现）

5. **生态萎缩**
   - Kurento 本身处于维护模式（最后一次 release 在 2023 年）
   - 官方焦点已全面转移到 OpenVidu v3（LiveKit + mediasoup）
   - 社区贡献和第三方模块减少

---

## 7. 对 AUDEMSP 的参考价值

### 7.1 PipelineEngine 架构对照

AUDEMSP 目前规划中有 `PipelineEngine` 作为微内核核心组件。Kurento 的 MediaPipeline 是其最直接、最成熟的参考实现：

| 概念 | Kurento | AUDEMSP (规划对照) |
|------|---------|-------------------|
| **流水线** | MediaPipeline (GStreamer pipeline 的包装) | PipelineEngine (计划中) |
| **处理单元** | MediaElement (四大类型: Input/Filter/Hub/Output) | 可复用此分类法，为 Rust trait 建模提供参考 |
| **连接语义** | `source.connect(sink)` → GStreamer pad link | 可借鉴单向数据流 + 自动格式适配 |
| **插件注册** | .kmd.json → 代码生成 → 工厂模式 | 可参考声明式接口定义 + 编译期注册 |
| **运行时控制** | JSON-RPC over WebSocket | AUDEMSP 可用 gRPC 或自研协议 |
| **自动适配** | Agnostic Media Adapter (自动转码) | 关键设计——减少用户手动处理格式转换 |
| **Hub 抽象** | Composite / Dispatcher / DispatcherOneToMany | 对多路媒体管理的优雅抽象，适合纳入 AUDEMSP |

### 7.2 值得借鉴的设计

1. **两层抽象：Element → Pipeline**
   - Element 定义"做什么"，Pipeline 定义"怎么做"
   - `connect()` 单一语义表达所有拓扑——极其简洁
   - AUDEMSP 的 PipelineEngine trait 可设计为 `fn connect(&mut self, output: ElementId, input: ElementId, sink_pad: PadId)`

2. **Hub 模式处理多路流**
   - Composite/Dispatcher 抽象将 N:M 路由降维为 hub.port 概念
   - 避免了在 Pipeline 层面维护复杂的 N×M 连接矩阵
   - AUDEMSP 的远程桌面场景（多窗口→合成→编码）天然适合此模式

3. **声明式插件接口 (kmd.json)**
   - 方法/属性/事件的完整元数据驱动
   - 自动生成多语言客户端绑定（Java/JS/TS）
   - AUDEMSP 的 napi-binding 和 gRPC 服务可参考此模式

4. **GStreamerFilter 的"逃生舱"模式**
   - 提供通用扩展点（注入 GStreamer pipeline），但不暴露内部复杂性
   - AUDEMSP 可为 PluginManager 设计类似的 RawFilter trait

### 7.3 应避免的设计

1. **单进程媒体处理**
   - Kurento 的 GStreamer pipeline 在单一进程内——CPU 密集时成为瓶颈
   - AUDEMSP 的 PipelineEngine 应从设计阶段考虑多线程/多进程调度

2. **ABI 脆弱性**
   - Kurento 插件的编译器版本绑定导致部署困难
   - AUDEMSP (Rust) 天然避免此问题，但需注意 C FFI 边界的稳定性

3. **过度依赖底层框架**
   - Kurento 强绑定 GStreamer→迁移成本极高
   - AUDEMSP 的 PipelineEngine 应保持后端无关（trait 抽象），支持多种后端（GStreamer/FFmpeg/自定义 Rust 实现）

4. **"全功能服务器"哲学**
   - Kurento 试图覆盖所有媒体处理场景→导致代码库庞大、维护负担重
   - AUDEMSP 应聚焦 AUDE 生态的核心需求（远程桌面、视频会议、直播推拉流、监控相机），其余通过插件扩展

### 7.4 Rust 实现的优势

| 维度 | Kurento (C++/GStreamer) | AUDEMSP 可获得的 Rust 优势 |
|------|------------------------|--------------------------|
| **内存安全** | GStreamer 引用计数 + 手动管理 | 所有权系统 + 生命周期编译期检查 |
| **并发** | GStreamer 内部线程池 + 全局锁 | async/await + tokio + 无数据竞争 |
| **跨平台** | Linux only (核心) | 跨平台（Windows/Linux/macOS） |
| **ABI** | C++ ABI 不稳定 | C FFI 稳定 + trait 系统无 ABI 问题 |
| **插件** | .so 动态加载 + kmd 代码生成 | proc macro 编译期注册 + cdylib 动态加载 |
| **测试** | 手动测试为主 | cargo test + proptest + miri |

### 7.5 总结

Kurento 的 MediaPipeline 模型是视频处理流水线的**最佳参考实现**——它的 Element 分类法、Hub 抽象、connect() 语义、声明式插件系统都经历了十余年的生产验证。AUDEMSP 的 PipelineEngine 应在吸收这些设计精华的基础上，利用 Rust 的类型系统和并发模型实现更安全、更高性能的版本，同时避免 Kurento 在单进程瓶颈和底层框架强绑定方面的历史包袱。

**核心借鉴:**
- `MediaPipeline::connect()` 单一语义 → PipelineEngine 核心 trait
- `Hub (Composite/Dispatcher)` → 多路流管理抽象
- `kmd.json` → 声明式插件元数据
- `GStreamerFilter` escape hatch → PluginManager 的 RawPlugin trait
- Agnostic Media Adapter → 自动格式协商/适配层

**核心规避:**
- 单进程瓶颈 → 多线程/多进程调度
- GStreamer 强绑定 → 后端无关 trait
- 全功能膨胀 → 聚焦 AUDE 生态核心场景
**相关决策**: D97 (SFU/MCU混合), D144-D145 (多后端trait)
