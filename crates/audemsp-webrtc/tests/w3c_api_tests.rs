//! W3C WebRTC API integration tests.
//!
//! Tests the public RTCPeerConnection/RTCDataChannel API layer against
//! the active backend (stub by default, webrtc-rs/webrtc-sys via features).
//!
//! Reference: webrtc-kit tests (w3c_state_transitions, w3c_observer_tests,
//! w3c_loopback_dc, mock_backend, etc.)

#[cfg(test)]
mod factory_tests {
    use audemsp_webrtc::peer_connection::{RTCConfiguration, RTCPeerConnectionState};
    use audemsp_webrtc::factory::RTCPeerConnectionFactory;
    use audemsp_webrtc::traits::PeerConnectionApi;


    #[test]
    fn factory_creates_default() {
        let factory = RTCPeerConnectionFactory::default();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        assert_eq!(pc.connection_state(), RTCPeerConnectionState::New);
    }

    #[test]
    fn factory_new_creates_pc() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        assert_eq!(pc.connection_state(), RTCPeerConnectionState::New);
    }

    #[test]
    fn factory_creates_multiple_pcs() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc1 = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("pc1");
        let pc2 = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("pc2");
        assert_eq!(pc1.connection_state(), RTCPeerConnectionState::New);
        assert_eq!(pc2.connection_state(), RTCPeerConnectionState::New);
    }
}

#[cfg(test)]
mod state_tests {
    use audemsp_webrtc::peer_connection::{RTCIceConnectionState, RTCIceGatheringState, RTCConfiguration, RTCPeerConnectionState, RTCSignalingState};
    use audemsp_webrtc::factory::RTCPeerConnectionFactory;
    use audemsp_webrtc::traits::PeerConnectionApi;


    #[test]
    fn initial_states_are_correct() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");

        assert_eq!(pc.connection_state(), RTCPeerConnectionState::New);
        assert_eq!(pc.ice_connection_state(), RTCIceConnectionState::New);
        assert_eq!(pc.ice_gathering_state(), RTCIceGatheringState::New);
        assert_eq!(pc.signaling_state(), RTCSignalingState::Stable);
    }

    #[test]
    fn close_changes_connection_state() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        rt.block_on(pc.close());
        assert_eq!(pc.connection_state(), RTCPeerConnectionState::Closed);
    }

    #[test]
    fn close_changes_ice_connection_state() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        rt.block_on(pc.close());
        assert_eq!(pc.ice_connection_state(), RTCIceConnectionState::Closed);
    }

    #[test]
    fn close_changes_signaling_state() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        rt.block_on(pc.close());
        assert_eq!(pc.signaling_state(), RTCSignalingState::Closed);
    }
}

#[cfg(test)]
mod sdp_tests {
    use audemsp_webrtc::peer_connection::{RTCAnswerOptions, RTCOfferOptions, RTCConfiguration};
    use audemsp_webrtc::factory::RTCPeerConnectionFactory;

    use audemsp_webrtc::sdp::RTCSdpType;
    use audemsp_webrtc::traits::PeerConnectionApi;

