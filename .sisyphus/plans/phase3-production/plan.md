# Phase 3 — Production Readiness Plan

> **状态**: 计划 | **日期**: 2026-07-24 | **前置**: Phase 2 完成 (75 commits, 187+ decisions)
> **审计基线**: 15 gaps found (5 CRITICAL, 5 HIGH, 5 MEDIUM) — incorporated below

## Executive Summary

Phase 2 完成了核心功能闭环（P2P 管线、mediasoup SFU、Docker/CI、macOS E2E 9/9）。Phase 3 聚焦**生产就绪**——消除审计发现的 15 个生产缺口，让系统可安全部署到真实环境。

**总估算**: ~65 工作日 (约 13 周，2 人并行)
**优先顺序** (per lead): (1) Graceful shutdown + health → (2) TLS/mTLS → (3) Rate limiting + audit → (4) Room lifecycle → (5) Metrics → (6) Config validation → (7) Backup/restore → (8) SFU DTLS

---

## 0. Pre-flight: Quick Fixes (1d)

Quick wins from auditor findings that don't need design:

| Fix | Effort | Detail |
|-----|--------|--------|
| Dockerfile EXPOSE port | 0.25d | `EXPOSE 8000` → `EXPOSE 9800` (matches config default) |
| Room leave on WS close | 0.25d | `handle_socket` already calls `leave_room()`, verify Close frame path |
| process::exit(1) audit | 0.5d | Replace 4x `process::exit(1)` in host/remote with `tracing::error!` + graceful shutdown (see §1) |

---

## 1. Graceful Shutdown & Health Checks (P0 — highest priority)

### 1.1 Current State
- ✅ `CoreError`: is_retryable / is_fatal 分类
- ✅ signaling.rs: WS 断连时 relay_handle.abort() + leave_room()
- ❌ **CRITICAL**: 4x `process::exit(1)` in host/remote — 无 drain, 无日志 flush, 无信令通知对端
- ❌ **CRITICAL**: 无 shutdown ordering (signaling → SFU → metrics → exit)
- ❌ **HIGH**: `/health` 返回 `"OK"` 字面量，不检查依赖
- ❌ `/ready` 仅检查 signaling server 存在性

### 1.2 Implementation

#### Task 1.1: Graceful Shutdown Pipeline (3d)
```
SIGTERM → stop accepting → drain WS (10s) → flush metrics → shutdown SFU → exit
```
- tokio `signal::ctrl_c()` + `axum::serve().with_graceful_shutdown()`
- Component 关闭顺序: Signaling → Relay → SFU → Metrics
- 每个 Component 实现 `async fn shutdown(&self, timeout: Duration) -> Result<()>`
- Drain timeout: 30s (可配置), 超时后 force exit
- 门面: `process::exit(1)` 改为 `tracing::error!(%err, "fatal"); shutdown().await; std::process::exit(1);`

#### Task 1.2: Deep Health Check (2d)
```rust
// /health → { status: "healthy"|"degraded", components: { signaling, sfu, relay } }
```
- SignalingComponent: room_manager 可用性
- SfuComponent: worker process alive + transport count
- RelayComponent: broadcast channel capacity
- `/ready` → k8s readiness (startup 完成)
- `/live` → k8s liveness (进程 alive, 仅 HTTP 200)

#### Task 1.3: Panic Boundary (1d)
- `std::panic::catch_unwind` 包装 WS handler
- `tracing-panic` 自动记录 backtrace
- Panic → 500 error + 不崩进程

#### Task 1.4: Crash-loop Guard (1d)
- ComponentManager: 60s 内重启 > 3 次 → 停止重启 + `component_status = 0`
- 触发 Prometheus 告警 `ComponentCrashed`

### 1.3 Priority: **P0** — 审计 CRITICAL #1

---

## 2. TLS & Transport Security (P0)

### 2.1 Current State
- ✅ PSK HMAC-SHA256 + constant_time_eq
- ❌ **CRITICAL**: 无 TLS — 信令明文传输，PSK 在网络暴露
- ❌ 无 mTLS 客户端证书
- ✅ operations.md: Phase 1 外部 TLS (nginx/Caddy)

### 2.2 Implementation

#### Task 2.1: TLS Reverse Proxy (2d)
- `docs/guides/tls.md`: nginx + Let's Encrypt 完整配置
- Docker Compose: nginx service + certbot auto-renew
- 裸金属: systemd socket activation + nginx
- WSS: nginx proxy WebSocket Upgrade headers

