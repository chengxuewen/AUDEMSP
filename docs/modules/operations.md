# Operations — 运维设计

> 状态：Phase 3 设计 | 整合：D99 + D-OPS-01~10 + D111 | 创建依据：doc-audit CR4

## 日志策略

| 组件 | 格式 | 输出 | Level |
|------|------|------|-------|
| Host | JSON (tracing-subscriber) | stdout + journald | info |
| Server | JSON (tracing-subscriber) | stdout + journald | info |
| Remote | JSON (tracing-subscriber) | stdout | info |

- tracing span: 每个请求/WS连接一个 span，trace_id 贯穿
- 生产日志: 30 天轮转，压缩归档

- Phase 1 裸金属部署: 使用 systemd-journald + tracing-journald crate。日志格式 JSON。Phase 2 迁移到 opentelemetry-otlp → collector。
## 指标 (Prometheus + /metrics endpoint)

| 指标 | 类型 | 说明 |
|------|------|------|
| `omsp_rooms_active` | gauge | 活跃房间数 |
| `omsp_peers_connected` | gauge | 连接对等点数 |
| `omsp_fps_current` | gauge | 当前帧率 |
| `omsp_latency_ms` | histogram | 端到端延迟 |
| `omsp_component_status` | gauge | 组件状态 (0=stopped,1=running,2=degraded) |
| `omsp_cpu_pct` | gauge | CPU 使用率 |
| `omsp_mem_mb` | gauge | 内存使用 |

## 告警 (Alertmanager)

| 告警 | 条件 | Severity |
|------|------|----------|
| ComponentCrashed | omsp_component_status=0 | critical |
| HighLatency | omsp_latency_ms > 200ms (5min) | warning |
| NoPeers | omsp_peers_connected=0 (1min) | warning |
| HighCPU | omsp_cpu_pct > 90 (5min) | warning |

## 告警通知

Phase 1 告警通知渠道：

| 渠道 | 方式 | 优先级 |
|------|------|--------|
| Slack webhook | 直接调用 Incoming Webhook | 主渠道 |
| Email | smtplib (纯文本/HTML) | 备用渠道 |

Phase 2: PagerDuty 集成 (on-call 排班)。

告警路由：
- ComponentCrashed / HighLatency / NoPeers → on-call
- HighCPU / HighMemory → infra team
## TLS

- Phase 1: systemd socket activation + 外部 TLS 终止 (nginx/Caddy)
- Phase 2: 内建 rustls + Let's Encrypt ACME
- 证书轮换: 30 天自动续期

## 备份

- SQLite: `sqlite3 .backup` 每日 + WAL checkpoint
- session_state.json: 文件轮转，保留最近 100 个
- 备份目标: 本地 + 远程 (Phase 2)

## 容量规划

| 部署规模 | Rooms | Peers | CPU | RAM | Disk |
|----------|-------|-------|-----|-----|------|
| 小 | 10 | 20 | 2核 | 2GB | 10GB |
| 中 | 50 | 100 | 4核 | 4GB | 50GB |
| 大 | 200 | 500 | 8核 | 8GB | 200GB |

## 资源预算

Phase 1 资源估算（待 profiling 数据细化）：

| 组件 | CPU | RAM | Disk | 网络上行 |
|------|-----|-----|------|---------|
| Host (720p H.264) | ~2 cores | ~500MB | ~50MB | 2-5 Mbps |
| Host (1080p H.265) | ~3 cores | ~800MB | ~50MB | 1-3 Mbps |
| Remote 解码 | ~1 core | ~300MB | ~20MB | — |
| Server (信令) | ~0.5 core | ~200MB | ~100MB | — |
| GPU 编码器 (NVENC) | — | ~200MB VRAM | — | — |

> Note: 以上为 Phase 1 估算值，待 profiling 数据后细化。

## 网络规划

端口分配：

| 服务 | 端口 | 说明 |
|------|------|------|
| Host 信令 WS | 可配置，默认 8080 | WebSocket 信令 |
| Server relay | 可配置 | 媒体 relay 端口 |
| TURN/STUN | 3478-3480 UDP+TCP | coturn 默认 |
| WebRTC media | 49152-65535 UDP | ephemeral 端口范围 |

