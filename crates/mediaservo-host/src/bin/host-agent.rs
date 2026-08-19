//! host-agent: 信令网关（Task D1，D-H6）— 各 host 进程 WS 连本地端口，
//! agent 聚合单 WS 上 Server（一车一会话）。协议语义见 [`mediaservo_host::gateway`]。
//!
//! 用法: `host-agent [--port <本地端口>] [--remote <ws url>] [--psk <psk>] [--room <整车房间>]`
//! 缺省: 端口 17980；remote/psk 走 `SFU_E2E_WS_URL`/`SFU_E2E_PSK`（缺省
//! `ws://127.0.0.1:9800/ws` / `mediaservo-dev`，对齐 streamer/e2e 约定）；room 缺省
//! `vehicle`（D3 起由 host.toml 配置接入）。

use mediaservo_host::gateway::{run_gateway, GatewayConfig};
use mediaservo_host::init_logging;

const USAGE: &str = "用法: host-agent [--port <本地端口>] [--remote <ws url>] [--psk <psk>] [--room <房间>]";

fn parse_args() -> Result<GatewayConfig, String> {
    let mut cfg = GatewayConfig::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                cfg.local_port = args
                    .next()
                    .ok_or("--port 缺值")?
                    .parse()
                    .map_err(|e| format!("--port 无效: {e}"))?
            }
            "--remote" => cfg.remote_url = args.next().ok_or("--remote 缺值")?,
            "--psk" => cfg.psk = args.next().ok_or("--psk 缺值")?,
            "--room" => cfg.room = args.next().ok_or("--room 缺值")?,
            _ => return Err(format!("未知参数: {arg}\n{USAGE}")),
        }
    }
    // 环境变量缺省（对齐 field/e2e_sfu 外部 server 约定）
    if cfg.remote_url == "ws://127.0.0.1:9800/ws" {
        cfg.remote_url =
            std::env::var("SFU_E2E_WS_URL").unwrap_or_else(|_| cfg.remote_url.clone());
    }
    if cfg.psk == "mediaservo-dev" {
        cfg.psk = std::env::var("SFU_E2E_PSK").unwrap_or_else(|_| cfg.psk.clone());
    }
    Ok(cfg)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging("agent");
    let cfg = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let port = run_gateway(cfg).await.map_err(|e| std::io::Error::other(e))?;
    tracing::info!(port, "host-agent 网关就绪");
    wait_shutdown().await;
    Ok(())
}

/// 等待 SIGINT/SIGTERM（unix 主路径；其他平台仅 ctrl_c）。
async fn wait_shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await.expect("ctrl_c handler");
    }
}
