//! `gpu/set-budget` — set a subsystem's VRAM budget. Mutates the per-machine GPU
//! authority, so it is `Privileged` (operator-level), never an arbitrary persona.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::gpu::{GpuMemoryManager, GpuStats, GpuSubsystem};
use crate::sdk_codegen::CommandError;

/// Inputs to `gpu/set-budget`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/gpu/GpuSetBudgetParams.ts"
)]
pub struct GpuSetBudgetParams {
    /// Which subsystem's budget to set: `rendering`, `inference`, or `tts`.
    pub subsystem: String,
    /// The new budget in megabytes. Must be > 0.
    #[ts(type = "number")]
    pub budget_mb: f64,
}

crate::action_command! {
    /// Set a GPU subsystem's VRAM budget (rendering / inference / tts), in megabytes.
    /// Returns the fresh full stats snapshot so the caller sees the applied result.
    /// This mutates the machine's GPU memory authority — an operator-level action.
    pub struct GpuSetBudget { manager: Arc<GpuMemoryManager> }
    name: "gpu/set-budget",
    access: Privileged,
    params: GpuSetBudgetParams,
    output: GpuStats,
    run(this, _ctx, p) => {
        if p.budget_mb <= 0.0 {
            return Err(CommandError::Invalid("budgetMb must be > 0".into()));
        }
        let subsystem = GpuSubsystem::from_name(&p.subsystem).ok_or_else(|| {
            CommandError::Invalid(format!(
                "Unknown subsystem '{}'. Valid: rendering, inference, tts",
                p.subsystem
            ))
        })?;
        let budget_bytes = (p.budget_mb * 1024.0 * 1024.0) as u64;
        this.manager.set_budget(subsystem, budget_bytes);
        Ok(this.manager.stats())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    fn cmd() -> GpuSetBudget {
        GpuSetBudget {
            manager: Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53_000_000_000)),
        }
    }

    // what this catches: name/access wiring — set-budget mutates the authority, so
    // it must be Privileged, not on the unconditional AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(GpuSetBudget::NAME, "gpu/set-budget");
        assert!(matches!(
            GpuSetBudget::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: a valid call applies the budget and the returned snapshot
    // reflects it — the command actually mutates the authority.
    #[tokio::test]
    async fn valid_budget_is_applied() {
        let stats = cmd()
            .run(
                &Ctx::default(),
                GpuSetBudgetParams {
                    subsystem: "inference".into(),
                    budget_mb: 2048.0,
                },
            )
            .await
            .unwrap();
        assert_eq!(stats.inference.budget_mb, 2048.0);
    }

    // what this catches: an unknown subsystem fails LOUD with a named cause, not a
    // silent no-op — bad input never silently corrupts the authority.
    #[tokio::test]
    async fn unknown_subsystem_fails_loud() {
        let err = cmd()
            .run(
                &Ctx::default(),
                GpuSetBudgetParams {
                    subsystem: "nonexistent".into(),
                    budget_mb: 100.0,
                },
            )
            .await
            .unwrap_err();
        assert!(format!("{err:?}").contains("Unknown subsystem"));
    }

    // what this catches: a non-positive budget is rejected — a 0 or negative budget
    // would wedge the subsystem.
    #[tokio::test]
    async fn non_positive_budget_rejected() {
        let err = cmd()
            .run(
                &Ctx::default(),
                GpuSetBudgetParams {
                    subsystem: "tts".into(),
                    budget_mb: -50.0,
                },
            )
            .await
            .unwrap_err();
        assert!(format!("{err:?}").contains("must be > 0"));
    }
}
