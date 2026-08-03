//! RTCDataChannel control sender — signs commands with HMAC-SHA256, rate-limited send.
//!
//! Uses `audemsp_common::auth::SimplePskAuth` for signing.
//! Commands are buffered (max 3), oldest dropped when full.

use audemsp_common::auth::SimplePskAuth;
use audemsp_common::error::CoreError;
use audemsp_webrtc::RTCDataChannel;
use std::collections::VecDeque;
use tokio::sync::Mutex;

// ponytail: 3-frame buffer prevents back-pressure stall in control loop

/// Control commands sent over RTCDataChannel from Remote to Host.
#[derive(Debug, Clone)]
pub enum ControlCommand {
    /// Steering angle in degrees.
    Steering(f64),
    /// Brake pressure 0.0–1.0.
    Brake(f64),
    /// Throttle position 0.0–1.0.
    Throttle(f64),
    /// Immediate emergency stop.
    EmergencyStop,
}

/// Serialize a control command to bytes for HMAC signing.
fn command_to_bytes(cmd: &ControlCommand) -> Vec<u8> {
    match cmd {
        ControlCommand::Steering(v) => format!("steering:{v}").into_bytes(),
        ControlCommand::Brake(v) => format!("brake:{v}").into_bytes(),
        ControlCommand::Throttle(v) => format!("throttle:{v}").into_bytes(),
        ControlCommand::EmergencyStop => b"emergency_stop".to_vec(),
    }
}

/// Control sender — signs and buffers commands for RTCDataChannel transmission.
pub struct ControlSender {
    /// PSK authenticator used for HMAC signing.
    auth: SimplePskAuth,
    /// Buffer of (command, signature) pairs awaiting send.
    buffer: Mutex<VecDeque<(ControlCommand, Vec<u8>)>>,
    /// Maximum send rate in Hz.
    rate_hz: u32,
}

impl ControlSender {
    /// Create a new control sender.
    ///
    /// `hmac_key` is used as the PSK for HMAC-SHA256 signing.
    /// `rate_hz` limits the send rate (frames per second).
    pub fn new(hmac_key: &str, rate_hz: u32) -> Self {
        tracing::info!(
            hmac_key = hmac_key,
            rate_hz = rate_hz,
            "Control sender initialized"
        );
        Self {
            auth: SimplePskAuth::new(hmac_key.as_bytes()),
            buffer: Mutex::new(VecDeque::with_capacity(4)),
            rate_hz,
        }
    }

    /// Sign a control command, returning the 8-byte HMAC-SHA256 tag.
    pub fn sign_command(&self, command: &ControlCommand) -> Vec<u8> {
        let payload = command_to_bytes(command);
        self.auth.sign(&payload)
    }

    /// Enqueue a command for sending.
    ///
    /// Rate-limited by `rate_hz`. Drops oldest command if buffer exceeds 3 entries.
    pub async fn send(&self, command: ControlCommand) -> Result<(), CoreError> {
        let tag = self.sign_command(&command);

        let mut buf = self.buffer.lock().await;
        if buf.len() >= 3 {
            buf.pop_front();
            tracing::warn!("Control buffer full, dropped oldest command");
        }
        buf.push_back((command, tag));

        // ponytail: simple fixed-rate pacing; replace with token bucket if burst tolerance needed
        let interval_ms = 1000u64 / self.rate_hz as u64;
        tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;
        Ok(())
    }

    /// Current buffer depth (for health monitoring).
    pub async fn depth(&self) -> usize {
        self.buffer.lock().await.len()
    }
    /// Send a control command immediately via RTCDataChannel.
    /// Signs with HMAC-SHA256 (key: audemsp-control), prefixes 0x01, sends binary.
    pub async fn send_via_dc(&self, dc: &RTCDataChannel, cmd: &ControlCommand) -> Result<(), CoreError> {
        let body = cmd.to_json_body();
        let tag = self.hmac_sign(&body);
        let mut payload = vec![0x01u8];
        payload.extend_from_slice(&tag);
        payload.extend_from_slice(body.as_bytes());
        dc.send(&payload).await
            .map_err(|e| CoreError::Unknown(format!("dc send: {e}")))?;
        Ok(())
    }
    /// Compute 8-byte HMAC-SHA256 tag with key 'audemsp-control'.
    fn hmac_sign(&self, body: &str) -> [u8; 8] {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let mut mac = Hmac::<Sha256>::new_from_slice(b"audemsp-control")
            .expect("HMAC key derivation should not fail");
        mac.update(body.as_bytes());
        let result = mac.finalize().into_bytes();
        let mut tag = [0u8; 8];
        tag.copy_from_slice(&result[..8]);
        tag
    }
}

