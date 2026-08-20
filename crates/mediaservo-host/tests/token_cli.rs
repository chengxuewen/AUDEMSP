//! Task C4: `host token issue` CLI 测试 — 最小令牌签发（G1 前置，C 阶段 e2e 需用）。
//!
//! 流程: `host init <dir>`（生成 etc/link/signing.pem PKCS#8）→
//! `host token issue --role <R> --node <id> [--topic <T>...] --out <path> [<dir>]
//! → TokenFile::decode 验签 + claims 断言（角色/节点/ACL 正确）。
//!
//! 负例: 未知角色 exit 2、缺 signing.pem exit 1、缺必填参数 exit 2。

use std::path::Path;
use std::process::{Command, Output};

use mediaservo_link::{Claims, Role, TokenFile};

fn host() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mediaservo-host"))
}

/// `host init <dir>` 生成 etc/host.toml + etc/link/signing.pem。
fn init(dir: &Path) {
    let out = host().arg("init").arg(dir).output().expect("spawn host init");
    assert!(
        out.status.success(),
        "host init 失败: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn issue(dir: &Path, args: &[&str]) -> Output {
    let mut cmd = host();
    cmd.arg("token").arg("issue");
    for a in args {
        cmd.arg(a);
    }
    cmd.arg(dir);
    cmd.output().expect("spawn host token issue")
}

fn decode_claims(path: &Path) -> Claims {
    let bytes = std::fs::read(path).expect("read token file");
    let (_vk, token) = TokenFile::decode(&bytes).expect("token file decode 应验签通过");
    token.verify(&_vk).expect("verify claims")
}

#[test]
fn issue_capture_token_roundtrip() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    let tok = dir.path().join("cam0.token");

    let out = issue(
        dir.path(),
        &[
            "--role", "capture", "--node", "cap-1",
            "--topic", "camera/cam0", "--out", tok.to_str().expect("utf8"),
        ],
    );
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(tok.exists(), "令牌文件应已写出");

    let claims = decode_claims(&tok);
    assert_eq!(claims.role, Role::Capture);
    assert_eq!(claims.node_id, "cap-1");
    assert_eq!(claims.acl.publish_allow, vec!["camera/cam0"]);
    assert!(claims.acl.subscribe_allow.is_empty(), "capture 无订阅权");
    assert!(claims.exp > 0, "exp 应已填充");
}

#[test]
fn issue_defaults_topics_per_role_matrix() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    let tok = dir.path().join("cap.token");

    // 无 --topic → ACL 矩阵缺省（Capture publish camera/*）
    let out = issue(dir.path(), &["--role", "capture", "--node", "n", "--out", tok.to_str().expect("utf8")]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let claims = decode_claims(&tok);
    assert_eq!(claims.acl.publish_allow, vec!["camera/*"]);
}

#[test]
fn issue_subscribe_role_puts_topics_in_subscribe_allow() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    let tok = dir.path().join("streamer.token");

    // streamer 角色: pusher（订阅 camera/*，发布 stats/*）
    let out = issue(
        dir.path(),
        &[
            "--role", "pusher", "--node", "stream-1",
            "--topic", "camera/cam0", "--out", tok.to_str().expect("utf8"),
        ],
    );
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let claims = decode_claims(&tok);
    assert_eq!(claims.role, Role::Pusher);
    assert_eq!(claims.acl.publish_allow, vec!["stats/*"], "pusher stats/* 发布权");
    assert_eq!(claims.acl.subscribe_allow, vec!["camera/cam0"]);
}

#[test]
fn issue_recorder_defaults_to_matrix_subscribe() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    let tok = dir.path().join("recorder.token");

    let out = issue(dir.path(), &["--role", "recorder", "--node", "rec-1", "--out", tok.to_str().expect("utf8")]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let claims = decode_claims(&tok);
    assert_eq!(claims.role, Role::Recorder);
    assert_eq!(claims.acl.subscribe_allow, vec!["camera/*", "video/*", "vision/*"]);
}

#[test]
fn issue_invalid_role_exits_2_with_clear_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    let tok = dir.path().join("bad.token");
    let out = issue(dir.path(), &["--role", "bogus", "--node", "n", "--out", tok.to_str().expect("utf8")]);
    assert_eq!(out.status.code(), Some(2), "未知角色应 exit 2");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("bogus"), "stderr 应指明角色名, got: {stderr}");
    assert!(!tok.exists(), "失败时不应写文件");
}

#[test]
fn issue_missing_signing_pem_exits_1_with_hint() {
    let dir = tempfile::tempdir().expect("tempdir");
    // 无 host init → 无 signing.pem
    let tok = dir.path().join("t.token");
    let out = issue(dir.path(), &["--role", "capture", "--node", "n", "--out", tok.to_str().expect("utf8")]);
    assert_eq!(out.status.code(), Some(1), "缺 signing.pem 应 exit 1");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("signing.pem"), "stderr 应指明文件, got: {stderr}");
    assert!(!tok.exists());
}

