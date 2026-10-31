//! Each resident's settled turns, fed to her service ledger (card 1c157668).
//!
//! Kimi ran her loop on the 5090 for 17 hours on 2026-10-26 and every turn was refused
//! before inference; Sahar stalled from 20:05Z the same way. Each refusal was probed, and
//! nothing counted them per mind. This is the count: every driven turn ends at ONE seat
//! (`act_observe::settle::drive_with_input`), and that seat folds the turn here, refusals
//! included, so a mind asked for work and given none reads as stalled. The judging and the
//! publishing live in the surface (`modules::citizen_health`), which closes a window per
//! resident on its own tick; this module only keeps the facts, like `health_ledger`.

use std::sync::LazyLock;

use continuum_service::resident::{Completion, Refusal, ResidentLedger, ResidentWindow, Turn};
use continuum_service::sample::Window;
use dashmap::DashMap;
use uuid::Uuid;

use crate::ai::provenance::{GenerationOutcome, GenerationReceipt};
use crate::cognition::act_observe::SettleOutcome;
use crate::cognition::workspace::{CapacityRefusal, TurnMetrics};

static LEDGERS: LazyLock<DashMap<Uuid, ResidentLedger>> = LazyLock::new(DashMap::new);

/// Fold one driven turn of `persona` that took `wall_ms`, settle to settle.
pub fn note_settled(persona: Uuid, settled: &SettleOutcome, wall_ms: u64) {
    let turn = turn_of(
        settled.capacity_refusal.as_ref(),
        settled.inference_error.is_some(),
        settled.deferred_before_dispatch,
        &settled.generation_receipts,
        &settled.metrics,
        settled.acts,
        wall_ms,
    );
    LEDGERS.entry(persona).or_default().note(turn);
}

/// One generation was served to `persona`, now. Counted where the response arrives, settled or
/// not, so a long turn that keeps serving reads as progress in every window it spans.
pub fn note_generation(persona: Uuid) {
    LEDGERS.entry(persona).or_default().note_generation();
}

/// Her loop stopped with a capacity refusal still parked: she asks nothing now.
pub fn unpark(persona: Uuid) {
    if let Some(mut ledger) = LEDGERS.get_mut(&persona) {
        ledger.unpark();
    }
}

/// Close every resident's window `span`: the turns settled since the last close, who is
/// still parked, and whose turn has been in flight since before the window opened. A mind
/// whose first turn of this boot never settles has no ledger yet, so the turns in flight
/// open one: Eitan was in flight on slot 1 for 4,500 s in ONE turn (enrico, 2026-10-26), and a window
/// that only counted settled turns read him as idle.
pub fn close_windows(span: Window) -> Vec<(Uuid, ResidentWindow)> {
    let in_flight = crate::cognition::turn_ledger::in_flight_starts();
    for (persona, _) in &in_flight {
        LEDGERS.entry(*persona).or_default();
    }
    LEDGERS
        .iter_mut()
        .map(|mut entry| {
            let persona = *entry.key();
            let since = in_flight.iter().find(|(p, _)| *p == persona).map(|(_, start)| *start);
            (persona, entry.value_mut().close(span, since))
        })
        .collect()
}

/// What one driven turn asked of the engine and got. A turn that served at least one
/// generation completed work, whatever ended it; the refusal that ended it (if any) is named
/// beside that, so a mind that worked and then parked is both.
fn turn_of(
    capacity: Option<&CapacityRefusal>,
    inference_failed: bool,
    deferred_before_dispatch: bool,
    receipts: &[GenerationReceipt],
    metrics: &TurnMetrics,
    acts: usize,
    wall_ms: u64,
) -> Turn {
    let served: Vec<&str> = receipts
        .iter()
        .filter_map(|r| match &r.outcome {
            GenerationOutcome::Served { model, .. } => Some(model.as_str()),
            GenerationOutcome::Faulted { .. } => None,
        })
        .collect();
    let completion = served.last().map(|model| Completion {
        acts: saturating_u32(acts),
        wall_ms,
        prompt_tokens: metrics.input_tokens,
        output_tokens: metrics.output_tokens,
        cached_tokens: metrics.cached_tokens,
        prefilled_tokens: metrics.prefill_tokens,
        model: (*model).to_string(),
    });
    let refusal = match capacity {
        Some(refusal) => Some(Refusal::PromptCapacity {
            required_tokens: saturating_u32(refusal.required_tokens),
            budget_tokens: saturating_u32(refusal.budget_tokens),
            context_window: refusal.readiness.as_ref().map(|r| r.observed_window()),
        }),
        None if inference_failed => Some(Refusal::InferenceFailed),
        None if deferred_before_dispatch => Some(Refusal::LaneStarved),
        None => None,
    };
    Turn { completion, refusal }
}

fn saturating_u32(n: usize) -> u32 {
    u32::try_from(n).map_or(u32::MAX, |n| n) // a count past u32::MAX saturates; no turn reaches it
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::workspace::PromptCapacityError;

    fn served(model: &str) -> GenerationReceipt {
        GenerationReceipt {
            submitted_request_id: "r".into(),
            outcome: GenerationOutcome::Served { model: model.into(), provider: "llamacpp".into(), provider_request_id: None },
            capture: None,
            genes: Vec::new(),
            gene_scales: Vec::new(),
            allocation: None,
        }
    }

    // what this catches: Kimi's turn at the seat. A capacity refusal before any generation is a
    // refusal with its numbers and no completion (demand, nothing done), never a quiet pass; a
    // turn that served generations and then parked is a completion AND the parking refusal.
    #[test]
    fn a_refusal_before_inference_is_demand_with_no_completion_and_keeps_its_numbers() {
        let kimi = CapacityRefusal::from(PromptCapacityError { required_tokens: 26_101, budget_tokens: 17_829 });
        let metrics = TurnMetrics::default();
        let turn = turn_of(Some(&kimi), false, true, &[], &metrics, 0, 2_000);
        assert_eq!(turn.completion, None);
        assert_eq!(
            turn.refusal,
            Some(Refusal::PromptCapacity { required_tokens: 26_101, budget_tokens: 17_829, context_window: None })
        );

        let worked = turn_of(Some(&kimi), false, false, &[served("qwen3.6-35b-a3b")], &metrics, 2, 90_000);
        assert_eq!(worked.completion.map(|c| (c.acts, c.model)), Some((2, "qwen3.6-35b-a3b".to_string())));
        assert!(worked.refusal.is_some());

        let starved = turn_of(None, false, true, &[], &metrics, 0, 600_000);
        assert_eq!((starved.completion, starved.refusal), (None, Some(Refusal::LaneStarved)));
    }
}
