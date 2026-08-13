# OpenVidu 部署参考

> 生成日期：2026-07-31 | 分类：视频会议部署

## 1. 概述

OpenVidu 是一个开源的 WebRTC 视频会议平台，提供从信令到媒体层的完整解决方案。本文档聚焦其 Docker Compose 部署架构：12 个容器协同工作，覆盖反向代理、信令、媒体引擎（LiveKit 内嵌）、录制/推流、TURN（LiveKit 内置）、对象存储等全链路。

所有镜像均来自 Docker Hub（`docker.io/openvidu/...`）。

### 1.1 版本与许可

| 维度 | 说明 |
|------|------|
| 社区版 | 基于 LiveKit（Pion Go SFU）的免费部署。Apache 2.0 许可 |
| 专业版 | 增加 mediasoup C++ 引擎切换、v2 兼容层、优先支持。商业许可 |
| 推荐部署 | Docker Compose（单机）或 Kubernetes（集群） |
| 版本 | 参考 v2.29+，2026 年 7 月稳定版 |

### 1.2 容器总览

```
┌───────────────────────────────────────────────────────────────────┐
│                      OpenVidu Docker Compose                        │
│                                                                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐      │
│  │  Caddy   │  │ OpenVidu │  │  MongoDB  │  │    Redis     │      │
│  │ 反向代理  │  │ 信令+引擎 │  │  持久化   │  │  缓存/队列   │      │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └──────┬───────┘      │
│       │             │             │                │               │
│  ┌────┴─────┐  ┌────┴─────┐  ┌────┴─────┐  ┌──────┴────────┐     │
│  │  MinIO   │  │Dashboard │  │ Ingress  │  │    Egress     │     │
│  │ 录制存储  │  │ 管理面板  │  │ 媒体接入  │  │ 录制/推流输出  │     │
│  └──────────┘  └──────────┘  └──────────┘  └───────────────┘     │
│                                                                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐      │
│  │ Operator │  │   Meet   │  │RdyCheck  │  │    Setup     │      │
│  │ 部署管理  │  │ Web 应用  │  │ 健康检查  │  │  busybox 初始化│     │
│  └──────────┘  └──────────┘  └──────────┘  └──────────────┘      │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────┐       │
│  │    LiveKit Server (Go 二进制, 运行在 openvidu 容器内)      │       │
│  │    · SFU 媒体转发（Pion WebRTC v3）                        │       │
│  │    · 内置 TURN/STUN (端口 3478) — 无需独立 Coturn         │       │
│  └─────────────────────────────────────────────────────────┘       │
└───────────────────────────────────────────────────────────────────┘
```

**关键架构事实：**
- LiveKit 不是独立容器——它是 Go 二进制（`livekit-server`），运行在 `openvidu` 容器内部
- TURN/STUN 由 LiveKit 内置提供（端口 3478），不依赖独立的 Coturn 容器
- 数据库使用 MongoDB（副本集），不使用 PostgreSQL
- 没有 Kurento 容器——录制/推流通过 Egress 服务处理

### 1.3 端口映射

| 端口 | 协议 | 用途 | 来源 |
|------|------|------|------|
| 443 | TCP | HTTPS 入口（Caddy TLS 终止） | caddy |
| 5443 | TCP | OpenVidu 信令 WebSocket API | openvidu |
| 7880 | TCP | LiveKit HTTP 健康检查（openvidu 容器内） | openvidu |
| 7881 | TCP | LiveKit RTC over TCP 回退 | openvidu |
| 7900-7999 | UDP | LiveKit 媒体端口（ICE/DTLS/SRTP） | openvidu |
| 3478 | TCP/UDP | LiveKit 内置 STUN/TURN | openvidu |
| 5349 | TCP/UDP | STUN/TURN over TLS（LiveKit 内置） | openvidu |
| 6379 | TCP | Redis 内部通信 | redis |
| 27017 | TCP | MongoDB 内部通信 | mongo |
| 9000 | TCP | MinIO S3 API | minio |
| 9001 | TCP | MinIO Web Console | minio |

