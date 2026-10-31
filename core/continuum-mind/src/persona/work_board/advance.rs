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
            // Read only for a close: whether her ledger records a findings conclusion.
            let findings = state == CardState::Closed && concluded_by_findings(airc, card_id, actor).await;
            match checkout_change(&root) {
                ChangeVerdict::NoChange if findings => {
                    // A close needs no write when she established why no change is needed (an
                    // already-fixed bug, a premise that no longer holds): her ledger is the proof,
                    // and nothing goes to a reviewer, so the gate's purpose still holds (Codex on
                    // #5653; Aris on card 7d044c91, 2026-10-31: 11 acts on a bug 73196f51a fixed).
                    continuum_base::probe!(
                        class = "work.closed_by_findings",
                        card = %short8(card_id.as_uuid()),
                        "a gated card closed with no change: her ledger records what she established"
                    );
                }
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
                         (code/edit) and mark done again; or, when no change is needed (already \
                         fixed, premise gone), record what you established (work/note with known) \
                         and close it (work/state closed); or release it (work/release).",
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
                        close_superseded_reviews(airc, parent_id, card_id, via).await;
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
            // The verdict is terminal for THIS review card: it closed above (line 179), the
            // parent moves to its post-verdict state, and we return that effective state.
            // Falling through would re-write this very card from Closed into the requested
            // one — a send-back lands Blocked onto a closed card and returns Blocked,
            // a pass can enter the generic gated-finisher branch below and reopen or
            // recreate its own review (the reviewer's 99c64d55 send-back, blocking #1).
            raw_advance(airc, parent_id, next, via).await?;
            // A PASS IS TERMINAL FOR EVERY PRIOR ROUND: other cards still sitting open on
            // this parent may review artifacts that are now approved, and left alone they
            // stay claimable P1s forever — the per-round cards outliving their parent's pass
            // is exactly the stale accumulation this card exists to end. The sweep runs only
            // AFTER the parent's own transition has landed: if that advance failed (`?`),
            // nothing else on the board moves with it (blocking #2). A send-back keeps its
            // rounds live too: the author may still be inside a round they are reviewing.
            if passed {
                close_superseded_reviews(airc, parent_id, card_id, via).await;
            }
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

/// Post the sibling review card for `parent` into the parent's own room, as the
/// caller's identity. The card BODY carries the reviewer's role prompt — recipe
/// data on the card, never a code branch in cognition.
/// A close with no change in her checkout concludes by findings when her newest ledger on the card
/// is hers and records what she established (`work/note` with known facts).
async fn concluded_by_findings(airc: &Arc<Airc>, card_id: WorkCardId, actor: Option<Uuid>) -> bool {
    use crate::experience::ledger::LedgerStore as _;
    let Some(actor) = actor else { return false };
    let Some((room, _)) = card_in_subscribed_rooms(airc, card_id).await else { return false };
    match crate::experience::ledger::WallLedgerStore::new(airc.clone()).read(&room, card_id.as_uuid()).await {
        Ok(Some(ledger)) => established_by(&ledger, actor),
        Ok(None) | Err(_) => false,
    }
}

/// PURE: the ledger is hers and holds at least one established fact.
fn established_by(ledger: &crate::experience::ledger::CardLedger, actor: Uuid) -> bool {
    ledger.by == actor && ledger.known.iter().any(|fact| !fact.trim().is_empty())
}

pub(crate) async fn open_review_card(airc: &Arc<Airc>, parent: WorkCardId) -> Result<WorkCardId, String> {
    let (room, card) = card_in_subscribed_rooms(airc, parent)
        .await
        .ok_or_else(|| "parent card is on no subscribed room's board".to_string())?;
    let Some((_, instance)) = crate::recipe::bench_title::parse_card_title(&card.title) else {
        // A room's own card (not a bench card): its review card comes from the parent
        // itself, under the room's policy (card fa4aaaaa). The bench branch below is
        // unchanged.
        return open_room_review_card(airc, &room, &card, parent, crate::cognition::card_holder::latest_submission(&card).map(|s| s.submission_id)).await;
    };
    let owner = card
        .owner
        .and_then(|o| {
            crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                .and_then(|reg| reg.get(o.as_uuid()))
                .map(|rt| rt.agent_name().to_string())
        })
        .unwrap_or_else(|| "the owner".to_string()); // unwrap_or: owner not resident = a neutral name in the review body
    let title = crate::recipe::bench_title::review_card_title(parent.as_uuid(), &instance, &owner);
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
pub async fn open_room_review_card(
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
pub async fn raw_advance(
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

#[cfg(test)]
mod tests {

    // what this catches: a findings close admitted on someone else's ledger, or on a ledger that
    // establishes nothing (an empty note would make the write gate a formality).
    #[test]
    fn a_findings_close_needs_her_own_ledger_with_an_established_fact() {
        let her = Uuid::from_u128(1);
        let ledger = |by: Uuid, known: Vec<&str>| crate::experience::ledger::CardLedger {
            card_id: Uuid::from_u128(9),
            known: known.into_iter().map(String::from).collect(),
            hypotheses: Vec::new(),
            unknown: String::new(),
            next_test: String::new(),
            decided_fix: None,
            at_ms: 1,
            by,
        };
        assert!(established_by(&ledger(her, vec!["llm_deliberation_faculty.rs is 931 lines; the cited line 2496 is gone (73196f51a)"]), her));
        assert!(!established_by(&ledger(Uuid::from_u128(2), vec!["a fact"]), her), "another holder's ledger");
        assert!(!established_by(&ledger(her, vec!["  "]), her), "a ledger that establishes nothing");
    }
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
/// A PASS CLOSES EVERY OTHER ROUND ON THE SAME PARENT THAT IS SETTLED BY THE
/// ACCEPTED SUBMISSION. Review cards are per round: a pass on one settles the artifact
/// that round was reviewing (its typed binding — or, for a legacy unbound round, the
/// newest submission in transcript order, which is exactly what such a round targets when
/// it lands its own verdict), and any other open round on this parent whose candidate is at
/// or behind that one reviews work that just got approved. Left alone they stay claimable
/// P1s forever — the per-round cards outliving their parent's pass is exactly the stale
/// accumulation this exists to end.
/// The sweep closes ONLY what the accepted submission settles (the reviewer's 99c64d55
/// send-back, blocking #3): a round whose bound candidate is NEWER than the accepted one
/// reviews work that postdates the pass — closing it would discard a live judgment of newer
/// work during concurrent resubmission — and any round whose candidate cannot be resolved
/// (a board that will not read, a binding with no submission) stays open for a hand.
/// A send-back never reaches here: its round may still be live for its reviewer, so only a
/// `passed` verdict sweeps (both call sites are inside `if passed`). The sweep must not
/// break the verdict path it serves — every failure is probed and swallowed; the parent's
/// own advance stands either way.
pub(crate) async fn close_superseded_reviews(
    airc: &Arc<Airc>,
    parent_id: WorkCardId,
    current_review: WorkCardId,
    via: &'static str,
) {
    let horizon = match super::horizon::board_horizon(airc).await {
        Ok(horizon) => horizon,
        Err(e) => {
            continuum_base::probe!(
                class = "work.review.superseded_sweep_failed",
                parent = %short8(parent_id.as_uuid()),
                error = %e,
                "could not read the boards to sweep stale review cards — the verdict stands"
            );
            return;
        }
    };
    let mut closed: u64 = 0;
    for (room, board) in &horizon.boards {
        // The parent's submissions are newest-first; a lower index is NEWER work.
        let Some(parent) = board.card(parent_id) else {
            continuum_base::probe!(
                class = "work.review.superseded_sweep_failed",
                parent = %short8(parent_id.as_uuid()),
                room = %room.name,
                "the parent card is not on this board — the sweep cannot resolve candidates here"
            );
            continue;
        };
        let submissions = &parent.submissions;
        if submissions.is_empty() {
            // No candidate to settle: nothing is provably superseded, so nothing closes.
            continuum_base::probe!(
                class = "work.review.superseded_sweep_failed",
                parent = %short8(parent_id.as_uuid()),
                room = %room.name,
                "the parent has no submissions — the sweep cannot tie any round to an accepted candidate"
            );
            continue;
        }
        // THE ACCEPTED CANDIDATE: what this pass settled. A typed round names it; a legacy
        // unbound round settles the newest submission in transcript order (the same target
        // its own verdict would resolve, per the projection's candidate_for_review).
        let accepted = board
            .review_request(current_review)
            .map(|r| r.submission_id)
            .unwrap_or(submissions[0].submission_id); // unwrap_or: an unbound legacy round settles the newest submission; empty was guarded above
        let Some(accepted_idx) = submissions.iter().position(|s| s.submission_id == accepted) else {
            continuum_base::probe!(
                class = "work.review.superseded_sweep_failed",
                parent = %short8(parent_id.as_uuid()),
                review = %short8(current_review.as_uuid()),
                room = %room.name,
                "the verdict's candidate is not on the board — the sweep cannot tie any round to it"
            );
            continue;
        };
        for card in &board.snapshot().cards {
            if card.card_id == current_review || card.reviews != Some(parent_id) {
                continue; // not a review of this parent, or the verdict landing right now
            }
            if matches!(card.state, CardState::Closed | CardState::Merged) {
                continue; // already terminal — nothing to sweep
            }
            // THE ROUND'S OWN CANDIDATE: its typed binding, or — for a legacy unbound round —
            // the newest submission in transcript order, which is what it would review when
            // it lands. A bound candidate missing from the parent is a broken link:
            // fail closed and leave the round open.
            let candidate = board
                .review_request(card.card_id)
                .map(|r| r.submission_id)
                .unwrap_or(submissions[0].submission_id); // unwrap_or: an unbound legacy round targets the newest submission in transcript order; nonempty guarded above
            let Some(candidate_idx) = submissions.iter().position(|s| s.submission_id == candidate)
            else {
                continuum_base::probe!(
                    class = "work.review.superseded_close_failed",
                    review = %short8(card.card_id.as_uuid()),
                    parent = %short8(parent_id.as_uuid()),
                    room = %room.name,
                    "a round's bound candidate is not on the board — it stays open for a hand"
                );
                continue;
            };
            if candidate_idx < accepted_idx {
                // Newer than what just passed: this pass settled nothing about that work.
                continuum_base::probe!(
                    class = "work.review.superseded_left_open",
                    review = %short8(card.card_id.as_uuid()),
                    parent = %short8(parent_id.as_uuid()),
                    room = %room.name,
                    "a round reviews a candidate newer than the accepted one — left open"
                );
                continue;
            }
            match raw_advance(airc, card.card_id, CardState::Closed, via).await {
                Ok(()) => closed += 1,
                Err(e) => continuum_base::probe!(
                    class = "work.review.superseded_close_failed",
                    review = %short8(card.card_id.as_uuid()),
                    parent = %short8(parent_id.as_uuid()),
                    room = %room.name,
                    error = %e,
                    "a stale per-round review card refused to close — it stays open for a hand"
                ),
            }
        }
    }
    if closed > 0 {
        continuum_base::probe!(
            class = "work.review.superseded_swept",
            parent = %short8(parent_id.as_uuid()),
            review = %short8(current_review.as_uuid()),
            closed,
            "a pass landed — the other open rounds settled by its accepted candidate are closed; no stale claimable P1s"
        );
    }
}

