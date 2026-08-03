//! GStreamer decode pipeline — receive H.264 via appsrc, decode via decodebin, render.
//!
//! Feature-gated behind `gstreamer`. In default builds, a no-op stub is provided.

use audemsp_media::error::MediaError;

#[cfg(feature = "gstreamer")]
use gstreamer::prelude::*;
/// GStreamer decode pipeline with appsrc input.
///
/// Pipeline: appsrc (H.264 byte-stream) → decodebin → videoconvert → videoscale → autovideosink
#[cfg(feature = "gstreamer")]
pub struct DecodePipeline {
    pipeline: gstreamer::Pipeline,
    appsrc: gstreamer_app::AppSrc,
}

#[cfg(feature = "gstreamer")]
impl DecodePipeline {
    /// Build the decode pipeline.
    ///
    /// `display_name` is logged for identification. `decoder` is reserved
    /// for pinned-element selection in production.
    pub fn new(
        display_name: &str,
        width: u32,
        height: u32,
        decoder: Option<&str>,
    ) -> Self {
        gstreamer::init().ok();
        if let Some(dec) = decoder {
            tracing::info!(decoder = dec, "Remote decoder hint");
        }

        let desc = format!(
            "appsrc name=src format=time is-live=true do-timestamp=true \
             caps=video/x-h264,stream-format=byte-stream,alignment=au \
             ! h264parse ! vtdec \
             ! videoconvert ! videoscale \
             ! video/x-raw,width={width},height={height} \
             ! autovideosink sync=false"
        );
        let pipeline = gstreamer::parse::launch(&desc)
            .unwrap_or_else(|e| panic!("Failed to create decode pipeline: {e}"))
            .downcast::<gstreamer::Pipeline>()
            .expect("Pipeline downcast failed");

        // ponytail: bus sync handler for error/warning logging (match host pattern)
        let bus = pipeline.bus().expect("Pipeline bus not available");
        bus.set_sync_handler(move |_bus, msg| {
            match msg.view() {
                gstreamer::MessageView::Error(e) => {
                    tracing::error!(
                        src = %e.src().map(|s| s.path_string()).unwrap_or_default(),
                        error = %e.error(),
                        debug = %e.debug().unwrap_or_default(),
                        "GStreamer bus error"
                    );
                }
                gstreamer::MessageView::Warning(w) => {
                    tracing::warn!(
                        src = %w.src().map(|s| s.path_string()).unwrap_or_default(),
                        error = %w.error(),
                        debug = %w.debug().unwrap_or_default(),
                        "GStreamer bus warning"
                    );
                }
                _ => {}
            }
            gstreamer::BusSyncReply::Pass
        });

        let appsrc = pipeline
            .by_name("src")
            .expect("appsrc element not found")
            .downcast::<gstreamer_app::AppSrc>()
            .expect("Failed to downcast to AppSrc");

        tracing::info!(display = display_name, "Decode pipeline created");
        Self { pipeline, appsrc }
    }

    /// Start the pipeline (set to Playing state).
    pub fn start(&mut self) -> Result<(), MediaError> {
        self.pipeline
            .set_state(gstreamer::State::Playing)
            .map_err(|e| MediaError::DecoderInit(format!("Pipeline start failed: {e}")))?;
        tracing::info!("Decode pipeline started");
        Ok(())
    }

    /// Stop the pipeline (set to Null state).
    pub fn stop(&self) -> Result<(), MediaError> {
        self.pipeline
            .set_state(gstreamer::State::Null)
            .map_err(|e| MediaError::Unknown(format!("Pipeline stop failed: {e}")))?;
        tracing::info!("Decode pipeline stopped");
        Ok(())
    }

    /// Push an H.264 byte-stream buffer into the pipeline.
    pub fn push_h264(&self, data: &[u8]) -> Result<(), MediaError> {
        // ponytail: copy buffer into GStreamer-owned memory
        let mut buffer = gstreamer::Buffer::with_size(data.len())
            .map_err(|_| MediaError::OutOfMemory)?;
        {
            let buffer_ref = buffer.make_mut();
            buffer_ref.copy_from_slice(0, data).map_err(|e| MediaError::Unknown(format!("buffer copy: {e}")))?;
        }
        self.appsrc
            .push_buffer(buffer)
            .map_err(|e| MediaError::Unknown(format!("push_buffer failed: {e}")))?;
        Ok(())
    }
}

#[cfg(feature = "gstreamer")]
impl Drop for DecodePipeline {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gstreamer::State::Null);
    }
}

// --- No-op stub when gstreamer feature is disabled ---

/// Stub pipeline when `gstreamer` feature is not enabled.
#[cfg(not(feature = "gstreamer"))]
pub struct DecodePipeline;

#[cfg(not(feature = "gstreamer"))]
impl DecodePipeline {
    pub fn new(
        _display_name: &str,
        _width: u32,
        _height: u32,
        _decoder: Option<&str>,
    ) -> Self {
        tracing::info!("Decode pipeline disabled (gstreamer feature not enabled)");
        Self
    }

    pub fn start(&mut self) -> Result<(), MediaError> {
        tracing::warn!("GStreamer decode not available — build with --features gstreamer");
        Ok(())
    }

    pub fn stop(&self) -> Result<(), MediaError> {
        Ok(())
    }

    pub fn push_h264(&self, _data: &[u8]) -> Result<(), MediaError> {
        Ok(())
    }
}
