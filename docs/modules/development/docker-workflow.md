# Docker Development Workflow

> 状态：Phase 3 完成 | 关联：SFU mediasoup 集成、D208 构建优化

## 概述

MediaServo 使用多阶段 Dockerfile (base → dev → builder → runtime) 和 docker-compose 简化开发环境搭建。mediasoup SFU 仅支持 Linux，macOS 开发者需通过 Docker 容器编译。

## macOS Docker 工作流

### 前置条件

- Docker Desktop 4.x 或 Colima
- VS Code + Dev Containers 扩展（可选）

### 使用 docker-compose

```bash
# 启动开发容器（挂载源码 + cargo 缓存）
docker compose up -d

# 进入容器
docker compose exec server bash

# 容器内编译（含 mediasoup SFU）
cargo check --features sfu-mediasoup

# 运行服务器
cargo run --bin mediaservo-server
```

### 使用 VS Code Dev Container

1. 安装 `Dev Containers` 扩展
2. 打开项目根目录
3. `Ctrl+Shift+P` → `Dev Containers: Reopen in Container`
4. rust-analyzer 自动识别 workspace，启用 `sfu-mediasoup` feature

### 编译加速

docker-compose.yml 已配置 `cargo-cache` named volume，避免重复下载依赖：

```bash
# 清理 cargo 缓存（如遇缓存损坏）
docker volume rm mediaservo_cargo-cache
```

## Linux 原生工作流

Linux 可直接本地编译，无需 Docker（mediasoup 原生支持）：

```bash
# 安装依赖（Debian/Ubuntu）
sudo apt-get install -y \
  pkg-config cmake ninja-build \
  libssl-dev libuv1-dev \
  python3 python3-pip

# 安装 meson（mediasoup Worker 编译需要）
pip3 install meson

# 拉取依赖
cargo fetch

# 编译含 SFU 的服务器
cargo build --features sfu-mediasoup --bin mediaservo-server

# 运行
cargo run --features sfu-mediasoup --bin mediaservo-server
```

### clippy 检查

```bash
cargo clippy --features sfu-mediasoup --all-targets -- -D warnings
```

## Docker 命令参考

| 命令 | 说明 |
|------|------|
| `docker compose up -d` | 后台启动开发容器 |
| `docker compose down` | 停止并移除容器 |
| `docker compose exec server bash` | 进入运行中容器 |
| `docker compose logs -f server` | 查看服务器日志 |
| `docker compose build --no-cache` | 重新构建镜像 |
| `docker compose run --rm server cargo test` | 一次性运行测试 |
| `docker compose run --rm server cargo clippy --features sfu-mediasoup` | 容器内 clippy |

## Dockerfile 构建阶段

| 阶段 | 用途 | 基础镜像 |
|------|------|----------|
| `base` | Rust + 系统依赖 + meson | ubuntu:22.04 |
| `dev` | 开发环境（源码 + cargo fetch） | base |
| `builder` | release 编译 | base |
| `runtime` | 最小运行时 | ubuntu:22.04 |

## 端口映射

| 端口 | 用途 |
|------|------|
| 8000 | HTTP/信令 |
| 40000-40100/udp | mediasoup RTP/RTCP |

> 详见 [SFU mediasoup Integration](../sfu-mediasoup-integration.md)
