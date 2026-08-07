//! SFU 媒体协商的纯函数构造 — 从 main.rs 抽出以便单测。
//!
//! P3 (v2)+P2 修复: Host 走标准 answerer 协商 — 用 server transport 参数构造 remote SDP
//! → set_remote_description → create_answer (对齐 libmediasoupclient SdpUtils / e2e_sfu 验证路径)。
//! 本模块提供 remote SDP 构造 + 从协商结果 RTCRtpParameters 构造 produce 请求。

use serde_json::{json, Value};
use audemsp_common::protocol::{DtlsParameters, IceCandidate, IceParameters};
use audemsp_webrtc::rtp::RTCRtpParameters;

/// 用 mediasoup transport 参数构造 remote SDP (ICE-Lite server offer)。
/// PIT-48: a=candidate 行必须位于 m= 行之后（media section 内）——
/// 会话级 candidate 被 libwebrtc 忽略 → remote candidate 丢失 → ICE 不发起 STUN
pub fn build_remote_sdp(
    ice_parameters: &IceParameters,
    dtls_parameters: &DtlsParameters,
    ice_candidates: Option<&Vec<IceCandidate>>,
    payload_type: u16,
    codec_name: &str,
    clock_rate: u32,
    fmtp: Option<&str>,
) -> String {
    let fp = &dtls_parameters.fingerprints[0];
    let conn_ip = ice_candidates
        .and_then(|cs| cs.iter().find(|c| !c.ip.contains(".local")))
        .map(|c| c.ip.clone())
        .unwrap_or_else(|| "0.0.0.0".to_string());

    let mut lines = vec![
        "v=0".to_string(),
        "o=- 0 0 IN IP4 0.0.0.0".to_string(),
        "s=-".to_string(),
        "t=0 0".to_string(),
        "a=group:BUNDLE video".to_string(),
        "a=ice-lite".to_string(),
        format!("a=ice-ufrag:{}", ice_parameters.username_fragment),
        format!("a=ice-pwd:{}", ice_parameters.password),
        format!(
            "a=fingerprint:{} {}",
            fp.algorithm.to_lowercase(),
            fp.value
        ),
        "a=setup:actpass".to_string(), // ICE-Lite responder expects client to initiate
    ];

    lines.extend_from_slice(&[
        format!("m=video 7 UDP/TLS/RTP/SAVPF {}", payload_type),
        format!("c=IN IP4 {}", conn_ip),
        "a=rtcp-mux".to_string(),
        "a=mid:video".to_string(),
        "a=recvonly".to_string(),
        format!("a=rtpmap:{} {}/{}", payload_type, codec_name, clock_rate),
    ]);

    if let Some(fmtp_val) = fmtp {
        lines.push(format!("a=fmtp:{} {}", payload_type, fmtp_val));
    }

    if let Some(candidates) = ice_candidates {
        for c in candidates {
            if c.ip.contains(".local") {
                continue;
            } // skip mDNS
            let ctype = match c.candidate_type.as_str() {
                "host" => "host",
                "srflx" => "srflx",
                "prflx" => "prflx",
                "relay" => "relay",
                _ => "host",
            };
            lines.push(format!(
                "a=candidate:{} 1 {} {} {} {} typ {}",
                c.foundation,
                c.protocol.to_uppercase(),
                c.priority,
                c.ip,
                c.port,
                ctype
            ));
        }
    }
    lines.push("a=end-of-candidates".to_string());

    lines.push(String::new());
    lines.join("\r\n")
}

/// PIT-65: 在 local answer 的 video m= 段注入 `x-google-max-keyframe-interval` fmtp。
/// libwebrtc 从 **local** answer 读该参数配置编码器 GOP（加在 remote SDP 无效——
/// 2026-08-05 实证稳态 GOP 仍 ~99s）。
/// 只处理 payload_type 匹配的 m=video 段；无匹配则原样返回。
pub fn inject_keyframe_interval(sdp: &str, payload_type: u16, interval_ms: u32) -> String {
    let mut out = Vec::new();
    let mut in_video_mline = false;
    let mut injected = false;
    // split('\n') 保留原行（含行尾 \r），join('\n') 保持 SDP 结构不变 —
    // 用 lines()+join("\r\n") 会把 \r\n 变成 \r\r\n 破坏 SDP (实测)
    for line in sdp.split('\n') {
        let trimmed = line.strip_suffix('\r').unwrap_or(line);
        if trimmed.starts_with("m=video") {
            in_video_mline = true;
        } else if trimmed.starts_with("m=") && !trimmed.starts_with("m=video") {
            in_video_mline = false;
        }
        out.push(line.to_string());
        if in_video_mline && !injected
            && trimmed == format!("a=rtpmap:{} VP8/90000", payload_type)
        {
            out.push(format!(
                "a=fmtp:{} x-google-max-keyframe-interval={}",
                payload_type, interval_ms
            ));
            injected = true;
        }
    }
    out.join("\n")
}
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