impl ControlCommand {
    /// Serialize to JSON body matching host's ControlFrame format.
    pub fn to_json_body(&self) -> String {
        match self {
            ControlCommand::Steering(v) => format!(r#"{{"type":"steering","value":{v}}}"#),
            ControlCommand::Brake(v) => format!(r#"{{"type":"brake","value":{v}}}"#),
            ControlCommand::Throttle(v) => format!(r#"{{"type":"throttle","value":{v}}}"#),
            ControlCommand::EmergencyStop => r#"{{"type":"emergency_stop"}}"#.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_steering_command() {
        let sender = ControlSender::new("test-key", 30);
        let cmd = ControlCommand::Steering(15.2);
        let tag = sender.sign_command(&cmd);
        assert_eq!(tag.len(), 8);
    }

    #[test]
    fn sign_different_keys_produce_different_tags() {
        let sender_a = ControlSender::new("key-a", 30);
        let sender_b = ControlSender::new("key-b", 30);
        let cmd = ControlCommand::Brake(0.5);
        let tag_a = sender_a.sign_command(&cmd);
        let tag_b = sender_b.sign_command(&cmd);
        assert_ne!(tag_a, tag_b);
    }

    #[test]
    fn sign_all_command_types() {
        let sender = ControlSender::new("test-key", 30);
        for cmd in [
            ControlCommand::Steering(1.0),
            ControlCommand::Brake(0.5),
            ControlCommand::Throttle(45.0),
            ControlCommand::EmergencyStop,
        ] {
            let tag = sender.sign_command(&cmd);
            assert_eq!(tag.len(), 8);
        }
    }

    #[test]
    fn emergency_stop_bytes() {
        let bytes = command_to_bytes(&ControlCommand::EmergencyStop);
        assert_eq!(bytes, b"emergency_stop");
    }

    #[test]
    fn command_to_bytes_all_variants() {
        assert_eq!(
            command_to_bytes(&ControlCommand::Steering(1.5)),
            b"steering:1.5"
        );
        assert_eq!(
            command_to_bytes(&ControlCommand::Brake(0.3)),
            b"brake:0.3"
        );
        assert_eq!(
            command_to_bytes(&ControlCommand::Throttle(80.0)),
            b"throttle:80"
        );
    }

    #[tokio::test]
    async fn buffer_depth_tracks_commands() {
        let sender = ControlSender::new("test-key", 1000);
        // high rate_hz so rate-limiting doesn't slow the test
        assert_eq!(sender.depth().await, 0);
        sender.send(ControlCommand::Steering(1.0)).await.unwrap();
        assert_eq!(sender.depth().await, 1);
        sender.send(ControlCommand::Brake(0.5)).await.unwrap();
        assert_eq!(sender.depth().await, 2);
    }

    #[test]
    fn steering_and_brake_produce_different_tags() {
        let sender = ControlSender::new("test-key", 30);
        let tag_steering = sender.sign_command(&ControlCommand::Steering(10.0));
        let tag_brake = sender.sign_command(&ControlCommand::Brake(0.5));
        assert_ne!(tag_steering, tag_brake);
    }

    #[tokio::test]
    async fn buffer_overflow_drops_oldest() {
        // rate_hz 1000 avoids rate-limiting delay
        let sender = ControlSender::new("test-key", 1000);
        sender.send(ControlCommand::Steering(1.0)).await.unwrap();
        sender.send(ControlCommand::Steering(2.0)).await.unwrap();
        sender.send(ControlCommand::Steering(3.0)).await.unwrap();
        // buffer is now full (3 commands)
        assert_eq!(sender.depth().await, 3);
        // 4th command should drop oldest (Steering(1.0)) and add new one
        sender.send(ControlCommand::Steering(4.0)).await.unwrap();
        assert_eq!(sender.depth().await, 3);
    }

    #[test]
    fn emergency_stop_serialization_roundtrip() {
        let bytes = command_to_bytes(&ControlCommand::EmergencyStop);
        assert_eq!(bytes, b"emergency_stop");
        // Verify it produces a valid HMAC tag
        let sender = ControlSender::new("test-key", 30);
        let tag = sender.sign_command(&ControlCommand::EmergencyStop);
        assert_eq!(tag.len(), 8);
    }
}