## 2. Docker Compose 服务详解

### 2.1 Caddy（反向代理）

OpenVidu 使用 Caddy 作为 TLS 终止代理。Caddy 自动从 Let's Encrypt 申请证书，无需手动管理证书文件。

**配置重点：**

- 对 `openvidu` 服务反向代理 `/openvidu` 路径，WebSocket 升级
- 对 LiveKit 信令反向代理（通过 openvidu 容器路由）
- 全局配置 HTTP/2 处于启用状态，但针对媒体流路径不做协议升级
- TLS 证书路径通过 Docker 卷挂载持久化
- 默认使用 `openvidu.example.com` 域名，部署时需替换为实际域名

**Caddyfile 核心段（简化）：**

```
openvidu.example.com {
    reverse_proxy /openvidu* openvidu:5443 {
        header_up Host {host}
    }
}
```

### 2.2 OpenVidu 信令服务（含 LiveKit 引擎）

OpenVidu 主容器提供 REST API 和 WebSocket 信令，负责房间管理、参与者生命周期、录制任务调度。**其核心不同之处在于：LiveKit Server 作为一个 Go 二进制（`livekit-server`）运行在该容器内部**，而非独立部署。

这意味着 LiveKit 的 SFU 媒体引擎、信令、TURN/STUN 全部集成在 `openvidu` 这一个容器中。容器内部通过 localhost 通信，无需额外网络跳。

**环境变量：**

```yaml
DOMAIN_OR_PUBLIC_IP=openvidu.example.com
OPENVIDU_SECRET=your-secret-key
OPENVIDU_RECORDING=true
OPENVIDU_RECORDING_PATH=/opt/openvidu/recordings
OPENVIDU_CDR=true  # Call Detail Records
LIVEKIT_KEYS: "openvidu: your-api-key"
LIVEKIT_RTC_PORT_RANGE: "7900-7999"
LIVEKIT_RTC_TCP_PORT: "7881"
LIVEKIT_RTC_UDP_PORT_RANGE: "7900-7999"
LIVEKIT_TURN_ENABLED: "true"
LIVEKIT_TURN_PORT: "3478"
LIVEKIT_TURN_TLS_PORT: "5349"
```

**限制请求体大小：** OpenVidu 默认限制请求体大小为 10MB，用于防止大文件上传攻击。生产环境建议根据实际需求调整 `OPENVIDU_WEBHOOK_MAXBODYSIZE`。

### 2.3 LiveKit（社区版 SFU 核心 — 内嵌于 openvidu 容器）

LiveKit 是 OpenVidu 社区版的默认媒体引擎，**不是独立容器**，而是作为 Go 二进制（`livekit-server`）在 `openvidu` 容器内运行。使用 Pion 库实现 WebRTC 协议栈。特点是单二进制部署、性能稳定、协议兼容性好。

**LiveKit 内部架构：**

