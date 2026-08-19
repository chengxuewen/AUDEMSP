//! Task F2: host-emergency 进程 e2e — 独立 PC + DC "emergency" + 本地兜底。
//!
//! 拓扑（生产路径，D2/D3 同款；F1 controller e2e 的兄弟测试）：
//! ```text
//! host-agent (网关进程)     ──WS──▶ 外部 mediasoup server（SFU_E2E_WS_URL）
//! host-controller (进程)    ──WS──▶ host-agent    (3 DC: chassis/gimbal/light)
//! host-emergency (进程)     ──WS──▶ host-agent    (1 DC: emergency)
//! mock 舱端 (测试内, 每 offer 独立 answerer PC) ──WS──▶ server（直连, PSK）
//! ```
//!
//! 断言：
//! ① 两次顺序协商经网关完成（controller 先 → emergency 后，两协商者都活着 —
//!    压力网关 p2p_owner 单槽归属：已完成协商不再上行 Sdp/ICE → 归属安全移交，
//!    见 task-F2-report）
//! ② emergency DC "stop" → actuator 触发 + 回执 {latched:true}；重复 → latched:false
//!    （一次性闩锁语义）
//! ③ controller 被 SIGKILL（崩溃）后 emergency 仍可急停（独立 PC 证明，D-H3）
//! ④ 本地兜底：SIGUSR1 → 同一 actuator 触发（source=local，网络无关，D-H3）
//! ⑤ 强审计 JSONL：dc×2 + local×1，字段 {ts, source, seq, cmd, latched, trigger_count}
//! ⑥ SIGTERM → emergency + agent 优雅退出 0
//!
//! 前置: `SFU_E2E_WS_URL` 指向外部 mediasoup server（C21 纯外部模式）；
//! 房间名唯一（vehicle-<pid>）。agent 必须先入房（房间类型守卫，F1 教训）。

#![cfg(target_os = "linux")]

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
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

fn ws_url() -> String {
    std::env::var("SFU_E2E_WS_URL").unwrap_or_else(|_| {
        panic!(
            "SFU_E2E_WS_URL 未设置 — emergency e2e 需连外部 mediasoup server (C21);
            例: SFU_E2E_WS_URL=ws://127.0.0.1:9800/ws"
        )
    })
}

fn psk() -> String {
    std::env::var("SFU_E2E_PSK").unwrap_or_else(|_| "mediaservo-dev".to_string())
}

/// C25: 清 iceoryx2 残留（agent 监控可能触碰 FrameBus）。
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
    for _ in 0..30 {
        if read_log(log).contains(needle) {
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    panic!("未见 {needle:?}, log:\n{}", read_log(log));
}

// ── mock 舱端：直连 server（PSK），每个 offer 一个独立 answerer PC ──
// F2 两次顺序协商需要两个对端 PC（一个 PC 无法 set_remote_description 两个 offer）：
// offer#1（controller, 3 DC）→ answerer 0；offer#2（emergency, 1 DC）→ answerer 1。
// 顺序由测试时序保证（controller 通道全 Open 后才 spawn emergency）。

struct AnswererPc {
    pc: RTCPeerConnection,
    channels: Arc<Mutex<HashMap<String, RTCDataChannel>>>,
}

struct MockCockpit {
    answerers: Arc<Mutex<Vec<AnswererPc>>>,
    task: tokio::task::JoinHandle<()>,
}

impl MockCockpit {
    async fn connect(url: &str, psk: &str, room: &str) -> Result<Self, String> {
        let signal = SignalClient::new(url, psk, room, PeerRole::Remote)
            .connect()
            .await
            .map_err(|e| format!("mock 信令连接失败: {e}"))?;

        let (ice_tx, mut ice_rx) = mpsc::unbounded_channel::<RTCIceCandidate>();
        // 已发送消息文本（server 房间广播会回显给自己 → 按文本去重）
        let sent: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
        // answerers 在任务内创建，经 Arc 共享给测试侧检查
        let answerers: Arc<Mutex<Vec<AnswererPc>>> = Arc::new(Mutex::new(Vec::new()));
        let answerers_task = answerers.clone();

        let mut events = signal.events();
        let room_owned = room.to_string();
        let task = tokio::spawn(async move {
            let s = signal; // 会话移入事件任务（SignalSession 无 Clone）
            let answerers = answerers_task;
            // 在途协商归属 = 最近创建的 answerer（顺序协商下 ICE 只属于活动协商）
            let mut active: Option<usize> = None;
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
                                    // 新 offer → 新 answerer PC（每协商独立对端）
                                    let factory = RTCPeerConnectionFactory::new();
                                    let pc = factory
                                        .create_peer_connection(RTCConfiguration::default())
                                        .await
                                        .expect("mock create_peer_connection");
                                    let channels: Arc<Mutex<HashMap<String, RTCDataChannel>>> =
                                        Arc::new(Mutex::new(HashMap::new()));
                                    let ch = channels.clone();
                                    pc.on_data_channel(move |dc| {
                                        ch.lock().unwrap().insert(dc.label().to_string(), dc);
                                    });
                                    let itx = ice_tx.clone();
                                    pc.on_ice_candidate(move |c| {
                                        let _ = itx.send(c);
                                    });
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
                                    let json =
                                        serde_json::to_string(&answer).expect("序列化 answer");
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
                                    let mut v = answerers.lock().unwrap();
                                    v.push(AnswererPc { pc, channels });
                                    active = Some(v.len() - 1);
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
                                    // 顺序协商: 在途 ICE 属于最近创建的 answerer
                                    // （前次协商完成后不再有该协商的 ICE）
                                    match active {
                                        Some(idx) => {
                                            let pc = {
                                                let v = answerers.lock().unwrap();
                                                v[idx].pc.clone()
                                            };
                                            pc.add_ice_candidate(&c)
                                                .await
                                                .expect("mock add ice");
                                        }
                                        None => tracing::warn!("ICE 无活动 answerer，丢弃"),
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
                        sent.lock().unwrap().insert(
                            serde_json::to_string(&msg).expect("序列化 ICE"),
                        );
                        s.send(msg).await.expect("mock 发送 ICE");
                    }
                }
            }
        });

        Ok(Self { answerers, task })
    }

    fn channel(&self, answerer: usize, label: &str) -> Option<RTCDataChannel> {
        self.answerers
            .lock()
            .unwrap()
            .get(answerer)
            .and_then(|a| a.channels.lock().unwrap().get(label).cloned())
    }

    fn answerer_channel_count(&self, answerer: usize) -> usize {
        self.answerers
            .lock()
            .unwrap()
            .get(answerer)
            .map(|a| a.channels.lock().unwrap().len())
            .unwrap_or(0)
    }

    async fn close(self) {
        self.task.abort();
        let pcs: Vec<_> = self.answerers.lock().unwrap().drain(..).map(|a| a.pc).collect();
        for pc in pcs {
            pc.close().await;
        }
    }
}

