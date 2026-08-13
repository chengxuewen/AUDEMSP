//! Remote configuration — loads from YAML into core RemoteConfig.
//!
//! The config module delegates to mediaservo_common::config::RemoteConfig
//! for the shared schema. This module exposes a convenience loader.

use mediaservo_common::config::RemoteConfig;
use mediaservo_common::error::CoreError;

/// Load RemoteConfig from a YAML file path.
///
/// # Errors
/// Returns `CoreError::ConfigParse` if the file cannot be read or parsed.
pub fn load(path: &str) -> Result<RemoteConfig, CoreError> {
    let file = std::fs::File::open(path)
        .map_err(|e| CoreError::ConfigParse(format!("Cannot open {path}: {e}")))?;
    serde_yaml::from_reader(file)
        .map_err(|e| CoreError::ConfigParse(format!("Parse error: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_remote_config() {
        let yaml = r#"
version: 1
server:
  signaling_url: "ws://server.local:9100/ws"
  ice_servers: []
"#;
        let config: RemoteConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.server.signaling_url, "ws://server.local:9100/ws");
        assert!(config.psk.is_none());
        assert!(config.render.is_none());
    }

    #[test]
    fn parse_full_remote_config() {
        let yaml = r#"
version: 1
server:
  signaling_url: "wss://server.example.com/ws"
  ice_servers:
    - "stun:stun.l.google.com:19302"
    - "turn:relay.example.com:3478"
psk: "secure-key-456"
render:
  backend: "metal"
"#;
        let config: RemoteConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.server.signaling_url, "wss://server.example.com/ws");
        assert_eq!(config.server.ice_servers.len(), 2);
        assert_eq!(config.psk.as_deref(), Some("secure-key-456"));
        let render = config.render.unwrap();
        assert_eq!(render.backend, "metal");
    }
}
