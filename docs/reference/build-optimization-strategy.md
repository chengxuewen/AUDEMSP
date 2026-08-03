# OMSPBase 构建优化策略

**生成**: 2026-08-03 | **来源**: 团队模式分析（4 并行分析师: docker-cache / mirror / prebake / workflow）+ 团队审核修正（fact-check / tech-review / risk-review / consistency-review，4 审核员） | **状态**: 已审核修正，待实施（关联 D208）

> 本文档沉淀"构建容器时间太长"问题的完整分析结论：时间分解、审计发现、方案对比（C1 格式）、执行路线图。所有镜像可达性结论均经 curl 实测验证。

## 1. 背景

omspbase-server 依赖 mediasoup-sys（C++ Worker，meson/ninja 编译，Linux x86_64 only），首次 Docker 构建需 **15-30 分钟**。本地开发（macOS）通过 `docker compose` 构建 server，痛苦集中在：

- 首次构建（新机器 / `docker volume rm` 后）全量编译
- dev 镜像不含预编译依赖，`cargo-cache` 卷首次填充 = 全量编译
- CI 的 gha cache 本地不可达（`type=gha` 只能 GitHub Actions 消费）

## 2. 时间分解（dev 路径）

| 阶段 | 对应层 | 预估耗时 | 占比 |
|------|--------|---------|------|
| apt 安装（tuna 镜像） | Dockerfile L7-12 | 1-2 min | ~7% |
| rustup 安装（tuna 镜像） | Dockerfile L19 | 1-2 min | ~7% |
| pip meson | Dockerfile L26 | ~0.5 min | ~2% |
| COPY . . + cargo fetch | dev L31-32 | 1-3 min | ~8% |
| mediasoup-sys C++ Worker（meson 子项目 + ninja ~100 C++ 文件 + flatbuffers codegen） | 容器内首次编译 | **8-15 min** | **~45%** |
| Rust 依赖编译（~300 crates，axum/tokio/mediasoup-rs 等） | 容器内首次编译 | **5-10 min** | **~35%** |
| 应用 crate 自身 | 同上 | 0.5-1 min | ~3% |

**关键结论**：builder 阶段已把全部依赖烤进镜像层（CI 增量构建仅 1-2 min）；15-30 min 的痛苦全部集中在 (a) dev 容器首次 `cargo run`（空卷全量编译）和 (b) 卷清空之后。CI 的 gha cache 与本地构建完全脱节。

## 3. 审计发现（curl 实测）

### 3.1 配置 bug（P0，镜像层未真正生效）

| # | 问题 | 实测 | 影响 |
|---|------|------|------|
| 1 | `scripts/pixi-init.sh` 的 rsproxy sparse URL 失效：`sparse+https://rsproxy.cn/crates.io-index/` → 404 | rsproxy sparse 协议必须用 `sparse+https://rsproxy.cn/index/`（200）；`/crates.io-index/` 是 **git 协议**地址（未废弃，pixi-init.sh L34 `[registries.rsproxy]` 仍合法），只是不提供 sparse 布局 | cargo 国内镜像实际未生效 |
| 2 | Dockerfile 用 tuna 作 cargo 镜像不合格：tuna 只镜像 index，`.crate` 二进制仍走 static.crates.io | tuna dl 404；static.crates.io 实测 52KB/s；rsproxy/ustc index+二进制全通 | 依赖下载慢 |
| 3 | pixi 无国内镜像配置：`channels = ["conda-forge"]` 直连 conda.anaconda.org | ustc/tuna conda 镜像实测 8.2MB/s | 最慢且唯一未配镜像的层 |

### 3.2 配置 bug（P1，脚本/编排失效）

| # | 问题 | 位置 |
|---|------|------|
| 4 | `docker-cargo.sh` 引用服务名 `dev`，compose 实际只有 `server`/`proxy` → 脚本必失败 | scripts/docker-cargo.sh L10/L13 |
| 5 | 生产 compose 挂 `cargo-cache:/workspace/target` — runtime 镜像无 /workspace，纯 dev compose 复制残留 | docker-compose.yml L25 + L38-39 |
| 6 | devcontainer.json 指向生产 compose（无 build:）→ devcontainer 拉到 runtime 镜像，无工具链 | .devcontainer/devcontainer.json L3 |
| 7 | `mirror.ghproxy.com` 已停运（2024 年项目终止），pixi-install.sh 回退失效 | scripts/pixi-install.sh L85 |

### 3.3 结构性不一致

