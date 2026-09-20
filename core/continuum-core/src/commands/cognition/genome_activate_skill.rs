//! `cognition/genome-activate-skill` — page a skill's LoRA adapter into the persona's
//! genome, evicting LRU under budget (typed, dep-holding).
//!
//! Drives the [`GenomePagingEngine`](crate::persona::genome_paging) virtual-memory
//! layer: ensures the named skill's adapter is resident, evicting the least-recently-used
//! adapters if the (GPU-detected or caller-supplied) memory budget would be exceeded.
//! Reuses the engine's [`ActivateSkillResult`] as the output. Captures the owning
//! module's [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::persona::genome_paging::ActivateSkillResult;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GenomeActivateSkillParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenomeActivateSkillParams {
    /// Persona whose genome pages in the skill.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Skill/domain to activate (e.g. "typescript-expertise").
    pub skill_name: String,
    /// Memory budget in MB; 0 or omitted uses the GPU-detected per-persona budget.
    #[serde(default)]
    #[ts(type = "number")]
    pub memory_budget_mb: f32,
}

crate::action_command! {
    /// Page the named skill's LoRA adapter into the persona's genome, evicting LRU
    /// adapters if the memory budget would be exceeded. Host-invoked.
    pub struct GenomeActivateSkill { state: Arc<CognitionState> }
    name: "cognition/genome-activate-skill",
    access: Internal,
    params: GenomeActivateSkillParams,
    output: ActivateSkillResult,
    run(this, _ctx, p) => {
        let memory_budget_mb = if p.memory_budget_mb > 0.0 {
            p.memory_budget_mb
        } else {
            this.state.per_persona_budget_mb()
        };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| CommandError::Internal(format!("system clock before UNIX epoch: {e}")))?
            .as_millis() as u64;

        let mut persona = this.state.get_or_create_persona(p.persona_id);
        persona.genome_engine.memory_budget_mb = memory_budget_mb;
        let result = persona.genome_engine.activate_skill(&p.skill_name, now_ms);

        crate::log_info!(
            "module",
            "cognition",
            "genome-activate-skill {}: {} activated={}, evicted={:?}, to_load={:?} ({:.0}μs)",
            p.persona_id,
            p.skill_name,
            result.activated,
            result.evicted,
            result.to_load,
            result.decision_time_us
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. genome-activate-skill is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(GenomeActivateSkill::NAME, "cognition/genome-activate-skill");
        assert_eq!(GenomeActivateSkill::ACCESS, AccessLevel::Internal);
    }
}
