# AUDEMSP CLI 构建体系 — `audemsp` 命令

> 计划: 2026-08-10 (v1) | **v2 修订: 2026-08-10（Momus + Oracle 双审核吸收）** | 命名确认: `audemsp`（audemsp.sh/.bat 薄壳 + scripts/audemsp_cli.py）| 状态: **待实施**
> 背景: 统一分散的构建入口（cargo/docker/scripts 5+ 处）；参考 Carla 0.9（Makefile 入口 + 平台分发）、vapkg 类（bootstrap → 自包含 CLI）、pixi [tasks]（跨平台任务）三方对比后定稿。
>
> **v2 修订要点（审核吸收）**:
> - 🔴 **BLOCKER-1（Momus/Oracle 双证）**: `clean` 默认 `docker compose down -v` 会删 cargo-cache 命名卷（docker-compose.dev.yml:26 `cargo-cache:/workspace/target`）→ 默认 `down` 不带 `-v`，卷清理显式化为 `clean --all`
> - 🔴 **BLOCKER-2（Oracle 实测）**: pixi 环境无 pyyaml（`import yaml` ModuleNotFoundError）→ pixi.toml 加 `pyyaml` 依赖，`config validate` 走真 YAML 解析
> - 🟡 **HIGH（Momus）**: pixi.toml `platforms` 无 win-64 → audemsp.bat 激活必失败；Windows 标 **best-effort**（激活失败时给出明确提示 + 文档标注"需先加 win-64 platform"），不加 win-64 任务（本期 Linux 目标）
> - 🟡 **HIGH（Oracle）**: `clean` 必须读 `os.environ.get("CARGO_TARGET_DIR")` 一并处理（全仓实测未设置，默认 target/ 在项目根 OK；用户设置 /tmp/w3c-target 时项目根清理会漏）
> - 🟡 **MEDIUM（Momus）**: `config` 路径钉死（`crates/audemsp-host/config/host.conf` + `config/server.docker.yaml`——仓库有两份 host.conf，schema 不同，必须钉死）
> - 🟢 LOW 合并: 13 子命令（原写 12）、status 规格统一（pixi/cargo/docker/node）、T2 加 chmod +x、logs 改 `docker compose logs -f server`、pixi 检测统一（三处对齐）、ci 命令明确（`cargo fmt --all -- --check` + `cargo clippy -- -D warnings`）、bat 加 `exit /b %errorlevel%`、e2e 前置说明（node/curl/pgrep + server/host/vite 运行中）

## 1. 目标与原则