#### Task 2.2: mTLS 可选 (1d, P2)
- nginx `ssl_client_certificate` + `ssl_verify_client optional`
- Server 读取 `X-SSL-Client-Verify` header → 增强认证
- 仅 Enterprise 部署需要，Phase 3 文档即可

#### Task 2.3: Native rustls (2d, P2 — defer to Phase 4)
- `axum-server` + `rustls` + `tokio-rustls`
- Let's Encrypt ACME auto-renew
- 优先级低于外部 TLS（nginx 已覆盖 Phase 3 需求）

### 2.3 Priority: **P0** (Task 2.1), **P2** (Task 2.2-2.3)

---

## 3. Rate Limiting & Audit Logging (P0)

### 3.1 Current State
- ✅ `ServerConfig.rate_limit: u32` 字段 (default 100)
- ❌ **CRITICAL**: 速率限制未实现 — 字段存在但无代码
- ❌ **CRITICAL**: 零审计日志 — operations.md 定义了 audit 消息 schema 但未生成
- ❌ WS 消息无大小上限
- ❌ 无请求超时

### 3.2 Implementation

#### Task 3.1: Rate Limiting (2d)
```rust
// ponytail: tower-governor 实现 GCRA，无需自研
use tower_governor::{GovernorLayer, GovernorConfigBuilder};
```
- 应用于 `/ws` 升级 + `/api/*` 路由
- 429 + `Retry-After` header on limit
- 配置 `burst_size = rate_limit × 2`

#### Task 3.2: Audit Logging (2d)
- Audit event schema (from operations.md):
  ```rust
  struct AuditEvent {
      timestamp: DateTime<Utc>,
      event: AuditEventType, // PeerConnected, RoomCreated, AuthFailed, RateLimited, ConfigReloaded
      peer_id: Option<String>,
      room_id: Option<String>,
      detail: String,
  }
  ```
- `tracing::info!(audit.event = "peer.connected", peer_id = %id, ...)`
- Separate audit log file (or dedicated tracing Layer)
- Log retention: 90 days (configurable)

#### Task 3.3: WS Message Size Limit (0.5d)
- `axum::extract::ws::WebSocketUpgrade::max_message_size(64 * 1024)`
- SDP + ICE candidate < 16KB, 64KB safe margin
- Frame 走 DataChannel, 不经过 WS

#### Task 3.4: Request Timeout (0.5d)
- `tower::timeout::TimeoutLayer::new(Duration::from_secs(30))`
- 应用于所有 HTTP routes

#### Task 3.5: Input Validation (1d)
- `room_id`: `^[a-zA-Z0-9_-]{1,64}$`
- `peer_role`: enum whitelist match
- SignalingMessage: 字段长度 bounds check

### 3.3 Priority: **P0** — 审计 CRITICAL #2

---

## 4. Room Lifecycle & Capacity (P0)

### 4.1 Current State
- ✅ `RoomManager::join_room()` / `leave_room()` + `RoomFull` 错误
- ❌ **HIGH**: 房间内存泄漏 — `DashMap` unbounded, 无 TTL/过期清理
- ❌ **HIGH**: 无容量强制 — `room_capacity` 仅检查单房间 peer 数，非全局
- ❌ `RoomManager` 无 background cleanup task

### 4.2 Implementation

#### Task 4.1: Room TTL & Auto-cleanup (2d)
- `RoomConfig`: `room_ttl_secs: u64` (default 300 = 5min)
- Background task: every 60s scan rooms → 无 peer + 超过 TTL → remove
- Orphan peer detection: peer 心跳超时 30s → force leave

#### Task 4.2: Global Capacity Enforcement (1d)
- `ServerConfig.global_peer_limit: usize` (default 500)
- `RoomManager::join_room()` 检查 `total_peers < global_peer_limit`
- 返回新 error: `CoreError::ServerFull`

#### Task 4.3: Room State Persistence (optional, P2)
- SQLite: rooms table (id, created_at, peer_count, status)
- 仅用于 crash recovery, 不依赖实时一致性

### 4.4 Priority: **P0** — 审计 HIGH #3 (内存泄漏)

---

## 5. Metrics & Monitoring (P0)

