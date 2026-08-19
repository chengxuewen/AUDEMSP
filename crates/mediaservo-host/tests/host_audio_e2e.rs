//! Task H2: host-audio 进程测试 — 音频会议参与者进程（tone 合成源，stub 麦克风）。
//!
//! - `bad_args_exit_2_with_usage`: 缺参/坏参（非 audio- 房间）→ exit 2 + stderr 用法
//! - `audio_process_publishes_and_exits_clean`: 直连外部 mediasoup server → 加入
//!   `audio-<id>` 房间 → publish 1 路 opus（tone）→ `--duration` 到期优雅退出 0；
//!   server 侧房间存在（间接证据: 进程日志 published producer + 退出码 0）。
//!   PIT-105: libwebrtc 音频编码不产包 — RTP 字节断言待修复后启用。
//!
//! 前置: `SFU_E2E_WS_URL` 指向外部 mediasoup server（C21）+ `SFU_E2E_PSK`。

#![cfg(target_os = "linux")]

use std::process::{Command, Stdio};
use std::time::Duration;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_host-audio")
}

fn psk() -> String {
    std::env::var("SFU_E2E_PSK").unwrap_or_else(|_| "e2e-host-sfu-psk".to_string())
}

fn ws_url() -> String {
    std::env::var("SFU_E2E_WS_URL").unwrap_or_else(|_| {
        panic!("SFU_E2E_WS_URL 未设置 — host-audio e2e 需连外部 mediasoup server (C21)")
    })
}

/// 跑 host-audio 并返回 (exit_code, 日志文本)。stdout/stderr 合并捕获。
fn run_audio(args: &[&str], timeout_secs: u64) -> (i32, String) {
    let mut child = Command::new(bin())
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn host-audio");
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let done = std::thread::spawn(move || {
        use std::io::Read;
        let mut out_s = String::new();
        let mut err_s = String::new();
        let _ = out.read_to_string(&mut out_s);
        let _ = err.read_to_string(&mut err_s);
        format!("{out_s}{err_s}")
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            panic!("host-audio 超时未退出（{timeout_secs}s）");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let log = done.join().unwrap();
    (status.code().unwrap_or(-1), log)
}

#[test]
fn bad_args_exit_2_with_usage() {
    // 缺 --room
    let (code, log) = run_audio(&[], 10);
    assert_eq!(code, 2, "缺参必须 exit 2: {log}");
    assert!(log.contains("用法"), "必须输出用法: {log}");

    // 非 audio- 房间
    let (code, log) = run_audio(&["--room", "ms-car1", "--server", "ws://127.0.0.1:1/ws", "--psk", "x"], 10);
    assert_eq!(code, 2, "非 audio- 房间必须 exit 2: {log}");
    assert!(log.contains("audio-<vehicle>"), "必须提示房间约定: {log}");
}

/// 直连 server: join → publish（tone）→ --duration 到期 → exit 0。
/// PIT-105: RTP 字节>0 断言待 libwebrtc 音频编码修复后启用。
#[test]
fn audio_process_publishes_and_exits_clean() {
    let url = ws_url();
    let room = format!("audio-e2e-proc-{}", std::process::id());
    let (code, log) = run_audio(
        &[
            "--server", &url,
            "--psk", &psk(),
            "--room", &room,
            "--duration", "6",
        ],
        30,
    );
    assert_eq!(code, 0, "host-audio 必须优雅退出 0: {log}");
    assert!(
        log.contains(&format!("已加入音频房间 {room}")),
        "必须加入音频房间: {log}"
    );
    assert!(
        log.contains("published producer"),
        "必须成功 publish opus: {log}"
    );
    assert!(log.contains("--duration 到期"), "必须按 duration 退出: {log}");
}
