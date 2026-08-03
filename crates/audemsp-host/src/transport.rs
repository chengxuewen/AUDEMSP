//! WebRTC transport — stub with tracing instrumentation.
//!
//! Real WebRTC connections need a running signaling server (Phase I integration).
//! The Transport struct holds a placeholder for the eventual RTCPeerConnection.

#[cfg(feature = "webrtc")]
mod imp {
    use base64::{Engine as _, engine::general_purpose};
    use audemsp_common::error::CoreError;

    pub struct Transport {
        tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    }

    impl Transport {
        pub fn new() -> Self {
            tracing::info!("WebRTC transport initialized (libwebrtc)");
            Transport { tx: None }
        }

        pub fn new_with_sender(tx: tokio::sync::mpsc::UnboundedSender<String>) -> Self {
            tracing::info!("WebRTC transport initialized (libwebrtc, with WS sender)");
            Transport { tx: Some(tx) }
        }

        /// Send an encoded H.264 frame to the peer.
        /// If the channel is configured, base64-encodes the frame and pushes
        /// a SignalingMessage::Frame JSON through the mpsc sender.
        pub async fn send_frame(&self, data: &[u8]) -> Result<(), CoreError> {
            tracing::debug!("WebRTC send_frame: {} bytes", data.len());
            if let Some(tx) = &self.tx {
                let b64 = general_purpose::STANDARD.encode(data);
                let frame_json = serde_json::json!({
                    "type": "frame",
                    "room_id": "default",
                    "codec": "h264",
                    "sequence": 0,
                    "is_keyframe": false,
                    "data_base64": b64,
                }).to_string();
                let _ = tx.send(frame_json);
            }
            Ok(())
        }
    }
}

#[cfg(not(feature = "webrtc"))]
mod imp {
    use base64::{Engine as _, engine::general_purpose};
    use audemsp_common::error::CoreError;

    pub struct Transport {
        tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    }

    impl Transport {
        pub fn new() -> Self {
            tracing::warn!("WebRTC stub (not compiled)");
            Transport { tx: None }
        }

        pub fn new_with_sender(tx: tokio::sync::mpsc::UnboundedSender<String>) -> Self {
            tracing::warn!("WebRTC stub (not compiled, with WS sender)");
            Transport { tx: Some(tx) }
        }

        pub async fn send_frame(&self, data: &[u8]) -> Result<(), CoreError> {
            tracing::debug!("WebRTC send_frame (stub): {} bytes", data.len());
            if let Some(tx) = &self.tx {
                let b64 = general_purpose::STANDARD.encode(data);
                let frame_json = serde_json::json!({
                    "type": "frame",
                    "room_id": "default",
                    "codec": "h264",
                    "sequence": 0,
                    "is_keyframe": false,
                    "data_base64": b64,
                }).to_string();
                let _ = tx.send(frame_json);
            }
            Ok(())
        }
    }
}

pub use imp::Transport;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_new_creates_stub() {
        let _t = Transport::new();
    }

    #[tokio::test]
    async fn transport_send_frame_returns_ok() {
        let t = Transport::new();
        let result = t.send_frame(b"test-frame-data").await;
        assert!(result.is_ok());
    }
}
