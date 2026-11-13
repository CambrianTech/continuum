//! The lever on the engine's host prompt cache: the limit serving last set on each engine, the
//! pure step that picks the next one, and the calls that carry a step out (lower by a share, or
//! take back to the launch grant). [`super::PromptCacheShedder`] decides when.

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

/// What serving last learned of an engine's cache: the limit it set, and what the engine said
/// the cache held after it (the receipt's `held_bytes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LastSet {
    pub engine: Option<EngineIncarnation>,
    pub limit_mib: NonZeroU64,
    pub held_bytes: u64,
}

/// What serving knows of the cache an engine holds now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Known {
    pub limit_mib: NonZeroU64,
    /// What the engine last said its cache held; `None` = not yet asked on this engine.
    pub held_bytes: Option<u64>,
}

impl Known {
    /// Where a reclaim plans from: the limit, or what the cache held if that is less. A limit
    /// far above the cache's contents frees nothing when lowered by the target (limit 9,369 MiB
    /// holding 2 GB, asked for 4 GB: the limit walks down per ask while nothing is freed).
    fn base_mib(self) -> NonZeroU64 {
        match self.held_bytes {
            None => self.limit_mib,
            Some(held) => NonZeroU64::new(whole_mib(held)).map_or(NonZeroU64::MIN, |held| held.min(self.limit_mib)),
        }
    }
}

/// The cache an engine holds now, as far as serving knows. A different engine (both identities
/// known and unequal) starts from its own launch grant, its contents unknown. Otherwise what
/// serving last learned stands, the limit never above the launch grant: a lowering is never a
/// RAISE (taking the cache back is [`PromptCacheLever::take_back`], only when the OS reads
/// normal). No last set and no launch grant (`0` = unknown on the snapshot) is `None`.
pub(super) fn current_limit(last: Option<LastSet>, now: Option<EngineIncarnation>, launched_mib: u32) -> Option<Known> {
    let launched = NonZeroU64::new(u64::from(launched_mib));
    match last {
        Some(LastSet { engine: Some(was), .. }) if now.is_some_and(|is| is != was) => {
            launched.map(|limit_mib| Known { limit_mib, held_bytes: None })
        }
        Some(LastSet { limit_mib, held_bytes, .. }) => Some(Known {
            limit_mib: launched.map_or(limit_mib, |l| l.min(limit_mib)),
            held_bytes: Some(held_bytes),
        }),
        None => launched.map(|limit_mib| Known { limit_mib, held_bytes: None }),
    }
}

/// The next limit for a reclaim of `target_bytes`: `current_mib` less the target in whole MiB
/// (rounded up, at least one), never below the route's floor, the least nonzero limit
/// (`limit_mib` in `1..=SIZE_MAX >> 20`, fork 9269444f6). `None` = already at that floor.
pub(super) fn next_limit(current_mib: NonZeroU64, target_bytes: u64) -> Option<NonZeroU64> {
    if current_mib == NonZeroU64::MIN {
        return None;
    }
    let lowered = current_mib.get().saturating_sub(whole_mib(target_bytes).max(1));
    Some(NonZeroU64::new(lowered).unwrap_or(NonZeroU64::MIN)) // unwrap_or: a target past the whole cache lowers it to the route's floor, the least limit it takes
}

/// One planned step: the engine it applies to, the limit before and after, and whether the
/// cache's contents were known when it was planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Step {
    engine: Option<EngineIncarnation>,
    held_known: bool,
    pub before_mib: NonZeroU64,
    pub after_mib: NonZeroU64,
}

/// Why no step was planned. Both mean the heavy lever decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoStep {
    /// The snapshot names no launch grant and serving never set one: nothing to lower from.
    LimitUnknown,
    /// The cache is already at the route's floor, or holds nothing above it.
    AtFloor { limit_mib: NonZeroU64 },
}

/// What one reclaim did to the cache: at most two limits set, their freed bytes summed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Applied {
    pub limit_after_mib: NonZeroU64,
    pub freed_bytes: u64,
    pub held_bytes: u64,
    pub calls: u64,
}

/// What taking the cache back did: the limit before and after, and what the engine said the
/// cache held after it (the `/prompt-cache/limit` reply).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Raised {
    pub before_mib: NonZeroU64,
    pub after_mib: NonZeroU64,
    pub held_bytes: u64,
}

