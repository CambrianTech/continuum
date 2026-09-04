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

use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};

/// Source identifier — used by budget presets, telemetry, and the projection that
/// routes this delivery into system-prompt grounding (standing framing).
/// `pub(crate)` because the deliberation faculty's grounding floor keys on the SAME
/// name to give a held claim's card content first call on the reservation — one
/// constant, never a second literal that can drift.
pub(crate) const SOURCE_ID: &str = "active-work";

/// Reads THIS persona's currently-claimed work cards from airc. A supertrait of
/// `AircCitizen` (like `AircRosterReader`): the persona's runtime implements it
/// against its own airc handle, so a claim is read as the persona itself.
#[async_trait]
pub trait AircWorkReader: Send + Sync {
    /// The cards this persona currently owns/claims, across all rooms. Empty when
    /// it holds none (or no daemon, in tests).
    async fn active_claims(&self) -> Result<Vec<WorkCard>, AircError>;
}

/// Token estimate — the ONE canonical chars/4 estimator (`cognition::token_budget`),
/// so every grounding layer's number is in the same unit the replay ledger reports.
/// (Was a private byte-length copy that drifted — converged.)
use crate::cognition::token_budget::estimate_prompt_tokens as estimate_tokens;

/// Persona-bound source reading the persona's own claimed work.
pub struct ActiveWorkSource {
    persona_id: uuid::Uuid,
    reader: Arc<dyn AircWorkReader>,
    /// Card ids → titles from the LAST successful read — the diff memory behind
    /// the lost-claim transition fact (#156). `None` until the first successful
    /// read (a fresh mind has no baseline to lose from). A sync Mutex locked
    /// only AFTER the `active_claims` await resolves — never held across await.
    prev_claims: std::sync::Mutex<Option<std::collections::HashMap<uuid::Uuid, String>>>,
}

impl ActiveWorkSource {
    pub fn new(persona_id: uuid::Uuid, reader: Arc<dyn AircWorkReader>) -> Self {
        Self {
            persona_id,
            reader,
            prev_claims: std::sync::Mutex::new(None),
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
        let id8: String = card_id.to_string().chars().take(8).collect();
        let content = format!(
            "[work] Your claim on card {id8} \"{title}\" is no longer held by you \
             (lease expired or released). Do not continue work on it as yours — \
             check the board before touching it again."
        );
        let tokens = estimate_tokens(&content);
        RagItem {
            content,
            tokens,
            metadata: json!({
                "fact": "claim_lost",
                "card_id": card_id.to_string(),
            }),
        }
    }

    fn empty() -> RagDelivery {
        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items: Vec::new(),
            tokens_used: 0,
            continuation: None,
            resolution_used: ResolutionPreference::Placeholder,
        }
    }
}

