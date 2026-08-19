//! translate.rs 翻译器测试（Task A2 + C1）：host.toml → oxfile.toml 文本。
//!
//! C1 增补：camera_config(s) 富解析（source/fps 缺省）+ to_oxfile_in_dir 的
//! capturer 实例追加 --config/--token 绝对路径。

#[test]
fn to_oxfile_emits_all_placeholder_apps() {
    let cfg = "[host]\ndevice_id = \"car-01\"\n[[cameras]]\nid = \"cam0\"\nsource = \"stub\"\nfps = 30\n[[streams]]\nid = \"cam0-stream\"\ncamera = \"cam0\"\ncodec = \"h264\"\n[record]\nenabled = false\n[control]\nenabled = false\n";
    let ox = mediaservo_host::translate::to_oxfile(cfg).unwrap();
    for app in ["host-agent", "host-capturer", "host-streamer", "host-recorder",
                "host-controller", "host-emergency", "host-audio"] {
        assert!(ox.contains(&format!("name = \"{app}\"")), "missing {app}");
    }
    assert!(ox.contains("host-capturer --camera cam0")); // 参数化实例
    assert!(ox.contains("host-streamer --stream cam0-stream"));
    // [defaults] 固定字段（A2 审查 M4 补强）
    assert!(ox.contains("version = 1"));
    assert!(ox.contains("namespace = \"host\""));
    assert!(ox.contains("restart_policy = \"always\""));
}

#[test]
fn camera_config_defaults_and_explicit() {
    // 缺省 source/fps → stub/30
    let cfg = "[[cameras]]\nid = \"cam0\"\n";
    let cams = mediaservo_host::translate::camera_configs(cfg).unwrap();
    assert_eq!(cams.len(), 1);
    assert_eq!(cams[0].id, "cam0");
    assert_eq!(cams[0].source, "stub");
    assert_eq!(cams[0].fps, 30);
    // 显式值
    let cfg = "[[cameras]]\nid = \"cam1\"\nsource = \"v4l2\"\nfps = 15\n";
    let cams = mediaservo_host::translate::camera_configs(cfg).unwrap();
    assert_eq!(cams[0].source, "v4l2");
    assert_eq!(cams[0].fps, 15);
    // 单个查找
    assert!(mediaservo_host::translate::camera_config(cfg, "cam1").unwrap().is_some());
    assert!(mediaservo_host::translate::camera_config(cfg, "nope").unwrap().is_none());
    // 坏配置 → Err
    assert!(mediaservo_host::translate::camera_configs("not toml [[[").is_err());
}

#[test]
fn to_oxfile_in_dir_appends_config_and_token_paths() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = "[[cameras]]\nid = \"cam0\"\n[[streams]]\nid = \"s0\"\n";
    let ox = mediaservo_host::translate::to_oxfile_in_dir(cfg, dir.path()).unwrap();
    // 绝对路径 + 每相机 token 文件
    let abs = std::path::absolute(dir.path()).unwrap();
    assert!(ox.contains(&format!("--config {}/etc/host.toml", abs.display())));
    assert!(ox.contains(&format!("--token {}/etc/link/cam0.token", abs.display())));
    // streamer 行不受影响（C2 再扩展）
    assert!(ox.contains("host-streamer --stream s0"));
    // 无路径变体保持 A2 形态
    let ox = mediaservo_host::translate::to_oxfile(cfg).unwrap();
    assert!(ox.contains("host-capturer --camera cam0"));
    assert!(!ox.contains("--config"));
}

#[test]
fn camera_config_rejects_zero_fps() {
    // C1 审查发现: fps=0 → generator.start 线程内 panic → 死线程 + 主线程永久阻塞
    // （C15 "failure as hang" 类）——必须在配置解析层拒绝。
    let cfg = "[[cameras]]\nid = \"cam0\"\nfps = 0\n";
    let err = mediaservo_host::translate::camera_configs(cfg).unwrap_err();
    assert!(err.contains("fps"), "错误信息应指明 fps, got: {err}");
    assert!(err.contains("cam0"), "错误信息应含相机 id, got: {err}");
    // 单查同样拒绝
    assert!(mediaservo_host::translate::camera_config(cfg, "cam0").is_err());
}
