//! offerer_prefs_test — setCodecPreferences offerer 模式核心机制验证
//!
//! T5 实证结论（set-codec-preferences 计划）:
//!   - offerer 模式: 按 track_id 设置偏好生效 — create_offer 的 codec 序按偏好重排
//!     （H264 全部先于 VP8, 实测）
//!   - answerer 模式（SFU server-offer）: set 成功但 answer 不受影响
//!     （libwebrtc 按 offer 序取交集）→ 固定 codec 需 reduceCodecs（mediasoup 官方模式）
//!   - mid 参数化不可行: 协商前 transceiver 无 mid（offerer 核心场景）→ track_id 定位

// 快速实验: offerer 模式下 setCodecPreferences 是否影响 create_offer 的 codec 序
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn offerer_prefs_reorders_offer() {
    use audemsp_webrtc::traits::PeerConnectionApi;
    use audemsp_webrtc::{
        RTCPeerConnectionFactory, RTCConfiguration, RTCOfferOptions, TrackKind,
    };
    let pc = RTCPeerConnectionFactory::new()
        .create_peer_connection(RTCConfiguration::default())
        .await
        .expect("PC");
    let track_id = pc.add_track("video", TrackKind::Video).expect("add_track");
    let _ = &track_id;

    // 偏好 H264 在前
    let caps = pc.get_sender_capabilities(TrackKind::Video).expect("caps").unwrap();
    let mut codecs = caps.codecs.clone();
    codecs.sort_by_key(|c| {
        if c.mime_type.starts_with("video/H264") { 0 } else { 1 }
    });
    codecs.retain(|c| c.mime_type.starts_with("video/H264") || c.mime_type.starts_with("video/VP8"));
    let r = pc.transceiver_set_codec_preferences(&track_id, codecs);
    println!("set_codec_preferences: {r:?}");
    assert!(r.is_ok(), "set 失败: {r:?}");

    let offer = pc.create_offer(&RTCOfferOptions::default()).await.expect("offer");
    // 打印 offer 的 codec 顺序
    for line in offer.sdp.lines() {
        if line.starts_with("a=rtpmap:") || line.starts_with("m=video") {
            println!("{line}");
        }
    }
    // 断言 H264 在 offer 中先于 VP8
    let h264_pos = offer.sdp.find("H264").unwrap_or(usize::MAX);
    let vp8_pos = offer.sdp.find("VP8").unwrap_or(usize::MAX);
    assert!(h264_pos < vp8_pos, "offer 应 H264 在前: h264={h264_pos} vp8={vp8_pos}");
}