#[async_trait]
impl RagSource for ActiveWorkSource {
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }

    fn expand_command(&self) -> Option<&'static str> {
        Some("work/list")
    }

    /// One claimed-card line — id, title, state. Same shape and size as the
    /// board's per-card line, which measured ~26 tokens live.
    fn floor_tokens(&self) -> u32 {
        32
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        // Persona-scoped: a cross-persona ctx gets nothing.
        if ctx.persona_id != self.persona_id {
            return Self::empty();
        }

        // One airc call (board-wide, all rooms). Failure is non-fatal — empty
        // delivery, cognition stays up (good-citizen doctrine). CRUCIALLY the
        // early return leaves `prev_claims` untouched: a degraded read must
        // never fabricate "you lost everything" transition facts.
        let claims = match self.reader.active_claims().await {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    persona_id = %self.persona_id,
                    "active_work: active_claims failed — empty delivery, cognition stays up"
                );
                return Self::empty();
            }
        };

        // Diff against the last SUCCESSFUL read (#156): cards that vanished are
        // lost claims — surface each ONCE as a transition fact, then adopt the
        // new baseline. Runs even when `claims` is empty: losing your LAST card
        // is exactly the silent-handoff case this exists for.
        let now: std::collections::HashMap<uuid::Uuid, String> = claims
            .iter()
            .map(|c| (c.card_id.as_uuid(), c.title.clone()))
            .collect();
        let lost: Vec<(uuid::Uuid, String)> = {
            let mut prev = self.prev_claims.lock().unwrap_or_else(|p| p.into_inner());
            let lost = match prev.as_ref() {
                Some(before) => before
                    .iter()
                    .filter(|(id, _)| !now.contains_key(*id))
                    .map(|(id, title)| (*id, title.clone()))
                    .collect(),
                // First successful read: no baseline, nothing to lose from.
                None => Vec::new(),
            };
            *prev = Some(now);
            lost
        };

        let mut items: Vec<RagItem> = Vec::new();
        let mut tokens_used: u32 = 0;
        for (card_id, title) in &lost {
            let item = Self::lost_claim_item(card_id, title);
            if tokens_used.saturating_add(item.tokens) > budget {
                break;
            }
            tokens_used += item.tokens;
            items.push(item);
        }
        // Rejected-claim facts (the #159-family sibling of the lost-claim
        // diff): `work/claim` records rejections into the per-persona ring;
        // rendering them here keeps "that card is NOT yours" in perception
        // past the raw receipt's short window — the rejection-amnesia fix
        // (glass-boxed 2026-08-02: accurate rejection reports for three
        // turns, then "I've claimed the task" once the receipts scrolled).
        for line in crate::persona::claim_rejections::recent(self.persona_id) {
            let tokens = estimate_tokens(&line);
            if tokens_used.saturating_add(tokens) > budget {
                break;
            }
            tokens_used += tokens;
            items.push(RagItem {
                content: line,
                tokens,
                metadata: json!({ "fact": "claim_rejected" }),
            });
        }
        if claims.is_empty() && items.is_empty() {
            return Self::empty();
        }
        // ONE card is this turn's (`work_focus`); the rest are held, not worked now —
        // said in the line itself, so two held cards never read as two jobs at once.
        let focus = crate::persona::work_focus::focus_card(claims.iter()).map(|c| c.card_id);
        for card in &claims {
            let id8: String = card.card_id.as_uuid().to_string().chars().take(8).collect();
            // Human-readable line; structured parts also ride in metadata so
            // prompt-assembly / verifiers can render without re-parsing.
            let stance = if claims.len() > 1 {
                if focus == Some(card.card_id) { " — THIS TURN" } else { " — held, not this turn" }
            } else {
                ""
            };
            let content = format!(
                "card {id8} [{state:?}] \"{title}\" (priority {prio:?}){stance}",
                state = card.state,
                title = card.title,
                prio = card.priority,
            );
            let tokens = estimate_tokens(&content);
            if tokens_used.saturating_add(tokens) > budget {
                // Budget exhausted — a truncated work list is still truthful for
                // the cards it names. Atomic unit = one card; no continuation.
                break;
            }
            tokens_used += tokens;
            items.push(RagItem {
                content,
                tokens,
                metadata: json!({
                    "card_id": card.card_id.as_uuid().to_string(),
                    "state": format!("{:?}", card.state),
                    "claim_id": card.claim_id.map(|c| c.as_uuid().to_string()),
                }),
            });
        }

        tracing::debug!(
            persona_id = %self.persona_id,
            budget,
            cards = items.len(),
            tokens_used,
            "active_work: deliver"
        );

        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items,
            tokens_used,
            continuation: None,
            resolution_used: resolution,
        }
    }

    async fn deliver_continuation(
        &self,
        _ctx: &RagContext,
        _cursor: ContinuationCursor,
        _budget: u32,
    ) -> Option<RagDelivery> {
        // Atomic units (one card each), no pagination — same as the roster.
        None
    }
}

/// True when an [active-work] contribution's rendered content names a card this
/// persona currently HOLDS in progress. Colocated with the `deliver` renderer above
/// (which writes `card <id8> [{state:?}] "<title>" …` for HELD cards only — the
/// lost-claim and rejection facts carry no state tag) so the wire format and its one
/// reader cannot drift apart. Matching our OWN emitted format is protocol decoding,
/// not inference about prose. Consumed by the deliberation prompt to choose the
/// working-presence contract on undirected turns — a structural claim-state fact.
pub(crate) fn renders_held_in_progress(active_work_content: &str) -> bool {
    active_work_content.contains("[InProgress]")
}

#[cfg(test)]
mod tests {
    use super::*;
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
            claim_id: None,
            claim_expires_at_ms: None,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: airc_core::PeerId::new(),
            created_at_ms: 1_000_000,
            updated_at_ms: 1_000_000,
            reviews: None,
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
            .deliver(&ctx(persona), 10_000, ResolutionPreference::Raw)
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
            .deliver(&ctx(other), 10_000, ResolutionPreference::Raw)
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
            .deliver(&ctx(persona), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(d1.items.len(), 1);
        assert!(d1.items[0].metadata.get("fact").is_none());

        // Read 2: the card vanished → the transition fact, once, loud.
        let d2 = src
            .deliver(&ctx(persona), 1_000, ResolutionPreference::Raw)
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
            .deliver(&ctx(persona), 1_000, ResolutionPreference::Raw)
            .await;
        assert!(d3.items.is_empty());
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
            .deliver(&ctx(persona), 1_000, ResolutionPreference::Raw)
            .await;
        // Degraded read: empty delivery, NO loss facts, baseline preserved.
        let err = src
            .deliver(&ctx(persona), 1_000, ResolutionPreference::Raw)
            .await;
        assert!(
            err.items.is_empty(),
            "degraded read stays empty — never a fake loss"
        );
        // Recovered read still holding: no facts (nothing was ever lost).
        let ok = src
            .deliver(&ctx(persona), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(ok.items.len(), 1);
        assert!(ok.items[0].metadata.get("fact").is_none());
        // NOW it's genuinely gone → the fact fires from the preserved baseline.
        let lost = src
            .deliver(&ctx(persona), 1_000, ResolutionPreference::Raw)
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
            .deliver(&ctx(persona), 1_000, ResolutionPreference::Raw)
            .await;
        assert!(
            d.items.is_empty(),
            "empty first read = empty delivery, no facts"
        );
    }
}
