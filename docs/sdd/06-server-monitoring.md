# SDD 06: Server Monitoring

## 1. 概述

Server 监控与可观测性模块。提供健康检查、Prometheus 指标、告警规则。

**决策引用**: D86-D91 (Server 架构), D99 (tracing + prometheus), D-OPS-09 (告警规则), D111 (健康检查)

## 2. 接口定义

```rust
/// Prometheus 指标注册
pub struct ServerMetrics {
    // HTTP
    pub http_requests_total: Counter,
    pub http_request_duration_ms: Histogram,
    // WebSocket
    pub ws_connections: Gauge,
    pub ws_messages_total: Counter,
    // Relay
    pub relay_active_sessions: Gauge,
    pub relay_bitrate_kbps: Gauge,
    pub relay_rtt_ms: Gauge,
    pub relay_packets_lost: Counter,
    // 系统
    pub session_duration_seconds: Histogram,
    pub process_rss_mb: Gauge,
}

/// 健康检查
pub struct HealthResponse {
    pub status: String,       // "ok"
}

pub struct ReadyResponse {
    pub ready: bool,
    pub checks: HashMap<String, String>,  // component → status
}
```

### 端点

| 端点 | 方法 | 用途 |
|------|------|------|
| /health | GET | K8s livenessProbe, 轻量存活检查 |
| /ready  | GET | K8s readinessProbe, 全组件状态 |
| /metrics | GET | Prometheus 抓取 |

## 3. 指标设计 (D99)

```
# HELP mediaservo_http_requests_total HTTP 请求总数
# TYPE mediaservo_http_requests_total counter
mediaservo_http_requests_total{method="GET",path="/health"} 1024

# HELP mediaservo_ws_connections WebSocket 连接数
# TYPE mediaservo_ws_connections gauge
mediaservo_ws_connections 2

# HELP mediaservo_relay_rtt_ms 中继 RTT 毫秒
# TYPE mediaservo_relay_rtt_ms gauge
mediaservo_relay_rtt_ms{peer="host-001"} 45
mediaservo_relay_rtt_ms{peer="remote-001"} 52
```

## 4. 告警规则 (D-OPS-09)

| 告警名 | 条件 | 级别 |
|--------|------|------|
| push_rtt_high | rtt_ms > 500 for 30s | warning |
| push_rtt_critical | rtt_ms > 1000 for 10s | critical |
| camera_lost | fps == 0 for 5s | critical |
| encoder_lag | encode_queue_depth > 5 for 10s | warning |
| gpu_mem_high | gpu_mem_pct > 90 for 30s | warning |
| signaling_lost | ws_connected == 0 for > 30s | critical |
| host_restart_loop | restart count > 5 in 5min | critical |
| frame_dropped_rate | control_frames_dropped / total > 0.1 | warning |

## 5. 日志 (D99)

- 框架: tracing + tracing-subscriber
- 格式: JSON stdout, Docker logs 收集
- traceId: axum TraceLayer 自动注入
- 轮转: Docker json-file (max-size: 10m, max-file: 3)

## 6. 测试计划

| 测试 | 类型 | 描述 |
|------|------|------|
| metric_accuracy | 集成 | HTTP 请求后 counter 递增 |
| metric_persistence | 集成 | /metrics 返回 Prometheus 格式文本 |
| health_endpoint | 集成 | /health 返回 200 {"status":"ok"} |
| ready_all_green | 集成 | 所有组件正常时 /ready 返回 ready:true |
| ready_partial_fail | 集成 | 某组件异常时 /ready 返回 ready:false |
| alert_firing | 集成 | 模拟高 RTT 触发 push_rtt_high 告警 |
| alert_resolve | 集成 | RTT 恢复后告警自动解决 |
| ws_connection_gauge | 集成 | WS 连接数随客户端增减变化 |
| tracing_json_format | 单元 | 日志输出为合法 JSON 行 |