### 5.1 Current State
- ✅ `CoreMetrics`: 4 Prometheus 指标 (active_connections, relayed_bytes, signaling_latency_us, error_count)
- ✅ `/metrics` in Prometheus text format
- ❌ **HIGH**: 缺应用级指标 (fps, component_status, cpu, memory)
- ❌ 指标无 label 维度
- ❌ 无 Grafana dashboard
- ❌ 无 Alertmanager 规则

### 5.2 Implementation

#### Task 5.1: Extended Metrics (3d)
- 新增指标 (labeled):
  - `component_status{component="signaling"|"sfu"|"relay"}` gauge
  - `room_peers{room_id}` gauge
  - `fps{room_id,peer_id}` gauge
  - `latency_ms` histogram (p50/p95/p99)
  - `packet_loss_ratio` gauge
  - `cpu_percent`, `mem_mb` (process-level, via `sysinfo` or `/proc`)
- 上报点:
  - SignalingComponent: per-room peer count
  - SfuComponent: transport stats (mediasoup getStats)
  - Media pipeline: fps tick

#### Task 5.2: Grafana Dashboard (1d)
- 4-panel JSON model: Overview | Media Quality | Signaling | Components
- Import via Docker Compose volume mount

#### Task 5.3: Alertmanager Rules (1d)
```yaml
groups:
  - name: omspbase
    rules:
      - alert: ComponentCrashed
        expr: component_status == 0
        for: 0m
        severity: critical
      - alert: HighLatency
        expr: histogram_quantile(0.95, latency_ms) > 200
        for: 5m
        severity: warning
      - alert: NoPeers
        expr: sum(room_peers) == 0
        for: 1m
        severity: warning
      - alert: HighPacketLoss
        expr: packet_loss_ratio > 0.05
        for: 2m
        severity: warning
```
- Alertmanager → Slack webhook (Phase 1)
- Alertmanager → PagerDuty (Phase 2)

### 5.3 Priority: **P0** — 审计 HIGH #5 (resource monitoring)

---

## 6. Configuration Validation (P1)

### 6.1 Current State
- ✅ Serde deserialize + `#[serde(default)]`
- ✅ Round-trip tests
- ❌ **HIGH**: 无跨字段/格式验证 (resolution, port range, framerate > 0)
- ❌ PSK 明文存 YAML
- ❌ 无环境变量覆盖
- ❌ 无 schema 文档

### 6.2 Implementation

#### Task 6.1: ConfigValidate Trait (2d)
```rust
trait ConfigValidate {
    fn validate(&self) -> Result<(), Vec<ConfigError>>;
}
```
- `HostConfig::validate()`: resolution `\d+x\d+`, framerate [1..240], bitrate [100..50000]
- `ServerConfig::validate()`: port [1..65535], room_capacity [1..10000], rate_limit [1..100000]
- Startup: validation fails → exit(1) with formatted errors

#### Task 6.2: Environment Variable Override (1d)
```
OMSPBASE_SERVER__LISTEN__PORT=8080
OMSPBASE_HOST__PSK=xxx
```
- `serde-env` or manual merge with `std::env::var()`
- PSK env var takes precedence over config file

#### Task 6.3: PSK File Support (0.5d)
```yaml
psk_file: "/etc/omspbase/psk.secret"  # preferred
psk: null                               # deprecated, dev only
```

#### Task 6.4: JSON Schema Generation (1d, P2)
- `schemars` derive → `cargo run -- --print-config-schema`

### 6.3 Priority: **P1**

---

## 7. Backup & Restore (P1)

### 7.1 Current State
- ❌ **MEDIUM**: 无 backup/restore — operations.md 有设计但未实现
- Server state: 全部在内存 (HashMap/DashMap), 无持久化
- SQLite users table: 可 sqlite3 .backup 但无自动化

### 7.2 Implementation

#### Task 7.1: SQLite Backup (1d)
- Cron script: `sqlite3 /data/omspbase.db ".backup /backup/omspbase-$(date +%Y%m%d).db"`
- WAL checkpoint before backup
- Retention: 7 daily + 4 weekly

#### Task 7.2: Session State Dump (1d)
- `/admin/api/dump` endpoint (admin-only): export current rooms/peers as JSON
- Phase 1 manual restore from dump file
- Phase 2: auto-snapshot every 5min

