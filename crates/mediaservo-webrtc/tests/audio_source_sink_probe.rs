//! PIT-105 证据探针（H2 I4 review 提交）: AudioTrackSource → capture_frame → AudioTrack
//! sink 交付链 — 源侧接入实证。
//!
//! 背景（PIT-105）: 音频发送链路五段中，本探针证明"源→轨道 sink"段工作
//! （capture_frame 成功 + sink 回调 70 次/20 帧 + 静音填充）; 丢失点在 libwebrtc
//! 音频发送通道（LocalAudioSinkAdapter 挂载 / channel StartSend 状态）— vendor 域。
//! `#[ignore]`: 探针不依赖 PIT-105 修复（sink 链本来就通），但保持低频运行
//! （FFI 探针语义 + 修复验证时的判别式）。

// 仅 backend-webrtc-sys 有 webrtc_sys 依赖（default = stub 后端, 无 FFI 面）—
// 无此门 default 构建编译失败（I1 re-review blocker）。
#![cfg(feature = "backend-webrtc-sys")]
#![cfg(target_os = "linux")]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

extern "C" fn noop_complete(_ctx: *const webrtc_sys::audio_track::SourceContext) {}

struct Counter(Arc<AtomicU64>);

impl webrtc_sys::audio_track::AudioSink for Counter {
    fn on_data(
        &self,
        _data: &[i16],
        _sample_rate: i32,
        _nb_channels: usize,
        _nb_frames: usize,
    ) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

/// capture_frame 推入的 PCM 必须到达 AudioTrack 的 sink（源→轨道链证据）。
/// 修复 PIT-105 时保留: 若此测试失败 = 源侧接线回归; 若通过但 e2e byte_count=0
/// = 丢失点仍在 libwebrtc 发送通道。
#[test]
#[ignore = "PIT-105 证据探针: 低频运行; 依赖 webrtc-sys FFI 运行时"]
fn audio_source_sink_probe() {
    use webrtc_sys::audio_track::ffi as at;

    let factory = webrtc_sys::peer_connection_factory::ffi::create_peer_connection_factory();
    let source = at::new_audio_track_source(
        at::AudioSourceOptions {
            echo_cancellation: false,
            noise_suppression: false,
            auto_gain_control: false,
        },
        48000,
        1,
        100,
    );
    let track = factory.create_audio_track("probe".into(), source.clone());
    let counter = Arc::new(AtomicU64::new(0));
    let sink = at::new_native_audio_sink(
        Box::new(webrtc_sys::audio_track::AudioSinkWrapper::new(Arc::new(Counter(
            counter.clone(),
        )))),
        48000,
        1,
    );
    track.add_sink(&sink);

    // 推 20 帧 10ms PCM（模拟 tone 节奏）
    let pcm: Vec<i16> = vec![0i16; 480];
    for i in 0..20 {
        let ok = unsafe {
            source.capture_frame(
                &pcm,
                48000,
                1,
                480,
                std::ptr::null(),
                webrtc_sys::audio_track::CompleteCallback(noop_complete),
            )
        };
        assert!(ok, "capture_frame[{i}] rejected");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // 等 queue 任务排空
    std::thread::sleep(std::time::Duration::from_millis(500));
    let n = counter.load(Ordering::Relaxed);
    eprintln!("audio_source_sink_probe: sink callbacks = {n}");
    assert!(n > 0, "capture_frame 的数据必须到达 track sink（源→轨道线断裂）");
}
