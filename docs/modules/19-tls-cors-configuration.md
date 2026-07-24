# 19. TLS Reverse Proxy & CORS Configuration

> Phase 1 — 部署参考 | 2026-07-24
> 关联文档: [security-architecture.md](security-architecture.md) §mTLS, [operations.md](operations.md) §TLS, [13-server-architecture.md](13-server-architecture.md) §部署
> 关联 crate: omspbase-server

## 19.1 概述

OMSPBase Phase 1 采用**外部 TLS 终止**策略：服务进程本身只监听 plain HTTP/WS，TLS 由前置的 reverse proxy（nginx/Caddy/Traefik）处理。这遵循 operations.md 的设计决策：「Phase 1: systemd socket activation + 外部 TLS 终止 (nginx/Caddy)」。

本文档提供：
- Reverse proxy 配置示例（nginx / Caddy / Traefik）
- CORS 中间件在 axum 中的正确配置
- WebSocket 代理的注意事项
- 证书管理（自签开发 vs Let's Encrypt 生产）

**当前代码状态**：`tower-http` 的 `cors` feature 已在 Cargo.toml 中声明但**未在任何 .rs 文件中使用**。CORS 中间件需要在 `main.rs` 中显式配置。服务端无 TLS 依赖（无 rustls/openssl），服务监听 plain TCP。

## 19.2 架构概览

```
Client (browser/GUI)
  │
  │ HTTPS (TLS 1.3)  ←── nginx/Caddy/Traefik handles TLS
  ▼
┌─────────────────────────────┐
│  Reverse Proxy              │
│  - TLS termination          │
│  - WebSocket upgrade proxy  │
│  - CORS headers (optional)  │
│  - Rate limiting (optional) │
└───────────┬─────────────────┘
            │ HTTP/1.1 plain (internal)
            ▼
┌─────────────────────────────┐
│  omspbase-server (port 9800)│
│  - axum HTTP + WS           │
│  - CORS via CorsLayer       │
│  - GovernorLayer (rate lim) │
│  - No TLS built-in          │
└─────────────────────────────┘
```

## 19.3 CORS 配置

### 19.3.1 为什么需要 CORS

当 Web UI (React) 从不同 origin 访问 Server API 时，浏览器会发送跨域请求。以下场景需要 CORS：

| 场景 | Origin A | Origin B | 需要 CORS |
|------|----------|----------|-----------|
| 开发环境 | `localhost:5173` (Vite dev) | `localhost:9800` (Server) | ✅ 是 |
| 生产同域 | `app.example.com` | `app.example.com/api` | ❌ 不需要 |
| 生产子域 | `web.example.com` | `api.example.com` | ✅ 是 |
| 嵌入场景 | `aude.example.com` | `omsp.example.com` | ✅ 是 |

### 19.3.2 当前状态

`crates/omspbase-server/Cargo.toml` (line 25):
```toml
tower-http = { version = "0.5", features = ["trace", "cors"] }
```

`cors` feature 已编译但**未被使用**。`crates/omspbase-server/src/main.rs` 的 axum router 仅配置了 `GovernorLayer`：

```rust
// 当前: 无 CORS
let app = Router::new()
    .nest("/", api_router)
    .layer(GovernorLayer {
        config: Box::leak(governor_config),
    });

// 应改为: 添加 CorsLayer
use tower_http::cors::{CorsLayer, AllowOrigin, AllowMethods, AllowHeaders};

let cors = CorsLayer::new()
    .allow_origin(AllowOrigin::exact("http://localhost:5173".parse().unwrap()))
    .allow_methods(AllowMethods::any())
    .allow_headers(vec![
        header::AUTHORIZATION,
        header::CONTENT_TYPE,
    ])
    .allow_credentials(true);

let app = Router::new()
    .nest("/", api_router)
    .layer(cors)
    .layer(GovernorLayer {
        config: Box::leak(governor_config),
    });
```

### 19.3.3 推荐配置