```
┌─────────────────────────────────────────────┐
│              LiveKit Server (Go)              │
│                                               │
│  ┌─────────────────────────────────────────┐ │
│  │         RTC 层 (Pion/webrtc v3)         │ │
│  │  ┌──────────┐  ┌──────────┐             │ │
│  │  │ ICE 传输  │  │ DTLS 传输 │             │ │
│  │  └──────────┘  └──────────┘             │ │
│  │  ┌──────────┐  ┌──────────┐             │ │
│  │  │ SRTP/SRTCP│  │ DataChannel│           │ │
│  │  └──────────┘  └──────────┘             │ │
│  └─────────────────────────────────────────┘ │
│                                               │
│  ┌─────────────────────────────────────────┐ │
│  │     Room 路由层                           │ │
│  │  · 房间创建/销毁                          │ │
│  │  · 参与者 Track 发布/订阅                  │ │
│  │  · 音频/视频/屏幕共享 3 种 Track 类型      │ │
│  │  · 单播 → 广播转发（Selective Forwarding）│ │
│  └─────────────────────────────────────────┘ │
│                                               │
│  ┌─────────────────────────────────────────┐ │
│  │  信令层（WebSocket 二进制协议）             │ │
│  │  · Protobuf 编码                          │ │
│  │  · SDP 协商                              │ │
│  │  · ICE Candidate 交换                     │ │
│  │  · Track 订阅管理                         │ │
│  ├─────────────────────────────────────────┤ │
│  │  TURN/STUN 层（内置）                      │ │
│  │  · STUN 绑定请求 (端口 3478)               │ │
│  │  · TURN 中继 (端口 3478/5349 TLS)          │ │
│  └─────────────────────────────────────────┘ │
└─────────────────────────────────────────────┘
```

**LiveKit 媒体端口范围：**

| 范围 | 用途 | 协议 |
|------|------|------|
| 7900-7999 | ICE/DTLS/SRTP 媒体流 | UDP |
| 7881 | RTC over TCP 回退 | TCP |
| 3478 | LiveKit 内置 STUN/TURN | TCP/UDP |
| 5349 | STUN/TURN over TLS | TCP/UDP |

100 个 UDP 端口在典型负载下支持约 300-500 并发参与者（取决于码率、分辨率、帧率）。

**内置 TURN 的工作方式：**

LiveKit 内置 TURN 服务器，无需部署独立的 Coturn 容器。当一个客户端无法建立 P2P ICE 连接时，LiveKit 的 TURN 服务器自动介入中继。配置在 `openvidu` 容器的环境变量中完成：

```yaml
LIVEKIT_TURN_ENABLED: "true"
LIVEKIT_TURN_PORT: "3478"
LIVEKIT_TURN_TLS_PORT: "5349"
```

### 2.4 mediasoup 引擎（专业版）

专业版在 LiveKit 基础上增加 mediasoup 引擎，通过 `openvidu.rtc.engine` 配置项切换。mediasoup 使用 C++ Worker 进程处理媒体，性能更高且支持更细粒度的编码控制。

**切换方式：**

```yaml
# 在 openvidu 容器环境变量中
OPENVIDU_RTC_ENGINE=mediasoup  # 或 pion（默认）

# mediasoup 额外配置
OPENVIDU_MEDIASOUP_WORKER_BIN=/opt/mediasoup/worker/mediasoup-worker
OPENVIDU_MEDIASOUP_RTC_MIN_PORT=7900
OPENVIDU_MEDIASOUP_RTC_MAX_PORT=7999
OPENVIDU_MEDIASOUP_NUM_WORKERS=2
```

**mediasoup 与 LiveKit 的差异：**

| 维度 | LiveKit 引擎 | mediasoup 引擎 |
|------|-------------|----------------|
| 语言 | Go（Pion 库） | C++ Worker + Rust/Node.js 绑定 |
| 进程模型 | 单进程多协程 | 多 Worker 子进程 |
| 编码控制 | 粗粒度（码率/分辨率控制） | 细粒度（每个 Consumer 可独立设置编码参数） |
| 可扩展性 | 水平扩展（Redis 集群路由） | 垂直扩展（多 Worker 绑定 CPU 核心） |
| 延迟 | 3-5ms 转发（Pion 优化） | 1-2ms 转发（原生的 C++） |
| 资源占用 | 每参与者 ~5MB 内存 | 每个 Worker 进程 ~50MB 基线 |

### 2.5 v2 兼容层（专业版）

专业版包含 `v2compatibility` 服务，实现 OpenVidu v2 API 的兼容层。这在从 OpenVidu v2 迁移到 v3 时至关重要。

**兼容层覆盖的 API：**

