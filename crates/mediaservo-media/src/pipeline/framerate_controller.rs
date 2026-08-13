//! FramerateController — VFR → CFR 帧率节流（移植 OpenCTK / libwebrtc）。
//!
//! 相机输出可变帧率（VFR），编码器需要固定帧率（CFR）。控制器按帧时间戳
//! （纳秒，如 V4L2 buffer timestamp）决策丢帧，保证输出节奏 ≤ max_framerate。
//!
//! 语义对照 OpenCTK `framerate_controller.cpp`：
//! 1. `kMinFramerate = 0.5` — max_framerate 低于该值 → 全丢
//! 2. `frame_interval_ns = 1e9 / fps` 整数除法；`<= 0`（fps 极大）→ 节流禁用
//! 3. 重置分支：`abs(time_until) >= 2×interval` → 重置为 `ts + interval/2`
//!    （首帧/时间戳大幅跳变时宽容处理，优先保留帧）
//! 4. 双路径推进：`should_drop_frame` 在"输出时刻"分支推进 next；
//!    `keep_frame` 对 drop 分支再推进（调用方强制保留一帧，节奏前进一帧）
//!
//! 时间戳单位：**纳秒**（与 V4L2 buffer timestamp 一致；µs 值由调用方 ×1000）。

const K_MIN_FRAMERATE: f64 = 0.5;
const NSECS_PER_SEC: i64 = 1_000_000_000;

/// 帧率节流控制器（VFR→CFR）。
#[derive(Debug, Clone)]
pub struct FramerateController {
    max_framerate: f64,
    next_frame_timestamp_ns: Option<i64>,
}

impl Default for FramerateController {
    fn default() -> Self {
        Self::new()
    }
}

impl FramerateController {
    /// 创建控制器，默认不节流（max_framerate = f64::MAX）。
    pub fn new() -> Self {
        Self {
            max_framerate: f64::MAX,
            next_frame_timestamp_ns: None,
        }
    }

    /// 创建控制器并设置最大帧率。
    pub fn with_max_framerate(fps: f64) -> Self {
        Self {
            max_framerate: fps,
            next_frame_timestamp_ns: None,
        }
    }

    pub fn set_max_framerate(&mut self, fps: f64) {
        self.max_framerate = fps;
    }

    pub fn max_framerate(&self) -> f64 {
        self.max_framerate
    }

    /// 重置为初始状态（不节流 + 清空节奏）。
    pub fn reset(&mut self) {
        self.max_framerate = f64::MAX;
        self.next_frame_timestamp_ns = None;
    }

    /// 帧率节流决策：返回 true = 该帧应丢弃。
    ///
    /// 副作用：输出时刻分支 / 首次 / 重置分支会推进 `next_frame_timestamp_ns`。
    pub fn should_drop_frame(&mut self, ts_ns: i64) -> bool {
        if self.max_framerate < K_MIN_FRAMERATE {
            return true;
        }
        let frame_interval_ns = NSECS_PER_SEC / self.max_framerate as i64;
        if frame_interval_ns <= 0 {
            return false; // 节流未启用（fps 极大 → interval 截断为 0）
        }

        if let Some(next) = self.next_frame_timestamp_ns {
            let time_until_next_frame_ns = next - ts_ns;
            // 在预期范围内（±2×interval）→ 正常节流决策
            if time_until_next_frame_ns.abs() < 2 * frame_interval_ns {
                if time_until_next_frame_ns > 0 {
                    return true; // 帧提前到达，丢
                }
                // 输出时刻：节奏前进一帧
                self.next_frame_timestamp_ns = Some(next + frame_interval_ns);
                return false;
            }
            // 时间戳大幅跳变 → 落到下方重置分支
        }

        // 首帧 / 越界重置：目标设为 ts + interval/2（抖动时优先保留帧）
        self.next_frame_timestamp_ns = Some(ts_ns + frame_interval_ns / 2);
        false
    }