/// 等待 answerer 的 label 通道出现并 Open（轮询 state()，≤15s）。
/// 注意: 不能依赖 spool 的 Open 事件 — 通道可能在观察者注册前已 Open
/// （libwebrtc 不回放历史状态），轮询 state() 是可靠闸门（F1 教训）。
async fn wait_channel_open(cockpit: &MockCockpit, answerer: usize, label: &str) -> RTCDataChannel {
    let dc = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Some(dc) = cockpit.channel(answerer, label) {
                return dc;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("answerer {answerer} 通道 {label} 未出现")
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
    .expect("seq {seq} ACK 超时");
    assert_ne!(deadline, serde_json::Value::Null, "seq {seq} 无 ACK");
    deadline
}

fn audit_lines(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .map(|s| {
            s.lines()
                .filter(|l| !l.trim().is_empty())
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn wait_audit_lines(path: &Path, n: usize) -> Vec<serde_json::Value> {
    for _ in 0..30 {
        let v = audit_lines(path);
        if v.len() >= n {
            return v;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    panic!(
        "审计文件未达 {n} 行, got {}:\n{}",
        audit_lines(path).len(),
        std::fs::read_to_string(path).unwrap_or_default()
    );
}

#[tokio::test]
async fn emergency_p2p_dc_with_local_fallback() {
    cleanup_iceoryx();
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
    // 房间类型守卫: agent（Host）必须先入房 → P2P 房间
    wait_for(&agent_log, "agent 已加入整车房间");

    // mock 舱端直连 server 入房（P2P 房间 remote 槽位）
    let cockpit = MockCockpit::connect(&ws_url(), &psk, &room)
        .await
        .expect("mock cockpit 连接");

    // ①a 协商 #1: host-controller（3 DC）— 先完成协商，为 #2 腾出 p2p_owner
    let ctrl_log = tempfile::NamedTempFile::new().expect("ctrl log");
    let mut controller = Command::new(env!("CARGO_BIN_EXE_host-controller"))
        .args(["--gateway", &format!("ws://127.0.0.1:{gw_port}/ws")])
        .stdout(Stdio::from(ctrl_log.reopen().expect("reopen ctrl log")))
        .stderr(Stdio::from(ctrl_log.reopen().expect("reopen ctrl log")))
        .spawn()
        .expect("spawn host-controller");
    wait_for(&ctrl_log, "controller ready");
    for label in ["chassis", "gimbal", "light"] {
        let dc = wait_channel_open(&cockpit, 0, label).await;
        assert_eq!(dc.label(), label);
    }
    assert_eq!(cockpit.answerer_channel_count(0), 3, "协商 #1 = controller 3 通道");

    // ①b 协商 #2: host-emergency（1 DC）— 两协商者都活着（p2p_owner 压力点）
    let audit_file = tempfile::NamedTempFile::new().expect("audit file");
    let em_log = tempfile::NamedTempFile::new().expect("emergency log");
    let mut emergency = Command::new(env!("CARGO_BIN_EXE_host-emergency"))
        .args([
            "--gateway",
            &format!("ws://127.0.0.1:{gw_port}/ws"),
            "--audit",
            audit_file.path().to_str().expect("audit path"),
        ])
        .stdout(Stdio::from(em_log.reopen().expect("reopen em log")))
        .stderr(Stdio::from(em_log.reopen().expect("reopen em log")))
        .spawn()
        .expect("spawn host-emergency");
    wait_for(&em_log, "emergency ready");
    let em_dc = wait_channel_open(&cockpit, 1, "emergency").await;
    assert_eq!(em_dc.label(), "emergency");
    assert_eq!(cockpit.answerer_channel_count(1), 1, "协商 #2 = emergency 1 通道");

    // ② 急停 DC: 首次触发 → 闩锁 armed（latched=true）；重复 → latched=false
    let ack1 = send_and_await_ack(&em_dc, 1, "stop", serde_json::json!({})).await;
    assert_eq!(ack1["ack"], 1);
    assert_eq!(ack1["result"]["ok"], true);
    assert_eq!(ack1["result"]["source"], "dc");
    assert_eq!(ack1["result"]["latched"], true, "首次触发必须 armed 闩锁");
    assert_eq!(ack1["result"]["trigger_count"], 1);
    assert_eq!(ack1["result"]["audit"], "ok");
    eprintln!("[emergency_e2e] ① 首次急停 OK: {ack1}");

    let ack2 = send_and_await_ack(&em_dc, 2, "stop", serde_json::json!({})).await;
    assert_eq!(ack2["result"]["ok"], true);
    assert_eq!(ack2["result"]["latched"], false, "重复触发不得重复 armed");
    assert_eq!(ack2["result"]["trigger_count"], 2);
    eprintln!("[emergency_e2e] ② 重复急停（闩锁保持）: {ack2}");

    // ③ controller 崩溃（SIGKILL）不影响 emergency（独立 PC，D-H3）
    unsafe { libc::kill(controller.id() as i32, libc::SIGKILL) };
    controller.wait().expect("wait controller");
    eprintln!("[emergency_e2e] controller 已 SIGKILL，emergency 继续服务");

    let ack3 = send_and_await_ack(&em_dc, 3, "stop", serde_json::json!({})).await;
    assert_eq!(ack3["result"]["ok"], true, "controller 崩溃后 emergency 必须仍可急停");
    assert_eq!(ack3["result"]["trigger_count"], 3);
    eprintln!("[emergency_e2e] ③ controller 崩溃后急停 OK: {ack3}");

    // ④ 本地兜底: SIGUSR1（网络无关路径）→ 同一 actuator
    unsafe { libc::kill(emergency.id() as i32, libc::SIGUSR1) };

    // ⑤ 强审计: 4 行 = dc(seq1,latched) / dc(seq2,已闩) / dc(seq3,崩后) / local(seq=null)
    let lines = wait_audit_lines(audit_file.path(), 4);
    assert_eq!(lines[0]["source"], "dc");
    assert_eq!(lines[0]["seq"], 1);
    assert_eq!(lines[0]["latched"], true);
    assert_eq!(lines[0]["trigger_count"], 1);
    assert_eq!(lines[0]["cmd"], "stop");
    assert!(lines[0]["ts"].as_str().is_some(), "审计必须带时间戳");
    assert_eq!(lines[1]["source"], "dc");
    assert_eq!(lines[1]["seq"], 2);
    assert_eq!(lines[1]["latched"], false);
    assert_eq!(lines[1]["trigger_count"], 2);
    assert_eq!(lines[2]["source"], "dc");
    assert_eq!(lines[2]["seq"], 3);
    assert_eq!(lines[2]["latched"], false);
    assert_eq!(lines[2]["trigger_count"], 3);
    assert_eq!(lines[3]["source"], "local");
    assert!(lines[3]["seq"].is_null(), "本地触发无信封 seq");
    assert_eq!(lines[3]["latched"], false, "闩锁已被前 3 次 DC 触发 armed — 本地触发如实记录未重复 armed");
    assert_eq!(lines[3]["trigger_count"], 4);
    eprintln!("[emergency_e2e] ④ 审计 4 行: {lines:?}");

    // ⑥ SIGTERM → emergency + agent 优雅退出 0
    unsafe { libc::kill(emergency.id() as i32, libc::SIGTERM) };
    let et = emergency.wait().expect("wait emergency");
    assert_eq!(et.code(), Some(0), "emergency 应优雅退出 0, got {et:?}");
    unsafe { libc::kill(agent.id() as i32, libc::SIGTERM) };
    let at = agent.wait().expect("wait agent");
    assert_eq!(at.code(), Some(0), "agent 应优雅退出 0, got {at:?}");

    cockpit.close().await;
}
