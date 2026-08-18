#!/usr/bin/env python3
"""mediaservo — MediaServo 统一构建 CLI（vapkg 式单入口）。

薄壳（mediaservo.sh/.bat）保证 pixi 环境激活后调用本脚本：
环境内 PATH/LIBCLANG_PATH 已注入，subprocess 直接调 cargo/docker 等。
平台差异仅 e2e（bash 脚本）与 clean（删除命令）两处。

用法: mediaservo [-h] {build,build-host,build-server,up,down,logs,e2e,test,ci,install,clean,config,status,version} ...
"""

from __future__ import annotations

import argparse
import os
import shutil
import time
import subprocess
import sys
from pathlib import Path

VERSION = "0.1.0"
ROOT = Path(__file__).resolve().parent.parent
COMPOSE_BASE = ["docker", "compose", "-f", "docker-compose.dev.yml"]
HOST_CONF = ROOT / "crates/mediaservo-host/config/host.conf"
SERVER_YAML = ROOT / "config/server.docker.yaml"


def _check(tool: str, hint: str) -> None:
    """依赖检查 — 缺失时明确报错退出（不静默）。"""
    if shutil.which(tool) is None:
        print(f"错误: 缺少依赖 '{tool}' — {hint}", file=sys.stderr)
        sys.exit(1)


def _run(cmd: list[str], env: dict[str, str] | None = None) -> int:
    """执行命令（默认继承环境），失败透传退出码。"""
    print(f"$ {' '.join(cmd)}")
    return subprocess.run(cmd, env=env).returncode


def _run_or_exit(cmd: list[str], env: dict[str, str] | None = None) -> None:
    code = _run(cmd, env=env)
    if code != 0:
        sys.exit(code)


def _cmd_build_host() -> None:
    _check("cargo", "pixi 环境未激活? 先运行: source bootstrap.sh / pixi.bat")
    _run_or_exit(["cargo", "build", "-p", "mediaservo-host", "-p", "mediaservo-client"])


def _cmd_build_server() -> None:
    _check("docker", "安装 docker 并启动 daemon")
    _run_or_exit(COMPOSE_BASE + ["build", "server"])


def _cmd_build_client() -> None:
    _check("cargo", "pixi 环境未激活? 先运行: source bootstrap.sh / pixi.bat")
    _run_or_exit(["cargo", "build", "-p", "mediaservo-client"])


def _cmd_build(target: str) -> None:
    """build <target> — all|host|server|client|bindings（默认 all）。"""
    if target in ("all", "host"):
        _cmd_build_host()
    if target in ("all", "server"):
        _cmd_build_server()
    if target in ("all", "client"):
        _cmd_build_client()
    if target == "bindings":
        _cmd_build_bindings()


def _workspace_version() -> str:
    """workspace 版本（[workspace.package] version，如 0.1.0）。py3.10 无 tomllib，轻量解析。"""
    text = (ROOT / "Cargo.toml").read_text()
    seg = text.split("[workspace.package]", 1)[1].split("[", 1)[0]
    for line in seg.splitlines():
        line = line.strip()
        if line.startswith("version") and "=" in line:
            return line.split("=", 1)[1].strip().strip('"')
    raise SystemExit("错误: workspace version 未找到")


def _symlink_force(target: str, link: Path) -> None:
    """幂等符号链接（存在则先删）。"""
    try:
        link.unlink()
    except FileNotFoundError:
        pass
    os.symlink(target, link)


def _cmd_build_bindings() -> None:
    """构建三 SDK cdylib + dev .so.<MAJOR> symlink（D241: DT_NEEDED 解析）。"""
    _check("cargo", "pixi 环境未激活? 先运行: source bootstrap.sh / pixi.bat")
    _run_or_exit([
        "cargo", "build",
        "-p", "mediaservo-field-c", "-p", "mediaservo-link-c", "-p", "mediaservo-deck-c",
    ])
    major = _workspace_version().split(".")[0]
    for sdk in ("field", "link", "deck"):
        _symlink_force(
            f"libmediaservo_{sdk}.so",
            ROOT / f"target/debug/libmediaservo_{sdk}.so.{major}",
        )
    print("bindings 构建完成: libmediaservo_{field,link,deck}.so (dev symlink .so.%s)" % major)


