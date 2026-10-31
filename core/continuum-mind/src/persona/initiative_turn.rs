//! The INITIATIVE turn: her own heartbeat's turn with her hands when she holds nothing.
//!
//! ## Why it exists
//!
//! The act-question (`act_question.rs`) gave a citizen a turn with her hands ONLY when
//! a card was already in them. With nothing held, a speak-pass ended her moment, so
//! there was no point at which she could choose work for herself: take something up,
//! start her own, read code, fix what she noticed, open a branch or a PR. She could act
//! only if something had already handed her a card. Measured on the M5, 2026-10-23: the
//! four residents made ~7 acts and 0 writes an hour, every gate read
//! `persona.work.gate decision="no_held_work"`, while Kimi on the 5090, holding cards,
//! made 45 acts and 10 writes. Joel: "They're supposed to be as capable as you so don't
//! constrain them"; "You can do shit without cards too."
//!
//! ## The contract
//!
//! - It is an OFFER, never an assignment. Its opening line says nothing is assigned and
//!   the choice is hers; her own-time framing (`deliberation_prompt::OWN_TIME_BLOCK`, the
//!   one owner of that wording) names her means as a capable engineer has them, and says
//!   none is expected. Nothing grades it and nothing tells her what to do.
//! - Her full hands, as a work turn has them: the turn is framed
//!   `TurnFraming::initiative`, which the deliberation faculty treats as an act turn
//!   (its hands surface, the ACT allowance, the work lane priority) with none of a
//!   card's deliverable contract.
//! - Her default hands: no card binds them, so they stand in her own workspace. The
//!   moment she claims or creates a card, the next beat's gate finds it held and the
//!   held-work turn takes over, unchanged.
//! - PACED BY HER OWN HEARTBEAT, no timer of its own: only the self-tick offers it
//!   ([`Initiative::Offered`]), so it runs at the beat `next_beat_after` sets. A turn in
//!   which she acted or spoke is engagement (the engaged beat); a pass is not, so the
//!   beat backs off toward rest exactly as a fruitless musing did. A message turn never
//!   offers it: the message snaps her beat to engaged, and that beat offers it.
//! - ASKED AGAIN ONLY ON SOMETHING NEW: after a pass, the next offer waits until something
//!   she perceives has moved ([`Initiative::on_beat`]); on unchanged perception her prompt
//!   is the same and so is the question.
//!
//! The earlier attempt this must not repeat (card_holder.rs, Kimi 2026-10-06): a gate
//! that could not see a followed claim read `no_held_work` and she created a new review
//! card every turn. This turn creates nothing; whether a card is created is her act.

use crate::cognition::act_observe::SettleStep;
use crate::persona::act_question::WorkQuestionOutcome;
use crate::persona::service_loop::{CitizenConversation, LIVE_MAX_ACTS};
use crate::persona::supervisor::HostedPersona;

/// Whether the caller's cadence offers her an initiative turn when she holds nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Initiative {
    /// Her own heartbeat (the self-tick, paced by `next_beat_after`) offers it.
    Offered,
    /// A message turn: the speak question was this turn's; her next beat offers it.
    NotOffered,
}

impl Initiative {
    /// Her heartbeat's offer. After she passes, the next offer waits until something she
    /// perceives has moved: with her perception unchanged her prompt is byte-identical, so
    /// asking again only spends the engine on the same question (the M5, 2026-10-27: Aiko
    /// was asked five times on the same 102,073 tokens, 25-88 s of engine each, card
    /// d0cf9473). A line, a board change or a view update in any of her activities moves
    /// it; her beat still rests through the quiet ticks between. Pure.
    pub(crate) fn on_beat(passed_at_ms: Option<u64>, perception_moved_at_ms: Option<u64>) -> Self {
        match passed_at_ms {
            None => Self::Offered,
            Some(passed) if perception_moved_at_ms.is_some_and(|moved| moved > passed) => Self::Offered,
            Some(_) => Self::NotOffered,
        }
    }
}

/// The line her initiative turn opens with. A statement of her situation, never an
/// instruction: what she may do is named once, in `OWN_TIME_BLOCK`, beside it.
pub(crate) const INITIATIVE_MOMENT: &str = "[initiative] Nothing is assigned to you and no \
     one is waiting on you. This moment and your hands are yours: what you do with them, \
     or whether you do anything, is your choice.";