| 问题 | 影响 |
|------|------|
| builder `--features sfu-mediasoup` 构建（无 `--no-default-features`） | cargo `--features` 是**加性**的：builder 实际 = defaults + sfu-mediasoup = **与 dev 完全一致**（server Cargo.toml L59 default = sfu-mediasoup + admin-dashboard）。无 feature 不一致问题。**真实缺口**：admin **dist 产物**未构建——build.rs L27 在 `www/apps/admin/dist` 缺失时回退 `ADMIN_DIST_DIR=/nonexistent/admin/dist`，编译通过但 /admin 运行时 404（PIT-23） |
| builder dummy src 只覆盖 2/7 crates（common+server） | Phase 2+ server 依赖 media/webrtc/codec 后构建会炸 |
| dev stage `COPY . .` 在 `cargo fetch` 前 | 任意源码变更使 fetch 层失效，每天重复几十次无效下载 |
| Cargo.toml L12-13 `lto=true + codegen-units=1` | **仅影响 release 构建**（builder/CI/生产镜像路径）被拖慢 2-3x；dev 的 debug 路径（`[profile.release]` 对 debug 零影响）不受此配置影响 |
| Dockerfile L19 rustup 未固定版本 + L26 pip meson 未固定 | 工具链升级使 base 层意外失效 |

## 4. 方案对比与推荐

### 4.1 预设容器环境（pre-baked image）— 推荐，最大单点收益

| 方案 | 做法 | 优点 | 缺点 | 推荐 |
|------|------|------|------|:----:|
| A. 只推 dev 镜像 | dev stage 烘焙 target/debug + CI 推送 | 改动最小，覆盖 90% 本地开发 | runtime 构建不受益 | |
| **B. dev + builder 双镜像** | dev 烘焙 debug 依赖；builder 推 registry cache | 本地开发 + 生产构建双覆盖；registry cache 本地可达 | 镜像体积 +2.5-3.5GB；CI 每 commit 多 ~15-25 min | ✅ |
| C. 只烘焙 registry cache | cargo fetch 产物进镜像 | 体积小（~300-400MB） | 只省下载不省编译（编译才是大头） | |
| D. 仅依赖 gha cache | 现状 | 零改动 | 本地不可达，对本地预设零帮助 | |

**来源**：docker/build-push-action + `type=registry` cache 标准实践；OpenVidu 生产/开发 compose 分离模式（同 PIT-32 教训）。

**方案 B 具体改动**：

1. **Dockerfile dev stage 重写**（L28-33）：
   - manifests-first（7 个 crate 全列）+ dummy src 全建
   - `cargo fetch && cargo build --bin omspbase-server`（**debug**，features 与 dev compose command 完全一致：`sfu-mediasoup,admin-dashboard`）
   - 删除 dummy src 后 `COPY . .`（bind mount 覆盖；target/ 被 .dockerignore 排除）
   - mediasoup-sys C++ 产物按 build-script 输入哈希缓存 → **workspace 源码变更不触发 C++ 重编**；仅 mediasoup-sys 版本、其依赖解析或 feature 变更时重跑（~10 min），由 CI 新 sha 镜像兜底（build.rs 无 rerun-if-changed → cargo 按包目录文件跟踪，registry 源码不可变 → 稳态不重跑）
2. **docker-compose.dev.yml**：`build:` → `image: ghcr.io/org/omspbase-server-dev:latest` + `pull_policy: always`（⚠️ 用 `always` 而非 `missing`：missing 是 compose 默认行为，对已存在的 `:latest` 不刷新——CI 每 commit 推新镜像，本地旧镜像会一直用到显式 pull；`always` 或 `newer`（compose 2.30+）才保证拉到最新烘焙产物）
   - ⚠️ **卷刷新语义（H2）**：`cargo-cache:/workspace/target` 命名卷的 copy-on-first-use **只在卷首次创建且为空时**把镜像内 target/debug 灌入。对**已存在的非空卷不触发**——升级路径上烘焙缓存会被静默忽略。落地时必须显式 `docker volume rm omspbase_cargo-cache`（或换新卷名），该命令写进执行步骤而非依赖自动灌入。Docker 只会把镜像内容拷入空卷，此后卷内容与镜像完全独立
   - command 保持与烘焙 feature 集一致，否则缓存失效
3. **新建 `docker-compose.dev.build.yml`**（回退 override，遵守 PIT-32）：
   ```yaml
   services:
     server:
       build: { context: ., target: dev }
   ```
   用法：`docker compose -f docker-compose.dev.yml -f docker-compose.dev.build.yml up -d --build`
