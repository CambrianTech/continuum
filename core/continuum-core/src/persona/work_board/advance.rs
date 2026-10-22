//! The state-advance seam WITH the room's review gate: one path for the `work/state` verb and
//! the held-work settle, so the gate cannot be bypassed by either.

use std::sync::Arc;

use airc_lib::{Airc, CardState, ChangeWorkCardState, CreateWorkCard, Priority, WorkCardId};
use uuid::Uuid;


use super::facts::{is_concluded, short8};
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
pub(crate) async fn advance_card_state(
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
pub(crate) enum ChangeVerdict {
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
pub(crate) fn change_verdict(
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
pub(crate) const VIA_VERDICT: &'static str = "verdict";

pub(crate) async fn advance_card_state_effective(
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
    raw_advance(airc, card_id, state, via).await?;
    Ok(state)
}

/// Post the sibling review card for `parent` into the parent's own room, as the
/// caller's identity. The card BODY carries the reviewer's role prompt — recipe
/// data on the card, never a code branch in cognition.
pub(crate) async fn open_review_card(airc: &Arc<Airc>, parent: WorkCardId) -> Result<WorkCardId, String> {
    let (room, card) = card_in_subscribed_rooms(airc, parent)
        .await
        .ok_or_else(|| "parent card is on no subscribed room's board".to_string())?;
    let Some((_, instance)) = crate::cognition::bench_title::parse_card_title(&card.title) else {
        // A room's own card (not a bench card): its review card comes from the parent
        // itself, under the room's policy (card fa4aaaaa). The bench branch below is
        // unchanged.
        return open_room_review_card(airc, &room, &card, parent, super::submissions::latest_submission(&card).map(|s| s.submission_id)).await;
    };
    let owner = card
        .owner
        .and_then(|o| {
            crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                .and_then(|reg| reg.get(o.as_uuid()))
                .map(|rt| rt.agent_name().to_string())
        })
        .unwrap_or_else(|| "the owner".to_string()); // unwrap_or: owner not resident = a neutral name in the review body
    let title = crate::cognition::bench_title::review_card_title(parent.as_uuid(), &instance, &owner);
    let p8 = parent.as_uuid().simple().to_string()[..8].to_string();
    let body = format!(
        "Review {owner}'s fix for `{instance}` (card {p8}). You are the fresh pair of \
         eyes: READ-ONLY. Do not edit their tree.\n\n\
         Their checkout is {owner}'s workspace at `swe/{instance}/` — your hands are rooted \
         there for this card. Run `git diff` to see the change; run the repo's tests for the \
         issue.\n\n\
         Judge the diff against the card's definition of done: does it fix the CAUSE with \
         the smallest edit, without touching the tests, and do the tests pass?\n\n\
         Verdict, with evidence — every claim cites a file:line or a test result:\n\
         - PASS: say `work/state <this card's id> done` — that closes {owner}'s card and \
         fires the grade.\n\
         - SEND BACK: say what must change (cited), then `work/state <this card's id> \
         blocked` — the card returns to {owner} in progress.\n\
         Report gaps, not style."
    );
    // The review lands on the PARENT's board, named, never through the shared current-room
    // pointer: parallel dones on two boards would otherwise race one join against the other
    // (IntelMac on #4838; the race #4838 removes from work/state).
    let mut req = CreateWorkCard::new(card.repo.clone(), title, Priority::P1).reviewing(parent);
    req.body = Some(body);
    airc.create_work_card_in(&room, req)
        .await
        .map_err(|e| e.to_string())
}

/// Post the review card for a ROOM's own card (not a bench card): titled and described
/// from the parent, linked natively (`reviewing(parent)`).
/// Whoever the room's policy admits reviews it with `work/review` (card fa4aaaaa).
pub(crate) async fn open_room_review_card(
    airc: &Arc<Airc>,
    room: &airc_lib::Room,
    card: &airc_lib::WorkCard,
    parent: WorkCardId,
    submission: Option<airc_work::SubmissionId>,
) -> Result<WorkCardId, String> {
    let owner = card
        .owner
        .and_then(|o| {
            crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                .and_then(|reg| reg.get(o.as_uuid()))
                .map(|rt| rt.agent_name().to_string())
        })
        .unwrap_or_else(|| "the owner".to_string()); // unwrap_or: owner not resident = a neutral name in the review body
    let p8 = parent.as_uuid().simple().to_string()[..8].to_string();
    let title: String = format!("review: {}", card.title).chars().take(160).collect();
    let body = format!(
        "Review {owner}'s work on card {p8} (\"{}\"). Read the latest submission          (work/submission) and the change it carries, judge it against the card, then give          your verdict with work/review (passed or failed, with evidence). This room's review          policy decides how many passing reviews finish the card and whether the author may          review her own work.",
        card.title
    );
    // on the parent's board, named: never through the shared current-room pointer (#4838)
    let mut req = CreateWorkCard::new(card.repo.clone(), title, Priority::P1).reviewing(parent);
    req.body = Some(body);
    match submission {
        Some(candidate) => airc.ensure_submission_review_in(room, parent, candidate, req).await,
        None => airc.create_work_card_in(room, req).await,
    }.map_err(|e| e.to_string())
}

/// The card a review card reviews, by airc's native link (`reviews`).
pub(crate) async fn native_review_parent(airc: &Arc<Airc>, card_id: WorkCardId) -> Option<Uuid> {
    card_in_subscribed_rooms(airc, card_id)
        .await
        .and_then(|(_, card)| card.reviews.map(|p| p.as_uuid()))
}

/// The raw advance + the bus event — what [`advance_card_state`] was before the gate.
pub(crate) async fn raw_advance(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
    state: CardState,
    via: &'static str,
) -> Result<(), String> {
    // THE CARD NAMES ITS ROOM. A board she can see holds the card: mutate it THERE, without
    // reading or moving the scope's one current-room pointer. Kimi on the 5090 (2026-10-06
    // 22:5xZ) closed 8 duplicate cards in ONE turn, in parallel, across #continuum and
    // #cambriantech: every call that refused in the current room "followed" its card by
    // switching that shared pointer, and the parallel follows flipped it under each other, so
    // 7 retries landed in the wrong room again and refused. Only a card on no visible board
    // (a bench round's run room) still takes the current room + follow path below.
    let mut attempt = match room_holding_card(airc, card_id).await {
        Some(room) => {
            airc.change_work_card_state_in(&room, ChangeWorkCardState { card_id, state })
                .await
        }
        None => {
            airc.change_work_card_state(ChangeWorkCardState { card_id, state })
                .await
        }
    };
    if matches!(
        attempt,
        Err(airc_lib::AircError::WorkCardNotInCurrentRoom { .. })
    ) && follow_card_room(airc, card_id, "work/state")
        .await
        .is_some()
    {
        attempt = airc
            .change_work_card_state(ChangeWorkCardState { card_id, state })
            .await;
    }
    attempt.map_err(|e| e.to_string())?;
    // A REOPEN MEANS NOBODY HOLDS IT, and airc's projection owns that: a transition to
    // Open ends the claim (airc #1598, card 12afbb2f). The round tracker's settled mark
    // is ours to clear, or the deck never offers the card again.
    if state == CardState::Open {
        crate::cognition::bench_round::reopen_card(card_id.as_uuid());
    }
    // The CARD's room, never `current_room()` — boards are per-room and the grade
    // subscriber refuses an event with no room.
    let room_id = room_holding_card(airc, card_id)
        .await
        .map(|r| r.channel.as_uuid().to_string())
        .unwrap_or_default(); // unwrap_or: a card we cannot place carries an empty room on the bus
    emit_card_state_changed(
        serde_json::json!({
            "card_id": card_id.as_uuid().to_string(),
            "state": serde_json::to_value(state).unwrap_or(serde_json::Value::Null),  // unwrap_or: an unserializable state = null on the bus, never a panic on the verb
            "room_id": room_id,
        }),
        via,
    )
    .await;
    Ok(())
}
