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
/// request's own `admission`, re-checked against her grant at dispatch); the slot's
/// window grant bounds the KV, and nothing is reserved by the cap. A pass keeps its measured grant.
///
/// Why (the M5, 2026-11-04): an act's cap was her measured reply grant, sized from read-sized
/// acts (Aiko 676-701 tokens, Aris 809, Benchy 518-690 on 31-43k windows), so every write-shaped
/// call arrived truncated (`__malformed_tool_arguments`, finish `length`, 22 since 11:00Z): a
/// downward measurement with no upward test. The reply's claim still sizes her view (the
/// allocation reserves `reserve`); only the request's cap changes, and never below that reserve.
///
/// `window` is already the smaller of her LEASED window and the physical sequence window (her
/// grant never exceeds what one sequence holds, `window_grant::PoolGrants::per_sequence_cap`; off
/// the pool it is the lane's own). The protocol margin is the fork's, not a guess: the engine's
/// count (`/input_tokens`, `handle_count_tokens`) already includes the template's generation
/// prompt and BOS (`tokenize_mixed(.., add_special = true, ..)`), and its admission refuses a
/// sequence that leaves no free cell (`server-context.cpp`: `n_tokens() >= n_ctx`), so the margin
/// is that one cell. The reply's grant is the MINIMUM room to keep, never a second maximum: when
/// the room is below it the act is refused typed ([`ShortRoom`]) and re-rendered, never sent with
/// a cap past either bound (the old `.max(reserve)` sent 80 tokens past a prompt that already
/// overflowed, the M5 2026-11-04).
pub(super) fn request_allowance(kind: TurnKind, reserve: u32, window: u32, charged_prompt: usize) -> Result<u32, ShortRoom> {
    let reserve = reserve.max(1);
    match kind {
        TurnKind::Pass => Ok(reserve),
        TurnKind::Act => {
            // The prompt is clamped to the window before the cast, so the cast cannot truncate.
            let prompt = charged_prompt.min(window as usize) as u32;
            let room = window.saturating_sub(prompt).saturating_sub(1); // the fork's one free cell (above)
            match room >= reserve {
                true => Ok(room),
                false => Err(ShortRoom { room, reserve, window, prompt }),
            }
        }
    }
}

/// The room an act's prompt leaves is below the reply's grant: refused, re-rendered, never sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ShortRoom {
    pub(super) room: u32,
    pub(super) reserve: u32,
    pub(super) window: u32,
    pub(super) prompt: u32,
}

/// The think's bound inside a turn's cap: the room her reply claim was granted past her MEASURED
/// answer (an act's call and its serialization, a pass's sentence), so the answer always keeps its
/// measured room; unmeasured, past the smallest usable reply
/// ([`LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS`], the reply claim's own floor). Never a
/// fixed count or a share of the cap, and never unbounded: a grant that leaves the think nothing
/// is a budget of ZERO (no thinking; the fork forces the end tag at once).
pub(super) fn act_reasoning_budget(room: ActRoom, reserve: u32) -> crate::ai::types::ThinkBudget {
    let answer = match room {
        ActRoom::Needs { answer, .. } => answer,
        ActRoom::Unmeasured => LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS,
    };
    crate::ai::types::ThinkBudget(reserve.saturating_sub(answer))
}

impl LlmDeliberationFaculty {
    /// The terms a turn is dispatched under (`DispatchTerms`): the admission it was rendered under
    /// (its window, the grant it was priced against, how its prompt was priced: the engine enforces
    /// all three at dispatch, after any lane wait) and the think's bound inside its cap. With an
    /// act's output-room receipt, which its allocation carries.
    pub(super) fn dispatch_terms(
        &self,
        kind: TurnKind,
        view: &DeliberationPromptView,
        calibration: PromptCalibration,
        (window, lease): (u32, Option<crate::capacity::window_grant::GrantLease>),
        request: &TextGenerationRequest,
    ) -> (crate::ai::types::DispatchTerms, Option<crate::ai::provenance::OutputRoom>) {
        let terms = crate::ai::types::DispatchTerms {
            admission: Some(crate::ai::types::RequestAdmission { window, lease, priced: calibration.admission }),
            think: Some(act_reasoning_budget(view.act_room, view.completion_reserve)),
        };
        let Some(cap) = request.max_tokens.filter(|_| kind == TurnKind::Act) else {
            return (terms, None);
        };
        let (density_estimated, density_measured) = calibration.density();
        let room = crate::ai::provenance::OutputRoom {
            model: request.model.clone().unwrap_or_default(), // unwrap_or_default: a request naming no model is the adapter's default, said as empty
            cap,
            reasoning_budget: terms.think.map(|think| think.0),
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
            reasoning_budget = room.reasoning_budget.map_or(0, u64::from), // map_or: 0 on the receipt = no bound on the think
            need = room.need.map_or(0, u64::from), // map_or: 0 on the receipt = unmeasured
            density_estimated,
            density_measured,
            admission = ?room.admission,
            "an act's output room as sent: think plus answer in one cap, priced at her measured density"
        );
        (terms, Some(room))
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
