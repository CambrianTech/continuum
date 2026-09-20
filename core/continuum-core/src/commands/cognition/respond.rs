//! `cognition/respond` — the persona-response pipeline entry point (typed, dep-holding).
//!
//! The single external IPC command for persona response. Replaces the old TS
//! `PersonaResponseGenerator` orchestration. Given a [`Signal`] (the host's raw event) and
//! a [`PersonaContext`] (the persona's stable per-turn state), it projects them into a
//! `RespondInput`, runs the hot-path admission gate + recall surface so the persona's engram
//! store grows and the model sees its own recent memory, then calls
//! [`persona::response::respond`](crate::persona::response::respond) — which builds the
//! prompt, runs inference, strips/emits `<think>` blocks, and returns the visible speech
//! (or `Silent`).
//!
//! Captures the owning module's [`CognitionState`](crate::modules::cognition::CognitionState)
//! (same dep-holding shape as the other stateful cognition commands). The admission gate +
//! recall read live per-persona state through it.
//!
//! Wire note: the params ARE a [`RespondRequest`] — the whole `{ signal, personaContext }`
//! payload deserializes in one step, matching the legacy arm that read `p.json("signal")` +
//! `p.json("personaContext")` separately. No `recipe` field: recipes are host-walked JSON,
//! not a cognition concern; an old-shape caller's extra `recipe` key is ignored (serde drops
//! unknown fields).
//!
//! Fail-loud note: an unusable signal kind for the chat-shaped projection (a `FrameUpdate` /
//! `CodeContext` routed to a chat-cognition step) is a host bug — `build_respond_input`
//! surfaces it as `Err`, mapped here to `CommandError::Invalid`, never silently-wrong
//! cognition output. Inference/parse failures fail loud as `CommandError::Internal`. The
//! admission gate is forensic-not-destructive: a persona with no `AdmissionState` (engine
//! never created) is skipped, not a chat-blocking error — the persona still responds, it
//! just doesn't grow memory until `cognition/create-engine` runs.
//!
//! `access: Internal` — the host chat path calls this once per candidate persona; the
//! cognition analysis cache means the shared analysis runs once per message even across M
//! responders. Not a persona toolbelt verb.

use std::sync::Arc;

use crate::modules::cognition::{run_inline_admission_gate, CognitionState};
use crate::persona::cognition_io::{build_respond_input, RespondRequest};
use crate::persona::response::{respond, PersonaResponse};
use crate::runtime;
use crate::sdk_codegen::CommandError;

/// Cap on recalled engrams injected into the prompt's `[Recent Memory]` block. A budget
/// policy: enough to ground the persona in continuity ("yes, the user mentioned teal
/// earlier") without dominating the prompt. Future tunable via per-persona `AdmissionConfig`;
/// v1 is a hardcoded sensible default (transplanted verbatim from the legacy arm).
const RECALL_LIMIT: usize = 5;

crate::action_command! {
    /// Run the persona-response pipeline for one persona against one signal: project
    /// (signal, context) → input, run the admission gate + recent-memory recall, then build
    /// the prompt, run inference, and return the visible speech (or Silent). The single
    /// entry point the host chat path calls per candidate responder; not a persona toolbelt
    /// verb.
    pub struct Respond { state: Arc<CognitionState> }
    name: "cognition/respond",
    access: Internal,
    params: RespondRequest,
    output: PersonaResponse,
    run(this, _ctx, req) => {
        let RespondRequest { signal, persona_context: ctx } = req;

        let mut input =
            build_respond_input(&signal, &ctx).map_err(CommandError::Invalid)?;

        // ── Hot-path admission gate (continuum#1211 PR-1) ──
        // Run admission BEFORE inference so the persona's engram store grows from real chat
        // turns. Forensic-not-destructive: a missing AdmissionState (persona never had
        // cognition/create-engine called) is logged and skipped, NOT a chat-blocking error.
        run_inline_admission_gate(&this.state, &signal, &ctx);

        // ── Hot-path recall surface (continuum#1211 PR-2) ──
        // Populate input.recalled_engrams with the persona's most-recently-admitted memory so
        // prompt_assembly can render a [Recent Memory] block. Empty when the persona has no
        // AdmissionState (same forensic-skip path as the gate) or no admitted engrams yet
        // (cold-start) — both normal early-life states, unchanged from pre-PR-2 behavior.
        if let Some(persona) = this.state.personas.get(&ctx.persona_id) {
            input.recalled_engrams = persona
                .admission
                .recall_recent(RECALL_LIMIT)
                .into_iter()
                .map(|e| e.content)
                .collect();
        }

        // Diagnostic: log what media survived the projection. Vision routing was failing
        // 2026-04-21 and this stays as the in-flight tap to confirm media shape arriving at
        // cognition matches what the host believed it sent.
        if !input.message_media.is_empty() {
            let shape: Vec<String> = input
                .message_media
                .iter()
                .map(|item| {
                    let has_b64 = item.base64.as_deref().map(|s| s.len()).unwrap_or(0);
                    let has_desc = item.description.is_some();
                    format!("{}(b64={}, desc={})", item.item_type, has_b64, has_desc)
                })
                .collect();
            runtime::logger("cognition").info_fmt(format_args!(
                "cognition/respond: message_media count={} shapes=[{}]",
                input.message_media.len(),
                shape.join(", ")
            ));
        }

        respond(input).await.map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. respond is the host-driven persona
    // response entry point, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(Respond::NAME, "cognition/respond");
        assert_eq!(Respond::ACCESS, AccessLevel::Internal);
    }
}
