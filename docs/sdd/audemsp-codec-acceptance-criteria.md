# SDD: omspbase-codec Acceptance Criteria

**Status:** Draft | **Phase:** pre-implementation | **Generated:** 2026-07-22
**References:** ITU-T H.264 (06/2019), FFmpeg libavcodec docs, x264 presets, ITU-T J.247 (PSNR/SSIM), RFC 6184 (H.264 RTP), ISO 14496-10

---

## Context

`omspbase-codec` is a planned workspace crate providing a unified encode/decode API over GStreamer (Host) and statically-linked FFmpeg (Remote). The crate must produce byte-compatible H.264 Annex‑B streams from both backends, meet realtime performance thresholds, and never dynamically load a codec at runtime (Remote path).

This document defines **concrete pass/fail acceptance criteria** for each spec axis. Every criterion is testable via automated integration tests in CI.

---

## 1. Roundtrip Fidelity (I420 → H.264 → I420)

**Goal:** An I420 frame encoded then immediately decoded must be perceptually identical to the source.

### 1.1 PSNR Threshold

| Resolution | PSNR-Y (min) | PSNR-U (min) | PSNR-V (min) | Preset |
|-----------|-------------|-------------|-------------|--------|
| 640×480   | ≥ 38 dB     | ≥ 40 dB     | ≥ 40 dB     | P1 (ultrafast) |
| 640×480   | ≥ 42 dB     | ≥ 44 dB     | ≥ 44 dB     | P4 (medium) |
| 1280×720  | ≥ 37 dB     | ≥ 39 dB     | ≥ 39 dB     | P4 (medium) |
| 1920×1080 | ≥ 36 dB     | ≥ 38 dB     | ≥ 38 dB     | P4 (medium) |

**Test method:**
```
1. Generate synthetic I420 test frame (color bars, gradient ramp, 1px checkerboard)
2. encoder.push_frame(frame) → encoder.pull_packet() → H.264 bitstream
3. decoder.push_packet(bitstream) → decoder.pull_frame() → I420 frame
4. Compute PSNR-Y, PSNR-U, PSNR-V between source and decoded
Pass: ALL channels ≥ threshold for 100 consecutive frames
```

**Reference:** ITU-T J.247 §8.1; ≥35 dB PSNR is "good quality" threshold.

### 1.2 SSIM Threshold

| Resolution | SSIM (min) | Preset |
|-----------|-----------|--------|
| 640×480   | ≥ 0.98    | P4 (medium) |
| 1280×720  | ≥ 0.97    | P4 (medium) |

**Test method:** Multi-scale SSIM (Wang et al. 2003) between source and decoded I420.

### 1.3 Pixel-Perfect for Lossless Mode

When `preset = P7 (lossless)` and `bitrate_kbps = 0`:
- Every decoded pixel MUST equal the source pixel (byte-identical Y, U, V planes).
- **Pass:** 100 consecutive frames, zero differing bytes.

---

## 2. Performance

### 2.1 Encode Latency (per-frame wall clock)

| Resolution | Preset | Max encode_latency_ms | Target fps |
|-----------|--------|---------------------|-----------|
| 640×480   | P1     | ≤ 5 ms              | 60        |
| 1280×720  | P4     | ≤ 10 ms             | 30        |
| 1920×1080 | P4     | ≤ 20 ms             | 30        |

**Test method:**
```
1. Feed synthetic I420 frames at target fps
2. Measure per-frame wall-clock: frame_submit → frame_encoded callback
3. Compute p50, p95, p99 latencies over 500 frames
Pass: p99 ≤ threshold
```

**Rationale:** x264 ultrafast typically achieves < 3ms at 720p on modern HW. GStreamer HW encoders (V4L2/VAAPI/VideoToolbox) can be sub-2ms.

### 2.2 Throughput

| Resolution | Min fps (sustained) | Bitrate |
|-----------|-------------------|---------|
| 640×480   | ≥ 60              | 1000 kbps |
| 1280×720  | ≥ 30              | 2000 kbps |
| 1920×1080 | ≥ 30              | 4000 kbps |

