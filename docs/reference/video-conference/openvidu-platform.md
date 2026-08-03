# OpenVidu Platform 参考分析

> 生成日期：2026-07-31 | 分类：视频会议
> 仓库：https://github.com/OpenVidu/openvidu

## 1. 产品画像

- **名称**：OpenVidu Platform
- **开发者**：OpenVidu（西班牙团队，源于 Universidad Rey Juan Carlos 的 Kurento 项目）
- **核心人物**：Luis López（CEO, 社区驱动）
- **首次发布**：2016 年（OpenVidu v1 基于 Kurento）
- **产品定位**：OpenVidu 是一个视频会议平台，而非媒体引擎。它在底层媒体引擎（Kurento / mediasoup / LiveKit）之上封装了完整的 REST API + 客户端 SDK + 部署套件 + 运维工具，目标是让开发者用 5 行代码在应用中嵌入视频会议
- **目标用户群体**：需要快速集成视频会议的中小企业、教育平台、远程医疗应用。典型用户：在线教育平台（OpenVidu 在欧洲在线教育市场占有率较高）、远程医疗咨询、企业协作 SaaS 集成
- **许可 / 商业模式**：Apache 2.0（CE 社区版，免费）。OpenVidu Pro 和 Enterprise 为付费版本，提供弹性集群、高可用、商业支持。v3 迁移后，CE 版获得了大量原 Pro 版功能

## 2. 架构

### 2.1 Master-Worker 架构

OpenVidu 采用经典的双层 Master-Worker 架构，控制面与媒体面完全分离：

```
┌──────────────────────────────────────────────────────────────────┐
│  openvidu-server (Java Spring Boot) — Master                     │
│                                                                  │
│  ┌────────────────────┐  ┌────────────────────┐                 │
│  │  REST API          │  │  WebSocket Signal   │                │
│  │  · POST /sessions  │  │  · SDP 协商        │                 │
│  │  · POST /connections│  │  · ICE 交换        │                 │
│  │  · DELETE /sessions │  │  · 连接状态管理     │                 │
│  └────────────────────┘  └────────────────────┘                 │
│  ┌────────────────────┐  ┌────────────────────┐                 │
│  │  Session Manager   │  │  Token Auth        │                 │
│  │  · 房间生命周期    │  │  · PUBLISHER/      │                 │
│  │  · 参与者管理      │  │    SUBSCRIBER 角色  │                 │
│  │  · 媒体路由      │  │  · 过期/吊销        │                 │
│  └────────────────────┘  └────────────────────┘                 │
│  ┌────────────────────┐  ┌────────────────────┐                 │
│  │  Recording Manager │  │  CDR (Call Detail  │                 │
│  │  · 录制触发/停止   │  │  Record)           │                 │
│  │  · 文件管理        │  │  · 通话统计        │                 │
│  └────────────────────┘  └────────────────────┘                 │
└──────────────────────────┬───────────────────────────────────────┘
                           │ REST / WebSocket
┌──────────────────────────┴──────────────────────────────────────┐
│  Media Node (Worker) — 媒体面                                    │
│                                                                  │
│  v2: Kurento Media Server (C++/GStreamer)                       │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  Kurento Media Pipeline                                   │  │
│  │  WebRtcEndpoint → Composite (MCU) / Dispatcher (SFU)      │  │
│  │  RecorderEndpoint → WebM/MP4                              │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                  │
│  v3 Enterprise: mediasoup (C++/Rust)                            │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  mediasoup Worker → Router → Transport → Producer/Consumer│  │
│  │  Simulcast/SVC 层选择, 1000+ stream per node              │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                  │
│  v3 CE: openvidu-livekit (Go, LiveKit fork)                    │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  LiveKit Server (Pion WebRTC, 纯 Go)                      │  │
│  │  信令 + SFU + 录制 (Egress) 一体化                        │  │
│  └───────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

### 2.2 v2 vs v3 架构对比

| 维度 | OpenVidu v2 | OpenVidu v3 |
|------|------------|------------|
| 媒体引擎 | Kurento (GStreamer, C++) | LiveKit (CE) / mediasoup (Enterprise) |
| 架构 | MCU + SFU 双模式 | 纯 SFU (LiveKit/mediasoup) |
| 性能 | 单节点 ~100 流 | 单节点 ~500-1000 流 (mediasoup) |
| 部署 | Docker Compose 单节点 | Docker Compose / K8s |
| 监控 | Elastic Stack | Prometheus + Loki + Grafana |
| E2EE | 不支持 | 支持 (Insertable Streams) |
| 迁移路径 | — | v2compatibility 模块保持 API 兼容 |

### 2.3 部署拓扑

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Browser     │     │  Browser     │     │  Browser     │
│  (Client)    │     │  (Client)    │     │  (Client)    │
└──────┬───────┘     └──────┬───────┘     └──────┬───────┘
       │                    │                    │
       │  HTTPS + WSS       │  HTTPS + WSS       │  HTTPS + WSS
       │  (REST + Signal)   │  (REST + Signal)   │  (REST + Signal)
       ├────────────────────┼────────────────────┘
       │                    │
       ▼                    ▼
┌──────────────────────────────────────────────────────────────┐
│  openvidu-server (Java Spring Boot)                          │
│  · 信令控制面 (REST API + WebSocket)                         │
│  · 会话管理 · Token 认证 · 录制编排                         │
│  · 媒体节点调度 (CE: 固定 / Pro: 弹性)                      │
└──────────────────────────┬───────────────────────────────────┘
                           │
                           │  内部 HTTP + WebSocket
                           │
┌──────────────────────────┴───────────────────────────────────┐
│  Media Node (Docker 容器)                                     │
│  · Kurento (v2) / LiveKit (v3 CE) / mediasoup (v3 Ent)      │
│  · WebRTC 媒体面 · 录制执行                                  │
│  · 生命周期: launching → running → terminated               │
└──────────────────────────────────────────────────────────────┘
```

