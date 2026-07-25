//! `cognition/dream-now` — the factory flywheel button: force ONE dream pass for a
//! persona NOW instead of waiting for the governor's material-driven tick.
//!
//! The dream region is deliberately demand-paced for user machines (dreams when
//! undigested experience accrues, sleeps otherwise, pressure-gated). At the factory
//! we run the flywheel 24/7: work lands experience → THIS forces the consolidation +
//! supersession pass → re-measure within the hour, no overnight wait. It drives the
//! SAME live region instance the governor drives (one region, two drivers), so the
//! per-persona single-flight guard holds — a forced pass while a dream is already
//! running is a cheap no-op, never a double-dream.
//!
//! `access: Internal` — operator/proctor-driven schedule control, not a persona
//! toolbelt verb (personas do not force each other's sleep).

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/DreamNowParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct DreamNowParams {
    /// Persona whose dream pass is being forced.
    #[ts(type = "string")]
    pub persona_id: Uuid,
}

/// What the forced tick did. The dream pass itself runs async on its own task
/// (results land through `admit_reflection` + the `hippocampus.supersede` /
/// `persona.dream.pass_complete` probes); this is the launch verdict.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/DreamNowResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct DreamNowResult {
    /// True when a dream pass launched (or decay ran) — the region found the
    /// persona resident. False only when she had no live reflective surface.
    pub ticked: bool,
    /// Human-readable note on what the tick decided (launched / sated / not resident).
    pub note: String,
}

crate::action_command! {
    /// Force one dream-consolidation pass for a persona now (factory flywheel).
    pub struct DreamNow { state: Arc<CognitionState> }
    name: "cognition/dream-now",
    access: Internal,
    params: DreamNowParams,
    output: DreamNowResult,
    run(this, _ctx, p) => {
        let _ = &this.state; // command host plumbing; the region is the authority
        let Some(region) = crate::cognition::dream_consolidation::global() else {
            return Err(CommandError::Internal(
                "no live dream region in this process — the substrate wiring \
                 (ipc start_server) has not installed it"
                    .to_string(),
            ));
        };
        let outcome = region.consolidate(p.persona_id).await;
        let note = match &outcome.cadence_hint {
            Some(h) => format!(
                "tick ran; cadence hint {h:?} — Sleep = sated (no undigested \
                 experience), otherwise a pass launched (watch \
                 persona.dream.pass_complete / hippocampus.supersede probes)"
            ),
            None => "tick ran; dream pass launched on its own task — watch \
                     persona.dream.pass_complete / hippocampus.supersede probes"
                .to_string(),
        };
        Ok(DreamNowResult { ticked: true, note })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. dream-now is operator
    // schedule-control (factory flywheel) — Internal, never a persona toolbelt
    // verb; and the wire name must keep routing to this command.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(DreamNow::NAME, "cognition/dream-now");
        assert_eq!(DreamNow::ACCESS, AccessLevel::Internal);
    }
}
