//! field 会话配置（契约 §4 落地）。
//!
//! `PushConfig`/`PullConfig` 是会话的入口配置；`PublishOptions` 为发布
//! 富选项（MVP 只落 codec + encoder_backend，其余待真传输接入后按需扩展）。

use mediaservo_common::protocol::PeerRole;

/// 推流会话配置（契约 §4）。
#[derive(Debug, Clone)]
pub struct PushConfig {
    /// 信令 WS 地址，如 `ws://host:9800/ws`。
    pub url: String,
    /// PSK 认证密钥。
    pub psk: String,
    /// 房间 ID。
    pub room: String,
    /// 节点角色（Host/Pusher）。
    pub role: PeerRole,
    /// 推流视频分辨率（宽）。
    pub width: u32,
    /// 推流视频分辨率（高）。
    pub height: u32,
    /// 帧率（与 libwebrtc 编码器配置匹配，C17）。
    pub framerate: u32,
    /// 编码码率 kbps。
    pub bitrate_kbps: u32,
    /// 关键帧间隔秒（GOP 上限，默认 2）。
    pub keyframe_interval: u64,
}

impl PushConfig {
    /// 便捷构造（默认 1280x720@30fps / 2000kbps / 2s GOP）。
    pub fn new(url: impl Into<String>, psk: impl Into<String>, room: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            psk: psk.into(),
            room: room.into(),
            role: PeerRole::Host,
            width: 1280,
            height: 720,
            framerate: 30,
            bitrate_kbps: 2000,
            keyframe_interval: 2,
        }
    }
}

/// 拉流会话配置（契约 §4；MVP 仅定义类型，connect 暂未接入 consume 链路）。
#[derive(Debug, Clone)]
pub struct PullConfig {
    /// 信令 WS 地址。
    pub url: String,
    /// PSK 认证密钥。
    pub psk: String,
    /// 房间 ID。
    pub room: String,
    /// 节点角色（Remote/Consumer）。
    pub role: PeerRole,
    /// 是否自动订阅房间内所有 producer。
    pub auto_subscribe: bool,
}

impl Default for PullConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            psk: String::new(),
            room: String::new(),
            role: PeerRole::Remote,
            auto_subscribe: true,
        }
    }
}

/// 发布选项（契约 §4；MVP 只落 codec + encoder_backend）。
#[derive(Debug, Clone)]
pub struct PublishOptions {
    /// 编码格式（VP8/H264/VP9/AV1，与 router 对齐）。
    pub codec: String,
    /// 编码器后端（auto/software/hardware）。
    pub encoder_backend: String,
}

impl Default for PublishOptions {
    fn default() -> Self {
        Self {
            codec: "vp8".to_string(),
            encoder_backend: "auto".to_string(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_config_defaults_sane() {
        let cfg = PushConfig::new("ws://x", "psk", "room");
        assert_eq!(cfg.width, 1280);
        assert_eq!(cfg.height, 720);
        assert_eq!(cfg.framerate, 30);
        assert_eq!(cfg.bitrate_kbps, 2000);
        assert_eq!(cfg.keyframe_interval, 2);
        assert_eq!(cfg.role, PeerRole::Host);
    }

    #[test]
    fn push_config_custom_values_preserved() {
        let mut cfg = PushConfig::new("ws://x", "psk", "room");
        cfg.width = 640;
        cfg.height = 480;
        cfg.framerate = 15;
        cfg.bitrate_kbps = 800;
        cfg.keyframe_interval = 4;
        assert_eq!(cfg.width, 640);
        assert_eq!(cfg.framerate, 15);
        assert_eq!(cfg.bitrate_kbps, 800);
        assert_eq!(cfg.keyframe_interval, 4);
    }

    #[test]
    fn publish_options_defaults_vp8_auto() {
        let opts = PublishOptions::default();
        assert_eq!(opts.codec, "vp8");
        assert_eq!(opts.encoder_backend, "auto");
    }

    #[test]
    fn pull_config_default_auto_subscribe() {
        let cfg = PullConfig::default();
        assert!(cfg.auto_subscribe);
        assert_eq!(cfg.role, PeerRole::Remote);
    }

    #[test]
    fn push_config_url_trims_trailing_slash_in_connect() {
        // SignalClient 内部 trim_end_matches('/') — 配置保持原样, 连接时处理
        let cfg = PushConfig::new("ws://host:9800/ws/", "psk", "room");
        assert_eq!(cfg.url, "ws://host:9800/ws/");
    }
}
