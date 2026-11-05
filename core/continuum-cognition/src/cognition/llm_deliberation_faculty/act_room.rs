//! An act's output room: what she may write, as ONE claim of think plus answer, priced against her
//! prompt at its measured density behind the engine's exact count.
//!
//! The live defect this owns (the M5, 2026-11-04 21:31Z-01:09Z, build a95afe3d1, 414 acts): 302
//! acts ended at `max_tokens` exactly, caps of 72-89 tokens while her window less the engine's
//! prompt count left a median 2,517 free (min 1,662). Every cut was reasoning only, so her thought
//! was cut at its first sentence and she looped (83 consecutive cuts). Three causes, each with its
//! owner now: the prompt was charged at its raw estimate while the engine counted 0.83 of it
//! ([`PromptCalibration::charge`], now below the estimate only behind the engine's count); the
//! reply's floor was her answer alone, clipped to what the other floors left ([`reply_claim`], now
//! her think plus answer, refused as the typed `IrreducibleShortfall` when it does not fit); and the
//! think shared `max_tokens` under a fixed 1,024 budget that never applied below a 1,024 cap
//! ([`act_reasoning_budget`], now her room past her measured answer).

use super::*;
use crate::cognition::working_set::ActRoom;

/// The `max_tokens` a turn's request carries, for the view it is sent with. Pure.
///
/// The two output channels have opposite natures, so each has one owner. The THINK runs away (a
/// thinking model fills whatever room it gets), so it is bounded by its reasoning budget on the
/// wire ([`act_reasoning_budget`]), never by `max_tokens`. The ANSWER of an act is payload: a tool
/// call carrying a file ends by itself, and its size is the file's, not her past answers'. So an
/// act's cap is the room this view's window physically leaves after its charged prompt, less the
/// one token the strict `prompt + reply < window` admission needs (`serving_guard`, and the
/// request's own `admission_window` when the prompt was priced below its estimate); the slot's
/// window grant bounds the KV, and nothing is reserved by the cap. A pass keeps its measured grant.
///
/// Why (the M5, 2026-11-04): an act's cap was her measured reply grant, sized from read-sized
/// acts (Aiko 676-701 tokens, Aris 809, Benchy 518-690 on 31-43k windows), so every write-shaped
/// call arrived truncated (`__malformed_tool_arguments`, finish `length`, 22 since 11:00Z): a
/// downward measurement with no upward test. The reply's claim still sizes her view (the
/// allocation reserves `reserve`); only the request's cap changes, and never below that reserve.
pub(super) fn request_allowance(kind: TurnKind, reserve: u32, window: u32, charged_prompt: usize) -> u32 {
    let reserve = reserve.max(1);
    match kind {
        TurnKind::Pass => reserve,
        TurnKind::Act => {
            // The prompt is clamped to the window before the cast, so the cast cannot truncate.
            let prompt = charged_prompt.min(window as usize) as u32;
            window.saturating_sub(prompt).saturating_sub(1).max(reserve)
        }
    }
}

/// The think's bound inside an act's cap: the room her claim was granted past her MEASURED answer
/// (the call and its serialization), so the answer always keeps its measured room; an unmeasured
/// act keeps the answer's share of its grant, the rule every other turn has
/// (`request_body::deliberation_reasoning_budget`). Never a fixed count. `None` when the grant
/// leaves the think nothing.
pub(super) fn act_reasoning_budget(room: ActRoom, reserve: u32) -> Option<u32> {
    match room {
        ActRoom::Needs { answer, .. } => Some(reserve.saturating_sub(answer)).filter(|budget| *budget > 0),
        ActRoom::Unmeasured => crate::inference::request_body::deliberation_reasoning_budget(u64::from(reserve))
            .and_then(|budget| u32::try_from(budget).ok()),
    }
}

impl LlmDeliberationFaculty {
    /// Stamp an act's request with its output room: the think's bound inside its cap and, when the
    /// prompt was priced below its estimate, the strict admission guard that makes that sound (the
    /// engine counts the finalized body and refuses at its exact count). Returns the receipt the
    /// request's allocation carries; a pass is left as its grant made it.
    pub(super) fn stamp_output_room(
        &self,
        kind: TurnKind,
        view: &DeliberationPromptView,
        calibration: PromptCalibration,
        window: u32,
        request: &mut TextGenerationRequest,
    ) -> Option<crate::ai::provenance::OutputRoom> {
        if kind != TurnKind::Act {
            return None;
        }
        let cap = request.max_tokens?;
        request.reasoning_budget = act_reasoning_budget(view.act_room, view.completion_reserve);
        request.admission_window = (calibration.admission == crate::ai::provenance::PromptAdmission::EngineCounted)
            .then_some(window);
        let (density_estimated, density_measured) = calibration.density();
        let room = crate::ai::provenance::OutputRoom {
            model: request.model.clone().unwrap_or_default(), // unwrap_or_default: a request naming no model is the adapter's default, said as empty
            cap,
            reasoning_budget: request.reasoning_budget,
            need: view.act_room.needed(),
            density_estimated,
            density_measured,
            admission: calibration.admission,
        };
        continuum_base::probe!(
            class = "delib.act.output_room",
            persona = %self.persona_name,
            model = %room.model,
            build = env!("CONTINUUM_BUILD_GIT_SHA"),
            window,
            cap,
            reasoning_budget = room.reasoning_budget.map_or(0, u64::from), // map_or: 0 on the receipt = the purpose's share rule
            need = room.need.map_or(0, u64::from), // map_or: 0 on the receipt = unmeasured
            density_estimated,
            density_measured,
            admission = ?room.admission,
            "an act's output room as sent: think plus answer in one cap, priced at her measured density"
        );
        Some(room)
    }

    /// An act refused because her measured output need does not fit her window: never sent, her
    /// need already recorded with her window's floor (`prompt/view.rs`), parked on readiness so the
    /// same window is never retried unchanged.
    pub(super) fn output_room_refused(
        &self,
        short: crate::cognition::workspace::IrreducibleShortfall,
        readiness: Arc<crate::cognition::workspace::CapacityReadiness>,
    ) -> Contribution {
        continuum_base::probe!(
            class = "delib.act.output_shortfall",
            persona = %self.persona_name,
            needed = short.needed,
            window = short.window,
            "her required floors and measured output need do not fit her window: the act is not sent"
        );
        Contribution::output_room_refused(short).with_capacity_readiness(readiness)
    }
}
