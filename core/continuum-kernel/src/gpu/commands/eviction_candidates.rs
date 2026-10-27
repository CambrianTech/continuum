//! `gpu/eviction-candidates` — the tracked consumers that MAY be evicted, sorted
//! highest-score (best to evict) first. Realtime consumers are excluded.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::gpu::{EvictableEntry, GpuMemoryManager};

/// `gpu/eviction-candidates` takes no input.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/gpu/GpuEvictionCandidatesParams.ts"
)]
pub struct GpuEvictionCandidatesParams {}

/// Result of `gpu/eviction-candidates` — the evictable consumers, best-candidate
/// first. A named wrapper so the wire type is a struct, not a bare `Array<T>`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/gpu/GpuEvictionCandidates.ts"
)]
pub struct EvictionCandidates {
    /// Tracked consumers that may be evicted, sorted highest-score (best to evict)
    /// first. Realtime consumers are excluded.
    pub candidates: Vec<EvictableEntry>,
}

crate::action_command! {
    /// List the GPU consumers that may be evicted to reclaim VRAM, best-candidate
    /// first. Realtime consumers (render targets, live audio) are never candidates,
    /// so they are excluded. Read this before reclaiming memory under pressure.
    pub struct GpuEvictionCandidates { manager: Arc<GpuMemoryManager> }
    name: "gpu/eviction-candidates",
    access: AiSafe,
    params: GpuEvictionCandidatesParams,
    output: EvictionCandidates,
    run(this, _ctx, _p) => {
        Ok(EvictionCandidates {
            candidates: this.manager.eviction_registry.candidates(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(GpuEvictionCandidates::NAME, "gpu/eviction-candidates");
        assert!(matches!(
            GpuEvictionCandidates::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: realtime consumers must never be offered as eviction
    // candidates — evicting a render target or live-audio buffer would stutter the
    // UI. Only the interactive consumer should come back.
    #[tokio::test]
    async fn excludes_realtime_consumers() {
        use crate::gpu::{make_entry, GpuPriority};
        let manager = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53_000_000_000));
        manager.eviction_registry.register(make_entry(
            "render:targets",
            "Render Targets",
            GpuPriority::Realtime,
            100_000_000,
        ));
        manager.eviction_registry.register(make_entry(
            "candle:llama",
            "Llama 3.2",
            GpuPriority::Interactive,
            3_000_000_000,
        ));
        let cmd = GpuEvictionCandidates {
            manager: manager.clone(),
        };
        let out = cmd
            .run(&Ctx::default(), GpuEvictionCandidatesParams {})
            .await
            .unwrap();
        let candidates = out.candidates;
        assert_eq!(candidates.len(), 1, "realtime must be excluded");
        assert_eq!(candidates[0].id, "candle:llama");
    }
}