def _cmd_install_bindings(prefix: str) -> None:
    """安装 bindings: libmediaservo_<sdk>.so.<MAJOR>.<MINOR>.<PATCH> 三件套（D241）
    + C/cxx 头文件（D248 include/mediaservo 布局）。"""
    ver = _workspace_version()
    major, minor, patch = ver.split(".")
    lib_dir = Path(prefix) / "lib"
    inc_dir = Path(prefix) / "include" / "mediaservo"
    lib_dir.mkdir(parents=True, exist_ok=True)
    inc_dir.mkdir(parents=True, exist_ok=True)

    for sdk in ("field", "link", "deck"):
        src = ROOT / f"target/debug/libmediaservo_{sdk}.so"
        if not src.exists():
            print(f"错误: {src} 不存在 — 先运行: mediaservo build bindings", file=sys.stderr)
            sys.exit(1)
        real = lib_dir / f"libmediaservo_{sdk}.so.{major}.{minor}.{patch}"
        shutil.copy2(src, real)
        _symlink_force(real.name, lib_dir / f"libmediaservo_{sdk}.so.{major}")
        _symlink_force(f"libmediaservo_{sdk}.so.{major}", lib_dir / f"libmediaservo_{sdk}.so")

    for h in (ROOT / "bindings/c/include/mediaservo").glob("*.h"):
        shutil.copy2(h, inc_dir)
    for sdk in ("field", "link", "deck"):
        for h in (ROOT / f"bindings/c/mediaservo-{sdk}-c/include/mediaservo").glob("*.h"):
            shutil.copy2(h, inc_dir)
        for h in (ROOT / f"bindings/cxx/mediaservo-{sdk}-cxx/include/mediaservo").glob("*.hpp"):
            shutil.copy2(h, inc_dir)

    # pkg-config (.pc) + CMake package config：模板渲染（FFmpeg/iceoryx2 惯例）
    tpl = ROOT / "bindings/install/templates"
    pc_dir = lib_dir / "pkgconfig"
    cmake_dir = lib_dir / "cmake" / "mediaservo"
    pc_dir.mkdir(parents=True, exist_ok=True)
    cmake_dir.mkdir(parents=True, exist_ok=True)
    prefix_abs = str(lib_dir.parent)  # 规范绝对 prefix（configure 传统；模板用 ${pcfiledir} 保持可重定位）
    for t in sorted(tpl.glob("*.pc.in")):
        content = t.read_text().replace("@VERSION@", ver).replace("${pcfiledir}/../..", prefix_abs)
        (pc_dir / t.name[:-3]).write_text(content)  # .pc.in → .pc
    for name, t in (("mediaservoConfig.cmake", "mediaservoConfig.cmake.in"),
                    ("mediaservoConfigVersion.cmake", "mediaservoConfigVersion.cmake.in")):
        content = (tpl / t).read_text().replace("@VERSION@", ver).replace("@MAJOR@", major)
        (cmake_dir / name).write_text(content)

    print(f"bindings 已安装到 {prefix}")
    print(f"  lib/    libmediaservo_{{field,link,deck}}.so.{major}.{minor}.{patch} + .so.{major} + .so")
    print(f"  lib/pkgconfig/   mediaservo-{{field,link,deck}}.pc（pkg-config 消费）")
    print(f"  lib/cmake/mediaservo/  mediaservoConfig.cmake + ConfigVersion.cmake（find_package(mediaservo)）")
    print(f"  include/mediaservo/  {{common,field,link,deck}}.h + {{field,link,deck}}.hpp")
    print("Python: pip install bindings/python/mediaservo（薄包；运行时 .so 定位: LD_LIBRARY_PATH 或 ldconfig）")


# 排除的接口类型/名称：docker 网桥、VPN 隧道、虚拟接口（这些 IP 客户端不可达）
_ANNOUNCED_IP_BLOCKED_IFACE = ("lo", "docker", "br-", "veth", "tun", "tap", "virbr", "vpn")