#### Task 7.3: Docker Volume Mounts (0.5d)
- `docker-compose.yml`: volumes for `/data`, `/backup`, `/config`
- Document backup/restore procedure in deployment guide

### 7.3 Priority: **P1**

---

## 8. Logging & Observability (P1)

### 8.1 Current State
- ✅ `tracing` crate with `tracing::info!`
- ✅ Audit logging schema designed (Task 3.2)
- ❌ 日志格式非结构化 (纯文本)
- ❌ 无 `trace_id` / `span_id` 贯穿请求
- ❌ 无日志级别热重载
- ❌ 各组件日志不一致

### 8.2 Implementation

#### Task 8.1: Structured JSON Logging (1d)
- `tracing-subscriber` + `fmt::json()` layer
- `LoggingConfig { level, format, audit_file }`
- `format = "json"` (prod) | `"pretty"` (dev)

#### Task 8.2: Trace ID Propagation (1.5d)
- WS 连接 → `tracing::info_span!("connection", trace_id, peer_id, room_id)`
- HTTP 请求 → `tracing::info_span!("request", trace_id, method, path)`
- `uuid::Uuid::new_v4()` as trace_id (Phase 2: OTLP header propagation)

#### Task 8.3: Log Level Hot Reload (1d)
- `tracing_subscriber::reload` + SIGHUP handler
- `kill -HUP <pid>` → reload `RUST_LOG` from config/env

#### Task 8.4: Sensitive Field Masking (0.5d)
- Custom tracing Layer: mask `psk`, `token`, `password` in log output
- Display as `***` (length preserved for debugging)

### 8.3 Priority: **P1**

---

## 9. Performance Baseline (P1)

### 9.1 Current State
- ✅ operations.md resource estimates
- ❌ 无 benchmarks
- ❌ 无 profiling data
- ❌ 无性能目标文档

### 9.2 Implementation

#### Task 9.1: Criterion Benchmarks (3d)
- `bench_encode_h264_720p`, `bench_decode_h264_720p`
- `bench_signal_relay`, `bench_ws_broadcast`, `bench_config_parse`
- CI: `cargo bench --bench production`

#### Task 9.2: Profiling Guide (1d)
- macOS: `cargo instruments`
- Linux: `perf record` + flamegraph
- Memory: `heaptrack` / `dhat`

#### Task 9.3: Performance Targets (0.5d)
| Metric | Target | Tool |
|--------|--------|------|
| H.264 720p encode | <5ms HW, <25ms SW | criterion |
| H.264 720p decode | <3ms HW, <15ms SW | criterion |
| Signaling latency | <50ms p95 | tracing span |
| WS broadcast (100 peers) | <10ms p95 | criterion |
| Server idle memory | <100MB | /metrics |

### 9.3 Priority: **P1**

---

## 10. Documentation (P1)

### 10.1 Current State
- ✅ `docs/architecture.md` (563 lines)
- ✅ 25+ module docs
- ✅ 187+ decisions
- ❌ 无部署指南
- ❌ 无配置参考 (all configurable fields)
- ❌ 无 API 文档
- ❌ 无故障排查指南

### 10.2 Implementation

#### Task 10.1: Deployment Guide (2d)
- Docker Compose (推荐): step-by-step + verification
- Bare metal: systemd services + nginx TLS
- Port/network requirements table
- First-run setup checklist

#### Task 10.2: Configuration Reference (1.5d)
- Per-component: Server / Host / Remote
- Every field: name, type, default, example, description
- Semi-auto from code comments + schemars

#### Task 10.3: API Reference (1d)
- REST: `/health`, `/ready`, `/stats`, `/metrics`
- WebSocket: SignalingMessage types + sequence diagram
- Auth flow: PSK handshake timing
- Error code table (1xxx-9xxx)

#### Task 10.4: Troubleshooting Guide (1d)
- Common issues: connection failed, auth failed, Room Full, ICE timeout
- Log grep patterns
- Metric → alert → resolution mapping

#### Task 10.5: CHANGELOG (0.5d)
- keepachangelog format, starting from v0.1.0

### 10.3 Priority: **P1**

---

## 11. Circuit Breaker & Error Recovery (P1)

### 11.1 Current State
- ✅ `CoreError::is_retryable()` / `is_fatal()`
- ❌ **MEDIUM**: 无 circuit breaker — 下游故障不断重试
- ❌ 无指数退避实现

