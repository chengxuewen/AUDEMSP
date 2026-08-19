//! host.toml → oxfile.toml 翻译器（Task A2 + C1）。
//!
//! 输入 host.toml 文本，输出 OxMgr oxfile.toml 文本（`version = 1` + `[defaults]` +
//! `[[apps]]`，字段对齐官方 [OXFILE.md](https://github.com/Vladimir-Urik/OxMgr)）。
//! apps 含 7 类 host 进程 + 每 camera 一个 capturer 实例 + 每 stream 一个 streamer
//! 实例（command 参数化）。Phase A 输出占位进程骨架；C1 起 capturer 实例追加
//! `--config`/`--token` 绝对路径（`to_oxfile_in_dir`），真实命令逐 Phase 替换。

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// host.toml 解析模型（Phase A 子集：只需 cameras/streams 做实例化）。
#[derive(Debug, Default, Deserialize)]
struct HostConfig {
    #[serde(default)]
    cameras: Vec<Camera>,
    #[serde(default)]
    streams: Vec<Stream>,
    #[serde(default)]
    record: Option<RecordSection>,
    #[serde(default)]
    signaling: Option<SignalingSection>,
}

#[derive(Debug, Deserialize)]
struct Camera {
    id: String,
    /// 采集源（缺省 "stub"；v4l2/mipi 后接）。
    #[serde(default)]
    source: Option<String>,
    /// 帧率（缺省 30）。
    #[serde(default)]
    fps: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct Stream {
    id: String,
    /// 引用的相机 id（缺省 = 流 id 自身，topic camera/<id> 直连）。
    #[serde(default)]
    camera: Option<String>,
    /// 编码格式（缺省 vp8；对齐 field PublishOptions 默认）。
    #[serde(default)]
    codec: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecordSection {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    out_dir: Option<String>,
}

/// `[signaling]` 段（D1: agent 网关本地端口）。
#[derive(Debug, Deserialize)]
struct SignalingSection {
    #[serde(default)]
    local_port: Option<u16>,
}

/// 网关本地端口（[signaling] local_port；缺省 None → agent 内置 17980）。
pub fn signaling_local_port(cfg: &str) -> Result<Option<u16>, String> {
    let cfg: HostConfig = toml::from_str(cfg).map_err(|e| format!("host.toml 解析失败: {e}"))?;
    Ok(cfg.signaling.and_then(|s| s.local_port))
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
/// 无路径变体：capturer 实例仅 `--camera <id>`（A2 形态，doctor/测试用）。
pub fn to_oxfile(cfg: &str) -> Result<String, String> {
    to_oxfile_with_paths(cfg, Path::new(""), Path::new(""))
}

/// host.toml → oxfile.toml，capturer 实例追加 `--config <dir>/etc/host.toml`
/// 与 `--token <dir>/etc/link/<cam>.token` 绝对路径（Task C1）。
pub fn to_oxfile_in_dir(cfg: &str, dir: &Path) -> Result<String, String> {
    let config_path = std::path::absolute(dir.join("etc").join("host.toml"))
        .unwrap_or_else(|_| dir.join("etc").join("host.toml"));
    let token_dir = std::path::absolute(dir.join("etc").join("link"))
        .unwrap_or_else(|_| dir.join("etc").join("link"));
    to_oxfile_with_paths(cfg, &config_path, &token_dir)
}

fn to_oxfile_with_paths(cfg: &str, config_path: &Path, token_dir: &Path) -> Result<String, String> {
    let (cameras, streams) = camera_and_stream_ids(cfg)?;

    let mut out = String::from("version = 1\n\n[defaults]\nnamespace = \"host\"\nrestart_policy = \"always\"\n\n");

    for name in FIXED_APPS {
        let mut cmd = exe_cmd(name);
        // C3: recorder 固定 app 与 capturer/streamer 同形追加 --config/--token
        // （订阅 camera/* 录制; 令牌文件 recorder.token）。
        if name == "host-recorder" && !config_path.as_os_str().is_empty() {
            cmd.push_str(&format!(
                " --config {} --token {}/recorder.token",
                config_path.display(),
                token_dir.display()
            ));
        }
        // D1: agent 网关本地端口（[signaling] local_port 配置；缺省 agent 内置 17980）
        if name == "host-agent" {
            if let Some(port) = signaling_local_port(cfg)? {
                cmd.push_str(&format!(" --port {port}"));
            }
        }
        // C4: recorder [record] enabled=false 时按设计 exit 0 — 在 oxmgr
        // restart_policy=always 下会重启风暴; 改 on_failure（崩溃重启，干净退出不重启）。
        let policy = if name == "host-recorder" { "on_failure" } else { "always" };
        push_app(&mut out, name, &cmd, policy);
    }
    for cam in &cameras {
        let name = instance_name("host-capturer", cam, cameras.len() > 1);
        let mut cmd = format!("{} --camera {}", exe_cmd("host-capturer"), cam);
        if !config_path.as_os_str().is_empty() {
            cmd.push_str(&format!(
                " --config {} --token {}/{}.token",
                config_path.display(),
                token_dir.display(),
                cam
            ));
        }
        push_app(&mut out, &name, &cmd, "always");
    }
    for stream in &streams {
        let name = instance_name("host-streamer", stream, streams.len() > 1);
        let mut cmd = format!("{} --stream {}", exe_cmd("host-streamer"), stream);
        if !config_path.as_os_str().is_empty() {
            cmd.push_str(&format!(
                " --config {} --token {}/{}.token",
                config_path.display(),
                token_dir.display(),
                stream
            ));
        }
        push_app(&mut out, &name, &cmd, "always");
    }
    Ok(out)
}

/// 提取 cameras/streams 的 id 列表（host init 生成 ros_bridge.yaml 复用，单一解析点）。
pub fn camera_and_stream_ids(cfg: &str) -> Result<(Vec<String>, Vec<String>), String> {
    let cfg: HostConfig = toml::from_str(cfg).map_err(|e| format!("host.toml 解析失败: {e}"))?;
    Ok((
        cfg.cameras.into_iter().map(|c| c.id).collect(),
        cfg.streams.into_iter().map(|s| s.id).collect(),
    ))
}

/// 相机配置（capturer 消费；source/fps 缺省 stub/30）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraConfig {
    pub id: String,
    pub source: String,
    pub fps: u32,
}

/// 解析全部相机配置（C1 capturer 用；`camera_and_stream_ids` 保持 A2/B3 消费面）。
/// fps=0 拒绝（generator.start(0) 线程内 panic → 静默挂起，C1 审查发现）。
pub fn camera_configs(cfg: &str) -> Result<Vec<CameraConfig>, String> {
    let cfg: HostConfig = toml::from_str(cfg).map_err(|e| format!("host.toml 解析失败: {e}"))?;
    let mut out = Vec::with_capacity(cfg.cameras.len());
    for c in cfg.cameras {
        let fps = c.fps.unwrap_or(30);
        if fps == 0 {
            return Err(format!("host.toml 解析失败: 相机 {} fps=0 无效（须 > 0）", c.id));
        }
        out.push(CameraConfig {
            id: c.id,
            source: c.source.unwrap_or_else(|| "stub".into()),
            fps,
        });
    }
    Ok(out)
}
/// 录制配置（recorder 进程消费；[record] 段缺省 disabled + 默认输出目录）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordConfig {
    pub enabled: bool,
    pub out_dir: PathBuf,
}

/// 默认录制输出目录（host.toml [record] out_dir 可覆盖；开发缺省在 /tmp）。
const DEFAULT_RECORD_DIR: &str = "/tmp/mediaservo-recordings";

/// 解析录制配置（C3 recorder 用）。缺省: disabled + /tmp/mediaservo-recordings。
pub fn record_config(cfg: &str) -> Result<RecordConfig, String> {
    let cfg: HostConfig = toml::from_str(cfg).map_err(|e| format!("host.toml 解析失败: {e}"))?;
    let rec = cfg.record.unwrap_or(RecordSection { enabled: None, out_dir: None });
    Ok(RecordConfig {
        enabled: rec.enabled.unwrap_or(false),
        out_dir: PathBuf::from(rec.out_dir.unwrap_or_else(|| DEFAULT_RECORD_DIR.to_string())),
    })
}


/// 按 id 查单个相机配置（不存在 → Ok(None)）。
pub fn camera_config(cfg: &str, id: &str) -> Result<Option<CameraConfig>, String> {
    Ok(camera_configs(cfg)?.into_iter().find(|c| c.id == id))
}

/// 流配置（streamer 消费；camera/codec 缺省 id/vp8）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamConfig {
    pub id: String,
    /// 引用的相机 id（决定 FrameBus topic camera/<id>）。
    pub camera: String,
    /// 编码格式（对齐 field PublishOptions: vp8/h264/vp9/av1）。
    pub codec: String,
}

/// 解析全部流配置（C2 streamer 用）。
pub fn stream_configs(cfg: &str) -> Result<Vec<StreamConfig>, String> {
    let cfg: HostConfig = toml::from_str(cfg).map_err(|e| format!("host.toml 解析失败: {e}"))?;
    Ok(cfg
        .streams
        .into_iter()
        .map(|s| {
            let id = s.id.clone();
            StreamConfig {
                id,
                camera: s.camera.unwrap_or_else(|| s.id),
                codec: s.codec.unwrap_or_else(|| "vp8".into()),
            }
        })
        .collect())
}

/// 按 id 查单个流配置（不存在 → Ok(None)）。
pub fn stream_config(cfg: &str, id: &str) -> Result<Option<StreamConfig>, String> {
    Ok(stream_configs(cfg)?.into_iter().find(|s| s.id == id))
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

fn push_app(out: &mut String, name: &str, command: &str, restart_policy: &str) {
    out.push_str(&format!(
        "[[apps]]\nname = \"{name}\"\ncommand = \"{command}\"\nrestart_policy = \"{restart_policy}\"\n\n"
    ));
}