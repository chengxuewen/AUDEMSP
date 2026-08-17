//! deck 错误类型。

/// deck 媒体数据面错误。
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DeckError {
    #[error("device: {0}")]
    Device(String),
    #[error("codec: {0}")]
    Codec(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not_found: {0}")]
    NotFound(String),
    #[error("invalid_state: {0}")]
    InvalidState(String),
}