def _detect_announced_ips() -> list[str]:
    """探测宿主机全部真实网卡 IP（供容器内 mediasoup announced_address 使用）。

    宿主 IP 会变且可能有多个（多网卡/DHCP）——返回全部真实 IP（逗号分隔），
    由 server 侧为每个 IP 创建 ListenInfo（WebRtcServer 多 announced）。
    按接口名过滤 docker 网桥(br-*)/VPN(tun*)/虚拟接口，仅保留物理/真实网卡。"""
    ips: list[str] = []
    try:
        out = subprocess.run(
            ["ip", "-o", "-4", "addr", "show"], capture_output=True, text=True, timeout=5, check=False
        )
        for line in out.stdout.splitlines():
            # 格式: "2: ens32    inet 192.168.2.127/24 brd ... scope global ..."
            parts = line.split()
            if len(parts) < 4 or parts[2] != "inet":
                continue
            iface = parts[1].rstrip(":")
            if any(iface.startswith(p) for p in _ANNOUNCED_IP_BLOCKED_IFACE):
                continue
            ip = parts[3].split("/")[0]
            if ip.startswith("127."):
                continue
            if ip not in ips:
                ips.append(ip)
    except OSError:
        # ip 命令不可用 → 回退 hostname -I（粗过滤）
        try:
            out = subprocess.run(
                ["hostname", "-I"], capture_output=True, text=True, timeout=5, check=False
            )
            for ip in out.stdout.split():
                ip = ip.strip()
                if ip and not ip.startswith("127.") and ip not in ips:
                    ips.append(ip)
        except OSError:
            pass
    return ips


def _compose_env() -> dict[str, str]:
    """docker compose 调用环境 — 确保 MEDIASERVO_SFU_ANNOUNCED_IP 有值。
    PIT-79: CLI 启动 server 时若未注入，mediasoup 公告 0.0.0.0 → 浏览器拉流失败。
    显式 env 优先，否则自动探测宿主机全部真实 IP（逗号分隔，多网卡支持）。"""
    env = {**os.environ}
    if not env.get("MEDIASERVO_SFU_ANNOUNCED_IP"):
        ips = _detect_announced_ips()
        if ips:
            env["MEDIASERVO_SFU_ANNOUNCED_IP"] = ",".join(ips)
            print(f"MEDIASERVO_SFU_ANNOUNCED_IP 自动探测: {env['MEDIASERVO_SFU_ANNOUNCED_IP']}")
    return env


def _cmd_start(target: str, foreground: bool = False) -> None:
    """start <target> [--foreground] — server: compose 幂等启动; host: 启动推流进程。
    --foreground/-f: 阻塞前台运行，输出实时透传（开发调试）。"""
    if target == "server":
        _check("docker", "安装 docker 并启动 daemon")
        cmd = COMPOSE_BASE + (["up"] if foreground else ["up", "-d", "server"])
        _run_or_exit(cmd, env=_compose_env())
    elif target == "host":
        if foreground:
            _run_host_foreground(_find_host_binary())
        else:
            _cmd_run_host()
    else:  # client
        print("start client: 待实现（client 骨架阶段）", file=sys.stderr)
        sys.exit(1)


def _cmd_restart(target: str) -> None:
    """restart <target> — 清除已运行的再启动（显式中断语义）。"""
    if target == "server":
        _check("docker", "安装 docker 并启动 daemon")
        print("重启 server: 停止旧容器...")
        subprocess.run(COMPOSE_BASE + ["down"], check=False, env=_compose_env())  # 无容器时忽略错误
        _run_or_exit(COMPOSE_BASE + ["up", "-d", "server"], env=_compose_env())
        print("✓ server 已重启")
    elif target == "host":
        _cmd_run_host()
    else:  # client
        print("restart client: 待实现（client 骨架阶段）", file=sys.stderr)
        sys.exit(1)


def _find_host_binary() -> Path:
    """找 host 二进制（优先 CARGO_TARGET_DIR，回退项目 target）。"""
    cargo_target = os.environ.get("CARGO_TARGET_DIR")
    candidates = []
    if cargo_target:
        candidates.append(Path(cargo_target) / "debug/mediaservo-host")
    candidates += [
        ROOT / "target/debug/mediaservo-host",
        ROOT / "target/release/mediaservo-host",
    ]
    bin_path = next((p for p in candidates if p.exists()), None)
    if bin_path is None:
        print("错误: 未找到 mediaservo-host 二进制 — 先运行: mediaservo build host", file=sys.stderr)
        sys.exit(1)
    return bin_path


