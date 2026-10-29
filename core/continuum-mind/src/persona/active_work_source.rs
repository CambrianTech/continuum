//! ActiveWorkSource — grounds a persona in ITS OWN live work across the grid.
//!
//! The glass box showed a persona misremembering a card it already owned, because
//! its prompt never contained its current work state. The WRONG fix is hardcoding
//! "your card is X". The RIGHT, dynamic fix is grounding: a `RagSource` that, every
//! turn, surfaces the persona's claimed cards + their states into context — read
//! live from airc's own work substrate, encapsulating airc, not reinventing it.
//!
//! Cross-activity by construction: the work roster spans ALL rooms, so a persona
//! sees its work regardless of which room this turn is in — one mind, many
//! activities, no severance ([[grid-distributed-cognition]]).
//!
//! The airc access lives behind [`AircWorkReader`] (a supertrait of `AircCitizen`,
//! same shape as `AircRosterReader`/`AircDoctrineReader`): the real runtime calls
//! `Airc::work_roster_status`; tests use a stub. So this source is unit-testable
//! and never holds a raw airc handle.

use std::sync::Arc;

use airc_lib::{AircError, WorkCard};
use async_trait::async_trait;
use serde_json::json;

use crate::cognition::rag_budget::{RagContext, RagDelivery, RagItem, RagSource};

use crate::cognition::faculty_ids::ACTIVE_WORK;

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

/// Persona-bound source reading the persona's own claimed work.
pub struct ActiveWorkSource {
    persona_id: uuid::Uuid,
    reader: Arc<dyn AircWorkReader>,
    /// Observation and source-admission state, never held across an await.
    claims_seen: std::sync::Mutex<ClaimsSeen>,
}

/// The last successful snapshot, the baseline a lost claim is diffed against. Every
/// transition is delivered the turn it is seen (nothing is refused for size), so no
/// historical notice is retained: the authoritative board owns detail.
#[derive(Default)]
struct ClaimsSeen {
    current: Option<std::collections::HashMap<uuid::Uuid, String>>,
}

impl ActiveWorkSource {
    pub fn new(persona_id: uuid::Uuid, reader: Arc<dyn AircWorkReader>) -> Self {
        Self {
            persona_id,
            reader,
            claims_seen: std::sync::Mutex::new(ClaimsSeen::default()),
        }
    }

    /// The lost-claim transition fact (#156, the Benchy case): a lease that
    /// expired or a card released between reads VANISHES from `active_claims`
    /// silently, and the persona keeps planning work on a card it no longer
    /// holds ("I've already claimed…", turns after peer 90e758b2 legitimately
    /// re-claimed it). One honest fact at the transition — we know the claim is
    /// GONE from this read; we do NOT know who holds it now (that would need a
    /// board read), so the fact says exactly that and points at the board.
    fn lost_claim_item(card_id: &uuid::Uuid, title: &str) -> RagItem {
        let id8 = airc_core::shown_id::shown_form(*card_id);
        let content = format!(
            "[work] Your claim on card {id8} \"{title}\" is no longer held by you \
             (lease expired or released). Do not continue work on it as yours — \
             check the board before touching it again."
        );
        // The key is the card: the transition is one fact about one card. The pointer
        // keeps the "not yours any more" fact alive once the whole line collapses.
        RagItem::whole(
            format!("lost:{card_id}"),
            content,
            json!({
                "fact": "claim_lost",
                "card_id": card_id.to_string(),
            }),
        )
        .with_pointer(format!("[work] claim on card {id8} ended — work/list"))
    }

    /// A rejection fact is keyed by its own text: the ring (`claim_rejections`) hands out
    /// lines, not ids, and one line is one rejection for as long as it lives there.
    fn rejected_claim_item(line: String) -> RagItem {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        line.hash(&mut hasher);
        let key = format!("rejected:{:016x}", hasher.finish());
        RagItem::whole(key, line, json!({ "fact": "claim_rejected" }))
            .with_pointer("[work] a claim was rejected — work/list")
    }
}

