//! Task C2: host-streamer 进程测试 — FrameBus 订阅 → WebRTC 推流（外部 mediasoup server）。
//!
//! - `bad_args_exit_2_with_usage`: 缺参/坏参 → exit 2 + stderr 用法提示
//! - `streamer_pushes_framebus_frames_to_sfu`: capturer（真进程）+ streamer（真进程）
//!   → 外部 Docker server 收流（streamer 日志 `streamer stats: bytes_sent>0 且
//!   frames_encoded>0`，对齐 field push_e2e D4 证据模式）→ SIGTERM 双双优雅退出 0
//!
//! 前置: `SFU_E2E_WS_URL` 指向外部 mediasoup server（C21 纯外部模式，不 import
//! server 类型）; C25: 跑前清 `/tmp/iceoryx2` + `/dev/shm/iox2_*`。

#![cfg(target_os = "linux")]

use std::io::Read;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;

use mediaservo_link::{
    CapabilityToken, Ed25519SigningKey, Ed25519VerifyingKey, NodeAcl, NodeId, Role, TokenFile,
};

// 测试用 Ed25519 密钥对（openssl 生成，仅测试用；与 link/deck/capturer 测试同源）。
const PRIV_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIObCg8b+Le6kKOI/+pE+4+YhXUlr6X6h7q8p/MjvHmXT\n-----END PRIVATE KEY-----\n";
const PUB_PEM: &str = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAgXprEbnahCZoZtLpiUqR0ruqtzEfRXk/Gl/6F6PEm4o=\n-----END PUBLIC KEY-----\n";

fn token(role: Role, node_id: &str) -> (CapabilityToken, Ed25519VerifyingKey) {
    let sk = Ed25519SigningKey::from_pem(PRIV_PEM.as_bytes());
    let vk = Ed25519VerifyingKey::from_pem(PUB_PEM.as_bytes());
    let acl = NodeAcl::for_role(NodeId::new(node_id), role);
    (CapabilityToken::sign(&acl, 3600, &sk).unwrap(), vk)
}