def _run_host_foreground(bin_path: Path) -> None:
    """前台阻塞运行 host — 输出实时透传终端，Ctrl+C 同步退出（开发调试用）。
    host 单实例端口 9801 独占：启动前必须清旧（与后台路径一致）。"""
    subprocess.run(["pkill", "-x", "mediaservo-host"], check=False)
    time.sleep(1)
    env = {**os.environ, "RUST_LOG": "info"}
    proc = subprocess.Popen([str(bin_path)], cwd=ROOT, env=env)
    try:
        proc.wait()
    except KeyboardInterrupt:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
        sys.exit(130)
    sys.exit(proc.returncode)


def _cmd_run_host() -> None:
    """启动 host 推流 — 先杀旧进程再启动（单实例端口 9801 独占，清旧是必要前置）。"""
    if sys.platform == "win32":
        print("run-host: Windows 暂不支持", file=sys.stderr)
        sys.exit(1)
    bin_path = _find_host_binary()
    if bin_path is None:
        print("错误: 未找到 mediaservo-host 二进制 — 先运行: mediaservo build-host", file=sys.stderr)
        sys.exit(1)
    # 2) 杀旧进程（pkill -x 精确进程名，避免误杀）
    subprocess.run(["pkill", "-x", "mediaservo-host"], check=False)
    time.sleep(1)
    # 3) 后台启动（start_new_session 脱离终端，日志 /tmp/mediaservo-host.log）
    log_path = Path("/tmp/mediaservo-host.log")
    env = {**os.environ, "RUST_LOG": "info"}
    proc = subprocess.Popen(
        [str(bin_path)],
        cwd=ROOT,
        env=env,
        stdout=open(log_path, "wb"),
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )
    time.sleep(3)
    if proc.poll() is None:
        print(f"✓ host 已启动 (PID {proc.pid}) — 配置: crates/mediaservo-host/config/host.conf")
        print(f"  日志: {log_path}")
    else:
        print(f"✗ host 启动失败 (exit {proc.returncode}) — 日志: {log_path}", file=sys.stderr)
        sys.exit(1)

def _cmd_stop(target: str) -> None:
    """stop <target> — server: compose stop（保留容器，秒级再启）; host/client: 杀进程。"""
    if target == "server":
        _check("docker", "安装 docker 并启动 daemon")
        _run_or_exit(COMPOSE_BASE + ["stop", "server"])
    elif target == "host":
        subprocess.run(["pkill", "-x", "mediaservo-host"], check=False)
        print("✓ host 已停止")
    else:  # client
        subprocess.run(["pkill", "-x", "mediaservo-client"], check=False)
        print("✓ client 已停止")


def _cmd_logs(target: str) -> None:
    """logs <target> — server: compose 日志; host: /tmp/mediaservo-host.log。"""
    if target == "server":
        _check("docker", "安装 docker 并启动 daemon")
        _run_or_exit(COMPOSE_BASE + ["logs", "-f", "server"])
    elif target == "host":
        log_path = Path("/tmp/mediaservo-host.log")
        if not log_path.exists():
            print(f"错误: 无 host 日志 {log_path} — 先运行: mediaservo up host", file=sys.stderr)
            sys.exit(1)
        _run_or_exit(["tail", "-f", str(log_path)])
    else:  # client
        print("logs client: 待实现（client 骨架阶段）", file=sys.stderr)
        sys.exit(1)


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
    _run_or_exit(["cargo", "test", "--workspace", "--exclude", "mediaservo-server"])


