//! `persona/hands/adopt_capture`: adopt a persona's head from one captured request, kept warm
//! (card 9cf06ade, `cognition::capture_adoption`).
//!
//! The operator names the capture by its cursor (`cognition/playback`) and digest, and the custody
//! it is asked over (`persona/hands/custody`). The capture is read and its digest checked here;
//! her next act adopts it only where her accepted frontier and what the act sends match it byte
//! for byte, and only over that custody, else refuses with where it differs.

use std::sync::Arc;

use uuid::Uuid;

use crate::cognition::capture_adoption::{CaptureBinding, CapturedHead};
use crate::sdk_codegen::{Category, CommandError, NextMoves, NoMove, Refusal, RefusalCause};

/// Why an adoption from a capture cannot be asked.
#[derive(Debug, thiserror::Error)]
pub enum AdoptCaptureRefusal {
    #[error("no persona here answers to {reference}: {why}")]
    Unresolved { reference: String, why: String },
    #[error("persona {persona} has no mind assembled on this node")]
    NotHere { persona: Uuid },
    #[error("persona {persona} has no hands whose head could be adopted")]
    NoHands { persona: Uuid },
    #[error("persona {persona}'s capture {cursor} cannot be read: {why}")]
    Unreadable { persona: Uuid, cursor: String, why: String },
    #[error("capture {cursor} digests to {actual}, not {expected}")]
    Digest { cursor: String, expected: String, actual: String },
    #[error("capture {cursor} is persona {captured:?}'s request, not persona {persona}'s")]
    NotHers { cursor: String, persona: Uuid, captured: Option<String> },
    #[error("capture {cursor}'s request cannot be digested")]
    Undigestable {
        cursor: String,
        #[source]
        source: serde_json::Error,
    },
}

impl RefusalCause for AdoptCaptureRefusal {
    fn category(&self) -> Category {
        match self {
            AdoptCaptureRefusal::Unresolved { .. } | AdoptCaptureRefusal::NotHere { .. } | AdoptCaptureRefusal::NoHands { .. } | AdoptCaptureRefusal::Unreadable { .. } => Category::NotFound,
            AdoptCaptureRefusal::Digest { .. } | AdoptCaptureRefusal::NotHers { .. } | AdoptCaptureRefusal::Undigestable { .. } => Category::Invalid,
        }
    }
}

fn refused(cause: AdoptCaptureRefusal) -> CommandError {
    Refusal::new(cause, NextMoves::None(NoMove::NothingMoreExists)).into()
}

/// Whose head to adopt, from which capture, over which custody.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaHandsAdoptCaptureParams.ts")]
pub struct PersonaHandsAdoptCaptureParams {
    /// The persona: full UUID, 8-char short id, or name.
    pub persona: crate::identity::PersonaRef,
    /// The capture's stable cursor, from cognition/playback.
    pub cursor: String,
    /// The captured request's canonical sha256 (the replay digest).
    pub request_sha256: String,
    /// Her custody's sha256 when this adoption is asked, from persona/hands/custody.
    pub expected_custody_sha256: String,
}

/// The adoption her next act checks.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaHandsAdoptCaptureResult.ts")]
pub struct PersonaHandsAdoptCaptureResult {
    /// Her persona id.
    #[ts(type = "string")]
    pub persona_id: Uuid,
}

crate::action_command! {
    /// Adopt a persona's head from one captured request at her next act, keeping the engine's KV
    /// for it: only where her accepted frontier and what the act sends match the capture byte for
    /// byte and her custody is still the one named; otherwise refused with where it differs, her
    /// refusal and view kept.
    pub struct PersonaHandsAdoptCapture;
    name: "persona/hands/adopt_capture",
    access: AiSafe,
    params: PersonaHandsAdoptCaptureParams,
    output: PersonaHandsAdoptCaptureResult,
    run(_this, ctx, params) => {
        let registry = crate::cognition::persona_workspace::global();
        let persona_id = registry
            .resolve_persona(&params.persona)
            .map_err(|why| refused(AdoptCaptureRefusal::Unresolved { reference: params.persona.as_str().to_string(), why }))?
            .as_uuid();
        let cycle = registry.get(&persona_id).ok_or_else(|| refused(AdoptCaptureRefusal::NotHere { persona: persona_id }))?;
        let body = cycle.acting().ok_or_else(|| refused(AdoptCaptureRefusal::NoHands { persona: persona_id }))?;
        let cursor = params.cursor.clone();
        let read = tokio::task::spawn_blocking(move || read_capture(persona_id, &cursor))
            .await
            .map_err(|error| CommandError::Internal(error.to_string()))?;
        let request = read.map_err(refused)?;
        let actual = match continuum_capture::request_digest::request_sha256(&request) {
            Ok(actual) => actual,
            Err(source) => return Err(refused(AdoptCaptureRefusal::Undigestable { cursor: params.cursor, source })),
        };
        if actual != params.request_sha256 {
            return Err(refused(AdoptCaptureRefusal::Digest { cursor: params.cursor, expected: params.request_sha256, actual }));
        }
        if request.persona_id.as_deref() != Some(persona_id.to_string().as_str()) {
            return Err(refused(AdoptCaptureRefusal::NotHers { cursor: params.cursor, persona: persona_id, captured: request.persona_id }));
        }
        let by = ctx.caller.as_ref().map_or_else(|| "the substrate".to_string(), |caller| format!("{caller:?}")); // map_or_else: no caller identity is the substrate's own local code
        let binding = CaptureBinding { cursor: params.cursor, request_sha256: params.request_sha256, expected_custody_sha256: params.expected_custody_sha256 };
        body.working_memory.request_capture_adoption(&by, CapturedHead { binding, request: Arc::new(request) });
        Ok(PersonaHandsAdoptCaptureResult { persona_id })
    }
}

/// Her captured request at `cursor`, decoded through the one capture decoder.
fn read_capture(persona: Uuid, cursor: &str) -> Result<crate::ai::types::TextGenerationRequest, AdoptCaptureRefusal> {
    use crate::cognition::prompt_capture;
    let unreadable = |why: String| AdoptCaptureRefusal::Unreadable { persona, cursor: cursor.to_string(), why };
    let dir = crate::cognition::capture::fixture_dir(prompt_capture::FIXTURE_DIR).ok_or_else(|| unreadable("no capture directory resolves on this node".to_string()))?;
    let detail = prompt_capture::detail(&dir, persona, cursor).map_err(|error| unreadable(error.to_string()))?;
    prompt_capture::decode(detail).map(|call| call.request).map_err(unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name and access contract. Adoption from a capture is a verb she may
    // call on herself (AiSafe), like persona/hands/adopt.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(PersonaHandsAdoptCapture::NAME, "persona/hands/adopt_capture");
        assert_eq!(PersonaHandsAdoptCapture::ACCESS, AccessLevel::AiSafe);
    }
}
