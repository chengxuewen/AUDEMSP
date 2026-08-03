use thiserror::Error;

/// Unified codec error type.
#[derive(Debug, Error)]
pub enum CodecError {
    #[error("invalid encoder configuration: {0}")]
    InvalidConfig(String),

    #[error("invalid dimensions: width={0}, height={1}")]
    InvalidDimension(u32, u32),

    #[error("unsupported codec: {0:?}")]
    UnsupportedCodec(crate::codec::CodecId),

    #[error("no backend compiled for codec {0:?}")]
    NoBackend(crate::codec::CodecId),

    #[error("invalid state: {0}")]
    InvalidState(String),

    #[error("resource exhausted: {0}")]
    ResourceExhausted(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("encoder error: {0}")]
    Encoder(String),

    #[error("decoder error: {0}")]
    Decoder(String),

    #[error("internal codec error: {0}")]
    Internal(String),
}