#[test]
fn issue_missing_required_args_exit_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    // 缺 --role / 缺 --node / 缺 --out → exit 2
    for args in [
        vec!["--node", "n", "--out", "x.token"],
        vec!["--role", "capture", "--out", "x.token"],
        vec!["--role", "capture", "--node", "n"],
    ] {
        let out = issue(dir.path(), &args);
        assert_eq!(out.status.code(), Some(2), "args {args:?} 应 exit 2");
    }
}

/// C4 review: 能力令牌文件必须 0600（与 signing.pem 同级凭据保护）。
#[test]
fn issue_token_file_is_0600() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        init(dir.path());
        let tok = dir.path().join("cam0.token");
        let out = issue(
            dir.path(),
            &[
                "--role", "capture", "--node", "cap-1",
                "--topic", "camera/cam0", "--out", tok.to_str().expect("utf8"),
            ],
        );
        assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
        let mode = std::fs::metadata(&tok).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "令牌文件必须 0600（能力凭据）, got {mode:o}");
    }
}

/// C4 review: --node/--topic 空字符串拒绝（exit 2 + 明确报错）。
#[test]
fn issue_rejects_empty_node_and_topic() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    for args in [
        vec!["--role", "capture", "--node", "", "--out", "x.token"],
        vec!["--role", "pusher", "--node", "n", "--topic", "", "--out", "x.token"],
    ] {
        let out = issue(dir.path(), &args);
        assert_eq!(out.status.code(), Some(2), "args {args:?} 空值应 exit 2");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.is_empty(), "空值应报错");
    }
}

// ── G1: --all 标准集 / --for-ros / --ttl / 校验 / 审计 ──

/// G1: `host token issue --all` 从 host.toml 签发标准车辆令牌集（跳过已存在，
/// D-H10 固定令牌）。模板配置 = cam0 + cam0-stream。
#[test]
fn issue_all_standard_set_with_correct_claims() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    let out = issue(dir.path(), &["--all"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    // etc/link/<cam>.token: Capture 仅发布 camera/<cam>
    let cam = decode_claims(&dir.path().join("etc/link/cam0.token"));
    assert_eq!(cam.role, Role::Capture);
    assert_eq!(cam.node_id, "host-capturer-cam0");
    assert_eq!(cam.acl.publish_allow, vec!["camera/cam0"]);
    assert!(cam.acl.subscribe_allow.is_empty());

    // etc/link/<stream>.token: Pusher 订阅 camera/<cam> + vision/<cam>（F3 视觉 DC）
    let stream = decode_claims(&dir.path().join("etc/link/cam0-stream.token"));
    assert_eq!(stream.role, Role::Pusher);
    assert_eq!(stream.node_id, "host-streamer-cam0-stream");
    assert_eq!(stream.acl.subscribe_allow, vec!["camera/cam0", "vision/cam0"]);
    assert_eq!(stream.acl.publish_allow, vec!["stats/*"]);

    // etc/link/recorder.token: Recorder 矩阵缺省（订阅 camera/video/vision + 发布 stats）
    let rec = decode_claims(&dir.path().join("etc/link/recorder.token"));
    assert_eq!(rec.role, Role::Recorder);
    assert_eq!(rec.node_id, "host-recorder");
    assert_eq!(rec.acl.subscribe_allow, vec!["camera/*", "video/*", "vision/*"]);
    assert_eq!(rec.acl.publish_allow, vec!["stats/*"]);

    // etc/link/agent.token: Monitor 矩阵缺省（订阅 camera/* + stats/*，无发布）
    let agent = decode_claims(&dir.path().join("etc/link/agent.token"));
    assert_eq!(agent.role, Role::Monitor);
    assert_eq!(agent.node_id, "host-agent");
    assert_eq!(agent.acl.subscribe_allow, vec!["camera/*", "stats/*"]);
    assert!(agent.acl.publish_allow.is_empty());

    // ROS 令牌非自动签发（--for-ros 显式）
    assert!(!dir.path().join("etc/link/ros-vision.token").exists());
}

#[test]
fn issue_all_skips_existing_tokens() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    // 预先签发 cam0.token（自定义 node）→ --all 不得覆盖（D-H10 固定令牌）
    let tok = dir.path().join("etc/link/cam0.token");
    let out = issue(
        dir.path(),
        &[
            "--role", "capture", "--node", "custom-node",
            "--topic", "camera/cam0", "--out", tok.to_str().expect("utf8"),
        ],
    );
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let out = issue(dir.path(), &["--all"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let cam = decode_claims(&tok);
    assert_eq!(cam.node_id, "custom-node", "--all 不得覆盖已存在令牌");
}

#[test]
fn issue_all_missing_host_toml_exits_1() {
    let dir = tempfile::tempdir().expect("tempdir");
    // 无 host init → 无 host.toml/signing.pem
    let out = issue(dir.path(), &["--all"]);
    assert_eq!(out.status.code(), Some(1), "缺 host.toml 应 exit 1");
}

#[test]
fn issue_for_ros_preset_issues_perception_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    let out = issue(dir.path(), &["--for-ros"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // 路径与 ros_bridge.yaml token_path 一致
    let tok = dir.path().join("etc/link/ros-vision.token");
    assert!(tok.exists(), "ros-vision.token 应已写出");
    let claims = decode_claims(&tok);
    assert_eq!(claims.role, Role::Perception);
    assert_eq!(claims.node_id, "ros-vision");
    assert_eq!(claims.acl.publish_allow, vec!["perception/*", "vision/*"]);
    assert_eq!(claims.acl.subscribe_allow, vec!["camera/*"]);
}

#[test]
fn issue_for_ros_conflicts_with_explicit_args() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    let out = issue(
        dir.path(),
        &["--for-ros", "--role", "capture", "--node", "n", "--out", "x.token"],
    );
    assert_eq!(out.status.code(), Some(2), "--for-ros 与显式 --role/--node/--out 冲突");
}

/// G1 加固: 显式 --topic 必须落在角色 ACL 矩阵允许范围内（越权拒绝）。
#[test]
fn issue_rejects_topic_outside_role_matrix() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    // capture 仅 camera/* — vision/cam0 越权
    let out = issue(
        dir.path(),
        &["--role", "capture", "--node", "n", "--topic", "vision/cam0", "--out", "x.token"],
    );
    assert_eq!(out.status.code(), Some(2), "capture 发布 vision/* 应拒绝");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("vision/cam0"), "stderr 应指明越权 topic, got: {stderr}");
    // pusher 仅订阅 camera/video/vision — stats/x 越权
    let out = issue(
        dir.path(),
        &["--role", "pusher", "--node", "n", "--topic", "stats/x", "--out", "y.token"],
    );
    assert_eq!(out.status.code(), Some(2), "pusher 订阅 stats/* 应拒绝");
    assert!(!dir.path().join("x.token").exists(), "失败时不应写文件");
}

