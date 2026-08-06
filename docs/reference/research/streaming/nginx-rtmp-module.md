# nginx-rtmp-module 参考研究

> **源码地址**: [arut/nginx-rtmp-module](https://github.com/arut/nginx-rtmp-module)  
> **最后更新**: 2024-12-24 | **Stars**: 13k+ | **许可证**: BSD-2-Clause  
> **作者**: Roman Arutyunyan (arut)  
> **创建时间**: 2012-03-14 | **主线状态**: 维护模式（非活跃开发）

---

## 1. 产品画像

nginx-rtmp-module 是基于 nginx 的流媒体服务器扩展模块，将 nginx 从一个 HTTP 服务器转变为功能完备的 RTMP/HLS/MPEG-DASH 流媒体服务器。它利用 nginx 成熟的 C 模块扩展机制，在 nginx 的事件驱动架构之上实现了完整的 RTMP 协议栈、直播推拉流、转封装（HLS/DASH）、录制、转码、HTTP 回调等功能。

**核心定位**：轻量级、高性能的流媒体中间件，适用于直播分发、视频会议、监控流接入等场景。它是 nginx 模块开发模式的教科书级案例，完整展示了如何在 nginx 上构建一个全新的应用层协议处理框架。

**模块组成**：
- `ngx_rtmp_module` — 核心模块（NGX_CORE_MODULE），管理 RTMP 连接、握手、Chunk 协议、AMF 编解码
- `ngx_rtmp_live_module` — 直播模块，管理发布者/订阅者、GOP 缓存、多订阅者广播
- `ngx_rtmp_relay_module` — 中继模块，实现 Push/Pull 推拉流、断线重连、静态拉流
- `ngx_rtmp_record_module` — 录制模块，支持 FLV 录制、关键帧录制、定时分割
- `ngx_rtmp_hls_module` — HLS 转封装模块，将 RTMP 流转为 HTTP Live Streaming
- `ngx_rtmp_dash_module` — DASH 转封装模块，将 RTMP 流转为 MPEG-DASH
- `ngx_rtmp_exec_module` — 外部程序执行模块，在流事件时触发外部命令（如 ffmpeg 转码）
- `ngx_rtmp_notify_module` — HTTP 回调通知模块（on_publish/on_play/on_done/on_update）
- `ngx_rtmp_control_module` — HTTP 控制模块，提供录制控制、客户端踢出等管理接口
- `ngx_rtmp_stat_module` — 统计模块，提供 XML 格式的 RTMP 状态信息
- `ngx_rtmp_codec_module` — 编解码模块，解析 H264/AAC/MP3 等编码信息

---

## 2. 技术特性

### 2.1 nginx 模块架构（事件循环、模块钩子、配置指令）

nginx-rtmp-module 是理解 nginx 模块扩展模式的最佳参考。它在 HTTP 协议之外定义了一套完整的 **程序化模块框架**，包括自定义的模块类型、配置层级、事件系统和生命周期。

#### 2.1.1 模块定义与类型注册

nginx 的所有扩展都通过 `ngx_module_t` 结构体注册。nginx-rtmp-module 定义了自己的模块类型 `NGX_RTMP_MODULE`，与 `NGX_HTTP_MODULE`、`NGX_EVENT_MODULE` 等并列：

```c
// 核心模块 — 类型为 NGX_CORE_MODULE，负责初始化 RTMP 子系统
ngx_module_t ngx_rtmp_module = {
    NGX_MODULE_V1,
    &ngx_rtmp_module_ctx,          // 模块上下文
    ngx_rtmp_commands,             // 配置指令（rtmp { }, server { }, listen 等）
    NGX_CORE_MODULE,               // 核心模块类型
    NULL,                           // init master
    NULL,                           // init module
    ngx_rtmp_init_process,         // init process — 在 worker 进程启动时调用
    NULL, NULL, NULL, NULL,
    NGX_MODULE_V1_PADDING
};

// 功能模块 — 类型为 NGX_RTMP_MODULE，实现具体功能
ngx_module_t ngx_rtmp_relay_module = {
    NGX_MODULE_V1,
    &ngx_rtmp_relay_module_ctx,    // RTMP 模块上下文
    ngx_rtmp_relay_commands,       // 模块指令（push, pull, relay_buffer 等）
    NGX_RTMP_MODULE,               // RTMP 模块类型
    NULL, NULL,
    ngx_rtmp_relay_init_process,   // init process
    NULL, NULL, NULL, NULL,
    NGX_MODULE_V1_PADDING
};
```

#### 2.1.2 三层配置体系

nginx-rtmp-module 仿照 HTTP 模块定义了 **Main → Server → Application** 三层配置继承体系：

| 层级 | 对应 nginx HTTP | 配置块 | 典型指令 |
|------|----------------|--------|---------|
| `main_conf` | `http` 块 | `rtmp { }` | `rtmp_auto_push` |
| `srv_conf` | `server` 块 | `server { }` | `listen 1935`, `chunk_size` |
| `app_conf` | `location` 块 | `application { }` | `live on`, `push`, `pull` |

配置的创建与合并遵循标准的 nginx 模式：

```c
// RTMP 模块上下文 — 定义配置创建/合并的钩子函数
static ngx_rtmp_module_t ngx_rtmp_relay_module_ctx = {
    NULL,                                    // preconfiguration
    ngx_rtmp_relay_postconfiguration,        // postconfiguration
    NULL,                                    // create main configuration
    NULL,                                    // init main configuration
    NULL,                                    // create server configuration
    NULL,                                    // merge server configuration
    ngx_rtmp_relay_create_app_conf,          // create app configuration
    ngx_rtmp_relay_merge_app_conf            // merge app configuration
};
```

在 `postconfiguration` 阶段，模块通过**事件钩子注册**将其功能注入到 RTMP 核心的事件处理链中：

```c
static ngx_int_t
ngx_rtmp_relay_postconfiguration(ngx_conf_t *cf)
{
    ngx_rtmp_core_main_conf_t *cmcf;
    ngx_rtmp_handler_pt *h;

    cmcf = ngx_rtmp_conf_get_module_main_conf(cf, ngx_rtmp_core_module);

    // 注册握手完成事件处理器
    h = ngx_array_push(&cmcf->events[NGX_RTMP_HANDSHAKE_DONE]);
    *h = ngx_rtmp_relay_handshake_done;

    // 拦截发布/播放/关闭流的处理函数（链式钩子）
    next_publish = ngx_rtmp_publish;
    ngx_rtmp_publish = ngx_rtmp_relay_publish;

    next_play = ngx_rtmp_play;
    ngx_rtmp_play = ngx_rtmp_relay_play;

    return NGX_OK;
}
```

这种**函数指针链式替换**模式是 nginx 模块协作的核心机制——多个模块可以独立注册对同一事件的处理，互不感知。

#### 2.1.3 nginx 事件循环集成

nginx 的事件循环核心函数是 `ngx_process_events_and_timers()`，其执行流程为：

1. 调用 `ngx_event_find_timer()` 找到最近要超时的定时器
2. 调用事件模块处理 I/O 事件（如 `epoll_wait`），等待超时时间
3. 读取/写入就绪的连接
4. 调用 `ngx_event_expire_timers()` 处理超时事件
5. 调用 `ngx_event_process_posted()` 处理 posted 事件队列

nginx-rtmp-module 完全插入这个事件循环中。新连接到达时：

```c
// 在 ngx_rtmp_optimize_servers 中注册连接处理器
ls->handler = ngx_rtmp_init_connection;  // 新连接回调
ls->pool_size = 4096;

// 建立连接后启动 RTMP 会话循环
void ngx_rtmp_cycle(ngx_rtmp_session_t *s)
{
    ngx_connection_t *c = s->connection;

    // 将 RTMP 读写函数注册为 nginx connection 的事件处理器
    c->read->handler = ngx_rtmp_recv;
    c->write->handler = ngx_rtmp_send;

    // 设置心跳定时器
    s->ping_evt.handler = ngx_rtmp_ping;
    ngx_rtmp_reset_ping(s);

    // 启动读取
    ngx_rtmp_recv(c->read);
}
```

RTMP 协议解析发生在 `ngx_rtmp_recv()` 中，该函数在 read 事件触发时被执行，解析 RTMP Chunk Header（fmt、csid、timestamp、message length、message type、message stream ID），然后通过 `ngx_rtmp_fire_event()` 将完整消息分发给注册的事件处理器：

```c
ngx_int_t
ngx_rtmp_fire_event(ngx_rtmp_session_t *s, ngx_uint_t evt,
    ngx_rtmp_header_t *h, ngx_chain_t *in)
{
    ngx_rtmp_core_main_conf_t *cmcf;
    ngx_array_t *ch;
    ngx_rtmp_handler_pt *hh;
    size_t n;

    cmcf = ngx_rtmp_get_module_main_conf(s, ngx_rtmp_core_module);
    ch = &cmcf->events[evt];
    hh = ch->elts;

    // 遍历该事件类型的所有注册处理器
    for (n = 0; n < ch->nelts; ++n, ++hh) {
        if (*hh && (*hh)(s, h, in) != NGX_OK) {
            return NGX_ERROR;
        }
    }

    return NGX_OK;
}
```

#### 2.1.4 关键事件类型

nginx-rtmp-module 定义了丰富的 RTMP 事件类型，模块通过向 `cmcf->events[]` 数组注册处理器来接收相应事件：

```c
// 协议级事件 — 自动注册标准处理器
NGX_RTMP_MSG_CHUNK_SIZE   // 设置 chunk 大小
NGX_RTMP_MSG_ABORT        // 中止消息
NGX_RTMP_MSG_ACK          // 确认收到
NGX_RTMP_MSG_ACK_SIZE     // 设置确认窗口大小
NGX_RTMP_MSG_BANDWIDTH    // 带宽限制

// 应用层事件 — 由各功能模块注册
NGX_RTMP_CONNECT           // 新连接
NGX_RTMP_HANDSHAKE_DONE    // 握手完成
NGX_RTMP_MSG_AMF_CMD       // AMF 命令（connect, createStream, publish, play 等）
NGX_RTMP_MSG_AMF_META      // 元数据
NGX_RTMP_MSG_AUDIO         // 音频数据
NGX_RTMP_MSG_VIDEO         // 视频数据
NGX_RTMP_DISCONNECT        // 断开连接
NGX_RTMP_STREAM_BEGIN      // 流开始
NGX_RTMP_STREAM_EOF        // 流结束
```

AMF 命令进一步通过哈希表路由到具体处理器（`connect` → `ngx_rtmp_cmd_connect`，`publish` → `ngx_rtmp_cmd_publish`，`play` → `ngx_rtmp_cmd_play` 等）。

### 2.2 RTMP 协议处理

nginx-rtmp-module 实现了完整的 RTMP 协议栈：

- **握手阶段**：C0/C1/C2 ↔ S0/S1/S2 四步握手，在 `ngx_rtmp_handshake()` 及相关函数中实现
- **Chunk 协议**：实现可变长度 Chunk Header（1/3/11/15 字节），支持多 Chunk Stream 复用。核心在 `ngx_rtmp_recv()` 中逐字节解析 fmt、csid，识别 Chunk Stream ID（0-65599）
- **消息类型分发**：协议控制消息（Set Chunk Size, Abort, ACK, Window ACK Size, Set Peer Bandwidth）由 `ngx_rtmp_protocol_message_handler` 处理；用户控制消息（Stream Begin, Stream EOF, Set Buffer Length 等）由 `ngx_rtmp_user_message_handler` 处理
- **AMF 编解码**：支持 AMF0 和 AMF3 编码，通过 `ngx_rtmp_amf_read()`/`ngx_rtmp_amf_write()` 进行序列化/反序列化。AMF 命令名通过哈希查找表路由到处理器
- **Session 管理**：每个连接对应一个 `ngx_rtmp_session_t`，包含输入/输出 Chunk Stream 数组（最多 `max_streams` 个）、输出消息环形队列（`out_queue` 大小可配）、Nagle-like 合并机制（`out_cork`）

### 2.3 多 Worker 直播分发

`rtmp_auto_push` 指令开启后，模块通过 **Unix Domain Socket** 在工作进程间自动转发流。当一个 worker 收到 publish，它会将流推送到其他 workers，使得订阅者可以连接到任意 worker 并获得相同的流。这是利用 nginx 多进程架构实现无状态水平扩展的优雅方案。

### 2.4 中继（Relay）模式

Push/Pull 模型基于 RTMP 客户端实现，完全复用 RTMP 协议栈：

- **Push**：在 publish 事件时自动连接到配置的远端 RTMP 服务器并推送流。支持断线重连（`push_reconnect` 配置间隔）
- **Pull**：在 play 事件时自动从远端 RTMP 服务器拉流。支持静态拉流（`static` 参数，在 nginx 启动时即开始拉流）
- **Session Relay**：中继操作在独立会话中执行，可跨不同 Application

### 2.5 HLS/DASH 转封装

HLS 和 DASH 模块从 RTMP 流的音频/视频数据中提取编码信息，按关键帧切分生成 `.m3u8`/`.mpd` 播放列表和 `.ts`/`.m4s` 分片文件。要求在内存文件系统（tmpfs）上运行以减少磁盘 I/O 开销。HTTP 部分通过标准的 nginx `http { }` 块来配置静态文件服务。

---

## 3. 关键能力

| 能力 | 描述 |
|------|------|
| **RTMP 直播** | 完整 RTMP 协议栈，支持推流/拉流、单推多拉（TV 模式）、多推多拉（视频会议模式） |
| **HLS 输出** | 实时 HLS 分段（m3u8 + ts），直接兼容 iOS/Safari 原生播放 |
| **MPEG-DASH 输出** | 实时 DASH 分段（mpd + m4s），兼容现代浏览器和播放器 |
| **中继推拉流** | Push/Pull 模型，支持同构 nginx-rtmp 集群跨节点分发，断线重连 |
| **多 Worker 分发** | 通过 Unix Socket 自动在 workers 间转发流，实现无状态水平扩展 |
| **VOD 点播** | 支持 FLV/MP4 文件作为 RTMP 流播放 |
| **流录制** | 录制为 FLV 文件，支持关键帧模式、定时分割、按时间/大小限制 |
| **在线转码** | 通过 exec 指令调用 ffmpeg，在 publish 事件时动态转码/缩放/转封装 |
| **HTTP 回调** | on_publish / on_play / on_done / on_update / on_record_done 等回调，支持鉴权和业务逻辑 |
| **HTTP 控制** | 通过 HTTP 接口控制录制（record/stop）和断开客户端 |
| **RTMP 统计** | XML 格式的实时统计信息（连接数、流数、码率等） |
| **访问控制** | 基于 IP 的 allow/deny 规则（publish/play 分别控制） |
| **缓冲控制** | 可配置的 GOP 缓存、播放缓冲大小、超时时间 |
| **日志级别** | 独立的 `NGX_LOG_DEBUG_RTMP` 日志级别，便于调试 |

**配置示例**：

```nginx
rtmp {
    server {
        listen 1935;
        chunk_size 4096;
        timeout 60s;

        application live {
            live on;
            record all;
            record_path /tmp/av;
            record_unique on;

            allow publish 127.0.0.1;
            deny publish all;

            hls on;
            hls_path /tmp/hls;

            dash on;
            dash_path /tmp/dash;

            on_publish http://localhost:8080/auth;
            on_done http://localhost:8080/done;

            push rtmp://cdn1.example.com/live;
            pull rtmp://origin.example.com/live name=camera1 static;

            exec ffmpeg -i rtmp://localhost/$app/$name -vcodec flv
                -acodec copy -s 640x360 -f flv
                rtmp://localhost/small/$name;
        }
    }
}
```

---

## 4. 部署与运维

### 4.1 编译方式

nginx-rtmp-module 采用 nginx 的 `--add-module` 编译方式，不单独构建：

```bash
./configure --add-module=/path/to/nginx-rtmp-module
make && make install
```

这种编译方式将模块编译进 nginx 二进制文件中，无法动态加载。这也是 nginx 历史模块系统的限制——动态模块加载（`load_module`）在 nginx 1.9.11+ 才引入，且多数第三方 RTMP 模块尚未适配。

### 4.2 运行依赖

- **nginx 1.6.x ~ 1.24.x**（特定版本需验证兼容性）
- **HLS/DASH**：需要 tmpfs 或高性能存储（分片文件频繁写入）
- **转码（exec）**：需要安装 ffmpeg
- **录制**：需要足够的磁盘空间

### 4.3 性能特性

- 单进程 **10,000+ 并发连接**（受限于 `worker_connections` 配置）
- 延迟：典型 LAN 环境 < 1 秒，公网 < 3 秒
- HLS 延迟：3~30 秒（取决于分片长度配置）
- 内存占用：每连接约 64KB~256KB（取决于缓冲区和流数量）

### 4.4 常见运维问题

- **非活跃维护**：作者自 2018 年起不再积极开发，仓库中仍有大量未处理的 Issues 和 PR
- **HLS/DASH 兼容性**：部分播放器对新版 HLS 规范的兼容性问题
- **内存泄漏**：特定场景下的少量内存泄漏（如频繁断开重连）
- **静态拉流**：在 nginx 启动时即开始拉流，即使无人订阅

---

## 5. 生态与市场

### 5.1 主要用户

nginx-rtmp-module 曾是直播行业的事实标准组件，广泛用于：

- **直播平台**：Twitch、YouTube Live 架构参考
- **CDN 边缘节点**：作为 RTMP 接入层，转 HLS 后走 HTTP CDN 分发
- **监控系统**：通过 RTMP 接入 IP 摄像头流
- **视频会议**：作为 SFU/MCU 的底层流接入层
- **私有直播**：企业内网直播、教育直播

### 5.2 替代/演进方案

| 方案 | 特点 | 对比 |
|------|------|------|
| **SRS (Simple-RTMP-Server)** | C++ 实现，专门为流媒体设计 | 更活跃的社区，功能更丰富，但扩展性不如 nginx 模块体系 |
| **Janus (WebRTC Gateway)** | C 实现，WebRTC 优先 | 面向 WebRTC，不支持传统 RTMP 推流（需转换） |
| **MediaMTX (rtsp-simple-server)** | Go 实现，多协议支持 | 支持 RTSP/RTMP/HLS/WebRTC，更现代的架构 |
| **nginx + SRTP/WebRTC** | nginx 原生 WebRTC 支持（NGINX Plus） | 需要付费版本，功能有限 |
| **OBS Studio + SRS** | 推流端 + 服务器 | 组合方案，非单一产品 |

### 5.3 市场位置

nginx-rtmp-module 代表了**以通用反向代理为基础，通过模块扩展实现流媒体能力**的技术路线。这种 "one binary, many protocols" 的理念在运维层面有显著优势（统一部署、统一监控、统一日志），但受限于 nginx C 模块的开发复杂性和维护瓶颈。

---

## 6. 亮点与局限

### 亮点

1. **架构复用的典范**：充分利用 nginx 的事件驱动、内存池、连接管理、配置解析等成熟基础设施，以最小代码量实现了一个完整的流媒体协议栈
2. **事件钩子系统**：通过 `ngx_rtmp_fire_event()` + 事件处理器数组实现了松耦合的模块协作，新增功能只需注册事件处理器，无需修改核心代码
3. **多 Worker 无缝分发**：`rtmp_auto_push` 利用 Unix Socket 实现的无状态分发，是对 nginx 多进程架构的深刻理解和巧妙利用
4. **配置驱动的灵活性**：所有功能通过 `nginx.conf` 声明式配置驱动，不需要额外的编排工具
5. **RTMP 协议实现完整**：支持 AMF0/AMF3、Chunk 协议、多 Stream 复用、窗口 ACK、带宽控制等完整特性
6. **生产验证**：在大量生产环境中验证过的稳定性（在不触及已知 bug 的场景下）

### 局限

1. **维护停滞**：核心维护者不再活跃开发，未合并的 PR 和未修复的 Issues 积压
2. **无 WebRTC 支持**：诞生于 WebRTC 之前，无法直接支持现代实时通信协议
3. **单机架构**：无原生集群协调，中继靠配置硬编码，无服务发现或自动负载均衡
4. **HLS/DASH 延迟高**：分段式传输固有延迟，不适合超低延迟场景（< 1 秒）
5. **C 语言开发壁垒**：模块开发需要深入理解 nginx 内部 API，学习曲线陡峭
6. **编译耦合**：模块必须编译进 nginx 二进制，无法热加载或独立升级
7. **无 SRT/RIST 支持**：不支持专为不可靠网络设计的流媒体协议

---

## 7. 对 AUDEMSP 的参考价值

### 7.1 nginx 模块模式 vs Rust Trait 扩展

nginx-rtmp-module 展示了**事件驱动 + 模块钩子 + 配置继承**的架构模式。AUDEMSP 作为 Rust 项目，可以借鉴其设计理念，用 Rust 的 trait 系统实现等价的扩展机制：

| nginx C 模式 | Rust 等价方案 | 说明 |
|-------------|--------------|------|
| `ngx_module_t` + `NGX_MODULE_V1` | `trait Module` + 派生宏 | 用 trait 定义模块生命周期（init_process, exit_process 等） |
| `ngx_rtmp_module_t`（create/merge conf） | `trait ModuleConfig<T>` | 通过关联类型指定配置结构体，`Default` trait 提供合并语义 |
| `ngx_command_t` 数组 | 属性宏 `#[rtmp_directive]` | 用属性宏标记配置解析函数，编译期生成指令注册代码 |
| 事件队列 `cmcf->events[]` | `EventBus<T>` 或观察者模式 | 用 `Vec<Box<dyn FnMut(...)>>` 存储事件处理器，支持动态注册 |
| 链式函数指针替换（next_xxx） | decorator/wrapper 模式 | 利用 Rust 的闭包或 `Box<dyn Fn>` 包装原始处理器 |
| RTMP Session 状态机 | `enum` + `match` | Rust 的 ADT 可以更安全地表达协议状态，编译器确保所有状态分支被处理 |
| `ngx_pool_t` 内存池 | `bumpalo` / `typed_arena` crate | Rust 的 RAII + 生命周期可自动管理内存，不一定需要显式内存池 |
| 配置合并 `ngx_conf_merge_*` | `Option::or()` / `unwrap_or()` 链 | Rust 的 Option 类型天然适合配置覆盖语义 |

### 7.2 架构决策借鉴

1. **协议无关的应用框架**：nginx-rtmp 证明了 nginx 不仅是 HTTP 服务器，它的核心是一个**可配置的事件循环 + 模块加载器**。HTTP 和 RTMP 都是在此基础上构建的"应用"。AUDEMSP 可以类比为：基于 Rust trait 扩展的应用框架，RTMP、WebRTC、SRT 等协议作为 trait 实现者插入。

2. **配置驱动的路由**：RTMP 通过 `application` 名称路由流，等价于 HTTP 的 `location` 路由。这种模式非常适合流媒体——不同的 `app` 可以有完全不同的处理管线（录制、转码、推送到 CDN）。

3. **中继模型的普适性**：Push/Pull 模式是流媒体分发的基础原语。AUDEMSP 应将这些作为一等公民设计，而非仅在应用层实现。

4. **声明式管线组合**：exec 指令体现了"流事件 → 外部程序 → 流输出"的声明式管线。AUDEMSP 可以用 Rust 的类型安全 + 编译期检查实现类似但更可靠的管线组合。

### 7.3 技术演进方向

nginx-rtmp-module 的历史局限为 AUDEMSP 指明了方向：

- **从编译时扩展到运行时扩展**：支持动态加载协议模块（Rust 的 `libloading` 或 WASM 嵌入）
- **从单协议到多协议**：RTMP + WebRTC + SRT + RIST 统一在同一个框架中
- **从配置硬编码到服务发现**：中继端点应支持动态注册和健康检查
- **从 C 到 Rust**：内存安全、并发安全在高吞吐流媒体场景中至关重要

> **总结**：nginx-rtmp-module 是"如何在一个成熟基础设施上构建流媒体应用"的经典案例。AUDEMSP 应取其架构精髓（模块化、事件驱动、配置分层、管线组合），用 Rust 的现代语言特性重新实现，避免其维护性陷阱。
**相关决策**: D152 (录制hooks), D-STREAM-TOPOLOGY
