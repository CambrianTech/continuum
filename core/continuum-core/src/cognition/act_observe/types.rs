//! Settlement wire types: the values callers read back from a driven turn.
//! Extracted verbatim from `act_observe` (pure code-motion, #386 decomposition).
//!
//! [`SettleOutcome`] is what [`drive_to_settle`](super::settle::drive_to_settle)
//! returns; [`SettleStep`] is what one [`settle_step`](super::settle::settle_step)
//! yields. Both are `pub` API surfaced by the module root.

use crate::ai::types::ToolCall;
use crate::cognition::workspace::{Decision, TurnMetrics};

/// The result of driving a mind to settlement.
pub struct SettleOutcome {
    /// The verdict the mind settled on: `Speak`/`RaiseUnprompted`/`Pass` when it
    /// settled, or the final un-driven `Act` if the external budget ran out
    /// mid-action (the grader grades that as "did not finish" — honest, never a
    /// fabricated answer).
    pub decision: Decision,
    /// The spoken text, present only when the settled decision is `Speak` /
    /// `RaiseUnprompted`. This is what an external observer (the grader, or a peer
    /// in the room) reads.
    pub spoken: Option<String>,
    /// How many actions were executed before settling.
    pub acts: usize,
    /// The final world-state, with each action's observation folded in — what the
    /// last tick perceived. Captured for replay/forensics.
    pub world_state: String,
    /// The accumulated cost of settling this task: every act→observe deliberation
    /// generation's latency + tokens, summed. `tokens_per_second()` re-derives
    /// throughput from the totals. This is the speed/latency the eval reports next
    /// to the accuracy grade — the same number a live turn could surface for the
    /// serving governor.
    pub metrics: TurnMetrics,
    /// `Some(cause)` when the settle loop stopped because the deliberation model
    /// call FAILED (timeout, 5xx, a serving lane refusing a model it isn't hosting)
    /// rather than settling on a verdict. The grader MUST treat this as an
    /// infrastructure failure — NOT a wrong answer — so an inference hiccup never
    /// corrupts the accuracy metric ([[self-improvement-is-a-control-loop]]: the
    /// reward is only as trustworthy as the metric). `None` on every settled turn.
    pub inference_error: Option<String>,
    /// File paths her acts NAMED (any `file_path`/`path` string arg on an executed
    /// call), in first-touch order, deduped. This is the turn's investigation trail
    /// as STATE — distinct from `files_changed` (what git saw): a failed edit or a
    /// read appears here and nowhere else. Exists because an N-chances retry is a
    /// FRESH turn with fresh working memory (glass-boxed 2026-08-08,
    /// benchy-sympy-22840-n4 attempt 2: the verdict said "the file you already
    /// identified" and she had no memory of identifying it — 10 acts re-deriving
    /// cse_main.py). The retry caller threads these into the next attempt's task.
    pub touched_paths: Vec<String>,
    /// The ACTIVITY this turn belonged to — its room (non-nil, witnessed at burst
    /// construction, #425). Carried on the outcome so the curriculum stream can
    /// attribute a lived turn to the activity it happened in; before this, the
    /// room survived only as prose inside `world_state`'s header.
    pub room: uuid::Uuid,
}

impl SettleOutcome {
    /// An infra-failure outcome: no verdict was reached because the deliberation
    /// call failed OR was aborted by a watchdog (e.g. a per-task deadline in the
    /// eval loop). The grader keys on `inference_error.is_some()` to score this a
    /// NAMED infrastructure failure — never a wrong answer — so a serving wedge
    /// never masquerades as a capability miss ([[self-improvement-is-a-control-loop]]).
    /// Zeroed metrics/acts because none accrued meaningfully. `TurnMetrics: Default`.
    pub fn infra_failure(room: uuid::Uuid, cause: impl Into<String>) -> Self {
        Self {
            decision: Decision::pass(),
            spoken: None,
            acts: 0,
            world_state: String::new(),
            metrics: TurnMetrics::default(),
            inference_error: Some(cause.into()),
            touched_paths: Vec::new(),
            room,
        }
    }
}

/// The outcome of ONE settlement [`settle_step`].
#[derive(Debug)]
pub enum SettleStep {
    /// She settled on speech (`Speak`/`RaiseUnprompted`) — the prose turn an
    /// observer (a peer, or the grader) reads.
    Spoke(String),
    /// She reached for her hands AND the act was carried out; the result is admitted
    /// as memory + a stamped proprioception trace. The caller re-perceives next
    /// (live: next metronome tick; eval: next loop step). The calls+intent ride
    /// along so a caller that paces acting (the eval budget) can report the final
    /// Act if its budget runs out on the following step.
    Acted {
        calls: Vec<ToolCall>,
        intent: String,
    },
    /// She decided to act but the caller's budget said no this step (`may_act =
    /// false`) — the act was NOT executed. Only the eval driver passes `may_act =
    /// false`; the live heartbeat always permits its one act, so it never sees this.
    WouldAct {
        calls: Vec<ToolCall>,
        intent: String,
    },
    /// She chose silence (`Pass`) — honored as a turn that produces no utterance.
    /// `reason` carries her OWN words for why (from `Decision::Pass`), so the
    /// held-work edge can tell a gradeable *done* from a *blocker* from a
    /// substrate-gap *nothing*; `None` for a bare pass.
    Passed { reason: Option<String> },
    /// She reached for an act that could NOT be carried out (no hands / executor
    /// error). No utterance; the intent rides along for honest logging/grading.
    ActUnfulfilled {
        calls: Vec<ToolCall>,
        intent: String,
    },
    /// The deliberation model call itself FAILED — a timeout, a 5xx, or the serving
    /// lane refusing a model it isn't hosting (the swept-model bug). NO verdict was
    /// produced. This is NOT a `Passed`: a failed model is not a chosen silence
    /// ([[fallbacks-are-illegal-fail-loud]]). Every caller surfaces the cause LOUD —
    /// the command reports `inferenceFailed`, the live heartbeat logs + retries next
    /// tick, the eval grades it an infra failure — never a serene no-op that hides a
    /// broken lane. `error` names the cause verbatim from the adapter.
    InferenceFailed { error: String },
}

