//! 整车状态上报注册表（Task E3）— 存储每房间最新 StatusReport。
//!
//! 消费方: H 阶段 admin API（当前仅存储，无存储之外消费路径）。
//! 语义: 覆盖式（每房间仅保留最近一次上报）；房间移除（空房）时清理，
//! 避免陈旧数据悬挂。

use std::collections::HashMap;
use std::sync::Mutex;

use mediaservo_common::protocol::SignalingMessage;

/// 每房间最新状态上报（room_id → 最近一次 StatusReport 消息）。
#[derive(Default)]
pub struct StatusRegistry {
    latest: Mutex<HashMap<String, SignalingMessage>>,
}

impl StatusRegistry {
    /// 存储最新上报（覆盖同房间旧值）。
    pub fn store(&self, room_id: &str, msg: SignalingMessage) {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(room_id.to_string(), msg);
    }

    /// 最近一次上报（H 阶段 admin API 读取）。
    pub fn get(&self, room_id: &str) -> Option<SignalingMessage> {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(room_id)
            .cloned()
    }

    /// 房间移除时清理（房间空 = 无状态可报）。
    pub fn remove(&self, room_id: &str) {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(room_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mediaservo_common::protocol::SignalStatusJson;

    fn report(room: &str, ts: u64) -> SignalingMessage {
        SignalingMessage::StatusReport {
            room_id: room.into(),
            topics: vec![],
            streams: vec![],
            processes: vec![],
            signal: SignalStatusJson {
                remote_connected: true,
                remote_since_secs: None,
                remote_peer_id: "p".into(),
                children: vec![],
                agent_uptime_secs: 1,
            },
            ts,
            config_version: 0,
        }
    }

    #[test]
    fn store_get_roundtrip() {
        let reg = StatusRegistry::default();
        assert!(reg.get("room-a").is_none());
        reg.store("room-a", report("room-a", 100));
        match reg.get("room-a") {
            Some(SignalingMessage::StatusReport { ts, .. }) => assert_eq!(ts, 100),
            other => panic!("expected StatusReport, got {other:?}"),
        }
    }

    #[test]
    fn store_overwrites_latest_per_room() {
        let reg = StatusRegistry::default();
        reg.store("room-a", report("room-a", 100));
        reg.store("room-a", report("room-a", 200));
        reg.store("room-b", report("room-b", 300));
        match reg.get("room-a") {
            Some(SignalingMessage::StatusReport { ts, .. }) => {
                assert_eq!(ts, 200, "同房间应覆盖旧值")
            }
            other => panic!("expected StatusReport, got {other:?}"),
        }
        match reg.get("room-b") {
            Some(SignalingMessage::StatusReport { ts, .. }) => {
                assert_eq!(ts, 300, "房间间应隔离")
            }
            other => panic!("expected StatusReport, got {other:?}"),
        }
    }

    #[test]
    fn remove_clears_room() {
        let reg = StatusRegistry::default();
        reg.store("room-a", report("room-a", 100));
        reg.remove("room-a");
        assert!(reg.get("room-a").is_none());
    }
}
