//! `vision/cache-warm` — bulk-restore the L1 cache from persisted L2 rows. Mutates the
//! shared cache → `Privileged`.

use std::sync::Arc;

use crate::modules::vision::{VisionCache, VisionWarmParams, VisionWarmResult};

crate::action_command! {
    /// Bulk-warm the L1 vision cache from persisted L2 rows (called by TS on startup
    /// to restore descriptions from the ORM after a deploy). Skips corrupt rows
    /// missing a content key or description; stops at capacity. Returns
    /// `{warmed, total}`.
    pub struct VisionCacheWarm { cache: Arc<VisionCache> }
    name: "vision/cache-warm",
    access: Privileged,
    params: VisionWarmParams,
    output: VisionWarmResult,
    run(this, _ctx, p) => {
        Ok(this.cache.warm(&p.entries))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — a cache mutator is Privileged.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(VisionCacheWarm::NAME, "vision/cache-warm");
        assert!(matches!(
            VisionCacheWarm::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
