//! `vision/*` — the content-addressed vision-description cache as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one verb per file.
//!
//! The cache is the system bridge that makes every persona see (per the sensory
//! architecture): a capable model receives the raw image, a lesser one receives a
//! text description — and that description, computed once per unique image, is stored
//! here and shared across all 14+ personas (one LLaVA call per image, forever; it
//! survives deploys via L2 warm-up). These verbs are the cache's wire surface.
//!
//! Access split: the three reads (description-get, description-status, cache-stats)
//! are `AiSafe`; the three mutators (description-put, cache-warm, cache-evict) write
//! the shared cache and — for `put` — publish `vision:description:ready`, so they are
//! `Privileged`. Each captures the module's `Arc<VisionCache>`; they are assembled by
//! [`command_objects`] and exposed through `VisionModule::commands`.

use std::sync::Arc;

use crate::modules::vision::VisionCache;
use crate::sdk_codegen::DynCommand;

pub mod cache_evict;
pub mod look;
pub mod cache_stats;
pub mod cache_warm;
pub mod description_get;
pub mod description_put;
pub mod description_status;

/// Build the dep-holding `vision/*` command objects over the shared
/// [`VisionCache`] + the module's executor slot (vision/look re-enters the bus
/// for the describe generate). Called from `VisionModule::commands`.
pub fn command_objects(
    cache: Arc<VisionCache>,
    executor_slot: Arc<crate::runtime::LateBound<crate::runtime::CommandExecutor>>,
) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(look::VisionLook { executor_slot }),
        Arc::new(description_get::VisionDescriptionGet {
            cache: cache.clone(),
        }),
        Arc::new(description_put::VisionDescriptionPut {
            cache: cache.clone(),
        }),
        Arc::new(description_status::VisionDescriptionStatus {
            cache: cache.clone(),
        }),
        Arc::new(cache_stats::VisionCacheStats {
            cache: cache.clone(),
        }),
        Arc::new(cache_warm::VisionCacheWarm {
            cache: cache.clone(),
        }),
        Arc::new(cache_evict::VisionCacheEvict { cache }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;
    use cache_evict::VisionCacheEvict;
    use cache_stats::VisionCacheStats;
    use cache_warm::VisionCacheWarm;
    use description_get::VisionDescriptionGet;
    use description_put::VisionDescriptionPut;
    use description_status::VisionDescriptionStatus;

    // what this catches: the six vision commands carry their `vision/<verb>` wire names
    // — the routing keys cu, the persona tool surface, and the grid bind to. The name
    // mirrors the file path; drift silently breaks the "file tree IS the namespace"
    // contract.
    #[test]
    fn vision_command_names_mirror_their_path() {
        assert_eq!(VisionDescriptionGet::NAME, "vision/description-get");
        assert_eq!(VisionDescriptionPut::NAME, "vision/description-put");
        assert_eq!(VisionDescriptionStatus::NAME, "vision/description-status");
        assert_eq!(VisionCacheStats::NAME, "vision/cache-stats");
        assert_eq!(VisionCacheWarm::NAME, "vision/cache-warm");
        assert_eq!(VisionCacheEvict::NAME, "vision/cache-evict");
    }

    // what this catches: command_objects assembles all seven verbs — a dropped entry
    // would silently remove a vision command from the registry.
    #[test]
    fn command_objects_assembles_all_seven() {
        let cache = Arc::new(VisionCache::new());
        let slot = Arc::new(crate::runtime::LateBound::new("vision executor"));
        let objs = command_objects(cache, slot);
        assert_eq!(objs.len(), 7);
    }
}
