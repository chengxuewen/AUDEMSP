# Upgrade & Migration — 升级与迁移

> 状态：Phase 3 前设计 | 关联决策：D118 (版本兼容), D-OPS-10 (Host 升级) | 创建依据：doc-audit H8

## 版本兼容矩阵 (D118)

```
Host/Server: MAJOR.minor.patch
  - MAJOR 号不同 = 不兼容，必须同步升级
  - minor/patch 可独立滚动

Remote (Client): MAJOR.minor.patch
  - 向后兼容前一个 MAJOR（即 v2 Remote 可连 v1 Server）
  - 当前 MAJOR 与 Server 一致时推荐升级
```

## Crate 版本策略 (doc-audit L7)

> 7-crate workspace 被 Rust 静态链接消费方与 napi 绑定消费方（第三方平台/宿主平台）使用，需独立于二进制矩阵的 crate 级版本策略。

### 分级 API 面

| 层 | crates | 变更语义 |
|----|--------|---------|
| 契约层 | mediaservo-common（protocol/auth/config） | **任何 wire 格式变更 = 所有消费者 breaking** |
| 能力层 | mediaservo-media / mediaservo-webrtc / mediaservo-codec | feature 化能力（见三后端矩阵） |
| 应用层 | mediaservo-host / mediaservo-client / mediaservo-server | 沿用 D118 二进制矩阵 |

### 规则

1. **独立 semver**：7 crate 各自版本，0.1.x 阶段允许 breaking（minor 递增）；进入 1.0 后严格 semver。`version.workspace = true` 仅用于同步发布节奏的 crate（当前 media/codec），不强制全 workspace 同步——mediaservo-common 0.2 不必拖带 server 0.2。
2. **mediaservo-webrtc 三后端 feature 矩阵**（C12）：stub/webrtc-rs/webrtc-sys 属 feature 级兼容；新增后端 = minor，后端行为变更 = major。
3. **MSRV**：rust-toolchain.toml 固定；[规划] CI 加 `cargo-semver-checks` 门禁（PR 检测公共 API breaking）。
4. **消费方锁定**：第三方平台 pin `=x.y.z` 或 commit 引用；宿主平台（napi）由 mediaservo-common 版本决定 ABI 面，发布时同版本记录。
5. **变更通知**：breaking 变更在 decisions.md + CHANGELOG 双记录（[规划] cargo release 或 per-crate changelog 生成）。
## 配置迁移

host.conf 使用 JSON Schema 加 `version` 字段：

```json
{
  "version": 2,
  "video": { "codec": "h265", "bitrate_kbps": 4000 }
}
```

- 启动时读取 `version`，执行迁移链：v1→v2→v3→...
- 每步迁移是纯函数 `fn migrate(config: Value) -> Value`
- serde `#[serde(default)]` 保证新增字段无需手动填充

## 二进制升级 (Host, D-OPS-10)

systemd service 控制：

```
1. systemctl stop mediaservo-host
2. 替换 /usr/local/bin/mediaservo-host
3. systemctl start mediaservo-host
```

Host 启动时检查 `version` 字段，自动执行 schema 迁移。Remote 通过 `mediaservo-server` 推送更新包。

## 容器镜像升级 (Server, Docker)

> 关联：C13 (Server 统一 Docker 构建), D208 (构建优化), PIT-39 (冒烟门禁) | 创建依据：doc-audit M6

Server 是唯一 Docker 部署的组件（C13），镜像由 CI 发布，无手工构建/上传流程。

### 镜像 tag 约定 (docker.yml)

| Tag | 含义 | 更新时机 |
|-----|------|---------|
| `ghcr.io/org/mediaservo-server:latest` | 滚动最新 | 每次 main push |
| `ghcr.io/org/mediaservo-server:sha-<commit>` | 精确版本，可回滚锚点 | 每次 main push |

CI（`.github/workflows/docker.yml`）流程：push main → 构建 runtime stage → 打双 tag → **冒烟门禁**（run 镜像 30s 内 `/health` 返回 200，失败则 push 失败，PIT-39）。

### 升级步骤

```
1. docker compose pull                       # 拉取新 latest
2. docker compose up -d                      # 滚动重建 server（proxy 不变）
3. docker compose ps | grep healthy          # 等待 healthcheck 通过
4. curl -sf http://localhost:9800/health     # 最终确认
```

