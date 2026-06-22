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
const SOURCE_ID: &str = "active-work";

/// Reads THIS persona's currently-claimed work cards from airc. A supertrait of
/// `AircCitizen` (like `AircRosterReader`): the persona's runtime implements it
/// against its own airc handle, so a claim is read as the persona itself.
#[async_trait]
pub trait AircWorkReader: Send + Sync {
    /// The cards this persona currently owns/claims, across all rooms. Empty when
    /// it holds none (or no daemon, in tests).
    async fn active_claims(&self) -> Result<Vec<WorkCard>, AircError>;
}

/// Cheap token estimate (≈4 chars/token) — grounding lines are short.
fn estimate_tokens(content: &str) -> u32 {
    ((content.len() / 4) as u32).max(1)
}

/// Persona-bound source reading the persona's own claimed work.
pub struct ActiveWorkSource {
    persona_id: uuid::Uuid,
    reader: Arc<dyn AircWorkReader>,
}

impl ActiveWorkSource {
    pub fn new(persona_id: uuid::Uuid, reader: Arc<dyn AircWorkReader>) -> Self {
        Self { persona_id, reader }
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
        // delivery, cognition stays up (good-citizen doctrine).
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
        if claims.is_empty() {
            return Self::empty();
        }

        let mut items: Vec<RagItem> = Vec::new();
        let mut tokens_used: u32 = 0;
        for card in &claims {
            let id8: String = card.card_id.as_uuid().to_string().chars().take(8).collect();
            // Human-readable line; structured parts also ride in metadata so
            // prompt-assembly / verifiers can render without re-parsing.
            let content = format!(
                "card {id8} [{state:?}] \"{title}\" (priority {prio:?})",
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