**Test method:**
```
1. Feed 600 frames at unlimited rate
2. Count encoder output frames in 10-second window
Pass: output_fps ≥ target_fps — 5% (allows for scheduling jitter)
```

### 2.3 Decode Latency

| Resolution | Max decode_latency_ms | Backend |
|-----------|---------------------|---------|
| 1280×720  | ≤ 8 ms              | GStreamer |
| 1280×720  | ≤ 8 ms              | FFmpeg (built-in H.264 decoder) |

**Test method:** Same as encode latency, on the decode path.

---

## 3. Correctness

### 3.1 Keyframe Interval

Given `gop = N` in `EncodeConfig`:
- The encoder MUST produce an IDR keyframe every `N` frames (±1 frame tolerance).
- `gop = 0` → no forced keyframes (encoder discretion).
- `gop = 1` → all frames are IDR keyframes.

**Test method:**
```
Pass: for each run (N ∈ {1, 30, 60, 120}):
  idr_spacing = max diff between consecutive IDR pts
  assert idr_spacing ≤ N + 1
```

### 3.2 SPS/PPS in IDR

Every IDR access unit MUST contain SPS and PPS NAL units **preceding** the IDR slice NAL unit.

**Test method:**
```
1. Parse H.264 bitstream (annex-b NAL unit parser)
2. For each access unit where primary_pic_type is IDR:
   - assert at least one NAL type 7 (SPS) in AU
   - assert at least one NAL type 8 (PPS) in AU
   - assert SPS/PPS appear before NAL type 5 (IDR slice)
Pass: all IDR AUs satisfy constraints
```

**Reference:** ITU-T H.264 §7.4.1.2.3, RFC 6184 §5.2.

### 3.3 Valid Annex‑B Bitstream

The encoder output MUST be a valid Annex‑B byte stream:
- NAL units delimited by `0x00 0x00 0x00 0x01` (4-byte start code) or `0x00 0x00 0x01` (3-byte).
- No emulation prevention bytes (`0x03`) outside NAL unit bodies.
- First NAL unit in stream: SPS (type 7), then PPS (type 8), then IDR (type 5).

**Test method:**
```
Pass: raw bitstream passes FFmpeg h264_mp4toannexb BSF roundtrip without error
  ffmpeg -i annexb.h264 -c copy -bsf h264_mp4toannexb -f null -
```

### 3.4 AVCC Output Mode (optional feature flag)

When `output_format = "avcc"`:
- SPS/PPS are emitted as `avcC` extradata (ISO 14496-15 §5.2.4.1).
- NAL units use 4-byte length prefix (not start codes).
- The `avcC` box is valid per ISO 14496-15.

**Test method:**
```
Pass: extract avcC bytes, parse with FFmpeg av_bsf_get_by_name("h264_mp4toannexb")
```

---

## 4. Backend Consistency

### 4.1 Bit-Exact Output

GStreamer (software x264enc) and FFmpeg (libx264) streams are NOT required to be byte-identical (different library versions, threading models). Instead:

**Requirement:** For identical `EncodeConfig` and input frames, GStreamer and FFmpeg backends must produce bitstreams whose **decoded I420 frames are bit-identical** (zero differing bytes across all YUV planes).

**Test method:**
```
1. Encode identical test sequence via GStreamer backend
2. Encode identical test sequence via FFmpeg backend
3. Decode both with reference decoder (FFmpeg avcodec built-in)
4. Compare decoded I420 planes byte-by-byte
Pass: max pixel diff = 0 for P7 (lossless), ±1 for P4 (medium)
```

### 4.2 Tolerance for P4 (medium)

At P4, allow ±1 in pixel values due to floating-point rounding differences between backends.

---

## 5. Error Handling

### 5.1 Invalid Input

| Input | Expected Behavior |
|-------|-----------------|
| `width = 0` or `height = 0` | `EncodeError::InvalidDimension` |
| `width % 2 != 0` or `height % 2 != 0` | `EncodeError::InvalidDimension` (I420 requires even dimensions) |
| `bitrate_kbps = 0` with `preset != P7` | `EncodeError::InvalidConfig` |
| `fps = 0` | `EncodeError::InvalidConfig` |
| I420 buffer size < `width × height × 3/2` | `EncodeError::BufferUnderflow` |
| Empty I420 buffer | `EncodeError::BufferUnderflow` |