/// G1: --node 字符集守卫（与 host.toml id 同规则 [A-Za-z0-9_-]+，防路径穿越/畸形 claims）。
#[test]
fn issue_rejects_node_with_invalid_chars() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    let out = issue(dir.path(), &["--role", "capture", "--node", "bad/node", "--out", "x.token"]);
    assert_eq!(out.status.code(), Some(2), "node 含 / 应拒绝");
    let out = issue(dir.path(), &["--role", "capture", "--node", "sp ace", "--out", "x.token"]);
    assert_eq!(out.status.code(), Some(2), "node 含空格应拒绝");
}

/// G1: --ttl <secs> 覆盖缺省 10 年（测试用）。
#[test]
fn issue_ttl_override_respected() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    let tok = dir.path().join("ttl.token");
    let out = issue(
        dir.path(),
        &[
            "--role", "capture", "--node", "n", "--topic", "camera/cam0",
            "--out", tok.to_str().expect("utf8"), "--ttl", "3600",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let claims = decode_claims(&tok);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock");
    let now = now.as_secs();
    assert!(claims.exp > now && claims.exp <= now + 3600 + 60, "ttl=3600 应生效, exp={}", claims.exp);
}

#[test]
fn issue_ttl_rejects_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    let out = issue(dir.path(), &["--role", "capture", "--node", "n", "--out", "x.token", "--ttl", "0"]);
    assert_eq!(out.status.code(), Some(2), "ttl=0 应拒绝");
}

/// G1: 每次签发写审计（etc/link/issuance.jsonl JSONL，D-H10 审计纪律）。
#[test]
fn issue_writes_audit_entry() {
    let dir = tempfile::tempdir().expect("tempdir");
    init(dir.path());
    let tok = dir.path().join("audit.token");
    let out = issue(
        dir.path(),
        &["--role", "pusher", "--node", "audit-1", "--topic", "camera/cam0", "--out", tok.to_str().expect("utf8")],
    );
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let log = std::fs::read_to_string(dir.path().join("etc/link/issuance.jsonl")).expect("issuance.jsonl 应存在");
    let line = log.lines().last().expect("至少一条审计");
    let v: serde_json::Value = serde_json::from_str(line).expect("审计行应可解析");
    assert_eq!(v["role"], "pusher");
    assert_eq!(v["node"], "audit-1");
    assert_eq!(v["topics"], serde_json::json!(["camera/cam0"]));
    assert_eq!(v["out"], serde_json::json!(tok.to_string_lossy()));
    assert!(v["ts"].as_u64().unwrap() > 0, "ts 应填充");
    assert_eq!(v["ttl"], serde_json::json!(10 * 365 * 24 * 3600), "缺省 ttl = 10 年");
}

