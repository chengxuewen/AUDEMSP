# 视频会议部署参考

## 1. 部署形态

视频会议模块支持四种部署形态，与 MediaServo 总体架构一致：

### 1.1 Embed (Rust crate)

**场景**: AUDESYS 嵌入式场景
**特点**: 静态链接，约 5 个插件，轻量级

```
AUDESYS binary
  └── mediaservo-core
      └── mediaservo-conference (crate)
          └── mediasoup (静态链接)

部署: single binary, no external deps
容量: <50 会话/SFU Worker
```

### 1.2 Sidecar (napi-rs 绑定)

**场景**: AUDEBase 企业应用
**特点**: 容器 + napi-rs 绑定，约 12 个插件

```
AUDEBase Docker
  └── mediaservo-service
      ├── napi-rs binding
      └── mediaservo-conference
          └── mediasoup Worker pool

部署: Docker container, sidecar 模式
容量: <200 会话/节点
```

### 1.3 Standalone

**场景**: 独立部署
**特点**: 完整后端 + Web UI，独立进程

```
mediaservo-server
  ├── Conference Controller (Rust)
  ├── Signaling (WebSocket)
  ├── mediasoup Worker pool
  ├── Recording pipeline
  └── Admin Web UI

部署: bare metal / VM / container
容量: <1000 会话/节点 (视 Worker 数)
```

### 1.4 AUDEBase 模块

**场景**: AUDEBase 平台集成
**特点**: Docker 容器模块，委托平台认证

```
AUDEBase Platform
  └── Container: mediaservo-conference
      ├── 委托 AUDEBase RBAC/LDAP
      └── 独立 SFU Worker 池

部署: AUDEBase 容器编排
容量: 取决于 AUDEBase 资源分配
```

## 2. 配置参考

```yaml
# config.yaml — 视频会议模块配置

conference:
  # 信令
  signaling:
    port: 8000
    tls: true
    cert_path: "/etc/mediaservo/certs/server.pem"
    key_path: "/etc/mediaservo/certs/server.key"
    max_connections: 10000
    rate_limit: 1000  # 每秒最大请求数

  # SFU
  sfu:
    worker_pool:
      min_workers: 2
      max_workers: 16
      auto_scaling: true
      cooldown_sec: 300
    worker:
      log_level: "warn"
      rtc_min_port: 40000
      rtc_max_port: 49999

  # 房间
  room:
    default_max_participants: 16
    max_duration_sec: 86400   # 24h
    inactivity_timeout_sec: 300  # 5min 无参与者自动结束
    max_rooms_per_user: 5

  # 媒体
  media:
    default_video_codec: "vp9"
    default_audio_codec: "opus"
    max_bitrate: 5000       # kbps
    max_resolution: "1080p"
    simulcast: true
    svc: true
    audio:
      fec: true
      dtx: true
      bitrate: 30000        # bps

  # 录制
  recording:
    enabled: true
    storage_path: "/var/mediaservo/recordings"
    format: "mp4"
    max_duration_sec: 43200  # 12h
    retention_days: 90

  # ICE / TURN
  ice:
    stun_servers:
      - "stun:stun.l.google.com:19302"
      - "stun:stun1.l.google.com:19302"
    turn_servers: []
    # turn_servers:
    #   - urls: "turn:turn.example.com:3478"
    #     username: "user"
    #     credential: "pass"

  # 分布式
  distributed:
    enabled: false
    redis_url: "redis://localhost:6379"
    region: "default"
    cluster:
      nodes: []
```

