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