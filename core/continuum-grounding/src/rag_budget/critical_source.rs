//! A REQUIRED grounding source: one whose absence would make a wrong turn (her room's doctrine,
//! who is present, the map her hands act on). Its contract is different from an optional source,
//! so it is a different type, and the requirement is enforced by that type, not reported at run
//! time: [`CriticalSource::deliver`] returns [`Grounded`], which cannot be built empty, and it
//! cannot fail. A required source holds its state as a required field with its floor in the type
//! (the roster always contains her; a room's doctrine is its own or the base doctrine), and any
//! read failure belongs to the seam that builds that state, never to a turn (Joel, 2026-11-02:
//! "We should never have the ability to complain about a parameter we require").
//!
//! The workspace's required tier takes only a `CriticalSource`, wrapped by [`Required`] into the
//! one `RagSource` the faculty reads.

use super::{RagContext, RagDelivery, RagItem, RagRoomScope, RagSource};

/// A required source's content: at least one item, by construction.
#[derive(Debug, Clone)]
pub struct Grounded {
    first: RagItem,
    rest: Vec<RagItem>,
    truncated: bool,
}

impl Grounded {
    /// The required item first, then any more.
    pub fn new(first: RagItem, rest: Vec<RagItem>, truncated: bool) -> Self {
        Self {
            first,
            rest,
            truncated,
        }
    }

    fn into_delivery(self, source_id: &str) -> RagDelivery {
        let mut items = Vec::with_capacity(1 + self.rest.len());
        items.push(self.first);
        items.extend(self.rest);
        RagDelivery::of(source_id, items, self.truncated)
    }
}

/// A source whose absence would be a wrong turn: it always grounds.
#[async_trait::async_trait]
pub trait CriticalSource: Send + Sync {
    fn source_id(&self) -> &'static str;
    fn expand_command(&self) -> Option<&'static str>;
    fn room_scope(&self) -> RagRoomScope {
        RagRoomScope::Conversation
    }
    async fn deliver(&self, ctx: &RagContext) -> Grounded;
}

/// The one adapter from a [`CriticalSource`] to the `RagSource` the faculty reads.
pub struct Required<S: ?Sized>(pub std::sync::Arc<S>);

#[async_trait::async_trait]
impl<S: CriticalSource + ?Sized> RagSource for Required<S> {
    fn room_scope(&self) -> RagRoomScope {
        self.0.room_scope()
    }

    fn source_id(&self) -> &'static str {
        self.0.source_id()
    }

    fn expand_command(&self) -> Option<&'static str> {
        self.0.expand_command()
    }

    async fn deliver(&self, ctx: &RagContext) -> RagDelivery {
        self.0.deliver(ctx).await.into_delivery(self.0.source_id())
    }
}
