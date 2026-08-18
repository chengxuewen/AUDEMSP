//! host.toml → oxfile.toml 翻译器（Task A2）。
//!
//! 输入 host.toml 文本，输出 OxMgr oxfile.toml 文本（`version = 1` + `[defaults]` +
//! `[[apps]]`，字段对齐官方 [OXFILE.md](https://github.com/Vladimir-Urik/OxMgr)）。
//! apps 含 7 类 host 进程 + 每 camera 一个 capturer 实例 + 每 stream 一个 streamer
//! 实例（command 参数化）。Phase A 输出占位进程骨架，真实命令在后续 Phase 替换。

use serde::Deserialize;

/// host.toml 解析模型（Phase A 子集：只需 cameras/streams 做实例化）。
#[derive(Debug, Default, Deserialize)]
struct HostConfig {
    #[serde(default)]
    cameras: Vec<Camera>,
    #[serde(default)]
    streams: Vec<Stream>,
}

#[derive(Debug, Deserialize)]
struct Camera {
    id: String,
}

#[derive(Debug, Deserialize)]
struct Stream {
    id: String,
}

/// 固定 5 类进程（无参数实例）。
const FIXED_APPS: [&str; 5] = [
    "host-agent",
    "host-recorder",
    "host-controller",
    "host-emergency",
    "host-audio",
];

/// host.toml → oxfile.toml 文本。
///
/// 单实例用类型名（如 `host-capturer`），多实例追加实例 id（如 `host-capturer-cam1`）
/// ——OxMgr validate 拒绝重复 app 名（CLI.md "duplicate app name" 硬错误）。
pub fn to_oxfile(cfg: &str) -> Result<String, String> {
    let cfg: HostConfig = toml::from_str(cfg).map_err(|e| format!("host.toml 解析失败: {e}"))?;

    let mut out = String::from("version = 1\n\n[defaults]\nnamespace = \"host\"\nrestart_policy = \"always\"\n\n");

    for name in FIXED_APPS {
        push_app(&mut out, name, &exe_cmd(name));
    }
    for cam in &cfg.cameras {
        let name = instance_name("host-capturer", &cam.id, cfg.cameras.len() > 1);
        push_app(&mut out, &name, &format!("{} --camera {}", exe_cmd("host-capturer"), cam.id));
    }
    for stream in &cfg.streams {
        let name = instance_name("host-streamer", &stream.id, cfg.streams.len() > 1);
        push_app(&mut out, &name, &format!("{} --stream {}", exe_cmd("host-streamer"), stream.id));
    }
    Ok(out)
}

/// 单实例用类型名，多实例追加实例 id 保证名字唯一。
fn instance_name(kind: &str, id: &str, plural: bool) -> String {
    if plural {
        format!("{kind}-{id}")
    } else {
        kind.to_string()
    }
}

/// 进程可执行文件路径：与 host CLI 同目录（同 target 产物）；测试运行时
/// current_exe 在 deps/ 下，回落裸名（test 仅断言命令行子串）。
// ponytail: 裸名依赖 PATH；部署阶段（A4 脚本/打包）再固化绝对路径。
fn exe_cmd(name: &str) -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name).to_string_lossy().into_owned()))
        .unwrap_or_else(|| name.to_string())
}

fn push_app(out: &mut String, name: &str, command: &str) {
    out.push_str(&format!("[[apps]]\nname = \"{name}\"\ncommand = \"{command}\"\n\n"));
}