impl SettleStep {
    /// Project a fully-driven [`SettleOutcome`] back onto the SAME `SettleStep` the
    /// live heartbeat already handles — so a DIRECTED live turn can `drive_to_settle`
    /// (converge to an answer in-turn, exactly as the eval path does) and feed the
    /// result through the one existing turn handler, no parallel match.
    ///
    /// The mapping is total and lossless for the live handler's purposes:
    ///   • `inference_error` present → `InferenceFailed` (a failed model is never a
    ///     chosen silence — [[fallbacks-are-illegal-fail-loud]]);
    ///   • `Speak`/`RaiseUnprompted` → `Spoke` (the prose reaches the room);
    ///   • `Act` → `Acted` — the drive spent its whole act budget without settling on
    ///     speech; the results are already in memory, so the live handler `continue`s
    ///     and the metronome re-perceives next tick (the honest long-tail degrade, and
    ///     the ONLY case a directed turn still leans on the tick loop);
    ///   • `Pass` → `Passed` (she declined in her own words).
    /// `drive_to_settle` collapses `WouldAct`/`ActUnfulfilled` into `Decision::Act`
    /// before returning, so those never surface here — an over-budget or un-carried
    /// act both land on `Acted`, which the live handler treats as "re-perceive next
    /// tick", the correct move either way.
    pub fn from_settled(outcome: SettleOutcome) -> (SettleStep, Option<TurnMetrics>) {
        let metrics = Some(outcome.metrics);
        if let Some(error) = outcome.inference_error {
            return (SettleStep::InferenceFailed { error }, metrics);
        }
        let step = match outcome.decision {
            Decision::Speak { text } | Decision::RaiseUnprompted { text } => {
                SettleStep::Spoke(text)
            }
            Decision::Act { calls, intent } => SettleStep::Acted { calls, intent },
            Decision::Pass { reason } => SettleStep::Passed { reason },
        };
        (step, metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// what this catches: `SettleStep::from_settled` mis-projecting a driven
    /// `SettleOutcome` onto the live turn handler — the seam that lets a DIRECTED
    /// live turn `drive_to_settle` and feed the ONE existing turn match (no parallel
    /// handler). A regression here would route a settled Speak to silence, hide an
    /// inference failure behind a serene Pass ([[fallbacks-are-illegal-fail-loud]]),
    /// or drop an over-budget Act instead of re-perceiving next tick.
    fn outcome_with(decision: Decision, inference_error: Option<String>) -> SettleOutcome {
        SettleOutcome {
            spoken: match &decision {
                Decision::Speak { text } | Decision::RaiseUnprompted { text } => Some(text.clone()),
                _ => None,
            },
            decision,
            acts: 0,
            world_state: String::new(),
            metrics: TurnMetrics::default(),
            inference_error,
            touched_paths: Vec::new(),
            room: uuid::Uuid::from_u128(7),
        }
    }

    #[test]
    fn from_settled_projects_every_terminal_outcome_onto_the_live_handler() {
        // Speak → Spoke (the prose reaches the room).
        let (step, m) = SettleStep::from_settled(outcome_with(
            Decision::Speak {
                text: "hello".into(),
            },
            None,
        ));
        assert!(matches!(step, SettleStep::Spoke(t) if t == "hello"));
        assert!(m.is_some(), "metrics always carry through");

        // RaiseUnprompted also speaks — initiative is still an utterance.
        let (step, _) = SettleStep::from_settled(outcome_with(
            Decision::RaiseUnprompted {
                text: "idea".into(),
            },
            None,
        ));
        assert!(matches!(step, SettleStep::Spoke(t) if t == "idea"));

        // Budget-spent Act → Acted (results already in memory; live handler
        // re-perceives next tick — the honest long-tail degrade).
        let (step, _) = SettleStep::from_settled(outcome_with(
            Decision::Act {
                calls: vec![],
                intent: "kept gathering".into(),
            },
            None,
        ));
        assert!(matches!(step, SettleStep::Acted { .. }));

        // Pass → Passed (chosen silence, honored) — reason carried through.
        let (step, _) = SettleStep::from_settled(outcome_with(Decision::pass(), None));
        assert!(matches!(step, SettleStep::Passed { reason: None }));

        // A reasoned pass carries her words through the projection.
        let (step, _) = SettleStep::from_settled(outcome_with(
            Decision::Pass {
                reason: Some("done — patch ready".into()),
            },
            None,
        ));
        assert!(matches!(step, SettleStep::Passed { reason: Some(r) } if r == "done — patch ready"));

        // inference_error present → InferenceFailed, REGARDLESS of decision — a
        // failed model is never a chosen silence.
        let (step, _) = SettleStep::from_settled(outcome_with(
            Decision::pass(),
            Some("lane refused model".into()),
        ));
        assert!(
            matches!(step, SettleStep::InferenceFailed { error } if error == "lane refused model"),
            "an inference failure must surface LOUD, never collapse to Passed"
        );
    }
}
