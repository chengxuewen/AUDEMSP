//! Shared config schemas for all OMSPBase components.
//!
//! Each component reads its YAML config file and deserializes into these types.

use serde::{Deserialize, Serialize};

/// Config for omspbase-host (capture + encode + push — field/vehicle side).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostConfig {
    /// Config schema version for migration compatibility.
    #[serde(default = "default_version")]
    pub version: u8,

    /// Server address (WebSocket signaling + WebRTC ICE).
    pub server: ServerAddress,

    /// Capture source configuration.
    pub capture: CaptureConfig,

    /// Encoding parameters.
    pub encoder: EncoderConfig,

    /// WebRTC configuration.
    pub webrtc: Option<WebRtcPushConfig>,

    /// PSK for signaling auth.
    pub psk: Option<String>,

    /// Room configuration.
    #[serde(default)]
    pub room: RoomConfig,
}

/// Config for omspbase-server (signaling + relay + monitoring).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Config schema version.
    #[serde(default = "default_version")]
    pub version: u8,

    /// Listen address for HTTP/WS server.
    pub listen: ListenConfig,

    /// Room capacity limit.
    #[serde(default = "default_room_capacity")]
    pub room_capacity: usize,

    /// PSK for signaling auth.
    pub psk: Option<String>,

    /// Rate limit (requests per second per connection).
    #[serde(default = "default_rate_limit")]
    pub rate_limit: u32,

    /// WebSocket max message size in bytes (default: 65536 = 64KB).
    #[serde(default = "default_ws_max_message_size")]
    pub ws_max_message_size: usize,

    /// TLS configuration (optional). When set, server listens on WSS/HTTPS.
    #[serde(default)]
    pub tls: Option<TlsConfig>,
}

/// Config for omspbase-client (pull + decode + control — cockpit/operator side).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    /// Config schema version.
    #[serde(default = "default_version")]
    pub version: u8,

    /// Server address.
    pub server: ServerAddress,

    /// PSK for signaling auth.
    pub psk: Option<String>,

    /// Room configuration.
    #[serde(default)]
    pub room: RoomConfig,

    /// Render window configuration (platform-specific, optional).
    pub render: Option<RenderConfig>,
}

// --- Sub-types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAddress {
    /// WebSocket signaling URL (e.g., "ws://192.168.1.1:9800/ws").
    pub signaling_url: String,

    /// ICE server addresses (STUN/TURN URIs, optional).
    #[serde(default)]
    pub ice_servers: Vec<String>,
}

/// Room configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomConfig {
    /// Room ID (default: "default").
    #[serde(default = "default_room_id")]
    pub id: String,
}