**开发环境** (宽松):
```rust
use tower_http::cors::{CorsLayer, AllowOrigin};

let cors = CorsLayer::permissive(); // 允许所有 origin
```

**生产环境** (严格):
```rust
use tower_http::cors::{CorsLayer, AllowOrigin};
use http::header;
use std::str::FromStr;

fn create_cors_layer(allowed_origins: &[&str]) -> CorsLayer {
    let origins: Vec<HeaderValue> = allowed_origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            http::Method::GET,
            http::Method::POST,
            http::Method::PUT,
            http::Method::DELETE,
            http::Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
        ])
        .allow_credentials(true)
        .max_age(Duration::from_secs(3600)) // preflight cache
}
```

### 19.3.4 CORS 与 Reverse Proxy 的关系

CORS 可以在**两层**处理：

| 层 | 适用性 | 推荐 |
|----|--------|------|
| Reverse proxy (nginx/Caddy) | 简单，外部配置 | 不推荐：WebSocket preflight 复杂，跨域 cookie 困难 |
| Application (axum CorsLayer) | 精确，类型安全 | **推荐**：与 JWT 认证协同，preflight 处理正确 |

**推荐策略**：CORS 在应用层处理（axum），reverse proxy 仅做 TLS 终止 + WebSocket 代理，不添加 CORS 头。双重 CORS 头会导致浏览器报错。

### 19.3.5 Config 化

在 `ServerConfig` 中添加 CORS 配置：

```rust
// crates/omspbase-common/src/config.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    /// 允许的 origins (逗号分隔)
    /// 开发: "*"
    /// 生产: "https://app.example.com,https://admin.example.com"
    #[serde(default = "default_cors_allowed_origins")]
    pub allowed_origins: Vec<String>,

    /// 是否允许 credentials (cookies, Authorization header)
    #[serde(default = "default_true")]
    pub allow_credentials: bool,
}

fn default_cors_allowed_origins() -> Vec<String> {
    vec!["*".to_string()]
}
```

## 19.4 Reverse Proxy 配置

### 19.4.1 端口规划

| 服务 | 内部端口 | 外部端口 | 协议 | 说明 |
|------|---------|---------|------|------|
| Server API | `9800` | `443` (HTTPS) | HTTP/1.1 + WS upgrade | TLS terminated by proxy |
| Host 信令 | `9801` | `9801` (直连) | WS | Host-side, 通常不与浏览器交互 |
| Client UI | `9101` | 不暴露 | HTTP | 本地 GUI |
| TURN/STUN | `3478-3480` | `3478-3480` | UDP+TCP | coturn |
| WebRTC media | `49152-65535` | `49152-65535` | UDP | ephemeral |

### 19.4.2 Nginx

**最小配置** (`/etc/nginx/sites-available/omspbase`):

```nginx
upstream omspbase_server {
    server 127.0.0.1:9800;
    keepalive 64;
}

server {
    listen 443 ssl http2;
    server_name omsp.example.com;

    # TLS
    ssl_certificate     /etc/letsencrypt/live/omsp.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/omsp.example.com/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384;
    ssl_prefer_server_ciphers off;
    ssl_session_cache shared:SSL:10m;
    ssl_session_timeout 1d;

    # Security headers
    add_header Strict-Transport-Security "max-age=63072000" always;
    add_header X-Content-Type-Options nosniff;
    add_header X-Frame-Options DENY;

    # API endpoints (no CORS headers — handled by axum)
    location /api/ {
        proxy_pass http://omspbase_server;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # WebSocket — critical: upgrade headers required
    location /ws {
        proxy_pass http://omspbase_server;
        proxy_http_version 1.1;

        # WebSocket upgrade
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;

        # Long-lived connection settings
        proxy_read_timeout 86400s;
        proxy_send_timeout 86400s;
    }

    # Health check
    location /health {
        proxy_pass http://omspbase_server;
        proxy_http_version 1.1;
        access_log off;
    }

    # Metrics (internal only in production)
    location /metrics {
        proxy_pass http://omspbase_server;
        proxy_http_version 1.1;
        # 生产环境建议 IP 白名单或 basic auth:
        # allow 10.0.0.0/8;
        # deny all;
    }
}

# HTTP → HTTPS redirect
server {
    listen 80;
    server_name omsp.example.com;
    return 301 https://$server_name$request_uri;
}
```

