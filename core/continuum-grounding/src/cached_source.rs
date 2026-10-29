//! CachedRagSource — ANY grounding source as an event-invalidated cache.
//!
//! ### The ruling this implements (Joel, 2026-08-12)
//!
//! "Some of the rag stuff needs to just be cache based. We aren't a benchmarking
//! machine, but that's what caches are for. Keep it general." Measured that day
//! (#266 evidence, Asha/pytest-5221): every compose re-fetched every grounding
//! source — airc roster/doctrine/board round-trips, a workspace-map directory
//! re-walk, ~4.6k tokens of tool schemas — and the churn re-prefilled ~5k tokens
//! per act (~50s of every ~51s act; only the first ~2.2k tokens ever cache-hit).
//!
//! ### The shape
//!
//! This is [[the-whole-system-is-event-based-not-polling]] applied to perception:
//! a source delivers its **last-good** projection until the event that actually
//! changes its substrate fires — roster until join/part, board until a card
//! event, workspace-map until one of the persona's own write receipts. The owner
//! who constructs the wrap holds a [`DirtyHandle`] and connects it to that event;
//! `deliver` never re-fetches on the compose path unless dirty (or the cache
//! genuinely cannot answer: a different room). No benchmark/drive mode — an exam composes fast
//! because nothing it depends on changes, a live room gets the same win because
//! rosters and doctrine barely change there either.
//!
//! Deliberately a DECORATOR, not a change to `RagSource` or to any source: one
//! wrap serves the whole ecosystem (compression), and a source that must never be
//! cached simply isn't wrapped.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use crate::rag_budget::{RagContext, RagDelivery, RagSource};

/// The invalidation lever the OWNER wires to the source's change event.
/// Cloneable and cheap; `mark()` from any thread/task.
#[derive(Clone)]
pub struct DirtyHandle(Arc<AtomicBool>);

impl DirtyHandle {
    pub fn mark(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// A weak lever for long-lived listeners (a bus subscriber task): `mark()`
    /// returns `false` once the wrapped source is gone, so the listener can
    /// exit instead of parking forever on behalf of a dead cache. This is what
    /// keeps per-fork invalidator tasks from leaking across ephemeral eval
    /// forks — the fork's cycle drops, the weak dies, the task ends.
    pub fn downgrade(&self) -> WeakDirtyHandle {
        WeakDirtyHandle(Arc::downgrade(&self.0))
    }
}

/// See [`DirtyHandle::downgrade`].
#[derive(Clone)]
pub struct WeakDirtyHandle(std::sync::Weak<AtomicBool>);

impl WeakDirtyHandle {
    /// Mark dirty if the cache is still alive. `false` == the owner dropped;
    /// the caller should stop listening.
    pub fn mark(&self) -> bool {
        match self.0.upgrade() {
            Some(flag) => {
                flag.store(true, Ordering::SeqCst);
                true
            }
            None => false,
        }
    }
}

struct CachedDelivery {
    delivery: RagDelivery,
    /// The room the cached projection was grounded in — a different room must
    /// never be served another room's roster/board/doctrine (the exam-bleed
    /// class). `None` == the context had no room; it only matches `None`.
    room: Option<uuid::Uuid>,
}

pub struct CachedRagSource {
    inner: Arc<dyn RagSource>,
    dirty: Arc<AtomicBool>,
    last_good: tokio::sync::Mutex<Option<CachedDelivery>>,
}

impl CachedRagSource {
    /// Wrap `inner`; returns the wrap and the [`DirtyHandle`] the owner must
    /// connect to the event that changes this source's substrate. Starts DIRTY
    /// so the first deliver always fetches.
    pub fn new(inner: Arc<dyn RagSource>) -> (Arc<Self>, DirtyHandle) {
        let dirty = Arc::new(AtomicBool::new(true));
        let handle = DirtyHandle(dirty.clone());
        (
            Arc::new(Self {
                inner,
                dirty,
                last_good: tokio::sync::Mutex::new(None),
            }),
            handle,
        )
    }

