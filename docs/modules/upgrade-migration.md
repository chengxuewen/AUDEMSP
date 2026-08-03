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
1. systemctl stop audemsp-host
2. 替换 /usr/local/bin/audemsp-host
3. systemctl start audemsp-host
```

Host 启动时检查 `version` 字段，自动执行 schema 迁移。Remote 通过 `audemsp-server` 推送更新包。

## 数据库迁移 (Server)

sqlx migrate 管理 SQLite schema：

```
migrations/
  20260701000001_init.sql
  20260715000002_add_room_config.sql
```

Server 启动时自动执行未应用的迁移。生产环境需先备份 `audemsp.db`。

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

> 详见 `.sisyphus/plans/consolidated-mvp/plan.md` Phase 3
