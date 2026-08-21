# 2026-08-21 app-branding-customization — 应用层品牌化定制化改造

**状态**: 计划待审核 | **边界**: SDK bindings 与 wire 协议固化；host/client/server 可定制品牌化
**依赖**: D247（C ABI 符号前缀）、D240（soname/ABI 纪律）、D243（FrameMeta 线格式）、C32（实例隔离三原则）

---

## 1. Proposal

### What

让 MediaServo 被第三方平台作为基石依赖（docs/architecture.md §1.2：静态链接/独立部署嵌入）时，host/client/server 三个应用层可按需定制、品牌化（白标），而不必 fork 重命名。引入统一 **Brand 机制**（编译期 `MEDIASERVO_BRAND` + 运行时同名 env 覆盖，缺省值 = 现状 "mediaservo"），应用层全部用户可见字符串、命名、布局、进程拓扑走 Brand 读取器。

### Why

- 当前 `mediaservo-` 前缀/固定布局散落于 host.rs 内联字符串、translate.rs namespace、cli.py install 布局、host.toml 模板、identity.json 设备前缀——依赖方白标只能 fork 做 D209 式全量重命名（259 文件/1 天成本，且与上游分叉失去演进同步）。
- 矛盾点：bindings（C ABI 符号前缀 D247）**必须固化**（多产品同进程共存）；而此前设计未区分"SDK 契约层固化 / 应用层可定制"——本计划把边界显式化。

### Scope

**In scope**:
- `mediaservo-common::brand` 读取器（env > 编译期 option_env > 默认 "mediaservo"）
- host：帮助文本/版本串、namespace（oxfile app 名 `host-agent` → `<brand>-agent`）、status 过滤（host.rs:408）、systemd unit 名前缀（`oxmgr-<brand>-*`）、identity.json 设备前缀（`ms-` → `<brand>-`，仅新生成 key，不迁移存量）、install 布局（`/opt/<brand>-host`）、默认 psk 注释
- client：GUI 窗口标题/产品名、默认 server_url
- server：admin 面板标题（www/apps/admin）、默认端口、默认 PSK 占位注释
- cli.py：`install host --brand` / `package host --brand` 参数化
- docs/modules 品牌化指南

**Out of scope（固化铁律，本计划零改动）**:
- `bindings/*` 全部：C ABI 符号 `mediaservo_*`（D247）、include/mediaservo/ 头文件布局、cxx/py/node 绑定、soname/ABI 纪律（D240）
- wire 协议：信令 SignalingMessage/SFU/RTP 参数、FrameMeta 线格式（D243）、DeviceStream SDP 帧
- crate 名（`mediaservo-*` workspace member——依赖面标识，需要独立发布时走 fork + D209 式重命名）
- oxmgr 本体（外部工具）与 docs/.agents 保留面

### Layers Affected

| 层 | 位置 | 影响 |
|---|---|---|
| Host 应用 | `crates/mediaservo-host/src/bin/host.rs`、`translate.rs` | 字符串/namespace/unit/设备前缀 → Brand |
| Client 应用 | `crates/mediaservo-client/src/` | 标题/默认 server_url |
| Server 应用 | `crates/mediaservo-server/src/` + `www/apps/admin/` | 面板标题/端口/PSK 占位 |
| SDK bindings | `bindings/*` | 🔒 固化——仅验证门禁 |
| 发布脚本 | `scripts/mediaservo_cli.py` | `--brand` 参数 |

### Risks

1. **设备前缀变更 vs G2 配发链**：brand 化后新实例 identity.json 用 `<brand>-<12hex>`——devices.yaml 已注册 `ms-3d37e51f7703` 保持兼容（默认品牌不变）；品牌化部署需重新配发（文档化 additive——不支持前缀迁移）
2. **unit 名切换残留**：brand 后 auto-stop 枚举前缀变化——老品牌 unit 残留 —— install auto-stop 按新旧双前缀枚举清理（`oxmgr-host-*` + `oxmgr-<brand>-*`）
3. **状态过滤语义**：status（host.rs:408）按当前品牌过滤——老 oxfile（旧 namespace）在品牌化实例下不显示（正确行为，文档化）；不要尝试兼容混读
4. **缺省行为回归**：默认品牌必须零行为变化——用现有测试断言（`grep mediaservo-host` 的字符串断言）作为回归门禁

### Success Criteria

- [ ] 缺省（无 env）：`pixi run cargo test -p mediaservo-host`（~104）全绿 + `install host` 成功 + `git diff bindings/` **为空**（固化验证门）+ `scripts/e2e-install-host.sh`（:54 断言 version 含 mediaservo-host）+ `scripts/e2e-package.sh`（dist 名 + SDK lib/include 名——固化边界）
- [ ] `MEDIASERVO_BRAND=cp` 下：`ps/status/monit` 显示 `cp-agent` 等 app 名、unit `oxmgr-cp-*.service`、identity 新 key `cp-<12hex>`、help 文本含品牌名、install `--prefix /opt/cp-host` 布局完整
- [ ] bindings 四语言（c/cxx/py/node）符号零 diff
- [ ] docs/modules 品牌化指南落盘（checklist：可改清单 vs 固化清单）

