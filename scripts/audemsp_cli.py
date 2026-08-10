#!/usr/bin/env python3
"""audemsp — AUDEMSP 统一构建 CLI（vapkg 式单入口）。

薄壳（audemsp.sh/.bat）保证 pixi 环境激活后调用本脚本：
环境内 PATH/LIBCLANG_PATH 已注入，subprocess 直接调 cargo/docker 等。
平台差异仅 e2e（bash 脚本）与 clean（删除命令）两处。

用法: audemsp [-h] {build,build-host,build-server,up,down,logs,e2e,test,ci,clean,config,status,version} ...
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

VERSION = "0.1.0"
ROOT = Path(__file__).resolve().parent.parent
COMPOSE_BASE = ["docker", "compose", "-f", "docker-compose.dev.yml"]
HOST_CONF = ROOT / "crates/audemsp-host/config/host.conf"
SERVER_YAML = ROOT / "config/server.docker.yaml"


def _check(tool: str, hint: str) -> None:
    """依赖检查 — 缺失时明确报错退出（不静默）。"""
    if shutil.which(tool) is None:
        print(f"错误: 缺少依赖 '{tool}' — {hint}", file=sys.stderr)
        sys.exit(1)


def _run(cmd: list[str]) -> int:
    """执行命令（继承环境），失败透传退出码。"""
    print(f"$ {' '.join(cmd)}")
    return subprocess.run(cmd).returncode


def _run_or_exit(cmd: list[str]) -> None:
    code = _run(cmd)
    if code != 0:
        sys.exit(code)


def _cmd_build_host() -> None:
    _check("cargo", "pixi 环境未激活? 先运行: source bootstrap.sh / pixi.bat")
    _run_or_exit(["cargo", "build", "-p", "audemsp-host", "-p", "audemsp-client"])


def _cmd_build_server() -> None:
    _check("docker", "安装 docker 并启动 daemon")
    _run_or_exit(COMPOSE_BASE + ["build", "server"])


def _cmd_build() -> None:
    _cmd_build_host()
    _cmd_build_server()


def _cmd_up() -> None:
    _check("docker", "安装 docker 并启动 daemon")
    _run_or_exit(COMPOSE_BASE + ["up", "-d", "server"])


def _cmd_down() -> None:
    _check("docker", "安装 docker 并启动 daemon")
    # v2 (审核 BLOCKER-1): 默认不带 -v — cargo-cache 命名卷必须保留
    _run_or_exit(COMPOSE_BASE + ["down"])


def _cmd_logs() -> None:
    _check("docker", "安装 docker 并启动 daemon")
    _run_or_exit(COMPOSE_BASE + ["logs", "-f", "server"])


def _cmd_e2e() -> None:
    if sys.platform == "win32":
        print("e2e: Windows 暂不支持（run-e2e-sfu.sh 为 bash 脚本）", file=sys.stderr)
        sys.exit(1)
    for tool, hint in (
        ("cargo", "pixi 环境未激活?"),
        ("docker", "server 容器需要 docker"),
        ("bash", "e2e 脚本需要 bash"),
        ("node", "e2e consume 脚本需要 node"),
    ):
        _check(tool, hint)
    # 前置: server 容器 + host 进程 + vite(5173) 运行中（脚本内会检查并明确报错）
    _run_or_exit(["bash", "scripts/run-e2e-sfu.sh"])


def _cmd_test() -> None:
    _check("cargo", "pixi 环境未激活?")
    _run_or_exit(["cargo", "test", "--workspace", "--exclude", "audemsp-server"])


def _cmd_ci() -> None:
    _check("cargo", "pixi 环境未激活?")
    _check("docker", "安装 docker 并启动 daemon")
    steps = [
        ["cargo", "fmt", "--all", "--", "--check"],
        ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        ["cargo", "test", "--workspace", "--exclude", "audemsp-server"],
    ]
    for step in steps:
        code = _run(step)
        if code != 0:
            sys.exit(code)
    _cmd_e2e()


def _rm_tree(path: Path) -> None:
    """跨平台目录删除（Windows 用 rmdir /s /q 语义，Unix 用 rmtree）。
    容器生成的 root 文件会导致 PermissionError — 捕获并提示手动删除，不中断后续清理。"""
    if not path.exists():
        return
    try:
        if sys.platform == "win32":
            _run_or_exit(["rmdir", "/s", "/q", str(path)])
        else:
            shutil.rmtree(path)
        print(f"已删除: {path}")
    except PermissionError:
        print(
            f"警告: 无法删除 {path}（含容器生成的 root 文件）— 手动执行: sudo rm -rf {path}",
            file=sys.stderr,
        )


def _cmd_clean(args: argparse.Namespace) -> None:
    _check("docker", "安装 docker 并启动 daemon")
    # 1) 停止容器 — 默认保留命名卷（审核 BLOCKER-1）
    down = COMPOSE_BASE + ["down"]
    if args.all:
        down.append("-v")  # --all 显式删卷（cargo-cache）→ 下次 build-server 15-30 分钟重建
        print("警告: clean --all 将删除 cargo-cache 命名卷（下次 server 构建全量重编 15-30 分钟）")
    _run_or_exit(down)
    # 2) 项目根 target（workspace 默认）
    _rm_tree(ROOT / "target")
    # 3) CARGO_TARGET_DIR 分支（审核: 用户设置时项目根清理会漏）
    cargo_target = os.environ.get("CARGO_TARGET_DIR")
    if cargo_target:
        print(f"注意: CARGO_TARGET_DIR={cargo_target}（可能被多项目共享）")
        _rm_tree(Path(cargo_target))
    # 4) --all 额外清 docker builder 缓存；不碰 .pixi-cache（包缓存）
    if args.all:
        _run_or_exit(["docker", "builder", "prune", "-f"])


def _cmd_config(args: argparse.Namespace) -> None:
    if args.config_cmd == "show":
        for path in (HOST_CONF, SERVER_YAML):
            print(f"--- {path.relative_to(ROOT)} ---")
            if path.exists():
                print(path.read_text(encoding="utf-8"))
            else:
                print(f"(缺失: {path})", file=sys.stderr)
        return
    # validate — pyyaml 真解析（审核 BLOCKER-2: pixi.toml 已加依赖）
    try:
        import yaml  # noqa: PLC0415
    except ImportError:
        print("错误: 缺少 pyyaml — 运行: pixi install", file=sys.stderr)
        sys.exit(1)
    ok = True
    for path in (HOST_CONF, SERVER_YAML):
        if not path.exists():
            print(f"缺失: {path.relative_to(ROOT)}", file=sys.stderr)
            ok = False
            continue
        try:
            yaml.safe_load(path.read_text(encoding="utf-8"))
            print(f"OK: {path.relative_to(ROOT)}")
        except yaml.YAMLError as e:
            print(f"YAML 错误: {path.relative_to(ROOT)}: {e}", file=sys.stderr)
            ok = False
    sys.exit(0 if ok else 1)


def _cmd_status() -> None:
    """环境诊断 — 逐工具检查版本，缺失标 MISSING。"""
    pixi_bin = shutil.which("pixi") or str(Path.home() / ".pixi/bin/pixi")
    tools = [
        ("pixi", [pixi_bin, "--version"]),
        ("cargo", ["cargo", "--version"]),
        ("docker", ["docker", "--version"]),
        ("node", ["node", "--version"]),
    ]
    for name, cmd in tools:
        try:
            result = subprocess.run(
                cmd, capture_output=True, text=True, timeout=10, check=False
            )
        except (OSError, subprocess.TimeoutExpired):
            print(f"{name:8s} MISSING（或超时）")
            continue
        output = (result.stdout or result.stderr).strip().splitlines()
        print(f"{name:8s} {output[0] if output else '?'}")


def _cmd_version() -> None:
    print(VERSION)


def main() -> None:
    parser = argparse.ArgumentParser(
        prog="audemsp",
        description="AUDEMSP 统一构建 CLI（单入口: build/up/e2e/clean/config/status...）",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("build", help="全量构建（build-host + build-server）")
    sub.add_parser("build-host", help="宿主原生构建 host/client（C22）")
    sub.add_parser("build-server", help="Docker 构建 server（mediasoup）")
    sub.add_parser("up", help="启动 server 容器")
    sub.add_parser("down", help="停止容器（保留 cargo-cache 卷）")
    sub.add_parser("logs", help="跟踪 server 日志")
    sub.add_parser(
        "e2e", help="e2e_sfu 回归（前置: server 容器 + host + vite(5173) 运行中）"
    )
    sub.add_parser("test", help="workspace 测试（排除 audemsp-server）")
    sub.add_parser("ci", help="CI 全链: fmt → clippy → test → e2e")
    clean_p = sub.add_parser("clean", help="清构建产物（默认保留卷）")
    clean_p.add_argument("--all", action="store_true", help="显式删卷 + docker builder prune（15-30 分钟重建代价）")
    clean_p.set_defaults(func=_cmd_clean)
    config_p = sub.add_parser("config", help="配置 show/validate")
    config_p.add_argument("config_cmd", choices=["show", "validate"])
    config_p.set_defaults(func=_cmd_config)
    sub.add_parser("status", help="环境诊断（pixi/cargo/docker/node）")
    sub.add_parser("version", help="CLI 版本")

    args = parser.parse_args()
    if args.command in ("build", "build-host", "build-server", "up", "down", "logs", "e2e", "test", "ci"):
        globals()[f"_cmd_{args.command}"]()
    elif args.command == "status":
        _cmd_status()
    elif args.command == "version":
        _cmd_version()
    elif hasattr(args, "func"):
        args.func(args)


if __name__ == "__main__":
    main()
