//! Backend abstraction — compile-time cfg dispatch.

#[cfg(all(feature = "backend-gstreamer", feature = "backend-ffmpeg"))]
compile_error!("Only one backend can be enabled at a time.");

#[cfg(feature = "backend-ffmpeg")]
pub(crate) mod ffmpeg;
#[cfg(feature = "backend-gstreamer")]
pub(crate) mod gstreamer;
#[cfg(not(any(feature = "backend-gstreamer", feature = "backend-ffmpeg")))]
pub(crate) mod stub;

// ── Compile-time type dispatch ──

#[cfg(feature = "backend-ffmpeg")]
pub(crate) type ActiveEncoder = ffmpeg::FfmpegEncoder;
#[cfg(feature = "backend-gstreamer")]
pub(crate) type ActiveEncoder = gstreamer::GstEncoder;
#[cfg(not(any(feature = "backend-gstreamer", feature = "backend-ffmpeg")))]
pub(crate) type ActiveEncoder = stub::StubEncoder;

#[cfg(feature = "backend-ffmpeg")]
pub(crate) type ActiveDecoder = ffmpeg::FfmpegDecoder;
#[cfg(feature = "backend-gstreamer")]
pub(crate) type ActiveDecoder = gstreamer::GstDecoder;
#[cfg(not(any(feature = "backend-gstreamer", feature = "backend-ffmpeg")))]
pub(crate) type ActiveDecoder = stub::StubDecoder;
