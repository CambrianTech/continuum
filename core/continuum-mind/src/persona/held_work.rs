//! Her held work, read from her own board once per tick (STATE-DRIVEN-SURFACE.md): the cards she
//! holds with what each one's preconditions put in her hands (`work_preconditions`) and the state
//! entry each belongs to (its occurrence's revision), the claims that ended since her
//! last good read, and her live rejected-claim facts. Cognition's cycle reads it through the
//! `held_work` extension slot and carries it on the workspace, where her tool surface and her
//! held-work item (`context_streams::held_work`) both read it. This replaces `ActiveWorkSource`,
//! a RagSource that rendered her claims as one grounding blob and whose text the deliberation
//! faculty grepped for `[InProgress]`.
//!
//! The airc access lives behind [`AircWorkReader`] (a supertrait of `AircCitizen`, same shape as
//! `AircRosterReader`/`AircDoctrineReader`): the real runtime calls `Airc::work_roster_status`;
//! tests use a stub. Cross-activity by construction: the roster spans ALL rooms, so she sees her
//! work whichever room this turn is in.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use airc_lib::{AircError, WorkCard, WorkCardId};
use async_trait::async_trait;
use uuid::Uuid;

use crate::cognition::work_preconditions::Held;
use crate::cognition::workspace::{EndedClaim, HeldWork};

/// Reads THIS persona's currently-claimed work cards from airc. A supertrait of
/// `AircCitizen` (like `AircRosterReader`): the persona's runtime implements it
/// against its own airc handle, so a claim is read as the persona itself.
#[async_trait]
pub trait AircWorkReader: Send + Sync {
    /// The cards this persona currently owns/claims, across all rooms. Empty when
    /// it holds none (or no daemon, in tests).
    async fn active_claims(&self) -> Result<Vec<WorkCard>, AircError>;

    /// ONE card as the board carries it now, whoever holds it — the read a wake needs
    /// to tell a citizen what became of a card she believed she held (card c8303c32).
    /// `None` when no subscribed board has it (or no daemon, in tests). Default `None`
    /// so a reader that only knows its own claims still compiles; production answers.
    async fn card(&self, _card_id: uuid::Uuid) -> Option<WorkCard> {
        None
    }
}

/// A claim as her last successful read saw it: what it is called, its state, and the board
/// revision at the first read that saw it in that state (its state entry).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Seen {
    title: String,
    state: airc_work::CardState,
    entered_ms: u64,
}

/// Each resident's claims at her last successful read: the baseline an ended claim and a state
/// entry are read against. One entry per resident, replaced on every good read (bounded by the
/// population).
static BASELINES: OnceLock<Mutex<HashMap<Uuid, HashMap<WorkCardId, Seen>>>> = OnceLock::new();

/// Her held work now, read through her airc citizen; `None` when she has no citizen on this node
/// or the read failed (a degraded read is an absence, never an empty board).
pub async fn read(persona: Uuid) -> Option<HeldWork> {
    let runtime = crate::persona::PersonaAircRuntimeRegistry::try_global()?.get(persona)?;
    let holder = runtime.airc().peer_id();
    let claims = AircWorkReader::active_claims(runtime.as_ref()).await;
    let baselines = BASELINES.get_or_init(Default::default);
    let mut baselines = baselines.lock().unwrap_or_else(|poisoned| poisoned.into_inner()); // unwrap_or_else: a poisoned baseline map still holds the last good reads
    let baseline = baselines.entry(persona).or_default();
    let mut seen = (!baseline.is_empty()).then(|| std::mem::take(baseline));
    let at_ms = continuum_base::time::EpochMs::now().millis(); // the read's own time: what her moves are decided at
    let held = observe(&mut seen, holder, at_ms, claims, crate::persona::claim_rejections::recent(persona));
    *baseline = seen.unwrap_or_default(); // unwrap_or_default: no good read yet leaves no baseline
    held
}

