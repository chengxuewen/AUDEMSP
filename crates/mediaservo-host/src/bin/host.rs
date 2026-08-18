//! host CLI: 业务视图运维入口（Phase A 骨架）。
//!
//! 子命令（`std::env::args` 手工解析，Phase A 不引入 clap）：
//! - `host init`   — 生成 `etc/host.toml` 配置模板
//! - `host status` — 调 `oxmgr list` 输出进程状态
//! - `host start`  — `oxmgr apply oxfile.toml` 拉起全部 host 进程
//! - `host stop`   — `oxmgr stop --namespace host` 停止全部 host 进程
//!
//! OxMgr 进程管理对接的精确动词在 Task A2 核对官方文档后落地
//! （`--dir` 变体、输出解析、oxfile 翻译见 A2/A3）。

use std::process::Command;

/// `host init` 生成的配置模板（host.toml 初版 schema）。
const HOST_TOML_TEMPLATE: &str = r#"# MediaServo host 配置（host init 生成）
[host]
device_id = "..."

[[cameras]]
id = "cam0"
source = "stub"
fps = 30

[[streams]]
id = "cam0-stream"
camera = "cam0"
codec = "h264"

[record]
enabled = false

[control]
enabled = false
"#;

fn main() {
    let mut args = std::env::args();
    let _prog = args.next();
    let Some(cmd) = args.next() else {
        print_usage();
        std::process::exit(2);
    };
    let code = match cmd.as_str() {
        "init" => cmd_init(),
        "status" => run_oxmgr(&["list"]),
        "start" => run_oxmgr(&["apply", "oxfile.toml"]),
        "stop" => run_oxmgr(&["stop", "--namespace", "host"]),
        _ => {
            eprintln!("未知子命令: {cmd}");
            print_usage();
            2
        }
    };
    std::process::exit(code);
}

fn print_usage() {
    eprintln!("用法: host <init|status|start|stop>");
}

/// `host init`: 生成 etc/host.toml 模板（已存在则跳过）。
fn cmd_init() -> i32 {
    let dir = std::path::Path::new("etc");
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("init: 创建 {} 失败: {e}", dir.display());
        return 1;
    }
    let path = dir.join("host.toml");
    if path.exists() {
        eprintln!("init: {} 已存在，跳过", path.display());
        return 0;
    }
    match std::fs::write(&path, HOST_TOML_TEMPLATE) {
        Ok(()) => {
            println!("已生成 {}", path.display());
            0
        }
        Err(e) => {
            eprintln!("init: 写入 {} 失败: {e}", path.display());
            1
        }
    }
}

/// 代理 oxmgr CLI；oxmgr 不在 PATH 时报清晰错误并提示安装。
fn run_oxmgr(args: &[&str]) -> i32 {
    match Command::new("oxmgr").args(args).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("oxmgr 执行失败: {e} — 请先安装 OxMgr 并加入 PATH");
            1
        }
    }
}