def _cmd_ci() -> None:
    _check("cargo", "pixi 环境未激活?")
    _check("docker", "安装 docker 并启动 daemon")
    steps = [
        ["cargo", "fmt", "--all", "--", "--check"],
        ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        ["cargo", "test", "--workspace", "--exclude", "mediaservo-server"],
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


def _cmd_install(args: argparse.Namespace) -> None:
    """install <target> — bindings。"""
    if args.target == "bindings":
        _cmd_install_bindings(args.prefix)


def _cmd_clean(args: argparse.Namespace) -> None:
    """clean <target> — all|server|host|client（默认 all）。
    server: 停容器(+--all 删卷+builder prune); host/client: 清宿主 cargo target。"""
    target = args.target
    if target in ("all", "server"):
        _check("docker", "安装 docker 并启动 daemon")
        down = COMPOSE_BASE + ["down"]
        if args.all:
            down.append("-v")  # --all 显式删卷（cargo-cache）→ 下次 build-server 15-30 分钟重建
            print("警告: clean --all 将删除 cargo-cache 命名卷（下次 server 构建全量重编 15-30 分钟）")
        _run_or_exit(down)
    if target in ("all", "host", "client"):
        # 项目根 target（workspace 默认，host/client 共享）
        _rm_tree(ROOT / "target")
        # CARGO_TARGET_DIR 分支（审核: 用户设置时项目根清理会漏）
        cargo_target = os.environ.get("CARGO_TARGET_DIR")
        if cargo_target:
            print(f"注意: CARGO_TARGET_DIR={cargo_target}（可能被多项目共享）")
            _rm_tree(Path(cargo_target))
    # --all 额外清 docker builder 缓存；不碰 .pixi-cache（包缓存）
    if args.all and target in ("all", "server"):
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
        prog="mediaservo",
        description="MediaServo 统一构建 CLI（单入口: build/up/e2e/clean/config/status...）",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    build_p = sub.add_parser("build", help="构建 <target>: all|host|server|client|bindings（默认 all）")
    build_p.add_argument("target", nargs="?", choices=["all", "host", "server", "client", "bindings"], default="all")
    for verb, help_txt in (
        ("stop", "停止 <target>: server(compose stop 保留容器) | host/client(进程)"),
        ("restart", "重启 <target>: 清旧再启（保留卷）"),
        ("logs", "日志 <target>: server(compose) | host(/tmp/mediaservo-host.log)"),
    ):
        vp = sub.add_parser(verb, help=help_txt)
        vp.add_argument("target", choices=["server", "host", "client"])
    start_p = sub.add_parser("start", help="启动 <target> [--foreground]: server(compose) | host(推流进程) | client")
    start_p.add_argument("target", choices=["server", "host", "client"])
    start_p.add_argument("--foreground", "-f", action="store_true", help="阻塞前台运行，输出实时透传（开发调试）")
    sub.add_parser(
        "e2e", help="e2e_sfu 回归（前置: server 容器 + host + vite(5173) 运行中）"
    )
    sub.add_parser("test", help="workspace 测试（排除 mediaservo-server）")
    sub.add_parser("ci", help="CI 全链: fmt → clippy → test → e2e")
    install_p = sub.add_parser("install", help="安装 <target>: bindings（lib 三件套 D241 + include/mediaservo 头 D248）")
    install_p.add_argument("target", choices=["bindings"])
    install_p.add_argument("--prefix", default=str(ROOT / "install"), help="安装前缀（默认 <项目根>/install）")
    install_p.set_defaults(func=_cmd_install)
    clean_p = sub.add_parser("clean", help="清理 <target>: all|server|host|client（默认 all）")
    clean_p.add_argument("target", nargs="?", choices=["all", "server", "host", "client"], default="all")
    clean_p.add_argument("--all", action="store_true", help="显式删卷 + docker builder prune（15-30 分钟重建代价）")
    clean_p.set_defaults(func=_cmd_clean)
    config_p = sub.add_parser("config", help="配置 show/validate")
    config_p.add_argument("config_cmd", choices=["show", "validate"])
    config_p.set_defaults(func=_cmd_config)
    sub.add_parser("status", help="环境诊断（pixi/cargo/docker/node）")
    sub.add_parser("version", help="CLI 版本")

    # 兼容别名: build-host → build host, build-server → build server, run-host → up host
    ALIASES = {
        "build-host": ["build", "host"],
        "build-server": ["build", "server"],
        "run-host": ["start", "host"],
        "up": ["start"],
        "down": ["stop"],
    }
    argv = sys.argv[1:]
    if argv and argv[0] in ALIASES:
        argv = ALIASES[argv[0]] + argv[1:]
    args = parser.parse_args(argv)
    if args.command == "start":
        _cmd_start(args.target, args.foreground)
    elif args.command in ("build", "stop", "restart", "logs"):
        globals()[f"_cmd_{args.command}"](args.target)
    elif args.command in ("e2e", "test", "ci"):
        globals()[f"_cmd_{args.command}"]()
    elif args.command == "status":
        _cmd_status()
    elif args.command == "version":
        _cmd_version()
    elif hasattr(args, "func"):
        args.func(args)


if __name__ == "__main__":
    main()
