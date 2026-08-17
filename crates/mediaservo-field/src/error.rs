//! field 错误类型。

use mediaservo_deck::DeckError;
use mediaservo_link::LinkError;

/// field 组合 SDK 错误（契约 §4）。
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FieldError {
    #[error("link: {0}")]
    Link(#[from] LinkError),
    #[error("deck: {0}")]
    Deck(#[from] DeckError),
    #[error("webrtc: {0}")]
    WebRtc(String),
    #[error("codec: {0}")]
    Codec(String),
    #[error("closed")]
    Closed,
    #[error("invalid_state: {0}")]
    InvalidState(String),
}