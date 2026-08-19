//! host-controller: 控制进程（Task F1）— 纯 DC PC P2P 控制通道。
//!
//! 用法: `host-controller [--gateway <本地网关 ws url>]`（缺省
//! `ws://127.0.0.1:17980/ws`，D2 本地网关）。
//!
//! 流程: SignalClient 经本地网关（信封 wire 无 PSK；整车 PSK 在 host-agent
//! 远端）加入本地房间 `control`（网关拦截并重写为整车房间）→ 建纯 DC PC
//! （create_data_channel × 3: chassis reliable-ordered / gimbal
//! partial-reliable / light reliable-ordered，D-H3）→ **controller 为 offerer**
//! （legacy webrtc_transport 惯例：host 发起 offer，舱端 client 应答；Sdp/ICE
//! 经网关 P2P relay，网关 p2p_owner 路由，D1）→ DC 消息路由到执行器接口
//! （F1 = `StubActuator`：日志 + 回执）→ 同通道回 ACK（`{ack, result}`，
//! [`mediaservo_host::control`]）。
//!
//! 失败语义（C15 + PIT-87 自愈惯例）：信令断开 / ICE Failed / 会话错误 →
//! 打日志退出 1，部署侧 restart_policy=always 拉起。

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use mediaservo_common::protocol::{PeerRole, SignalingMessage};
use mediaservo_host::control::{
    Actuator, ControlAck, StubActuator, parse_envelope,
};
use mediaservo_link::{SignalClient, SignalEvent};
use mediaservo_webrtc::data_channel::{RTCDataChannel, RTCDataChannelEvent, RTCDataChannelInit};
use mediaservo_webrtc::peer_connection::{RTCConfiguration, RTCIceCandidate};
use mediaservo_webrtc::sdp::{RTCSdpType, RTCSessionDescription};
use mediaservo_webrtc::traits::PeerConnectionApi;
use mediaservo_webrtc::{RTCPeerConnection, RTCPeerConnectionFactory};
use tokio::sync::mpsc;

/// 本地房间名（网关拦截并重写为整车房间；子进程本地房间仅作下行改写目标）。
const ROOM: &str = "control";
/// 本地信封 src（网关子进程标识）。
const SRC: &str = "host-controller";
/// ICE Failed 自愈退出前等待（PIT-87 同款：状态收敛后退出待拉起）。
/// ICE Failed 自愈退出前等待（PIT-87 同款：状态收敛后退出待拉起）。
const ICE_FAILED_WAIT: Duration = Duration::from_secs(1);

const USAGE: &str = "用法: host-controller [--gateway <本地网关 ws url>]";

/// 通道可靠性（D-H3）: chassis/light 可靠有序（急停/开关类命令）；
/// gimbal partial-reliable（云台连续调节可丢帧，低延迟优先）。
fn channel_init(label: &str) -> RTCDataChannelInit {
    match label {
        "gimbal" => RTCDataChannelInit {
            ordered: false,
            max_retransmits: Some(5),
            ..Default::default()
        },
        _ => RTCDataChannelInit::default(), // chassis / light: reliable ordered
    }
}

fn parse_args() -> Result<String, String> {
    gateway_from(std::env::args().skip(1))
}

/// 纯参数解析（可单测）：`--gateway <url>`；缺省本地网关（D2）。
fn gateway_from(args: impl Iterator<Item = String>) -> Result<String, String> {
    let mut args = args.peekable();
    let mut gateway: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--gateway" => gateway = Some(args.next().ok_or("--gateway 缺值")?),
            _ => return Err(format!("未知参数: {arg}\n{USAGE}")),
        }
    }
    Ok(gateway.unwrap_or_else(|| "ws://127.0.0.1:17980/ws".to_string()))
}

/// 等待 SIGINT/SIGTERM（unix 主路径；其他平台仅 ctrl_c）。
async fn shutdown_signal() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    Ok(())
}

/// 单通道路由：spool 接收 → 解析信封 → actuator → 同通道回 ACK。
async fn route_channel(dc: RTCDataChannel, actuator: Arc<dyn Actuator>) {
    let label = dc.label().to_string();
    let mut rx = dc.spool().await;
    tracing::info!(label, "DC 路由启动");
    while let Some(ev) = rx.recv().await {
        match ev {
            RTCDataChannelEvent::Open => tracing::info!(label, "DC open"),
            RTCDataChannelEvent::Closed => {
                tracing::info!(label, "DC closed");
                break;
            }
            RTCDataChannelEvent::Error(e) => tracing::warn!(label, "DC error: {e}"),
            RTCDataChannelEvent::Message(m) => {
                let env = match parse_envelope(&m.data) {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!(label, len = m.data.len(), "信封解析失败: {e}");
                        continue;
                    }
                };
                let ack = match actuator.on_command(&label, &env) {
                    Ok(result) => ControlAck::ok(env.seq, result),
                    Err(e) => {
                        // C15: 错误响应必须打日志（回执已发，但本侧可观测性不能丢）
                        tracing::warn!(
                            channel = %label,
                            cmd = %env.cmd,
                            seq = env.seq,
                            error = %e,
                            "actuator 命令失败"
                        );
                        ControlAck::err(env.seq, e)
                    }
                };
                if let Ok(json) = serde_json::to_string(&ack) {
                    if let Err(e) = dc.send_text(&json).await {
                        tracing::warn!(label, seq = env.seq, "ACK 发送失败: {e}");
                    }
                }
            }
        }
    }
}

