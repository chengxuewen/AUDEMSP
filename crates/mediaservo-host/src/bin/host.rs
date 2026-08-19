//! host CLI: 业务视图运维入口（Phase A）。
//!
//! 子命令（`std::env::args` 手工解析，Phase A 不引入 clap）：
//! - `host init <dir>`        — 生成 `etc/host.toml` 模板 + `etc/link/signing.pem`
//!   （Ed25519 PKCS#8 keypair，0600）+ `etc/link/ros_bridge.yaml`（B3：ROS 节点
//!   配置单一来源——topic 清单 + 令牌路径，从 host.toml 相机/流清单导出）
//! - `host start --dir <dir>` — 读 etc/host.toml → translate → 写 run/oxfile.toml
//!   → `oxmgr apply run/oxfile.toml` 拉起全部 host 进程
//! - `host stop --dir <dir>`  — `oxmgr stop run/oxfile.toml` + `oxmgr delete run/oxfile.toml`
//!   （config 目标解析 oxfile 内全部 app，幂等；动词见 OxMgr docs/CLI.md）
//! - `host status --dir <dir>`— `oxmgr list --json` 过滤 `namespace == "host"` 输出状态表
//! - `host doctor --dir <dir>`— 环境诊断（oxmgr 可用 / host.toml 解析 / oxfile 生成），
//!   退出码 = 失败检查数
//! - `host token issue ...`  — 用 etc/link/signing.pem 签发能力令牌（C4 最小签发，
//!   G1 全量签发前的 e2e 需用）
//! - `host version`           — 打印版本号
//!
//! OxMgr 动词核对（C11/C18，来源 .refinfo/OxMgr/docs/CLI.md + SKILL.md）：
//! `apply <config>` / `list [--json]` / `stop <name|id|config>` / `delete <name|id|config>`
//! （**无** `stop/delete --namespace` 旗标；config 目标自动解析 oxfile 内全部 app，
//! 见 CLI.md "Lifecycle" 段）。

use std::path::{Path, PathBuf};
use std::process::Command;

use mediaservo_link::{
    CapabilityToken, Ed25519SigningKey, Ed25519VerifyingKey, NodeAcl, NodeId, Role, TokenFile,
};
use ed25519_dalek::pkcs8::{DecodePrivateKey, EncodePublicKey};
use pkcs8::LineEnding;

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

const USAGE: &str = "用法: host <init|start|stop|status|doctor|token|version> [--dir <dir>]";

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
        "token" => cmd_token(&mut args),
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

