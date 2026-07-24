//! OMSPBase Host — headless capture + encode + WebRTC push.
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

use futures_util::StreamExt;
use omspbase_media::engine::PipelineEngine;
use omspbase_common::protocol::SignalingMessage;
use signaling::SignalingClient;
mod config;
mod control;
mod emergency;
mod metrics;
mod pipeline;
mod engine_adapters;
mod session;
mod signaling;
mod transport;
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

    tracing::info!("OMSPBase Host v{} starting", env!("CARGO_PKG_VERSION"));

    // Parse config — collect args once for bounds-safe access
    let config_path = {
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 3 && args[2] == "--config" {
            args[3].clone()
        } else {
            "/opt/omspbase/etc/host.conf".to_string()
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
    let core_metrics = omspbase_common::metrics::CoreMetrics::new();
    let shared_metrics = std::sync::Arc::new(std::sync::RwLock::new(core_metrics));
    let metrics_router = metrics::metrics_router(shared_metrics.clone());

let app = axum::Router::new()
    .merge(metrics_router)
    .layer(TimeoutLayer::new(Duration::from_secs(30)));
        .merge(metrics_router);

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
        .unwrap_or_else(|| "omspbase-dev".to_string());
    let room_id = config.room.id.clone();

    const MAX_RETRIES: u32 = 5;
    let mut last_err = None;
    let client = SignalingClient::new(&signaling_url, &psk, &room_id);
    let (ws_sender, mut ws_receiver) = {
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

    let engine = PipelineEngine::new(tokio::runtime::Handle::current());

    engine.add_chain(
        "capture".into(),
        Box::new(engine_adapters::GstCaptureSource::new(push_pipeline.clone())),
        vec![],
        vec![Box::new(engine_adapters::WebrtcOutputSink::new(
            push_webrtc.clone(),
        ))],
    ).expect("Failed to add capture chain");

    engine.start().expect("Failed to start engine");

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
psk: "omspbase-dev"
"#.to_string()
}
