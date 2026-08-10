# AUDEMSP CLI 构建体系 — `audemsp` 命令

> 计划: 2026-08-10 | 命名确认: `audemsp`（audemsp.sh/.bat 薄壳 + scripts/audemsp_cli.py）| 状态: **待实施**
> 背景: 统一分散的构建入口（cargo/docker/scripts 5+ 处）；参考 Carla 0.9（Makefile 入口 + 平台分发）、vapkg 类（bootstrap → 自包含 CLI）、pixi [tasks]（跨平台任务）三方对比后定稿。

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
           ├─▶ subprocess: cargo / docker compose / pnpm / scripts/*.sh
```

**要点**：CLI 运行于 pixi 环境内（PATH/LIBCLANG_PATH 已注入），直接调命令，不再包 pixi run。薄壳保证"pixi 存在 + 环境激活"两个前置。

## 3. 文件结构

```
AUDEMSP/
├── audemsp.sh              # Linux/macOS 薄壳（~10 行，chmod +x）
├── audemsp.bat             # Windows 薄壳（~10 行）
└── scripts/
    └── audemsp_cli.py      # CLI 唯一实现（argparse，~300 行）
```

不新增 scripts/build/ 子目录——单命令任务（build-host 等）在 CLI 内联 subprocess 一行；仅 e2e 复用现有 `scripts/run-e2e-sfu.sh`（shell 特性必要）。

## 4. CLI 规格（`audemsp_cli.py`）

```
usage: audemsp [-h] {build,build-host,build-server,up,down,logs,e2e,test,ci,clean,config,status,version} ...
```

| 子命令 | 行为 | 底层命令 | 依赖检查 |
|--------|------|---------|:---:|
| `build` | 全量（host/client 原生 + server Docker）| build-host + build-server 顺序 | cargo, docker |
| `build-host` | 宿主原生构建 host/client（C22）| `cargo build -p audemsp-host -p audemsp-client` | cargo |
| `build-server` | Docker 构建 server | `docker compose -f docker-compose.dev.yml build server` | docker |
| `up` | 启动 server 容器 | `docker compose -f docker-compose.dev.yml up -d server` | docker |
| `down` | 停止容器 | `docker compose -f docker-compose.dev.yml down` | docker |
| `logs` | 跟踪 server 日志 | `docker logs -f audemsp-server-1` | docker |
| `e2e` | e2e_sfu 回归（宿主原生连 Docker server）| `bash scripts/run-e2e-sfu.sh`（Unix）；Win 提示不支持 | cargo, docker, bash |
| `test` | workspace 测试 | `cargo test --workspace --exclude audemsp-server` | cargo |
| `ci` | CI 全链 | fmt + clippy + test + e2e（顺序，失败即停）| 全 |
| `clean` | 清构建产物 | `rm -rf target* + docker compose dev down -v`（路径平台分支）| docker |
| `config` | `show` / `validate` | 读 host.conf + server.docker.yaml（YAML 校验）| — |
| `status` | 环境诊断 | 检查 pixi/cargo/docker/pnpm/clang 版本 | — |
| `version` | CLI 版本 | 打印（读 Cargo.toml workspace version）| — |

**实现细节**：
- argparse `add_subparsers`，每个子命令 `-h`
- 项目根：`pathlib.Path(__file__).resolve().parent.parent`（任意 cwd 可调）
- subprocess 失败：非零退出码透传 + 打印 stderr
- 依赖检查：`shutil.which()`（跨平台）；缺失给修复提示（"先运行 bootstrap / 安装 docker"）
- `status`：`pixi --version` / `cargo --version` / `docker --version` / `node --version`，缺项标记（ANSI 仅 tty 时）
- 平台分支：仅两处——路径分隔（pathlib 自动）、e2e 的 bash 调用（`sys.platform == "win32"` 时提示 NotSupported）
- 常量：`VERSION = "0.1.0"`；配置路径相对项目根

## 5. 薄壳规格

**audemsp.sh**（Linux/macOS）：
```bash
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
if ! command -v pixi >/dev/null 2>&1 && [ ! -x "$HOME/.pixi/bin/pixi" ]; then
    echo "pixi 未安装 — 先运行: source bootstrap.sh" >&2
    exit 1
fi
source "$ROOT/scripts/pixi-shell.sh"        # 激活（同进程，含 PATH/LIBCLANG）
exec python "$ROOT/scripts/audemsp_cli.py" "$@"
```

**audemsp.bat**（Windows）：
```bat
@echo off
set "ROOT=%~dp0"
if not exist "%USERPROFILE%\.pixi\bin\pixi.exe" (
    echo pixi 未安装 — 先运行: bootstrap.bat
    exit /b 1
)
call "%ROOT%scripts\pixi-shell.bat"          :: 激活（同进程）
python "%ROOT%scripts\audemsp_cli.py" %*
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
| cargo/clang | 环境内 PATH 已注入 → 子命令直接可用 |
| 与 pixi tasks 关系 | 保留 `[tasks] init/shell`（pixi 原生入口）；CLI 是另一条等价路径，不冲突 |

## 8. 平台差异矩阵

| 项 | Unix | Windows | 处理 |
|----|------|---------|------|
| 薄壳 | audemsp.sh | audemsp.bat | 双文件（平台原生，不可避免）|
| CLI | 同一份 | 同一份 | Python 跨平台 |
| 激活 | source | call | 薄壳各自调 |
| e2e | bash run-e2e-sfu.sh | NotSupported 提示 | `sys.platform` 分支 |
| clean | rm -rf | rmdir /s /q（或提示）| pathlib + 平台分支 |
| 依赖检查 | command -v | where | `shutil.which()` |

## 9. 与现有设施关系

| 现有 | 关系 |
|------|------|
| bootstrap.sh/.bat | 保留；末尾加 CLI 提示 |
| pixi.toml [tasks] | 保留（init/shell）；CLI 并行路径 |
| scripts/run-e2e-sfu.sh | CLI `e2e` 直接调用（Unix）|
| scripts/pixi-shell.{sh,bat} | 薄壳复用 |
| C13/C22 | CLI 内固化（server→Docker、host→原生）|

## 10. 实施步骤

| 步骤 | 内容 | 验证 |
|------|------|------|
| T1 | `scripts/audemsp_cli.py`（全子命令 + -h + status/version）| `python scripts/audemsp_cli.py -h` |
| T2 | `audemsp.sh` + `audemsp.bat` 薄壳 | `./audemsp.sh version` |
| T3 | bootstrap.sh/.bat 末尾 CLI 提示 | source bootstrap 后提示出现 |
| T4 | `./audemsp.sh status` 环境诊断 + `config validate` | 输出正确 |
| T5 | 提交 + README 或 docs 登记 CLI 用法 | 文档可查 |

## 11. 验收标准

| 标准 | 通过条件 |
|------|---------|
| `audemsp -h` | 全部 12 子命令列出 |
| `audemsp version` | 输出 0.1.0 |
| `audemsp status` | pixi/cargo/docker 版本正确显示 |
| `audemsp build-host` | cargo 编译成功（复用宿主构建缓存）|
| `audemsp up` | server 容器启动 |
| `audemsp e2e` | 4/4 通过（Linux）|
| 平台 | audemsp.bat 语法检查（无 Linux 可验证，代码评审）|

## 12. 风险与缓解

| 风险 | 缓解 |
|------|------|
| Windows 未实测 | .bat 走现有 pixi-shell.bat 路径（已验证）；CLI 纯 Python 跨平台；CI 加 windows job 验证 version/status |
| 薄壳激活后 python 不在 PATH | pixi-shell 已验证注入；CLI 内 fallback 绝对路径 python |
| CLI 与 scripts 逻辑漂移 | CLI 内联为主；e2e 单一脚本引用点 |
| 命名冲突 | 已定名 audemsp，无系统命令冲突 |

## 13. 参考

- Carla 0.9: Makefile + Vars.mk + Linux.mk/Windows.mk + Setup.sh（入口+平台分发模式）
- vapkg 类: bootstrap → 自包含 CLI（子命令 + -h 交互）
- pixi 官方文档: [tasks] 通用 + [target.<platform>.tasks] 平台覆盖（实测验证 0.74.0）
- 本项目: scripts/pixi-shell.{sh,bat}、scripts/run-e2e-sfu.sh、pixi.toml
