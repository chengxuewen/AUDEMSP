# omspbase-media Crate Design

**Created:** 2026-07-20 | **Updated:** 2026-07-22
**Status:** ✅ Implemented — divergences from spec documented below

## Overview

`omspbase-media` is a new workspace crate providing video frame types, pixel format buffers, source/sink pipeline traits, frame generators, and spatial/color transforms. Code is ported from [webrtc-kit](https://github.com/chengxuewen/webrtc-kit) `video/` module, with enhancements from OpenCTK's `media/source` reference architecture.

**Design principle:** Media crate with minimal `omspbase-core` dependency (for `CoreError`). Async pipeline engine (`tokio`). Self-contained video types with `VideoFrame<T>`, `VideoBuffer` trait, `VideoSource<F>` / `VideoSink<F>` pipeline traits.

**Divergence from original spec:** Added `omspbase-core` and `tokio` deps after engine/plugin modules moved from core (D169).

## Crate Structure

```
crates/omspbase-media/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # pub mod exports + compile_error! guard
│   ├── error.rs                  # MediaError (use thiserror::Error)
│   ├── pixel_format.rs           # PixelFormat enum (unified RGB + YUV)
│   ├── transform.rs              # VideoTransform trait (scale/mirror/crop/rotate + color convert)
│   ├── engine.rs                 # PipelineEngine (moved from core, D169)
│   ├── plugin.rs                 # PluginManager (moved from core, D169)
│   ├── base/                     # Zero-dependency foundation types
│   │   ├── mod.rs
│   │   ├── frame.rs              # VideoFrame<T>, FrameMetadata, BoxVideoFrame
│   │   ├── buffer.rs             # VideoBuffer trait + I420/I422/I444/NV12/I010
│   │   └── rotation.rs           # VideoRotation enum (0/90/180/270)
│   ├── pipeline/                 # Source/Sink pipeline
│   │   ├── mod.rs
│   │   ├── source.rs             # VideoSource<F> trait
│   │   ├── sink.rs               # VideoSink<F> trait + VideoSinkWants
│   │   ├── broadcaster.rs        # VideoBroadcaster<F> (frame fan-out)
│   │   ├── core.rs               # Pipeline traits (moved from core: MediaSource etc.)
│   │   ├── broadcast.rs          # FragmentBroadcaster<P> (moved from core)
│   │   └── generator/
│   │       └── ...               # VideoFrameGenerator + patterns
│   └── backend/                  # Backend implementations (singular)
│       ├── mod.rs
│       ├── native.rs             # Pure Rust (feature = "backend-native")
│       └── libyuv.rs             # yuv-sys bindings (default feature)
```

**Layer dependency:** `pixel_format` + `base` (zero deps) → `pipeline` → `transform` + `backends`.

## Cargo.toml

```toml
[package]
name = "omspbase-media"
version.workspace = true
edition.workspace = true
description = "OMSPBase Media — video frame types, buffers, source/sink pipeline, transforms"
license.workspace = true

[dependencies]
thiserror = "2"
tracing = "0.1"
chrono = { version = "0.4", features = ["clock"] }
rand = "0.8"
yuv-sys = { version = "0.3", optional = true }
omspbase-core = { path = "../omspbase-core" }
tokio = { version = "1", features = ["sync", "rt-multi-thread", "time", "macros"] }

[dev-dependencies]
eframe = "0.30"
egui = "0.30"
image = "0.25"

[features]
default = ["backend-yuv-sys"]
backend-yuv-sys = ["dep:yuv-sys"]
backend-native = []

[package.metadata.docs.rs]
features = ["backend-yuv-sys"]
```

**Divergence from original spec:** `libyuv-sys` renamed to `yuv-sys`. Added `omspbase-core`, `tokio`, `chrono`, `rand` deps (engine/plugin from core). Added egui dev-deps for examples.

- Minimal dependencies: `thiserror` + `tracing` + optional `libyuv-sys`
- Inherits `version`/`edition`/`license` from workspace
- No `tokio`, `serde`, `async-trait` — synchronous library

## Key Types & Traits

### `PixelFormat` — Unified Pixel Format Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    // RGB formats
    ARGB,
    BGRA,
    ABGR,
    RGBA,
    // YUV planar formats
    I420,
    I422,
    I444,
    // YUV biplanar
    NV12,
    // YUV 10-bit
    I010,
}
```

Unified enum replacing webrtc-kit's separate `VideoBufferType` + `VideoFormatType`. Follows OpenCTK's single `VideoType` approach. Replaces `VideoBuffer::buffer_type() -> VideoBufferType` with `format() -> PixelFormat`, and `i420_to_argb(fmt: VideoFormatType)` with `i420_to_argb(fmt: PixelFormat)`.

### Base Types (zero external dependencies, pure `std`)

#### `VideoRotation`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoRotation {
    #[default]
    Rotation0 = 0,
    Rotation90 = 90,
    Rotation180 = 180,
    Rotation270 = 270,
}
```

