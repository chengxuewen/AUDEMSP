//! Multiprocess 骨架测试（Task A1）：9 个 bin 声明 + 占位进程生命周期。
//!
//! - all_bins_declared: Cargo.toml 必须声明 host / host-agent / host-capturer /
//!   host-streamer / host-recorder / host-controller / host-emergency / host-audio / host-legacy
//! - placeholder_blocks_and_exits_on_signal: host-capturer 占位进程打印就绪 →
//!   阻塞存活 → SIGTERM → 退出码 0

use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[test]
fn all_bins_declared() {
    // 读取 Cargo.toml [[bin]] 段：必须含 host, host-agent, host-capturer,
    // host-streamer, host-recorder, host-controller, host-emergency, host-audio, host-legacy
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    for bin in [
        "host",
        "host-agent",
        "host-capturer",
        "host-streamer",
        "host-recorder",
        "host-controller",
        "host-emergency",
        "host-audio",
        "host-legacy",
    ] {
        assert!(
            manifest.contains(&format!("name = \"{bin}\"")),
            "missing bin {bin}"
        );
    }
}

#[test]
fn placeholder_blocks_and_exits_on_signal() {
    // spawn host-capturer 二进制（env CARGO_BIN_EXE_host-capturer）→ 等 200ms
    // → 断言进程存活（输出含 "capturer placeholder ready"）→ SIGTERM → 退出码 0
    let log = tempfile::NamedTempFile::new().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_host-capturer"))
        .stdout(Stdio::from(log.reopen().unwrap()))
        .spawn()
        .expect("spawn host-capturer");

    thread::sleep(Duration::from_millis(200));
    assert!(
        child.try_wait().unwrap().is_none(),
        "host-capturer exited prematurely"
    );

    let mut out = String::new();
    log.reopen().unwrap().read_to_string(&mut out).unwrap();
    assert!(
        out.contains("capturer placeholder ready"),
        "stdout missing ready line, got: {out:?}"
    );

    // SIGTERM → 优雅退出（退出码 0）
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    let status = child.wait().expect("wait host-capturer");
    assert_eq!(status.code(), Some(0), "expected graceful exit 0, got {status:?}");
}
