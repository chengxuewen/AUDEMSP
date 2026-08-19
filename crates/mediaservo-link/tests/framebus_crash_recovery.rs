//! C5 核心: FrameBus 订阅端跨发布端崩溃重启恢复（同句柄收帧）。
//!
//! 场景（生产等价）: 长生命周期订阅端（host-streamer/host-recorder）挂载后，
//! 发布端（capturer）被 SIGKILL（崩溃路径）→ 新发布端进程同 topic 重发布 →
//! **旧订阅端句柄必须恢复收帧**（不是新建订阅端才收得到）。
//!
//! C25: 跑前清 /tmp/iceoryx2 + /dev/shm/iox2_*（本测试自带清理）。

use std::process::Command;
use std::time::Duration;

use mediaservo_link::{
    CapabilityToken, Ed25519SigningKey, Ed25519VerifyingKey, FrameBus, FrameStream, FrameTopic,
    NodeAcl, NodeId, Role,
};

const PRIV_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIObCg8b+Le6kKOI/+pE+4+YhXUlr6X6h7q8p/MjvHmXT\n-----END PRIVATE KEY-----\n";
const PUB_PEM: &str = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAgXprEbnahCZoZtLpiUqR0ruqtzEfRXk/Gl/6F6PEm4o=\n-----END PUBLIC KEY-----\n";

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

/// 等流上收到 ≥n 帧（返回期间最大 seq）。
async fn wait_frames(stream: &FrameStream, n: u32, timeout: Duration) -> u64 {
    let deadline = std::time::Instant::now() + timeout;
    let mut seen = 0u32;
    let mut max_seq = 0u64;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), stream.recv()).await {
            Ok(Some(frame)) => {
                max_seq = max_seq.max(frame.meta().seq);
                seen += 1;
                if seen >= n {
                    return max_seq;
                }
            }
            Ok(None) => panic!("订阅流关停"),
            Err(_) => {}
        }
    }
    panic!("{timeout:?} 内只收到 {seen} 帧 (需 ≥{n}), max_seq={max_seq}");
}

/// 启动发布子进程（新进程 = 新 node + 新 publisher port），返回 Child。
fn spawn_pub_loop(topic: &str, frame_bytes: usize, fps: u64) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_framebus_pub_loop"))
        .arg(topic)
        .arg(frame_bytes.to_string())
        .arg(fps.to_string())
        .spawn()
        .expect("spawn framebus_pub_loop")
}

fn kill9(child: &mut std::process::Child) {
    let pid = child.id();
    let st = Command::new("kill").args(["-9", &pid.to_string()]).status().expect("kill");
    assert!(st.success(), "kill -9 {pid} 失败");
}

#[tokio::test]
async fn same_subscriber_recovers_across_publisher_sigkill() {
    // 生产参数: 1080p I420 帧 (3.1MB) @ 30fps（host-capturer 同规格）
    crash_recovery_run(3_110_400, 30).await;
}

#[tokio::test]
async fn same_subscriber_recovers_across_publisher_sigkill_small_frames() {
    // 小帧对照（低配环境快速回归）
    crash_recovery_run(64, 10).await;
}

async fn crash_recovery_run(frame_bytes: usize, fps: u64) {
    cleanup_iceoryx();
    let sk = Ed25519SigningKey::from_pem(PRIV_PEM.as_bytes());
    let vk = Ed25519VerifyingKey::from_pem(PUB_PEM.as_bytes());
    // Recorder 角色可订阅 camera/*
    let acl = NodeAcl::for_role(NodeId::new("crash-sub"), Role::Recorder);
    let tok = CapabilityToken::sign(&acl, 3600, &sk).unwrap();
    let bus = FrameBus::attach("", &tok, &vk).unwrap();
    // 唯一 topic（跨 run 隔离）
    let topic = FrameTopic::new(format!("camera/crash/{}/raw", std::process::id()));

    // ① 订阅端先挂载（生产时序：streamer/recorder 先于/同时于 capturer）
    let stream = bus.subscribe(&topic).unwrap();

    // ② 发布端 v1 收帧基线
    let mut pub1 = spawn_pub_loop(topic.as_str(), frame_bytes, fps);
    let seq_before = wait_frames(&stream, 3, Duration::from_secs(10)).await;

    // ③ SIGKILL 发布端 v1（崩溃路径）
    kill9(&mut pub1);
    let _ = pub1.wait();
    // 给 iceoryx2 一点时间让残留可见（新节点创建时清理）
    tokio::time::sleep(Duration::from_millis(500)).await;

    // ④ 发布端 v2 同 topic 重发布 → 同一订阅端句柄必须恢复收帧
    let mut pub2 = spawn_pub_loop(topic.as_str(), frame_bytes, fps);
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut resumed = false;
    let mut seen = 0u32;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), stream.recv()).await {
            Ok(Some(frame)) => {
                if frame.meta().seq < seq_before {
                    // 新实例 seq 归零（pub_loop 从 1 开始）→ 非旧连接残留帧
                    resumed = true;
                }
                seen += 1;
                if resumed && seen >= 3 {
                    break;
                }
            }
            Ok(None) => panic!("订阅流关停"),
            Err(_) => {}
        }
    }
    assert!(
        resumed,
        "发布端 SIGKILL 重启后同一订阅端句柄未恢复收帧（seq_before={seq_before}）— 订阅端 stale"
    );
    eprintln!("[crash_recovery] OK: 同句柄跨发布端崩溃恢复 (seq {seq_before} → 重启后新帧)");

    kill9(&mut pub2);
    let _ = pub2.wait();
}
