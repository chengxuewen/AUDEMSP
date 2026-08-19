//! Task 2: 静态 ACL 权限矩阵测试（D237）。

use mediaservo_link::{FrameTopic, NodeAcl, NodeId, Role};

fn acl(role: Role) -> NodeAcl {
    NodeAcl::for_role(NodeId::new("n0"), role)
}

#[test]
fn capture_pub_camera_not_control_no_sub() {
    let a = acl(Role::Capture);
    assert!(a.can_publish(&FrameTopic::new("camera/front/raw")));
    assert!(!a.can_publish(&FrameTopic::new("control/cmd")));
    assert!(!a.can_subscribe(&FrameTopic::new("camera/front/raw")), "capture 不订阅");
}

#[test]
fn processor_sub_camera_pub_video() {
    let a = acl(Role::Processor);
    assert!(a.can_subscribe(&FrameTopic::new("camera/front/raw")));
    assert!(a.can_publish(&FrameTopic::new("video/stitched")), "派生 topic(D239)");
    assert!(!a.can_publish(&FrameTopic::new("control/cmd")));
}

#[test]
fn pusher_sub_not_pub() {
    let a = acl(Role::Pusher);
    assert!(a.can_subscribe(&FrameTopic::new("camera/front/raw")));
    assert!(a.can_subscribe(&FrameTopic::new("video/stitched")));
    assert!(!a.can_publish(&FrameTopic::new("camera/front/raw")));
}

#[test]
fn recorder_sub_not_pub() {
    let a = acl(Role::Recorder);
    assert!(a.can_subscribe(&FrameTopic::new("camera/front/raw")));
    assert!(a.can_subscribe(&FrameTopic::new("video/stitched")));
    assert!(!a.can_publish(&FrameTopic::new("camera/front/raw")));
}

#[test]
fn control_pub_cmd_sub_telemetry_status() {
    let a = acl(Role::Control);
    assert!(a.can_publish(&FrameTopic::new("control/cmd")));
    assert!(!a.can_publish(&FrameTopic::new("camera/front/raw")));
    assert!(a.can_subscribe(&FrameTopic::new("control/telemetry")));
    assert!(a.can_subscribe(&FrameTopic::new("status/x")));
    assert!(!a.can_subscribe(&FrameTopic::new("camera/front/raw")));
}

#[test]
fn perception_pub_perception_sub_camera() {
    let a = acl(Role::Perception);
    assert!(a.can_publish(&FrameTopic::new("perception/objects")));
    assert!(a.can_subscribe(&FrameTopic::new("camera/front/raw")));
    assert!(!a.can_publish(&FrameTopic::new("video/x")));
}

    #[test]
    fn puller_no_perm() {
    let a = acl(Role::Puller);
    assert!(!a.can_publish(&FrameTopic::new("camera/x")));
    assert!(!a.can_subscribe(&FrameTopic::new("camera/x")));
}

#[test]
    fn pusher_pub_stats_sub_frames() {
    let a = acl(Role::Pusher);
    assert!(a.can_subscribe(&FrameTopic::new("camera/front/raw")));
    assert!(!a.can_publish(&FrameTopic::new("camera/front/raw")));
    // E2 推流状态上报（streamer 进程）
    assert!(a.can_publish(&FrameTopic::new("stats/stream-s0")));
}

#[test]
    fn recorder_pub_stats_sub_frames() {
    let a = acl(Role::Recorder);
    assert!(a.can_subscribe(&FrameTopic::new("camera/front/raw")));
    assert!(!a.can_publish(&FrameTopic::new("camera/front/raw")));
    // E2 推流状态上报（streamer 令牌缺省 Recorder，C2 遗留）
    assert!(a.can_publish(&FrameTopic::new("stats/stream-s0")));
}

    #[test]
fn monitor_sub_frames_and_stats_no_pub() {
    let a = acl(Role::Monitor);
    assert!(a.can_subscribe(&FrameTopic::new("camera/front/raw")));
    assert!(a.can_subscribe(&FrameTopic::new("stats/stream-s0")));
    assert!(!a.can_subscribe(&FrameTopic::new("control/cmd")));
    assert!(!a.can_publish(&FrameTopic::new("camera/front/raw")));
    assert!(!a.can_publish(&FrameTopic::new("stats/stream-s0")));
}
