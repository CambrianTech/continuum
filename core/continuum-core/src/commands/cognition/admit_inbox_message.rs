//! `cognition/admit-inbox-message` — run a persona's admission gate over one inbox
//! message (typed, dep-holding).
//!
//! Converts the JSON-transport [`InboxMessageRequest`](crate::ipc::InboxMessageRequest)
//! to the domain [`InboxMessage`](crate::persona::InboxMessage) via the one canonical
//! `to_inbox_message()` seam, then runs the persona's admission gate
//! ([`admit`](crate::persona::admission_state)) — recording side-effects (admitted
//! engram → store, content_hash → dedup, AIRC event_id → replay-protection). Returns
//! the typed [`AdmissionDecision`](crate::persona::AdmissionDecision) plus funnel
//! telemetry (engram count, trace seam count). Captures the owning module's
//! [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! A [`CognitionTrace`](crate::persona::trace::CognitionTrace) is built and threaded
//! through so the seam count comes back in the response (the TS/IPC caller surfaces
//! admission-funnel telemetry). Read-only persona lookup: fails loud with `NotFound`
//! when the persona has no cognition engine (created via `cognition/create-engine`).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::ipc::InboxMessageRequest;
use crate::modules::cognition::CognitionState;
use crate::persona::trace::CognitionTrace;
use crate::persona::AdmissionDecision;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/AdmitInboxMessageParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct AdmitInboxMessageParams {
    /// Persona whose admission gate runs.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// The message to admit (JSON-transport shape, string UUIDs + string enums).
    pub message: InboxMessageRequest,
}

/// The admission outcome plus admission-funnel telemetry.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/AdmitInboxMessageResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct AdmitInboxMessageResult {
    /// The typed admission decision (Admit / Drop / Quarantine).
    pub decision: AdmissionDecision,
    /// Total engrams currently in the persona's admitted store.
    #[ts(type = "number")]
    pub engram_count: usize,
    /// Number of trace seams the admission run recorded (funnel telemetry).
    #[ts(type = "number")]
    pub trace_seam_count: usize,
}

crate::action_command! {
    /// Run the persona's admission gate over an inbox message and record its
    /// side-effects. Host-invoked, typically once per drained inbox frame.
    pub struct AdmitInboxMessage { state: Arc<CognitionState> }
    name: "cognition/admit-inbox-message",
    access: Internal,
    params: AdmitInboxMessageParams,
    output: AdmitInboxMessageResult,
    run(this, _ctx, p) => {
        let inbox_msg = p.message.to_inbox_message().map_err(CommandError::Invalid)?;

        let persona = this
            .state
            .personas
            .get(&p.persona_id)
            .ok_or_else(|| CommandError::NotFound(format!("No cognition for {}", p.persona_id)))?;

        // Build a trace so the seam count comes back in the response (the TS/IPC
        // caller surfaces admission-funnel telemetry). The in-process inline gate
        // passes None because it doesn't propagate the trace anywhere.
        let mut trace = CognitionTrace::new();
        let decision = persona
            .admission
            .admit(&inbox_msg, Some(&mut trace))
            .map_err(|err| CommandError::Invalid(format!("admission error: {err}")))?;

        Ok(AdmitInboxMessageResult {
            decision,
            engram_count: persona.admission.engram_count(),
            trace_seam_count: trace.seam_count(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. admit-inbox-message is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(AdmitInboxMessage::NAME, "cognition/admit-inbox-message");
        assert_eq!(AdmitInboxMessage::ACCESS, AccessLevel::Internal);
    }
}