    /// 强制保留本帧（调用方需求，如该帧是必要的参考帧）。
    ///
    /// 内部先做丢帧决策；若判为丢（drop 分支，next 未推进），则推进节奏一帧
    /// ——保证后续帧按新节奏输出（与 OpenCTK 双路径推进一致）。
    pub fn keep_frame(&mut self, ts_ns: i64) {
        let dropped = self.should_drop_frame(ts_ns);
        if !dropped {
            return;
        }
        // drop 路径：本帧被保留，节奏前进一帧
        if self.max_framerate < K_MIN_FRAMERATE {
            return;
        }
        let frame_interval_ns = NSECS_PER_SEC / self.max_framerate as i64;
        if frame_interval_ns <= 0 {
            return;
        }
        if let Some(next) = self.next_frame_timestamp_ns {
            self.next_frame_timestamp_ns = Some(next + frame_interval_ns);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL_30FPS_NS: i64 = 33_333_333; // 1e9/30

    #[test]
    fn keep_path_outputs_frames_at_max_rate() {
        // 30fps 节流下，33ms 间隔的帧全部保留（首帧重置 → 后续输出时刻推进）
        let mut c = FramerateController::with_max_framerate(30.0);
        let mut ts = 0i64;
        let mut kept = 0;
        for _ in 0..10 {
            if !c.should_drop_frame(ts) {
                kept += 1;
            }
            ts += INTERVAL_30FPS_NS;
        }
        // 首帧重置保留 + 之后每帧到达输出时刻 → 全部保留
        assert_eq!(kept, 10, "33ms 间隔 (30fps) 应全部保留");
    }

    #[test]
    fn drop_path_drops_early_frames() {
        // 帧提前到达（早于 next 输出时刻）→ 丢弃
        let mut c = FramerateController::with_max_framerate(30.0);
        // 首帧重置: next = 0 + 16.67ms
        assert!(!c.should_drop_frame(0));
        // 下一帧 10ms 到达（早于 next 16.67ms）→ time_until > 0 → 丢
        assert!(c.should_drop_frame(10_000_000), "早于输出时刻应丢弃");
    }

    #[test]
    fn reset_path_tolerates_timestamp_jump() {
        // 时间戳大幅跳变（> 2×interval）→ 重置而非连续丢帧
        let mut c = FramerateController::with_max_framerate(30.0);
        assert!(!c.should_drop_frame(0));
        // 跳变 1s（远大于 2×33ms）→ 重置，不丢
        assert!(!c.should_drop_frame(1_000_000_000), "大幅跳变应重置而非丢帧");
        // 重置后节奏恢复：下一帧提前到达 → 丢
        assert!(c.should_drop_frame(1_000_000_000 + 10_000_000));
    }

    #[test]
    fn keep_frame_advances_rhythm_on_drop_path() {
        // keep_frame：drop 分支强制保留并推进节奏
        let mut c = FramerateController::with_max_framerate(30.0);
        c.keep_frame(0); // 首帧保留（重置）
        c.keep_frame(20_000_000); // 提前帧：判丢但强制保留 → 推进
        // 推进后，下一个输出时刻 = 20ms + interval ≈ 53ms
        // 53ms 处到达 → 输出时刻分支（推进）→ 保留
        assert!(!c.should_drop_frame(53_333_333), "keep 推进后 53ms 应输出");
    }

    #[test]
    fn min_framerate_drops_everything() {
        let mut c = FramerateController::with_max_framerate(0.1); // < 0.5
        assert!(c.should_drop_frame(0));
        assert!(c.should_drop_frame(1_000_000_000));
    }

    #[test]
    fn default_disables_throttling() {
        let mut c = FramerateController::new();
        // max_framerate = MAX → interval 截断为 0 → 节流禁用
        assert!(!c.should_drop_frame(0));
        assert!(!c.should_drop_frame(1));
        assert!(!c.should_drop_frame(2));
    }
}
