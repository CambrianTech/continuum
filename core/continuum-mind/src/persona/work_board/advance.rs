//! The state-advance seam WITH the room's review gate: one path for the `work/state` verb and
//! the held-work settle, so the gate cannot be bypassed by either.

use std::sync::Arc;

use airc_lib::{Airc, CardState, ChangeWorkCardState, CreateWorkCard, Priority, WorkCardId};
use uuid::Uuid;


use crate::cognition::card_facts::{is_concluded, short8};
use super::horizon::{card_in_subscribed_rooms, follow_card_room, room_holding_card};
use super::events::emit_card_state_changed;

/// Transition a work card's lifecycle state AND feed the grade/lifecycle
/// subscribers in-process — the ONE place a card's state is written and
/// announced (the compression law: `work/state` the verb, the held-work settle
/// edge, and any future writer all call THIS, never re-implement the change +
/// emit pair). Writing without the direct emit is the #434 delivery-hole that
/// let finished work grade as NOTHING; the (card,state) ring in
/// `emit_card_state_changed` makes the direct feeder and the wire echo publish
/// exactly once when both fire, so callers may always emit.
///
/// `via` labels the feeder on the probe stream so two writers of the same
/// transition are distinguishable.
pub async fn advance_card_state(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
    state: CardState,
    via: &'static str,
    actor: Option<Uuid>,
) -> Result<(), String> {
    advance_card_state_effective(airc, card_id, state, via, actor)
        .await
        .map(|_| ())
}

/// What a holder's checkout says about her work: something to review, nothing,
/// or a checkout that could not be read (never silently either of the others).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeVerdict {
    Changed,
    NoChange,
    CouldNotLook(String),
}

/// Bound on each git read under the review gate. A checkout on a cold disk answers
/// in well under this; a hang is reported as CouldNotLook, never waited out.
pub(crate) const CHECKOUT_READ_BOUND: std::time::Duration = std::time::Duration::from_secs(10);

/// Pure half of the write check: dirty tree, or a commit made after the checkout
/// was staged (HEAD's committer time vs the `.git` entry's mtime — base-free, so it
/// holds for a clone and for a worktree alike).
pub fn change_verdict(
    porcelain: &str,
    head_commit_ts: Option<u64>,
    staged_at_ts: Option<u64>,
) -> ChangeVerdict {
    if !porcelain.trim().is_empty() {
        return ChangeVerdict::Changed;
    }
    match (head_commit_ts, staged_at_ts) {
        (Some(head), Some(staged)) if head > staged => ChangeVerdict::Changed,
        (Some(_), Some(_)) => ChangeVerdict::NoChange,
        (None, _) => ChangeVerdict::CouldNotLook("HEAD commit time unreadable".into()),
        (_, None) => ChangeVerdict::CouldNotLook("staging time unreadable".into()),
    }
}