## 3. REST API

### 3.1 Session API

OpenVidu 的核心 API 围绕 Session 和 Connection 两个抽象：

```
Session (房间)
├── sessionId: 唯一标识
├── createdAt: 创建时间
├── connections: Connection[]
├── recording: 录制配置
├── mediaMode: ROUTED / RELAYED
├── defaultOutputMode: COMPOSED / INDIVIDUAL
└── customSessionId: (可选) 自定义 ID

Connection (参与者连接)
├── connectionId: 唯一标识
├── status: pending / connecting / connected / disconnected
├── role: PUBLISHER / SUBSCRIBER
├── token: 认证令牌
├── clientData: 客户端自定义数据
├── serverData: 服务端自定义数据
├── createdAt: 创建时间
├── location: 地理位置 (IP 推断)
├── platform: 客户端平台 (浏览器/OS 版本)
├── publishers: Publisher[] (PUBLISHER 角色)
└── subscribers: Subscriber[] (PUBLISHER 角色)
```

### 3.2 关键端点

| 端点 | 方法 | 描述 |
|------|------|------|
| `/api/sessions` | POST | 创建 Session。参数: `customSessionId`, `mediaMode`, `recordingMode`, `defaultOutputMode` |
| `/api/sessions/<sessionId>` | GET | 获取 Session 信息 |
| `/api/sessions/<sessionId>` | DELETE | 关闭 Session, 断开所有连接 |
| `/api/sessions/<sessionId>/connection` | POST | 创建 Connection。参数: `role`, `data`, `kurentoOptions` |
| `/api/sessions/<sessionId>/connection/<connId>` | DELETE | 断开指定 Connection |
| `/api/tokens` | POST | 生成 Token。参数: `session`, `role`, `data`, `kurentoOptions` |
| `/api/recordings/start` | POST | 开始录制 Session |
| `/api/recordings/stop` | POST | 停止录制 |
| `/api/recordings` | GET | 列出所有录制 |
| `/api/recordings/<recordingId>` | DELETE | 删除录制文件 |
| `/api/recordings/<recordingId>` | GET | 获取录制详情 |
| `/api/health` | GET | 健康检查 |
| `/api/call-detail-records` | GET | 通话详情记录 (CDR) |

### 3.3 Token 认证

OpenVidu 使用 Token 机制控制访问权限：

```json
// POST /api/sessions/<sessionId>/connection 响应
{
  "id": "con_xxxxx",
  "token": "wss://openvidu.example.com?sessionId=ses_xxx&token=tok_xxxxx",
  "status": "pending",
  "createdAt": 1720000000000,
  "role": "PUBLISHER",
  "clientData": "{\"displayName\":\"Alice\"}",
  "serverData": "{\"userId\":\"user_123\"}"
}
```

Token 生成流程：
1. 服务端调用 `POST /api/sessions` 创建 Session
2. 服务端调用 `POST /api/sessions/<id>/connection` 生成 Token
3. Token 包含 `role` (PUBLISHER/SUBSCRIBER) 和自定义数据
4. 客户端使用 Token 连接 WebSocket 信令
5. 连接后根据角色决定媒体权限：PUBLISHER 可发布和订阅，SUBSCRIBER 仅可订阅

### 3.4 角色模型

| 角色 | 发布权限 | 订阅权限 | 典型场景 |
|------|---------|---------|---------|
| PUBLISHER | ✅ 发布音视频 | ✅ 订阅所有 | 普通与会者 |
| SUBSCRIBER | ❌ 不可发布 | ✅ 订阅所有 | 观众/监看者 |
| (自定义) | 可通过 API 组合 | 可通过 API 组合 | 自定义角色 |

## 4. Session 生命周期

### 4.1 标准流程

