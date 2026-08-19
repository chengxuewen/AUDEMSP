//! Task F1: host-controller 进程 e2e — 纯 DC P2P 控制通道（经本地网关）。
//!
//! 拓扑（生产路径，D2/D3 同款）：
//! ```text
//! host-agent (网关进程) ──WS──▶ 外部 mediasoup server（SFU_E2E_WS_URL）
//! host-controller (进程) ──WS──▶ host-agent
//! mock 舱端 (测试内 PC)  ──WS──▶ server（直连，PSK）
//! ```
//!
//! 断言：
//! ① P2P 协商经网关完成（controller=offerer，mock=answerer，legacy 惯例）
//! ② 3 条 DC（chassis/gimbal/light）到达 mock，label 正确
//! ③ 每 label 发送控制信封 → 同 label 收到 ACK（label 路由 + 往返）
//! ④ chassis 可靠有序：5 连发 ACK 顺序一致
//! ⑤ SIGTERM → controller + agent 优雅退出 0
//!
//! 前置: `SFU_E2E_WS_URL` 指向外部 mediasoup server（C21 纯外部模式）；
//! 房间名唯一（vehicle-<pid>，避免与 e2e_sfu/streamer_e2e 的 "vehicle" 撞车）。
//! 注意: 房间类型由首 join 者角色决定（Host→P2P 放行 Sdp/ICE；
//! Remote→DeviceStream 丢弃非 Frame）— agent 必须先入房（D1 惯例）。

#![cfg(target_os = "linux")]

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mediaservo_common::protocol::{PeerRole, SignalingMessage};
use mediaservo_link::{SignalClient, SignalEvent};
use mediaservo_webrtc::data_channel::{RTCDataChannel, RTCDataChannelEvent};
use mediaservo_webrtc::peer_connection::RTCIceCandidate;
use mediaservo_webrtc::sdp::{RTCSdpType, RTCSessionDescription};
use mediaservo_webrtc::traits::PeerConnectionApi;
use mediaservo_webrtc::{RTCPeerConnection, RTCPeerConnectionFactory, RTCConfiguration};
use tokio::sync::mpsc;

const CHANNELS: [&str; 3] = ["chassis", "gimbal", "light"];

fn ws_url() -> String {
    std::env::var("SFU_E2E_WS_URL").unwrap_or_else(|_| {
        panic!(
            "SFU_E2E_WS_URL 未设置 — controller e2e 需连外部 mediasoup server (C21);
            例: SFU_E2E_WS_URL=ws://127.0.0.1:9800/ws"
        )
    })
}

fn psk() -> String {
    std::env::var("SFU_E2E_PSK").unwrap_or_else(|_| "mediaservo-dev".to_string())
}