    #[test]
    fn create_offer_returns_offer_type() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        let offer = rt
            .block_on(pc.create_offer(&RTCOfferOptions::default()))
            .expect("create offer");
        assert_eq!(offer.sdp_type, RTCSdpType::Offer);
    }

    #[test]
    fn create_answer_returns_answer_type() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        let answer = rt
            .block_on(pc.create_answer(&RTCAnswerOptions::default()))
            .expect("create answer");
        assert_eq!(answer.sdp_type, RTCSdpType::Answer);
    }

    #[test]
    fn set_local_description_succeeds() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        let offer = rt
            .block_on(pc.create_offer(&RTCOfferOptions::default()))
            .expect("create offer");
        rt.block_on(pc.set_local_description(&offer))
            .expect("set local");
    }

    #[test]
    fn set_remote_description_succeeds() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        let answer = rt
            .block_on(pc.create_answer(&RTCAnswerOptions::default()))
            .expect("create answer");
        rt.block_on(pc.set_remote_description(&answer))
            .expect("set remote");
    }

    #[test]
    fn sdp_round_trip_offer_answer() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc1 = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("pc1");
        let pc2 = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("pc2");

        let offer = rt
            .block_on(pc1.create_offer(&RTCOfferOptions::default()))
            .expect("offer");
        rt.block_on(pc1.set_local_description(&offer))
            .expect("pc1 set local");
        rt.block_on(pc2.set_remote_description(&offer))
            .expect("pc2 set remote");

        let answer = rt
            .block_on(pc2.create_answer(&RTCAnswerOptions::default()))
            .expect("answer");
        rt.block_on(pc2.set_local_description(&answer))
            .expect("pc2 set local");
        rt.block_on(pc1.set_remote_description(&answer))
            .expect("pc1 set remote");
    }

    #[test]
    fn offer_with_receive_audio_video() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        let options = RTCOfferOptions {
            ice_restart: false,
            offer_to_receive_audio: true,
            offer_to_receive_video: true,
        };
        let offer = rt.block_on(pc.create_offer(&options)).expect("create offer");
        assert_eq!(offer.sdp_type, RTCSdpType::Offer);
    }
}

#[cfg(test)]
mod ice_tests {
    use audemsp_webrtc::peer_connection::{RTCIceCandidate, RTCConfiguration};
    use audemsp_webrtc::factory::RTCPeerConnectionFactory;
    use audemsp_webrtc::traits::PeerConnectionApi;


    #[test]
    fn add_ice_candidate_succeeds() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        let candidate = RTCIceCandidate {
            candidate: "candidate:1 1 UDP 2130706431 192.168.1.1 12345 typ host".into(),
            sdp_mid: Some("0".into()),
            sdp_mline_index: Some(0),
        };
        rt.block_on(pc.add_ice_candidate(&candidate))
            .expect("add ice candidate");
    }

    #[test]
    fn add_multiple_ice_candidates() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        for i in 0..5 {
            let candidate = RTCIceCandidate {
                candidate: format!(
                    "candidate:{} 1 UDP 2130706431 192.168.1.{} 12345 typ host",
                    i, i
                ),
                sdp_mid: Some("0".into()),
                sdp_mline_index: Some(0),
            };
            rt.block_on(pc.add_ice_candidate(&candidate))
                .expect("add ice candidate");
        }
    }
}

#[cfg(test)]
mod datachannel_tests {
    use audemsp_webrtc::data_channel::{RTCDataChannelInit, RTCDataChannelState};
    use audemsp_webrtc::peer_connection::{RTCConfiguration};
    use audemsp_webrtc::factory::RTCPeerConnectionFactory;
    use audemsp_webrtc::traits::PeerConnectionApi;


    #[test]
    fn create_data_channel_returns_correct_label() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        let dc = rt
            .block_on(pc.create_data_channel("test-dc", RTCDataChannelInit::default()))
            .expect("create dc");
        assert_eq!(dc.label(), "test-dc");
    }

    #[test]
    fn create_data_channel_with_default_init() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        let dc = rt
            .block_on(pc.create_data_channel("dc-default", RTCDataChannelInit::default()))
            .expect("create dc");
        assert_eq!(dc.label(), "dc-default");
    }

    #[test]
    fn data_channel_state_is_closed_after_close() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        let mut dc = rt
            .block_on(pc.create_data_channel("close-test", RTCDataChannelInit::default()))
            .expect("create dc");
        rt.block_on(dc.close());
        assert_eq!(dc.state(), RTCDataChannelState::Closed);
    }

    #[test]
    fn data_channel_send_succeeds() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        let dc = rt
            .block_on(pc.create_data_channel("send-test", RTCDataChannelInit::default()))
            .expect("create dc");
        rt.block_on(dc.send(b"hello")).expect("send");
    }

    #[test]
    fn data_channel_send_text_succeeds() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        let dc = rt
            .block_on(pc.create_data_channel("text-test", RTCDataChannelInit::default()))
            .expect("create dc");
        rt.block_on(dc.send_text("hello world")).expect("send_text");
    }

    #[test]
    fn create_multiple_data_channels() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        let dc1 = rt
            .block_on(pc.create_data_channel("dc-1", RTCDataChannelInit::default()))
            .expect("dc1");
        let dc2 = rt
            .block_on(pc.create_data_channel("dc-2", RTCDataChannelInit::default()))
            .expect("dc2");
        assert_eq!(dc1.label(), "dc-1");
        assert_eq!(dc2.label(), "dc-2");
    }

    // ponytail: empty label is valid per W3C spec
    #[test]
    fn create_data_channel_empty_label() {
        let factory = RTCPeerConnectionFactory::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pc = rt
            .block_on(factory.create_peer_connection(RTCConfiguration::default()))
            .expect("create pc");
        let dc = rt
            .block_on(pc.create_data_channel("", RTCDataChannelInit::default()))
            .expect("create dc");
        assert_eq!(dc.label(), "");
    }
}

