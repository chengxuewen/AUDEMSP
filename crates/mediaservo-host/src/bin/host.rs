//! host CLI: 业务视图运维入口（Phase A）。
//!
//! 子命令（`std::env::args` 手工解析，Phase A 不引入 clap）：
//! - `host init <dir>`        — 生成 `etc/host.toml` 模板 + `etc/link/signing.pem`
//!   （Ed25519 PKCS#8 keypair，0600）+ 空 `etc/link/` 目录
//! - `host start --dir <dir>` — 读 etc/host.toml → translate → 写 run/oxfile.toml
//!   → `oxmgr apply run/oxfile.toml` 拉起全部 host 进程
//! - `host stop --dir <dir>`  — `oxmgr stop run/oxfile.toml` + `oxmgr delete run/oxfile.toml`
//!   （config 目标解析 oxfile 内全部 app，幂等；动词见 OxMgr docs/CLI.md）
//! - `host status --dir <dir>`— `oxmgr list --json` 过滤 `namespace == "host"` 输出状态表
//! - `host doctor --dir <dir>`— 环境诊断（oxmgr 可用 / host.toml 解析 / oxfile 生成），
//!   退出码 = 失败检查数
//! - `host version`           — 打印版本号
//!
//! OxMgr 动词核对（C11/C18，来源 .refinfo/OxMgr/docs/CLI.md + SKILL.md）：
//! `apply <config>` / `list [--json]` / `stop <name|id|config>` / `delete <name|id|config>`
//! （**无** `stop/delete --namespace` 旗标；config 目标自动解析 oxfile 内全部 app，
//! 见 CLI.md "Lifecycle" 段）。

use std::path::{Path, PathBuf};
use std::process::Command;

/// `host init` 生成的配置模板（host.toml 初版 schema，A1）。
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

const USAGE: &str = "用法: host <init|start|stop|status|version> [--dir <dir>]";

fn main() {
    let mut args = std::env::args();
    let _prog = args.next();
    let Some(cmd) = args.next() else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    let code = match cmd.as_str() {
        "init" => cmd_init(&mut args),
        "start" => cmd_start(&mut args),
        "stop" => cmd_stop(&mut args),
        "status" => cmd_status(&mut args),
        "doctor" => cmd_doctor(&mut args),
        "version" => {
            println!("mediaservo-host {}", env!("CARGO_PKG_VERSION"));
            0
        }
        _ => {
            eprintln!("未知子命令: {cmd}");
            eprintln!("{USAGE}");
            2
        }
    };
    std::process::exit(code);
}

/// 从子命令参数解析 `--dir <dir>`；缺省 `.`。
fn parse_dir(args: &mut impl Iterator<Item = String>) -> Option<PathBuf> {
    let mut dir = PathBuf::from(".");
    while let Some(arg) = args.next() {
        if arg == "--dir" {
            dir = PathBuf::from(args.next()?);
        } else {
            eprintln!("未知参数: {arg}");
            return None;
        }
    }
    Some(dir)
}

/// `host init <dir>`: 生成 etc/host.toml 模板 + etc/link/signing.pem（Ed25519，0600）。
fn cmd_init(args: &mut impl Iterator<Item = String>) -> i32 {
    let dir = args.next().map(PathBuf::from).unwrap_or_default();
    let etc = dir.join("etc");
    let link = etc.join("link");
    if let Err(e) = std::fs::create_dir_all(&link) {
        eprintln!("init: 创建 {} 失败: {e}", link.display());
        return 1;
    }

    let cfg_path = etc.join("host.toml");
    if cfg_path.exists() {
        eprintln!("init: {} 已存在，跳过", cfg_path.display());
    } else if let Err(e) = std::fs::write(&cfg_path, HOST_TOML_TEMPLATE) {
        eprintln!("init: 写入 {} 失败: {e}", cfg_path.display());
        return 1;
    } else {
        println!("已生成 {}", cfg_path.display());
    }

    let pem_path = link.join("signing.pem");
    if pem_path.exists() {
        eprintln!("init: {} 已存在，跳过", pem_path.display());
    } else {
        match gen_signing_pem() {
            Ok(pem) => {
                if let Err(e) = write_private_pem(&pem_path, &pem) {
                    eprintln!("init: 写入 {} 失败: {e}", pem_path.display());
                    return 1;
                }
                println!("已生成 {}（Ed25519 私钥，0600）", pem_path.display());
            }
            Err(e) => {
                eprintln!("init: 生成 Ed25519 密钥失败: {e}");
                return 1;
            }
        }
    }
    0
}

/// 生成 Ed25519 PKCS#8 PEM 私钥（link 令牌签名消费，D238）。
fn gen_signing_pem() -> Result<String, String> {
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    let mut csprng = rand_core::OsRng;
    let signing = ed25519_dalek::SigningKey::generate(&mut csprng);
    let doc = signing
        .to_pkcs8_pem(pkcs8::LineEnding::LF)
        .map_err(|e| e.to_string())?;
    Ok(doc.to_string())
}

/// 写私钥文件并设 0600 权限。
fn write_private_pem(path: &Path, pem: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    f.write_all(pem.as_bytes())
}

