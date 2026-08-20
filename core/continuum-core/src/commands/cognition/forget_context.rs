//! `cognition/forget-context` — the amnesia flash: wipe ONE episode's engrams from a
//! persona's memory, leaving the rest of her life intact.
//!
//! The command half of the proctored-exam protocol
//! ([[benchmarks-are-proctored-exams-of-the-natural-living-persona]]): the living
//! persona sits a benchmark with her full memory, and afterwards the exam episode —
//! every engram tagged with the exam room's `context_id` — is neuralyzed so future
//! passes can't be contaminated by learned answer keys. Scope is the ONE context id;
//! her other memories are untouched (`AdmissionState::forget_context`, tested by
//! `forget_context_wipes_only_the_tagged_episode`).
//!
//! `access: Internal` — host/proctor-driven exam hygiene, not a persona toolbelt
//! verb (personas do not neuralyze each other).

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ForgetContextParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ForgetContextParams {
    /// Persona whose episode is being forgotten.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// The episode to wipe — every engram whose `context_id` matches is dropped.
    /// Named `episode_id` on the wire because `contextId` is a reserved envelope
    /// axis (identity/context/session) that the transport lifts OUT of params —
    /// glass-boxed live: a `contextId` param is consumed by the envelope and the
    /// command sees `missing field`.
    #[ts(type = "string")]
    pub episode_id: Uuid,
}

/// How much was forgotten, and what remains.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ForgetContextResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ForgetContextResult {
    /// Engrams dropped (all tagged with the episode's context id).
    #[ts(type = "number")]
    pub forgotten: usize,
    /// Engrams remaining in her store after the flash.
    #[ts(type = "number")]
    pub engram_count: usize,
}

crate::action_command! {
    /// Amnesia-flash one episode (by context id) out of a persona's memory.
    /// Proctor-invoked after a benchmark pass; her other memories are untouched.
    pub struct ForgetContext { state: Arc<CognitionState> }
    name: "cognition/forget-context",
    access: Internal,
    params: ForgetContextParams,
    output: ForgetContextResult,
    run(this, _ctx, p) => {
        // LIVE personas register their mind (and its AdmissionState, via the
        // ActingBody) in `persona_workspace::global()` — the production spawn
        // path. The `CognitionState` map is the IPC-era registry
        // (`cognition/create-engine`). Live first, IPC fallback: one command
        // serves both worlds, fails loud when neither knows the persona.
        let live = crate::cognition::persona_workspace::global()
            .get(&p.persona_id)
            .and_then(|cycle| cycle.acting().map(|a| a.admission.clone()));
        let admission = match live {
            Some(a) => a,
            None => this
                .state
                .personas
                .get(&p.persona_id)
                .map(|persona| persona.admission.clone())
                .ok_or_else(|| {
                    CommandError::NotFound(format!("No cognition for {}", p.persona_id))
                })?,
        };

        let forgotten = admission.forget_context(p.episode_id);

        Ok(ForgetContextResult {
            forgotten,
            engram_count: admission.engram_count(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. forget-context is proctor-driven
    // exam hygiene — Internal, never a persona toolbelt verb (no mutual neuralyzing).
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(ForgetContext::NAME, "cognition/forget-context");
        assert_eq!(ForgetContext::ACCESS, AccessLevel::Internal);
    }
}