**WebSocket 代理关键点**:
- `Upgrade` 和 `Connection` header 必须传递
- `proxy_read_timeout` 需设置足够长（WS 是长连接，默认 60s 不够）
- Nginx 1.3+ 原生支持 WebSocket proxy

### 19.4.3 Caddy

**最小配置** (`/etc/caddy/Caddyfile`):

```caddyfile
omsp.example.com {
    # TLS automatically managed by Let's Encrypt

    # API
    handle /api/* {
        reverse_proxy localhost:9800 {
            header_up Host {host}
            header_up X-Forwarded-Proto {scheme}
        }
    }

    # WebSocket
    handle /ws {
        reverse_proxy localhost:9800
    }

    # Health
    handle /health {
        reverse_proxy localhost:9800
    }

    # Metrics (internal-only in production)
    handle /metrics {
        reverse_proxy localhost:9800
    }

    # All other requests
    handle {
        reverse_proxy localhost:9800
    }
}
```

Caddy 优势：
- **自动 TLS**：无需手动配置证书，自动从 Let's Encrypt 获取和续期
- **自动 HTTP→HTTPS 重定向**
- **WebSocket 自动检测**：无需特殊配置
- 配置简洁，适合小规模部署

### 19.4.4 Traefik (Docker Compose)

```yaml
# docker-compose.yml
services:
  traefik:
    image: traefik:v3.0
    command:
      - "--providers.docker=true"
      - "--providers.docker.exposedbydefault=false"
      - "--entrypoints.websecure.address=:443"
      - "--certificatesresolvers.letsencrypt.acme.tlschallenge=true"
      - "--certificatesresolvers.letsencrypt.acme.email=admin@example.com"
      - "--certificatesresolvers.letsencrypt.acme.storage=/letsencrypt/acme.json"
    ports:
      - "443:443"
      - "80:80"
    volumes:
      - "/var/run/docker.sock:/var/run/docker.sock:ro"
      - "./letsencrypt:/letsencrypt"

  omspbase-server:
    image: omspbase-server:latest
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.omspbase.rule=Host(`omsp.example.com`)"
      - "traefik.http.routers.omspbase.entrypoints=websecure"
      - "traefik.http.routers.omspbase.tls.certresolver=letsencrypt"
      # WebSocket handled automatically by Traefik 2.x+
      - "traefik.http.services.omspbase.loadbalancer.server.port=9800"
```

## 19.5 证书管理

### 19.5.1 Phase 1: 自签证书（开发/内网）

```bash
# 生成自签 CA
openssl req -x509 -newkey rsa:4096 -days 365 -nodes \
  -keyout ca-key.pem -out ca-cert.pem \
  -subj "/CN=OMSPBase Dev CA"

# 生成服务端证书
openssl req -newkey rsa:4096 -nodes \
  -keyout server-key.pem -out server-req.pem \
  -subj "/CN=omsp.example.com"

# 用 CA 签名
openssl x509 -req -in server-req.pem -days 90 \
  -CA ca-cert.pem -CAkey ca-key.pem -CAcreateserial \
  -out server-cert.pem
```

安全策略：
- 私钥权限 `0600`
- 证书 90 天有效期
- 开发环境可导入 CA 证书到系统信任锚

### 19.5.2 Phase 2: Let's Encrypt (生产)

| 工具 | 集成方式 | 适用场景 |
|------|---------|---------|
| certbot (nginx) | `certbot --nginx` 自动配置 | 裸金属 nginx |
| Caddy 内建 | 自动获取+续期，零配置 | 小规模部署 |
| Traefik ACME | Docker labels 配置 | Docker 部署 |
| rustls-acme (Phase 3) | 内建 ACME 客户端 | OMSPBase 自身处理 |

