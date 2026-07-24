use std::time::Duration;
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
use futures_util::FutureExt;
use omspbase_server::config;
use omspbase_server::monitor;
use omspbase_server::signaling;

/// Entry point — install panic hook, then run server with graceful shutdown.
fn main() {
    // ── Panic boundary ───────────────────────────────────────────────────────
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_string());
        // Log before the process aborts — tracing may not flush in time,
        // so also emit to stderr as a fallback.
        eprintln!("FATAL PANIC at {location}: {msg}");
        tracing::error!(panic.location = %location, panic.message = %msg, "Server panic");
    }));

    // Wrap async body in catch_unwind so panics are logged before exit.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    let result = rt.block_on(async {
        std::panic::AssertUnwindSafe(run_server()).catch_unwind().await
    });

    match result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::error!("Server error: {}", e);
            std::process::exit(1);
        }
        Err(_panic) => {
            // Already logged by panic hook
            tracing::error!("Server terminated due to panic");
            std::process::exit(1);
        }
    }
}

async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap()),
        )
        .init();

    tracing::info!(
        "OMSPBase Server v{} starting",
        env!("CARGO_PKG_VERSION")
    );

    // Parse config — collect args once for bounds-safe access
    let config_path = {
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 3 && args[2] == "--config" {
            args[3].clone()
        } else {
            "/opt/oomspbase/etc/server.conf".to_string()
        }
    };
    let config = match config::load(&config_path) {
        Ok(c) => {
            tracing::info!("Config loaded from {config_path}");
            c
        }
        Err(e) => {
            tracing::warn!("Config {config_path}: {e}, using defaults");
            serde_yaml::from_str(DEFAULT_SERVER_CONFIG).unwrap()
        }
    };

    // Create the signaling server (shared state for WebSocket rooms)
    #[cfg(feature = "sfu-mediasoup")]
    let signaling_server = {
        use std::sync::Arc;
        use omspbase_server::sfu;

        match sfu::SfuManager::new().await {
            Ok(m) => {
                tracing::info!("SFU manager initialized (mediasoup)");
                signaling::SignalingServer::new(Arc::new(m), config.ws_max_message_size)
            }
            Err(e) => {
                tracing::info!("SFU manager skipped: {e}");
                panic!("sfu-mediasoup feature enabled but worker failed: {e}");
            }
        }
    };
    #[cfg(not(feature = "sfu-mediasoup"))]
    let signaling_server = signaling::SignalingServer::new(config.ws_max_message_size);

    // Build axum router
    let signaling_router = signaling::signaling_router(signaling_server.clone());
    let monitor_router = monitor::monitor_router(signaling_server.clone());

    let app = axum::Router::new()
        .merge(signaling_router)
        .merge(monitor_router);

    // Rate limiting: per-IP governor using config.rate_limit requests/sec
    let governor_conf = Box::leak(Box::new(
        GovernorConfigBuilder::default()
            .per_second(u64::from(config.rate_limit).max(1))
            .burst_size((u64::from(config.rate_limit).max(1) * 2) as u32)
            .finish()
            .unwrap(),
    ));
    let app = app.layer(GovernorLayer {
        config: governor_conf,
    });

    // Bind address from omspbase_common config
    let bind_addr = format!("{}:{}", config.listen.host, config.listen.port);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await.map_err(|e| {
        format!("Failed to bind {}: {}", bind_addr, e)
    })?;

    tracing::info!("Listening on {}", bind_addr);

    // Notify systemd / process manager
    tracing::info!("Server ready on {}", bind_addr);

    // Run server with graceful shutdown + connection draining
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Shutdown signal received, initiating graceful shutdown...");

        // Signal all WebSocket connections to close
        signaling_server.shutdown();

        // Drain active connections with a 30-second timeout
        let drain_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let remaining = signaling_server.active_connections();
            if remaining == 0 {
                tracing::info!("All connections drained, shutting down");
                break;
            }
            if tokio::time::Instant::now() >= drain_deadline {
                tracing::warn!(
                    "Shutdown timeout reached (30s) with {} active connections, forcing exit",
                    remaining
                );
                break;
            }
            tracing::info!("Draining: {} active connections remaining", remaining);
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });

    if let Err(e) = server.await {
        tracing::error!("Server error: {}", e);
    }

    tracing::info!("Shutdown complete");
    Ok(())
}

/// Default server config for headless/E2E fallback.
const DEFAULT_SERVER_CONFIG: &str = r#"
listen:
  host: "0.0.0.0"
  port: 9800
room_capacity: 10
rate_limit: 100
psk: "omspbase-dev"
"#;
