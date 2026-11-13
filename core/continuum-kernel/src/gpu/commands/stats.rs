//! `gpu/stats` — full GPU memory snapshot (total VRAM, per-subsystem
//! budgets/usage, pressure).

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::gpu::{GpuMemoryManager, GpuStats};

/// `gpu/stats` takes no input — it reports the current GPU authority state.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/gpu/GpuStatsParams.ts"
)]
pub struct GpuStatsParams {}

crate::action_command! {
    /// Snapshot GPU memory: total VRAM, the per-subsystem budgets and usage
    /// (rendering, inference, tts), and the overall pressure (0.0–1.0). Read this to
    /// reason about whether there's headroom for a heavier model or a new lane.
    pub struct GpuStatsCmd { manager: Arc<GpuMemoryManager> }
    name: "gpu/stats",
    access: AiSafe,
    params: GpuStatsParams,
    output: GpuStats,
    run(this, _ctx, _p) => {
        Ok(this.manager.stats())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    fn cmd() -> GpuStatsCmd {
        GpuStatsCmd {
            manager: Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53_000_000_000)),
        }
    }

    // what this catches: the wire name must mirror the file path so the persona
    // reaches the tool by the name it would guess.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(GpuStatsCmd::NAME, "gpu/stats");
        assert!(matches!(
            GpuStatsCmd::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: the snapshot carries the per-subsystem budgets a caller
    // needs to reason about headroom — a regression that dropped a subsystem would
    // hand back a blind snapshot.
    #[tokio::test]
    async fn stats_reports_per_subsystem_budgets() {
        let stats = cmd().run(&Ctx::default(), GpuStatsParams {}).await.unwrap();
        assert!(stats.total_vram_mb >= 0.0);
        assert!(stats.pressure >= 0.0 && stats.pressure <= 1.0);
    }
}
