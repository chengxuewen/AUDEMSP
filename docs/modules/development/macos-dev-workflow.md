# macOS Development Workflow

> 状态：Phase 0-1 | 关联：Docker SFU 架构、Native Host/Client

## 架构概览

macOS 开发采用**混合架构**：Host 和 Client 原生运行，Server (SFU) 通过 Docker 运行。

```
┌───────────────────────────────────────────────┐
│                  Docker (Linux)               │
│  ┌─────────────────────────────────────────┐  │
│  │         audemsp-server                 │  │
│  │  ┌──────────┐  ┌────────────────────┐   │  │
│  │  │ Signaling │  │  mediasoup Worker  │   │  │
│  │  │  (WS)    │  │  (RTP relay)       │   │  │
│  │  └────┬─────┘  └────────┬───────────┘   │  │
│  └───────┼────────────────┼────────────────┘  │
│          │ 8000/WS        │ 40000-40100/udp   │
└──────────┼────────────────┼───────────────────┘
           │                │
     ┌─────┴────┐     ┌─────┴────┐
     │  macOS   │     │  macOS   │
     │  Host    │     │  Client  │
     │ (native) │     │ (native) │
     │ :9801    │     │ :9101    │
     └──────────┘     └──────────┘
```

- **Server**：仅 Linux 支持（mediasoup C++ Worker 依赖 Linux epoll + libuv）。macOS 下必须通过 Docker 运行。
- **Host / Client**：纯 Rust，macOS 原生编译运行。Host 负责采集+编码+推流，Client 负责拉流+解码。

## 快速开始

### 前置条件

- macOS 13+ (Intel 或 Apple Silicon)
- Docker Desktop 4.x 或 Colima
- pixi (通过 `curl -fsSL https://pixi.sh/install.sh | bash`)

第一次使用：

```bash
source bootstrap.sh   # 安装 pixi + GStreamer + Rust 工具链
```

之后每次：

```bash
source pixi.sh        # 仅激活 pixi 环境
```

### 三步启动

```bash
# 1. 启动 Docker SFU 服务器
docker compose up -d
# 验证：docker compose logs -f server

# 2. 启动 Host（macOS 原生）
cargo run --bin audemsp-host -- --config config/host.conf

# 3. 启动 Client（macOS 原生，新终端）
source pixi.sh
cargo run --bin audemsp-client -- --config crates/audemsp-client/config/remote.conf
```

## 配置文件

### host.conf

路径：`config/host.conf`（默认 `/opt/audemsp/etc/host.conf`）

```yaml
host:
  id: "host-001"

signaling:
  ws_url: "ws://localhost:9800/ws"   # 指向 Docker 中的 Server

media:
  camera: "/dev/video0"              # macOS 摄像头设备
  width: 1280
  height: 720
  fps: 30
  bitrate_kbps: 2000
  encoder: "auto"                    # 或 "nvh264enc" / "x264enc"
  format: "I420"

# 可选：TURN 服务器
# turn:
#   urls: "turn:192.168.1.100:3478"
#   username: "user"
#   credential: "pass"
```

### remote.conf

路径：`crates/audemsp-client/config/remote.conf`（默认 `/opt/audemsp/etc/remote.conf`）

```yaml
version: 1

server:
  signaling_url: "ws://localhost:9800/ws"

# psk: "audemsp-dev"        # 信号认证预共享密钥（默认值）
```

## 日常开发流程

```
编辑代码 → 编译检查 → 运行测试 → 集成测试
```

### 编辑-编译循环

```bash
# 增量检查（最快）
cargo check --workspace

# 只检查 Host
cargo check --bin audemsp-host

# 完整构建（含 SFU feature，需在 Docker 内）
docker compose exec server cargo build --features sfu-mediasoup --bin audemsp-server
```

### 运行测试

```bash
# 全部测试（排除 SFU，macOS 不支持 mediasoup）
cargo test --workspace

# 仅 Host 测试
cargo test -p audemsp-host

# Docker 内运行 SFU 测试
docker compose exec server cargo test -p audemsp-server --features sfu-mediasoup

# Lint 检查
cargo clippy --workspace --all-targets -- -D warnings

# 格式化检查
cargo fmt --all -- --check
```