证书策略：
- 90 天有效期，自动续期（剩余 30 天时触发）
- 使用 `EC secp256r1` (ECDSA P-256) 密钥
- 私钥文件权限 `0600`

## 19.6 Docker Compose 完整示例

```yaml
# docker-compose.prod.yml
version: '3.8'

services:
  # === Reverse Proxy (Caddy) ===
  caddy:
    image: caddy:2
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - caddy_data:/data
      - caddy_config:/config
    restart: unless-stopped

  # === OMSPBase Server ===
  server:
    image: omspbase-server:latest
    environment:
      - OMSP_SERVER_HOST=0.0.0.0
      - OMSP_SERVER_PORT=9800
      - OMSP_JWT_SECRET=${JWT_SECRET}
      - OMSP_CORS_ORIGINS=https://omsp.example.com
      - RUST_LOG=info,omspbase_server=debug
    expose:
      - "9800"
    volumes:
      - ./data:/var/lib/omspbase
      - ./config/server.conf:/etc/omspbase/server.conf:ro
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:9800/health"]
      interval: 30s
      timeout: 5s
      retries: 3

  # === Monitoring ===
  prometheus:
    image: prom/prometheus
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prometheus_data:/prometheus
    expose:
      - "9090"
    restart: unless-stopped

  grafana:
    image: grafana/grafana
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=${GRAFANA_PASSWORD}
    ports:
      - "3000:3000"
    volumes:
      - grafana_data:/var/lib/grafana
    restart: unless-stopped

volumes:
  caddy_data:
  caddy_config:
  prometheus_data:
  grafana_data:
```

### Caddyfile

```caddyfile
omsp.example.com {
    # API routes
    handle /api/* {
        reverse_proxy server:9800 {
            header_up Host {host}
            header_up X-Forwarded-Proto https
        }
    }

    # WebSocket
    handle /ws {
        reverse_proxy server:9800
    }

    # Health & metrics
    handle /health {
        reverse_proxy server:9800
    }

    handle /metrics {
        @internal remote_ip 10.0.0.0/8 172.16.0.0/12 192.168.0.0/16
        handle @internal {
            reverse_proxy server:9800
        }
        respond "Forbidden" 403
    }

    # Catch-all
    handle {
        reverse_proxy server:9800
    }
}
```

## 19.7 Phase 演进

| Phase | TLS 策略 | CORS 策略 | 证书管理 |
|-------|---------|----------|---------|
| Phase 1 | 外部终止 (nginx/Caddy) | axum CorsLayer | 自签/手动 Let's Encrypt |
| Phase 2 | 外部终止 + SFU DTLS | axum CorsLayer + config | certbot/Caddy 自动续期 |
| Phase 3 | 内建 rustls (可选) | axum CorsLayer (保留) | rustls-acme 集成 |
| Phase 4 | mTLS + 内建 rustls | 保留 | 内建 ACME client |

## 19.8 检查清单

部署前验证：

- [ ] TLS 证书有效期中（`openssl s_client -connect host:443`）
- [ ] HTTP → HTTPS 重定向工作
- [ ] WebSocket 连接可升级（浏览器 DevTools → Network → WS）
- [ ] CORS preflight 通过（OPTIONS 请求返回 200）
- [ ] `/health` 端点在 proxy 后可访问
- [ ] `/metrics` 端点在 production 中受保护
- [ ] 安全 header 已设置（HSTS, X-Content-Type-Options）
- [ ] 私钥文件权限为 0600
- [ ] 证书自动续期 cron/systemd timer 已配置

## 19.9 交叉引用

- 认证架构: [security-architecture.md](security-architecture.md)
- 运维设计: [operations.md](operations.md)
- Server 架构: [13-server-architecture.md](13-server-architecture.md)
- 部署模式: [02-deployment-modes.md](02-deployment-modes.md)
- 信令架构: [10-signaling-architecture.md](10-signaling-architecture.md)