### 5.2 Encoder Reset

**Requirement:** After calling `encoder.reset(new_config)`, the next encoded frame MUST be an IDR keyframe, and the encoder MUST accept frames without error.

**Test method:**
```
1. encode 30 frames with config A (gop=30)
2. encoder.reset(config_B)  (gop=60, different resolution)
3. encode 60 frames with config B
Pass:
  - Frame 31 (first after reset) is IDR
  - Frame 31 SPS/PPS present
  - No errors in frames 31-90
```

### 5.3 OOM Recovery

When the encoder exhausts an internal buffer (e.g., out-of-order returns):
- `encode()` returns `EncodeError::Overload` (not panic).
- After returning `Overload`, the next `encode()` call MUST succeed (encoder is recoverable).

### 5.4 Panic-Free

No encoder or decoder function may panic on ANY input. All error paths return `Result::Err`.

**Test method:** Fuzz test — random `&[u8]` input to `decoder.decode()` for 10 seconds. Zero panics.

---

## 6. Thread Safety

### 6.1 Encode: `Send + Sync`

The encoder type must implement `Send + Sync`.

### 6.2 Concurrent Encode (send-only safety)

| Scenario | Result |
|---------|--------|
| Single encoder, 4 threads calling `encode()` concurrently | No data race, no crash. At least one encode succeeds per call. Internal queue is unbounded or bounded with backpressure. |
| Two encoders (separate instances), each called from different threads | Both produce correct output independently. |

**Test method (single encoder, concurrent):**
```
1. Create 1 encoder
2. Spawn 4 threads, each calling encode() 100 times in a loop
3. Wait for all threads
Pass: total encoded frames ≥ 400, no panic, no deadlock, no lost frames
```

**Note:** The tester MUST verify with `cargo test --test thread_safety_codec` under TSAN (`RUSTFLAGS="-Zsanitizer=thread"` on nightly). TSAN MUST report zero races.

### 6.3 Decode Thread Safety

Decoder must be `Send` (not necessarily `Sync` — decoder can be owned by one thread with shared reference for config queries).

---

## 7. Static Linking Verification (Remote/FFmpeg backend)

### 7.1 No Runtime dlopen

With `backend-ffmpeg` feature enabled:
- The compiled binary MUST NOT contain any call to `dlopen`, `dlsym`, `dlclose`, `LoadLibrary`, `GetProcAddress`.
- All FFmpeg symbols are resolved at link time (static `.a` libraries).

**Test method:**
```
1. cargo build --release --features backend-ffmpeg
2. Verify with:
   nm target/release/omspbase-client | grep " U " | grep -E "avcodec_|avformat_|avutil_|sws_"
   # Should be empty — all FFmpeg symbols resolved (no Undefined)
3. Verify no dlopen:
   strings target/release/omspbase-client | grep -E "dlopen|dlclose|dlsym"
   # Should be empty
4. Check with objdump -T | grep UND for dynamic FFmpeg symbols
```

### 7.2 Build-Time Feature Probe

`build.rs` MUST compile a short C probe that calls `avcodec_find_decoder(AV_CODEC_ID_H264)`. If the probe fails to compile or link, the build MUST fail with a clear error message.

### 7.3 Binary Size

With `backend-ffmpeg` (minimal decode-only FFmpeg, `--enable-small`, LTO):

| Platform | Max binary size |
|---------|---------------|
| Linux x86_64 | ≤ 12 MB |
| macOS arm64 | ≤ 12 MB |
| Windows MSVC | ≤ 14 MB |

---

## 8. Reference Compliance

### 8.1 ITU-T H.264 (AVC) Profile/Level

| Resolution@fps | Profile | Level | Max DPB frames |
|---------------|---------|-------|---------------|
| 640×480@60    | Constrained Baseline | 3.1 | 5 |
| 1280×720@30   | Constrained Baseline (or Main) | 3.1 | 5 |
| 1920×1080@30  | High | 4.0 | 4 |