#[async_trait]
impl RagSource for ActiveWorkSource {
    fn source_id(&self) -> &'static str {
        ACTIVE_WORK
    }

    fn expand_command(&self) -> Option<&'static str> {
        Some("work/list")
    }

    async fn deliver(&self, ctx: &RagContext) -> RagDelivery {
        // Persona-scoped: a cross-persona ctx gets nothing.
        if ctx.persona_id != self.persona_id {
            return RagDelivery::empty(ACTIVE_WORK);
        }

        // One airc call (board-wide, all rooms). Failure is non-fatal — empty
        // delivery, cognition stays up (good-citizen doctrine). CRUCIALLY the
        // early return leaves `claims_seen` untouched: a degraded read must
        // never fabricate "you lost everything" transition facts.
        let claims = match self.reader.active_claims().await {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    persona_id = %self.persona_id,
                    "active_work: active_claims failed — empty delivery, cognition stays up"
                );
                return RagDelivery::empty(ACTIVE_WORK);
            }
        };

        // Reconcile the successful observation with the baseline: every claim that
        // vanished since the last good read is delivered now, as an item.
        let now: std::collections::HashMap<uuid::Uuid, String> = claims
            .iter()
            .map(|c| (c.card_id.as_uuid(), c.title.clone()))
            .collect();
        let mut items: Vec<RagItem> = Vec::new();
        {
            let mut seen = self.claims_seen.lock().unwrap_or_else(|p| p.into_inner());
            let mut lost: Vec<_> = match seen.current.as_ref() {
                Some(before) => before
                    .iter()
                    .filter(|(id, _)| !now.contains_key(*id))
                    .map(|(id, title)| (*id, title.clone()))
                    .collect(),
                // First successful read: no baseline, nothing to lose from.
                None => Vec::new(),
            };
            // Stable ordering keeps the delivery reproducible.
            lost.sort_unstable_by_key(|(id, _)| *id);
            items.extend(lost.iter().map(|(card_id, title)| Self::lost_claim_item(card_id, title)));
            seen.current = Some(now);
        }
        // Rejected-claim facts (the #159-family sibling of the lost-claim
        // diff): `work/claim` records rejections into the per-persona ring;
        // rendering them here keeps "that card is NOT yours" in perception
        // past the raw receipt's short window — the rejection-amnesia fix
        // (glass-boxed 2026-08-02: accurate rejection reports for three
        // turns, then "I've claimed the task" once the receipts scrolled).
        items.extend(
            crate::persona::claim_rejections::recent(self.persona_id)
                .into_iter()
                .map(Self::rejected_claim_item),
        );
        if claims.is_empty() && items.is_empty() {
            return RagDelivery::empty(ACTIVE_WORK);
        }
        // Claim selection is not a receipt that this running turn rebound its hands.
        // WorkClaim may select/stage another card during a burst, while that burst
        // retains its admitted workspace. Describe the shared selector's result only.
        let focus = crate::persona::work_focus::focus_workspace_card(claims.iter()).map(|c| c.card_id);
        for card in &claims {
            let id8 = airc_core::shown_id::shown_form(card.card_id.as_uuid());
            // Human-readable line; structured parts also ride in metadata so
            // prompt-assembly / verifiers can render without re-parsing.
            let stance = if claims.len() > 1 {
                if focus == Some(card.card_id) {
                    " \u{2014} selected claim"
                } else {
                    " \u{2014} held"
                }
            } else {
                ""
            };
            let content = format!(
                "card {id8} [{state:?}] \"{title}\" (priority {prio:?}){stance}",
                state = card.state,
                title = card.title,
                prio = card.priority,
            );
            items.push(
                RagItem::whole(
                    format!("card:{}", card.card_id.as_uuid()),
                    content,
                    json!({
                        "card_id": card.card_id.as_uuid().to_string(),
                        "state": format!("{:?}", card.state),
                        "claim_id": card.claim_id.map(|c| c.as_uuid().to_string()),
                    }),
                )
                .with_pointer(format!("[card {id8}] you hold it — work/list")),
            );
        }

        let delivery = RagDelivery::of(ACTIVE_WORK, items, false);
        tracing::debug!(
            persona_id = %self.persona_id,
            items = delivery.items.len(),
            tokens_used = delivery.tokens_used,
            "active_work: deliver"
        );
        delivery
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::faculty_ids::{renders_held_card, renders_held_in_progress};
    use airc_work::{CardState, Priority, RepoId, WorkCardId};
    use std::sync::Mutex;
    use uuid::Uuid;

    // what this catches: the held-card wire format and its one reader drifting
    // apart — `renders_held_in_progress` keys the working-presence contract, so a
    // renderer that stops writing `[InProgress]` (or a fact line that starts
    // matching) silently flips ambient turns back to the conversational contract.
    #[test]
    fn held_card_line_matches_and_fact_lines_do_not() {
        assert!(renders_held_in_progress(
            "card feadd5dc [InProgress] \"PROJECT [swe] psf__requests-2148\" (priority P1)"
        ));
        let lost = ActiveWorkSource::lost_claim_item(&Uuid::new_v4(), "some card");
        assert!(!renders_held_in_progress(&lost.content));
        assert!(!renders_held_in_progress(""));
        // The lane gate's key: a CLAIMED card is held work too (2026-09-15).
        assert!(renders_held_card(
            "card feadd5dc [Claimed] \"PROJECT [swe] psf__requests-2148\" (priority P1)"
        ));
        assert!(renders_held_card("card feadd5dc [InProgress] \"x\""));
        assert!(!renders_held_card(&lost.content));
    }

    struct StubWork {
        /// Each deliver pops the front result; `Err` = degraded read.
        results: Mutex<Vec<Result<Vec<WorkCard>, ()>>>,
    }

    #[async_trait]
    impl AircWorkReader for StubWork {
        async fn active_claims(&self) -> Result<Vec<WorkCard>, AircError> {
            match self.results.lock().unwrap().remove(0) {
                Ok(cards) => Ok(cards),
                Err(()) => Err(AircError::UnknownPeer(airc_core::PeerId::new())),
            }
        }
    }

    fn card(title: &str) -> WorkCard {
        WorkCard {
            card_id: WorkCardId::new(),
            repo: RepoId::new("acme/continuum").expect("valid repo id in fixture"),
            title: title.to_string(),
            body: None,
            priority: Priority::P2,
            lane_id: None,
            state: CardState::Claimed,
            owner: None,
            claim_provenance: None,
            claim_id: None,
            claim_expires_at_ms: None,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: airc_core::PeerId::new(),
            created_at_ms: 1_000_000,
            updated_at_ms: 1_000_000,
            reviews: None,
            submissions: Vec::new(),
            last_submission_rejection: None,
        }
    }

    fn ctx(persona: Uuid) -> RagContext {
        RagContext::for_persona(persona, 1_000_000)
    }

    fn source(persona: Uuid, results: Vec<Result<Vec<WorkCard>, ()>>) -> ActiveWorkSource {
        ActiveWorkSource::new(
            persona,
            Arc::new(StubWork {
                results: Mutex::new(results),
            }),
        )
    }

    // what this catches: a newer review heartbeat must not change selection, and
    // a new explicit claim must not be advertised as an already-rebound turn.
    #[tokio::test]
    async fn selected_claim_stance_does_not_assert_turn_binding() {
        let persona = Uuid::new_v4();
        let mut working = card("Source-hygiene ratchet");
        working.state = CardState::InProgress;
        working.last_heartbeat_at_ms = Some(1_000_000);
        let mut in_review = card("[bench swe-bench-verified] django__django-16136");
        in_review.state = CardState::Review;
        in_review.last_heartbeat_at_ms = Some(2_000_000);
        let mut selected = card("New explicit task");
        selected.claim_provenance = Some(airc_work::model::ClaimProvenance {
            origin: airc_work::ClaimOrigin::Explicit,
            selected_at_ms: 3_000_000,
        });
        let src = source(persona, vec![
            Ok(vec![in_review.clone(), working.clone()]),
            Ok(vec![in_review, working, selected]),
        ]);
        for selected_title in ["Source-hygiene", "New explicit task"] {
            let delivery = src.deliver(&ctx(persona)).await;
            let selected_lines: Vec<_> = delivery.items.iter()
                .filter(|item| item.content.contains(" \u{2014} selected claim"))
                .collect();
            assert_eq!(selected_lines.len(), 1);
            assert!(selected_lines[0].content.contains(selected_title));
            assert!(delivery.items.iter().all(|item|
                !item.content.contains("THIS TURN") && !item.content.contains("not this turn")));
            let review = delivery.items.iter().find(|item| item.content.contains("django")).expect("review remains held");
            assert!(review.content.ends_with(" \u{2014} held"));
        }
    }

    // what this catches: the rejection-amnesia fix (the #159-family
    // sibling of claim_lost) — a rejection recorded by work/claim renders
    // as a [work] claim_rejected fact even when the persona holds ZERO
    // cards (exactly the amnesia case: nothing claimed, receipt gone,
    // belief resurfacing), and ONLY for the persona it belongs to.
    #[tokio::test]
    async fn rejected_claim_renders_as_fact_for_its_persona_only() {
        let persona = Uuid::new_v4();
        let other = Uuid::new_v4();
        crate::persona::claim_rejections::record(
            persona,
            "44ebaa41",
            "already claimed by another peer",
        );

        let src = source(persona, vec![Ok(vec![]), Ok(vec![])]);
        let delivery = src
            .deliver(&ctx(persona))
            .await;
        assert_eq!(
            delivery.items.len(),
            1,
            "rejection fact renders with zero claims"
        );
        assert!(delivery.items[0].content.contains("44ebaa41"));
        assert!(delivery.items[0].content.contains("REJECTED"));
        assert_eq!(delivery.items[0].metadata["fact"], "claim_rejected");

        // Another persona's source never sees it.
        let src_other = source(other, vec![Ok(vec![])]);
        let delivery = src_other
            .deliver(&ctx(other))
            .await;
        assert!(delivery.items.is_empty());
    }

    // what this catches (#156, the Benchy silent-handoff case): a card that
    // VANISHES between reads — lease expired or re-claimed by another peer —
    // must surface as a [work] claim_lost transition fact exactly ONCE, even
    // when it was the persona's LAST card (the empty-claims path). The next
    // read carries no fact (baseline adopted): a transition, not a nag loop.
    #[tokio::test]
    async fn lost_claim_surfaces_once_even_when_it_was_the_last_card() {
        let persona = Uuid::new_v4();
        let millbrook = card("Build and launch the Millbrook Bakery website");
        let src = source(
            persona,
            vec![Ok(vec![millbrook.clone()]), Ok(vec![]), Ok(vec![])],
        );

        // Read 1: holds the card — normal grounding line, no facts.
        let d1 = src
            .deliver(&ctx(persona))
            .await;
        assert_eq!(d1.items.len(), 1);
        assert!(d1.items[0].metadata.get("fact").is_none());

        // Read 2: the card vanished → the transition fact, once, loud.
        let d2 = src
            .deliver(&ctx(persona))
            .await;
        assert_eq!(d2.items.len(), 1);
        assert_eq!(d2.items[0].metadata["fact"], "claim_lost");
        assert!(
            d2.items[0].content.contains("Millbrook"),
            "names the lost card"
        );
        assert!(
            d2.items[0].content.contains("no longer held by you"),
            "states the transition plainly"
        );

        // Read 3: baseline adopted — silence, not a nag loop.
        let d3 = src
            .deliver(&ctx(persona))
            .await;
        assert!(d3.items.is_empty());
    }

    // what this catches: the old budget-coupled state — every ended claim is delivered
    // the turn it is seen, however long its title, with a stable per-card key and a
    // pointer; nothing is deferred to a later turn, and another persona's context
    // neither sees the facts nor disturbs the baseline.
    #[tokio::test]
    async fn every_lost_claim_is_delivered_at_once_with_keys_and_pointers() {
        let persona = Uuid::new_v4();
        let mut large = card(&"long title ".repeat(100));
        let mut small = card("small");
        large.card_id = WorkCardId::from_uuid(Uuid::from_u128(1));
        small.card_id = WorkCardId::from_uuid(Uuid::from_u128(2));
        let src = source(
            persona,
            vec![
                Ok(vec![large.clone(), small.clone()]),
                Ok(vec![]),
                Ok(vec![large.clone()]),
            ],
        );
        src.deliver(&ctx(persona)).await;
        let lost = src.deliver(&ctx(persona)).await;
        let keys: Vec<_> = lost.items.iter().map(|i| i.key.clone()).collect();
        assert_eq!(
            keys,
            [
                format!("lost:{}", large.card_id.as_uuid()),
                format!("lost:{}", small.card_id.as_uuid()),
            ]
        );
        assert!(lost.items.iter().all(|i| i.pointer.is_some() && i.metadata["fact"] == "claim_lost"));
        assert!(!lost.truncated);
        assert!(src.deliver(&ctx(Uuid::new_v4())).await.items.is_empty());
        // Reacquisition: a plain current claim keyed by its card id, no loss fact.
        let regained = src.deliver(&ctx(persona)).await;
        assert_eq!(regained.items.len(), 1);
        assert_eq!(regained.items[0].key, format!("card:{}", large.card_id.as_uuid()));
        assert!(regained.items[0].metadata.get("fact").is_none());
    }

    // what this catches: a DEGRADED read must never fabricate loss — the
    // baseline survives the error, and the loss fact only fires when a
    // SUCCESSFUL read actually shows the card gone.
    #[tokio::test]
    async fn degraded_read_never_fabricates_loss() {
        let persona = Uuid::new_v4();
        let c = card("real work");
        let src = source(
            persona,
            vec![Ok(vec![c.clone()]), Err(()), Ok(vec![c]), Ok(vec![])],
        );

        let _hold = src
            .deliver(&ctx(persona))
            .await;
        // Degraded read: empty delivery, NO loss facts, baseline preserved.
        let err = src
            .deliver(&ctx(persona))
            .await;
        assert!(
            err.items.is_empty(),
            "degraded read stays empty — never a fake loss"
        );
        // Recovered read still holding: no facts (nothing was ever lost).
        let ok = src
            .deliver(&ctx(persona))
            .await;
        assert_eq!(ok.items.len(), 1);
        assert!(ok.items[0].metadata.get("fact").is_none());
        // NOW it's genuinely gone → the fact fires from the preserved baseline.
        let lost = src
            .deliver(&ctx(persona))
            .await;
        assert_eq!(lost.items.len(), 1);
        assert_eq!(lost.items[0].metadata["fact"], "claim_lost");
    }

    // what this catches: the FIRST successful read has no baseline — a fresh
    // mind (or a rebooted core) must not hallucinate losses it never held.
    #[tokio::test]
    async fn first_read_carries_no_loss_facts() {
        let persona = Uuid::new_v4();
        let src = source(persona, vec![Ok(vec![])]);
        let d = src
            .deliver(&ctx(persona))
            .await;
        assert!(
            d.items.is_empty(),
            "empty first read = empty delivery, no facts"
        );
    }
}