4. **docker.yml 推送双镜像**（推送顺序：**dev 先推、runtime 最后**，避免中途失败导致双镜像 feature/依赖漂移）：
   - builder 步骤 `push: true`，tags `ghcr.io/org/omspbase-builder:latest` + `:sha-${{ github.sha }}`，cache-to 加 `type=registry,ref=...,mode=max`
   - runtime 步骤 cache-from 加 registry ref；cache-to 也需加 `type=registry`（否则 runtime 自身新层不进入 registry cache）
   - 新增 dev 推送 step（`target: dev`）
   - PIT-32 gate 加一条：dev.yml 不得含 `build:`
5. **标签/刷新**：`latest`（滚动）+ `sha-${{ github.sha }}`（精确）；每 main push 刷新（沿用现有 trigger）
6. **存储与 CI 配额治理（H4）**：
   - **GHCR 清理 workflow**：sha 标签永不清除 → 每 commit 新增 dev(2.5-3.5GB)+builder+runtime ≈ 5-7GB，免费档（500MB-2GB）几天即爆。新增 cleanup job（`gh api` 删除旧 sha tag，保留最近 N=10）
   - **path-filter**：仅当 `Cargo.lock` / `Cargo.toml` / `Dockerfile` / `crates/**` / `pixi.toml` 变更才跑 dev 镜像推送（大部分 commit 跳过，同时砍出口流量）
   - CI 总时长预算：现 docker.yml ~15-20 min/commit，加 dev bake 后 35-45 min，叠加 ci.yml matrix 需核算免费档 2000 min/月
   - 回退 override（docker-compose.dev.build.yml）可加 `cache_from: [type=registry, ref=ghcr.io/org/omspbase-server-builder:latest]` 让本地 --build 路径也吃到 registry cache
7. **前置条件（H3 — ghcr 可达性）**：方案价值链依赖本地拉取 2.5-3.5GB 预烘焙镜像。本项目环境 GitHub/ghcr 直连不稳（PIT-14/31，daemon 需镜像加速 + 代理兜底）：
   - 实施前实测 ghcr 拉取可达性与速度（daemon 代理已配置的前提下）
   - 私有仓库：本地首次 pull 需 `gh auth token | docker login ghcr.io -u <user> --password-stdin`，镜像可见性 = 仓库可见性
   - 若 ghcr 实测不可达：改用国内可达 registry（阿里 ACR / 腾讯 TCR）承载 dev 镜像与 registry cache ref
8. **平台（M4）**：烘焙镜像在 ubuntu-22.04 runner 构建 = **linux/amd64 only**。Apple Silicon 开发者拉取后走 qemu 仿真（与现状本地 build 的 amd64 仿真无回归，但烘焙收益在仿真下打折）。dev service 显式声明 `platform: linux/amd64`，README 文档化仿真前提
9. **决策承接（M5）**：本方案**承接并扩展 D207**（预构建 dev 镜像），机制从 D207 的 FROM 预构建 base 改为 **compose `image:` + pull**（本地零构建），镜像命名统一为 `omspbase-server-dev` / `omspbase-server-builder`（与现有 prod `omspbase-server` 前缀一致），D207 相应修订（另见 D208）

**时间对比（估算）**：

| 场景 | 现状（本地 build） | 预烘焙（pull） |
|------|------|------|
| 首次 | 15-30 min | pull 2.5-3.5GB（**ghcr 可达性需实测，代理下可能 30min+**）+ 重编 workspace crates 1-3 min |
| 日常增量 | 1-5 min（卷已缓存） | 同左（首次需 `docker volume rm` 让烘焙产物灌入，之后增量） |
| Cargo.lock 变更 | 全量重编 15-30 min | **已有卷不会被新镜像刷新**（copy-on-first-use 仅空卷）→ `docker volume rm` 后 pull 新 sha 镜像 |

**镜像体积估算**：base ~1.2-1.5GB + registry ~300-400MB + target/debug ~800MB-1.2GB ≈ **2.5-3.5GB**。

### 4.2 国内镜像 — 立即修复，成本最低

**pixi/conda**（消灭最慢层，不改 lockfile）——`~/.pixi/config.toml`：

```toml
# pixi 官方 [mirrors] 表 — 拉取时透明替换，lockfile 不变
[mirrors]
"https://conda.anaconda.org/conda-forge" = [
    "https://mirrors.ustc.edu.cn/anaconda/cloud/conda-forge",   # 主, 8.2MB/s（repodata 取首个镜像）
    "https://mirrors.tuna.tsinghua.edu.cn/anaconda/cloud/conda-forge",  # 备
    "https://conda.anaconda.org/conda-forge",  # 原 URL 兜底（官方细则：不显式列出则原频道被完全替换）
]
```