- Session 创建/关闭（映射到 LiveKit Room）
- Connection 创建（映射到 LiveKit Participant）
- Publisher/Subscriber 模型（映射到 LiveKit Track 发布/订阅）
- Token 生成（v2 的 role 映射到 LiveKit 的 canPublish/canSubscribe）

**兼容限制：**

- `Filter` API（v2 的媒体处理滤镜）仅在 mediasoup 引擎下生效
- v2 的录制回调格式与 v3 不同，需要适配
- 部分 v2 事件（如 `participantPublished`）的字段映射可能存在差异

### 2.6 Redis（缓存/队列/发布订阅）

Redis 在 OpenVidu 中承担三个角色：

| 角色 | 用途 | 数据结构 |
|------|------|----------|
| 缓存 | 会话 Token、房间元数据、短时配置 | String（TTL 设置） |
| 队列 | 异步录制任务、Webhook 事件 | List（RPUSH/LPOP） |
| 发布订阅 | 跨服务事件通知（房间状态变更） | Pub/Sub |

**持久化配置：**

```yaml
redis:
  image: redis:7-alpine
  command: redis-server --appendonly yes --save 60 1000
  volumes:
    - redis-data:/data
```

AOF（Append Only File）持久化模式每秒 fsync，确保 Redis 重启后缓存数据不丢失。RDB 快照作为辅助备份，每 60 秒或在 1000 次写入后触发。

### 2.7 MongoDB（持久化数据库）

MongoDB 副本集存储 OpenVidu 的持久化数据：

- 用户账户和角色
- 录制元数据（录制 ID、时间戳、文件路径、状态）
- CDR（Call Detail Records）——通话详单
- 应用配置（Webhook URL、录制存储策略）

**副本集配置：**

```yaml
mongo:
  image: mongo:7
  command: mongod --replSet rs0 --bind_ip_all
  volumes:
    - mongo-data:/data/db
```

生产环境下 MongoDB 以副本集模式运行，提供高可用和自动故障转移。OpenVidu 通过 MongoDB 驱动连接副本集，读写分离以提升性能。

### 2.8 MinIO（录制存储）

MinIO 提供兼容 S3 的录制文件存储。OpenVidu 录制生成的 `.webm` 文件通过 MinIO 客户端 SDK 上传。

**存储路径结构：**

```
recordings/
  └── {session_id}/
      ├── participant_{participant_id}/
      │   ├── video_{timestamp}.webm
      │   └── audio_{timestamp}.webm
      └── metadata.json
```

**数据生命周期：**

MinIO 默认保留所有文件。生产环境应配置生命周期策略：

```yaml
# 通过 MinIO 客户端配置
mc ilm rule add --expire-days 90 myminio/recordings
```

### 2.9 Dashboard（管理面板）

Dashboard 容器提供基于 Web 的管理面板，用于：

- 实时查看活跃会话和参与者
- 录制文件管理和回放
- 系统配置和引擎切换
- 监控指标可视化

镜像来源：`docker.io/openvidu/dashboard`

### 2.10 Ingress（媒体接入）

Ingress 服务负责外部媒体流的接入，支持：

- RTMP 推流接入
- WHIP（WebRTC-HTTP Ingestion Protocol）接入
- 外部 RTSP 源拉取

Ingress 将外部流转换为 LiveKit Track，使其可以被会议中的参与者消费。

### 2.11 Egress（录制/推流输出）

Egress 服务负责录制合成和流输出，取代了旧版 OpenVidu v2 中的 Kurento：

- **录制合成**：多路视频合成为一路（Composite 模式）或分别录制（Individual 模式）
- **RTMP 推流输出**：将会议画面推流到直播平台（YouTube、Twitch 等）
- **文件输出**：MP4 / WebM 格式的录制文件直写 MinIO

Egress 直接与 LiveKit 交互，通过 gRPC 获取媒体流，不经过 OpenVidu 信令层。