/// The plumbing half: two bounded git reads plus one stat, folded by [`change_verdict`].
pub(crate) fn checkout_change(root: &std::path::Path) -> ChangeVerdict {
    use crate::system_resources::bounded_command::probe;
    let root_s = root.to_string_lossy().to_string();
    let status = probe(
        "git",
        &["-C", &root_s, "status", "--porcelain"],
        CHECKOUT_READ_BOUND,
    );
    let Some(porcelain) = status.stdout_if_ok() else {
        return ChangeVerdict::CouldNotLook(format!("git status: {}", status.outcome()));
    };
    let head = probe(
        "git",
        &["-C", &root_s, "log", "-1", "--format=%ct"],
        CHECKOUT_READ_BOUND,
    );
    let head_ts = head
        .stdout_if_ok()
        .and_then(|t| t.trim().parse::<u64>().ok());
    let staged_ts = std::fs::metadata(root.join(".git"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| continuum_base::time::EpochMs::of(t).ok())
        .map(continuum_base::time::EpochMs::secs);
    change_verdict(porcelain, head_ts, staged_ts)
}

/// The state-advance seam WITH the review gate (team-recipe Arm 2, 2026-09-03).
/// Returns the state that actually landed:
/// - a verdict on a REVIEW card: `done` = pass → the review retires and the PARENT
///   closes (its grade fires); `blocked` = send back → the review retires and the
///   parent returns to `in_progress` (the reviewer's critique is already in the room);
/// - an owner's `done` on a GATED parent → `review` + a sibling review card is
///   posted for a non-owner to pull;
/// - everything else advances as asked.
/// One seam for the `work/state` verb and the deterministic held-work settle, so
/// the gate cannot be bypassed by either.
/// The feeder label a recorded verdict uses — the one finisher that skips the
/// review gate, because the grade IS the review.
pub const VIA_VERDICT: &'static str = "verdict";

pub async fn advance_card_state_effective(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
    state: CardState,
    via: &'static str,
    // The persona making the transition, when one is (the verb's caller, the
    // held-work settle's owner). The operator passes None and is never gated.
    actor: Option<Uuid>,
) -> Result<CardState, String> {
    let finishing = is_concluded(&state);
    // THE GATE READS THE ROOM (card fa4aaaaa): a room that declares a review policy
    // decides; a room that declares none is ungated. Read only on transitions that can
    // finish.
    let room_gate = if finishing || matches!(state, CardState::Review | CardState::Blocked) {
        super::review::room_review(airc, card_id).await
    } else {
        None
    };
    let gated = room_gate.as_ref().is_some_and(|r| r.outstanding());
    // The card this one reviews, by airc's native link — read only where it decides
    // something: the write check below and the review verdict path.
    let review_parent = if finishing || matches!(state, CardState::Review | CardState::Blocked) {
        native_review_parent(airc, card_id).await
    } else {
        None
    };
    // Card 381ccf3a: a citizen's `done` (or `review`) on a GATED card must carry a
    // write. Measured 2026-09-06/07: cards moved to REVIEW with zero edits as a
    // release valve — the reviewer then reviewed nothing and the grade ran on the
    // base commit. Her acting root is the checkout the card staged for her; no
    // change there since staging = nothing to review.
    if (finishing || state == CardState::Review)
        && gated
        && review_parent.is_none()
    {
        if let Some(root) = actor.and_then(crate::cognition::persona_workspace::acting_root_of) {
            match checkout_change(&root) {
                ChangeVerdict::NoChange => {
                    continuum_base::probe!(
                        class = "work.review.refused_no_write",
                        card = %short8(card_id.as_uuid()),
                        persona = %actor.map(|a| a.to_string()).unwrap_or_default(),  // unwrap_or: probe label; actor is Some on this branch
                        root = %root.display(),
                        "a done on a gated card with no change in her checkout — refused; \
                         write the fix or release the card"
                    );
                    return Err(format!(
                        "done refused: {} has no change since it was staged (clean tree, no new \
                         commit) — there is nothing for a reviewer to review. Write the fix \
                         (code/edit) and mark done again, or release the card (work/release) \
                         so another holder can take it.",
                        root.display()
                    ));
                }
                ChangeVerdict::Changed => {}
                ChangeVerdict::CouldNotLook(reason) => continuum_base::probe!(
                    class = "work.review.write_check_unknown",
                    card = %short8(card_id.as_uuid()),
                    root = %root.display(),
                    reason = %reason,
                    "could not read her checkout — the transition proceeds; the reviewer decides"
                ),
            }
        }
    }
    if let Some(parent) = review_parent {
        if finishing || state == CardState::Blocked {
            let passed = finishing;
            raw_advance(airc, card_id, CardState::Closed, via).await?;
            let parent_id = WorkCardId::from_uuid(parent);
            // A room with a policy counts PASSING REVIEWS by distinct reviewers on the
            // parent's submission: one review card closing is not enough when it asks
            // for more, and the parent waits in Review (card fa4aaaaa).
            if passed {
                if let Some(parent_gate) = super::review::room_review(airc, parent_id).await {
                    if parent_gate.outstanding() {
                        continuum_base::probe!(
                            class = "work.review.awaiting_more",
                            parent = %short8(parent),
                            passed = parent_gate.passed as u64,
                            required = parent_gate.policy.required,
                            "a review closed, but the room's policy asks for more passing reviews — the parent waits"
                        );
                        return Ok(CardState::Review);
                    }
                }
            }
            // A PASS ON A CARD WITH AN OPEN PR LEAVES IT MERGEABLE. The card finishes through
            // its merge, as Merged; closing it here made `airc work merge` refuse it ("state
            // is Closed, but merge requires Review"). Kimi passed #4825's submission on
            // 2026-10-06 and the card closed with its PR open, so the merge took a hand move
            // back to Review. A card with no PR (a bench round's) still closes on the pass.
            if passed {
                if let Some((_, parent_card)) = card_in_subscribed_rooms(airc, parent_id).await {
                    if parent_card.pull_request.is_some() && parent_card.state != CardState::Merged {
                        if parent_card.state != CardState::Review {
                            raw_advance(airc, parent_id, CardState::Review, via).await?;
                        }
                        continuum_base::probe!(
                            class = "work.review.passed_awaiting_merge",
                            review = %short8(card_id.as_uuid()),
                            parent = %short8(parent),
                            "reviewer's pass on a card with an open PR — it stays in Review for \
                             its merge, which moves it to Merged"
                        );
                        return Ok(CardState::Review);
                    }
                }
            }
            let next = if passed {
                CardState::Closed
            } else {
                CardState::InProgress
            };
            continuum_base::probe!(
                class = "work.review.verdict",
                review = %short8(card_id.as_uuid()),
                parent = %short8(parent),
                passed,
                "reviewer's verdict — a pass closes the parent (grade fires), a send-back \
                 returns it to in_progress"
            );
            raw_advance(airc, parent_id, next, via).await?;
            return Ok(next);
        }
    }
    // A VERDICT is the review's outcome: it closes the parent outright. Every
    // other finisher on a gated card goes to review first.
    if finishing && via != VIA_VERDICT && gated {
        raw_advance(airc, card_id, CardState::Review, via).await?;
        match open_review_card(airc, card_id).await {
            Ok(review) => continuum_base::probe!(
                class = "work.review.opened",
                parent = %short8(card_id.as_uuid()),
                review = %short8(review.as_uuid()),
                "owner's done became review — a sibling review card is on the deck for a \
                 non-owner; only the reviewer's done closes this card"
            ),
            Err(e) => continuum_base::probe!(
                class = "work.review.open_failed",
                parent = %short8(card_id.as_uuid()),
                error = %e,
                "the card is in review but no review card could be posted — the next \
                 done retries the gate"
            ),
        }
        return Ok(CardState::Review);
    }
    // A RESIDENT'S FINISH NEVER STRANDS OR UNDOES A MERGE (card 4356a90f). On a card whose
    // linked PR has not merged, her `closed` (or `merged`) lands in Review and the merge
    // finishes it, as Merged: the twin of the reviewer's-pass rule above. Closing it made
    // `airc work merge` refuse ("state is Closed, but merge requires Review"): Kimi closed
    // 13d7e47a and then 1fc73f18 right after their passes (2026-10-24/25). A card already
    // Merged keeps that terminal state: her close is acknowledged, never a downgrade to
    // Closed that a later close would read as "PR linked, not merged" and reopen (Codex on
    // #5399). The airc card's merge fact IS its Merged state (set by PullRequestMerged).
    if finishing && via != VIA_VERDICT {
        if let Some((_, card)) = card_in_subscribed_rooms(airc, card_id).await {
            match resident_finish(actor, card.pull_request.is_some(), &card.state) {
                ResidentFinish::AwaitMerge => {
                    if card.state != CardState::Review {
                        raw_advance(airc, card_id, CardState::Review, via).await?;
                    }
                    continuum_base::probe!(
                        class = "work.state.close_awaits_merge",
                        card = %short8(card_id.as_uuid()),
                        asked = ?state,
                        "a resident finished a card whose PR has not merged; it stays in Review \
                         for its merge, which moves it to Merged"
                    );
                    return Ok(CardState::Review);
                }
                ResidentFinish::StayMerged => {
                    continuum_base::probe!(
                        class = "work.state.already_merged",
                        card = %short8(card_id.as_uuid()),
                        asked = ?state,
                        "a resident finished a card that is already merged; it keeps Merged"
                    );
                    return Ok(CardState::Merged);
                }
                ResidentFinish::Proceed => {}
            }
        }
    }
    raw_advance(airc, card_id, state, via).await?;
    Ok(state)
}

/// What a resident's finish does to a card, from its PR link and current state.
#[derive(Debug, PartialEq, Eq)]
enum ResidentFinish {
    /// The linked PR has not merged: wait in Review for the merge.
    AwaitMerge,
    /// Already merged: keep the terminal state, never downgrade it.
    StayMerged,
    /// No PR, or the operator: the finish proceeds as asked.
    Proceed,
}

/// PURE: the operator (no actor) is never held; a Merged card stays Merged; a card with a linked,
/// unmerged PR waits for its merge.
fn resident_finish(actor: Option<Uuid>, pr_linked: bool, current: &CardState) -> ResidentFinish {
    match (actor, current) {
        (None, _) => ResidentFinish::Proceed,
        (Some(_), CardState::Merged) => ResidentFinish::StayMerged,
        (Some(_), _) if pr_linked => ResidentFinish::AwaitMerge,
        (Some(_), _) => ResidentFinish::Proceed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 4356a90f, Codex on #5399): a resident's `closed` after a pass
    // stranded a green PR (airc work merge needs Review), and a close on an already-merged
    // card must not downgrade it to Closed, where a repeated close would reopen it.
    #[test]
    fn a_residents_finish_waits_for_the_merge_and_never_undoes_it() {
        use ResidentFinish::*;
        let her = Some(Uuid::from_u128(7));
        assert_eq!(resident_finish(her, true, &CardState::Review), AwaitMerge, "after a pass, her close waits");
        assert_eq!(resident_finish(her, true, &CardState::InProgress), AwaitMerge);
        assert_eq!(resident_finish(her, true, &CardState::Merged), StayMerged, "merged stays merged");
        assert_eq!(resident_finish(her, false, &CardState::Merged), StayMerged);
        assert_eq!(resident_finish(her, false, &CardState::Review), Proceed, "no PR: a close is a close");
        assert_eq!(resident_finish(None, true, &CardState::Review), Proceed, "the operator is never held");
        assert_eq!(resident_finish(None, true, &CardState::Merged), Proceed);
    }
}
