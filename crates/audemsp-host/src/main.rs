//! AUDEMSP Host — headless capture + encode + WebRTC push.
//!
//! # Startup flow
//! 1. Parse config (host.conf YAML)
//! 2. Load or create session state
//! 3. Start GStreamer pipeline
//! 4. Initialize WebRTC transport
//! 5. Create control handler
//! 6. Build axum router (metrics + signaling)
//! 7. Start emergency UDP listener
//! 8. Serve until shutdown signal


use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower_http::timeout::TimeoutLayer;

use futures_util::{SinkExt, StreamExt};
use audemsp_media::engine::PipelineEngine;
use audemsp_common::protocol::{DtlsParameters, Fingerprint, IceCandidate, IceParameters, MediaKind, SignalingMessage, TransportDirection};
use tokio_tungstenite::tungstenite::Message;
use signaling::SignalingClient;
use audemsp_webrtc::{RTCPeerConnectionFactory, RTCConfiguration, RTCIceServer, RTCIceTransportPolicy, RTCSessionDescription, RTCSdpType, RTCAnswerOptions, TrackKind, TrackSender, TrackRef, RTCPeerConnectionState};
use audemsp_webrtc::traits::PeerConnectionApi;
mod config;
mod control;
mod emergency;
mod metrics;
mod pipeline;
mod engine_adapters;
mod session;
mod signaling;
mod transport;
#[cfg(feature = "webrtc-p2p")]
mod webrtc_transport;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().expect("hardcoded directive")),
        )
        .init();

    tracing::info!("AUDEMSP Host v{} starting", env!("CARGO_PKG_VERSION"));

    // Parse config — collect args once for bounds-safe access
    let config_path = {
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 2 && args[1] == "--config" {
            args[2].clone()
        } else {
            "/opt/audemsp/etc/host.conf".to_string()
        }
    };
    let config = match config::load(&config_path) {
        Ok(c) => {
            tracing::info!("Config loaded from {config_path}");
            c
        }
        Err(e) => {
            tracing::warn!("Config {config_path}: {e}, using defaults");
            serde_yaml::from_str(&default_host_config()).unwrap()
        }
    };// ponytail: fallback to defaults when config file missing, add config wizard when needed

    // Parse resolution "WIDTHxHEIGHT"
    let (width, height) = parse_resolution(&config.capture.resolution);
    let framerate = config.capture.framerate;
    let bitrate = config.encoder.bitrate_kbps;
    let encoder = &config.encoder.backend;

    // Phase 1: Load or create session state
    let session_state = session::Session::load(None);
    let persist_handle = session_state.start_persist();

    // Collect background task handles for clean shutdown
    let mut background_tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    // Phase 2: Start GStreamer pipeline
    let pipeline = std::sync::Arc::new(pipeline::Pipeline::new(
        &config.capture,
        width,
        height,
        framerate,
        bitrate,
        encoder,
    )
    .unwrap_or_else(|e| {
        tracing::warn!("Pipeline init failed: {e}, running headless");
        // ponytail: return dummy pipeline for headless mode (E2E testing)
        pipeline::Pipeline::dummy()
    }));
    // ponytail: pipeline start may fail without GStreamer; non-fatal for E2E
    if let Err(e) = pipeline.start() {
        tracing::warn!("Pipeline start failed: {e}, continuing headless");
    }

    // Phase 3: Create control handler (shared with metrics)
    // Phase 4: Create control handler (shared with metrics)
    let control_handler = control::ControlHandler::new();
    let frames_dropped = control_handler.frames_dropped.clone();
    let control_handler = Arc::new(Mutex::new(control_handler));

    // Phase 5: Build axum router (metrics)
    let core_metrics = audemsp_common::metrics::CoreMetrics::new();
    let shared_metrics = std::sync::Arc::new(std::sync::RwLock::new(core_metrics));
    let metrics_router = metrics::metrics_router(shared_metrics.clone());

