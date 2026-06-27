//! `vision/cache-evict` — drop L1 entries idle longer than a threshold. Mutates the
//! shared cache → `Privileged`.

use std::sync::Arc;

use crate::modules::vision::{VisionCache, VisionEvictParams, VisionEvictResult};

/// Default idle window: evict entries not accessed in the last 30 minutes.
const DEFAULT_IDLE_MS: u64 = 1_800_000;

crate::action_command! {
    /// Evict L1 vision-cache entries not accessed within `idle_ms` (default 30 min).
    /// Manual reclaim for memory pressure; complements the automatic LRU eviction on
    /// `description-put`. Returns `{evicted, remaining}`.
    pub struct VisionCacheEvict { cache: Arc<VisionCache> }
    name: "vision/cache-evict",
    access: Privileged,
    params: VisionEvictParams,
    output: VisionEvictResult,
    run(this, _ctx, p) => {
        Ok(this.cache.evict(p.idle_ms.unwrap_or(DEFAULT_IDLE_MS)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — a cache mutator is Privileged.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(VisionCacheEvict::NAME, "vision/cache-evict");
        assert!(matches!(
            VisionCacheEvict::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
