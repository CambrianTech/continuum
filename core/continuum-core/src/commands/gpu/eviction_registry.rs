//! `gpu/eviction-registry` — full snapshot of every tracked GPU consumer.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::gpu::{EvictionRegistrySnapshot, GpuMemoryManager};

/// `gpu/eviction-registry` takes no input.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/gpu/GpuEvictionRegistryParams.ts"
)]
pub struct GpuEvictionRegistryParams {}

crate::action_command! {
    /// Snapshot every GPU consumer the eviction registry tracks: each entry's id,
    /// label, priority, and footprint, plus the total tracked bytes and how many are
    /// evictable. Read this to see who is holding VRAM before deciding what to evict.
    pub struct GpuEvictionRegistry { manager: Arc<GpuMemoryManager> }
    name: "gpu/eviction-registry",
    access: AiSafe,
    params: GpuEvictionRegistryParams,
    output: EvictionRegistrySnapshot,
    run(this, _ctx, _p) => {
        Ok(this.manager.eviction_registry.snapshot())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(GpuEvictionRegistry::NAME, "gpu/eviction-registry");
        assert!(matches!(
            GpuEvictionRegistry::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: a registered consumer surfaces in the snapshot with its
    // footprint summed into the total — the read actually reflects the registry.
    #[tokio::test]
    async fn registered_consumer_appears_in_snapshot() {
        use crate::gpu::{make_entry, GpuPriority};
        let manager = Arc::new(GpuMemoryManager::detect());
        manager.eviction_registry.register(make_entry(
            "candle:llama",
            "Llama 3.2",
            GpuPriority::Interactive,
            3_000_000_000,
        ));
        let cmd = GpuEvictionRegistry {
            manager: manager.clone(),
        };
        let snap = cmd
            .run(&Ctx::default(), GpuEvictionRegistryParams {})
            .await
            .unwrap();
        assert_eq!(snap.entries.len(), 1);
        assert_eq!(snap.total_tracked_bytes, 3_000_000_000);
    }
}
