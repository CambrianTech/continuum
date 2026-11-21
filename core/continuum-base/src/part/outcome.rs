//! What a part answers a verb with (object model §3.1, `Outcome`).

use std::future::Future;
use std::pin::Pin;

/// The rest of a pending answer: the part's own work still finishing (a write in flight, pages
/// still copying). Its owner drives it to the end; until then the owner keeps custody of the part.
pub type Settling<E> = Pin<Box<dyn Future<Output = Result<(), E>> + Send + 'static>>;

/// A part's answer to one verb.
pub enum Outcome<E> {
    /// The verb is complete.
    Done,
    /// The verb has started work that is still finishing; [`Settling`] completes it.
    Pending(Settling<E>),
    /// The verb could not be done, and why.
    Failed(E),
}

impl<E> Outcome<E> {
    /// A pending answer over `rest`.
    pub fn pending(rest: impl Future<Output = Result<(), E>> + Send + 'static) -> Self {
        Self::Pending(Box::pin(rest))
    }
}

impl<E: std::fmt::Debug> std::fmt::Debug for Outcome<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Done => f.write_str("Done"),
            Self::Pending(_) => f.write_str("Pending"),
            Self::Failed(error) => f.debug_tuple("Failed").field(error).finish(),
        }
    }
}
