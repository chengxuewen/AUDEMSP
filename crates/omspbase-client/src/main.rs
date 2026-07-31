use std::process;
use std::time::Duration;
use tower_http::timeout::TimeoutLayer;
mod config;
mod control;
mod decode;
mod signaling;
mod transport;
mod webrtc_transport;
mod engine_adapters;

// ponytail: no direct CoreError use in main — errors handled via config load / pipeline API
use omspbase_media::engine::PipelineEngine;
use omspbase_common::metrics::CoreMetrics;

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap()),
        )
        .init();

    tracing::info!("OMSPBase Remote v{} starting", env!("CARGO_PKG_VERSION"));

    // Parse config — collect args once for bounds-safe access
    let config_path = {
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 3 && args[2] == "--config" {
            args[3].clone()
        } else {
            "/opt/oomspbase/etc/remote.conf".to_string()
        }
    };
    let config = match config::load(&config_path) {
        Ok(c) => {
            tracing::info!("Config loaded: server={}", c.server.signaling_url);
            c
        }
        Err(e) => {
            tracing::warn!("Config {config_path}: {e}, using defaults");
            serde_yaml::from_str(DEFAULT_REMOTE_CONFIG).unwrap()
        }
    };// ponytail: fallback to defaults for E2E, add config wizard when needed

    // ponytail: media/control params hardcoded for MVP; promote to config when config schema extends
    let display_name = "default";
    let width: u32 = 1280;
    let height: u32 = 720;
    let decoder: Option<&str> = None;
    let hmac_key = "omspbase-control";

    // Create + start decode pipeline, then share via Arc for receive loop
    let mut pipeline = decode::DecodePipeline::new(display_name, width, height, decoder);
    if let Err(e) = pipeline.start() {
        tracing::warn!("Decode pipeline start failed: {e}, continuing headless");
    };
    let pipeline = std::sync::Arc::new(pipeline);
    // Phase 3: Build axum router (health + config + metrics)
    let metrics = std::sync::Arc::new(CoreMetrics::new());
    let metrics_arc = metrics.clone();
let app = axum::Router::new()
    .route("/health", axum::routing::get(health_handler))
    .route("/config", axum::routing::get(config_handler))
    .route("/metrics", axum::routing::get(move || {
        let m = metrics_arc.clone();
        async move { m.encode() }
    }))
    .layer(TimeoutLayer::new(Duration::from_secs(30)));

    let bind_addr = "0.0.0.0:9101";

    let listener = match tokio::net::TcpListener::bind(bind_addr).await {
        Ok(l) => {
            tracing::info!("Config UI listening on {bind_addr}");
            l
        }
        Err(e) => {
            tracing::error!("Failed to bind {bind_addr}: {e}");
            process::exit(1);
        }
    };
    // Phase 5: Connect to server signaling via WebSocket
    let psk = config.psk.as_deref().unwrap_or("omspbase-dev").to_string();
    let (frame_tx, frame_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(10);


    let signaling = signaling::SignalingClient::new_with_frame_tx(
        &config.server.signaling_url,
        &psk,
        &config.room.id,
        frame_tx,
    );
    tokio::spawn(async move {
        const MAX_RETRIES: u32 = 5;
        let mut delay = std::time::Duration::from_secs(1);
        for attempt in 1..=MAX_RETRIES {
            match signaling.connect().await {
                Ok(()) => {
                    tracing::info!(attempt, "Signaling connection completed");
                    return;
                }
                Err(e) => {
                    tracing::warn!(attempt, max = MAX_RETRIES, "Signaling failed: {e}, retrying in {delay:?}");
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(std::time::Duration::from_secs(16));
                }
            }
        }
        tracing::error!("Signaling connection failed after {MAX_RETRIES} attempts, giving up");
    });
    tracing::info!("Signaling connection initiated to {}", config.server.signaling_url);

    // PipelineEngine: orchestrate WebRTC frame receive → decode
    let engine = PipelineEngine::new(tokio::runtime::Handle::current());

    engine.add_chain(
        "receive".into(),
        Box::new(engine_adapters::FrameSource::new(frame_rx)),
        vec![],
        vec![Box::new(engine_adapters::DecodeSink::new(pipeline.clone()))],
    ).expect("Failed to add receive chain");

    engine.start().expect("Failed to start engine");


    // Report ready: active_connections bumped for startup
    metrics.active_connections.inc();
    tracing::info!("Remote ready (server: {})", config.server.signaling_url);

    // Run server with graceful shutdown
    let server = axum::serve(listener, app).with_graceful_shutdown(async {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Shutting down...");
    });

    if let Err(e) = server.await {
        tracing::error!("Server error: {e}");
    }

    // Stop engine
    if let Err(e) = engine.stop().await {
        tracing::error!("Engine stop error: {e}");
    }

    // Stop pipeline
    if let Err(e) = pipeline.stop() {
        tracing::error!("Decode pipeline stop error: {e}");
    }

    metrics.active_connections.dec();
    tracing::info!("Shutdown complete");
}

/// Health check endpoint.
async fn health_handler() -> &'static str {
    "OK"
}

/// Config status endpoint (safe subset, no secrets).
async fn config_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "running",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Default remote config for headless/E2E fallback.
const DEFAULT_REMOTE_CONFIG: &str = r#"
server:
  signaling_url: "ws://localhost:9800/ws"
  ice_servers: []
psk: "omspbase-dev"
"#;