### 2.12 Operator（部署管理）

Operator 容器管理 OpenVidu 部署的生命周期：

- 自动证书更新
- 服务健康监控和自动重启
- 配置热更新
- 版本升级编排

### 2.13 Setup 容器（初始化）

Setup 容器使用 `busybox` 镜像，在 OpenVidu 主服务启动前完成初始化工作：

```yaml
setup:
  image: busybox:1.36
  entrypoint:
    - /bin/sh
    - -c
    - |
      mkdir -p /opt/openvidu/recordings
      chown -R 1000:1000 /opt/openvidu/recordings
      mkdir -p /opt/openvidu/custom-layout
      echo "Init complete"
  volumes:
    - openvidu-data:/opt/openvidu
```

**Setup 容器执行的工作：**

1. 创建录制目录 `/opt/openvidu/recordings`（OpenVidu 用户 UID 1000）
2. 设置目录权限，确保 OpenVidu 容器写入权限
3. 创建自定义布局目录 `/opt/openvidu/custom-layout`
4. 初始化日志目录 `/opt/openvidu/logs`
5. 完成后退出（`restart: "no"`）

### 2.14 Ready Check（健康检查）

Ready Check 容器在启动时验证所有依赖服务是否就绪（MongoDB、Redis、MinIO、LiveKit），通过后向 Caddy 返回就绪信号，Caddy 才开始转发流量。

### 2.15 Meet（Web 会议应用）

OpenVidu Meet 容器提供开箱即用的 Web 会议前端应用，基于 React 构建。用户可直接通过浏览器加入会议，无需自行开发客户端。

镜像来源：`docker.io/openvidu/openvidu-meet`

## 3. 网络拓扑

### 3.1 内部网络

OpenVidu 使用 Docker 自定义网络 `openvidu-net`，默认子网 `172.28.0.0/16`。所有容器通过服务名互相寻址，无需手动配置 IP。

```
openvidu-net (172.28.0.0/16)
 ├── caddy (172.28.0.2)
 ├── openvidu (172.28.0.3)  ← LiveKit Server 二进制在内部运行
 ├── mongo (172.28.0.4)
 ├── redis (172.28.0.5)
 ├── minio (172.28.0.6)
 ├── dashboard (172.28.0.7)
 ├── ingress (172.28.0.8)
 ├── egress (172.28.0.9)
 ├── operator (172.28.0.10)
 ├── meet (172.28.0.11)
 ├── ready-check (172.28.0.12)
 └── setup (172.28.0.13)
```

### 3.2 外部流量路径

```
外部客户端
    │
    ├── HTTPS (443) → Caddy → TLS 终止
    │   ├── /openvidu/* → WebSocket/REST → openvidu:5443
    │   └── /livekit/* → WebSocket → openvidu (LiveKit 内嵌)
    │
    ├── STUN (3478) → openvidu (LiveKit 内置 STUN)
    │
    ├── TURN (5349/TLS) → openvidu (LiveKit 内置 TURN/TLS)
    │
    └── UDP 媒体 (7900-7999) → openvidu (LiveKit ICE/DTLS/SRTP)
```

### 3.3 防火墙规则

生产环境至少需要以下防火墙入站规则：

| 规则 | 端口 | 协议 | 来源 | 说明 |
|------|------|------|------|------|
| HTTPS | 443 | TCP | 0.0.0.0/0 | Web 访问和信令 |
| STUN | 3478 | TCP/UDP | 0.0.0.0/0 | 候选地址发现（LiveKit 内置） |
| TURN/TLS | 5349 | TCP/UDP | 0.0.0.0/0 | 加密中继连接（LiveKit 内置） |
| 媒体端口 | 7900-7999 | UDP | 0.0.0.0/0 | RTP/RTCP 媒体流 |
| SSH | 22 | TCP | 限定管理 IP | 服务器管理 |

## 4. 启动流程

