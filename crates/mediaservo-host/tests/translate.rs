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
    // streamer 行追加 --gateway/—config/--token（D2 网关 + C2 同形）
    assert!(ox.contains("host-streamer --stream s0 --gateway ws://127.0.0.1:17980/ws --config"));
    assert!(ox.contains(&format!("--token {}/etc/link/s0.token", abs.display())));
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
    assert!(mediaservo_host::translate::camera_config(cfg, "cam0").is_err());
}

#[test]
fn stream_config_defaults_and_explicit() {
    // 缺省 camera/codec → id/vp8
    let cfg = "[[streams]]\nid = \"s0\"\n";
    let streams = mediaservo_host::translate::stream_configs(cfg).unwrap();
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0].id, "s0");
    assert_eq!(streams[0].camera, "s0");
    assert_eq!(streams[0].codec, "vp8");
    // 显式值
    let cfg = "[[streams]]\nid = \"s1\"\ncamera = \"cam0\"\ncodec = \"h264\"\n";
    let streams = mediaservo_host::translate::stream_configs(cfg).unwrap();
    assert_eq!(streams[0].camera, "cam0");
    assert_eq!(streams[0].codec, "h264");
    // 单个查找
    assert!(mediaservo_host::translate::stream_config(cfg, "s1").unwrap().is_some());
    assert!(mediaservo_host::translate::stream_config(cfg, "nope").unwrap().is_none());
    // 坏配置 → Err
    assert!(mediaservo_host::translate::stream_configs("not toml [[[").is_err());
}

#[test]
fn record_config_defaults_and_explicit() {
    // 缺省: disabled + 默认输出目录（C3）
    let cfg = "[[cameras]]\nid = \"cam0\"\n";
    let rec = mediaservo_host::translate::record_config(cfg).unwrap();
    assert!(!rec.enabled, "缺省应 disabled");
    assert_eq!(rec.out_dir, std::path::PathBuf::from("/tmp/mediaservo-recordings"));
    // 显式值
    let cfg = "[[cameras]]\nid = \"cam0\"\n[record]\nenabled = true\nout_dir = \"/var/rec\"\n";
    let rec = mediaservo_host::translate::record_config(cfg).unwrap();
    assert!(rec.enabled, "显式 enabled=true 应生效");
    assert_eq!(rec.out_dir, std::path::PathBuf::from("/var/rec"));
    // 缺 out_dir → 默认目录
    let cfg = "[record]\nenabled = true\n";
    let rec = mediaservo_host::translate::record_config(cfg).unwrap();
    assert!(rec.enabled);
    assert_eq!(rec.out_dir, std::path::PathBuf::from("/tmp/mediaservo-recordings"));
    // 坏配置 → Err
    assert!(mediaservo_host::translate::record_config("not toml [[[").is_err());
}

#[test]
fn to_oxfile_in_dir_recorder_appends_config_and_token_paths() {
    // C3: recorder 固定 app 与 capturer/streamer 同形追加 --config/--token
    let dir = tempfile::tempdir().unwrap();
    let cfg = "[[cameras]]\nid = \"cam0\"\n[record]\nenabled = true\n";
    let ox = mediaservo_host::translate::to_oxfile_in_dir(cfg, dir.path()).unwrap();
    let abs = std::path::absolute(dir.path()).unwrap();
    assert!(ox.contains("host-recorder --config"));
    assert!(ox.contains(&format!("--config {}/etc/host.toml", abs.display())));
    assert!(ox.contains(&format!("--token {}/etc/link/recorder.token", abs.display())));
    // 无路径变体保持 A2 形态（无参数）
    let ox = mediaservo_host::translate::to_oxfile(cfg).unwrap();
    assert!(ox.contains("name = \"host-recorder\""));
    assert!(!ox.contains("host-recorder --config"));
}
#[test]
fn signaling_local_port_passed_to_host_agent() {
    // [signaling] local_port 配置 → host-agent 命令追加 --port
    let cfg = "[[cameras]]\nid = \"cam0\"\n[signaling]\nlocal_port = 17980\n";
    let ox = mediaservo_host::translate::to_oxfile(cfg).unwrap();
    assert!(ox.contains("host-agent --port 17980"), "agent 命令应带 --port, got:\n{ox}");
    // 缺省：不追加（agent 内置默认 17980）
    let ox = mediaservo_host::translate::to_oxfile("[[cameras]]\nid = \"cam0\"\n").unwrap();
    assert!(ox.contains("host-agent\"") && !ox.contains("host-agent --port"), "缺省不追加 --port");
    assert_eq!(mediaservo_host::translate::signaling_local_port(cfg).unwrap(), Some(17980));
    assert_eq!(mediaservo_host::translate::signaling_local_port("").unwrap(), None);
}

#[test]
fn signaling_gateway_url_resolution_and_streamer_arg() {
    // D2: [signaling] local_port → 子进程网关 URL；缺省 17980
    let cfg = "[[cameras]]\nid = \"cam0\"\n[[streams]]\nid = \"s0\"\ncamera = \"cam0\"\n[signaling]\nlocal_port = 18000\n";
    assert_eq!(
        mediaservo_host::translate::signaling_gateway_url(cfg).unwrap(),
        "ws://127.0.0.1:18000/ws"
    );
    // 缺省（无 [signaling] 段）→ 17980
    assert_eq!(
        mediaservo_host::translate::signaling_gateway_url("[[cameras]]\nid = \"cam0\"\n").unwrap(),
        "ws://127.0.0.1:17980/ws"
    );
    // streamer 命令追加 --gateway（with paths 与无 paths 变体一致）
    let ox = mediaservo_host::translate::to_oxfile_in_dir(cfg, std::path::Path::new("/tmp/x")).unwrap();
    assert!(
        ox.contains("host-streamer --stream s0 --gateway ws://127.0.0.1:18000/ws --config"),
        "streamer 行应带 --gateway, got:\n{ox}"
    );
    let ox = mediaservo_host::translate::to_oxfile(cfg).unwrap();
    assert!(
        ox.contains("host-streamer --stream s0 --gateway ws://127.0.0.1:18000/ws"),
        "无路径变体也应带 --gateway, got:\n{ox}"
    );
}
#[test]
fn to_oxfile_in_dir_passes_config_to_host_agent() {
    // E1: agent 拓扑监控期望态数据源 — 与 recorder 同形追加 --config
    let dir = tempfile::tempdir().unwrap();
    let cfg = "[[cameras]]\nid = \"cam0\"\n";
    let ox = mediaservo_host::translate::to_oxfile_in_dir(cfg, dir.path()).unwrap();
    let abs = std::path::absolute(dir.path()).unwrap();
    assert!(ox.contains("host-agent --config"), "agent 命令应带 --config, got:\n{ox}");
    assert!(ox.contains(&format!("--config {}/etc/host.toml", abs.display())));
    // 无路径变体保持 A2 形态（无参数）
    let ox = mediaservo_host::translate::to_oxfile(cfg).unwrap();
    assert!(!ox.contains("host-agent --config"));
}

