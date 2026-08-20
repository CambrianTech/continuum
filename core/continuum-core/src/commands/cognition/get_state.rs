//! `cognition/get-state` — read a persona's live cognitive state (typed, dep-holding).
//!
//! Projects the persona's [`PersonaState`](crate::persona::PersonaState) (energy,
//! attention, mood, inbox load, budget) plus the derived adaptive
//! [`service_cadence_ms`](crate::persona::PersonaState::service_cadence_ms) into one
//! camelCase wire result. Read-only: fails loud with `NotFound` when the persona has no
//! cognition engine (never lazily creates one on a read). Captures the owning module's
//! [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::persona::Mood;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GetStateParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GetStateParams {
    /// Persona whose cognitive state is read.
    #[ts(type = "string")]
    pub persona_id: Uuid,
}

/// The persona's live cognitive state — a camelCase projection of
/// [`PersonaState`](crate::persona::PersonaState) plus the derived service cadence.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GetStateResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GetStateResult {
    /// Energy level 0.0–1.0 (depletes with work, recovers with rest).
    pub energy: f32,
    /// Attention level 0.0–1.0 (focus capacity).
    pub attention: f32,
    /// Current mood derived from state (serializes lowercase: `active`/`tired`/…).
    pub mood: Mood,
    /// Pending inbox items.
    pub inbox_load: u32,
    /// Last activity timestamp (unix ms).
    #[ts(type = "number")]
    pub last_activity_time: u64,
    /// Responses in the current window.
    pub response_count: u32,
    /// Compute budget remaining (rate limiting), 0.0–1.0.
    pub compute_budget: f32,
    /// Adaptive max-wait before a service timeout (ms), derived from mood.
    #[ts(type = "number")]
    pub service_cadence_ms: u64,
}

crate::action_command! {
    /// Read a persona's live cognitive state (energy, attention, mood, budget, cadence).
    /// Host-invoked. Fails loud when the persona has no cognition engine.
    pub struct GetState { state: Arc<CognitionState> }
    name: "cognition/get-state",
    access: Internal,
    params: GetStateParams,
    output: GetStateResult,
    run(this, _ctx, p) => {
        let persona = this
            .state
            .personas
            .get(&p.persona_id)
            .ok_or_else(|| CommandError::NotFound(format!("No cognition for {}", p.persona_id)))?;

        let state = persona.engine.state();
        Ok(GetStateResult {
            energy: state.energy,
            attention: state.attention,
            mood: state.mood,
            inbox_load: state.inbox_load,
            last_activity_time: state.last_activity_time,
            response_count: state.response_count,
            compute_budget: state.compute_budget,
            service_cadence_ms: state.service_cadence_ms(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. get-state is host-driven cognition
    // IPC, so it is Internal — registered and grid-routable, never a remote-callable
    // persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(GetState::NAME, "cognition/get-state");
        assert_eq!(GetState::ACCESS, AccessLevel::Internal);
    }
}