带宽参考：
- 720p@30 H.264: ~2-4 Mbps
- 1080p@30 H.265: ~1.5-3 Mbps
- 信令通道: <50 Kbps
- RTCDataChannel 控制: <10 Kbps

## 系统资源限制

systemd unit 模板示例：

```ini
[Service]
MemoryMax=1G
CPUQuota=200%
TasksMax=512
```

### Docker 资源限额 (compose)

> 关联：D208 构建优化 | 创建依据：doc-audit M6

生产 `docker-compose.yml` 为 server 服务声明资源限额（`docker-compose.yml:10-17`）：

| 项 | limits | reservations |
|----|--------|--------------|
| CPU | 2 核 | 1 核 |
| 内存 | 4G | 2G |

**数值依据**（对齐本文件 §容量规划"小"档）：
- **limits 2c/4G**：容器内同时运行 Rust server + mediasoup C++ Worker（RTP 转发、Router 管理），Caddy 反代是独立服务不占此配额。小规模（10 rooms/20 peers）规划为 2核/2GB，4G 上限为 mediasoup Worker 的突发转发（大房间 + 多 transport）留出余量。
- **reservations 1c/2G**：信令基线约 0.5 core/200MB，1c/2G 保证 Worker 启动与常规转发不因调度挤占而饥饿。

**dev 环境无限额（有意为之）**：`docker-compose.dev.yml` 不设 `deploy.resources`。dev 容器承担 `cargo build`（mediasoup-sys meson/ninja C++ 编译，PIT-11/33）等重负载，限额会 OOM 杀死构建进程；开发机资源由 Docker Desktop / 宿主调度兜底。

**⚠️ 生效范围**：`deploy.resources` 仅在 Docker Swarm（`docker stack deploy`）下生效，普通 `docker compose up` 会忽略。当前单机部署若需强制限额，改为容器级字段：

```yaml
services:
  server:
    mem_limit: 4g
    cpus: 2
```

**调整方式**：按 §容量规划档位缩放——中规模（50 rooms/100 peers）建议 limits 4c/8G、reservations 2c/4G；大规模 8c/16G。改动后 `docker compose up -d` 重启，`docker compose config` 验证。

## 日志聚合

### 格式

```json
{"timestamp":"2026-07-29T12:00:00Z","level":"info","msg":"transport connected","peer_id":"test-consumer","room_id":"room-1","transport_id":"t-abc"}
```
- 结构化 JSON，字段：timestamp/level/msg + 业务字段
- 使用 `tracing-subscriber` JSON formatter（Phase 2 已集成）

### 导出

| 阶段 | 方案 | 说明 |
|------|------|------|
| Phase 1 | 文件 + stdout | `tracing-subscriber` JSON 到文件，Docker stdout 到 `docker logs` |
| Phase 2 | OpenTelemetry OTLP | `opentelemetry-otlp` crate，导出到 OTel Collector |
| Phase 3 | Loki/Promtail | 轻量，与现有 Prometheus 栈集成 |

### 保留策略

| 日志类型 | 保留时间 | 说明 |
|----------|---------|------|
| 应用日志 | 30 天 | 含 SFU 操作、信令、错误 |
| 访问日志 | 90 天 | API 请求审计 |
| 告警日志 | 180 天 | 事故追溯 |
| 系统日志 | 7 天 | systemd/journald 自动轮转 |

### 关键日志点

| 事件 | 级别 | 字段 |
|------|------|------|
| transport.connect() | info | peer_id, room_id, transport_id |
| transport.connect() 失败 | error | error_code, reason |
| producer/consumer 创建 | info | kind, producer_id/consumer_id |
| Worker 崩溃 | critical | worker_id, uptime |
| 认证失败 | warn | client_addr, reason |

## Phase 依赖

- Phase 3 Component 框架: /metrics endpoint, tracing span 集成
- Phase 4 Admin Dashboard: 运维面板 (Dashboard, Rooms)
- Phase 2 mediasoup SFU: 媒体质量指标 (丢包率、jitter、RTT)