```
┌─────────┐     ┌──────────┐     ┌──────────┐
│ Client  │     │ openvidu │     │ Media    │
│         │     │ -server  │     │ Node     │
└────┬────┘     └────┬─────┘     └────┬─────┘
     │                │                │
     │ 1. POST /sessions              │
     │────────────────>│              │
     │  {sessionId}    │              │
     │<────────────────│              │
     │                │                │
     │ 2. POST /connection            │
     │────────────────>│              │
     │  {token, role}  │              │
     │<────────────────│              │
     │                │                │
     │ 3. WebSocket 连接              │
     │  (with token)   │              │
     │════════════════>│              │
     │                │                │
     │ 4. joinRoom     │              │
     │────────────────>│  5. 创建媒体   │
     │                │  资源         │
     │                │──────────────>│
     │                │               │
     │ 6. SDP Offer   │ 7. processOffer│
     │<═══ SDP Offer ═══│════════════>│
     │                │              │
     │ 8. SDP Answer  │ 9. SDP Answer│
     │════════════════>│<══════════════│
     │                │              │
     │════════════════════════════════│
     │  ICE/DTLS/SRTP 媒体通道       │
     │════════════════════════════════│
```

### 4.2 Session 状态机

```
         ┌──────────┐
         │  Created │    (POST /api/sessions)
         └────┬─────┘
              │
      ┌───────┴───────┐
      │  In Progress  │    (至少 1 个连接)
      └───────┬───────┘
              │
       ┌──────┴──────┐
       │  Destroyed  │    (DELETE /api/sessions/<id>)
       └─────────────┘
```

Session 在以下情况下自动销毁：
- 所有参与者断开连接后，经过 `session.config.sessionGracefulShutdownTimeout` 超时（默认 30 秒）
- 显式调用 DELETE API
- 媒体节点故障触发

### 4.3 媒体模式

OpenVidu 支持两种媒体模式：

| 模式 | 描述 | 适用场景 |
|------|------|---------|
| ROUTED | 所有媒体流经 Server。Server 负责路由、录制、转码。强制使用媒体节点 | 录制、转码、网络质量优化 |
| RELAYED | 客户端间 P2P 直连。Server 仅做信令中介。不经过媒体节点 | 低延迟、减少服务器负载 |

## 5. Server 架构

### 5.1 openvidu-server (Java Spring Boot)

openvidu-server 是整个平台的控制面中枢：

```
openvidu-server/
├── src/main/java/io/openvidu/server/
│   ├── OpenViduServer.java           # 主入口, Spring Boot 启动
│   ├── config/                       # 配置 (CORS, Security, WebSocket)
│   ├── controllers/                  # REST API 控制器
│   │   ├── SessionController.java    # /api/sessions
│   │   ├── ConnectionController.java # /api/connections
│   │   ├── RecordingController.java  # /api/recordings
│   │   ├── TokenController.java      # /api/tokens
│   │   └── HealthController.java     # /api/health
│   ├── services/                     # 业务逻辑
│   │   ├── SessionService.java       # 会话管理
│   │   ├── MediaNodeService.java     # 媒体节点调度
│   │   ├── RecordingService.java     # 录制管理
│   │   └── CdrService.java           # 通话详情记录
│   ├── websocket/                    # WebSocket 信令
│   │   ├── SignalingHandler.java     # 信令消息处理
│   │   └── WebSocketConfig.java      # WS 端点配置
│   ├── models/                       # 领域模型
│   │   ├── Session.java
│   │   ├── Connection.java
│   │   ├── Token.java
│   │   ├── Recording.java
│   │   └── MediaNode.java
│   └── kurento/                      # Kurento 集成 (v2)
│       ├── KurentoSessionManager.java
│       └── KurentoMediaPipeline.java
```

### 5.2 配置项

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `OPENVIDU_SECRET` | 无 | 服务端 API 密钥 |
| `OPENVIDU_PUBLIC_URL` | `https://localhost:4443` | 公开访问 URL |
| `OPENVIDU_CDR` | false | 启用通话详情记录 |
| `OPENVIDU_CDR_PATH` | `/opt/openvidu/cdr` | CDR 日志路径 |
| `OPENVIDU_RECORDING` | false | 启用录制功能 |
| `OPENVIDU_RECORDING_PATH` | `/opt/openvidu/recordings` | 录制文件存储路径 |
| `OPENVIDU_RECORDING_COMPOSED_OVERLAY` | false | 复合录制水印 |
| `OPENVIDU_SESSIONS_GARBAGE_INTERVAL` | 900 | Session 清理间隔 (秒) |
| `OPENVIDU_SESSIONS_GARBAGE_THRESHOLD` | 3600 | Session 过期阈值 (秒) |
| `OPENVIDU_MEDIA_NODE_CPU_USE_THRESHOLD` | 80 | 媒体节点 CPU 阈值 (%) |
| `OPENVIDU_MEDIA_NODE_CPU_RESUME_THRESHOLD` | 60 | 恢复调度阈值 (%) |

### 5.3 健康检查

openvidu-server 暴露 `/api/health` 端点，返回 JSON 状态：

```json
{
  "status": "UP",
  "mediaNodes": [
    {
      "id": "node_xxxxx",
      "status": "running",
      "cpuUsage": 45.2,
      "memoryUsage": 60.1,
      "sessionCount": 12,
      "connectionCount": 48
    }
  ],
  "version": "2.29.0"
}
```

