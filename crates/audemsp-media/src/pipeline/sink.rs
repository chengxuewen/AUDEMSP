use crate::error::MediaError;

/// Unique identifier for a registered sink.
pub type SinkId = u64;

/// Backpressure signals from a sink to the source.
#[derive(Debug, Clone, Copy)]
pub struct VideoSinkWants {
    /// Whether this sink is actively consuming frames.
    pub is_active: bool,
    /// Maximum pixel count (width × height). 0 = no limit.
    pub max_pixel_count: u32,
    /// Maximum framerate this sink can handle.
    pub max_framerate_fps: u32,
    /// Pixel alignment requirement (typically 2 for chroma subsampling).
    pub resolution_alignment: u32,
    /// Whether the sink applies rotation itself.
    pub rotation_applied: bool,
}

impl Default for VideoSinkWants {
    fn default() -> Self {
        Self {
            is_active: true,
            max_pixel_count: 0,
            max_framerate_fps: 0,
            resolution_alignment: 1,
            rotation_applied: false,
        }
    }
}

/// Consumer of video frames. Returns updated wants for dynamic backpressure.
pub trait VideoSink<F>: Send {
    fn on_frame(&self, frame: &F) -> Result<VideoSinkWants, MediaError>;
}
