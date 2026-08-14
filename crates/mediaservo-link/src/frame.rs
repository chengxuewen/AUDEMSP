//! 帧元数据 + 帧引用 + 帧流（latest-slot）。

use std::sync::{Arc, Mutex};

/// 帧元数据（定长 LE 编码，D243）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameMeta {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    /// 像素格式（0=未知, 1=I420, 2=NV12, 3=RGBA）。
    pub format: u8,
    /// 元数据版本（演进用，D243）。
    pub version: u8,
    pub is_keyframe: bool,
    pub ts_mono_ns: u64,
    pub ts_epoch_ns: u64,
}

impl FrameMeta {
    /// 定长编码字节数：
    /// seq(8) + width(4) + height(4) + format(1) + version(1) + keyframe(1) + reserved(1)
    /// + ts_mono_ns(8) + ts_epoch_ns(8) = 36。
    pub const WIRE_LEN: usize = 36;

    pub fn encode(&self) -> [u8; Self::WIRE_LEN] {
        let mut b = [0u8; Self::WIRE_LEN];
        b[0..8].copy_from_slice(&self.seq.to_le_bytes());
        b[8..12].copy_from_slice(&self.width.to_le_bytes());
        b[12..16].copy_from_slice(&self.height.to_le_bytes());
        b[16] = self.format;
        b[17] = self.version;
        b[18] = u8::from(self.is_keyframe);
        b[19] = 0; // reserved
        b[20..28].copy_from_slice(&self.ts_mono_ns.to_le_bytes());
        b[28..36].copy_from_slice(&self.ts_epoch_ns.to_le_bytes());
        b
    }

    pub fn decode(b: &[u8]) -> Result<Self, crate::LinkError> {
        if b.len() < Self::WIRE_LEN {
            return Err(crate::LinkError::Bus(format!(
                "frame meta too short: {} < {}",
                b.len(),
                Self::WIRE_LEN
            )));
        }
        Ok(Self {
            seq: u64::from_le_bytes(b[0..8].try_into().expect("8 bytes")),
            width: u32::from_le_bytes(b[8..12].try_into().expect("4 bytes")),
            height: u32::from_le_bytes(b[12..16].try_into().expect("4 bytes")),
            format: b[16],
            version: b[17],
            is_keyframe: b[18] != 0,
            ts_mono_ns: u64::from_le_bytes(b[20..28].try_into().expect("8 bytes")),
            ts_epoch_ns: u64::from_le_bytes(b[28..36].try_into().expect("8 bytes")),
        })
    }
}

/// 帧引用（元数据 + payload）。Task 5 可演进为 iceoryx2 SHM 零拷贝视图。
pub struct FrameRef {
    meta: FrameMeta,
    payload: Vec<u8>,
}

impl FrameRef {
    pub fn new(meta: FrameMeta, payload: Vec<u8>) -> Self {
        Self { meta, payload }
    }
    pub fn meta(&self) -> &FrameMeta {
        &self.meta
    }
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// 帧流（latest-slot：慢消费者跳到最新帧，审核 H5）。
///
/// 内部：`Arc<Mutex<Option<FrameRef>>>` + `tokio::sync::Notify`。
/// FrameBus 后台线程每收到一帧调 [`Self::deliver`] 替换槽内帧并通知；
/// 消费者 [`Self::recv`] 等待通知后取最新帧。**禁用无界队列**（会重新引入积压）。
pub struct FrameStream {
    slot: Arc<Mutex<Option<FrameRef>>>,
    notify: Arc<tokio::sync::Notify>,
}

impl FrameStream {
    pub(crate) fn new() -> Self {
        Self {
            slot: Arc::new(Mutex::new(None)),
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// 投递最新帧（替换旧帧），由 FrameBus 后台线程调用。
    pub(crate) fn deliver(&self, frame: FrameRef) {
        *self.slot.lock().expect("frame slot lock") = Some(frame);
        self.notify.notify_waiters();
    }

    /// 取最新帧；无帧时等待。慢消费者自动跳到最新（不积压）。
    pub async fn recv(&self) -> Option<FrameRef> {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(f) = self.slot.lock().expect("frame slot lock").take() {
                return Some(f);
            }
            notified.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(seq: u64) -> FrameRef {
        FrameRef::new(
            FrameMeta { seq, ..Default::default() },
            vec![seq as u8],
        )
    }

    #[tokio::test]
    async fn latest_slot_skips_to_newest() {
        let s = FrameStream::new();
        s.deliver(frame(1));
        s.deliver(frame(2));
        s.deliver(frame(3));
        let f = s.recv().await.expect("frame");
        assert_eq!(f.meta().seq, 3, "慢消费者应跳到最新帧");
    }

    #[tokio::test]
    async fn recv_waits_for_delivery() {
        let s = Arc::new(FrameStream::new());
        let s2 = Arc::clone(&s);
        let h = tokio::spawn(async move {
            let f = s2.recv().await.expect("frame");
            f.meta().seq
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        s.deliver(frame(42));
        assert_eq!(h.await.expect("join"), 42);
    }
}
