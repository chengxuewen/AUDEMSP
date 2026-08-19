//! 控制平面（Task F1）— 控制信封 + 执行器接口。
//!
//! 语义（D-H3 控制通道落地）：
//! - 通道边界 = DC label（chassis/gimbal/light），信封内不重复携带通道名；
//! - 请求 `{seq, cmd, payload}` → 回执 `{ack, result}`（同通道发回）；
//!   `seq` 由发送方单调递增，回执 `ack` 原样回传供对端配对；
//! - 执行器接口 trait 化：F1 为 `StubActuator`（日志 + 回执），CAN/GPIO
//!   实现在 Phase I 后接入（D-H3 本地兜底归 F2）。
//! - 通道可靠性（host-controller 创建）：chassis/light reliable-ordered，
//!   gimbal partial-reliable（D-H3：急停 reliable / 云台 partial-reliable）。

use serde::{Deserialize, Serialize};

/// 控制请求信封（JSON，UTF-8 文本；`payload` 为执行器语义自由体）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlEnvelope {
    /// 发送方单调递增序号（回执配对用）。
    pub seq: u64,
    /// 执行器命令（如 "steer"/"pan"/"on"；语义由执行器实现定义）。
    pub cmd: String,
    /// 命令参数（缺省 = 空对象）。
    #[serde(default = "default_payload")]
    pub payload: serde_json::Value,
}

/// 控制回执（与请求同通道发回）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlAck {
    /// 回执对应的请求 seq。
    pub ack: u64,
    /// 执行器结果；失败时 `{"error": "<原因>"}`。
    pub result: serde_json::Value,
}

impl ControlAck {
    pub fn ok(seq: u64, result: serde_json::Value) -> Self {
        Self { ack: seq, result }
    }

    pub fn err(seq: u64, message: impl Into<String>) -> Self {
        Self {
            ack: seq,
            result: serde_json::json!({ "error": message.into() }),
        }
    }
}

/// 从 DC 字节解析请求信封。
pub fn parse_envelope(data: &[u8]) -> Result<ControlEnvelope, serde_json::Error> {
    serde_json::from_slice(data)
}

/// 缺省 payload = 空对象（`Value::default()` 是 Null，语义不符）。
fn default_payload() -> serde_json::Value {
    serde_json::json!({})
}

/// 执行器接口 — 按通道路由命令；返回回执 result（Err → `ControlAck::err`）。
/// 实现方必须打日志（C15）；错误信息返回给对端（ACK 语义，非静默）。
pub trait Actuator: Send + Sync {
    fn on_command(
        &self,
        channel: &str,
        env: &ControlEnvelope,
    ) -> Result<serde_json::Value, String>;
}

/// Stub 执行器（F1 阶段）：日志 + 回执 `{"ok": true, "channel": .., "seq": ..}`。
/// CAN/GPIO 真实实现在 Phase I 后替换（接口不变）。
pub struct StubActuator;

impl Actuator for StubActuator {
    fn on_command(
        &self,
        channel: &str,
        env: &ControlEnvelope,
    ) -> Result<serde_json::Value, String> {
        tracing::info!(
            channel,
            cmd = %env.cmd,
            seq = env.seq,
            payload = %env.payload,
            "actuator: stub 执行器收到命令"
        );
        Ok(serde_json::json!({ "ok": true, "channel": channel, "seq": env.seq }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_roundtrip() {
        let env = ControlEnvelope {
            seq: 42,
            cmd: "steer".into(),
            payload: serde_json::json!({ "value": 0.35 }),
        };
        let json = serde_json::to_string(&env).unwrap();
        let back: ControlEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back.seq, 42);
        assert_eq!(back.cmd, "steer");
        assert_eq!(back.payload["value"], 0.35);
    }

    #[test]
    fn envelope_parse_from_bytes() {
        let data = br#"{"seq":7,"cmd":"pan","payload":{"deg":90}}"#;
        let env = parse_envelope(data).unwrap();
        assert_eq!(env.seq, 7);
        assert_eq!(env.cmd, "pan");
        assert_eq!(env.payload["deg"], 90);
    }

    #[test]
    fn envelope_payload_defaults_empty() {
        let env: ControlEnvelope = serde_json::from_str(r#"{"seq":1,"cmd":"on"}"#).unwrap();
        assert_eq!(env.payload, serde_json::json!({}));
    }

    #[test]
    fn envelope_rejects_missing_seq() {
        let err = serde_json::from_str::<ControlEnvelope>(r#"{"cmd":"on"}"#);
        assert!(err.is_err(), "缺 seq 必须解析失败");
    }

    #[test]
    fn ack_ok_shape() {
        let ack = ControlAck::ok(12, serde_json::json!({ "ok": true }));
        let json = serde_json::to_string(&ack).unwrap();
        assert!(json.contains(r#""ack":12"#), "ack 字段: {json}");
        let back: ControlAck = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ack, 12);
        assert_eq!(back.result["ok"], true);
    }

    #[test]
    fn ack_err_shape() {
        let ack = ControlAck::err(3, "unknown cmd");
        let back: ControlAck = serde_json::from_str(&serde_json::to_string(&ack).unwrap()).unwrap();
        assert_eq!(back.ack, 3);
        assert_eq!(back.result["error"], "unknown cmd");
    }

    #[test]
    fn stub_actuator_replies_with_channel_and_seq() {
        let actuator = StubActuator;
        let env = ControlEnvelope {
            seq: 5,
            cmd: "steer".into(),
            payload: serde_json::json!({ "value": -0.2 }),
        };
        let result = actuator.on_command("chassis", &env).unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(result["channel"], "chassis", "回执应回显通道（label 路由证据）");
        assert_eq!(result["seq"], 5);
    }

    #[test]
    fn stub_actuator_distinguishes_channels() {
        let actuator = StubActuator;
        let env = ControlEnvelope { seq: 1, cmd: "pan".into(), payload: serde_json::json!({}) };
        let chassis = actuator.on_command("chassis", &env).unwrap();
        let gimbal = actuator.on_command("gimbal", &env).unwrap();
        assert_eq!(chassis["channel"], "chassis");
        assert_eq!(gimbal["channel"], "gimbal");
    }
}