#### `VideoFrame<T>`

```rust
pub struct VideoFrame<T> {
    pub rotation: VideoRotation,
    pub timestamp_us: i64,
    pub metadata: Option<FrameMetadata>,
    pub buffer: T,
}

/// Builder-style construction
impl<T> VideoFrame<T> {
    pub fn new(buffer: T) -> Self;
    pub fn with_rotation(mut self, rotation: VideoRotation) -> Self;
    pub fn with_timestamp(mut self, timestamp_us: i64) -> Self;
    pub fn with_metadata(mut self, metadata: FrameMetadata) -> Self;
}

pub type BoxVideoFrame = VideoFrame<Box<dyn VideoBuffer>>;
```

#### `FrameMetadata`

```rust
/// Per-frame metadata carried alongside pixel data
pub struct FrameMetadata {
    /// Application-assigned timestamp (e.g., capture time)
    pub user_timestamp: Option<u64>,
    /// Monotonically incrementing frame sequence number
    pub frame_id: Option<u32>,
}
```

#### `VideoBuffer` trait

```rust
pub trait VideoBuffer: Debug + Send + Sync {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn format(&self) -> PixelFormat;

    /// Zero-copy borrow of I420 planes for backend processing
    fn as_i420_ref(&self) -> Option<I420BufferRef<'_>>;

    /// Convert any pixel format to I420 (center format)
    fn to_i420(&self) -> Result<I420Buffer, MediaError>;

    // Downcast helpers (default returns None)
    fn as_i420(&self) -> Option<&I420Buffer> { None }
    fn as_i422(&self) -> Option<&I422Buffer> { None }
    fn as_i444(&self) -> Option<&I444Buffer> { None }
    fn as_nv12(&self) -> Option<&NV12Buffer> { None }
    fn as_i010(&self) -> Option<&I010Buffer> { None }
}
```

#### `I420BufferRef`

Borrowed view of I420 planes for backend trait arguments. `Copy` + passed by value (not `&`) to avoid double-indirection in hot path.

```rust
#[derive(Copy, Clone)]
pub struct I420BufferRef<'a> {
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub stride_y: usize,
    pub stride_u: usize,
    pub stride_v: usize,
}
```

#### Concrete buffer types

| Type | Format | Planes |
|------|--------|--------|
| `I420Buffer` | YUV 4:2:0 8-bit | 3 planes (Y, U, V) |
| `I422Buffer` | YUV 4:2:2 8-bit | 3 planes (Y, ½w×U, ½w×V) |
| `I444Buffer` | YUV 4:4:4 8-bit | 3 planes (full Y, U, V) |
| `NV12Buffer` | YUV 4:2:0 8-bit | 2 planes (Y, interleaved UV) |
| `I010Buffer` | YUV 4:2:0 10-bit | `Vec<u16>` |

All buffers implement `VideoBuffer`. `I420Buffer` is the canonical center format — all other types support `to_i420()`.

### Pipeline Traits (depends on `base`)

#### `VideoSinkWants`

Backpressure mechanism from OpenCTK:

```rust
pub struct VideoSinkWants {
    /// Whether this sink is actively consuming frames
    pub is_active: bool,
    /// Maximum pixel count (width × height cap). 0 means no limit.
    pub max_pixel_count: u32,
    /// Maximum framerate that this sink can handle
    pub max_framerate_fps: u32,
    /// Pixel alignment requirement (typically 2 for chroma subsampling)
    pub resolution_alignment: u32,
    /// Whether the sink applies rotation itself
    pub rotation_applied: bool,
}
```

#### `VideoSource<F>` / `VideoSink<F>`

```rust
pub type SinkId = u64;

pub trait VideoSink<F>: Send {
    /// Consume a frame, optionally returning updated wants (dynamic backpressure)
    fn on_frame(&self, frame: &F) -> Result<VideoSinkWants, MediaError>;
}

pub trait VideoSource<F>: Send + Sync {
    /// Register or update a sink. Returns a SinkId for later removal.
    fn add_or_update_sink(&self, sink: Box<dyn VideoSink<F>>, wants: VideoSinkWants) -> SinkId;
    /// Remove a previously registered sink by its SinkId.
    fn remove_sink(&self, id: SinkId);
}
```

