//! Why `llama_decode` did not run a batch, by its own return codes.

/// A refused decode, typed by what the caller can do about it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeRefusal {
    /// The memory has no room for the batch (code 1). In a pool several minds share, another mind paged
    /// out makes room; nothing of the batch was decoded.
    #[error("llama_decode: no room in the memory for the batch")]
    NoRoom,
    /// Any other code: aborted (2), an invalid batch (-1), or fatal (< -1).
    #[error("llama_decode returned {0}")]
    Failed(i32),
    /// The native call threw; its message, bounded.
    #[error("llama_decode threw: {0}")]
    Threw(String),
}

impl DecodeRefusal {
    pub(crate) fn from_code(rc: i32) -> Self {
        if rc == 1 {
            Self::NoRoom
        } else {
            Self::Failed(rc)
        }
    }
}

impl From<DecodeRefusal> for String {
    fn from(refusal: DecodeRefusal) -> Self {
        refusal.to_string()
    }
}