### References

- docs/architecture.md §1.2（第三方平台关系：静态链接/独立部署）
- D247/D248（符号前缀 + 手工维护门禁）、D240/D241（soname/ABI）、D243（FrameMeta）、D209（重命名先例成本）

---

## 2. Design

### Architecture

```
env MEDIASERVO_BRAND ──┐
option_env!(编译期)  ───┼──▶ common::brand::media_brand() -> &'static Brand
默认 "mediaservo"    ──┘        │
                                ▼
                     Brand { product, bin_prefix, unit_prefix,
                             device_prefix, default_psk }
                                │
        ┌───────────┬───────────┼───────────┬───────────┐
        ▼           ▼           ▼           ▼           ▼
   host.rs 字符串  translate.rs  cli.py      client     server
   帮助/namespace   app 名/      --brand    标题/      标题/
   /unit/status    oxfile      安装布局    server_url  端口/PSK
```

- 优先级：运行时 env（品牌化部署免重编译）> 编译期 option_env（CI 一键出白标包）> 默认 "mediaservo"（现行为）
- Brand 是 `&'static`（env 首次读取后缓存——进程生命周期内不变，避免并发读 env 竞态）

**默认品牌 → legacy 串映射表（零行为变化门禁的硬约束——禁止按 `<product>-` 直推）**:

| Brand 字段 | 默认 "mediaservo"（legacy 串） | 非默认 brand（如 cp） |
|---|---|---|
| app 名前缀 | `host-`（host-agent/.../host-streamer 7 个，translate.rs:107） | `<brand>-`（cp-agent） |
| unit 前缀 | `oxmgr-host-`（host.rs:1039; cli.py:210） | `oxmgr-<brand>-` |
| 设备前缀 | `ms-`（identity.rs:21——identity.rs:92-94 单测 + tests/identity_cli.rs:28 断言锁死） | `<brand>-`（仅新 key） |
| namespace | `mediaservo-host`（host.rs:408） | `<brand>-host` |
| product 显示名 | `mediaservo-host` | env 值 |

> 即 `product` 字段 ≠ legacy 命名串（默认下 product 是显示名，命名串独立映射）——否则自破缺省零变化门（~104 host 测试 + identity_cli 断言）。

### Files to Touch

| 操作 | 文件 | 目的 |
|---|---|---|
| Create | `crates/mediaservo-common/src/brand.rs` | Brand 结构 + 读取器 + 单测 |
| Modify | `crates/mediaservo-host/src/bin/host.rs` | 帮助文本/版本串/namespace 过滤/unit 名/设备前缀 → Brand |
| Modify | `crates/mediaservo-host/src/translate.rs` | app 名前缀（host-agent → cp-agent）、oxfile namespace |
| Modify | `crates/mediaservo-host/src/identity.rs`（DEVICE_ID_PREFIX）+ bin/host.rs cmd_init | identity.json 设备前缀（新 key 用 brand；存量不迁移） |
| Modify | `scripts/mediaservo_cli.py` | `--brand` 参数、install 布局、单位双前缀枚举、双快捷方式名 |
| Modify | `crates/mediaservo-client/src/` | 窗口标题/默认 server_url |
| Modify | `crates/mediaservo-server/src/` + `www/apps/admin/` | 面板标题/默认端口/PSK 占位 |
| Create | `docs/modules/24-app-branding.md` | 品牌化指南（可改/固化清单） |

### Integration Points

- **Brand 边界**：只影响用户可见层（字符串/命名/布局）——不进入任何 wire 类型、不进入 FrameMeta、不进入符号表
- **host.rs:408 过滤**：`p["namespace"] == brand.app_namespace()`（默认 "mediaservo-host"）
- **install 布局**：`--brand cp` → 默认 prefix `/opt/cp-host`、bin 双快捷 `cp` + `cp-host`（符号链接到 mediaservo-host 二进制——同一二进制多品牌共存）
- **unit 名**：`oxmgr-<brand>-<abs-dir>.service`（host CLI self-managed startup——名前缀可控）

### Error Handling

- env 解析失败（非法 brand 字符串：非 [a-z0-9-]）：`warn!` + 回落默认（不阻断启动——品牌是显示层）
- identity 前缀：新生成 key 用 brand 前缀；已存在 key 原样保留（幂等——不覆盖）——与现有 `host init` 幂等语义一致（C15 相关，失败显式报错）

### Testing Strategy