### 4.1 首次启动顺序

```
时序 容器           动作
 │    ├── setup       创建目录结构，设置权限，退出
 │    ├── mongo       启动数据库副本集，等待就绪
 │    ├── redis       启动缓存，等待就绪
 │    ├── minio       启动 S3 存储，等待就绪
 │    ├── openvidu    启动主服务 + LiveKit Server 二进制
 │    │   ├── 连接 MongoDB、Redis、MinIO
 │    │   ├── 启动 LiveKit Server（端口 7880/7881/7900-7999/3478/5349）
 │    │   └── 启动信令 WebSocket 服务（端口 5443）
 │    ├── ingress     启动媒体接入服务
 │    ├── egress      启动录制/推流服务
 │    ├── dashboard   启动管理面板
 │    ├── meet        启动 Web 会议前端
 │    ├── operator    启动部署管理
 │    ├── ready-check 验证所有依赖就绪
 │    └── caddy       最后启动，配置 TLS 证书，开始接收流量
 ▼
就绪                  ready-check 返回 200，Caddy 开始转发
```

### 4.2 健康检查

OpenVidu 提供多个健康检查端点：

| 端点 | 用途 | 返回 |
|------|------|------|
| `/openvidu/health` | OpenVidu 主服务 | `200 OK` |
| `/livekit/health` | LiveKit 引擎（openvidu 容器内） | `serving` |
| `/openvidu/api/rooms` | API 可用性 | 房间列表 |

### 4.3 日志诊断

**常见启动问题：**

| 症状 | 根因 | 解法 |
|------|------|------|
| Caddy TLS 证书申请失败 | 域名未绑定到公网 IP | 检查 DNS A 记录 |
| LiveKit 启动失败 | 端口 7900-7999 被占用 | `netstat -tulpn \| grep 7900` 检查 |
| MongoDB 副本集未初始化 | 首次部署需手动 `rs.initiate()` | 运行 mongo shell 执行初始化 |
| 录制文件无法写入 | MinIO 未就绪或权限不足 | 检查 MinIO 日志和 bucket 策略 |

## 5. 持久化存储

### 5.1 卷映射

```yaml
volumes:
  caddy-data:
    external: false
  openvidu-data:
    external: false
  redis-data:
    external: false
  mongo-data:
    external: false
  minio-data:
    external: false
```

建议生产环境使用 NFS 或云存储提供商的持久化卷，确保容器重启后数据不丢失。

### 5.2 数据备份策略

| 数据 | 备份频率 | 方法 | 保留策略 |
|------|---------|------|---------|
| MongoDB | 每日 | mongodump + MinIO 上传 | 30 天 |
| Redis AOF | 每小时 | 复制 AOF 文件 | 7 天 |
| MinIO 录制 | 按需 | 跨区域复制 | 90 天 |
| Caddy 证书 | 每次更新 | 自动备份到 MinIO | 365 天 |

## 6. 引擎切换详解

### 6.1 运行时切换

OpenVidu 专业版支持在运行时通过环境变量切换 RTC 引擎。切换后重启 `openvidu` 容器即可生效，无需重新部署整个栈。

```
openvidu.rtc.engine = pion
    └── OpenVidu 通过 LiveKit 客户端 SDK 控制内嵌的 LiveKit Server
    └── 所有媒体路由经过 LiveKit 的 Pion 引擎

openvidu.rtc.engine = mediasoup
    └── OpenVidu 通过 mediasoup 客户端 SDK 控制 mediasoup Worker
    └── 所有媒体路由经过 mediasoup 的 C++ Worker
    └── mediasoup Worker 内嵌于 OpenVidu 容器，不增额外容器
```

### 6.2 引擎切换的影响范围