/// C25: 清 iceoryx2 残留（controller 不直接用 FrameBus，但 agent 监控可能触碰）。
fn cleanup_iceoryx() {
    let _ = std::fs::remove_dir_all("/tmp/iceoryx2");
    if let Ok(entries) = std::fs::read_dir("/dev/shm") {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with("iox2_") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

fn free_local_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind probe");
    l.local_addr().expect("probe addr").port()
}

fn read_log(file: &tempfile::NamedTempFile) -> String {
    let mut out = String::new();
    file.reopen()
        .expect("reopen log")
        .read_to_string(&mut out)
        .expect("read log");
    out
}

fn wait_for(log: &tempfile::NamedTempFile, needle: &str) {
    for _ in 0..20 {
        if read_log(log).contains(needle) {
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    panic!("未见 {needle:?}, log:\n{}", read_log(log));
}

// ── mock 舱端：直连 server（PSK），answerer 角色，收集 3 条 incoming DC ──

struct MockCockpit {
    pc: RTCPeerConnection,
    channels: Arc<Mutex<HashMap<String, RTCDataChannel>>>,
    task: tokio::task::JoinHandle<()>,
}

impl MockCockpit {
    async fn connect(url: &str, psk: &str, room: &str) -> Result<Self, String> {
        // 直连 server（生产舱端路径；controller 才走网关）
        let signal = SignalClient::new(url, psk, room, PeerRole::Remote)
            .connect()
            .await
            .map_err(|e| format!("mock 信令连接失败: {e}"))?;

        let factory = RTCPeerConnectionFactory::new();
        let pc = factory
            .create_peer_connection(RTCConfiguration::default())
            .await
            .map_err(|e| format!("mock create_peer_connection: {e}"))?;
        let pc_task = pc.clone(); // 事件任务持自己的 PC 副本

        let channels: Arc<Mutex<HashMap<String, RTCDataChannel>>> = Arc::new(Mutex::new(HashMap::new()));
        let ch = channels.clone();
        pc.on_data_channel(move |dc| {
            ch.lock().unwrap().insert(dc.label().to_string(), dc);
        });

        // 已发送消息文本（server 房间广播会回显给自己 → 按文本去重）
        let sent: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

        let (ice_tx, mut ice_rx) = mpsc::unbounded_channel::<RTCIceCandidate>();
        pc.on_ice_candidate(move |c| {
            let _ = ice_tx.send(c);
        });

        let mut events = signal.events();
        let room_owned = room.to_string();
        let task = tokio::spawn(async move {
            let s = signal; // 会话移入事件任务（SignalSession 无 Clone）
            let pc = pc_task;
            let mut remote_set = false;
            let mut pending: Vec<RTCIceCandidate> = Vec::new();
            loop {
                tokio::select! {
                    ev = events.recv() => match ev {
                        Ok(SignalEvent::Message(m)) => {
                            let text = serde_json::to_string(&m).unwrap_or_default();
                            if sent.lock().unwrap().contains(&text) {
                                continue; // 自己消息的回显
                            }
                            match m {
                                SignalingMessage::Sdp { sdp, .. } => {
                                    let desc: RTCSessionDescription =
                                        serde_json::from_str(&sdp).expect("mock 解析 Sdp JSON");
                                    if desc.sdp_type != RTCSdpType::Offer {
                                        continue;
                                    }
                                    pc.set_remote_description(&desc)
                                        .await
                                        .expect("mock set_remote_description");
                                    let answer = pc
                                        .create_answer(&Default::default())
                                        .await
                                        .expect("mock create_answer");
                                    pc.set_local_description(&answer)
                                        .await
                                        .expect("mock set_local_description");
                                    let json = serde_json::to_string(&answer).expect("序列化 answer");
                                    sent.lock().unwrap().insert(
                                        serde_json::to_string(&SignalingMessage::Sdp {
                                            room_id: room_owned.clone(),
                                            target: None,
                                            sdp: json.clone(),
                                        })
                                        .expect("序列化 Sdp 消息"),
                                    );
                                    s.send(SignalingMessage::Sdp {
                                        room_id: room_owned.clone(),
                                        target: None,
                                        sdp: json,
                                    })
                                    .await
                                    .expect("mock 发送 answer");
                                    remote_set = true;
                                    for c in pending.drain(..) {
                                        pc.add_ice_candidate(&c).await.expect("mock add ice");
                                    }
                                }
                                SignalingMessage::RTCIceCandidate {
                                    candidate,
                                    sdp_mid,
                                    sdp_mline_index,
                                    ..
                                } => {
                                    let c = RTCIceCandidate {
                                        candidate,
                                        sdp_mid,
                                        sdp_mline_index,
                                    };
                                    if remote_set {
                                        pc.add_ice_candidate(&c).await.expect("mock add ice");
                                    } else {
                                        pending.push(c);
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(SignalEvent::Disconnected { .. }) | Err(_) => break,
                        _ => {}
                    },
                    Some(c) = ice_rx.recv() => {
                        let msg = SignalingMessage::RTCIceCandidate {
                            room_id: room_owned.clone(),
                            target: None,
                            candidate: c.candidate,
                            sdp_mid: c.sdp_mid,
                            sdp_mline_index: c.sdp_mline_index,
                        };
                        sent.lock().unwrap().insert(serde_json::to_string(&msg).expect("序列化 ICE"));
                        s.send(msg).await.expect("mock 发送 ICE");
                    }
                }
            }
        });

        Ok(Self { pc, channels, task })
    }

    fn channel(&self, label: &str) -> Option<RTCDataChannel> {
        self.channels.lock().unwrap().get(label).cloned()
    }

    async fn close(self) {
        self.task.abort();
        self.pc.close().await;
    }
}

/// 等待 label 的通道出现并 Open（轮询 state()，≤15s）。
/// 注意: 不能依赖 spool 的 Open 事件 — 通道可能在观察者注册前已 Open
/// （libwebrtc 不回放历史状态），轮询 state() 是可靠闸门。
async fn wait_channel_open(cockpit: &MockCockpit, label: &str) -> RTCDataChannel {
    let dc = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Some(dc) = cockpit.channel(label) {
                return dc;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("通道 {label} 未出现")
    .clone();
    tokio::time::timeout(Duration::from_secs(15), async {
        while dc.state() != mediaservo_webrtc::data_channel::RTCDataChannelState::Open {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("通道 {label} 未 Open");
    dc
}

/// 在 label 通道发送信封并等待同通道 ACK（ack 序号配对）。
async fn send_and_await_ack(
    dc: &RTCDataChannel,
    label: &str,
    seq: u64,
    cmd: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    let mut rx = dc.spool().await;
    let env = serde_json::json!({ "seq": seq, "cmd": cmd, "payload": payload });
    dc.send_text(&env.to_string()).await.expect("mock 发送信封");
    let deadline = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(ev) = rx.recv().await {
            if let RTCDataChannelEvent::Message(m) = ev {
                let v: serde_json::Value =
                    serde_json::from_slice(&m.data).unwrap_or(serde_json::Value::Null);
                if v["ack"] == serde_json::json!(seq) {
                    return v;
                }
            }
        }
        serde_json::Value::Null
    })
    .await
    .expect("label {label} seq {seq} ACK 超时");
    assert_ne!(deadline, serde_json::Value::Null, "label {label} seq {seq} 无 ACK");
    deadline
}

#[tokio::test]
async fn controller_p2p_dc_control_through_gateway() {
    cleanup_iceoryx();
    let _url = ws_url();
    let pid = std::process::id();
    let room = format!("vehicle-{pid}");
    let psk = psk();

    // host-agent（本地网关）: 随机端口 + 远端真 server + 唯一房间
    let gw_port = free_local_port();
    let agent_log = tempfile::NamedTempFile::new().expect("agent log");
    let mut agent = Command::new(env!("CARGO_BIN_EXE_host-agent"))
        .args([
            "--port",
            &gw_port.to_string(),
            "--remote",
            &ws_url(),
            "--psk",
            &psk,
            "--room",
            &room,
        ])
        .stdout(Stdio::from(agent_log.reopen().expect("reopen agent log")))
        .stderr(Stdio::from(agent_log.reopen().expect("reopen agent log")))
        .spawn()
        .expect("spawn host-agent");
    // ① 房间类型守卫: agent（Host）必须先入房 → P2P 房间（否则 Remote 首入 → DeviceStream 丢弃 Sdp）
    wait_for(&agent_log, "agent 已加入整车房间");

    // mock 舱端直连 server 入房（P2P 房间 remote 槽位）
    let cockpit = MockCockpit::connect(&ws_url(), &psk, &room)
        .await
        .expect("mock cockpit 连接");

    // host-controller 经本地网关
    let ctrl_log = tempfile::NamedTempFile::new().expect("ctrl log");
    let mut controller = Command::new(env!("CARGO_BIN_EXE_host-controller"))
        .args(["--gateway", &format!("ws://127.0.0.1:{gw_port}/ws")])
        .stdout(Stdio::from(ctrl_log.reopen().expect("reopen ctrl log")))
        .stderr(Stdio::from(ctrl_log.reopen().expect("reopen ctrl log")))
        .spawn()
        .expect("spawn host-controller");
    wait_for(&ctrl_log, "controller ready");

    // ② 3 条 DC 到达 mock + Open（offerer=controller 创建的通道，answerer 收）
    let mut dcs = Vec::new();
    for label in CHANNELS {
        let dc = wait_channel_open(&cockpit, label).await;
        assert_eq!(dc.label(), label, "label 应正确");
        dcs.push(dc);
    }
    eprintln!("[controller_e2e] 3 通道全部 Open: {CHANNELS:?}");

    // ③ label 路由 + 往返: 每通道发 1 条 → 同通道 ACK（回执带 channel 回显）
    let cases: [(&str, &str, serde_json::Value); 3] = [
        ("chassis", "steer", serde_json::json!({ "value": 0.35 })),
        ("gimbal", "pan", serde_json::json!({ "deg": 15 })),
        ("light", "on", serde_json::json!({})),
    ];
    for (i, (label, cmd, payload)) in cases.iter().enumerate() {
        let dc = &dcs[i];
        let ack = send_and_await_ack(dc, label, (i + 1) as u64, cmd, payload.clone()).await;
        assert_eq!(ack["ack"], (i + 1) as u64, "{label} ACK 序号");
        assert_eq!(ack["result"]["ok"], true, "{label} 结果 ok");
        assert_eq!(ack["result"]["channel"], serde_json::json!(label), "{label} 回执通道回显（路由证据）");
        eprintln!("[controller_e2e] {label} 往返 OK: {ack}");
    }

    // ④ chassis 可靠有序: 5 连发 → ACK 顺序一致
    for seq in 10..15u64 {
        let dc = &dcs[0];
        let ack = send_and_await_ack(
            dc,
            "chassis",
            seq,
            "steer",
            serde_json::json!({ "value": seq as f64 / 100.0 }),
        )
        .await;
        assert_eq!(ack["ack"], seq, "顺序 {seq} 的 ACK");
        eprintln!("[controller_e2e] chassis 顺序 ACK #{seq}");
    }

    // ⑤ SIGTERM → controller + agent 优雅退出 0
    unsafe { libc::kill(controller.id() as i32, libc::SIGTERM) };
    let ct = controller.wait().expect("wait controller");
    assert_eq!(ct.code(), Some(0), "controller 应优雅退出 0, got {ct:?}");
    unsafe { libc::kill(agent.id() as i32, libc::SIGTERM) };
    let at = agent.wait().expect("wait agent");
    assert_eq!(at.code(), Some(0), "agent 应优雅退出 0, got {at:?}");

    cockpit.close().await;
}