/// `host init <dir>`: 生成 etc/host.toml 模板 + etc/link/signing.pem（Ed25519，0600）
/// + etc/link/ros_bridge.yaml（topic 清单 + 令牌路径，从 host.toml 导出）。
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

    // 生成 ros_bridge.yaml（B3）：topic 清单 + 令牌路径，ROS 节点配置单一来源。
    // 从已存在的 host.toml 解析（init 刚写入模板或用户已编辑），解析失败即报错——
    // 静默写空清单会让 ROS 节点连不上任何 topic（C15）。
    let cfg_text = match std::fs::read_to_string(&cfg_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("init: 读取 {} 失败: {e}", cfg_path.display());
            return 1;
        }
    };
    let (cameras, streams) = match mediaservo_host::translate::camera_and_stream_ids(&cfg_text) {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!("init: {e}");
            return 1;
        }
    };
    let token_path = std::path::absolute(link.join("ros-vision.token"))
        .unwrap_or_else(|_| link.join("ros-vision.token"))
        .to_string_lossy()
        .into_owned();
    let yaml = mediaservo_link::bridge::ros_bridge(&cameras, &streams, &token_path);
    let ros_path = link.join("ros_bridge.yaml");
    if let Err(e) = std::fs::write(&ros_path, &yaml) {
        eprintln!("init: 写入 {} 失败: {e}", ros_path.display());
        return 1;
    }
    println!("已生成 {}", ros_path.display());
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
    let ox = match mediaservo_host::translate::to_oxfile_in_dir(&cfg, &dir) {
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

/// `host token issue`: 用 `etc/link/signing.pem`（host init 生成，PKCS#8 Ed25519）签发
/// 能力令牌（C4 最小签发；G1 全量签发前 e2e 需用）。
///
/// 令牌写为 TokenFile 单文件（内嵌公钥 + JWT，MSTK 格式）——与 translate.rs
/// oxfile `--token` 引用的 `<cam>.token`/`<stream>.token`/`recorder.token` 同构。
/// 缺省 TTL 10 年（D-H10 固定令牌策略）。
const TOKEN_USAGE: &str = "用法: host token issue --role <capture|processor|pusher|puller|recorder|control|perception> --node <id> [--topic <T>]... --out <path> [--dir <dir>]";
/// D-H10 固定令牌策略: 令牌长期有效，不随部署轮换。
const DEFAULT_TOKEN_TTL_SECS: u64 = 10 * 365 * 24 * 3600;

fn cmd_token(args: &mut impl Iterator<Item = String>) -> i32 {
    let Some(sub) = args.next() else {
        eprintln!("{TOKEN_USAGE}");
        return 2;
    };
    if sub != "issue" {
        eprintln!("未知 token 子命令: {sub}");
        eprintln!("{TOKEN_USAGE}");
        return 2;
    }
    cmd_token_issue(args)
}

fn cmd_token_issue(args: &mut impl Iterator<Item = String>) -> i32 {
    let mut role: Option<Role> = None;
    let mut node: Option<String> = None;
    let mut topics: Vec<String> = Vec::new();
    let mut out: Option<PathBuf> = None;
    let mut dir = PathBuf::from(".");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--role" => {
                let Some(v) = args.next() else {
                    eprintln!("--role 缺值");
                    return 2;
                };
                match parse_role(&v) {
                    Ok(r) => role = Some(r),
                    Err(e) => {
                        eprintln!("{e}");
                        return 2;
                    }
                }
            }
            "--node" => {
                let Some(v) = args.next() else {
                    eprintln!("--node 缺值");
                    return 2;
                };
                node = Some(v);
            }
            "--topic" => {
                let Some(v) = args.next() else {
                    eprintln!("--topic 缺值");
                    return 2;
                };
                topics.push(v);
            }
            "--out" => {
                let Some(v) = args.next() else {
                    eprintln!("--out 缺值");
                    return 2;
                };
                out = Some(PathBuf::from(v));
            }
            "--dir" => {
                let Some(v) = args.next() else {
                    eprintln!("--dir 缺值");
                    return 2;
                };
                dir = PathBuf::from(v);
            }
            _ => {
                eprintln!("未知参数: {arg}");
                eprintln!("{TOKEN_USAGE}");
                return 2;
            }
        }
    }
    let (Some(role), Some(node), Some(out)) = (role, node, out) else {
        eprintln!("缺少必填参数: --role/--node/--out");
        eprintln!("{TOKEN_USAGE}");
        return 2;
    };

    let pem_path = dir.join("etc").join("link").join("signing.pem");
    let pem = match std::fs::read(&pem_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("token: 读取 {} 失败: {e} — 先运行 host init <dir>", pem_path.display());
            return 1;
        }
    };
    let signing = match ed25519_dalek::SigningKey::from_pkcs8_pem(&String::from_utf8_lossy(&pem)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("token: {} 不是有效 PKCS#8 Ed25519 私钥: {e}", pem_path.display());
            return 1;
        }
    };
    let acl = match build_acl(NodeId::new(node), role, topics) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("token: {e}");
            return 2;
        }
    };
    let sk = Ed25519SigningKey::from_pem(&pem);
    let token = match CapabilityToken::sign(&acl, DEFAULT_TOKEN_TTL_SECS, &sk) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("token: 签名失败: {e}");
            return 1;
        }
    };
    // TokenFile 内嵌 verifying key PEM（派生自私钥，同一密钥对）
    let vk_pem = match signing.verifying_key().to_public_key_pem(LineEnding::LF) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("token: 导出公钥失败: {e}");
            return 1;
        }
    };
    let vk = Ed25519VerifyingKey::from_pem(vk_pem.as_bytes());
    let bytes = TokenFile::encode(&token, &vk);
    if let Err(e) = std::fs::write(&out, bytes) {
        eprintln!("token: 写入 {} 失败: {e}", out.display());
        return 1;
    }
    println!("已签发 {:?} 令牌 → {}（node={} ttl={}s）", role, out.display(), acl.node_id.as_str(), DEFAULT_TOKEN_TTL_SECS);
    0
}

/// 角色名 → Role（小写变体名; 未知 → 明确报错）。
fn parse_role(s: &str) -> Result<Role, String> {
    match s.to_ascii_lowercase().as_str() {
        "capture" => Ok(Role::Capture),
        "processor" => Ok(Role::Processor),
        "pusher" => Ok(Role::Pusher),
        "puller" => Ok(Role::Puller),
        "recorder" => Ok(Role::Recorder),
        "control" => Ok(Role::Control),
        "perception" => Ok(Role::Perception),
        _ => Err(format!("未知角色: {s}（可选: capture/processor/pusher/puller/recorder/control/perception）")),
    }
}

/// --topic 缺省 = ACL 矩阵缺省（NodeAcl::for_role）；显式 --topic 覆盖角色
/// **单方向** ACL 列表（发布型角色 → publish_allow，订阅型角色 → subscribe_allow）。
/// 双方向/无方向角色（processor/perception/control/puller）显式 --topic 报错——
/// C 阶段最小签发只服务单方向角色，双方向留 G1 全量签发。
fn build_acl(node_id: NodeId, role: Role, topics: Vec<String>) -> Result<NodeAcl, String> {
    if topics.is_empty() {
        return Ok(NodeAcl::for_role(node_id, role));
    }
    let base = NodeAcl::for_role(node_id, role);
    match (base.publish_allow.is_empty(), base.subscribe_allow.is_empty()) {
        (false, true) => Ok(NodeAcl { publish_allow: topics, subscribe_allow: vec![], ..base }),
        (true, false) => Ok(NodeAcl { publish_allow: vec![], subscribe_allow: topics, ..base }),
        _ => Err(format!(
            "角色 {:?} 为双方向/无方向 ACL — 显式 --topic 仅支持单方向角色（capture/pusher/recorder）; 省略 --topic 使用矩阵缺省",
            role
        )),
    }
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
            println!("[fail] oxfile 生成失败: 无配置可翻译（host.toml 不可读）");
            return failed + 2; // ②③ 均因无配置失败，各打一条 [fail] 与计数一致
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