## 6. openvidu-livekit (LiveKit Fork)

### 6.1 定位

openvidu-livekit 是 OpenVidu v3 的核心组件，但它不是通用 LiveKit 的分支——它是为 OpenVidu 定制化的 LiveKit 变体，主要改动集中在：

- **Analytics 管线**：将 LiveKit 的 Prometheus 指标通过 MongoDB 持久化，提供历史查询能力
- **Telemetry 采集**：扩展了 LiveKit 的 TelemetryService，增加 OpenVidu 特定的 RTP/ICE 质量指标
- **v2compatibility 模块**：保持与 OpenVidu v2 REST API 的向后兼容
- **移除 AI Agent 相关代码**：OpenVidu 不需要 LiveKit Agents Framework

### 6.2 架构

```
┌──────────────────────────────────────────────────────────────┐
│  openvidu-livekit (Go, 基于 LiveKit)                          │
│                                                              │
│  ┌────────────────────┐  ┌────────────────────┐             │
│  │  LiveKit Server    │  │  Analytics 扩展     │            │
│  │  (Pion WebRTC)     │  │  ───────────────── │             │
│  │  · WebSocket 信令  │  │  · RTP 质量统计    │             │
│  │  · Room Service    │  │  · ICE 连接统计    │             │
│  │  · SFU Pipeline    │  │  · 客户端版本统计   │             │
│  │  · Egress 录制     │  │  · 会话级指标聚合   │             │
│  └────────────────────┘  └────────────────────┘             │
│  ┌────────────────────┐  ┌────────────────────┐             │
│  │  v2compatibility   │  │  MongoDB Adapter    │            │
│  │  · REST API 映射   │  │  · 指标持久化       │             │
│  │  · Token 转换      │  │  · 历史查询        │             │
│  │  · Session 翻译    │  │  · 趋势分析        │             │
│  └────────────────────┘  └────────────────────┘             │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Telemetry/Event Pipeline                             │  │
│  │  · 参与者加入/离开 → 事件记录 → MongoDB               │  │
│  │  · 连接质量 → 5s 采样 → MongoDB 时序集合              │  │
│  │  · 录制状态 → 事件记录 → MongoDB                      │  │
│  └──────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────┘
```

### 6.3 与 LiveKit 的核心差异

| 维度 | LiveKit (原版) | openvidu-livekit (fork) |
|------|---------------|------------------------|
| AI Agent | 一等公民, Agents Framework | 已移除 (OpenVidu 不需要) |
| 存储 | Redis (分布式) / Local (单机) | MongoDB (仅 analytics) |
| 路由 | LocalRouter / RedisRouter | 简化: 单节点, 无分布式路由 |
| 指标 | Prometheus 原生 | Prometheus + MongoDB 持久化 |
| 客户端 | 14 种 SDK, 私有信令协议 | 保持 OpenVidu v2 客户端 SDK, 内部适配 |
| API 层 | LiveKit Twirp/REST API | OpenVidu REST API 通过 v2compatibility 模块 |
| 录制 | Egress 独立服务 | 继承 Egress, 保持 OpenVidu 录制 API |

### 6.4 重要结论

openvidu-livekit 的 LiveKit fork 不是媒体路由层面的选择——它是对 LiveKit **Telemetry 和 Analytics 管线的借用**。OpenVidu 需要：

1. 会话级质量指标（参与者延迟、丢包率、解像度）
2. 历史趋势分析（按时间、按房间、按参与者聚合）
3. CDR (Call Detail Record) 用于计费和审计

LiveKit 的 TelemetryService 已经采集了这些数据，OpenVidu 通过 MongoDB 持久化使其可查询。这比 OpenVidu v2 中基于 Elastic Stack 的方案更轻量、运维成本更低。

## 7. 客户端 SDK

### 7.1 openvidu-browser (v2)

OpenVidu v2 的核心客户端库是 `openvidu-browser.js`，封装了浏览器 WebRTC API：

```javascript
import { OpenVidu } from 'openvidu-browser';

// 1. 初始化
const OV = new OpenVidu();
const session = OV.initSession();

// 2. 连接 Session
session.connect(token)
  .then(() => console.log('connected'))
  .catch(err => console.error(err));

// 3. 发布本地媒体
const publisher = await OV.initPublisher('camera', {
  audioSource: undefined,  // 默认麦克风
  videoSource: undefined,  // 默认摄像头
  publishAudio: true,
  publishVideo: true,
  resolution: '1280x720',
  frameRate: 30,
});
await session.publish(publisher);

// 4. 订阅远程媒体
session.on('streamCreated', event => {
  const subscriber = session.subscribe(event.stream, 'subscriber-div');
  subscriber.on('videoElementCreated', event => {
    document.getElementById('remote-video').appendChild(event.element);
  });
});
```

### 7.2 v3 客户端迁移

OpenVidu v3 将客户端 SDK 从自研 `openvidu-browser.js` 迁移到 LiveKit 客户端 SDK：