impl Default for RoomConfig {
    fn default() -> Self {
        Self { id: default_room_id() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenConfig {
    /// Bind host.
    #[serde(default = "default_host")]
    pub host: String,

    /// HTTP/WS port.
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    /// Capture source type: "screen", "camera", or "test_pattern".
    pub source: String,

    /// Resolution in format "WIDTHxHEIGHT" (e.g., "1280x720").
    #[serde(default = "default_resolution")]
    pub resolution: String,

    /// Frame rate.
    #[serde(default = "default_framerate")]
    pub framerate: u32,

    /// Device path (e.g., /dev/video0 for V4L2). Optional.
    pub device: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderConfig {
    /// Encoder backend: "nvenc", "videotoolbox", "vaapi", or "software".
    #[serde(default = "default_encoder")]
    pub backend: String,

    /// Target bitrate in kbps.
    #[serde(default = "default_bitrate")]
    pub bitrate_kbps: u32,

    /// Keyframe interval (GOP size in frames).
    #[serde(default = "default_gop")]
    pub keyframe_interval: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcPushConfig {
    /// ICE connection timeout in seconds.
    #[serde(default = "default_ice_timeout")]
    pub ice_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderConfig {
    /// Render backend: "auto" (platform default) or explicit like "gtk", "metal", etc.
    #[serde(default = "default_render_backend")]
    pub backend: String,
}
/// TLS configuration for native rustls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to PEM-encoded certificate chain file.
    pub cert_path: String,

    /// Path to PEM-encoded private key file.
    pub key_path: String,
}


// --- Defaults ---

fn default_version() -> u8 {
    1
}
fn default_host() -> String {
    "0.0.0.0".into()
}
fn default_port() -> u16 {
    9800
}
fn default_resolution() -> String {
    "1280x720".into()
}
fn default_framerate() -> u32 {
    30
}
fn default_encoder() -> String {
    "auto".into()
}
fn default_bitrate() -> u32 {
    2000
}
fn default_gop() -> u32 {
    60
}
fn default_room_capacity() -> usize {
    10
}
#[allow(dead_code)]
fn default_rate_limit() -> u32 {
    100
}
fn default_ws_max_message_size() -> usize {
    65536
}
fn default_room_id() -> String {
    "default".to_string()
}
fn default_ice_timeout() -> u64 {
    30
}
fn default_render_backend() -> String {
    "auto".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_config_roundtrip() {
        let yaml = r#"
server:
  signaling_url: "ws://server:9800/ws"
  ice_servers: ["stun:stun.example.com:3478"]
capture:
  source: "camera"
  resolution: "1920x1080"
  framerate: 60
  device: "/dev/video0"
encoder:
  backend: "nvenc"
  bitrate_kbps: 4000
  keyframe_interval: 120
psk: "secret123"
webrtc:
  ice_timeout_secs: 45
"#;
        let parsed: HostConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.server.signaling_url, "ws://server:9800/ws");
        assert_eq!(parsed.server.ice_servers, vec!["stun:stun.example.com:3478"]);
        assert_eq!(parsed.capture.source, "camera");
        assert_eq!(parsed.capture.resolution, "1920x1080");
        assert_eq!(parsed.capture.framerate, 60);
        assert_eq!(parsed.capture.device.as_deref(), Some("/dev/video0"));
        assert_eq!(parsed.encoder.backend, "nvenc");
        assert_eq!(parsed.encoder.bitrate_kbps, 4000);
        assert_eq!(parsed.encoder.keyframe_interval, 120);
        assert_eq!(parsed.psk.as_deref(), Some("secret123"));
        assert_eq!(parsed.webrtc.as_ref().unwrap().ice_timeout_secs, 45);

        // serialize → parse round-trip
        let re_serialized = serde_yaml::to_string(&parsed).unwrap();
        let re_parsed: HostConfig = serde_yaml::from_str(&re_serialized).unwrap();
        assert_eq!(re_parsed.server.signaling_url, parsed.server.signaling_url);
        assert_eq!(re_parsed.capture.framerate, parsed.capture.framerate);
    }

    #[test]
    fn server_config_roundtrip() {
        let yaml = r#"
listen:
  host: "127.0.0.1"
  port: 8080
room_capacity: 50
rate_limit: 200
psk: "server-psk"
"#;
        let parsed: ServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.listen.host, "127.0.0.1");
        assert_eq!(parsed.listen.port, 8080);
        assert_eq!(parsed.room_capacity, 50);
        assert_eq!(parsed.rate_limit, 200);
        assert_eq!(parsed.psk.as_deref(), Some("server-psk"));

        let re_serialized = serde_yaml::to_string(&parsed).unwrap();
        let re_parsed: ServerConfig = serde_yaml::from_str(&re_serialized).unwrap();
        assert_eq!(re_parsed.listen.port, parsed.listen.port);
        assert_eq!(re_parsed.room_capacity, parsed.room_capacity);
    }

    #[test]
    fn remote_config_roundtrip() {
        let yaml = r#"
server:
  signaling_url: "ws://remote:9800/ws"
psk: "remote-psk"
render:
  backend: "metal"
"#;
        let parsed: RemoteConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.server.signaling_url, "ws://remote:9800/ws");
        assert_eq!(parsed.psk.as_deref(), Some("remote-psk"));
        assert_eq!(parsed.render.as_ref().unwrap().backend, "metal");

        let re_serialized = serde_yaml::to_string(&parsed).unwrap();
        let re_parsed: RemoteConfig = serde_yaml::from_str(&re_serialized).unwrap();
        assert_eq!(re_parsed.server.signaling_url, parsed.server.signaling_url);
    }

    #[test]
    fn version_default() {
        let yaml = r#"
server:
  signaling_url: "ws://host:9800/ws"
capture:
  source: "screen"
encoder:
  backend: "auto"
"#;
        let parsed: HostConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.version, 1);
    }

    #[test]
    fn optional_psk_field() {
        let with_psk = r#"
server:
  signaling_url: "ws://host:9800/ws"
capture:
  source: "screen"
encoder:
  backend: "auto"
psk: "my-psk"
"#;
        let parsed: HostConfig = serde_yaml::from_str(with_psk).unwrap();
        assert_eq!(parsed.psk.as_deref(), Some("my-psk"));

        let without_psk = r#"
server:
  signaling_url: "ws://host:9800/ws"
capture:
  source: "screen"
encoder:
  backend: "auto"
"#;
        let parsed: HostConfig = serde_yaml::from_str(without_psk).unwrap();
        assert_eq!(parsed.psk, None);
    }

    #[test]
    fn capture_config_defaults() {
        let yaml = r#"
source: "test_pattern"
"#;
        let parsed: CaptureConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.source, "test_pattern");
        assert_eq!(parsed.resolution, "1280x720");
        assert_eq!(parsed.framerate, 30);
        assert_eq!(parsed.device, None);
    }

    #[test]
    fn encoder_config_defaults() {
        let yaml = r#"
backend: "software"
"#;
        let parsed: EncoderConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.backend, "software");
        assert_eq!(parsed.bitrate_kbps, 2000);
        assert_eq!(parsed.keyframe_interval, 60);
    }

    #[test]
    fn listen_config_defaults() {
        let yaml = r#"
host: "192.168.1.1"
"#;
        let parsed: ListenConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.host, "192.168.1.1");
        assert_eq!(parsed.port, 9800);
    }

    #[test]
    fn server_config_defaults() {
        let yaml = "listen: {}";
        let parsed: ServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.listen.host, "0.0.0.0");
        assert_eq!(parsed.listen.port, 9800);
        assert_eq!(parsed.room_capacity, 10);
        assert_eq!(parsed.rate_limit, 100);
        assert_eq!(parsed.psk, None);
        assert_eq!(parsed.ws_max_message_size, 65536);
        assert!(parsed.tls.is_none());
    }

    #[test]
    fn server_config_with_tls() {
        let yaml = r#"
listen:
  port: 443
tls:
  cert_path: "/etc/certs/server.crt"
  key_path: "/etc/certs/server.key"
"#;
        let parsed: ServerConfig = serde_yaml::from_str(yaml).unwrap();
        let tls = parsed.tls.unwrap();
        assert_eq!(tls.cert_path, "/etc/certs/server.crt");
        assert_eq!(tls.key_path, "/etc/certs/server.key");
    }
}
