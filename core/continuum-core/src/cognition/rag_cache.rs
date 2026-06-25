//! `RagSlice<T>` — the unit of the RAG cache.
//!
//! [`docs/cognition/RAG-AS-PERSISTENT-CACHE.md`]: RAG is the only channel to the
//! model, so everything that affects inference is a RAG slice — and a slice is a
//! *persisted* value that holds its last good state until a process changes it,
//! never a value recomputed on the hot path.
//!
//! A `RagSlice<T>` is that value, published over [`tokio::sync::watch`] (the
//! substrate's default state channel — CONCURRENCY-STYLE-GUIDE §"State
//! distribution"): one off-loop **servicer** owns the writer and `publish()`es;
//! the hot inference loop holds [`RagSliceReader`]s and only `latest()`-reads. The
//! split is enforced at the type level — a reader has no `publish`, so the hot
//! loop *cannot* do expensive production, only cheap selection over what a servicer
//! already staged. This is the same shape as `inference::llama_server`'s
//! `serving.snapshot`.
//!
//! The load-bearing property is **never-starve-after-first-write**: once a servicer
//! publishes, `latest()` returns that value on every subsequent tick until the
//! servicer replaces it — regardless of how many ticks pass with no new input. That
//! is what fixes the recompute-on-read starvation (working memory / roster
//! rendering empty on a fraction of ticks).
//!
//! Values are wrapped in `Arc` so a read is an `Arc` clone (a pointer bump), never
//! a copy of `T` — the pointer-envelope discipline the airc substrate is built on.

use std::sync::Arc;

use tokio::sync::watch;

/// The writer half of a RAG cache slice. Owned by the servicer that maintains this
/// slice; it is the ONLY way to change the slice's value. Holds the `watch::Sender`
/// alive — when the last `RagSlice` for a value drops, readers see the channel
/// close, but `latest()` still returns the last published value (watch retains it).
pub struct RagSlice<T> {
    tx: watch::Sender<Option<Arc<T>>>,
}

/// The reader half — cheap to clone, handed to every faculty / hot-loop consumer of
/// this slice. Deliberately has no `publish`: the hot loop reads, it never writes.
#[derive(Clone)]
pub struct RagSliceReader<T> {
    rx: watch::Receiver<Option<Arc<T>>>,
}

impl<T> RagSlice<T> {
    /// A slice with no value yet — `latest()` returns `None` until the servicer's
    /// first `publish()`. (A faculty reading an un-serviced slice contributes
    /// nothing, exactly as before; the win is that AFTER the first publish it never
    /// goes back to empty.)
    pub fn empty() -> Self {
        let (tx, _rx) = watch::channel(None);
        Self { tx }
    }

    /// A slice seeded with an initial value — for slices that always have a
    /// meaningful default (e.g. an empty-but-present window) so the hot loop never
    /// even sees the un-serviced `None`.
    pub fn seeded(initial: T) -> Self {
        let (tx, _rx) = watch::channel(Some(Arc::new(initial)));
        Self { tx }
    }

    /// Replace the slice's value. The new value is what every subsequent
    /// `latest()` returns until the next `publish()` — this is the "persist until
    /// changed by a process" semantics. Cheap: wraps in `Arc` and bumps the watch.
    pub fn publish(&self, value: T) {
        // send_replace ignores the "no receivers" case — a slice with no current
        // readers still retains its value for a reader that subscribes later.
        let _ = self.tx.send_replace(Some(Arc::new(value)));
    }

    /// Hand out a reader for the hot loop / a faculty. Clone freely.
    pub fn reader(&self) -> RagSliceReader<T> {
        RagSliceReader {
            rx: self.tx.subscribe(),
        }
    }

    /// The current value from the writer side (servicers occasionally need to read
    /// their own slice, e.g. to diff before republishing).
    pub fn latest(&self) -> Option<Arc<T>> {
        self.tx.borrow().clone()
    }
}

impl<T> RagSliceReader<T> {
    /// The last good value, or `None` if the servicer hasn't published yet. An
    /// `Arc` clone — O(1), lock-free, and it NEVER starves: a tick with no new
    /// input still returns the last published value.
    pub fn latest(&self) -> Option<Arc<T>> {
        self.rx.borrow().clone()
    }

    /// Has the servicer published at least once? (`latest().is_some()` without
    /// cloning the `Arc`.)
    pub fn is_initialized(&self) -> bool {
        self.rx.borrow().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: THE core property — never-starve-after-first-write. After
    // one publish, repeated reads with NO new publish keep returning the last good
    // value. This is the fix for recompute-on-read starvation: a tick with no fresh
    // input does not render empty.
    #[test]
    fn persists_last_value_across_reads_with_no_republish() {
        let slice = RagSlice::seeded(0u32);
        let reader = slice.reader();
        slice.publish(42);
        assert_eq!(reader.latest().as_deref().copied(), Some(42));
        // many ticks pass, nothing republished — still the last good value
        assert_eq!(reader.latest().as_deref().copied(), Some(42));
        assert_eq!(reader.latest().as_deref().copied(), Some(42));
    }

    // what this catches: an un-serviced empty slice reads None (a faculty
    // contributes nothing), but the FIRST publish flips it to Some forever after —
    // the starvation only exists before first write, never after.
    #[test]
    fn empty_until_first_publish_then_never_empty() {
        let slice: RagSlice<String> = RagSlice::empty();
        let reader = slice.reader();
        assert!(!reader.is_initialized());
        assert!(reader.latest().is_none());
        slice.publish("window".to_string());
        assert!(reader.is_initialized());
        assert_eq!(reader.latest().as_deref().map(String::as_str), Some("window"));
    }

    // what this catches: publish REPLACES (persist-until-changed) — the newest value
    // wins, the old one is gone; readers see the new value on the next borrow.
    #[test]
    fn publish_replaces_with_newest() {
        let slice = RagSlice::seeded(1u32);
        let reader = slice.reader();
        slice.publish(2);
        slice.publish(3);
        assert_eq!(reader.latest().as_deref().copied(), Some(3));
    }

    // what this catches: a reader subscribed AFTER a publish still sees the retained
    // value — watch keeps the last value, so a faculty wired up late isn't starved.
    #[test]
    fn late_reader_sees_retained_value() {
        let slice = RagSlice::empty();
        slice.publish(7u32);
        let late = slice.reader();
        assert_eq!(late.latest().as_deref().copied(), Some(7));
    }

    // what this catches: the value is Arc-shared, not copied — two readers get the
    // SAME Arc (pointer-envelope discipline: reads are pointer bumps, never T copies).
    #[test]
    fn readers_share_one_arc() {
        let slice = RagSlice::empty();
        slice.publish(vec![1, 2, 3]);
        let a = slice.reader().latest().unwrap();
        let b = slice.reader().latest().unwrap();
        assert!(Arc::ptr_eq(&a, &b));
    }
}