健康检查门禁：compose healthcheck（30s 间隔/3s 超时/3 次重试，`docker-compose.yml:18-22`）与 Dockerfile HEALTHCHECK 双份存在；容器不健康时 compose 不会自动回滚，需人工介入。

### 回滚

```
docker compose pull                          # 或跳过：直接指定旧 tag
docker compose up -d ghcr.io/org/mediaservo-server:sha-<旧commit>
docker compose up -d                          # 恢复 compose 文件默认 latest
```

保留策略：`sha-` tag 随 main push 累积，建议每 N 个 release 清理旧 tag（ghcr 手动删除）。

### 配置与数据迁移

- 配置：`./config/server.docker.yaml` 挂载只读（`docker-compose.yml:24`），镜像升级不触碰配置；新字段经 serde `#[serde(default)]` 兼容（见上文 §配置迁移）。
- 数据库：sqlx migrate 启动时自动执行（见下节 §数据库迁移）；升级前备份 `mediaservo.db`（operations.md §备份）。
- 版本兼容：遵循 §版本兼容矩阵 (D118)——MAJOR 不同必须同步升级 Host/Remote。

### 与 Host 升级的差异

| 维度 | Host (D-OPS-10) | Server (Docker) |
|------|----------------|-----------------|
| 分发 | Server WS 推送 tarball + SHA256 校验 | CI 推 ghcr 镜像 |
| 执行 | systemctl stop/start | compose pull + up -d |
| 验证 | GET /health → 200 | 冒烟门禁 + compose healthcheck |
| 回滚 | 恢复旧二进制 | 切回 `sha-<commit>` tag |
| 自动回滚 | Phase 2 (60s 健康检查失败) | 未实现，人工回滚 |
## 数据库迁移 (Server)

sqlx migrate 管理 SQLite schema：

```
migrations/
  20260701000001_init.sql
  20260715000002_add_room_config.sql
```

Server 启动时自动执行未应用的迁移。生产环境需先备份 `mediaservo.db`。

## 功能开关迁移

新增功能通过 feature flag 控制灰度：

```toml
[features]
default = ["webrtc"]
webrtc = []
h265 = []        # Phase 2
srttransport = []# Phase 3
```

Phase 切换策略：flag 默认关闭 2 个 release → beta 默认开启 → stable 默认开启并移除 flag。

## 运行时状态迁移

### 零停机部署

#### 蓝绿部署（单实例）

```
当前实例 (blue)                    新实例 (green)
├── 健康检查: OK                   ├── 启动完成
├── 活跃连接: 50 peers             ├── 健康检查: OK
│                                  ├── 开始接收新连接
├── 收到 SIGTERM                   │
├── 停止接受新连接                 ├── 活跃连接: 12 peers
├── Drain: 等待现有连接结束        │
├── 活跃连接: 0 → 进程退出         │
└── blue 实例移除                  └── green 接管全部流量
```

#### Connection Draining

| 阶段 | 时间 | 动作 |
|------|------|------|
| SIGTERM 收到 | 0s | 停止接受新 WS 连接，健康检查返回 503 |
| Drain 中 | 0-30s | 现有连接继续，Host 重新协商到新实例 |
| 超时 | 30s | 强制关闭所有连接，进程退出 |
| 失败回滚 | 60s | 新实例健康检查失败 → systemd 回滚到旧版本 |

#### SFU 状态迁移

| 组件 | 迁移方式 |
|------|---------|
| mediasoup Router | 新实例重新创建（不迁移） |
| 活跃 transport | Drain 期间保持，Host 重新连接到新实例 |
| 生产者/消费者 | 不迁移，Host 重新 produce，浏览器重新 consume |
| 会话状态 | JSON 文件持久化，新实例加载 |

#### Rolling Update（多实例）

```
实例 1: blue → green (drain 30s)
实例 2: blue → green (等实例 1 green 就绪后启动)
实例 3: blue → green (等实例 2 green 就绪后启动)
```

- 并行度：最多 1 个实例同时升级（确保最小容量）
- 健康检查：新实例必须通过 `/health` 后才升级下一个
- 回滚：任一实例健康检查失败 → 停止 rolling，回滚当前实例

### Graceful Drain
- SIGTERM → 停止接受新连接 → 等待现有连接自然结束 (max 30s) → 超时后强制关闭。

### WebRTC 连接
- Drain 期间保持活跃，通过 ICE consent freshness 检测对端存活。


### 自动回滚 (Phase 2)
- 新二进制启动后 60s 内健康检查失败 → systemd 回滚到旧版本。

> 已实施完成（Phase 3）
