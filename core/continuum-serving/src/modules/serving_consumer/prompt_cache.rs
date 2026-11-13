//! Serving's lightest reclaim lever: give the engine's host prompt cache back first (card
//! 494c99f1).
//!
//! The M5 (64 GB, Qwen3.6-35B-A3B, four residents) on 2026-11-13: 60 MB free, 34 GB in the
//! compressor, swap 8.6 of 10 GB, prefill 92 tok/s against ~190, while the engine held a FIXED
//! 9,369 MiB host prompt cache that never gave memory back. The cache is the cheapest memory
//! serving holds: a dropped state costs one re-prefill of an old prompt, while the heavy levers
//! (tier-down to a smaller model, unload) cost every mind its model. So under pressure serving
//! lowers the cache's limit by what the authority asked for, through the fork's live route, and
//! reaches for a heavy lever only when the cache is at its floor or the engine refuses.
//!
//! This file owns the cache limit serving last set on each engine and the pure step that picks
//! the next one; [`super::ServingConsumer::reclaim`] carries it out and confirms it physically.

use std::num::NonZeroU64;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use crate::inference::engine_residency::EngineIncarnation;
use crate::inference::llama_server::{LlamaServerError, PromptCacheLimit};

/// Whole MiB that hold `bytes`, rounded up: a reclaim is never answered short by rounding.
fn whole_mib(bytes: u64) -> u64 {
    bytes.div_ceil(1 << 20)
}

/// The engine seam the lever drives: which engine is serving now, and the live limit route.
/// The serving daemon implements it over its `LlamaServerControl`; tests hand a fake.
#[async_trait]
pub trait PromptCacheControl: Send + Sync {
    /// The engine process serving now (pid + OS start time + port). `None` = unknown.
    fn engine(&self) -> Option<EngineIncarnation>;

    /// Move the engine's host prompt-cache limit to `limit_mib`; the cache evicts to fit.
    async fn set_limit(&self, limit_mib: NonZeroU64) -> Result<PromptCacheLimit, LlamaServerError>;
}

/// The limit serving last set, and on which engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LastSet {
    pub engine: Option<EngineIncarnation>,
    pub limit_mib: NonZeroU64,
}

/// The cache limit an engine holds now, as far as serving knows. A different engine (both
/// identities known and unequal) starts from its own launch grant. Otherwise the limit serving
/// last set stands, never above the launch grant: an engine this lever cannot tell apart is
/// never handed a RAISE by a reclaim (raising when pressure clears is its own slice). No last
/// set and no launch grant (`0` = unknown on the snapshot) is `None`: nothing to lower from.
pub(super) fn current_limit(last: Option<LastSet>, now: Option<EngineIncarnation>, launched_mib: u32) -> Option<NonZeroU64> {
    let launched = NonZeroU64::new(u64::from(launched_mib));
    match last {
        Some(LastSet { engine: Some(was), .. }) if now.is_some_and(|is| is != was) => launched,
        Some(LastSet { limit_mib, .. }) => Some(launched.map_or(limit_mib, |l| l.min(limit_mib))),
        None => launched,
    }
}

/// The next limit for a reclaim of `target_bytes`: the current limit less the target in whole
/// MiB (rounded up, at least one), never below the route's floor, the least nonzero limit
/// (`limit_mib` in `1..=SIZE_MAX >> 20`, fork 9269444f6). `None` = the cache is at that floor.
pub(super) fn next_limit(current_mib: NonZeroU64, target_bytes: u64) -> Option<NonZeroU64> {
    if current_mib == NonZeroU64::MIN {
        return None;
    }
    let lowered = current_mib.get().saturating_sub(whole_mib(target_bytes).max(1));
    Some(NonZeroU64::new(lowered).unwrap_or(NonZeroU64::MIN)) // unwrap_or: a target past the whole cache lowers it to the route's floor, the least limit it takes
}

/// One planned step: the engine it applies to, and the limit before and after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Step {
    engine: Option<EngineIncarnation>,
    pub before_mib: NonZeroU64,
    pub after_mib: NonZeroU64,
}

/// Why no step was planned. Both mean the heavy lever decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoStep {
    /// The snapshot names no launch grant and serving never set one: nothing to lower from.
    LimitUnknown,
    /// The cache is already at the route's floor.
    AtFloor { limit_mib: NonZeroU64 },
}

/// The lever: the engine seam plus the limit serving last set on it.
pub(super) struct PromptCacheLever {
    control: Arc<dyn PromptCacheControl>,
    /// Brief-locked; never held across an await.
    last: Mutex<Option<LastSet>>,
}

impl PromptCacheLever {
    pub(super) fn new(control: Arc<dyn PromptCacheControl>) -> Self {
        Self { control, last: Mutex::new(None) }
    }

    /// Plan the step for a reclaim of `target_bytes` from the engine's launch grant.
    pub(super) fn plan(&self, launched_mib: u32, target_bytes: u64) -> Result<Step, NoStep> {
        let engine = self.control.engine();
        let current = current_limit(*self.last.lock(), engine, launched_mib).ok_or(NoStep::LimitUnknown)?;
        let after = next_limit(current, target_bytes).ok_or(NoStep::AtFloor { limit_mib: current })?;
        Ok(Step { engine, before_mib: current, after_mib: after })
    }

    /// Carry the step out on the engine; on success the new limit is what serving last set.
    pub(super) async fn apply(&self, step: Step) -> Result<PromptCacheLimit, LlamaServerError> {
        let receipt = self.control.set_limit(step.after_mib).await?;
        *self.last.lock() = Some(LastSet { engine: step.engine, limit_mib: step.after_mib });
        Ok(receipt)
    }
}
