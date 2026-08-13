//! Adapters that wrap remote crate types as mediaservo_common pipeline nodes,
//! so they can be orchestrated by PipelineEngine.
//!
//! - FrameSource: wraps tokio::sync::mpsc::UnboundedReceiver<Vec<u8>> as MediaSource
//! - DecodeSink: wraps crate::decode::DecodePipeline as MediaSink

use std::sync::Arc;

use mediaservo_media::error::MediaError;
use mediaservo_media::pipeline::core::{
    EncodedFragment, FormatSpec, FragmentFlags, FrameTiming, InternalPacket, MediaSink,
    MediaSource, MediaType, NodeCapability, NodeInfo, PipelineNode,
};

type Result<T> = std::result::Result<T, MediaError>;

// ── NAL unit helpers ──

/// Scan Annex B byte-stream for a NAL unit header at `offset`.
fn scan_nal_header(data: &[u8], offset: usize) -> Option<(u8, usize)> {
    if offset + 4 > data.len() {
        return None;
    }
    let header_len = if data[offset..].starts_with(&[0x00, 0x00, 0x00, 0x01]) {
        4
    } else if data[offset..].starts_with(&[0x00, 0x00, 0x01]) {
        3
    } else {
        return None;
    };
    let nal_pos = offset + header_len;
    if nal_pos >= data.len() {
        return None;
    }
    let nal_type = data[nal_pos] & 0x1F;
    let nal_end = (nal_pos + 1..data.len().saturating_sub(3))
        .find(|&i| {
            data[i..].starts_with(&[0x00, 0x00, 0x01])
                || data[i..].starts_with(&[0x00, 0x00, 0x00, 0x01])
        })
        .unwrap_or(data.len());
    Some((nal_type, nal_end))
}

/// Check if byte-stream contains an IDR keyframe (NAL type 5).
fn is_keyframe(data: &[u8]) -> bool {
    let mut offset = 0;
    while let Some((nal_type, next)) = scan_nal_header(data, offset) {
        if nal_type == 5 {
            return true;
        }
        offset = next;
    }
    false
}

// ── FrameSource ──

/// Wraps an unbounded MPSC receiver for frames arriving from WebRTC transport.
/// Yields encoded H.264 byte-stream fragments to the pipeline.
pub struct FrameSource {
    rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    frame_num: u64,
}

impl FrameSource {
    pub fn new(rx: tokio::sync::mpsc::Receiver<Vec<u8>>) -> Self {
        Self { rx, frame_num: 0 }
    }
}

impl NodeInfo for FrameSource {
    fn name(&self) -> &str {
        "webrtc-source"
    }

    fn capabilities(&self) -> NodeCapability {
        NodeCapability {
            input: FormatSpec {
                media_type: MediaType::Both,
                codecs: None,
                pixel_formats: vec![],
            },
            output: FormatSpec {
                media_type: MediaType::Encoded,
                codecs: None,
                pixel_formats: vec![],
            },
        }
    }
}

impl PipelineNode for FrameSource {
    fn on_start(&mut self) -> Result<()> {
        Ok(())
    }

    fn on_stop(&mut self) -> Result<()> {
        Ok(())
    }
}

impl MediaSource for FrameSource {
    type Output = InternalPacket;

    fn poll_fragment(&mut self) -> Result<Option<Self::Output>> {
        match self.rx.try_recv() {
            Ok(data) => {
                self.frame_num += 1;
                let is_kf = is_keyframe(&data);
                let pts = self.frame_num;
                Ok(Some(InternalPacket::Encoded(EncodedFragment {
                    track_id: "webrtc".into(),
                    timing: FrameTiming {
                        dts: pts,
                        pts,
                        duration: 0,
                        wall_clock: Some(std::time::Instant::now()),
                    },
                    flags: FragmentFlags {
                        keyframe: is_kf,
                        independent: true,
                        discardable: false,
                    },
                    codec: "h264".into(),
                    init_data: None,
                    payload: data,
                })))
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                tracing::info!("FrameSource: channel disconnected");
                Ok(None)
            }
        }
    }
}

// ── DecodeSink ──

/// Wraps DecodePipeline as an engine sink: receives encoded packets from
/// the pipeline and feeds them into the GStreamer decode pipeline.
pub struct DecodeSink {
    pipeline: Arc<crate::decode::DecodePipeline>,
}

impl DecodeSink {
    pub fn new(pipeline: Arc<crate::decode::DecodePipeline>) -> Self {
        Self { pipeline }
    }
}

impl NodeInfo for DecodeSink {
    fn name(&self) -> &str {
        "decode-sink"
    }

    fn capabilities(&self) -> NodeCapability {
        NodeCapability {
            input: FormatSpec {
                media_type: MediaType::Encoded,
                codecs: None,
                pixel_formats: vec![],
            },
            output: FormatSpec {
                media_type: MediaType::Encoded,
                codecs: None,
                pixel_formats: vec![],
            },
        }
    }
}

impl PipelineNode for DecodeSink {
    fn on_start(&mut self) -> Result<()> {
        Ok(())
    }

    fn on_stop(&mut self) -> Result<()> {
        Ok(())
    }
}

impl MediaSink for DecodeSink {
    type Input = InternalPacket;

    fn on_fragment(&mut self, fragment: Self::Input) -> Result<()> {
        let payload = match fragment {
            InternalPacket::Encoded(f) => f.payload,
            _ => return Ok(()),
        };
        self.pipeline.push_h264(&payload)
    }
}
