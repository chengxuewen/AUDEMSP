//! field 组合 SDK 测试：re-export 完整性 + 错误代理 + 会话 stub 行为。

use mediaservo_field::{
    CameraSource, CaptureOptions, DeviceId, FieldError, FrameBus, MediaDevices, MediaDeviceKind,
    NodeAcl, Player, Recorder, Role, SignalClient,
};

#[test]
fn reexports_bring_entire_closed_loop() {
    // link 面：信令 + 帧总线类型可达
    let _: Option<SignalClient> = None;
    let _: Option<FrameBus> = None;

    // deck 面：采集/录制/回放类型可达
    let cams = MediaDevices::enumerate(MediaDeviceKind::Camera);
    assert_eq!(cams.len(), 1);
    let _ = CameraSource::open(DeviceId("stub:test-camera".into()), CaptureOptions::default());
    let _: Option<Player> = None;
    let _: Option<Recorder> = None;
}

#[test]
fn link_error_flows_into_field_error() {
    // FieldError: From<LinkError> 代理（坏 token 触发 LinkError::Attach）
    let err: FieldError = mediaservo_link::LinkError::Attach("bad token".into()).into();
    assert!(matches!(err, FieldError::Link(_)));
}

#[tokio::test]
async fn session_stub_reports_phase2() {
    // MVP 阶段 PushSession/PullSession::connect 明确失败（避免调用方静默）
    match mediaservo_field::PushSession::connect().await {
        Err(FieldError::InvalidState(msg)) if msg.contains("Phase 2") => {}
        other => panic!("expected InvalidState(Phase 2), got {other:?}"),
    }
    match mediaservo_field::PullSession::connect().await {
        Err(FieldError::InvalidState(msg)) if msg.contains("Phase 2") => {}
        other => panic!("expected InvalidState(Phase 2), got {other:?}"),
    }
}

#[test]
fn link_token_types_reexported() {
    // 能力令牌 API 经 link 可达（组合 SDK 认证面）
    let node = mediaservo_field::NodeId::new("field-test");
    let acl = NodeAcl::for_role(node.clone(), Role::Pusher);
    assert!(acl.can_subscribe(&mediaservo_field::FrameTopic::new("camera/*")));
    let _sk = mediaservo_field::Ed25519SigningKey::from_pem(
        b"-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIObCg8b+Le6kKOI/+pE+4+YhXUlr6X6h7q8p/MjvHmXT\n-----END PRIVATE KEY-----\n",
    );
    let _vk = mediaservo_field::Ed25519VerifyingKey::from_pem(
        b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAgXprEbnahCZoZtLpiUqR0ruqtzEfRXk/Gl/6F6PEm4o=\n-----END PUBLIC KEY-----\n",
    );
}