| 能力 | v2 (openvidu-browser) | v3 (LiveKit SDK) |
|------|----------------------|-------------------|
| 信令 | 私有 WebSocket 协议 | LiveKit 二进制 WS 协议 |
| 发布 | `session.publish(publisher)` | `room.localParticipant.publishTrack()` |
| 订阅 | `session.subscribe(stream)` | `room.on('trackSubscribed', ...)` |
| 屏幕共享 | `OV.initPublisher('screen')` | `createLocalScreenTrack()` |
| 录制 | 通过 OpenVidu REST API | 通过 OpenVidu REST API (兼容) |
| 角色 | OpenVidu 角色模型 | 映射到 LiveKit 权限 |

### 7.3 服务端 SDK

| 语言 | 包名 | 用途 |
|------|------|------|
| Java | `openvidu-java-client` | 后端应用集成 |
| Node.js | `openvidu-node-client` | Node.js 后端集成 |
| Python | `openvidu-python-client` | Python 后端 (社区维护) |
| Ruby | `openvidu-ruby-client` | Ruby 后端 (社区维护) |
| PHP | `openvidu-php-client` | PHP 后端 (社区维护) |
| Go | `openvidu-go-client` | Go 后端 (社区维护) |
| .NET | `openvidu-dotnet-client` | .NET 后端 (社区维护) |

服务端 SDK 只封装 REST API 调用（创建 Session、生成 Token、管理录制等），不涉及 WebRTC 媒体处理：

```java
// Java 服务端 SDK 示例
OpenVidu openvidu = new OpenVidu("https://openvidu.example.com", "MY_SECRET");

// 创建 Session
Session session = openvidu.createSession();
String sessionId = session.getSessionId();

// 生成 Token
ConnectionProperties properties = new ConnectionProperties.Builder()
    .role(OpenViduRole.PUBLISHER)
    .data("{\"displayName\":\"Alice\"}")
    .build();
Connection connection = session.createConnection(properties);
String token = connection.getToken();

// 返回 token 给客户端
```

## 8. 录制

### 8.1 录制模式

OpenVidu 支持两种录制输出模式：

| 模式 | 描述 | 输出格式 | 适用场景 |
|------|------|---------|---------|
| COMPOSED | 所有参与者合成为单个视频网格。GStreamer 合成管线 | MP4 (H.264+AAC) | 会议存档, 回放 |
| INDIVIDUAL | 每个参与者独立录制, 单独文件 | WEBM (VP8+Opus) | 后期编辑, 单流分析 |

### 8.2 COMPOSED 录制架构

COMPOSED 录制使用 GStreamer 合成管线：

```
┌────────────────────────────────────────────────────────────────┐
│  GStreamer 合成管线 (COMPOSED 录制)                             │
│                                                                │
│  ┌────────┐  ┌────────┐  ┌────────┐                           │
│  │  Stream 1│  │  Stream 2│  │  Stream N│                       │
│  │  (VP8)  │  │  (VP8)  │  │  (VP8)  │                       │
│  └───┬────┘  └───┬────┘  └───┬────┘                           │
│      │           │           │                                │
│      ▼           ▼           ▼                                │
│  ┌──────────────────────────────────────────────────────┐    │
│  │  compositor (GStreamer compositor plugin)             │    │
│  │  · 网格布局: 1x1, 2x2, 3x3, 4x4...                   │    │
│  │  · 自动调整参与者位置                                 │    │
│  └────────────────────────┬─────────────────────────────┘    │
│                           │                                   │
│                           ▼                                   │
│  ┌──────────────────────────────────────────────────────┐    │
│  │  encoder (H.264) + muxer (MP4)                       │    │
│  │  · 视频: H.264 编码, 可配置码率                      │    │
│  │  · 音频: AAC 混合, 所有参与者音频混合                 │    │
│  └──────────────────────────────────────────────────────┘    │
│                           │                                   │
│                           ▼                                   │
│                    ┌──────────────┐                           │
│                    │  output.mp4  │                           │
│                    └──────────────┘                           │
└────────────────────────────────────────────────────────────────┘
```

### 8.3 录制 API

```json
// POST /api/recordings/start
{
  "session": "ses_xxxxx",
  "name": "meeting-record-001",
  "outputMode": "COMPOSED",
  "hasAudio": true,
  "hasVideo": true,
  "resolution": "1920x1080",
  "frameRate": 25,
  "shmSize": 536870912
}

// 响应
{
  "id": "rec_xxxxx",
  "sessionId": "ses_xxxxx",
  "name": "meeting-record-001",
  "status": "started",
  "recordingMode": "COMPOSED",
  "resolution": "1920x1080",
  "frameRate": 25,
  "createdAt": 1720000000000
}
```

### 8.4 录制生命周期

```
status: started → stopped → ready → (failed)
                        ↓
                    available
                        ↓
                (DELETE → deleted)
```

