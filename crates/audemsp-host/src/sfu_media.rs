//! SFU 媒体协商的纯函数构造 — 从 main.rs 抽出以便单测锁定 PIT-54/56 修复。
//!
//! 回归测试防的坑:
//! - PIT-54: produce rtp_parameters 必须含 H264 parameters (packetization-mode/profile-level-id)
//! - PIT-56: remote SDP 方向 recvonly; ssrc 必须从 answer SDP 提取 (非硬编码)

use serde_json::{json, Value};

/// 构造 Host 侧 remote SDP (mediasoup 的 offer)。
/// PIT-56: `a=recvonly` (mediasoup 是接收方) + candidate 行在 m= 段内 (PIT-46)。
pub fn build_remote_sdp(
    ice_ufrag: &str,
    ice_pwd: &str,
    fingerprint_algorithm: &str,
    fingerprint_value: &str,
    candidates: &[(String, String, u32, u16, String)], // (foundation, protocol, priority, port, ip)
) -> String {
    let mut lines = vec![
        "v=0".to_string(),
        "o=- 0 0 IN IP4 0.0.0.0".to_string(),
        "s=-".to_string(),
        "t=0 0".to_string(),
        "a=group:BUNDLE video".to_string(),
        "a=ice-lite".to_string(),
        format!("a=ice-ufrag:{ice_ufrag}"),
        format!("a=ice-pwd:{ice_pwd}"),
        format!("a=fingerprint:{} {}", fingerprint_algorithm.to_lowercase(), fingerprint_value),
        "a=setup:actpass".to_string(), // ICE-Lite responder requirement
    ];
    let conn_ip = candidates
        .iter()
        .find(|(_, _, _, _, ip)| !ip.contains(".local"))
        .map(|(_, _, _, _, ip)| ip.clone())
        .unwrap_or_else(|| "0.0.0.0".to_string());
    const H264_PT: u16 = 101; // ponytail: get from rtp_capabilities when server supports it
    lines.extend_from_slice(&[
        format!("m=video 7 UDP/TLS/RTP/SAVPF {H264_PT}"),
        "b=AS:2000".to_string(), // PIT-65: 码率预算 — Squares 复杂内容防编码器跳帧 (seq 跳变 → Consumer 拒绝)
        format!("c=IN IP4 {conn_ip}"),
        "a=rtcp-mux".to_string(),
        "a=mid:video".to_string(),
        // PIT-56: mediasoup 是接收方 (Host send transport) — sendonly 会让 libwebrtc
        // answer recvonly → 不建发送管线 → 无 RTP
        "a=recvonly".to_string(),
        format!("a=rtpmap:{H264_PT} H264/90000"),
        format!("a=fmtp:{H264_PT} level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"),
    ]);
    // candidates 必须在 media section 内（m= 行之后）— PIT-46
    for (foundation, protocol, priority, port, ip) in candidates {
        if ip.contains(".local") {
            continue; // skip mDNS
        }
        lines.push(format!(
            "a=candidate:{foundation} 1 {} {priority} {ip} {port} typ host",
            protocol.to_uppercase()
        ));
    }
    lines.push("a=end-of-candidates".to_string());
    lines.push(String::new());
    lines.join("\r\n")
}

/// 从 answer SDP 提取 libwebrtc 实际协商的发送 ssrc。
/// PIT-56: produce encodings 的 ssrc 必须与实发一致 (硬编码 12345 → mediasoup 收不到流)。
pub fn negotiated_ssrc_from_sdp(answer_sdp: &str) -> Option<u32> {
    answer_sdp
        .lines()
        .find_map(|l| {
            let l = l.trim_start();
            l.strip_prefix("a=ssrc:")
                .and_then(|s| s.split(' ').next())
                .and_then(|s| s.parse().ok())
        })
}

/// 构造 produce 的 rtp_parameters (mediasoup 期望格式)。
/// PIT-54: H264 必须带 parameters (4d0032/pm=1/level-asymmetry-allowed) —
/// 缺省 packetization-mode 按 0 匹配 Router 的 1 → UnsupportedCodec。
pub fn build_produce_rtp_parameters(ssrc: u32) -> Value {
    json!({
        "codecs": [{
            "mimeType": "video/H264",
            "payloadType": 101,
            "clockRate": 90000,
            "parameters": {
                "level-asymmetry-allowed": 1,
                "packetization-mode": 1,
                "profile-level-id": "4d0032"
            }
        }],
        "headerExtensions": [],
        "encodings": [{"ssrc": ssrc}],
        "rtcp": {"reducedSize": true}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_sdp_has_recvonly_and_in_section_candidates() {
        let sdp = build_remote_sdp(
            "ufrag1",
            "pwd1",
            "SHA-256",
            "AA:BB",
            &[("f1".into(), "udp".into(), 1000, 20000, "10.0.0.1".into())],
        );
        assert!(sdp.contains("a=recvonly"), "PIT-56: 方向必须 recvonly:\n{sdp}");
        // candidate 行必须在 m= 行之后 (PIT-46)
        let m_pos = sdp.find("m=video").expect("m= line");
        let c_pos = sdp.find("a=candidate:f1").expect("candidate line");
        assert!(c_pos > m_pos, "candidate 必须在 m= 段内");
        assert!(sdp.contains("a=end-of-candidates"));
        assert!(sdp.contains("a=ice-lite"));
    }

    #[test]
    fn remote_sdp_skips_mdns_candidates() {
        let sdp = build_remote_sdp(
            "u",
            "p",
            "sha-256",
            "fp",
            &[
                ("mdns".into(), "udp".into(), 1, 100, "abc.local".into()),
                ("real".into(), "udp".into(), 2, 20000, "10.0.0.2".into()),
            ],
        );
        assert!(!sdp.contains("abc.local"), "mDNS 候选必须跳过");
        assert!(sdp.contains("a=candidate:real"));
    }

    #[test]
    fn negotiate_ssrc_from_answer_sdp() {
        let sdp = "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\nm=video 9 UDP/TLS/RTP/SAVPF 101\r\na=ssrc:1949911776 cname:x\r\n";
        assert_eq!(negotiated_ssrc_from_sdp(sdp), Some(1949911776));
        assert_eq!(negotiated_ssrc_from_sdp("no ssrc here"), None);
    }

    #[test]
    fn produce_rtp_parameters_has_h264_params() {
        let v = build_produce_rtp_parameters(1949911776);
        // PIT-54: 必须含 parameters — 缺 packetization-mode → match_codecs 按 0 vs Router 1 → UnsupportedCodec
        let codec = &v["codecs"][0];
        assert_eq!(codec["mimeType"], "video/H264");
        assert_eq!(codec["payloadType"], 101);
        assert_eq!(codec["clockRate"], 90000);
        assert_eq!(codec["parameters"]["packetization-mode"], 1);
        assert_eq!(codec["parameters"]["profile-level-id"], "4d0032");
        assert_eq!(codec["parameters"]["level-asymmetry-allowed"], 1);
        // PIT-56: ssrc 来自协商结果
        assert_eq!(v["encodings"][0]["ssrc"], 1949911776);
    }
}