## 3. 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `CONFERENCE_SIGNALING_PORT` | 8000 | 信令端口 |
| `CONFERENCE_SFU_MIN_PORT` | 40000 | RTC 最小端口 |
| `CONFERENCE_SFU_MAX_PORT` | 49999 | RTC 最大端口 |
| `CONFERENCE_REDIS_URL` | - | Redis 连接 |
| `CONFERENCE_REGION` | default | 部署区域 |
| `CONFERENCE_RECORDING_PATH` | /var/mediaservo/recordings | 录制存储路径 |
| `CONFERENCE_RECORDING_ENABLED` | true | 启用录制 |
| `CONFERENCE_DEFAULT_CODEC` | vp9 | 默认视频编码 |
| `CONFERENCE_MAX_BITRATE` | 5000 | 最大码率 (kbps) |
| `CONFERENCE_MAX_ROOMS` | 1000 | 最大房间数 |
| `CONFERENCE_WORKER_MIN` | 2 | 最小 Worker 数 |
| `CONFERENCE_WORKER_MAX` | 16 | 最大 Worker 数 |

## 4. Docker 部署

```dockerfile
# Dockerfile — conference module
FROM ubuntu:24.04 AS base
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

FROM base AS build
RUN apt-get install -y --no-install-recommends \
    build-essential cmake clang libclang-dev \
    && rm -rf /var/lib/apt/lists/*
# ... build mediaservo-conference ...

FROM base
COPY --from=build /opt/mediaservo/bin/mediaservo-conference /usr/local/bin/
COPY config.yaml /etc/mediaservo/config.yaml
EXPOSE 8000 40000-49999
CMD ["mediaservo-conference", "--config", "/etc/mediaservo/config.yaml"]
```

### docker-compose

```yaml
version: "3.8"
services:
  conference:
    image: mediaservo/conference:latest
    ports:
      - "8000:8000"
      - "40000-49999:40000-49999/udp"
    volumes:
      - ./config.yaml:/etc/mediaservo/config.yaml
      - ./recordings:/var/mediaservo/recordings
      - ./certs:/etc/mediaservo/certs
    environment:
      - CONFERENCE_REGION=beijing
      - RUST_LOG=info
    deploy:
      resources:
        limits:
          cpus: "8"
          memory: "8G"

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
```

## 5. 防火墙端口

| 端口 | 协议 | 用途 |
|------|------|------|
| 8000 | TCP | 信令 WebSocket |
| 8001 | TCP | gRPC 服务间通信 |
| 40000-49999 | UDP | WebRTC 媒体 |
| 443 | TCP | 可选 TLS 信令 |
| 3478 | UDP | TURN 服务 |

## 6. 监控

```yaml
# Prometheus 指标端点
metrics:
  port: 9090
  path: "/metrics"

# 关键指标
- conference_rooms_active      # 活跃房间数
- conference_participants_total # 总参与人数
- conference_workers_count      # Worker 数
- conference_sfu_cpu_usage     # SFU CPU 使用率
- conference_bitrate_total     # 总码率 (bps)
- conference_packet_loss       # 丢包率
- conference_connection_errors # 连接错误数
- conference_recording_active  # 录制中数
```

## 7. 容量规划

| 规格 | 小 | 中 | 大 |
|------|-----|-----|-----|
| CPU | 4 核 | 8 核 | 32 核 |
| 内存 | 8 GB | 16 GB | 64 GB |
| Worker 数 | 2 | 6 | 24 |
| 并发房间 | 50 | 200 | 1000 |
| 并发参与者 | 200 | 2000 | 10000 |
| 带宽 | 100 Mbps | 1 Gbps | 10 Gbps |
| 部署形态 | Embed | Sidecar/Standalone | Standalone 集群 |

## 8. 高可用

```
                 ┌──────────┐
                 │  LB      │
                 │ (DNS/GLB)│
                 └────┬─────┘
                       │
         ┌─────────────┼─────────────┐
         │             │             │
         ▼             ▼             ▼
   ┌──────────┐  ┌──────────┐  ┌──────────┐
   │ Node 1   │  │ Node 2   │  │ Node N   │
   │ Beijing  │  │ Beijing  │  │ Shanghai │
   └──────────┘  └──────────┘  └──────────┘
         │             │             │
         └─────────────┼─────────────┘
                       │
                  ┌──────────┐
                  │  Redis   │
                  │  Cluster │
                  └──────────┘
```

- 多节点水平扩展，Redis 共享状态
- 区域就近接入，跨区域级联
- 节点故障自动迁移会议
- 无单点故障