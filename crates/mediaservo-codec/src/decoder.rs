use crate::{DecoderConfig, VideoFrame, CodecError};

/// Video decoder trait — push encoded packets, pull decoded frames.
pub trait VideoDecoder: Send {
    fn configure(&mut self, config: &DecoderConfig) -> Result<(), CodecError>;
    fn push_packet(&mut self, data: &[u8]) -> Result<(), CodecError>;
    fn pull_frame(&mut self) -> Result<Option<VideoFrame>, CodecError>;
    fn flush(&mut self) -> Result<(), CodecError>;
}
