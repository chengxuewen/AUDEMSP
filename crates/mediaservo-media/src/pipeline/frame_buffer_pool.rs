//! FrameBufferPool — I420 帧缓冲池（内存复用，避免每帧分配）。
//!
//! 高帧率管线中每帧 `I420Buffer::new` 会产生堆分配压力（GC/分配器抖动）。
//! 缓冲池按 (width, height) 分桶复用已回收的 buffer。
//!
//! 对照 OpenCTK `video_frame_buffer_pool.hpp`：Get → 复用（无空闲则新建），
//! Recycle → 归还。非线程安全（调用方保证单线程或外部同步）。

use std::collections::HashMap;

use crate::base::buffer::{I420Buffer, VideoBuffer};

/// I420 帧缓冲池。
#[derive(Debug, Default)]
pub struct FrameBufferPool {
    /// key = (width, height)，value = 空闲 buffer 栈。
    free: HashMap<(u32, u32), Vec<I420Buffer>>,
    /// 池上限（每个尺寸）；超限回收直接丢弃。
    max_pool_size_per_size: usize,
}

impl FrameBufferPool {
    /// 创建缓冲池，默认每尺寸上限 4 个。
    pub fn new() -> Self {
        Self::with_limit(4)
    }

    /// 创建缓冲池并设置每尺寸空闲上限。
    pub fn with_limit(max_pool_size_per_size: usize) -> Self {
        Self {
            free: HashMap::new(),
            max_pool_size_per_size,
        }
    }

    /// 获取一个 (width, height) 的 I420 buffer（复用或新建）。
    pub fn get(&mut self, width: u32, height: u32) -> I420Buffer {
        self.free
            .get_mut(&(width, height))
            .and_then(|stack| stack.pop())
            .unwrap_or_else(|| I420Buffer::new(width, height))
    }

    /// 归还 buffer 复用；空闲栈超限则丢弃（避免池无限增长）。
    pub fn recycle(&mut self, buffer: I420Buffer) {
        let key = (buffer.width(), buffer.height());
        let stack = self.free.entry(key).or_default();
        if stack.len() < self.max_pool_size_per_size {
            stack.push(buffer);
        }
        // 超限：buffer 直接 drop（释放内存）
    }

    /// 当前空闲 buffer 总数。
    pub fn idle_count(&self) -> usize {
        self.free.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_recycles_same_size_buffer() {
        let mut pool = FrameBufferPool::new();
        let b = pool.get(640, 480);
        assert_eq!(b.width(), 640);
        assert_eq!(b.height(), 480);
        pool.recycle(b);
        assert_eq!(pool.idle_count(), 1);
        // 再次获取 → 复用（idle 减少，无新分配）
        let b2 = pool.get(640, 480);
        assert_eq!(pool.idle_count(), 0);
        assert_eq!(b2.width(), 640);
    }

    #[test]
    fn different_sizes_have_separate_buckets() {
        let mut pool = FrameBufferPool::new();
        let b1 = pool.get(640, 480);
        let b2 = pool.get(320, 240);
        pool.recycle(b1);
        pool.recycle(b2);
        assert_eq!(pool.idle_count(), 2);
        // 不同尺寸不互相复用
        let _ = pool.get(640, 480);
        assert_eq!(pool.idle_count(), 1);
    }

    #[test]
    fn recycle_drops_when_pool_exceeds_limit() {
        let mut pool = FrameBufferPool::with_limit(2);
        let b1 = pool.get(100, 100);
        let b2 = pool.get(100, 100);
        let b3 = pool.get(100, 100);
        pool.recycle(b1);
        pool.recycle(b2);
        pool.recycle(b3); // 第 3 个 → 丢弃
        assert_eq!(pool.idle_count(), 2);
    }

    #[test]
    fn get_creates_new_when_pool_empty() {
        let mut pool = FrameBufferPool::new();
        let b = pool.get(16, 16);
        assert_eq!(b.width(), 16);
        assert_eq!(pool.idle_count(), 0);
    }
}