替代（不推荐）：改 pixi.toml channels 为镜像 URL → 需 `pixi update` 重生成 lockfile 并全员提交；镜像为字节级同步副本，sha256 不变，安全但产生锁变更。

**cargo**（rsproxy 主 + ustc 备）——本地与 Dockerfile 统一：

```toml
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"    # 注意: 不是 /crates.io-index/（旧 URL 404）

[source.ustc-sparse]
registry = "sparse+https://mirrors.ustc.edu.cn/crates.io-index/"
```

> cargo `replace-with` 不支持自动回退列表；rsproxy 自身为 CDN 多节点（每分钟同步），ustc 作手动切换备选。

**rustup 本地**：

```bash
export RUSTUP_DIST_SERVER="https://rsproxy.cn"
export RUSTUP_UPDATE_ROOT="https://rsproxy.cn/rustup"
```

**镜像实测矩阵**：

| 层 | 可用镜像（实测） | 不可用（实测） |
|----|----------------|---------------|
| conda | ustc 8.2MB/s、tuna 8.2MB/s | sjtu 403、aliyun 404 |
| cargo | rsproxy（CDN 最快）、ustc 765KB/s、sjtu 242KB/s | **tuna（只镜像 index，二进制 404）** |
| rustup | tuna、ustc、rsproxy | sjtu 301 未达 |

**CI vs 本地边界**：GitHub Actions 跑在 GitHub 基础设施，不受国内网络影响；CI 加速 = gha cache；镜像配置只服务本地/自建 Docker。

### 4.3 定制构建镜像 — 与 4.1 合并实施

- builder 阶段即"定制构建镜像"（manifest-first + dummy build + deps 编译）
- **features 统一已无必要（H1）**：cargo `--features` 是加性的，builder 实际已含 admin-dashboard（defaults = 两 feature 全开），与 dev 一致。**真正的修复**：Docker 构建流程加 `pnpm build:admin` + COPY dist 步骤（见 §7；PIT-23 顺序约束：dist 必须先于 cargo build 构建，编译期嵌入）
- **CI 两步构建合并为单步**（docker.yml L33-41 + L42-51）：`target: runtime, push: true, cache-to: type=gha + type=registry` 一次构建即缓存全部中间层；第二步只是重复 cache-restore
- builder dummy src 泛化：`for d in crates/*/; do mkdir -p $d/src; touch $d/src/lib.rs; done`（幂等）
- 推送 `ghcr.io/org/omspbase-builder:buildcache` 作 registry cache → **本地首建 15-30 min → 分钟级**（本地可达，替代 gha）

### 4.4 其他优化（ROI 排序）

| # | 优化项 | 预估节省 | 工作量 | 风险 |
|---|--------|---------|--------|------|
| 1 | dev stage manifests-first（去掉 COPY . . 前置 fetch） | 每次构建省 30-60s | S | 低 |
| 2 | Docker 资源配额 ≥8GB RAM / 6+ CPU（文档化） | 全量编译 2-3x | S | 无 |
| 3 | warm-dev.sh 后台预热脚本（早晨跑一次） | 首调 15-28 min 后台化 | S | 低 |
| 4 | `lto="thin" + codegen-units=16`（**仅影响 release**：builder/CI/生产镜像路径；dev debug 路径无收益，debug profile 独立） | release 依赖编译 2-3x（代价 1-2% 体积/性能） | S | 低 |
| 5 | sccache 双路（原生 + 容器卷） | 依赖冷变更 8-15 → 2-4 min | M | 中 — daemon 内存 ~1GB；mediasoup C++ 部分收益受限 |
| 6 | mediasoup 预编译 worker（OpenVidu 思路） | 每构建省 4-8 min | M-L | 中 — mediasoup-sys 需 patch；版本/glibc 严格绑定 |
| 7 | BuildKit cache mount（registry + meson OUT_DIR） | fetch 3 min → 5s；meson 重编秒级 | S-M | 低 |
| 8 | .dockerignore 补 `.pixi/ .omo/ .sisyphus/ .codegraph/` | 上下文上传减小 | S | 无 |
| 9 | pixi 环境瘦身：gst-plugins-ugly/gst-libav/llvm 移入 test feature | 环境创建省 1-2 min | S-M | 低 |
| 10 | CI：docker.yml 加 PR 触发 build-only 校验 | 提前发现 Dockerfile 破坏 | S | 低 |

