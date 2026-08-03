//! WebRTC transport — RTCPeerConnection factory for remote pull side.
//!
//! Feature-gated behind `cfg(feature = "webrtc")`. In default builds,
//! a stub is provided that logs the server address.

use audemsp_common::error::CoreError;

/// WebRTC transport placeholder.
///
/// In production (with `webrtc` feature), manages a RTCPeerConnection
/// that pulls remote tracks from the server relay.
#[cfg(feature = "webrtc")]
pub struct Transport {
    _server_addr: String,
    rx: Option<tokio::sync::mpsc::Receiver<Vec<u8>>>, 
}

#[cfg(feature = "webrtc")]
impl Transport {
    pub fn new(server_addr: &str) -> Self {
        tracing::info!("Remote transport targeting {}", server_addr);
        Self {
            _server_addr: server_addr.to_string(),
            rx: None,
        }
    }

    pub fn new_with_receiver(
        server_addr: &str,
        rx: tokio::sync::mpsc::Receiver<Vec<u8>>, 
    ) -> Self {
        tracing::info!("Remote transport targeting {} (with frame receiver)", server_addr);
        Self {
            _server_addr: server_addr.to_string(),
            rx: Some(rx),
        }
    }

    pub async fn receive_frame(&mut self) -> Result<Vec<u8>, CoreError> {
        let rx = self
            .rx
            .as_mut()
            .ok_or_else(|| CoreError::Unknown("no frame receiver configured".into()))?;
        rx.recv()
            .await
            .ok_or_else(|| CoreError::Unknown("frame channel closed".into()))
    }

    pub async fn connect(&self) -> Result<(), CoreError> {
        // ponytail: stub — real RTCPeerConnection connect deferred to full WebRTC integration
        tracing::info!("WebRTC transport connect stub");
        Ok(())
    }
}

/// Stub when `webrtc` feature is disabled.
#[cfg(not(feature = "webrtc"))]
/// Stub when `webrtc` feature is disabled.
#[cfg(not(feature = "webrtc"))]
pub struct Transport {
    rx: Option<tokio::sync::mpsc::Receiver<Vec<u8>>>, 
}

#[cfg(not(feature = "webrtc"))]
impl Transport {
    pub fn new(server_addr: &str) -> Self {
        tracing::info!("Remote transport to {} (stub)", server_addr);
        Self { rx: None }
    }

    pub fn new_with_receiver(
        server_addr: &str,
        rx: tokio::sync::mpsc::Receiver<Vec<u8>>, 
    ) -> Self {
        tracing::info!("Remote transport to {} (stub, with frame receiver)", server_addr);
        Self { rx: Some(rx) }
    }

    pub async fn receive_frame(&mut self) -> Result<Vec<u8>, CoreError> {
        let rx = self
            .rx
            .as_mut()
            .ok_or_else(|| CoreError::Unknown("no frame receiver configured".into()))?;
        rx.recv()
            .await
            .ok_or_else(|| CoreError::Unknown("frame channel closed".into()))
    }

    pub async fn connect(&self) -> Result<(), CoreError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_new_creates_stub() {
        let t = Transport::new("ws://localhost:9800/ws");
        // ponytail: verify no panic on construction
        let _ = t;
    }

    #[tokio::test]
    async fn transport_connect_returns_ok() {
        let t = Transport::new("ws://localhost:9800/ws");
        let result = t.connect().await;
        assert!(result.is_ok());
    }
}
