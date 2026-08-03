# Janus Gateway — 参考分析

> 研究日期: 2026-07-19 | 版本: v1.4.0 | 仓库: [meetecho/janus-gateway](https://github.com/meetecho/janus-gateway)

## 1. 产品画像

| 属性 | 值 |
|------|-----|
| **名称** | Janus WebRTC Server (Janus Gateway) |
| **开发者** | [Meetecho](https://www.meetecho.com/) s.r.l. |
| **首次发布** | 2014 |
| **当前版本** | v1.4.0 (2026-02-06) |
| **许可证** | GPL v3.0 (server), MIT (janus.js) |
| **GitHub Stars** | ~9,100 |
| **语言** | C (82.8%), JavaScript (11.7%) |
| **定位** | 通用 WebRTC 服务器 — 轻量、模块化、不限信令协议 |
| **目标用户** | 需要自托管、可扩展 WebRTC 媒体服务器的开发者 |

## 2. 架构特征

### 2.1 核心+插件架构

Janus 的设计哲学：**核心极薄，所有业务逻辑在插件中**。

```
┌─────────────────────────────────────────────────────────┐
│                    Janus Core                            │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────────┐   │
│  │ Session  │  │  Handle  │  │ ICE/DTLS/SRTP/SCTP   │   │
│  │ Manager  │  │ Manager  │  │ (libnice/OpenSSL)    │   │
│  └──────────┘  └──────────┘  └──────────────────────┘   │
│  ┌──────────────────────────────────────────────────┐   │
│  │  Transport Layer (HTTP/WS/RabbitMQ/MQTT/Nanomsg) │   │
│  └──────────────────────────────────────────────────┘   │
│  ┌──────────────────────────────────────────────────┐   │
│  │  Plugin Loader (dlopen .so)                      │   │
│  └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
         │                    │                    │
    ┌────┴────┐         ┌────┴────┐         ┌────┴────┐
    │ Echotest│         │VideoRoom│         │Streaming│  ...
    │  .so    │         │  .so    │         │  .so    │
    └─────────┘         └─────────┘         └─────────┘
```

**核心职责**：仅处理 WebRTC 协议栈（ICE/DTLS/SRTP）+ 插件加载 + 消息路由。

**插件不接触**：ICE 协商、DTLS 握手、SRTP 加解密 —— 核心对插件透明。

### 2.2 Session/Handle 模型

```
Browser → 1. Create Session (session_id)
        → 2. Attach to Plugin (handle_id)
              ↳ handle ↔ janus_plugin_session (opaque)
        → 3. Messages (handle_message)
              ↳ JSEP offer/answer (optional, for WebRTC)
        → 4. Media Flow (RTP/RTCP via handle's ICE)
        → 5. Detach/Destroy
```

一个 session 可 attach 多个 handle 到不同插件，实现一个用户同时使用多个功能。

### 2.3 线程模型

- **每个 handle 一个 GMainLoop 线程**（PR #1399, 2018）：替代了旧的 2-threads-per-RTCPeerConnection
- **插件辅助线程**（可选）：VideoRoom 和 Streaming 插件支持解耦收包和分发
- **`event_loops` 池**：可配置的线程池，复用而非每 handle 新建

### 2.4 传输层

支持 6 种传输协议，全部由可加载的 transport 插件实现：HTTP/HTTPS (libmicrohttpd)、WebSocket (libwebsockets)、RabbitMQ、MQTT、Nanomsg、Unix Sockets。

### 2.5 插件 API

```c
// 插件必须实现的回调
struct janus_plugin {
    int (*init)(const char *config_path);           // 启动时
    void (*destroy)(void);                           // 关闭时
    void (*create_session)(janus_plugin_session *, int *error);
    void (*destroy_session)(janus_plugin_session *, int *error);
    struct janus_plugin_result *(*handle_message)(   // 消息处理
        janus_plugin_session *, char *, json_t *, json_t *);
    // 可选：媒体回调
    void (*incoming_rtp)(...);   // 接收 RTP
    void (*incoming_rtcp)(...);
    void (*setup_media)(...);    // PC 就绪
    void (*hangup_media)(...);
};

// 核心暴露给插件的接口
struct janus_callbacks {
    int (*push_event)(...);     // 推送事件给客户端
    void (*relay_rtp)(...);     // 转发 RTP
    void (*relay_rtcp)(...);
    void (*close_pc)(...);
    void (*end_session)(...);
};
```

加载方式：`dlopen()` + `dlsym("create_p")` 获取插件实例。

## 3. 关键能力

| 能力 | 说明 |
|------|------|
| **SFU** | VideoRoom 插件：1 个发布者 → N 个订阅者 |
| **MCU** | AudioBridge 插件：Opus 音频混音 |
| **编解码** | VP8/VP9/H.264/H.265/AV1, Opus/G.711/G.722 |
| **Simulcast** | VP8/VP9/H.264 多空间/时间层 |
| **Multistream** | v1.0.0+ 完整 Unified Plan 支持 |
| **录制** | 内置 .mjr 格式 + janus-pp-rec 后处理工具 |
| **级联** | v1.4.0 Remote Publishers：跨实例 RTP 转发 |
| **扩展点** | 12 个官方插件 + 自定义 .so 开发 |

## 4. 插件列表（v1.4.0）

| 插件 | 用途 |
|------|------|
| Echo Test | 回环测试 |
| Video Room | SFU 多人视频（publish/subscribe） |
| Video Call | 1-1 视频通话 |
| Streaming | 广播 RTSP/RTMP 流到 WebRTC |
| Record & Play | 录制 WebRTC 流 + 回放 |
| Text Room | RTCDataChannel 聊天 |
| Audio Bridge | MCU 音频混音 |
| SIP Gateway | WebRTC↔SIP 互通 |
| Lua/Duktape | Lua/JS 脚本编写插件 |

## 5. 生态与市场

- **生产案例**：IETF 会议直播、企业级视频会议、流媒体分发
- **社区**：Discourse 论坛活跃，300+ 贡献者
- **商业支持**：Meetecho 提供咨询服务
- **竞争**：LiveKit（更现代但更重）、mediasoup（SFU 专精）、Jitsi（完整方案）

## 6. 亮点与局限

### 亮点

- **极端灵活**：插件架构对应用逻辑零约束
- **不限信令**：不带信令服务器，你用自己的
- **多传输**：6 种传输协议，全异步 JSON
- **轻量**：纯 C，依赖少，Raspberry Pi 可运行
- **成熟**：自 2014 年投入生产使用

### 局限

- **C 代码**：116K 行 C，内存安全风险（use-after-free、缓冲区溢出）
- **无内置集群**：一个房间只能在一个实例上，集群需外部构建
- **插件 ABI 脆弱**：版本号升一次，所有 .so 需重编译
- **无内置鉴权**：需外部系统叠加
- **Windows 不支持**：仅 Linux/macOS
- **RTP 扩展被核心剥离**：导致部分扩展失效

## 7. 对 AUDEMSP 的参考价值

### 可采纳

| 概念 | Janus 做法 | AUDEMSP 应用 |
|------|-----------|--------------|
| **薄核心+插件** | 核心只处理 WebRTC 协议，全部业务在插件 | audemsp-core 微内核 + 插件体系 |
| **Session/Handle 隔离** | opaque `janus_plugin_session` + 引用计数 | `Arc<SessionHandle>` + trait object |
| **异步消息模式** | `handle_message` → 返回结果 → `push_event` | `handle_message()` → `tokio::mpsc::Sender` |
| **传输层抽象** | transport 也是可加载插件 | `TransportProvider` trait |
| **不限信令** | 不强加信令协议 | 自研 WebSocket 信令 + 可替换 |

### 需改造

| Janus 做法 | AUDEMSP 适配 |
|-----------|--------------|
| `dlopen .so` 运行时加载 | **编译期 trait** — `Vec<Box<dyn PluginTrait>>`，类型安全 |
| C 函数指针表 | **Rust trait** — 编译时检查，默认实现可选方法 |
| 单线程 GMainLoop per handle | **tokio async** — 每个 handle 一个 task，工作窃取调度 |
| `json_t` 消息 | **serde + 强类型 Message enum** |
| 手动 refcounting | **Arc + 所有权系统** — 消除 use-after-free |

### 需避免

- **单线程阻塞** → 用 tokio async 避免
- **C ABI 脆弱** → 用 Rust trait + semver
- **无内置集群** → 从 Phase 2 开始设计集群意识
- **仅内存会话** → 可选持久化层（Redis/SQLite）
- **janus.js 是演示库** → 提供一等 client SDK
**相关决策**: D97 (插件架构), D144-D145 (多后端trait)