`VideoSink::on_frame` returns `VideoSinkWants` to enable dynamic backpressure — the sink can signal changing constraints (CPU load, buffer fullness) frame-by-frame. The source/broadcaster re-aggregates after each frame delivery.

#### `VideoBroadcaster<F>`

Fan-out combinator — implements both `VideoSource<F>` and `VideoSink<F>`:

```rust
pub struct VideoBroadcaster<F> { /* Mutex<Vec<(SinkId, sink, wants)>> */ }

impl<F: Send + 'static> VideoBroadcaster<F> {
    pub fn new() -> Self;
    /// Aggregate wants from all active sinks
    pub fn wants(&self) -> VideoSinkWants;
    /// Broadcast frame to all active sinks (uses Arc<F> internally, does not clone F)
    pub fn on_frame(&self, arc_frame: &Arc<F>) -> Result<(), MediaError>;
    pub fn sink_count(&self) -> usize;
}
```

No `F: Clone` bound — frames are shared via `Arc<F>` for zero-copy broadcast.

**Want aggregation rules:**
- `max_pixel_count` → minimum across all sinks
- `max_framerate_fps` → minimum across all sinks
- `is_active` → `false` if ALL sinks inactive
- `resolution_alignment` → LCM of all sink alignments

#### `FramePattern` / `VideoFrameGenerator`

```rust
pub trait FramePattern: Send + Sync {
    fn draw(&mut self, y: &mut [u8], u: &mut [u8], v: &mut [u8],
            stride_y: usize, stride_u: usize, stride_v: usize);
}

pub struct VideoFrameGenerator {
    // implements VideoSource<VideoFrame<Box<dyn VideoBuffer>>>
    // Runs on dedicated thread, calls pattern.draw() per frame
    // Used for P2P testing as video source when no camera is available
}
```

### `VideoTransform` Trait (depends on `base`; compile-time monomorphized)

Merged trait combining spatial transforms and color conversion:

```rust
pub trait VideoTransform: Send + Sync {
    // --- Spatial transforms ---

    fn scale(src: I420BufferRef, src_w: u32, src_h: u32,
             dst: I420BufferRef, dst_w: u32, dst_h: u32) -> Result<(), MediaError>;
    fn mirror(src: I420BufferRef, w: u32, h: u32,
              dst: I420BufferRef) -> Result<(), MediaError>;
    fn crop(src: I420BufferRef, x: u32, y: u32, w: u32, h: u32,
            dst: I420BufferRef) -> Result<(), MediaError>;
    fn rotate(src: I420BufferRef, w: u32, h: u32, rot: VideoRotation,
              dst: I420BufferRef) -> Result<(), MediaError>;

    // --- Color space conversion ---

    fn argb_to_i420(argb: &[u8], w: u32, h: u32,
                    dst: I420BufferRef) -> Result<(), MediaError>;
    fn i420_to_argb(src: I420BufferRef, w: u32, h: u32, fmt: PixelFormat,
                    dst: &mut [u8]) -> Result<(), MediaError>;
    fn i420_to_nv12(src: I420BufferRef, w: u32, h: u32,
                    dst_y: &mut [u8], dst_uv: &mut [u8]) -> Result<(), MediaError>;
    fn nv12_to_i420(src_y: &[u8], src_uv: &[u8], w: u32, h: u32,
                    dst: I420BufferRef) -> Result<(), MediaError>;
}
```

- `I420BufferRef` passed by value (it is `Copy`)
- `Send + Sync` bound ensures thread-safe use across threads

#### Backend selection (compile-time via feature flags)

```rust
// In backends/mod.rs:
#[cfg(feature = "backend-libyuv-sys")]
mod libyuv;
#[cfg(feature = "backend-libyuv-sys")]
pub use libyuv::LibyuvTransform;

#[cfg(feature = "backend-native")]
mod native;
#[cfg(feature = "backend-native")]
pub use native::NativeTransform;

#[cfg(feature = "backend-libyuv-sys")]
pub fn get_backend() -> impl VideoTransform {
    libyuv::LibyuvTransform
}

#[cfg(all(feature = "backend-native", not(feature = "backend-libyuv-sys")))]
pub fn get_backend() -> impl VideoTransform {
    native::NativeTransform
}

// In lib.rs:
#[cfg(not(any(feature = "backend-libyuv-sys", feature = "backend-native")))]
compile_error!("At least one backend feature must be enabled: \
    backend-libyuv-sys or backend-native");
```