/// The lever: the engine seam plus what serving last learned of its cache.
pub(super) struct PromptCacheLever {
    control: Arc<dyn PromptCacheControl>,
    /// Brief-locked; never held across an await.
    last: Mutex<Option<LastSet>>,
}

impl PromptCacheLever {
    pub(super) fn new(control: Arc<dyn PromptCacheControl>) -> Self {
        Self { control, last: Mutex::new(None) }
    }

    /// What serving last learned of the cache, on whichever engine it set it.
    pub(super) fn last(&self) -> Option<LastSet> {
        *self.last.lock()
    }

    /// Take the cache back to the engine's launch grant, when serving lowered it below that grant
    /// on this engine. `Ok(None)` = nothing to take back (never lowered, or a relaunched engine
    /// that already starts at its grant). The whole grant at once: a raised limit allocates
    /// nothing, the cache fills only as prompts are stored, and the next deficit lowers it again.
    pub(super) async fn take_back(&self, launched_mib: u32) -> Result<Option<Raised>, LlamaServerError> {
        let engine = self.control.engine();
        let Some(grant) = NonZeroU64::new(u64::from(launched_mib)) else {
            return Ok(None);
        };
        let Some(known) = current_limit(*self.last.lock(), engine, launched_mib).filter(|known| known.limit_mib < grant) else {
            return Ok(None);
        };
        let reply = self.control.set_limit(grant).await?;
        self.learned(engine, grant, reply.held_bytes);
        Ok(Some(Raised { before_mib: known.limit_mib, after_mib: grant, held_bytes: reply.held_bytes }))
    }

    /// Plan the step for a reclaim of `target_bytes`, from what the cache holds when known,
    /// from its limit otherwise (the engine's launch grant on first use).
    pub(super) fn plan(&self, launched_mib: u32, target_bytes: u64) -> Result<Step, NoStep> {
        let engine = self.control.engine();
        let known = current_limit(*self.last.lock(), engine, launched_mib).ok_or(NoStep::LimitUnknown)?;
        let base = known.base_mib();
        let after = next_limit(base, target_bytes).ok_or(NoStep::AtFloor { limit_mib: base })?;
        Ok(Step { engine, held_known: known.held_bytes.is_some(), before_mib: known.limit_mib, after_mib: after })
    }

    /// Carry the step out on the engine. When the cache's contents were unknown and the first
    /// limit freed less than the target while the cache still holds more than the shortfall, one
    /// more limit is set from what it holds, in the same reclaim: never more than two calls. A
    /// refused second call keeps the first's result.
    pub(super) async fn apply(&self, step: Step, target_bytes: u64) -> Result<Applied, LlamaServerError> {
        let first = self.control.set_limit(step.after_mib).await?;
        self.learned(step.engine, step.after_mib, first.held_bytes);
        let mut applied = Applied { limit_after_mib: step.after_mib, freed_bytes: first.freed_bytes, held_bytes: first.held_bytes, calls: 1 };
        let short = target_bytes.saturating_sub(first.freed_bytes);
        if step.held_known || short == 0 || first.held_bytes <= short {
            return Ok(applied);
        }
        let Some(second_limit) = NonZeroU64::new(whole_mib(first.held_bytes)).and_then(|held| next_limit(held, short)) else {
            return Ok(applied);
        };
        if second_limit >= step.after_mib {
            return Ok(applied);
        }
        match self.control.set_limit(second_limit).await {
            Ok(second) => {
                self.learned(step.engine, second_limit, second.held_bytes);
                applied = Applied {
                    limit_after_mib: second_limit,
                    freed_bytes: first.freed_bytes.saturating_add(second.freed_bytes),
                    held_bytes: second.held_bytes,
                    calls: 2,
                };
            }
            Err(error) => continuum_base::probe!(
                class = "serving.reclaim.prompt_cache",
                limit_after_mib = second_limit.get(),
                refusal = %error,
                "the second prompt-cache limit was refused: the first one's result stands"
            ),
        }
        Ok(applied)
    }

    fn learned(&self, engine: Option<EngineIncarnation>, limit_mib: NonZeroU64, held_bytes: u64) {
        *self.last.lock() = Some(LastSet { engine, limit_mib, held_bytes });
    }
}
