//! `cognition/genome-evict-under-pressure` — drive genome eviction down to a target
//! pressure ratio (typed, dep-holding).
//!
//! The PressureBroker lever: evicts LRU adapters (respecting critical-adapter protection,
//! priority > 0.9) until memory pressure falls to `target_pressure`, without an
//! `activate-skill` call. Uses the same formula and victim selection as activate-skill's
//! implicit eviction. When the broker singleton lands and registers per-persona
//! ResourcePool wrappers, this is the command those wrappers call. Captures the owning
//! module's [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;

fn default_target_pressure() -> f32 {
    0.75
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GenomeEvictUnderPressureParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenomeEvictUnderPressureParams {
    /// Persona whose genome is evicted.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Target memory-pressure ratio to evict down to (default 0.75).
    #[serde(default = "default_target_pressure")]
    #[ts(type = "number")]
    pub target_pressure: f32,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GenomeEvictUnderPressureResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenomeEvictUnderPressureResult {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    #[ts(type = "number")]
    pub target_pressure: f32,
    #[ts(type = "number")]
    pub pressure_before: f32,
    #[ts(type = "number")]
    pub pressure_after: f32,
    #[ts(type = "number")]
    pub bytes_freed: u64,
}

crate::action_command! {
    /// Evict the persona's genome LRU adapters down to `target_pressure` (critical
    /// adapters protected). Returns bytes freed + pressure before/after. Host-invoked.
    pub struct GenomeEvictUnderPressure { state: Arc<CognitionState> }
    name: "cognition/genome-evict-under-pressure",
    access: Internal,
    params: GenomeEvictUnderPressureParams,
    output: GenomeEvictUnderPressureResult,
    run(this, _ctx, p) => {
        let mut persona = this.state.get_or_create_persona(p.persona_id);
        let pressure_before = persona.genome_engine.memory_pressure();
        let bytes_freed = persona.genome_engine.evict_under_pressure(p.target_pressure);
        let pressure_after = persona.genome_engine.memory_pressure();

        crate::log_info!(
            "module",
            "cognition",
            "genome-evict-under-pressure {}: target={:.2} pressure {:.2} → {:.2}, freed {} bytes",
            p.persona_id,
            p.target_pressure,
            pressure_before,
            pressure_after,
            bytes_freed
        );

        Ok(GenomeEvictUnderPressureResult {
            persona_id: p.persona_id,
            target_pressure: p.target_pressure,
            pressure_before,
            pressure_after,
            bytes_freed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. genome-evict-under-pressure is
    // host-driven cognition IPC, so it is Internal — registered and grid-routable,
    // never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(
            GenomeEvictUnderPressure::NAME,
            "cognition/genome-evict-under-pressure"
        );
        assert_eq!(GenomeEvictUnderPressure::ACCESS, AccessLevel::Internal);
    }
}