Zero dynamic dispatch — the compiler monomorphizes the backend at crate compilation time.

## Error Handling

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MediaError {
    #[error("buffer size mismatch: expected {expected}, got {actual}")]
    BufferSizeMismatch { expected: u32, actual: u32 },
    #[error("unsupported pixel format: {0:?}")]
    UnsupportedFormat(PixelFormat),
    #[error("invalid dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("invalid rotation: {0:?}")]
    InvalidRotation(VideoRotation),
    #[error("backend error: {0}")]
    BackendError(String),
}

pub type MediaResult<T> = Result<T, MediaError>;
```

Uses `thiserror` (aligns with OMSPBase conventions; webrtc-kit used manual impls). Import style matches existing crates: `use thiserror::Error; #[derive(Error, Debug)]`.

## Testing Strategy

Target: ≥80% line coverage.

| Category | Approach | Tools |
|----------|----------|-------|
| **Buffer round-trip** | Each format → `to_i420()` → verify dimensions + planes | std test |
| **Transform correctness** | Pure-color I420 frame → scale/mirror/crop/rotate → assert expected pixel values | std test |
| **Backend consistency** | Same input through both backends → output within ±1 pixel (rounding tolerance) | conditional feature test |
| **Known values** | Hand-crafted 4×4 I420 frames with known patterns, verify output pixel-for-pixel | param test |
| **Color convert RT** | RGBA → I420 → RGBA → verify round-trip loss ≤ 1 per channel | std test |
| **Broadcaster** | 3 sinks with different wants → verify aggregation, verify all receive frames | std test |
| **Generator** | Create generator, attach sink, verify frames arrive at expected rate | std test |

## Excluded from Scope

- Audio source/sink (AudioSource, AudioSink, DefaultAudioSource) — deferred
- Camera capture (V4L2, AVFoundation, PipeWire)
- Encoder/decoder interfaces (VideoEncoder, VideoDecoder)
- Network transport (RTP/RTCP)
- Platform-specific GPU textures (DXGI, Metal, Vulkan)
- WASM support (current `#[cfg(target_arch = "wasm32")]` branches removed)
- GStreamer integration
- Buffer pool / allocator — deferred

## Design Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | Independent crate, no `omspbase-core` dependency | Self-contained video pipeline; OMSPBase-core types are a different abstraction layer |
| D2 | `I420BufferRef` borrowed view for backend traits | Zero-copy, lifetime-checked plane access; avoids owned-buffer allocation in transform hot path |
| D3 | `I420` as canonical center format | Industry standard; all conversion converges to I420; transforms operate on I420 |
| D4 | Feature flags `backend-libyuv-sys` / `backend-native` | Matches `omspbase-webrtc` naming convention; compile-time backend selection |
| D5 | `VideoTransform` as merged trait (spatial + color) | Both always needed together; returned as single backend; simplifies feature-gating |
| D6 | Batch operations on slices, caller pre-allocates | Zero internal allocation in transform hot path; matches libyuv C API pattern |
| D7 | `VideoSinkWants` backpressure + dynamic `on_frame` return | Enables downstream to signal changing resolution/framerate constraints frame-by-frame |
| D8 | `thiserror` for MediaError | Aligns with all existing OMSPBase crates; replaces webrtc-kit manual Display+Error impl |
| D9 | No `serde` dependency in media crate | Frame buffers are runtime data; serialization belongs at application/codec layer |
| D10 | `as_i420_ref()` instead of buffer-level `scale()` | Decouples buffer from scaling algorithm; backend handles all transforms |
| D11 | `SinkId` for sink lifecycle (not `&Box<dyn Sink>`) | Type-safe removal; avoids fragile pointer comparison |
| D12 | Broadcaster uses `Arc<F>` internally | `Box<dyn VideoBuffer>` is not `Clone`; `Arc<F>` enables zero-copy broadcast |
| D13 | Unified `PixelFormat` enum (RGB + YUV) | Follows OpenCTK's `VideoType` approach; simpler than two separate enums |
| D14 | `I420BufferRef` is `Copy`, passed by value | All fields are `&[u8]` + `usize` (all `Copy`); avoids double-indirection (`&&`) |
| D15 | `compile_error!` guard for zero backends | Matches `omspbase-webrtc` convention; clear error message vs confusing missing-function |
| D16 | `VideoFrameGenerator` retained for P2P testing | Needed for early P2P push/pull streaming tests without camera hardware |