/// Drive her initiative turn in `turn_room`: in her held card's hands when she holds one, her
/// default hands otherwise. Reports `drove` always, and `engaged` only when she acted or spoke,
/// so her beat rests after a pass.
pub(crate) async fn drive_initiative_turn(
    ctx: &HostedPersona,
    cycle: &crate::cognition::workspace::WorkspaceCycle,
    conversation: &mut dyn CitizenConversation,
    lamport: u64,
    turn_room: uuid::Uuid,
) -> WorkQuestionOutcome {
    let Ok(room) = crate::identity::ActivityRoom::from_uuid(turn_room) else {
        // The service loop guarantees a non-nil turn room; a nil one is named, never driven.
        continuum_base::probe!(
            class = "persona.initiative.no_room",
            persona = %ctx.identity.agent_name,
            "initiative turn not driven: the turn room was nil"
        );
        return WorkQuestionOutcome::default();
    };
    // Her own heartbeat caused it, so the cause is Ambient (a lived turn), never Synthetic.
    let burst = crate::cognition::workspace::Burst::from_turns_at(
        room,
        vec![crate::cognition::workspace::BurstTurn::opaque(INITIATIVE_MOMENT)],
        Some(continuum_base::time::EpochMs::now().millis()),
        crate::cognition::workspace::Cause::Ambient,
    );
    // HER OWN TIME CAN BE CARD WORK, and card work is credit-bearing (card 959f3d5c). Driven
    // without a credit capture, Sahar's own-time submit (2026-10-25, act 0 of an initiative
    // turn) was never staged: its issuing generation was never indexed, so its review held
    // forever. Her held card's turn, exactly as a room turn drives it; with no card held this
    // is her default hands and no capture, as before. Bookkeeping on her own act: it changes
    // neither her framing nor what she is told.
    let (settled, _) = crate::persona::card_hands::drive_in_held_card(
        cycle,
        ctx.identity.peer_id.as_uuid(),
        &ctx.identity.agent_name,
        INITIATIVE_MOMENT,
        burst,
        crate::cognition::workspace::TurnFraming::initiative(),
        LIVE_MAX_ACTS,
        conversation,
    )
    .await;
    let acts = settled.acts;
    let (step, _, _) = SettleStep::from_settled(settled);
    let capacity = match &step {
        SettleStep::CapacityRefused { error, .. } => Some(error.clone()),
        _ => None,
    };
    let engaged = matches!(step, SettleStep::Spoke(_) | SettleStep::Acted { .. }) || acts > 0;
    match step {
        SettleStep::Spoke(text) => {
            // A raw tool-call envelope is an un-acted call, never a contribution (the
            // self-tick's guard, same reason).
            if crate::ai::json_in_prompt_tools::parse_tool_call(&text).is_none() {
                if let Err(e) = crate::persona::service_loop::publish_own_speech(
                    conversation,
                    ctx.identity.peer_id,
                    turn_room,
                    &text,
                )
                .await
                {
                    tracing::warn!(error = %e, "initiative turn: her report failed to send");
                }
            }
            continuum_base::probe!(
                class = "persona.turn.initiative",
                persona = %ctx.identity.agent_name,
                lamport = lamport,
                acts = acts as u64,
                decision = "spoke",
                "initiative turn settled with something she chose to say"
            );
        }
        SettleStep::Passed { reason } => continuum_base::probe!(
            class = "persona.turn.initiative",
            persona = %ctx.identity.agent_name,
            lamport = lamport,
            acts = acts as u64,
            decision = "passed",
            reason = reason.as_deref().unwrap_or(""), // unwrap_or: no reason given = empty probe field
            "initiative turn passed: her choice, honored; her beat rests"
        ),
        other => continuum_base::probe!(
            class = "persona.turn.initiative",
            persona = %ctx.identity.agent_name,
            lamport = lamport,
            acts = acts as u64,
            decision = ?std::mem::discriminant(&other),
            "initiative turn settled without a spoken report"
        ),
    }
    WorkQuestionOutcome {
        drove: true,
        room: Some(turn_room),
        capacity,
        engaged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a harness creeping into her own time. The initiative turn's
    // opening line and the own-time framing beside it must say nothing is assigned and
    // the choice is hers, must never steer her to a card or a specific task, and must
    // never grade her (Joel, 2026-10-23: "You can do shit without cards too").
    #[test]
    fn the_initiative_prompt_offers_a_choice_and_steers_to_no_task() {
        let own_time = crate::cognition::deliberation_prompt::OWN_TIME_BLOCK;
        for (name, text) in [("opening", INITIATIVE_MOMENT), ("own time", own_time)] {
            let lower = text.to_lowercase();
            assert!(lower.contains("nothing is assigned"), "{name}: says nothing is assigned: {text}");
            assert!(lower.contains("your choice"), "{name}: says the choice is hers: {text}");
            for steer in ["card", "must", "should", "you need", "graded", "score", "your task"] {
                assert!(!lower.contains(steer), "{name} steers her with {steer:?}: {text}");
            }
        }
        // Her means are named, as options, in the framing (once), not in the opening.
        for means in ["code", "pull request", "room", "boards"] {
            assert!(own_time.contains(means), "own time names {means:?} among her means");
            assert!(!INITIATIVE_MOMENT.contains(means), "the opening line is not a menu: {means:?}");
        }
    }

    // what this catches: re-asking a mind that passed on a prompt that cannot have changed
    // (card d0cf9473), and the opposite failure, never asking again once her world moves.
    #[test]
    fn after_a_pass_only_a_moved_perception_offers_her_initiative_again() {
        assert_eq!(Initiative::on_beat(None, None), Initiative::Offered, "never passed: offered");
        assert_eq!(Initiative::on_beat(Some(1_000), None), Initiative::NotOffered, "nothing ever moved");
        assert_eq!(Initiative::on_beat(Some(1_000), Some(1_000)), Initiative::NotOffered, "moved before the pass");
        assert_eq!(Initiative::on_beat(Some(1_000), Some(1_001)), Initiative::Offered, "moved since the pass");
    }

    // what this catches: the initiative framing losing its act shape or gaining a
    // deliverable. It must be self-initiated, undirected and an initiative, and must NOT
    // carry `workspace_deliverable` (whose act-budget facts tell her the deliverable is
    // a file change and whose re-perception refuses a plain spoken answer).
    #[test]
    fn the_initiative_framing_is_her_own_undirected_act_with_no_deliverable() {
        let f = crate::cognition::workspace::TurnFraming::initiative();
        assert!(f.initiative && f.self_initiated, "{f:?}");
        assert!(!f.attention.is_addressed(), "nobody named her: {f:?}");
        assert!(!f.workspace_deliverable, "no card contract rides it: {f:?}");
    }
}