**Test method:**
```
1. Encode 100 frames
2. Parse SPS (NAL type 7)
3. Assert profile_idc, level_idc match target
4. Assert constraint_set0_flag == 1 for Constrained Baseline
Pass: all SPS NAL units match expected profile/level
```

### 8.2 x264 Presets (Reference for Quality Baselines)

The PSNR/SSIM thresholds in §1 are calibrated against x264 software encoder at the stated presets (libx264 commit `5db6aa6` or later). Hardware encoders (VAAPI, VideoToolbox, NVENC) may trade quality for speed — their PSNR thresholds are lowered by 2 dB across the board.

| Backend | PSNR adjustment |
|---------|----------------|
| x264 (software, FFmpeg libx264) | Baseline (thresholds in §1) |
| VideoToolbox (macOS HW) | -2 dB tolerance |
| VAAPI (Linux HW) | -2 dB tolerance |
| NVENC (NVIDIA HW) | -1 dB tolerance |

### 8.3 Emulation Prevention

The encoder MUST insert `0x03` emulation prevention bytes as defined in ITU-T H.264 §7.4.1 "NAL unit semantics."

**Test method:**
```
Pass: 100-frame output stream verified by FFmpeg h264_parse tool with emulation prevention check:
  ffmpeg -i output.h264 -c copy -f null - 2>&1 | grep -i "emulation"
  # Should be empty (no warnings)
```

---

## 9. API Surface

### 9.1 Encoder Trait

```rust
pub trait VideoEncoder: Send {
    /// Configure the encoder. Called once before first push_frame().
    fn configure(&mut self, config: &EncodeConfig) -> Result<(), CodecError>;

    /// Push a raw I420 frame into the encoder.
    /// Follow with pull_packet() calls until None.
    fn push_frame(&mut self, frame: &I420BufferRef) -> Result<(), CodecError>;

    /// Pull the next encoded packet (Annex-B H.264 NAL unit).
    /// Returns None when encoder needs more input (call push_frame again).
    fn pull_packet(&mut self) -> Result<Option<Vec<u8>>, CodecError>;

    /// Signal end-of-stream. Flushes all buffered packets.
    /// After flush(), call pull_packet() in a loop until None.
    fn flush(&mut self) -> Result<(), CodecError>;

    /// Force a keyframe on the next push_frame() call.
    fn request_keyframe(&mut self) -> Result<(), CodecError>;

    /// Reset the encoder state without reallocation.
    /// Next encoded frame MUST be an IDR.
    fn reset(&mut self, config: &EncodeConfig) -> Result<(), CodecError>;

    /// Return live encoder statistics.
    fn stats(&self) -> EncodeStats;
}

// Convenience wrapper for single-frame encode (non-realtime use).
pub fn encode_simple(encoder: &mut dyn VideoEncoder, frame: &I420BufferRef) -> Result<Vec<u8>, CodecError> {
    encoder.push_frame(frame)?;
    let mut packets = vec![];
    while let Some(pkt) = encoder.pull_packet()? { packets.push(pkt); }
    encoder.flush()?;
    while let Some(pkt) = encoder.pull_packet()? { packets.push(pkt); }
    Ok(packets.concat())
}

### 9.2 Decoder Trait

```rust
pub trait VideoDecoder: Send {
    /// Configure the decoder.
    fn configure(&mut self, config: &DecodeConfig) -> Result<(), CodecError>;

    /// Push an encoded H.264 bitstream fragment into the decoder.
    /// Follow with pull_frame() calls until None.
    fn push_packet(&mut self, data: &[u8]) -> Result<(), CodecError>;

    /// Pull the next decoded I420 frame.
    /// Returns None when decoder needs more input (call push_packet again).
    fn pull_frame(&mut self) -> Result<Option<I420Frame>, CodecError>;

    /// Flush decoder. Call pull_frame() in a loop after flush.
    fn flush(&mut self) -> Result<(), CodecError>;

