//! Typed errors for the client API.

use thiserror::Error;

/// Errors a `Connection`, `CommandClient`, or `EventSubscriber` can
/// surface. Shaped so FFI bridges (Swift / Kotlin / Dart) can map each
/// variant to an idiomatic exception type without losing structure.
#[derive(Debug, Error)]
pub enum ClientError {
    /// Transport couldn't establish or maintain a session.
    #[error("connect failed: {0}")]
    Connect(String),

    /// Transport was used after close, or the substrate dropped the
    /// session.
    #[error("connection closed")]
    Closed,

    /// Substrate accepted the command but returned an error.
    #[error("substrate refused command `{command}`: {reason}")]
    Refused { command: String, reason: String },

    /// Serialization or deserialization of params/result failed at the
    /// client boundary.
    #[error("codec error: {0}")]
    Codec(String),

    /// Transport-level failure (socket error, timeout, etc).
    #[error("transport error: {0}")]
    Transport(String),

    /// Feature/path not yet implemented in this skeleton.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

impl From<serde_json::Error> for ClientError {
    fn from(e: serde_json::Error) -> Self {
        ClientError::Codec(e.to_string())
    }
}
