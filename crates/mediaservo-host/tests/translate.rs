//! translate.rs 翻译器测试（Task A2）：host.toml → oxfile.toml 文本。

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
}