| 状态 | 说明 |
|------|------|
| started | 录制已开始, 正在写入文件 |
| stopped | 录制已停止, 文件正在处理 (合成/转码) |
| ready | 录制文件已就绪, 可下载 |
| failed | 录制失败 (磁盘满/编码错误) |
| available | 录制文件可获取 (v2/v3 兼容状态) |
| deleted | 录制文件已被删除 |

### 8.5 录制文件管理

录制文件默认存储在 `OPENVIDU_RECORDING_PATH` 路径下，支持 S3/Azure Blob Storage 同步：

```bash
# 录制文件结构
/opt/openvidu/recordings/
├── rec_xxxxx/
│   ├── video.mp4           # COMPOSED 录制
│   └── metadata.json       # 录制元数据
├── rec_yyyyy/
│   ├── participant_1.webm  # INDIVIDUAL 录制
│   ├── participant_2.webm
│   └── metadata.json
```

## 9. Operator (弹性集群)

### 9.1 媒体节点生命周期

OpenVidu Pro 的弹性集群管理媒体节点的完整生命周期：

```
         ┌───────────┐
         │ launching │  (Docker 容器启动中)
         └─────┬─────┘
               │
         ┌─────┴─────┐
         │  running  │  (正常运行, 接受媒体会话)
         └─────┬─────┘
               │
         ┌─────┴─────┐
         │  draining │  (不接收新会话, 等待现有会话结束)
         └─────┬─────┘
               │
         ┌─────┴──────┐
         │ terminated │  (容器销毁, 资源释放)
         └────────────┘
```

### 9.2 CPU 驱动自动扩缩容

OpenVidu Pro 的媒体节点扩缩容基于 CPU 负载：

```
                          CPU > 80%
                 ┌──────────────────────┐
                 │                      ▼
          ┌──────────┐            ┌──────────┐
          │  Node A  │            │  Node B  │  (新启动)
          │  65% CPU │            │  0% CPU  │
          └──────────┘            └──────────┘
                 │                      │
                 │                CPU < 60%
                 │                      │
                 │                      ▼
                 │              ┌──────────┐
                 │              │  Node B  │
                 │              │  drain   │  (开始排空)
                 │              └──────────┘
                 │                      │
                 │                      │ 所有会话结束
                 │                      ▼
                 │              ┌──────────┐
                 │              │  Node B  │
                 │              │  term.   │  (销毁)
                 │              └──────────┘
```

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `OPENVIDU_MEDIA_NODE_CPU_USE_THRESHOLD` | 80 | CPU 超过此值触发扩容 (%) |
| `OPENVIDU_MEDIA_NODE_CPU_RESUME_THRESHOLD` | 60 | CPU 低于此值触发缩容 (%) |
| 扩缩容检测间隔 | 15 秒 | 周期性检查节点负载 |
| 最小节点数 | 1 | 保证至少一个节点运行 |
| 最大节点数 | 10 | 防止无限扩容 |

### 9.3 媒体节点注册

媒体节点启动时自动向 openvidu-server 注册：

```json
{
  "id": "node_xxxxx",
  "ip": "10.0.1.100",
  "uri": "ws://10.0.1.100:8888/kurento",
  "status": "running",
  "cpuUsage": 45.2,
  "memoryUsage": 60.1,
  "sessionCount": 12,
  "connectionCount": 48,
  "createdAt": 1720000000000
}
```

## 10. 演进历程

### 10.1 三代架构

```
OpenVidu v1 (2016)       OpenVidu v2 (2018)      OpenVidu v3 (2024-2025)
─────────────────────    ─────────────────────    ────────────────────────
Kurento (MCU+SFU)        Kurento (SFU 为主)       LiveKit + mediasoup
├── Kurento CE            ├── Kurento CE           ├── LiveKit (CE)
├── 无集群                ├── Elastic Stack        ├── MongoDB Analytics
├── 无录制                ├── Docker Compose       ├── Prometheus + Grafana
└── 无 Dashboard          ├── Pro/Enterprise       ├── E2EE
                          └── 弹性集群             └── v2compatibility
```

### 10.2 演进驱动力

**v1 → v2 (Kurento CE 到 Pro/Enterprise)**：
- 市场需求：生产级部署需要弹性扩展和监控
- 商业模式：CE 免费引流, Pro/Enterprise 提供商业价值
- 技术改进：从 MCU 转向纯 SFU 提高性能

**v2 → v3 (Kurento 到 LiveKit + mediasoup)**：
- **性能瓶颈**：Kurento 的 GStreamer 管线 CPU 开销大, 纯 SFU 场景下 mediasoup 可达 2x 容量
- **维护成本**：Kurento 处于维护模式, 社区焦点转移
- **LiveKit 的吸引力**：单二进制 Go 部署 + 内置信令 + 高质量 Telemetry 管线
- **Enterprise 需求**：中大型客户需要更高性能的 SFU 引擎

### 10.3 版本对应关系