/// 建立纯 DC PC：注册回调、创建 3 通道、发起 offer。
async fn setup_pc(
    pc: &RTCPeerConnection,
    actuator: Arc<dyn Actuator>,
    ice_tx: mpsc::UnboundedSender<RTCIceCandidate>,
    labels: &[&str],
) -> Result<(), String> {
    // 对端创建的通道（F1 offerer 路径不预期，防御性路由）
    let a = actuator.clone();
    pc.on_data_channel(move |dc| {
        tokio::spawn(route_channel(dc, a.clone()));
    });
    pc.on_ice_candidate(move |candidate| {
        let _ = ice_tx.send(candidate);
    });
    for label in labels {
        let dc = pc
            .create_data_channel(label, channel_init(label))
            .await
            .map_err(|e| format!("create_data_channel {label}: {e}"))?;
        tokio::spawn(route_channel(dc, actuator.clone()));
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    mediaservo_host::init_logging("controller");
    let gateway = match parse_args() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };

    // 信令：经本地网关（D2 信封 wire；网关拦截 RoomJoin 合成 RoomJoined）
    let signal = match SignalClient::new_gateway(&gateway, SRC, ROOM, PeerRole::Host)
        .connect()
        .await
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("controller: 信令连接失败: {e}");
            return ExitCode::from(1);
        }
    };
    tracing::info!(room = %signal.room_id(), "controller 已加入本地房间");

    // 纯 DC PC（无 track）
    let factory = RTCPeerConnectionFactory::new();
    let pc = match factory
        .create_peer_connection(RTCConfiguration::default())
        .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("controller: create_peer_connection 失败: {e}");
            return ExitCode::from(1);
        }
    };

    // ICE 候选上行通道（回调同步线程 → 主循环异步发送）
    let (ice_tx, mut ice_rx) = mpsc::unbounded_channel::<RTCIceCandidate>();
    let labels = ["chassis", "gimbal", "light"];
    let actuator: Arc<dyn Actuator> = Arc::new(StubActuator);
    if let Err(e) = setup_pc(&pc, actuator, ice_tx, &labels).await {
        eprintln!("controller: {e}");
        return ExitCode::from(1);
    }

    // ICE Failed 自愈（PIT-87 模式：状态收敛后退出待拉起）
    let (fail_tx, mut fail_rx) = mpsc::unbounded_channel::<()>();
    pc.on_ice_connection_state_change(move |state| {
        if state == mediaservo_webrtc::peer_connection::RTCIceConnectionState::Failed {
            tracing::error!("ICE Failed — 退出待重启（PIT-87 自愈）");
            let _ = fail_tx.send(());
        }
    });

    // 标准 offerer 协商（legacy 惯例：host 发起 offer）
    let offer = match pc.create_offer(&Default::default()).await {
        Ok(o) => o,
        Err(e) => {
            eprintln!("controller: create_offer 失败: {e}");
            return ExitCode::from(1);
        }
    };
    if let Err(e) = pc.set_local_description(&offer).await {
        eprintln!("controller: set_local_description 失败: {e}");
        return ExitCode::from(1);
    }
    let offer_json = match serde_json::to_string(&offer) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("controller: 序列化 offer 失败: {e}");
            return ExitCode::from(1);
        }
    };
    if let Err(e) = signal
        .send(SignalingMessage::Sdp {
            room_id: ROOM.into(),
            target: None,
            sdp: offer_json,
        })
        .await
    {
        eprintln!("controller: 发送 offer 失败: {e}");
        return ExitCode::from(1);
    }
    tracing::info!("offer 已发送（等待舱端 answer）");

    // 远端候选在 answer 落地前缓存（libwebrtc 协商前 add_ice_candidate 不可靠）
    let mut pending_ice: Vec<RTCIceCandidate> = Vec::new();
    let mut remote_set = false;
    let mut signal_events = signal.events();
    let mut exit_code: u8 = 0;

    println!(
        "controller ready: gateway={gateway} room={ROOM} labels={}",
        labels.join(",")
    );

    'run: loop {
        tokio::select! {
            sig = shutdown_signal() => match sig {
                Ok(()) => break 'run,
                Err(e) => {
                    eprintln!("controller: 信号处理失败: {e}");
                    exit_code = 1;
                    break 'run;
                }
            },
            _ = fail_rx.recv() => {
                // ICE Failed：短暂等待状态收敛后退出（PIT-87）
                tokio::time::sleep(ICE_FAILED_WAIT).await;
                exit_code = 1;
                break 'run;
            }
            ev = signal_events.recv() => match ev {
                Ok(SignalEvent::Message(SignalingMessage::Sdp { sdp, .. })) => {
                    let desc = match serde_json::from_str::<RTCSessionDescription>(&sdp) {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::warn!("answer 解析失败: {e}");
                            continue;
                        }
                    };
                    if desc.sdp_type != RTCSdpType::Answer {
                        tracing::warn!(sdp_type = %desc.sdp_type, "非 answer Sdp，忽略");
                        continue;
                    }
                    match pc.set_remote_description(&desc).await {
                        Ok(()) => {
                            tracing::info!("answer 已设置 — P2P 协商完成");
                            remote_set = true;
                            for c in pending_ice.drain(..) {
                                if let Err(e) = pc.add_ice_candidate(&c).await {
                                    tracing::warn!("add_ice_candidate: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("set_remote_description 失败: {e}");
                            exit_code = 1;
                            break 'run;
                        }
                    }
                }
                Ok(SignalEvent::Message(SignalingMessage::RTCIceCandidate {
                    candidate, sdp_mid, sdp_mline_index, ..
                })) => {
                    let c = RTCIceCandidate {
                        candidate,
                        sdp_mid,
                        sdp_mline_index,
                    };
                    if remote_set {
                        if let Err(e) = pc.add_ice_candidate(&c).await {
                            tracing::warn!("add_ice_candidate: {e}");
                        }
                    } else {
                        pending_ice.push(c);
                    }
                }
                Ok(SignalEvent::Message(_)) => {} // RoomJoined/其他透传忽略
                Ok(SignalEvent::Error(e)) => {
                    tracing::error!("信令错误: {e}");
                    exit_code = 1;
                    break 'run;
                }
                Ok(SignalEvent::Disconnected { reason }) => {
                    tracing::error!("信令断开: {reason} — 退出待重启");
                    exit_code = 1;
                    break 'run;
                }
                Ok(SignalEvent::Connected { .. }) => {}
                // SignalEvent non_exhaustive 兜底
                Ok(_) => {}
                Err(_) => {
                    tracing::error!("信令事件流关闭");
                    exit_code = 1;
                    break 'run;
                }
            },
            Some(candidate) = ice_rx.recv() => {
                if let Err(e) = signal
                    .send(SignalingMessage::RTCIceCandidate {
                        room_id: ROOM.into(),
                        target: None,
                        candidate: candidate.candidate,
                        sdp_mid: candidate.sdp_mid,
                        sdp_mline_index: candidate.sdp_mline_index,
                    })
                    .await
                {
                    tracing::warn!("ICE 候选上行失败: {e}");
                }
            }
        }
    }

    if let Err(e) = signal.close().await {
        tracing::warn!("close: {e}");
    }
    pc.close().await;
    tracing::info!("controller stopped (exit={exit_code})");
    ExitCode::from(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_init_reliability_semantics() {
        // D-H3: chassis/light 可靠有序；gimbal partial-reliable
        let chassis = channel_init("chassis");
        assert!(chassis.ordered);
        assert_eq!(chassis.max_retransmits, None);
        let light = channel_init("light");
        assert!(light.ordered);
        let gimbal = channel_init("gimbal");
        assert!(!gimbal.ordered, "云台 partial-reliable: 无序");
        assert_eq!(gimbal.max_retransmits, Some(5), "云台 5 次重传上限");
    }

    #[test]
    fn gateway_from_override_wins() {
        let gw = gateway_from(vec!["--gateway".into(), "ws://127.0.0.1:18888/ws".into()].into_iter()).unwrap();
        assert_eq!(gw, "ws://127.0.0.1:18888/ws");
    }

    #[test]
    fn gateway_from_defaults_to_local_gateway() {
        // 无参数 → 缺省本地网关（D2）
        let gw = gateway_from(vec![].into_iter()).unwrap();
        assert_eq!(gw, "ws://127.0.0.1:17980/ws");
    }

    #[test]
    fn gateway_from_rejects_unknown_arg() {
        assert!(gateway_from(vec!["--bogus".into(), "x".into()].into_iter()).is_err());
    }

    #[test]
    fn gateway_from_requires_value() {
        assert!(gateway_from(vec!["--gateway".into()].into_iter()).is_err());
    }
}
