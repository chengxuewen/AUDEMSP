use crate::codec::CodecId;

/// Encoded video packet (H.264 NAL unit or Annex‑B fragment).
#[derive(Debug, Clone)]
pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub pts: u64,
    pub dts: u64,
    pub keyframe: bool,
    pub codec: CodecId,
}