    /// Reset decoder state.
    fn reset(&mut self) -> Result<(), CodecError>;
}
```

### 9.3 Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("invalid encoder configuration: {0}")]
    InvalidConfig(String),
    #[error("invalid dimensions: width={0}, height={1}")]
    InvalidDimension(u32, u32),
    #[error("input buffer too small: expected={expected}, got={got}")]
    BufferUnderflow { expected: usize, got: usize },
    #[error("encoder overloaded, retry after backpressure")]
    Overload,
    #[error("hardware encoder unavailable: {0}")]
    HwUnavailable(String),
    #[error("internal codec error: {0}")]
    Internal(String),
    #[error("out of memory")]
    OutOfMemory,
}
```

---

## 10. Test Matrix

| Criterion ID | Type | Target | Backend |
|-------------|------|--------|---------|
| AC-RT-01 | Integration | PSNR ≥ 42 dB @ 640×480 P4 | GStreamer, FFmpeg |
| AC-RT-02 | Integration | SSIM ≥ 0.98 @ 640×480 P4 | GStreamer, FFmpeg |
| AC-RT-03 | Integration | Byte-identical P7 roundtrip | GStreamer, FFmpeg |
| AC-PERF-01 | Benchmark | encode p99 ≤ 10ms @ 720p P4 | GStreamer, FFmpeg |
| AC-PERF-02 | Benchmark | throughput ≥ 30 fps @ 720p | GStreamer, FFmpeg |
| AC-CORR-01 | Unit | keyframe interval = gop | GStreamer, FFmpeg |
| AC-CORR-02 | Unit | SPS/PPS before IDR | GStreamer, FFmpeg |
| AC-CORR-03 | Unit | valid Annex‑B (ffmpeg parse) | GStreamer, FFmpeg |
| AC-CONS-01 | Integration | decoded pixel diff ≤ 1 (P4) across backends | GStreamer vs FFmpeg |
| AC-ERR-01 | Unit | error on width=0 | shared |
| AC-ERR-02 | Unit | error on odd dimensions | shared |
| AC-ERR-03 | Integration | reset → next frame is IDR | GStreamer, FFmpeg |
| AC-ERR-04 | Fuzz | zero panics with random input | shared |
| AC-TH-01 | Concurrency (TSAN) | 4-thread concurrent encode, no race | GStreamer, FFmpeg |
| AC-TH-02 | Concurrency | dual-encoder independence | GStreamer, FFmpeg |
| AC-LINK-01 | Static | nm: no undefined FFmpeg symbols | FFmpeg only |
| AC-LINK-02 | Build | build.rs probe compiles | FFmpeg only |
| AC-LINK-03 | Size | binary ≤ 14 MB | FFmpeg only |
| AC-REF-01 | Unit | profile/level match config | GStreamer, FFmpeg |
| AC-REF-02 | Unit | emulation prevention correct | GStreamer, FFmpeg |

---

## 11. Non-Requirements (explicitly out of scope)

- **Audio codec** support (OPUS, AAC) — belongs to `omspbase-webrtc::rtp` layer.
- **H.265/HEVC** — separate SDD; this is H.264-only.
- **Dynamic codec negotiation** — this is SDP/WebRTC layer concern.
- **Bitrate adaptation (GCC/REMB)** — belongs to transport layer.
- **Simulcast/SVC** — Phase 3.
- **NAL unit rewriting/filtering** — not in this crate.

---

## 12. References

| Reference | Description |
|----------|------------|
| ITU-T H.264 (06/2019) | Advanced Video Coding specification |
| ITU-T J.247 (08/2008) | Objective perceptual video quality measurement |
| RFC 6184 | RTP Payload Format for H.264 Video |
| ISO 14496-10 | MPEG-4 Part 10: Advanced Video Coding |
| ISO 14496-15 §5.2.4.1 | AVC file format (avcC box) |
| x264 5db6aa6 | Reference software encoder (P1-P7 presets) |
| FFmpeg libavcodec 7.x | H.264 decoder/parser/BSF |
| `docs/reference/ffmpeg-static-build-strategy.md` | Project build strategy for FFmpeg |
| `docs/sdd/02-webrtc-push.md` | EncodeConfig schema origin |
| `docs/superpowers/specs/2026-07-20-omspbase-media-crate-design.md` | Base types and traits |
