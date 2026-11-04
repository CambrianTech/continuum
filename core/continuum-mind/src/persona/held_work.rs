//! Her held work, read from her own board once per tick (STATE-DRIVEN-SURFACE.md): the cards she
//! holds with the moves each one's state gives her (`card_moves`), the claims that ended since her
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

use crate::cognition::card_moves::Held;
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

/// Each resident's claims at her last successful read: the baseline an ended claim is diffed
/// against. One entry per resident, replaced on every good read (bounded by the population).
static BASELINES: OnceLock<Mutex<HashMap<Uuid, HashMap<WorkCardId, String>>>> = OnceLock::new();

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
    let held = observe(&mut seen, holder, claims, crate::persona::claim_rejections::recent(persona));
    *baseline = seen.unwrap_or_default(); // unwrap_or_default: no good read yet leaves no baseline
    held
}

/// PURE but for the baseline it advances: her held work from one read of her claims. A failed
/// read returns `None` and leaves the baseline untouched, so it never fabricates "you lost every
/// claim"; the first good read has no baseline and reports nothing ended; every later one reports
/// each claim gone since the last, once.
fn observe(
    baseline: &mut Option<HashMap<WorkCardId, String>>,
    holder: airc_core::PeerId,
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
    let now: HashMap<WorkCardId, String> = cards.iter().map(|card| (card.card_id, card.title.clone())).collect();
    let mut ended: Vec<EndedClaim> = match baseline.as_ref() {
        Some(before) => before
            .iter()
            .filter(|(card, _)| !now.contains_key(*card))
            .map(|(card, title)| EndedClaim { card: *card, title: title.clone() })
            .collect(),
        None => Vec::new(),
    };
    ended.sort_unstable_by_key(|ended| ended.card); // reproducible order
    *baseline = Some(now);
    // Which claim her work turns are bound to, by the one selector hands use.
    let selected = crate::persona::work_focus::focus_workspace_card(cards.iter()).map(|card| card.card_id);
    let holds = cards.into_iter().filter_map(|card| Held::of(card, holder)).collect();
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
        let mut card = crate::cognition::card_moves::fixture_card(CardState::Claimed, Some(her()), None, title);
        card.card_id = WorkCardId::from_u128(id);
        card
    }

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
        let first = observe(&mut baseline, her(), Ok(vec![card(1, "Millbrook Bakery")]), Vec::new()).expect("a good read");
        assert!(first.ended.is_empty(), "the first read has no baseline to lose from");
        assert_eq!(first.holds.len(), 1);
        assert!(observe(&mut baseline, her(), degraded(), Vec::new()).is_none(), "a degraded read is an absence");
        let lost = observe(&mut baseline, her(), Ok(vec![]), Vec::new()).expect("a good read");
        assert_eq!(lost.ended, vec![EndedClaim { card: WorkCardId::from_u128(1), title: "Millbrook Bakery".into() }]);
        assert!(lost.holds.is_empty());
        let after = observe(&mut baseline, her(), Ok(vec![]), Vec::new()).expect("a good read");
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
        let both = observe(&mut baseline, her(), Ok(vec![review, working]), Vec::new()).expect("a good read");
        assert_eq!(both.selected, Some(WorkCardId::from_u128(1)), "the actionable card is the selected one");
        let rejected = observe(&mut None, her(), Ok(vec![]), vec!["[work] Your claim on card 44ebaa41 was REJECTED".into()]).expect("a good read");
        assert_eq!(rejected.rejections.len(), 1);
        assert!(rejected.holds.is_empty());
    }
}