| OpenVidu 版本 | 底层引擎 | 部署方式 | 支持状态 |
|--------------|---------|---------|---------|
| v2.29.x (CE) | Kurento | Docker Compose | 维护模式, 安全更新 |
| v2.29.x (Pro) | Kurento | Docker Compose + 弹性集群 | 维护模式 |
| v2.29.x (Enterprise) | Kurento | Docker Compose + HA | 维护模式 |
| v3.x (CE) | LiveKit | Docker Compose | 活跃开发 |
| v3.x (Enterprise) | mediasoup | Docker Compose + K8s | 活跃开发 |

### 10.4 性能对比

| 指标 | v2 (Kurento) | v3 CE (LiveKit) | v3 Enterprise (mediasoup) |
|------|-------------|----------------|--------------------------|
| 单节点流数 | ~100 | ~500 | ~1000+ |
| 连接延迟 | ~1s | ~0.5s | ~0.25s |
| CPU 效率 | 低 (GStreamer 解码+编码) | 中 (Go SFU) | 高 (C++ SFU) |
| 部署镜像 | ~1.5GB (Kurento + GStreamer) | ~50MB (Go 二进制) | ~200MB (mediasoup + deps) |
| 启动时间 | ~30s | ~3s | ~5s |

## 11. E2E 测试

### 11.1 openvidu-loadtest

OpenVidu 提供 `openvidu-loadtest` 工具，用于压力测试和性能基准：

```bash
# 安装
git clone https://github.com/OpenVidu/openvidu-loadtest.git
cd openvidu-loadtest

# 运行压力测试
BROWSER_USER=1 \           # 模拟浏览器用户数
BROWSER_SESSIONS=1 \       # 并发 Session 数
DURATION=60000 \            # 测试持续时间 (ms)
OPENVIDU_URL=https://openvidu.example.com \
OPENVIDU_SECRET=MY_SECRET \
npm start
```

### 11.2 测试指标

| 指标 | 采集方式 | 用途 |
|------|---------|------|
| 连接成功率 | 每个参与者连接状态 | 网络/服务可用性 |
| 连接延迟 | 从 Token 到 ICE 连接完成 | 用户体验 |
| 发布延迟 | 从 publish() 到第一个 RTP 包 | 媒体管道延迟 |
| 订阅延迟 | 从 subscribe() 到第一个视频帧 | 订阅管道延迟 |
| 丢包率 | RTCP RR 统计 | 网络质量 |
| CPU 使用率 | 节点监控 | 扩缩容决策 |
| 内存使用率 | 节点监控 | 容量规划 |

### 11.3 测试拓扑

```
┌────────────────────────────────────────────────────────────────┐
│  openvidu-loadtest 架构                                        │
│                                                                │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│  │  Browser 1   │  │  Browser 2   │  │  Browser N   │         │
│  │  (Puppeteer) │  │  (Puppeteer) │  │  (Puppeteer) │         │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘         │
│         │                 │                 │                  │
│         └─────────────────┼─────────────────┘                  │
│                           │                                    │
│                           ▼                                    │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  openvidu-loadtest (Node.js 协调器)                      │  │
│  │  · 创建/销毁 Session                                    │  │
│  │  · 生成 Token                                           │  │
│  │  · 编排参与者生命周期                                   │  │
│  │  · 收集指标                                             │  │
│  └──────────────────────────────────────────────────────────┘  │
│                           │                                    │
│         ┌─────────────────┼─────────────────┐                  │
│         │                 │                 │                  │
│         ▼                 ▼                 ▼                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│  │ openvidu     │  │ Media Node 1│  │ Media Node 2│         │
│  │ -server      │  │ (Kurento/   │  │ (Kurento/   │         │
│  │              │  │  LiveKit)   │  │  LiveKit)   │         │
│  └──────────────┘  └──────────────┘  └──────────────┘         │
└────────────────────────────────────────────────────────────────┘
```

## 12. 对 AUDEMSP 的参考价值

### [Adopt] 可直接借鉴

1. **Master-Worker 架构**：openvidu-server 的 Java 控制面 + Kurento/LiveKit 媒体面分离模式。AUDEMSP 的 audemsp-server (控制面) + media node (媒体面) 的架构设计可直接参考此模式

2. **Session/Connection/Token 三层抽象**：Session (房间) → Connection (参与者) → Token (认证) 的模型设计简洁且实用。AUDEMSP 的信令层可直接复用此抽象层次

3. **PUBLISHER/SUBSCRIBER 角色模型**：简洁的双角色权限模型——发布者拥有全部权限，订阅者仅可观看。AUDEMSP 的远程桌面和监控场景天然适配此模型（Host=PUBLISHER, Client=SUBSCRIBER）

4. **COMPOSED/INDIVIDUAL 录制模式**：两种录制模式覆盖了绝大多数场景——COMPOSED 用于会议回放，INDIVIDUAL 用于后期编辑。AUDEMSP 的录制模块应直接支持这两种模式

5. **CDR (Call Detail Record)**：通话详情记录是运维和计费的基础设施。AUDEMSP 的 Telemetry 模块应内置 CDR 采集

6. **API 密钥认证 (OPENVIDU_SECRET)**：简单但有效的服务端认证方案。AUDEMSP 的 Server API 认证可直接参考此模式

