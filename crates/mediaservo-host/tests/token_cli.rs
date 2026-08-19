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
    Command::new(env!("CARGO_BIN_EXE_host"))
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

    // streamer 角色: pusher（订阅 camera/*，不发布）
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
    assert!(claims.acl.publish_allow.is_empty(), "pusher 无发布权");
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
