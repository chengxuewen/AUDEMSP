//! MediaServo Media — video frame types, buffers, source/sink pipeline, transforms.

pub mod error;
pub mod pixel_format;
pub mod transform;
pub mod base;
pub mod pipeline;
pub mod backend;
pub mod engine;
pub mod plugin;

#[cfg(not(any(feature = "backend-yuv-sys", feature = "backend-native")))]
compile_error!(
    "At least one backend feature must be enabled: \
     backend-yuv-sys or backend-native"
);