/// PURE but for the baseline it advances: her held work from one read of her claims. A failed
/// read returns `None` and leaves the baseline untouched, so it never fabricates "you lost every
/// claim"; the first good read has no baseline and reports nothing ended; every later one reports
/// each claim gone since the last, once.
fn observe(
    baseline: &mut Option<HashMap<WorkCardId, Seen>>,
    holder: airc_core::PeerId,
    at_ms: u64,
    claims: Result<Vec<WorkCard>, AircError>,
    rejections: Vec<String>,
) -> Option<HeldWork> {
    let cards = match claims {
        Ok(cards) => cards,
        Err(error) => {
            continuum_base::probe!(
                class = "persona.held_work.read_failed",
                error = %error,
                "her claims could not be read: her turn carries no held work, and no claim reads as ended"
            );
            return None;
        }
    };
    // A card still in the state the last read saw keeps that entry; a new state (or a card new to
    // her) enters at the board's revision now. A heartbeat moves the revision, never the entry.
    let now: HashMap<WorkCardId, Seen> = cards
        .iter()
        .map(|card| {
            let kept = baseline.as_ref().and_then(|before| before.get(&card.card_id)).filter(|seen| seen.state == card.state);
            let entered_ms = kept.map_or(card.updated_at_ms, |seen| seen.entered_ms); // map_or: a state she has not seen it in enters now
            (card.card_id, Seen { title: card.title.clone(), state: card.state, entered_ms })
        })
        .collect();
    let mut ended: Vec<EndedClaim> = match baseline.as_ref() {
        Some(before) => before
            .iter()
            .filter(|(card, _)| !now.contains_key(*card))
            .map(|(card, seen)| EndedClaim { card: *card, title: seen.title.clone() })
            .collect(),
        None => Vec::new(),
    };
    ended.sort_unstable_by_key(|ended| ended.card); // reproducible order
    // Which claim her work turns are bound to, by the one selector hands use.
    let selected = crate::persona::work_focus::focus_workspace_card(cards.iter()).map(|card| card.card_id);
    let holds = cards
        .into_iter()
        .filter_map(|card| {
            let entered_ms = now.get(&card.card_id).map_or(card.updated_at_ms, |seen| seen.entered_ms); // map_or: every card read is in `now`
            Held::of(card, holder, at_ms, entered_ms)
        })
        .collect();
    *baseline = Some(now);
    Some(HeldWork { holds, selected, ended, rejections })
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_work::CardState;

    fn her() -> airc_core::PeerId {
        airc_core::PeerId::from_uuid(Uuid::from_u128(1))
    }

    fn card(id: u128, title: &str) -> WorkCard {
        let mut card = crate::cognition::work_preconditions::fixture_card(CardState::Claimed, Some(her()), None, title);
        card.card_id = WorkCardId::from_u128(id);
        card
    }

    const AT: u64 = crate::cognition::work_preconditions::FIXTURE_NOW_MS;

    fn degraded() -> Result<Vec<WorkCard>, AircError> {
        Err(AircError::UnknownPeer(airc_core::PeerId::new()))
    }

    // what this catches (#156, the Benchy silent-handoff case): a card that VANISHES between
    // reads (lease expired, re-claimed by another peer) is an ended claim exactly ONCE, even when
    // it was her LAST card; the next read carries none (a transition, not a nag loop); the first
    // read of a fresh mind reports none; a degraded read never fabricates one and keeps the
    // baseline for the read that really shows the card gone.
    #[test]
    fn an_ended_claim_is_reported_once_and_a_degraded_read_never_fabricates_one() {
        let mut baseline = None;
        let first = observe(&mut baseline, her(), AT, Ok(vec![card(1, "Millbrook Bakery")]), Vec::new()).expect("a good read");
        assert!(first.ended.is_empty(), "the first read has no baseline to lose from");
        assert_eq!(first.holds.len(), 1);
        assert!(observe(&mut baseline, her(), AT, degraded(), Vec::new()).is_none(), "a degraded read is an absence");
        let lost = observe(&mut baseline, her(), AT, Ok(vec![]), Vec::new()).expect("a good read");
        assert_eq!(lost.ended, vec![EndedClaim { card: WorkCardId::from_u128(1), title: "Millbrook Bakery".into() }]);
        assert!(lost.holds.is_empty());
        let after = observe(&mut baseline, her(), AT, Ok(vec![]), Vec::new()).expect("a good read");
        assert!(after.ended.is_empty(), "the baseline adopted: silence, not a nag loop");
    }

    // what this catches: two claims rendering as two "selected" cards, or a rejection fact dropped
    // when she holds nothing (the rejection-amnesia case, glass-boxed 2026-08-02: she reported the
    // rejection for three turns, then "I've claimed the task" once the receipts scrolled).
    #[test]
    fn one_claim_is_selected_and_a_rejection_rides_with_no_claims() {
        let mut baseline = None;
        let mut working = card(1, "Source-hygiene ratchet");
        working.state = CardState::InProgress;
        let mut review = card(2, "django__django-16136");
        review.state = CardState::Review;
        let both = observe(&mut baseline, her(), AT, Ok(vec![review, working]), Vec::new()).expect("a good read");
        assert_eq!(both.selected, Some(WorkCardId::from_u128(1)), "the actionable card is the selected one");
        let rejected = observe(&mut None, her(), AT, Ok(vec![]), vec!["[work] Your claim on card 44ebaa41 was REJECTED".into()]).expect("a good read");
        assert_eq!(rejected.rejections.len(), 1);
        assert!(rejected.holds.is_empty());
    }

    // what this catches (Codex on the occurrence contract): a state entry that moves with every
    // heartbeat (her card re-rendered as new every lease renewal) or that is reused when the card
    // re-enters a state (Review, InProgress, Review again resolving to the first Review). The entry
    // holds through heartbeats and is the board's revision at each new entry.
    #[test]
    fn a_state_entry_holds_through_heartbeats_and_is_new_on_reentry() {
        fn entered(baseline: &mut Option<HashMap<WorkCardId, Seen>>, card: &WorkCard) -> u64 {
            let held = observe(baseline, her(), AT, Ok(vec![card.clone()]), Vec::new()).expect("a good read"); // expect: Ok in
            held.holds.first().map_or(0, |held| held.entered_ms) // map_or: the fixture card is hers, so it is held
        }
        let mut baseline = None;
        let mut c = card(1, "x");
        c.state = CardState::Review;
        c.updated_at_ms = 10;
        assert_eq!(entered(&mut baseline, &c), 10);
        c.updated_at_ms = 15; // a heartbeat
        assert_eq!(entered(&mut baseline, &c), 10, "a heartbeat is not a new entry");
        c.state = CardState::InProgress;
        c.updated_at_ms = 20;
        assert_eq!(entered(&mut baseline, &c), 20);
        c.state = CardState::Review;
        c.updated_at_ms = 30;
        assert_eq!(entered(&mut baseline, &c), 30, "Review again is a new entry");
    }
}
