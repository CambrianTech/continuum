//! Test fixtures for learning: one lease and one admission every learning test shares.

use super::*;

/// Counts the optimizer bytes it admitted and never releases them by itself: its owner does.
pub(super) struct Counting(pub(super) std::sync::Arc<std::sync::atomic::AtomicU64>);

impl OptimizerLease for Counting {
    fn admit(&self, _: Slot, bytes: u64) -> bool {
        self.0.fetch_add(bytes, std::sync::atomic::Ordering::Relaxed);
        true
    }
    fn release(&self, _: Slot, bytes: u64) {
        self.0.fetch_sub(bytes, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Admits every workspace check, so a test sees only where the admission lives.
pub(super) struct Open;

impl ReplyAdmission for Open {
    fn workspace(&self, _: BufferType, _: u64, _: u64) -> bool {
        true
    }
}
