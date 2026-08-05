//! VideoAdapter — 源适配组件（丢帧 + 分辨率适配）。
//!
//! 消费方：非 SFU 本地 sinks（录制/预览/webrtc-rs 后端）——SFU 生产路径下
//! libwebrtc 后端内部已做 AdaptFrame，此组件不参与（PIT-62 审核结论）。
//!
//! 设计对照 OpenCTK `video_adapter.hpp`：
//! - 丢帧决策委托 [`FramerateController`]（VFR→CFR）
//! - 分辨率适配：保持宽高比缩放到目标分辨率内
//!   （完整 crop-then-scale 需 buffer 层 crop 支持——当前提供 scale 计算，TODO）

use super::framerate_controller::FramerateController;

/// 适配计数（可观测性——对应 OpenCTK video_adaptation_counters）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AdaptationCounters {
    /// 因帧率节流丢弃的帧数。
    pub frames_dropped: u64,
    /// 因分辨率适配缩放的帧数。
    pub frames_scaled: u64,
}

/// 源适配器：按消费者需求（帧率上限 + 分辨率上限）适配帧。
#[derive(Debug, Clone)]
pub struct VideoAdapter {
    framerate_controller: FramerateController,
    /// 目标最大分辨率（保持宽高比缩放）；None = 不缩放。
    scale_resolution_down_to: Option<(u32, u32)>,
    counters: AdaptationCounters,
}

impl Default for VideoAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoAdapter {
    pub fn new() -> Self {
        Self {
            framerate_controller: FramerateController::new(),
            scale_resolution_down_to: None,
            counters: AdaptationCounters::default(),
        }
    }

    /// 设置帧率上限（VFR→CFR 节流）。
    pub fn set_max_framerate(&mut self, fps: f64) {
        self.framerate_controller.set_max_framerate(fps);
    }

    /// 设置最大分辨率（保持宽高比缩放，None 禁用）。
    pub fn set_scale_resolution_down_to(&mut self, max: Option<(u32, u32)>) {
        self.scale_resolution_down_to = max;
    }

    /// 丢帧决策：返回 true = 该帧应丢弃（帧率节流）。
    pub fn should_drop_frame(&mut self, ts_ns: i64) -> bool {
        let drop = self.framerate_controller.should_drop_frame(ts_ns);
        if drop {
            self.counters.frames_dropped += 1;
        }
        drop
    }

    /// 分辨率适配：返回缩放后的 (w, h)；None = 无需缩放。
    ///
    /// 保持宽高比缩放到 `scale_resolution_down_to` 内。
    /// TODO(crop): 完整 crop-then-scale 需 buffer 层 crop 支持（OpenCTK adaptFrameResolution）。
    pub fn adapt_resolution(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let (max_w, max_h) = self.scale_resolution_down_to?;
        if width <= max_w && height <= max_h {
            return None;
        }
        // 保持宽高比：取两个缩放因子中的较小值
        let scale_w = max_w as f64 / width as f64;
        let scale_h = max_h as f64 / height as f64;
        let scale = scale_w.min(scale_h);
        let out_w = ((width as f64 * scale).round() as u32).max(2);
        let out_h = ((height as f64 * scale).round() as u32).max(2);
        if out_w == width && out_h == height {
            None
        } else {
            self.counters.frames_scaled += 1;
            Some((out_w, out_h))
        }
    }

    /// 当前适配计数（调试/可观测性）。
    pub fn counters(&self) -> AdaptationCounters {
        self.counters
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_frame_throttles_by_framerate() {
        let mut a = VideoAdapter::new();
        a.set_max_framerate(30.0);
        assert!(!a.should_drop_frame(0), "首帧保留");
        assert!(a.should_drop_frame(10_000_000), "提前到达丢弃");
        assert!(!a.should_drop_frame(33_333_333), "输出时刻保留");
    }

    #[test]
    fn adapt_resolution_scales_down_preserving_aspect() {
        let mut a = VideoAdapter::new();
        // 不设置 → 不缩放
        assert_eq!(a.adapt_resolution(640, 480), None);

        let mut a = VideoAdapter::new();
        a.set_scale_resolution_down_to(Some((320, 240)));
        // 640x480 → 320x240 (等比)
        assert_eq!(a.adapt_resolution(640, 480), Some((320, 240)));
        // 1280x720 → 320x180 (16:9 等比)
        assert_eq!(a.adapt_resolution(1280, 720), Some((320, 180)));
        // 已小于目标 → 不缩放
        assert_eq!(a.adapt_resolution(160, 120), None);
        // 单边超限 → 按较小因子缩放 (100x400 → 60x240, 保 1:4)
        assert_eq!(a.adapt_resolution(100, 400), Some((60, 240)));
    }

    #[test]
    fn adapt_resolution_handles_non_square_targets() {
        let mut a = VideoAdapter::new();
        a.set_scale_resolution_down_to(Some((640, 360))); // 16:9 目标
        assert_eq!(a.adapt_resolution(1920, 1080), Some((640, 360)));
    }
}

    #[test]
    fn counters_track_drops_and_scales() {
        let mut a = VideoAdapter::new();
        a.set_max_framerate(30.0);
        assert!(!a.should_drop_frame(0));
        assert!(a.should_drop_frame(10_000_000)); // drop
        assert_eq!(a.counters().frames_dropped, 1);

        a.set_scale_resolution_down_to(Some((320, 240)));
        assert_eq!(a.adapt_resolution(640, 480), Some((320, 240))); // scale
        assert_eq!(a.counters().frames_scaled, 1);
        assert_eq!(a.adapt_resolution(160, 120), None); // no scale
        assert_eq!(a.counters().frames_scaled, 1);
    }
