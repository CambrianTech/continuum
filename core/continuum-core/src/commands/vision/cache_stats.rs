//! `vision/cache-stats` — report L1 cache diagnostics. A pure read → `AiSafe`.

use std::sync::Arc;

use crate::modules::vision::{VisionCache, VisionCacheStatsResult, VisionStatsParams};

crate::action_command! {
    /// Report L1 vision-cache diagnostics: entry count, capacity, hit/miss counters,
    /// hit rate, and total evictions. A pure read for observability.
    pub struct VisionCacheStats { cache: Arc<VisionCache> }
    name: "vision/cache-stats",
    access: AiSafe,
    params: VisionStatsParams,
    output: VisionCacheStatsResult,
    run(this, _ctx, _p) => {
        Ok(this.cache.stats())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — a pure read is on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(VisionCacheStats::NAME, "vision/cache-stats");
        assert!(matches!(
            VisionCacheStats::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }
}