let app = axum::Router::new()
    .merge(metrics_router)
    .layer(TimeoutLayer::new(Duration::from_secs(30)));

    // Determine bind address
    let bind_addr = "0.0.0.0:9801"; // ponytail: separate port from server (9800)

    let listener = match tokio::net::TcpListener::bind(bind_addr).await {
        Ok(l) => {
            tracing::info!("Listening on {}", bind_addr);
            l
        }
            Err(e) => {
                tracing::error!("Failed to bind {}: {}", bind_addr, e);
                return Err(anyhow::anyhow!("Failed to bind {bind_addr}: {e}"));
            }
        };

    tracing::info!("Host ready — session id={}", config.capture.source);

    // Phase 4: Connect to signaling and create WebRTC transport
    let signaling_url = config.server.signaling_url.clone();
    let psk = config
        .psk
        .clone()
        .unwrap_or_else(|| "audemsp-dev".to_string());
    let room_id = config.room.id.clone();

    const MAX_RETRIES: u32 = 5;
    let mut last_err = None;
    let client = SignalingClient::new(&signaling_url, &psk, &room_id);
    let (mut ws_sender, mut ws_receiver) = {
        let mut delay = std::time::Duration::from_secs(1);
        let mut result = None;
        for attempt in 1..=MAX_RETRIES {
            match client.connect().await {
                Ok(pair) => { result = Some(pair); break; }
                Err(e) => {
                    tracing::warn!(attempt, max = MAX_RETRIES, "Signaling connect failed: {e}, retrying in {delay:?}");
                    last_err = Some(e);
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(std::time::Duration::from_secs(16));
                }
            }
        }
        match result {
            Some(pair) => pair,
            None => {
                tracing::error!("Signaling connection failed after {MAX_RETRIES} attempts: {last_err:?}");
                return Err(anyhow::anyhow!("Signaling connection failed after {MAX_RETRIES} attempts: {last_err:?}"));
            }
        }
    };
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;


    // PipelineEngine: P2P path only — SFU uses audemsp-webrtc (C12)
    // ponytail: engine declared here so it's visible to cleanup at end of main()
    #[cfg(feature = "webrtc-p2p")]
    #[allow(unused_variables)]
    let engine = PipelineEngine::new(tokio::runtime::Handle::current());
    #[cfg(not(feature = "webrtc-p2p"))]
    let engine = (); // placeholder for cleanup scope

    // SFU produce (mediasoup) — via audemsp-webrtc abstraction
    if config.sfu_produce {
        tracing::info!("SFU produce mode — negotiating mediasoup transport");
        let peer_id = "host"; // ponytail: use config peer identity when available
        let sfu_room = room_id.clone();

        // Step 1: CreateWebRtcTransport (direction: Send)
        let create_transport = SignalingMessage::CreateWebRtcTransport {
            room_id: sfu_room.clone(),
            peer_id: peer_id.to_string(),
            direction: TransportDirection::Send,
        };
        let json = serde_json::to_string(&create_transport)?;
        ws_sender
            .send(Message::Text(json.into()))
            .await
            .map_err(|e| anyhow::anyhow!("SFU CreateWebRtcTransport send: {}", e))?;
        tracing::info!("SFU: CreateWebRtcTransport sent");

        // Step 2: Wait for WebRtcTransportCreated (with real ice_candidates from Part C)
        let (transport_id, ice_parameters, dtls_parameters, ice_candidates) = loop {
            match ws_receiver.next().await {
                Some(Ok(Message::Text(text))) => {
                    let msg: SignalingMessage = serde_json::from_str(&text)
                        .map_err(|e| anyhow::anyhow!("SFU parse: {}", e))?;
                    match msg {
                        SignalingMessage::WebRtcTransportCreated {
                            room_id: _, peer_id: _, transport_id, ice_parameters, dtls_parameters, ice_candidates,
                        } => break (transport_id, ice_parameters, dtls_parameters, ice_candidates),
                        SignalingMessage::Sdp { .. } | SignalingMessage::RTCIceCandidate { .. } => {
                            tracing::debug!("SFU: skipping P2P message");
                        }
                        SignalingMessage::Error { code, message } => {
                            return Err(anyhow::anyhow!("SFU error [{}]: {}", code, message));
                        }
                        other => {
                            return Err(anyhow::anyhow!("SFU: unexpected {:?}", other));
                        }
                    }
                }
                Some(Ok(other)) => return Err(anyhow::anyhow!("SFU: unexpected WS: {:?}", other)),
                Some(Err(e)) => return Err(anyhow::anyhow!("SFU: WS error: {}", e)),
                None => return Err(anyhow::anyhow!("SFU: WS closed")),
            }
        };
        let candidate_count = ice_candidates.as_ref().map_or(0, |v| v.len());
        tracing::info!("SFU: WebRtcTransportCreated id={}, candidates={}", transport_id, candidate_count);

        // Step 3: Create RTCPeerConnection via audemsp-webrtc (NOT webrtc-rs)
        let ice_servers: Vec<RTCIceServer> = config.server.ice_servers.iter()
            .map(|url| RTCIceServer {
                urls: vec![url.clone()],
                username: String::new(),
                password: String::new(),
            })
            .collect();
        let rtc_config = RTCConfiguration {
            ice_servers,
            ice_transport_type: RTCIceTransportPolicy::All,
        };
        let factory = RTCPeerConnectionFactory::new();
        let pc = factory.create_peer_connection(rtc_config).await
            .map_err(|e| anyhow::anyhow!("PC create: {}", e))?;

        // B4: Register ICE/PC state callbacks
        let produce_trigger = Arc::new(tokio::sync::Notify::new());
        let produce_ready = produce_trigger.clone();
        // ponytail: pc is Clone, so we can capture it in callbacks
        {
            let _pc = pc.clone();
            pc.on_ice_connection_state_change(move |state| {
                let _ = &_pc;
                tracing::info!("SFU ICE state: {:?}", state);
            });
        }
        {
            let _pc = pc.clone();
            let ready = produce_ready.clone();
            pc.on_peer_connection_state_change(move |state| {
                let _ = &_pc;
                tracing::info!("SFU PC state: {:?}", state);
                if state == RTCPeerConnectionState::Connected {
                    ready.notify_one();
                }
            });
        }

        // B1: Build remote SDP from real server ICE candidates (not 127.0.0.1)
        // PIT-48: a=candidate 行必须位于 m= 行之后（media section 内）——
        // 会话级 candidate 被 libwebrtc 忽略 → remote candidate 丢失 → ICE 不发起 STUN
        let remote_sdp = {
            let fp = &dtls_parameters.fingerprints[0];
            let mut lines = vec![
                "v=0".to_string(),
                "o=- 0 0 IN IP4 0.0.0.0".to_string(),
                "s=-".to_string(),
                "t=0 0".to_string(),
                "a=group:BUNDLE video".to_string(),
                "a=ice-lite".to_string(),
                format!("a=ice-ufrag:{}", ice_parameters.username_fragment),
                format!("a=ice-pwd:{}", ice_parameters.password),
                format!("a=fingerprint:{} {}", fp.algorithm.to_lowercase(), fp.value),
                "a=setup:actpass".to_string(), // ICE-Lite responder requirement
            ];
            let conn_ip = ice_candidates.as_ref()
                .and_then(|cs| cs.iter().find(|c| !c.ip.contains(".local")))
                .map(|c| c.ip.clone())
                .unwrap_or_else(|| "0.0.0.0".to_string());
            const H264_PT: u16 = 101; // ponytail: get from rtp_capabilities when server supports it
            lines.extend_from_slice(&[
                format!("m=video 7 UDP/TLS/RTP/SAVPF {}", H264_PT),
                format!("c=IN IP4 {}", conn_ip),
                "a=rtcp-mux".to_string(),
                "a=mid:video".to_string(),
                "a=sendonly".to_string(),
                format!("a=rtpmap:{} H264/90000", H264_PT),
                format!("a=fmtp:{} level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f", H264_PT),
            ]);
            // candidates 必须在 media section 内（m= 行之后）
            if let Some(ref candidates) = ice_candidates {
                for c in candidates {
                    if c.ip.contains(".local") { continue; } // skip mDNS
                    let ctype = match c.candidate_type.as_str() {
                        "host" => "host", "srflx" => "srflx",
                        "prflx" => "prflx", "relay" => "relay",
                        _ => "host",
                    };
                    lines.push(format!(
                    "a=candidate:{} 1 {} {} {} {} typ {}",
                        c.foundation, c.protocol.to_uppercase(), c.priority,
                        c.ip, c.port, ctype
                    ));
                }
            }
            lines.push("a=end-of-candidates".to_string());
            lines.push(String::new());
            lines.join("\r\n")
        };
        tracing::debug!("SFU remote SDP:\n{}", remote_sdp);

        let remote_desc = RTCSessionDescription::new(RTCSdpType::Offer, remote_sdp);
        pc.set_remote_description(&remote_desc).await
            .map_err(|e| anyhow::anyhow!("set remote: {}", e))?;

        // B2: add_track BEFORE create_answer (correct timing!)
        let track_id = pc.add_track("video", TrackKind::Video)
            .map_err(|e| anyhow::anyhow!("add_track: {}", e))?;
        let track_ref = pc.get_track(&track_id)
            .ok_or_else(|| anyhow::anyhow!("track not found after add_track"))?;
        let video_track = match track_ref {
            TrackRef::Sender(s) => s,
            _ => return Err(anyhow::anyhow!("expected sender track")),
        };
        tracing::info!("SFU: video track added (id={})", track_id);

        let answer = pc.create_answer(&RTCAnswerOptions::default()).await
            .map_err(|e| anyhow::anyhow!("create answer: {}", e))?;
        tracing::debug!("SFU local answer SDP:\n{}", answer.sdp);
        pc.set_local_description(&answer).await
            .map_err(|e| anyhow::anyhow!("set local: {}", e))?;

        // B3: Extract DTLS fingerprint via audemsp-webrtc API (not SDP parsing)
        let fp_hex = pc.local_dtls_fingerprint()
            .ok_or_else(|| anyhow::anyhow!("no DTLS fingerprint"))?;
        let connect = SignalingMessage::ConnectWebRtcTransport {
            room_id: sfu_room.clone(),
            peer_id: peer_id.to_string(),
            transport_id: transport_id.clone(),
            dtls_parameters: DtlsParameters {
                fingerprints: vec![Fingerprint {
                    algorithm: "sha-256".to_string(),
                    value: fp_hex,
                }],
                role: "client".to_string(),
            },
        };
        let json = serde_json::to_string(&connect)?;
        ws_sender
            .send(Message::Text(json.into()))
            .await
            .map_err(|e| anyhow::anyhow!("SFU ConnectWebRtcTransport: {}", e))?;
        tracing::info!("SFU: ConnectWebRtcTransport sent");

        // B6: Wait for ICE/DTLS connection (30s timeout)
        match tokio::time::timeout(Duration::from_secs(30), produce_trigger.notified()).await {
            Ok(()) => tracing::info!("SFU: PC Connected, sending Produce"),
            Err(_) => return Err(anyhow::anyhow!("SFU: ICE/DTLS connection timeout after 30s")),
        }

        // Step 4: Produce video (B6: 10s WS timeout)
        const H264_PT2: u16 = 101;
        let produce = SignalingMessage::Produce {
            room_id: sfu_room,
            transport_direction: TransportDirection::Send,
            kind: MediaKind::Video,
            rtp_parameters: serde_json::json!({
                "mid": "0",
                "codecs": [{
                    "mimeType": "video/H264",
                    "payloadType": H264_PT2,
                    "clockRate": 90000
                }],
                "headerExtensions": [],
                "encodings": [{"ssrc": 12345}],
                "rtcp": {"reducedSize": true}
            }),
        };
        let json = serde_json::to_string(&produce)?;
        match tokio::time::timeout(Duration::from_secs(10), ws_sender.send(Message::Text(json.into()))).await {
            Ok(Ok(())) => tracing::info!("SFU: Produce (Video) sent"),
            Ok(Err(e)) => return Err(anyhow::anyhow!("SFU Produce send error: {}", e)),
            Err(_) => return Err(anyhow::anyhow!("SFU Produce send timeout after 10s")),
        }

        // B5: Spawn I420 frame sender using write_raw_i420 (no audemsp-codec needed)
        let video_track_send = video_track.clone();
        tokio::spawn(async move {
            let width: u32 = 640;
            let height: u32 = 480;
            let y_size = (width * height) as usize;
            let uv_size = (width * height / 4) as usize;
            let frame_size = y_size + 2 * uv_size;
            let mut frame = vec![0u8; frame_size];
            // U/V planes: neutral gray (128)
            frame[y_size..y_size + uv_size].fill(128);
            frame[y_size + uv_size..].fill(128);
            let mut seq = 0u64;
            loop {
                tokio::time::sleep(Duration::from_millis(33)).await; // ~30fps
                let y_plane = &mut frame[..y_size];
                let offset = (seq % 200) as u8;
                for y in 0..height {
                    for x in 0..width {
                        let idx = (y * width + x) as usize;
                        let color = if ((x / 40 + y / 40 + offset as u32 / 20) & 1) == 0 { 40 } else { 200 };
                        y_plane[idx] = color;
                    }
                }
                match video_track_send.write_raw_i420(&frame, width, height).await {
                    Ok(()) => {}
                    Err(e) => {
                        tracing::warn!("SFU write_raw_i420 error: {}", e);
                        break;
                    }
                }
                seq += 1;
                if seq % 90 == 0 {
                    tracing::debug!("SFU: sent {} frames", seq);
                }
            }
        });
        tracing::info!("SFU produce transport {} ready — I420 frame loop started", transport_id);
    } else {

    // P2P transport path — gated behind webrtc-p2p feature
    #[cfg(feature = "webrtc-p2p")]
    {
    // ponytail: wait for remote to join room before sending SDP offer
    // ponytail: wait for remote to join room before sending SDP offer

    let (webrtc_transport, dc_events) =
        webrtc_transport::WebrtcTransport::new(ws_sender, room_id)
            .await
            .map_err(|e| {
                tracing::error!("WebRTC transport creation failed: {e}");
                anyhow::anyhow!("WebRTC transport creation failed: {e}")
            })?;

    let webrtc = Arc::new(webrtc_transport);

    // Spawn WS receiver loop — handles incoming SDP answers and ICE candidates
    let ws_webrtc = webrtc.clone();
    let ws_receiver_handle = tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(msg) => {
                    if let Ok(text) = msg.to_text() {
                        tracing::debug!("WS received: {}", text);
                        if let Ok(sig_msg) =
                            serde_json::from_str::<SignalingMessage>(text)
                        {
                            match sig_msg {
                                SignalingMessage::Sdp { sdp, .. } => {
                                    // ponytail: ignore offer echo from server relay; only process answers
                                    let sdp_type = serde_json::from_str::<serde_json::Value>(&sdp)
                                        .ok()
                                        .and_then(|v| v.get("type")?.as_str().map(String::from));
                                    if sdp_type.as_deref() != Some("answer") {
                                        tracing::debug!("Ignoring non-answer SDP (type={sdp_type:?})");
                                        continue;
                                    }
                                    tracing::info!("Received SDP answer, setting remote description");
                                    match ws_webrtc.handle_answer(&sdp).await {
                                        Ok(()) => tracing::info!("Remote description set"),
                                        Err(e) => tracing::error!("Failed to set remote description: {e}"),
                                    }
                                }
                                SignalingMessage::RTCIceCandidate { candidate, sdp_mid, sdp_mline_index, .. } => {
                                    let candidate_json = serde_json::json!({
                                        "candidate": candidate,
                                        "sdpMid": sdp_mid,
                                        "sdpMLineIndex": sdp_mline_index,
                                    }).to_string();
                                    match ws_webrtc.handle_remote_ice(&candidate_json).await {
                                        Ok(()) => tracing::debug!("ICE candidate added"),
                                        // ponytail: ICE-before-SDP race is expected; non-fatal
                                        Err(e) => tracing::debug!("ICE candidate deferred: {e}"),
                                    }
                                }
                                SignalingMessage::RoomLeave { .. } => {
                                    tracing::info!("Peer left room");
                                }
                                SignalingMessage::WebRtcTransportCreated { transport_id, .. } => {
                                    // ponytail: logged for SFU path; SFU skeleton consumes this synchronously,
                                    // this is a fallback for async arrival
                                    tracing::info!("SFU: WebRtcTransportCreated id={} (async)", transport_id);
                                }
                                _ => {} // ponytail: ignore other variants
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!("WS receive error: {e}"),
            }
        }
        tracing::warn!("WS receiver loop ended");
    });
    background_tasks.push(ws_receiver_handle);

    // Spawn DC event loop — logs lifecycle events
    let dc_event_handle = tokio::spawn(async move {
        webrtc_transport::run_dc_event_loop(dc_events, control_handler.clone()).await;
    });
    background_tasks.push(dc_event_handle);

    // Phase 7: Emergency UDP listener (background)
    let emergency_handle = tokio::spawn(async move {
        let listener = match emergency::EmergencyListener::bind(9999).await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("Emergency listener failed to bind: {e}");
                return;
            }
        };
        if let Err(e) = listener.listen().await {
            tracing::error!("Emergency listener error: {e}");
        }
    });
    background_tasks.push(emergency_handle);

    // PipelineEngine: orchestrate capture → encode → WebRTC push
    let push_pipeline = pipeline.clone();
    let push_webrtc = webrtc.clone();
    let shared_m = shared_metrics.clone();

    engine.add_chain(
        "capture".into(),
        Box::new(engine_adapters::GstCaptureSource::new(push_pipeline.clone())),
        vec![],
        vec![Box::new(engine_adapters::WebrtcOutputSink::new(
            push_webrtc.clone(),
        ))],
    ).expect("Failed to add capture chain");

    engine.start().expect("Failed to start engine");

    }  // end P2P cfg
    #[cfg(not(feature = "webrtc-p2p"))]
    {
        tracing::error!("SFU produce disabled and webrtc-p2p not enabled; no transport available");
        return Err(anyhow::anyhow!("No WebRTC transport feature available"));
    }
    }  // end else (P2P path)
    // Start metrics updater: sync dropped frames counter
    let dropped = frames_dropped;
    let metrics_updater = tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            let _ = dropped.load(std::sync::atomic::Ordering::Relaxed);
        }
    });
    background_tasks.push(metrics_updater);

    // Run server (blocks until shutdown signal)
    let server = axum::serve(listener, app).with_graceful_shutdown(async {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Shutting down...");
    });

    if let Err(e) = server.await {
        tracing::error!("Server error: {}", e);
    }
    tracing::info!("Shutdown complete");

    // Stop pipeline
    if let Err(e) = pipeline.stop() {
        tracing::error!("Pipeline stop error: {}", e);
    }

    // Clean up background tasks
    // Stop engine before aborting tasks
    #[cfg(feature = "webrtc-p2p")]
    let _ = engine.stop().await;

    for handle in background_tasks {
        handle.abort();
    }
    // ponytail: brief wait for graceful abort before runtime drops
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // persist_handle runs for session lifetime, abort last
    persist_handle.abort();

Ok(())
}

/// Parse "WIDTHxHEIGHT" into (width, height). Defaults to 1280x720.
fn parse_resolution(res: &str) -> (u32, u32) {
    let parts: Vec<&str> = res.split('x').collect();
    if parts.len() == 2 {
        let w = parts[0].parse().unwrap_or(1280);
        let h = parts[1].parse().unwrap_or(720);
        (w, h)
    } else {
        (1280, 720)
    }
}

/// Generate a default host config YAML for headless/E2E fallback.
fn default_host_config() -> String {
    r#"
server:
  signaling_url: "ws://localhost:9800/ws"
  ice_servers: []
capture:
  source: "test_pattern"
  resolution: "1280x720"
  framerate: 30
  device: null
encoder:
  backend: "auto"
  bitrate_kbps: 2000
  keyframe_interval: 60
room:
  id: "default-room"
psk: "audemsp-dev"
"#.to_string()
}
