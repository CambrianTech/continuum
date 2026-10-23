//! The `STAGING → READY` gate (#442): a round is never staged into a lane that cannot work it.
//!
//! # The defect, and why it is not "a missing check"
//!
//! `benchmark/dispatch` has called [`await_ready_serving`] for a long time — and used the
//! answer only to SIZE the auto-fire cap. When the lane was dead it set the cap to zero and
//! **staged the cards anyway**: a full round of work posted to a board, kickoffs sent, and
//! nothing on the box able to decode a single token. The round then sat there looking
//! dispatched. That is #455 stated exactly — *"hosting is correctly blocked while the lane
//! thrashes, and we stage work into the gap"*.
//!
//! So the fix is not to add a probe. The probe was there and correct. The fix is to make its
//! answer LOAD-BEARING: not-ready is a STATE the round stops at, not a parameter that quietly
//! degrades the round into an empty one
//! ([ROUND-LIFECYCLE-AS-RECIPE-OWNED-STATE-MACHINE.md](../../../../docs/architecture/ROUND-LIFECYCLE-AS-RECIPE-OWNED-STATE-MACHINE.md) §4:
//! *"dispatch refuses to stage into a not-ready room. That is the `STAGING → READY` gate,
//! expressed as a state instead of a check."*)
//!
//! # Refuse, don't guess — and never fake readiness
//!
//! [`ServingSnapshot::ready`] is a CACHED CLAIM with no expiry; its own doc records the
//! 2026-08-05 incident where `serving/status` answered `ready: true` after the llama-server
//! had been SIGKILLed. This module therefore never reads that bare bool. It consumes
//! [`await_ready_serving`], which holds a lane — local OR a pinned external endpoint — to a
//! real multi-token decode, so a compute-wedged lane is rejected rather than believed
//! ([[fallbacks-are-illegal-fail-loud]]).
//!
//! The refusal NAMES what is missing, because a gate that blocks without saying why just moves
//! the archaeology from the run to the gate.
//!
//! # The `force` escape, and why it exists
//!
//! Same shape as `start --force` (#420): refuse by default, allow an explicit operator
//! override, and make the override ANNOUNCE that it skipped a gate. A gate with no override
//! gets worked around by whoever needs to ship; a silent override is worse than none.

use crate::inference::llama_server::ServingSnapshot;

/// Why a round may not be staged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotReady {
    /// Nothing is serving and nothing is coming up — no active model at all.
    NothingServing,
    /// A model is named but has not proven it can decode within the wait budget. This is the
    /// cold-load case AND the wedged-lane case; from the round's side they are the same fact,
    /// because both mean "work staged now cannot be worked now".
    NotDecodeVerified { model: String },
}

impl NotReady {
    /// The operator-facing sentence. States the fact, names the model when there is one, and
    /// says the one thing that changes the answer — never a bare "not ready".
    pub fn explain(&self) -> String {
        match self {
            NotReady::NothingServing => "no model is serving on this node — a round staged now \
                 would post cards nobody can work. Bring a lane up (`continuum serving/status` \
                 to see the plan), then dispatch again."
                .to_string(),
            NotReady::NotDecodeVerified { model } => format!(
                "`{model}` is named but has not proven it can decode within the wait budget — \
                 it is still loading, or the lane is wedged. Either way a round staged now \
                 would post cards nobody can work. Re-run when `serving/status` reports it \
                 decode-verified."
            ),
        }
    }
}

/// The gate's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoundReadiness {
    /// Serving proved it can decode. `lanes` is what the round may fan out across.
    Ready { lanes: u32 },
    /// Refuse — with the reason, so the caller never has to guess.
    Blocked(NotReady),
}

/// The rule alone, with the clock and the network taken out of it.
///
/// `awaited` is [`await_ready_serving`]'s answer (`None` = it never proved decode within the
/// budget). `current` is the unblocking snapshot, used ONLY to name the model in the refusal —
/// never to override the verdict, because that bool is the cached claim this gate exists to
/// distrust.
pub fn decide(awaited: Option<&ServingSnapshot>, current: Option<&ServingSnapshot>) -> RoundReadiness {
    match awaited {
        // `lanes` can legitimately be 0 on a snapshot that is otherwise ready (a plan mid
        // recompute); a round needs at least one, and claiming zero lanes are usable would
        // hand the caller a division by nothing.
        Some(s) => RoundReadiness::Ready { lanes: s.lanes.max(1) },
        None => blocked_reason(current),
    }
}

/// Name the refusal from whatever the unblocking snapshot knows. Split out so the two
/// refusal shapes read as one decision instead of a nested match inside [`decide`].
fn blocked_reason(current: Option<&ServingSnapshot>) -> RoundReadiness {
    match current.and_then(|s| s.active_model.clone()) {
        Some(model) if !model.is_empty() => {
            RoundReadiness::Blocked(NotReady::NotDecodeVerified { model })
        }
        _ => RoundReadiness::Blocked(NotReady::NothingServing),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(model: Option<&str>, ready: bool, lanes: u32) -> ServingSnapshot {
        ServingSnapshot {
            active_model: model.map(|m| m.to_string()),
            ready,
            lanes,
            ..ServingSnapshot::empty()
        }
    }

    /// what this catches: the gate believing `ready: true` off a snapshot that never proved
    /// decode — the exact 2026-08-05 shape where `serving/status` said ready with the process
    /// SIGKILLed. Only `await_ready_serving`'s verdict may open the gate; `current` is for
    /// NAMING the refusal, never for granting one.
    #[test]
    fn a_cached_ready_claim_can_never_open_the_gate() {
        let lying = snap(Some("qwen3.8-27b"), true, 3);
        assert_eq!(
            decide(None, Some(&lying)),
            RoundReadiness::Blocked(NotReady::NotDecodeVerified {
                model: "qwen3.8-27b".to_string()
            }),
            "a ready:true snapshot that failed the decode bar must still BLOCK — the bool is a \
             cached claim with no expiry, which is why this gate reads the awaited verdict"
        );
    }

    /// what this catches: a refusal that says "not ready" and nothing else, which just relocates
    /// the archaeology. Each reason must name the model when one exists, and say what changes it.
    #[test]
    fn every_refusal_names_what_is_missing() {
        assert_eq!(
            decide(None, None),
            RoundReadiness::Blocked(NotReady::NothingServing)
        );
        assert!(NotReady::NothingServing.explain().contains("no model is serving"));

        let loading = snap(Some("devstral-24b"), false, 0);
        let RoundReadiness::Blocked(reason) = decide(None, Some(&loading)) else {
            panic!("a lane that never decode-verified must block");
        };
        let msg = reason.explain();
        assert!(msg.contains("devstral-24b"), "the refusal must NAME the model: {msg}");
        assert!(
            msg.contains("loading") || msg.contains("wedged"),
            "and say what would change the answer: {msg}"
        );
    }

    /// what this catches: a ready lane reporting zero usable lanes — a plan caught mid-recompute
    /// would otherwise hand dispatch a fan-out width of 0 and stage a round that fires nothing.
    #[test]
    fn a_ready_lane_always_offers_at_least_one_lane() {
        assert_eq!(
            decide(Some(&snap(Some("m"), true, 0)), None),
            RoundReadiness::Ready { lanes: 1 }
        );
        assert_eq!(
            decide(Some(&snap(Some("m"), true, 4)), None),
            RoundReadiness::Ready { lanes: 4 }
        );
    }
}
