# SDD 05: Host Web Config

## 1. 概述

Host 内嵌 Web 配置页面。通过浏览器本地配置 Host 设备，无需 GUI。

**决策引用**: D81 (内嵌 Web), D84 (SSE 推送), D85 (HTTP Basic Auth), D114 (host.conf)

## 2. 接口定义

```rust
pub struct WebConfig {
    pub bind: SocketAddr,         // 默认 127.0.0.1:9800
    pub username: String,
    pub password: String,         // SHA256 哈希存储
}

pub enum WebRoute {
    #[get("/health")]
    Health,
    #[get("/ready")]
    Ready,
    #[get("/")]                   // 配置页面 (HTML)
    Index,
    #[post("/api/config")]        // 提交配置
    UpdateConfig,
    #[get("/api/status")]         // SSE 实时状态
    StatusStream,
}
```

### Host Config Schema (D114)

```yaml
# /opt/mediaservo/etc/host.conf
host:
  id: "host-001"
signaling:
  ws_url: "ws://server:8080/ws"
media:
  camera: "/dev/video0"
  width: 1280
  height: 720
  fps: 30
  bitrate_kbps: 2000
  encoder: "nvh264enc"
web:
  bind: "127.0.0.1:9800"
  username: "admin"
  password: "changeme"
turn:
  urls: "turn:server:3478"
  username: "user"
  credential: "pass"
```

## 3. 技术栈

| 组件 | 选型 | 说明 |
|------|------|------|
| HTTP 框架 | axum | `include_str!` 嵌入 HTML |
| 前端 | Vanilla JS | ~100KB, 无框架依赖 |
| 实时推送 | SSE | 每秒推送 CPU/GPU/fps/bitrate/rtt |
| 认证 | HTTP Basic Auth | 浏览器原生支持 |
| 配置读写 | serde_yaml | 持久化到 host.conf |

## 4. 安全性

| 防护 | 机制 |
|------|------|
| 绑定地址 | 默认 127.0.0.1 (仅本机) |
| 局域网访问 | 配置 `web.bind: "0.0.0.0:9800"` + 密码 |
| 认证 | HTTP Basic Auth, 密码 SHA256 哈希比对 |
| CSRF | 无状态 API (非 cookie), SameSite 无须担心 |

## 5. SSE 状态推送

```
Event: status
Data: {"fps":29.5,"bitrate":2048,"rtt":45,"gpu_util":67,"camera":"ok","encoder":"ok"}
```

每秒推送, 浏览器配置页实时展示。

## 6. 错误处理

| 条件 | 分类 | 响应 |
|------|------|------|
| 配置格式错误 | Recoverable | 400 Bad Request + 错误字段标注 |
| 认证失败 | Recoverable | 401 Unauthorized |
| 配置写入失败 | Fatal | 500 + 日志记录, 保留旧配置 |
| 必填字段缺失 | Recoverable | 400 + 缺失字段列表 |

## 7. 测试计划

| 测试 | 类型 | 描述 |
|------|------|------|
| config_parse_valid | 单元 | 完整 host.conf 解析为 WebConfig |
| config_parse_invalid | 单元 | 缺失字段/类型错误返回错误 |
| auth_success | 集成 | 正确凭据返回 200 |
| auth_failure | 集成 | 错误凭据返回 401 |
| auth_bypass | 安全 | 无 Authorization header 返回 401 |
| sse_push | 集成 | SSE 连接后每秒收到 status 事件 |
| config_update_persist | 集成 | POST 配置后 host.conf 文件更新 |
| health_ready_endpoints | 集成 | /health /ready 返回 200 |