### 集成测试流程

```bash
# 1. 重启 Server（应用改动）
docker compose restart server

# 2. 新终端：启动 Host
cargo run --bin audemsp-host -- --config config/host.conf

# 3. 新终端：启动 Client
cargo run --bin audemsp-client -- --config crates/audemsp-client/config/remote.conf

# 4. 查看日志
docker compose logs -f server          # Server 日志
# Host/Client 日志输出到各自终端（JSON 格式）
```

## 端口说明

| 端口 | 进程 | 用途 |
|------|------|------|
| 8000 | Server (Docker) | WebSocket 信令 |
| 9801 | Host (macOS) | metrics HTTP |
| 9101 | Client (macOS) | health/config/metrics HTTP |
| 40000-40100/udp | Server (Docker) | mediasoup RTP/RTCP 媒体流 |
| 9999 | Host (macOS) | 紧急控制 UDP |

macOS 本机端口 8000 由 Docker 端口映射转发到容器内，Host/Client 连接 `ws://localhost:9800/ws` 即可。

## 编译加速

### Cargo 缓存

docker-compose.yml 已配置 `cargo-cache` named volume，避免重复下载：

```bash
# 清理缓存（如遇损坏）
docker volume rm audemsp_cargo-cache
```

### macOS 本机编译

- 使用 `sccache` 加速：
  ```bash
  brew install sccache
  # 在 .cargo/config.toml 添加：
  # [build]
  # rustc-wrapper = "sccache"
  ```
- 使用 `lld` 链接器（已由 pixi 安装）：比默认 ld 快 3-5x

## 故障排查

### Docker 网络

**问题**：Host/Client 无法连接 `ws://localhost:9800`

```bash
# 检查 Server 是否运行
docker compose ps

# 查看 Server 日志
docker compose logs server --tail 20

# 测试端口连通性
curl -v http://localhost:9800/ 2>&1 | head -5

# 如果端口冲突：
sudo lsof -i :9800
# 修改 docker-compose.yml 左侧端口映射，如 "18000:9800"
```

### STUN / NAT 穿透

**问题**：WebRTC ICE 连接失败，日志出现 `ICE connection failed`

```bash
# 1. 确认双方在同网络（本机开发默认 localhost，无需 STUN）
# 2. 跨网络开发：在 host.conf 和 remote.conf 添加 STUN/TURN
```

host.conf 中取消注释：
```yaml
turn:
  urls: "turn:your-turn-server:3478"
  username: "user"
  credential: "pass"
```

### Cargo 缓存损坏

```bash
# macOS 本机
cargo clean

# Docker
docker compose down -v && docker compose up -d
```

### GStreamer 缺失

macOS 下 pixi 自动安装 GStreamer。如遇到 pipeline 初始化失败：

```bash
# 验证 GStreamer 可用
gst-inspect-1.0 --version

# 如未安装，通过 pixi 重装
pixi install
```

Host 在 GStreamer 不可用时会自动降级为 headless 模式（无实际采集输出），专为 E2E 测试设计。

### Apple Silicon vs Intel

pixi.toml 已声明 `osx-64` 和 `osx-arm64` 两个平台，pixi 自动选择匹配的 conda 包。无需手动切换。

## Docker 命令参考

| 命令 | 说明 |
|------|------|
| `docker compose up -d` | 后台启动 Server 容器 |
| `docker compose down` | 停止并移除容器 |
| `docker compose restart server` | 重启 Server（应用代码改动后） |
| `docker compose exec server bash` | 进入容器 shell |
| `docker compose logs -f server` | 实时查看 Server 日志 |
| `docker compose build --no-cache` | 完整重建镜像 |
| `docker compose exec server cargo check --features sfu-mediasoup` | 容器内编译检查 |
| `docker compose exec server cargo test -p audemsp-server --features sfu-mediasoup` | 容器内运行 SFU 测试 |

## 相关文档

- [Docker Development Workflow](docker-workflow.md) — Docker 镜像构建阶段、Dev Container、Linux 原生工作流
- [Architecture](../../architecture.md) — 整体架构设计
