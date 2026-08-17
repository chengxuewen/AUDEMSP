//! field 会话门面（Phase 2 接 webrtc 真传输；当前提供类型骨架 + 事件定义）。
//!
//! 契约 §4：`PushSession`（采集→编码→推流）/ `PullSession`（订阅→解码→出帧）。
//! MVP 声明类型与 `SessionEvents`，`connect` 明确报"未实现"——webrtc 传输接入
//! （host SFU 推流链路复用) 在 field Phase 2。

use mediaservo_deck::DeckError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::FieldError;

/// 会话事件流（契约 §4）。
#[derive(Debug)]
#[non_exhaustive]
pub enum SessionEvent {
    Connected,
    Disconnected { reason: String },
    Error(FieldError),
}

/// 会话事件接收端。
pub type SessionEvents = UnboundedReceiver<SessionEvent>;

/// 会话事件发送端（内部）。
pub(crate) type EventSender = UnboundedSender<SessionEvent>;

/// 推流会话（Phase 2 实现：采集→编码→webrtc 推流）。
#[derive(Debug)]
pub struct PushSession {
    _events: EventSender,
}

impl PushSession {
    /// 创建会话（Phase 2 前为 stub，connect 必失败以便调用方尽早感知）。
    pub async fn connect() -> Result<(Self, SessionEvents), FieldError> {
        Err(FieldError::InvalidState(
            "PushSession 需要 webrtc 传输接入 (field Phase 2)".into(),
        ))
    }
}

/// 拉流会话（Phase 2 实现：订阅→解码→出帧）。
#[derive(Debug)]
pub struct PullSession {
    _events: EventSender,
}

impl PullSession {
    /// 创建会话（Phase 2 前为 stub）。
    pub async fn connect() -> Result<(Self, SessionEvents), FieldError> {
        Err(FieldError::InvalidState(
            "PullSession 需要 webrtc 传输接入 (field Phase 2)".into(),
        ))
    }
}

/// 从 deck/link 错误便捷转换为 FieldError（供 Phase 2 使用）。
pub(crate) fn deck_err(e: DeckError) -> FieldError {
    FieldError::Deck(e)
}