#[cfg(test)]
mod stats_and_rtp_tests {
    use audemsp_webrtc::rtp::{
        RTCRtpCodecParameters, RTCRtpEncodingParameters, RTCRtpHeaderExtensionParameters, RTCRtpParameters,
    };
    use audemsp_webrtc::stats::{
        RTCInboundRtpStreamStats, RTCPeerConnectionStats, RTCStats,
    };
    use audemsp_webrtc::traits::PeerConnectionApi;

    #[test]
    fn rtc_stats_types_construct() {
        let stats = vec![
            RTCStats::RTCPeerConnection(RTCPeerConnectionStats {
                id: "pc1".into(),
                timestamp: 0.0,
                data_channels_opened: 1,
                data_channels_closed: 0,
            }),
            RTCStats::InboundRtp(RTCInboundRtpStreamStats {
                id: "in1".into(),
                timestamp: 0.0,
                ssrc: 12345,
                kind: "video".into(),
                packets_received: 100,
                packets_lost: 2,
                bytes_received: 50000,
                frames_decoded: 30,
                frame_width: 1920,
                frame_height: 1080,
                frames_per_second: 30.0,
            }),
        ];
        assert_eq!(stats.len(), 2);
    }

    #[test]
    fn rtc_stats_serializes() {
        let stats = RTCStats::RTCPeerConnection(RTCPeerConnectionStats {
            id: "pc1".into(),
            timestamp: 1.0,
            data_channels_opened: 1,
            data_channels_closed: 0,
        });
        let json = serde_json::to_string(&stats).expect("serialize");
        assert!(json.contains("pc1"));
        assert!(json.contains("data_channels_opened"));
    }

    #[test]
    fn rtp_parameters_default_values() {
        let params = RTCRtpParameters::default();
        assert!(params.transaction_id.is_empty());
        assert!(params.codecs.is_empty());
        assert!(params.encodings.is_empty());
    }

    #[test]
    fn rtp_codec_parameters_h264() {
        let codec = RTCRtpCodecParameters {
            mime_type: "video/H264".into(),
            payload_type: 96,
            clock_rate: 90000,
            channels: None,
            sdp_fmtp_line: Some("profile-level-id=42e01f".into()),
        };
        assert_eq!(codec.mime_type, "video/H264");
        assert_eq!(codec.payload_type, 96);
    }

    #[test]
    fn rtp_encoding_default_active() {
        let enc = RTCRtpEncodingParameters::default();
        assert!(enc.active);
        assert!(enc.ssrc.is_none());
    }

    #[test]
    fn rtp_header_extension_parameters() {
        let ext = RTCRtpHeaderExtensionParameters {
            uri: "urn:ietf:params:rtp-hdrext:ssrc-audio-level".into(),
            id: 1,
            encrypted: false,
        };
        assert_eq!(ext.uri, "urn:ietf:params:rtp-hdrext:ssrc-audio-level");
        assert_eq!(ext.id, 1);
    }
}
