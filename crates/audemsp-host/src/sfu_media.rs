//! SFU 媒体协商的纯函数构造 — 从 main.rs 抽出以便单测。
//!
//! P3 (v2): Host 走标准协商（add_transceiver_with_track → create_offer → get_sending_rtp_parameters），
//! 不再手工构造 remote SDP / 手工解析 ssrc / 手工硬编码 rtp_parameters (C18)。
//! 本模块只保留「从协商结果 RTCRtpParameters 构造 mediasoup produce 请求」的纯函数。

use serde_json::{json, Value};
use audemsp_webrtc::rtp::RTCRtpParameters;

/// P3 (v2): 从协商结果 (RTCRtpParameters) 构造 mediasoup produce 的 rtp_parameters。
/// 对齐官方客户端 — 数据来自 transceiver.sender.get_parameters()，非手工硬编码。
pub fn build_produce_rtp_parameters_from_rtp(params: &RTCRtpParameters) -> Value {
    let codecs: Vec<Value> = params.codecs.iter().map(|c| {
        json!({
            "mimeType": c.mime_type,
            "payloadType": c.payload_type,
            "clockRate": c.clock_rate,
        })
    }).collect();
    let encodings: Vec<Value> = params.encodings.iter().map(|e| {
        let mut enc = json!({});
        if let Some(ssrc) = e.ssrc {
            enc["ssrc"] = json!(ssrc);
        }
        if let Some(max_bitrate) = e.max_bitrate {
            enc["maxBitrate"] = json!(max_bitrate);
        }
        enc
    }).collect();
    json!({
        "codecs": codecs,
        "headerExtensions": [],
        "encodings": encodings,
        "rtcp": {"reducedSize": params.rtcp.reduced_size},
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use audemsp_webrtc::rtp::{RTCRtpCodecParameters, RTCRtpEncodingParameters, RTCRtcpParameters};

    #[test]
    fn produce_from_rtp_includes_negotiated_ssrc() {
        let params = RTCRtpParameters {
            transaction_id: "tx1".into(),
            mid: "0".into(),
            codecs: vec![RTCRtpCodecParameters {
                mime_type: "video/H264".into(),
                payload_type: 101,
                clock_rate: 90000,
                channels: None,
                sdp_fmtp_line: Some("profile-level-id=42e01f".into()),
            }],
            encodings: vec![RTCRtpEncodingParameters {
                ssrc: Some(1949911776),
                active: true,
                max_bitrate: Some(2_000_000),
                ..Default::default()
            }],
            header_extensions: vec![],
            rtcp: RTCRtcpParameters { cname: None, reduced_size: true },
        };
        let v = build_produce_rtp_parameters_from_rtp(&params);
        // 协商结果: ssrc + codec 字段来自 get_sending_rtp_parameters
        assert_eq!(v["codecs"][0]["mimeType"], "video/H264");
        assert_eq!(v["codecs"][0]["payloadType"], 101);
        assert_eq!(v["codecs"][0]["clockRate"], 90000);
        assert_eq!(v["encodings"][0]["ssrc"], 1949911776);
        assert_eq!(v["encodings"][0]["maxBitrate"], 2_000_000);
        assert_eq!(v["rtcp"]["reducedSize"], true);
    }

    #[test]
    fn produce_from_rtp_empty_encodings() {
        let params = RTCRtpParameters::default();
        let v = build_produce_rtp_parameters_from_rtp(&params);
        assert!(v["codecs"].as_array().unwrap().is_empty());
        assert!(v["encodings"].as_array().unwrap().is_empty());
    }
}