- 单测：`brand.rs`（env 覆盖优先级、非法值回落、默认值正确性）3-4 个
- 回归门（缺省零行为变化）：`cargo test -p mediaservo-host` 全绿 + `git diff bindings/` 空 + `install host` 成功
- 品牌化验证（手动/脚本）：`MEDIASERVO_BRAND=cp` 启动 → ps/status/monit/unit/identity/help 断言（playwright `vehicle-*` spec 或 shell 断言）
- e2e：默认品牌跑现有 e2e（e2e_sfu/field push/streamer）——品牌化不影响 wire

### Dependencies

无新依赖。纯现有 crate 内改造。

---

## 3. Tasks

## Phase 1: Brand 基础（common）

- [ ] **`common/src/brand.rs` — Brand 结构 + media_brand() 读取器**
  - File: `crates/mediaservo-common/src/brand.rs`（+ lib.rs 导出）
  - 内容: `Brand { product, bin_prefix, unit_prefix, device_prefix, default_psk }` + 优先级（env > option_env > 默认）+ 非法回落 warn
  - Verify: `cargo test -p mediaservo-common brand`（3-4 单测：默认/覆盖/非法）

## Phase 2: host 应用层改造

- [ ] **host.rs 用户字符串 → Brand**
  - File: `crates/mediaservo-host/src/bin/host.rs`
  - 内容: 帮助文本/版本串/`用法:` 前缀、快捷名列表（`host`/`<bin_prefix>-host`）、404 `namespace` 过滤（408 行）
  - Verify: `cargo check -p mediaservo-host` + 默认品牌下 `-h` 输出与现状一致（diff 对照）

- [ ] **translate.rs app 名/namespace → Brand**
  - File: `crates/mediaservo-host/src/translate.rs`
  - 内容: `host-agent` 等 app 生成名前缀、namespace 注入（默认 "mediaservo-host"）
  - Verify: translate 测试（14 个）全绿 + `MEDIASERVO_BRAND=cp` 下 oxfile 生成 `cp-agent`

- [ ] **identity 设备前缀（新 key）**
  - File: `crates/mediaservo-host/src/identity.rs`（DEVICE_ID_PREFIX）+ bin/host.rs cmd_init
  - 内容: 新 key 用 `brand.device_prefix + "-<12hex>"`；已有 key 不迁移（幂etc）
  - Verify: `host init` 幂等（重复生成同 key）+ brand 下新实例 `cp-<12hex>` + 单测

- [ ] **startup unit 名 + auto-stop 双前缀枚举**
  - File: `crates/mediaservo-host/src/bin/host.rs` startup_install + `scripts/mediaservo_cli.py` install auto-stop
  - 内容: unit 名 `oxmgr-<brand>-<abs-dir>.service`；install 枚举 `oxmgr-host-*` + `oxmgr-<brand>-*` 双清
  - Verify: brand 下 startup on → unit 文件存在；install 重装老 unit 被停

- [ ] **cli.py `--brand` 发布参数**
  - File: `scripts/mediaservo_cli.py`
  - 内容: `install host --brand cp` → `/opt/cp-host` 布局 + 双快捷（`cp` + `cp-host` 符号链接）、`package host` 包名
  - Verify: `install host --brand cp --prefix /tmp/cp-test` 全流程 + 布局断言

## Phase 3: client/server 轻量品牌化

- [ ] **client 标题/默认 server_url**
  - File: `crates/mediaservo-client/src/`
  - Verify: `cargo check -p mediaservo-client` + 默认值不变

- [ ] **server admin 标题/端口/PSK 占位**
  - File: `crates/mediaservo-server/src/` + `www/apps/admin/`（标题常量）
  - 机制（C24 约束——admin dist 编译期 include_bytes! 嵌入，**运行时 env 够不到**）: 编译期 `option_env!("MEDIASERVO_BRAND")` → build.rs 注入/vite define；Docker 部署经 build-arg 透传（C13——compose/Dockerfile 加 `ARG MEDIASERVO_BRAND` + build.rs rerun-if-changed）
  - Verify: `pnpm build` + server 重建后 9800 admin 标题含品牌名；缺省不变（默认分支不改）

## Phase 4: 验证回归

- [ ] **缺省零行为回归门**
  - Verify: `cargo test -p mediaservo-host`（~104）+ `install host` 成功 + **`git diff bindings/` 为空**（固化验证）
- [ ] **品牌化手动验证矩阵**
  - Verify: `MEDIASERVO_BRAND=cp ./mediaservo-host ps/status/monit/startup/init` 全断言（app 名/unit/设备前缀/help）
- [ ] **e2e 默认品牌回归**
  - Verify: e2e_sfu 4/4 + field push_e2e 6/6 + streamer_e2e（wire 不受影响）

## Phase 5: 文档与记忆

- [ ] **docs/modules/24-app-branding.md 品牌化指南**
  - 内容: 可改清单（字符串/命名/布局/拓扑）× 固化清单（bindings 符号/wire/FrameMeta/crate 名）；`MEDIASERVO_BRAND` 用法 + 发布参数
- [ ] **记忆沉淀**
  - `.agents/memorys/`: 新决策（D 编号——Brand 机制 + 边界划分）、status 更新