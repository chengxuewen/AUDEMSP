//! MediaServo Deck — 媒体数据面 SDK。
//!
//! 独立部署场景（本地监控/NVR）直采直录，无 WebRTC 依赖：
//! - [`source`]: 采集（CameraSource/AudioSource/ScreenSource + MediaDevices 枚举）
//! - [`record`]: 录制（Recorder，FFmpeg mux 落盘）
//! - [`playback`]: 回放（Player，demux + decode）— Phase 3

pub mod error;
pub mod playback;
pub mod record;
pub mod source;

pub use error::DeckError;
pub use source::{CameraSource, CaptureOptions, FrameStream, MediaDevices, MediaDeviceKind};