| 维度 | Pion 引擎 | mediasoup 引擎 |
|------|----------|----------------|
| 容器数 | 12（标准部署） | 12（无增减） |
| 内存占用 | ~200MB baseline | ~400MB baseline |
| CPU 占用 | 转发 100 路流 ~10% | 转发 100 路流 ~5% |
| 信令协议 | LiveKit Protobuf | mediasoup JSON |
| 录制路径 | Egress 服务 | Egress 服务（不变） |
| v2 兼容 | 基本兼容 | 完整兼容（含 Filter） |

### 6.3 切换建议

- **选择 Pion 引擎**：轻量部署、快速启动、社区版默认。适合中小规模会议（<50 参与者）、IoT 设备、快速原型验证
- **选择 mediasoup 引擎**：高性能场景、大规模会议（100+ 参与者）、需要细粒度编码控制的场景。适合专业视频会议、直播推流、教育平台

## 7. MediaServo 借鉴

### 7.1 架构借鉴

| OpenVidu 实践 | MediaServo 对应 | 借鉴价值 |
|---------------|---------------|----------|
| 12 容器微服务架构 | 7 crate 工作区 | 容器化边界明确，服务间职责清晰 |
| Caddy 反向代理做 TLS 终止 | mediaservo-server 当前直接暴露 | 引入反向代理层分离证书管理 |
| Setup 容器做目录初始化 | mediaservo-host 启动时初始化 | 分离初始化逻辑，确保容器启动顺序正确 |
| 引擎可切换（pion/mediasoup） | mediaservo-webrtc 三后端抽象 | 后端切换模式一致，可复用 LiveKit 的配置驱动思路 |
| LiveKit 内嵌模式（无独立 SFU 容器） | mediaservo-server SFU 集成 | SFU 与服务同容器减少网络跳数和部署复杂度 |
| TURN 由 SFU 内置提供 | mediaservo-server ICE 候选地址 | 减少独立 TURN 容器的运维开销 |

### 7.2 端口规划借鉴

| 端口范围 | 复用方式 |
|----------|----------|
| 7900-7999 | 100 端口范围适用于中等规模部署，MediaServo 当前 40000-40100 范围过窄 |
| 3478/5349 | LiveKit 内置 TURN 端口，MediaServo 可参考将 STUN/TURN 集成到 SFU 而非独立部署 |

### 7.3 引入反向代理层

MediaServo 当前 `mediaservo-server` 直接暴露 9800 端口。借鉴 OpenVidu 的 Caddy 模式：

```
当前：
  Client → mediaservo-server:9800 (直连，无 TLS)

借鉴后：
  Client → Caddy (443 TLS) → mediaservo-server:9800 (内网)
```

优势：
- TLS 证书自动管理（Let's Encrypt）
- 路径路由（/signaling, /admin, /livekit 等）
- WebSocket 升级统一处理
- 负载均衡和健康检查集中管理

### 7.4 引擎切换策略

OpenVidu 的 `openvidu.rtc.engine` 配置驱动切换模式与 MediaServo 的 `mediaservo-webrtc` 三后端抽象不谋而合。MediaServo 可借鉴其配置方式：

```toml
# MediaServo 未来配置
[webrtc]
backend = "webrtc-sys"  # webrtc-rs | webrtc-sys | str0m
```

社区版默认使用 `webrtc-sys`（libwebrtc 稳定性能），专业版允许切换。切换逻辑在 `mediaservo-webrtc/src/backend/` 中由 feature flag 和运行时配置共同决定。

### 7.5 录制架构

OpenVidu 的录制依赖 MinIO + Egress 服务（不再使用 Kurento）。MediaServo 的录制路径可简化为：

```
MediaServo 录制（当前）：
  Host → RTP → mediaservo-server → 文件写入

OpenVidu 录制（借鉴）：
  LiveKit → Egress 合成/编码 → MinIO S3 存储 → 生命周期管理
```

MediaServo 未来录制可引入 MinIO 兼容层，复用 OpenVidu 的 S3 录制路径和生命周期策略。