    /// Can `cached` answer THIS call without a fetch?
    ///
    /// - Same room (a projection is room-scoped state).
    /// - Not invalidated (the caller checks `dirty` before asking).
    ///
    /// A source is never handed a size, so a delivery is not budget-shaped and
    /// `truncated` is the source's own fetch limit: neither changes the answer.
    fn answers(cached: &CachedDelivery, room: Option<uuid::Uuid>) -> bool {
        cached.room == room
    }
}

#[async_trait]
impl RagSource for CachedRagSource {
    fn room_scope(&self) -> crate::rag_budget::RagRoomScope {
        self.inner.room_scope()
    }

    fn source_id(&self) -> &'static str {
        self.inner.source_id()
    }

    fn expand_command(&self) -> Option<&'static str> {
        self.inner.expand_command()
    }

    async fn deliver(&self, ctx: &RagContext) -> RagDelivery {
        let room = ctx.airc_room.as_ref().map(|r| r.as_uuid());
        let mut guard = self.last_good.lock().await;
        if !self.dirty.load(Ordering::SeqCst) {
            if let Some(cached) = guard.as_ref() {
                if Self::answers(cached, room) {
                    return cached.delivery.clone();
                }
            }
        }
        // Fetch, then clear dirty ONLY if nothing re-marked during the fetch —
        // an event landing mid-fetch must not be lost (swap-then-check would
        // serve a projection already known stale).
        self.dirty.store(false, Ordering::SeqCst);
        let delivery = self.inner.deliver(ctx).await;
        *guard = Some(CachedDelivery {
            delivery: delivery.clone(),
            room,
        });
        delivery
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    /// Counts fetches; delivers a stamped item so staleness is observable.
    struct CountingSource {
        fetches: AtomicU32,
    }

    #[async_trait]
    impl RagSource for CountingSource {
        fn source_id(&self) -> &'static str {
            "counting"
        }
        fn expand_command(&self) -> Option<&'static str> {
            None
        }
        async fn deliver(&self, _ctx: &RagContext) -> RagDelivery {
            let n = self.fetches.fetch_add(1, Ordering::SeqCst) + 1;
            RagDelivery::of(
                "counting",
                vec![crate::rag_budget::RagItem::whole(
                    "counting",
                    format!("fetch #{n}"),
                    serde_json::json!({}),
                )],
                false,
            )
        }
    }

    // what this catches: the per-act refetch this whole module exists to kill
    // (#266/#398) — N composes with no change event must hit the inner source
    // ONCE; a DirtyHandle::mark() must force exactly one refetch; and a room
    // switch must never be served the other room's cached projection.
    #[tokio::test]
    async fn serves_last_good_until_dirty_and_never_across_rooms() {
        let inner = Arc::new(CountingSource {
            fetches: AtomicU32::new(0),
        });
        let (cached, dirty) = CachedRagSource::new(inner.clone());
        let me = uuid::Uuid::new_v4();
        let ctx = RagContext::for_persona(me, 0);

        for _ in 0..5 {
            let d = cached.deliver(&ctx).await;
            assert_eq!(
                d.items[0].content, "fetch #1",
                "unchanged world → cached projection"
            );
        }
        assert_eq!(
            inner.fetches.load(Ordering::SeqCst),
            1,
            "5 composes, ONE fetch"
        );

        dirty.mark();
        let d = cached.deliver(&ctx).await;
        assert_eq!(
            d.items[0].content, "fetch #2",
            "dirty → exactly one refetch"
        );
        assert_eq!(inner.fetches.load(Ordering::SeqCst), 2);

        // A DIFFERENT room must not see this room's projection (exam-bleed class).
        let other = RagContext::for_persona_in_room(me, 0, uuid::Uuid::new_v4());
        let d = cached.deliver(&other).await;
        assert_eq!(
            d.items[0].content, "fetch #3",
            "room switch → fresh fetch, never bleed"
        );
    }
}