**明确不做/低价值**：
- **跨平台 target 共享不可行**：macOS native（aarch64-apple-darwin）与容器（aarch64/x86_64-unknown-linux-gnu）产物按 triple 分键；mediasoup worker 按 arch+OS 编译；sccache 同样按 compiler+target 分键。唯一跨平台机会 = 预编译 worker 二进制（Linux x64）
- sccache 优先级分析师分歧：docker-cache 认为当前收益低（层缓存已覆盖）建议 P3 暂缓；workflow 排本月。**建议按触发条件上**：Cargo.lock 变更频率上升或 CI 加 feature 矩阵时再引入

## 5. 执行路线图

### 本周（纯修复，全部 <1 天）

1. 修 `pixi-init.sh` rsproxy URL（`/crates.io-index/` → `/index/`）
2. 新增 `~/.pixi/config.toml` `[mirrors]`（ustc 主 + tuna 备 + 原 URL 兜底）
3. Dockerfile cargo 镜像 tuna → rsproxy（index+二进制全通）；**修订 D206**（apt/rustup 清华保留，cargo 换 rsproxy）
4. 修 `docker-cargo.sh` 服务名 `dev` → `server`；修 devcontainer.json 指向 dev compose
5. 删 docker-compose.yml 生产版 cargo-cache volume 误用
6. dev stage manifests-first + `lto="thin"` + `codegen-units=16`（release 限定）+ .dockerignore 补充（含 `.env*`、`*.key` 等密钥类）
7. 修 `pixi-install.sh` ghproxy 回退（ghproxy.com 已停运）→ 换 `gh-proxy.com` 或从 conda 镜像装 pixi
8. warm-dev.sh + 基线测量落档
9. 实测 ghcr.io 可达性（H3 前置条件）；固定 rustup/meson 版本（防 base 层意外失效）

### 本月（结构性）

10. Dockerfile dev stage 烘焙 debug 依赖（4.1-1）
11. CI 推 dev/builder 双镜像（4.1-4）+ 两步构建合并 + registry cache（4.3）+ **GHCR 清理 workflow + path-filter（H4）**
12. compose image 化（`pull_policy: always`）+ `docker-compose.dev.build.yml` 回退 override（含 `cache_from` registry）
13. CI PR 构建校验 job + 构建时长软 gate
14. **admin dist 修复**：Docker 构建流程加 `pnpm build:admin` + COPY dist（PIT-23 顺序约束）

### 下月（按需）

15. sccache（触发条件见 4.4）
16. mediasoup 预编译 worker
17. pixi 环境瘦身、BuildKit cache mount

**预计总收益**：本地首次构建 15-28 min → 2-5 min（预烘焙 + 镜像）；日常增量每轮省 30-60s（manifests-first）；依赖冷变更日 8-15 → 2-4 min（sccache）。

## 6. 测量与回归门禁

**基线方法**（每项优化前后各测一次，记录到 status.md）：

```bash
# per-stage 时间（buildx 每层 DONE x.xs）
docker compose -f docker-compose.dev.yml build --progress=plain server

# 依赖 vs 自身编译拆分
docker compose exec server cargo build --timings -p omspbase-server --features sfu-mediasoup

# 端到端（连续 3 次取中位）
time scripts/docker-cargo.sh check -p omspbase-server --features sfu-mediasoup
```

**门禁**：
- 软 gate：CI 以 `--progress=plain` 构建 builder 并解析 per-stage 时间；>12 min 且 gha 命中 → 告警注释（网络抖动误报率高，不硬失败）
- per-stage 时间打进 build summary/artifact，人工周查
- 规则：任何 Dockerfile/pixi.toml 变更必须附构建时长实测

## 7. 附带发现（非构建优化）

- **prod 镜像功能缺口（H1 修正）**：builder 与 dev 的 feature 集实际一致（均含 admin-dashboard，cargo `--features` 加性叠加）；真实缺口是 admin **dist 产物**未构建——build.rs L27 在 dist 缺失时回退 `ADMIN_DIST_DIR=/nonexistent/admin/dist`，编译通过但 /admin 运行时 404（PIT-23）。修复需在 Docker 构建流程加 `pnpm build:admin`（**必须先于 cargo build**）+ COPY dist 步骤（超出本文档范围，仅记录）
- docs/modules/development/docker-workflow.md 中 `docker volume rm omspbase_cargo-cache` 命令随方案落地后需同步更新

## 参考

- [docker-workflow.md](../modules/development/docker-workflow.md) — Docker 开发工作流
- [ffmpeg-static-build-strategy.md](./ffmpeg-static-build-strategy.md) — 构建策略参考（同类）
- 项目记忆: PIT-11/12/13/19/31/32/33（mediasoup 构建、代理、Docker 教训）
- 决策: D206（cargo 镜像修订）、D207（预构建镜像承接）、D208（构建优化策略实施）
