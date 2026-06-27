//! `persona/allocate` — hardware-aware persona allocation.
//!
//! Dep-holding: the allocation decision reads live GPU stats from the shared
//! [`GpuMemoryManager`](crate::gpu::GpuMemoryManager), so the command captures an
//! `Arc` of it (handed in by [`PersonaAllocatorModule`](crate::modules::persona_allocator)'s
//! `commands()`). The catalog + the allocation algorithm are pure domain functions
//! in [`crate::persona::allocator`]; this is the thin typed surface over them.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::gpu::GpuMemoryManager;
use crate::persona::{allocate_personas, load_catalog, AllocationResult};

/// Params for `persona/allocate`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaAllocateParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct PersonaAllocateParams {
    /// Provider API-key env names present on this machine (e.g.
    /// `ANTHROPIC_API_KEY`). A cloud persona whose key is absent is skipped;
    /// empty ⇒ local-only allocation.
    #[serde(default)]
    pub available_api_keys: Vec<String>,
}

crate::action_command! {
    /// Decide which personas should exist on THIS machine: detect GPU/VRAM, read
    /// the persona catalog, and return the allocation — local personas sized to the
    /// VRAM budget plus one cloud persona per present API key. The single source of
    /// truth for "which personas should exist on this machine" (seed time AND at
    /// runtime when keys are added/removed). Read-only planning query — mutates
    /// nothing.
    pub struct PersonaAllocate { gpu_manager: Arc<GpuMemoryManager> }
    name: "persona/allocate",
    access: Privileged,
    params: PersonaAllocateParams,
    output: AllocationResult,
    run(this, _ctx, p) => {
        let catalog = load_catalog();
        // No per-persona overrides at the planning surface: `persona/allocate` is a
        // stateless hardware-tier query with no persona homes in scope. The runtime
        // assignment path (`persona/reassign-model`) is the caller that resolves each
        // persona's home, loads her PersonaModelOverride, and passes the populated map.
        let overrides = std::collections::HashMap::new();
        Ok(allocate_personas(&this.gpu_manager, &p.available_api_keys, &catalog, &overrides))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    fn cmd() -> PersonaAllocate {
        PersonaAllocate {
            gpu_manager: Arc::new(GpuMemoryManager::detect()),
        }
    }

    // what this catches: name/access wiring — allocation is an owner/UI planning
    // surface (reveals hardware tier + drives seeding), so Privileged, not a persona
    // toolbelt action.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(PersonaAllocate::NAME, "persona/allocate");
        assert!(matches!(
            PersonaAllocate::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: the typed body runs the shared GPU manager + catalog
    // end-to-end and returns a populated AllocationResult (allocations + GPU
    // diagnostics + the resolved local model). Proves the dep-holding object reaches
    // the live allocator, not a stub.
    #[tokio::test]
    async fn allocates_from_the_shared_gpu_manager() {
        let out = cmd()
            .run(&Ctx::default(), PersonaAllocateParams::default())
            .await
            .expect("allocation must succeed with no keys");
        assert!(!out.gpu_name.is_empty(), "GPU name is diagnosed");
        assert!(!out.local_model.is_empty(), "a local model is resolved");
    }

    // what this catches: a present API key surfaces its cloud persona in the plan —
    // the key→persona projection still flows through the typed command.
    #[tokio::test]
    async fn present_key_yields_its_cloud_persona() {
        let params = PersonaAllocateParams {
            available_api_keys: vec!["ANTHROPIC_API_KEY".into()],
        };
        let out = cmd()
            .run(&Ctx::default(), params)
            .await
            .expect("allocation must succeed");
        assert!(
            out.allocations
                .iter()
                .any(|a| a.api_key_env.as_deref() == Some("ANTHROPIC_API_KEY")),
            "an Anthropic persona is allocated when its key is present"
        );
    }
}
