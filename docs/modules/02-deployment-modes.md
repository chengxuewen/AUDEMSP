# 部署形态

> MediaServo 支持四种部署形态，适应不同场景需求。

---

## 部署形态总览

| 形态 | 架构 | 适用场景 | 插件数量 |
|------|------|---------|---------|
| **Embed** | Rust crate 静态链接 | 第三方平台嵌入远程桌面 + 遥操作 | ~5 个 |
| **Sidecar** | 容器 + napi-rs 绑定 | 宿主平台企业应用 | ~12 个 |
| **Standalone** | 独立进程 + 完整后端 | 独立部署场景 | 全插件 + Web UI |
| **平台模块** | Docker 容器模块 | 融入宿主平台 | 委托平台认证 |

---

## 一、Embed — Rust crate 静态链接

**目标**：将远程桌面和遥操作能力嵌入第三方平台。

**特点**：
- 仅包含核心插件（屏幕捕获、编码、传输、解码、输入注入）
- 通过 C FFI 暴露给第三方平台
- 无 Web UI，无后台服务
- 资源占用最低

**集成方式**：
```rust
// 第三方平台项目中引用
extern crate mediaservo_core;
use mediaservo_core::remote::RemoteDesktopClient;
```

---

## 二、Sidecar — 容器 + napi-rs 绑定

**目标**：宿主平台企业应用的多媒体扩展。

**特点**：
- 以容器形式部署在宿主平台上
- 通过 napi-rs 提供 Node.js 绑定
- 约 12 个核心插件
- 与宿主平台共享基础设施

**部署**：
```bash
docker run -d --name mediaservo-sidecar mediaservo/sidecar:latest
```

---

## 三、Standalone — 独立进程 + 完整后端

**目标**：完全独立的多媒体服务。

**特点**：
- 完整后端服务（用户管理、权限控制、License、信令）
- Web UI（Tauri v2 桌面应用）
- 全部插件可用
- 自带 SQLite + JWT 认证

**启动**：
```bash
mediaservo-server --config /etc/mediaservo/config.toml
mediaservo-client  # 启动桌面 GUI
```

**Phase 1 运维策略**：
- systemd service 配置 `Restart=always` + `RestartSec=5s`（D155）
- 单进程部署：Host 功能内聚于 mediaservo-host（D155 决策）

---

## 四、平台模块 — Docker 容器

**目标**：作为宿主平台的 Docker 模块运行，类比群晖 Surveillance Station。

**特点**：
- 零硬依赖宿主平台
- 委托平台 RBAC/LDAP 进行用户/权限管理
- 通过 gRPC 与宿主平台通信
- 配置：`auth.mode: "aude"`

**类比**：类似 Jira 安装在群晖上，使用 DSM 的 LDAP 账户。

---

## Docker Phase 2 规划

MVP Phase 1 以单进程 systemd 为主。Phase 2 引入 Docker 化：
- docker-compose 多服务编排（Server + Client + Host 分离）
- 容器健康检查与自动重启
- Kubernetes readiness/liveness probe
