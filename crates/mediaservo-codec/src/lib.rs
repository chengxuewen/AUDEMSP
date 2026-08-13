//! MediaServo Codec — unified video encode/decode API.
//!
//! Two backends: GStreamer (dynamic .so, Host default) and FFmpeg (static .a, Remote default).
//! With no features enabled, a stub backend is used for compilation checks.

pub mod codec;
pub mod config;
pub mod encoder;
pub mod decoder;
pub mod factory;
pub mod error;
pub mod frame;
pub mod packet;
pub mod backend;

pub use codec::{BackendId, CodecId, PixelFormat, VideoFormat, FrameRate};
pub use config::{EncoderConfig, DecoderConfig, EncoderPreset, Bitrate};
pub use encoder::VideoEncoder;
pub use decoder::VideoDecoder;
pub use error::CodecError;
pub use factory::CodecFactory;
pub use frame::{VideoFrame, Plane};
pub use packet::EncodedPacket;