| 原则 | 说明 |
|------|------|
| 单入口 | `audemsp <cmd>` 全平台统一，vapkg 式子命令 + `-h` |
| 自举 | bootstrap 保证 pixi 就绪 → CLI 即可用（解决首次无 pixi）|
| 零逻辑双写 | CLI 一份 Python 跨平台；薄壳仅 ~10 行平台差异 |
| 复用现有 | 底层调 cargo / docker compose / scripts/*.sh（不动已验证逻辑）|
| 约束合规 | C13（server→Docker）、C22（host→宿主原生）、C20（无硬编码）|

## 2. 总体架构

```
bootstrap.sh / .bat（首次一次性）
   │ ① 装 pixi（如缺）→ ② pixi install → ③ 提示 CLI 就绪
   ▼
audemsp.sh / audemsp.bat（薄壳，仓库内提交）
   │ ① 检测 pixi 缺失 → 提示先 bootstrap（幂等，不自动装）
   │ ② 激活 pixi 环境（source/call pixi-shell，同进程）
   └─▶ python scripts/audemsp_cli.py "$@"   ← 唯一逻辑实现
           │ argparse 子命令分派
           ├─▶ subprocess: cargo / docker compose / scripts/*.sh
```

**要点**：CLI 运行于 pixi 环境内（PATH/LIBCLANG_PATH 已注入），直接调命令，不再包 pixi run。薄壳保证"pixi 存在 + 环境激活"两个前置。

## 3. 文件结构

```
AUDEMSP/
├── audemsp.sh              # Linux/macOS 薄壳（~12 行，chmod +x）
├── audemsp.bat             # Windows 薄壳（~14 行，best-effort）
└── scripts/
    └── audemsp_cli.py      # CLI 唯一实现（argparse，~350 行）
```

不新增 scripts/build/ 子目录——单命令任务（build-host 等）在 CLI 内联 subprocess 一行；仅 e2e 复用现有 `scripts/run-e2e-sfu.sh`（shell 特性必要）。

## 4. CLI 规格（`audemsp_cli.py`）

```
usage: audemsp [-h] {build,build-host,build-server,up,down,logs,e2e,test,ci,clean,config,status,version} ...
```

（**13 个子命令**，v2 修正计数）

| 子命令 | 行为 | 底层命令 | 依赖检查 |
|--------|------|---------|:---:|
| `build` | 全量（host/client 原生 + server Docker）| build-host + build-server 顺序 | cargo, docker |
| `build-host` | 宿主原生构建 host/client（C22）| `cargo build -p audemsp-host -p audemsp-client` | cargo |
| `build-server` | Docker 构建 server | `docker compose -f docker-compose.dev.yml build server` | docker |
| `up` | 启动 server 容器 | `docker compose -f docker-compose.dev.yml up -d server` | docker |
| `down` | 停止容器（**不带 -v**，v2）| `docker compose -f docker-compose.dev.yml down` | docker |
| `logs` | 跟踪 server 日志 | `docker compose -f docker-compose.dev.yml logs -f server`（v2: 不硬编码容器名）| docker |
| `e2e` | e2e_sfu 回归（宿主原生连 Docker server）| `bash scripts/run-e2e-sfu.sh`（Unix）；Win NotSupported | cargo, docker, bash, node, curl |
| `test` | workspace 测试 | `cargo test --workspace --exclude audemsp-server` | cargo |
| `ci` | CI 全链 | `cargo fmt --all -- --check` → `cargo clippy -- -D warnings` → test → e2e（失败即停）| 全 |
| `clean` | 清构建产物（v2: 见下）| `down`（无 -v）+ `rm -rf target` + **CARGO_TARGET_DIR 分支**；`clean --all` 才加 `-v` | docker |
| `config` | `show` / `validate` | 读 `crates/audemsp-host/config/host.conf` + `config/server.docker.yaml`（**v2: 钉死路径**）；validate 用 pyyaml 真解析 | pyyaml |
| `status` | 环境诊断 | 检查 pixi/cargo/docker/node 版本（v2: 统一 node，去掉 pnpm 矛盾）| — |
| `version` | CLI 版本 | 打印 `VERSION = "0.1.0"` 常量（v2: 不读 Cargo.toml——PyInstaller 打包兼容）| — |

**clean 语义（v2 修订）**：
- 默认：`docker compose down`（无 -v，保留 cargo-cache 卷）+ `rm -rf target`（项目根）
- `CARGO_TARGET_DIR` 已设置：一并清理该目录（删除前提示可能多项目共享）
- `clean --all`：才执行 `down -v`（删 cargo-cache 卷）+ `docker builder prune`（显式全清，标注 15-30 分钟重建代价）
- 不碰 `.pixi-cache`（PIXI_CACHE_DIR，包缓存）

**实现细节**：
- argparse `add_subparsers`，每个子命令 `-h`
- 项目根：`pathlib.Path(__file__).resolve().parent.parent`（任意 cwd 可调）
- subprocess 失败：非零退出码透传 + 打印 stderr
- 依赖检查：`shutil.which()`（跨平台）；缺失给修复提示（"先运行 bootstrap / 安装 docker"）
- `status`：`subprocess.run([tool, "--version"], capture_output=True, text=True, timeout=10)`——捕获 stdout+stderr，逐工具 try/except（FileNotFoundError/timeout 标缺失）
- 平台分支：仅三处——路径分隔（pathlib 自动）、e2e 的 bash 调用（`sys.platform == "win32"` 时 NotSupported）、clean 的删除命令
- 常量：`VERSION = "0.1.0"`（PyInstaller 兼容，不依赖 __file__ 读 Cargo.toml）

## 5. 薄壳规格

**audemsp.sh**（Linux/macOS）：
```bash
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
# v2: 检测统一 — 优先 command -v，回退 ~/.pixi/bin（与 _common.sh 对齐: 检测通过即导出 PIXI_BIN）
if command -v pixi >/dev/null 2>&1; then
    export PIXI_BIN="$(command -v pixi)"
elif [ -x "$HOME/.pixi/bin/pixi" ]; then
    export PIXI_BIN="$HOME/.pixi/bin/pixi"
else
    echo "pixi 未安装 — 先运行: source bootstrap.sh" >&2
    exit 1
fi
source "$ROOT/scripts/pixi-shell.sh"        # 激活（同进程，含 PATH/LIBCLANG）
exec python "$ROOT/scripts/audemsp_cli.py" "$@"
```

**audemsp.bat**（Windows，v2: best-effort）：
```bat
@echo off
set "ROOT=%~dp0"
if not exist "%USERPROFILE%\.pixi\bin\pixi.exe" (
    echo pixi 未安装 — 先运行: bootstrap.bat
    exit /b 1
)
REM v2: Windows 为 best-effort — pixi.toml platforms 无 win-64 时激活会失败，提示明确
call "%ROOT%scripts\pixi-shell.bat" || (
    echo 激活失败: pixi.toml platforms 需含 win-64（当前不支持 Windows，见 plan.md）
    exit /b 1
)
python "%ROOT%scripts\audemsp_cli.py" %*
exit /b %errorlevel%     :: v2: 显式传播退出码
```

## 6. bootstrap 修改点

- `bootstrap.sh` / `bootstrap.bat` 末尾追加：
  ```
  AUDEMSP CLI ready: ./audemsp.sh -h        (或 audemsp.bat -h)
  ```
- 不自动装 PyInstaller 产物（源码运行零成本）
- 可选（不在本期）：软链 `~/.local/bin/audemsp`

## 7. pixi 集成

| 项 | 处理 |
|----|------|
| 激活 | 薄壳内 source/call `scripts/pixi-shell.{sh,bat}`（现有，已验证）|
| Python | pixi 环境 3.12.*（pixi.toml 已锁）→ CLI 直接 `python` 可执行 |
| **pyyaml（v2 新增）** | pixi.toml `[dependencies]` 加 `pyyaml = ">=6.0,<7"`（conda-forge，`config validate` 需要）|
| cargo/clang | 环境内 PATH 已注入 → 子命令直接可用 |
| 与 pixi tasks 关系 | 保留 `[tasks] init/shell`（pixi 原生入口）；CLI 是另一条等价路径；注意 CLI `build`（=host+server）与 pixi `build`（workspace 排除 server）**语义不同**，文档标注差异 |

## 8. 平台差异矩阵

| 项 | Unix | Windows | 处理 |
|----|------|---------|------|
| 薄壳 | audemsp.sh | audemsp.bat | 双文件（平台原生，不可避免）|
| CLI | 同一份 | 同一份 | Python 跨平台 |
| 激活 | source | call | 薄壳各自调 |
| pixi.toml platforms | linux-64/osx-* | **无 win-64（v2: best-effort）** | audemsp.bat 激活失败时明确提示 |
| e2e | bash run-e2e-sfu.sh | NotSupported 提示 | `sys.platform` 分支 |
| clean | rm -rf | rmdir /s /q（或提示）| pathlib + 平台分支 |
| 依赖检查 | command -v | where | `shutil.which()` |

## 9. 与现有设施关系

| 现有 | 关系 |
|------|------|
| bootstrap.sh/.bat | 保留；末尾加 CLI 提示 |
| pixi.toml [tasks] | 保留（init/shell）；CLI 并行路径；build 语义差异文档标注 |
| scripts/run-e2e-sfu.sh | CLI `e2e` 直接调用（Unix）；前置说明见下 |
| scripts/pixi-shell.{sh,bat} | 薄壳复用 |
| C13/C22 | CLI 内固化（server→Docker、host→原生）|

**e2e 前置（v2 补充）**：`audemsp e2e -h` 注明——需 server 容器运行中 + host 进程运行中 + vite(5173) 运行中（脚本内端口/pgrep 检查，失败有明确报错）。

## 10. 实施步骤

| 步骤 | 内容 | 验证 |
|------|------|------|
| T1 | pixi.toml 加 `pyyaml` 依赖 + `pixi install` | `python -c "import yaml"` 成功 |
| T2 | `scripts/audemsp_cli.py`（13 子命令 + -h + status/version）| `python scripts/audemsp_cli.py -h` 列 13 子命令 |
| T3 | `audemsp.sh`（chmod +x）+ `audemsp.bat` 薄壳 | `./audemsp.sh version` |
| T4 | bootstrap.sh/.bat 末尾 CLI 提示 | source bootstrap 后提示出现 |
| T5 | `./audemsp.sh status` + `config validate` + `clean`（含 CARGO_TARGET_DIR 分支测试）| 输出正确；clean 默认不删卷 |
| T6 | 提交 + README 或 docs 登记 CLI 用法（含 e2e 前置、Windows best-effort 标注）| 文档可查 |

## 11. 验收标准

| 标准 | 通过条件 |
|------|---------|
| `audemsp -h` | **13** 个子命令列出 |
| `audemsp version` | 输出 0.1.0 |
| `audemsp status` | pixi/cargo/docker/node 版本正确显示 |
| `audemsp config validate` | pyyaml 解析 host.conf + server.docker.yaml 成功（无 pyyaml 时明确报错提示）|
| `audemsp clean` | target 删除 + 容器停止；**cargo-cache 卷保留**（`docker volume ls` 验证）|
| `audemsp clean --all` | 卷 + builder 缓存清理（提示重建代价）|
| `audemsp build-host` | cargo 编译成功（复用宿主构建缓存）|
| `audemsp up` | server 容器启动 |
| `audemsp e2e` | 4/4 通过（Linux，前置环境就绪时）|
| 平台 | audemsp.bat 语法检查 + Windows best-effort 提示（无 win-64 不实测）|

## 12. 风险与缓解（v2 更新）

| 风险 | 缓解 |
|------|------|
| 🔴 **clean 误删 cargo-cache 卷** | 默认 down 无 -v；`clean --all` 显式 + 代价提示 |
| 🔴 **config validate 运行期崩溃（pyyaml 缺失）** | pixi.toml 加 pyyaml（T1 先行）；缺失时 CLI 明确报错而非 traceback |
| 🟡 **Windows 激活失败（platforms 无 win-64）** | best-effort 标注 + 激活失败明确提示；不加 win-64 任务（本期 Linux 目标）|
| 🟡 **CARGO_TARGET_DIR 分支** | clean 读 env 一并处理（多项目共享时提示）|
| Windows 未实测 | .bat 走现有 pixi-shell.bat 路径（Linux 逻辑同构）；CLI 纯 Python 跨平台 |
| 薄壳激活后 python 不在 PATH | pixi-shell 已验证注入；CLI 内 fallback 绝对路径 python |
| CLI 与 scripts 逻辑漂移 | CLI 内联为主；e2e 单一脚本引用点；build 语义与 pixi tasks 差异文档标注 |
| 打包版（未来）__file__ 定位失败 | version 用 VERSION 常量；config 路径接受 root 参数/env 回退（本期不打包）|

## 13. 参考

- Carla 0.9: Makefile + Vars.mk + Linux.mk/Windows.mk + Setup.sh（入口+平台分发模式）
- vapkg 类: bootstrap → 自包含 CLI（子命令 + -h 交互）
- pixi 官方文档: [tasks] 通用 + [target.<platform>.tasks] 平台覆盖（实测验证 0.74.0）
- 审核: Momus（计划质量）+ Oracle（技术核查）2026-08-10
- 本项目: scripts/pixi-shell.{sh,bat}、scripts/run-e2e-sfu.sh、pixi.toml、docker-compose.dev.yml（cargo-cache 卷）
