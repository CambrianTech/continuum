//! The awareness strip as a grounding source: what she perceives across ALL her
//! activities, loudest first, with her continuation and her dial (EVENT-MIND §6, the
//! strip). One `RagSource` like the roster and the doctrine, so it rides the same
//! budgeter, the same capture and the same prefix order as every other grounding.
//! Without it (found 2026-10-04 by BigMama while building the Wake consumer) the fold
//! was published on a watch nobody held: a Resume turn could not show her own note.
use std::sync::Arc;

use crate::persona::active_work_source::AircWorkReader;
use crate::cognition::rag_budget::{RagContext, RagDelivery, RagItem, RagSource};

use crate::cognition::faculty_ids::AWARENESS;

/// The heading the block renders under; fixed, so the prefix bytes are stable.
const HEADING: &str = "Across your activities (loudest first):";

pub struct AwarenessRagSource {
    persona_id: uuid::Uuid,
    /// Her own airc handle, for the one fact the strip needs that her region does not
    /// hold: whether she holds work. `None` when the offer cannot be computed.
    claims: Option<Arc<dyn AircWorkReader>>,
}

/// THE OFFER (BigMama, step 1b of HER-LOOP phase 2): she holds work and has written no
/// continuation, so the strip's lead line says so and names the verb. An offer in her
/// perception; the system never writes her note for her. The 2026-09-02 shape (22 cards
/// claimed, zero progress) becomes a line she can read instead of a silent stall.
pub(crate) fn offer_line(held: usize) -> String {
    format!(
        "[working] you hold {held} card{} with no note of where you are; focus/continue writes one",
        if held == 1 { "" } else { "s" }
    )
}

impl AwarenessRagSource {
    pub fn new(persona_id: uuid::Uuid, claims: Option<Arc<dyn AircWorkReader>>) -> Self {
        Self { persona_id, claims }
    }

    /// Keyed lines for the turn: the offer first when it applies (her continuation leads
    /// the strip when she has one; the offer stands in its place when she has none). A
    /// key is the identity of what the line describes, so the view re-appends a line only
    /// when its activity's line changes.
    async fn lines_for_turn(&self, snapshot: &crate::persona::awareness::AwarenessSnapshot, room: Option<uuid::Uuid>) -> Vec<(String, String)> {
        // `render_lines_in` emits the continuation (if any) then one line per visible activity.
        let mut keys: Vec<String> = Vec::new();
        if snapshot.continuation.is_some() {
            keys.push("continuation".to_owned());
        }
        keys.extend(snapshot.visible().iter().map(|l| l.activity.to_string()));
        let mut lines: Vec<(String, String)> =
            keys.into_iter().zip(snapshot.render_lines_in(self.persona_id, room)).collect();
        if snapshot.continuation.is_none() {
            if let Some(reader) = &self.claims {
                match reader.active_claims().await {
                    Ok(claims) if !claims.is_empty() => lines.insert(0, ("offer".to_owned(), offer_line(claims.len()))),
                    Ok(_) => {}
                    Err(e) => continuum_base::probe!(
                        class = "mind.strip.claims_unreadable",
                        persona = %self.persona_id,
                        error = %e.to_string(),
                        "her claims could not be read for the offer — the strip stands without it"
                    ),
                }
            }
        }
        lines
    }
}

#[async_trait::async_trait]
impl RagSource for AwarenessRagSource {
    fn source_id(&self) -> &'static str {
        AWARENESS
    }

    fn expand_command(&self) -> Option<&'static str> {
        // The strip is already her whole awareness, reduced; nothing fuller to expand to.
        None
    }

    async fn deliver(&self, ctx: &RagContext) -> RagDelivery {
        if ctx.persona_id != self.persona_id {
            return RagDelivery::empty(AWARENESS);
        }
        let Some(snapshot) = crate::persona::perception_feed::awareness_of(self.persona_id, ctx.now_ms) else {
            return RagDelivery::empty(AWARENESS);
        };
        let lines = self.lines_for_turn(&snapshot, ctx.airc_room.map(|r| r.as_uuid())).await;
        if lines.is_empty() {
            return RagDelivery::empty(AWARENESS);
        }
        // The heading is its own first item: the lines read as nothing without it. Each
        // line follows, loudest first, as its own unit for the view to level. Each is its own
        // pointer: there is no verb to reopen the strip, so collapsed it stays as it is.
        let meta = serde_json::json!({
            "live_activities": snapshot.load.live_activities,
            "unread_total": snapshot.load.unread_total,
        });
        let mut items = Vec::with_capacity(lines.len() + 1);
        items.push(RagItem::whole("heading", HEADING, meta.clone()).with_pointer(HEADING));
        items.extend(lines.iter().map(|(key, line)| RagItem::whole(key.clone(), line.clone(), meta.clone()).with_pointer(line.clone())));
        let delivery = RagDelivery::of(AWARENESS, items, false);
        continuum_base::probe!(
            class = "mind.strip.delivered",
            persona = %self.persona_id,
            lines_total = lines.len() as u64,
            tokens = delivery.tokens_used,
            "her awareness strip is in this turn's grounding"
        );
        delivery
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the offer names the verb and the count, and is an offer (the
    // word "writes one" — it never claims to have written anything).
    #[test]
    fn the_offer_names_the_verb_and_never_writes() {
        let one = offer_line(1);
        assert!(one.starts_with("[working] you hold 1 card with no note") && one.contains("focus/continue writes one"), "{one}");
        assert!(offer_line(3).contains("3 cards"));
    }
}