### 11.2 Implementation

#### Task 11.1: Backoff Utility (1d)
```rust
struct Backoff { initial: Duration, max: Duration, multiplier: f64, jitter: bool }
impl Backoff { fn next_delay(&mut self) -> Duration; fn reset(&mut self); }
```
- Host/Remote WS 重连使用

#### Task 11.2: Circuit Breaker (1.5d)
- `CircuitBreaker` in `omspbase-common`
- States: Closed → (failures > threshold) → Open → (timeout) → HalfOpen → (success) → Closed
- Applied to: SFU transport creation, WS reconnect loop
- Config: `threshold: 5, timeout: 30s`

### 11.3 Priority: **P1**

---

## 12. SFU DTLS (P2 — defer to Phase 4)

### 12.1 Current State
- ✅ mediasoup WebRTC transport creation works
- ✅ Producer/Consumer creation works
- ❌ **MEDIUM**: DTLS `ConnectWebRtcTransport` stubbed (ponytail comment in signaling.rs)
- Impact: DTLS fingerprint conversion between protocol enum and mediasoup types is non-trivial

### 12.2 Deferral Rationale
- DTLS handshake is required for SRTP key exchange — but currently mediasoup accepts the connect
- Full DTLS parameter mapping requires mediasoup-sys type alignment
- Phase 3 scope: document the limitation; Phase 4: implement proper DTLS + test
- Workaround: mediasoup may auto-connect; if not, Phase 4 fix needed before SFU production use

---

## Timeline

```
Week 1-2: Sprint 3A — P0 Critical Fixes (~14d)
├─ §0  Pre-flight fixes              1d
├─ §1  Graceful shutdown + health    7d  ← CRITICAL #1
├─ §2  TLS reverse proxy docs        2d  ← CRITICAL #2
├─ §3  Rate limiting + audit log     6d  ← CRITICAL #3-4
├─ §4  Room lifecycle + capacity     3d
└─ §5  Extended metrics + alerts     5d

Week 3-4: Sprint 3B — P1 Operations (~12d)
├─ §6  Config validation             3.5d
├─ §7  Backup & restore              2.5d
├─ §8  Structured logging            4d
├─ §9  Performance baseline          4.5d
├─ §10 Documentation                 6d
└─ §11 Circuit breaker               2.5d

Week 5+: Sprint 3C — P2 Polish (deferrable)
└─ §12 SFU DTLS                     → Phase 4
```

## Dependencies

```
§1 Shutdown ──→ §4 Room lifecycle (cleanup on shutdown)
§3 Audit log ──→ §8 Logging (audit events use same tracing infra)
§5 Metrics ──→ §1 Health (component_status gauge)
§3 Rate limit ──→ §1 Shutdown (429 needs drain)
§11 Circuit breaker ──→ §9 Performance (breaker triggers need metrics)
```

## Key Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| D188 | `tower-governor` for rate limiting | GCRA algorithm, no self-implementation |
| D189 | External TLS (nginx) Phase 1 | Avoid rustls+ACME complexity, defer to Phase 4 |
| D190 | `psk_file` replaces plaintext `psk` | Security audit compliance |
| D191 | `ConfigValidate` trait + fail-fast | Startup validation prevents silent misconfig |
| D192 | `tracing` JSON for structured logs | Zero extra deps, Docker/Loki compatible |
| D193 | Room TTL auto-cleanup background task | Prevents unbounded DashMap growth (auditor finding) |
| D194 | SFU DTLS deferred to Phase 4 | Non-trivial type mapping, not blocking Phase 3 MVP |
| D195 | SQLite backup cron + WAL checkpoint | Simplest recovery; no pg_dump dependency |

## Risk Register

| Risk | Prob | Impact | Mitigation |
|------|------|--------|------------|
| mediasoup-sys API change | Low | High | Lock v0.22.x; Phase 4 upgrade |
| Rate limit false-positive | Med | Med | Configurable burst_size; default宽松 |
| Room TTL kills active room | Low | High | Heartbeat-based activity check, not idle-time |
| Circuit breaker false-open | Low | Med | Half-open probing after timeout |
| Audit log disk growth | Med | Low | Log rotation (logrotate); 90d retention config |

---

*Plan v2 — 2026-07-24 | 12 sections, 40+ tasks, ~65d | Auditor findings incorporated per lead priority*