/// C25: 清 iceoryx2 0.9.3 运行时残留（/tmp/iceoryx2 + /dev/shm/iox2_*）。
fn cleanup_iceoryx() {
    let _ = std::fs::remove_dir_all("/tmp/iceoryx2");
    if let Ok(entries) = std::fs::read_dir("/dev/shm") {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with("iox2_") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

fn ws_url() -> String {
    std::env::var("SFU_E2E_WS_URL").unwrap_or_else(|_| {
        panic!(
            "SFU_E2E_WS_URL 未设置 — streamer e2e 需连外部 mediasoup server (C21);
            例: SFU_E2E_WS_URL=ws://127.0.0.1:9800/ws"
        )
    })
}

#[test]
fn bad_args_exit_2_with_usage() {
    for args in [
        vec![],                   // 全缺
        vec!["--stream"],         // 缺值
        vec!["--stream", "s0"],   // 缺 config/token
        vec!["--bogus", "x"],     // 未知参数
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_host-streamer"))
            .args(&args)
            .output()
            .expect("spawn host-streamer");
        assert_eq!(out.status.code(), Some(2), "args {args:?} 应 exit 2");
        assert!(
            !out.stderr.is_empty(),
            "args {args:?} stderr 应有用法提示, got: {:?}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// I1 审查: fps != 30 必须在启动即拒绝（推流编码器内置 30fps，PIT-64 类
/// rate-control 失配）。纯配置校验路径 — fps 检查先于令牌/信令，无需 server。
#[test]
fn rejects_non_30_fps_with_clear_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg_path = dir.path().join("host.toml");
    std::fs::write(
        &cfg_path,
        "[[cameras]]\nid = \"cam0\"\nfps = 25\n[[streams]]\nid = \"s0\"\ncamera = \"cam0\"\n",
    )
    .expect("write host.toml");
    let tok_path = dir.path().join("t.token");
    std::fs::write(&tok_path, b"garbage").expect("write token");
    let out = Command::new(env!("CARGO_BIN_EXE_host-streamer"))
        .args([
            "--stream",
            "s0",
            "--config",
            cfg_path.to_str().expect("cfg utf8"),
            "--token",
            tok_path.to_str().expect("token utf8"),
        ])
        .output()
        .expect("spawn host-streamer");
    assert_eq!(out.status.code(), Some(1), "fps=25 应 exit 1");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("fps"), "应指明 fps 字段, got: {stderr}");
    assert!(stderr.contains("25"), "应含实际值 25, got: {stderr}");
}

/// 读取子进程日志（stdout+stderr 合并到同一文件）。
fn read_log(file: &tempfile::NamedTempFile) -> String {
    let mut out = String::new();
    file.reopen()
        .expect("reopen log")
        .read_to_string(&mut out)
        .expect("read log");
    out
}

/// 轮询日志直到出现 needle（≤10s）。
fn wait_for(log: &tempfile::NamedTempFile, needle: &str) {
    for _ in 0..20 {
        if read_log(log).contains(needle) {
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    panic!("未见 {needle:?}, log:\n{}", read_log(log));
}

/// 轮询日志直到 stats 行 bytes_sent>0 且 frames_encoded>0（≤30s, D4 证据模式）。
fn wait_for_flow(log: &tempfile::NamedTempFile) -> String {
    for _ in 0..60 {
        for line in read_log(log).lines() {
            let Some(rest) = line.split("streamer stats:").nth(1) else {
                continue;
            };
            let bytes: u64 = rest
                .split_whitespace()
                .find_map(|t| t.strip_prefix("bytes_sent="))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let frames: u64 = rest
                .split_whitespace()
                .find_map(|t| t.strip_prefix("frames_encoded="))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if bytes > 0 && frames > 0 {
                return format!("bytes_sent={bytes} frames_encoded={frames}");
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    panic!("30s 内未见流证据, log:\n{}", read_log(log));
}

/// E2E: capturer（真进程发布 camera/cam0）→ streamer（真进程订阅 + 推流）
/// → 外部 mediasoup server 收流（bytes_sent>0 且 frames_encoded>0）。
#[tokio::test]
async fn streamer_pushes_framebus_frames_to_sfu() {
    cleanup_iceoryx();
    let _url = ws_url();
    let dir = tempfile::tempdir().expect("tempdir");
    let pid = std::process::id();
    let stream_id = format!("s{pid}-stream");

    // host.toml: cam0 stub 30fps + 唯一流（显式 camera 引用，验证缺省外路径）
    let cfg_path = dir.path().join("host.toml");
    std::fs::write(
        &cfg_path,
        format!(
            "[[cameras]]\nid = \"cam0\"\nsource = \"stub\"\nfps = 30\n\
             [[streams]]\nid = \"{stream_id}\"\ncamera = \"cam0\"\ncodec = \"vp8\"\n"
        ),
    )
    .expect("write host.toml");

    // 令牌: capturer=Capture（可发布 camera/*），streamer=Recorder（可订阅 camera/*）
    let (cap_tok, cap_vk) = token(Role::Capture, &format!("capture-{pid}"));
    let cap_path = dir.path().join("cam0.token");
    std::fs::write(&cap_path, TokenFile::encode(&cap_tok, &cap_vk)).expect("write cap token");
    let (str_tok, str_vk) = token(Role::Recorder, &format!("streamer-{pid}"));
    let str_path = dir.path().join("streamer.token");
    std::fs::write(&str_path, TokenFile::encode(&str_tok, &str_vk)).expect("write str token");

    // capturer 进程（先起：streamer 首帧 gate 依赖发布端）
    let cap_log = tempfile::NamedTempFile::new().expect("cap log");
    let mut capturer = Command::new(env!("CARGO_BIN_EXE_host-capturer"))
        .args([
            "--camera",
            "cam0",
            "--config",
            cfg_path.to_str().expect("cfg utf8"),
            "--token",
            cap_path.to_str().expect("cap token utf8"),
        ])
        .stdout(Stdio::from(cap_log.reopen().expect("reopen cap log")))
        .stderr(Stdio::from(cap_log.reopen().expect("reopen cap log")))
        .spawn()
        .expect("spawn host-capturer");
    wait_for(&cap_log, "capturer ready");

    // streamer 进程
    let str_log = tempfile::NamedTempFile::new().expect("str log");
    let mut streamer = Command::new(env!("CARGO_BIN_EXE_host-streamer"))
        .args([
            "--stream",
            &stream_id,
            "--config",
            cfg_path.to_str().expect("cfg utf8"),
            "--token",
            str_path.to_str().expect("str token utf8"),
        ])
        .stdout(Stdio::from(str_log.reopen().expect("reopen str log")))
        .stderr(Stdio::from(str_log.reopen().expect("reopen str log")))
        .spawn()
        .expect("spawn host-streamer");
    wait_for(&str_log, "streamer ready");

    // 流证据: 出站统计 bytes_sent>0 且 frames_encoded>0（server 已收帧）
    let evidence = wait_for_flow(&str_log);
    eprintln!("[streamer_e2e] 流证据: {evidence}");

    // SIGTERM → 双双优雅退出 0
    unsafe { libc::kill(streamer.id() as i32, libc::SIGTERM) };
    let st = streamer.wait().expect("wait streamer");
    assert_eq!(st.code(), Some(0), "streamer 应优雅退出 0, got {st:?}");
    unsafe { libc::kill(capturer.id() as i32, libc::SIGTERM) };
    let ct = capturer.wait().expect("wait capturer");
    assert_eq!(ct.code(), Some(0), "capturer 应优雅退出 0, got {ct:?}");
}
