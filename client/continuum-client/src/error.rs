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
    ///
    /// `outcome` is the command's OWN typed result when the refusal carried one — a
    /// handler that answers `{ success: false, errorKind, nextHistoryOffset, … }` is
    /// refusing with data, and that data must reach the caller (card f4d2fa49:
    /// `genome/job-status`'s HistoryIncomplete/HistoryCorrupt lost their continuation
    /// offset and malformed count at this boundary and arrived as prose). `None` when
    /// the refusal was a transport or gate string with nothing structured behind it.
    #[error("substrate refused command `{command}`: {reason}")]
    Refused {
        command: String,
        reason: String,
        outcome: Option<serde_json::Value>,
    },

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
