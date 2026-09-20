//! `cognition/gpu-budget` — the per-persona GPU inference budget (typed, dep-holding).
//!
//! Reports the GPU authority state the TS genome initializer needs to size its adapter
//! working set: the device name + total VRAM, the inference subsystem budget, the live
//! persona count, the derived per-persona budget, and the overall pressure. Captures the
//! owning module's [`CognitionState`](crate::modules::cognition::CognitionState) and reads
//! its optional [`GpuMemoryManager`](crate::gpu::GpuMemoryManager).
//!
//! No-GPU is a valid state, not a masked failure: on a CPU-only deploy the module holds no
//! `gpu_manager`, so the budget query honestly reports a zeroed device (`gpu_name:
//! "unknown"`, `total_vram_mb: 0`) alongside the still-meaningful `per_persona_budget_mb`
//! floor. This is explicit reporting of "no GPU present," not a fallback hiding an error —
//! the two branches are the two real runtime states.
//!
//! `access: Internal` — host-driven cognition IPC (genome init reads it), not a persona
//! toolbelt verb.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::cognition::CognitionState;

/// `cognition/gpu-budget` takes no input — it reports the current GPU authority state.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GpuBudgetParams.ts"
)]
pub struct GpuBudgetParams {}

/// The GPU budget snapshot a genome initializer reads to size its adapter working set.
/// A camelCase projection of the [`GpuMemoryManager`](crate::gpu::GpuMemoryManager) stats
/// plus the module's derived per-persona budget. On a CPU-only deploy every VRAM field is
/// zero and `gpu_name` is `"unknown"` — the honest "no GPU present" reading.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GpuBudgetInfo.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GpuBudgetInfo {
    /// Device name, or `"unknown"` when no GPU manager is wired (CPU-only deploy).
    pub gpu_name: String,
    /// Total VRAM (MB), 0 when no GPU is present.
    #[ts(type = "number")]
    pub total_vram_mb: f32,
    /// The inference subsystem's VRAM budget (MB), 0 when no GPU is present.
    #[ts(type = "number")]
    pub inference_budget_mb: f32,
    /// Live persona count the budget is divided across.
    #[ts(type = "number")]
    pub persona_count: u32,
    /// Per-persona inference budget (MB) — the genome's adapter working-set ceiling.
    /// Non-zero even with no GPU (a CPU floor), so genome init always has a real number.
    #[ts(type = "number")]
    pub per_persona_budget_mb: f32,
    /// Overall GPU pressure 0.0–1.0, 0 when no GPU is present.
    pub pressure: f32,
}

crate::action_command! {
    /// Report the per-persona GPU inference budget: device name + total VRAM, the inference
    /// subsystem budget, the live persona count, the derived per-persona budget, and overall
    /// pressure. Host-invoked (genome init sizes its adapter working set from it); not a
    /// persona toolbelt verb. On a CPU-only deploy the VRAM fields are zero and the
    /// per-persona budget reports the CPU floor.
    pub struct GpuBudget { state: Arc<CognitionState> }
    name: "cognition/gpu-budget",
    access: Internal,
    params: GpuBudgetParams,
    output: GpuBudgetInfo,
    run(this, _ctx, _p) => {
        let per_persona = this.state.per_persona_budget_mb();
        let persona_count = this.state.personas.len() as u32;

        // The two branches are the two real runtime states (GPU present / CPU-only),
        // not a happy-path + fallback. No-GPU honestly reports a zeroed device.
        let info = match this.state.gpu_manager.as_ref() {
            Some(mgr) => {
                let stats = mgr.stats();
                GpuBudgetInfo {
                    gpu_name: stats.gpu_name,
                    total_vram_mb: stats.total_vram_mb,
                    inference_budget_mb: stats.inference.budget_mb,
                    persona_count,
                    per_persona_budget_mb: per_persona,
                    pressure: stats.pressure,
                }
            }
            None => GpuBudgetInfo {
                gpu_name: "unknown".to_string(),
                total_vram_mb: 0.0,
                inference_budget_mb: 0.0,
                persona_count,
                per_persona_budget_mb: per_persona,
                pressure: 0.0,
            },
        };

        Ok(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. gpu-budget is host-driven cognition
    // IPC (genome init reads it), so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(GpuBudget::NAME, "cognition/gpu-budget");
        assert_eq!(GpuBudget::ACCESS, AccessLevel::Internal);
    }
}