/// `host start --dir <dir>`: 翻译 host.toml → run/oxfile.toml → `oxmgr apply`。
fn cmd_start(args: &mut impl Iterator<Item = String>) -> i32 {
    let Some(dir) = parse_dir(args) else {
        return 2;
    };
    let cfg_path = dir.join("etc").join("host.toml");
    let cfg = match std::fs::read_to_string(&cfg_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("start: 读取 {} 失败: {e} — 先运行 host init <dir>", cfg_path.display());
            return 1;
        }
    };
    let ox = match mediaservo_host::translate::to_oxfile(&cfg) {
        Ok(ox) => ox,
        Err(e) => {
            eprintln!("start: {e}");
            return 1;
        }
    };
    let run_dir = dir.join("run");
    if let Err(e) = std::fs::create_dir_all(&run_dir) {
        eprintln!("start: 创建 {} 失败: {e}", run_dir.display());
        return 1;
    }
    let oxfile = run_dir.join("oxfile.toml");
    if let Err(e) = std::fs::write(&oxfile, ox) {
        eprintln!("start: 写入 {} 失败: {e}", oxfile.display());
        return 1;
    }
    println!("start: 已生成 {}", oxfile.display());
    run_oxmgr(&["apply", oxfile.to_str().expect("oxfile path utf8")])
}

/// `host stop --dir <dir>`: `oxmgr stop <oxfile>` + `oxmgr delete <oxfile>`（幂等）。
///
/// 无 run/oxfile.toml（从未 apply 或已删除）→ 视为无进程，直接成功。
fn cmd_stop(args: &mut impl Iterator<Item = String>) -> i32 {
    let Some(dir) = parse_dir(args) else {
        return 2;
    };
    let oxfile = dir.join("run").join("oxfile.toml");
    if !oxfile.exists() {
        println!("stop: 无 {}，无已管理进程", oxfile.display());
        return 0;
    }
    let oxfile = oxfile.to_str().expect("oxfile path utf8");
    let code = run_oxmgr(&["stop", oxfile]);
    if code != 0 {
        return code;
    }
    run_oxmgr(&["delete", oxfile])
}

/// `host status --dir <dir>`: `oxmgr list --json` 过滤 host 命名空间，输出状态表。
fn cmd_status(args: &mut impl Iterator<Item = String>) -> i32 {
    let Some(_dir) = parse_dir(args) else {
        return 2;
    };
    let out = match Command::new("oxmgr").args(["list", "--json"]).output() {
        Ok(out) => out,
        Err(e) => {
            eprintln!("oxmgr 执行失败: {e} — 请先安装 OxMgr 并加入 PATH");
            return 1;
        }
    };
    if !out.status.success() {
        eprintln!("oxmgr list 失败: {}", String::from_utf8_lossy(&out.stderr).trim());
        return 1;
    }
    let procs: serde_json::Value = match serde_json::from_slice(&out.stdout) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("status: 解析 oxmgr list --json 输出失败: {e}");
            return 1;
        }
    };
    let Some(rows) = procs.as_array() else {
        eprintln!("status: oxmgr list --json 输出非数组");
        return 1;
    };
    let host_procs: Vec<&serde_json::Value> = rows
        .iter()
        .filter(|p| p.get("namespace").and_then(|n| n.as_str()) == Some("host"))
        .collect();
    if host_procs.is_empty() {
        println!("host 命名空间无已管理进程（先 host start --dir <dir>）");
        return 0;
    }
    println!("{:<28} {:<10} PID", "NAME", "STATUS");
    for p in &host_procs {
        let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let status = p.get("status").and_then(|v| v.as_str()).unwrap_or("?");
        let pid = p.get("pid").and_then(|v| v.as_u64()).map_or("-".to_string(), |v| v.to_string());
        println!("{name:<28} {status:<10} {pid}");
    }
    0
}
/// `host doctor --dir <dir>`: 环境诊断。三项检查：
/// ① oxmgr 可执行（PATH 内）② etc/host.toml 可解析 ③ host.toml → oxfile 可生成。
/// 退出码 = 失败检查数（0..=3）。
fn cmd_doctor(args: &mut impl Iterator<Item = String>) -> i32 {
    let Some(dir) = parse_dir(args) else {
        return 2;
    };
    let mut failed = 0;

    match Command::new("oxmgr").arg("--version").output() {
        Ok(_) => println!("[ok] oxmgr 可用"),
        Err(e) => {
            println!("[fail] oxmgr 不可用: {e} — 请先安装并加入 PATH（npm install -g oxmgr）");
            failed += 1;
        }
    }

    let cfg_path = dir.join("etc").join("host.toml");
    let cfg = match std::fs::read_to_string(&cfg_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("[fail] 读取 {} 失败: {e} — 先运行 host init <dir>", cfg_path.display());
            return failed + 2; // ②③ 均因无配置失败
        }
    };
    match toml::from_str::<toml::Value>(&cfg) {
        Ok(_) => println!("[ok] host.toml 可解析"),
        Err(e) => {
            println!("[fail] host.toml 解析失败: {e}");
            failed += 1;
        }
    }
    match mediaservo_host::translate::to_oxfile(&cfg) {
        Ok(_) => println!("[ok] oxfile 生成成功"),
        Err(e) => {
            println!("[fail] oxfile 生成失败: {e}");
            failed += 1;
        }
    }
    if failed == 0 {
        println!("doctor: 全部通过（{}）", cfg_path.display());
    }
    failed
}


/// 代理 oxmgr CLI；oxmgr 不在 PATH 时报清晰错误并提示安装。
fn run_oxmgr(args: &[&str]) -> i32 {
    match Command::new("oxmgr").args(args).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("oxmgr 执行失败: {e} — 请先安装 OxMgr 并加入 PATH（npm install -g oxmgr，见 https://github.com/Vladimir-Urik/OxMgr#install）");
            1
        }
    }
}