### [Adapt] 需修改后采用

1. **Token 生成流程**：OpenVidu 的 Token 包含 role (PUBLISHER/SUBSCRIBER) 和自定义数据。AUDEMSP 应扩展为更丰富的访问控制——支持轨道级权限（某个用户只能订阅某路视频流）、时间约束、IP 白名单

2. **弹性集群扩缩容**：CPU 驱动的扩缩容策略是基础方案。AUDEMSP 应增加更多维度——内存使用率、连接数、网络带宽、自定义指标。同时支持基于预测的 proactive 扩容

3. **录制管理器**：OpenVidu 的录制管理器是 Server 的一部分。AUDEMSP 应设计为独立录制服务（类似 LiveKit Egress），避免录制 CPU 负载影响信令响应

4. **Media Node 生命周期**：launching → running → draining → terminated 四阶段模型很好。AUDEMSP 应增加 health check 和 auto-recovery 机制——节点故障时自动迁移会话

5. **v2compatibility 策略**：OpenVidu v3 的 v2compatibility 模块是供应商迁移的参考模版。AUDEMSP 在 API 演进时也应保持至少一个大版本的向后兼容

### [Avoid] 已知坑与不适用场景

1. **Kurento 的 CPU 密集型 MCU 模式**：OpenVidu v2 的 Kurento 方案 CPU 效率低, 部署镜像 ~1.5GB。AUDEMSP 避免 GStreamer 强绑定的 MCU 模式——纯 SFU 是更高效的选择

2. **Java 控制面的高资源消耗**：openvidu-server (Java Spring Boot) 需要 ~512MB-1GB 内存, 启动时间 ~30s。AUDEMSP 的 control plane 应以 Rust 实现（audemsp-server 已有骨架）——零 JVM 开销, 毫秒级启动

3. **LiveKit 私有信令协议**：OpenVidu v3 使用 LiveKit 的私有二进制 WebSocket 信令, 导致标准 WebRTC 客户端无法直接接入。AUDEMSP 应使用标准 SDP offer/answer 模型, 保持互操作性

4. **单点 openvidu-server**：openvidu-server 是单点, 故障时所有会话受影响。AUDEMSP 的 Server 设计应原生支持无状态水平扩展——多个 Server 实例共享会话状态

5. **录制与媒体节点耦合**：OpenVidu 录制在媒体节点上执行, 录制 CPU 负载影响媒体节点性能。AUDEMSP 应设计独立的录制服务, 通过 RTP Forwarding 接收流

6. **过度依赖 Docker Compose**：OpenVidu 的部署几乎完全绑定 Docker Compose。AUDEMSP 应支持多种部署方式——原生二进制、Docker、K8s、systemd 服务

### 核心总结

| 维度 | OpenVidu 经验 | AUDEMSP 应用 |
|------|-------------|-------------|
| 架构 | Master-Worker 控制面/媒体面分离 | audemsp-server + media node |
| 抽象 | Session/Connection/Token 三层 | 信令层直接复用 |
| 角色 | PUBLISHER/SUBSCRIBER | Host=发布者, Client=订阅者 |
| 录制 | COMPOSED/INDIVIDUAL 双模式 | 录制模块直接支持 |
| 集群 | CPU 驱动扩缩容 | 增加多维度 + 预测调度 |
| 迁移 | v2compatibility 模块 | API 向后兼容策略 |
| 监控 | CDR + Prometheus + MongoDB | Telemetry 内置 CDR 采集 |

**总体评分**：★★★★☆ (4/5)

> 评价：OpenVidu 的价值不在于其媒体引擎能力（Kurento 已老化, LiveKit 是第三方, mediasoup 也是第三方）——而在于其 **Master-Worker 平台架构**和 **Session/Connection/Token 抽象模型**。OpenVidu 证明了「在媒体引擎之上封装完整平台 API」的商业和技术可行性。AUDEMSP 的 audemsp-server 应借鉴 OpenVidu 的平台架构设计, 但媒体引擎直接使用 mediasoup (而非 Kurento 或 LiveKit fork), 控制面以 Rust 实现 (而非 Java Spring Boot), 信令使用标准 SDP/JSEP (而非私有协议)。取 OpenVidu 的平台化经验 + mediasoup 的性能 + Rust 的零成本抽象 = AUDEMSP Server 的最佳路径。

---

> **参考来源**
> GitHub: OpenVidu/openvidu (Apache 2.0)
> GitHub: OpenVidu/openvidu-loadtest (压力测试框架)
> 官方文档: docs.openvidu.io
> OpenVidu v3 迁移指南: docs.openvidu.io/en/stable/openvidu-v3/
> OpenVidu Pro 架构: docs.openvidu.io/en/stable/openvidu-pro/
> Kurento/openvidu/kurento-media-server
> LiveKit: livekit/livekit (forked as openvidu-livekit)
> AUDEMSP: docs/research/video-conference.md

---

**相关决策**: D97 (SFU/MCU混合), D138 (mediasoup 选型), D-KURENTO-EVOLUTION