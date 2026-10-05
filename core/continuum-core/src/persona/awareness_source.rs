//! The awareness strip as a grounding source: what she perceives across ALL her
//! activities, loudest first, with her continuation and her dial (EVENT-MIND §6, the
//! strip). One `RagSource` like the roster and the doctrine, so it rides the same
//! budgeter, the same capture and the same prefix order as every other grounding.
//! Without it (found 2026-10-04 by BigMama while building the Wake consumer) the fold
//! was published on a watch nobody held: a Resume turn could not show her own note.
use std::sync::Arc;

use crate::persona::active_work_source::AircWorkReader;
use crate::persona::rag_budget::{ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference};
use crate::persona::viewstate_rag::estimate_tokens;

pub(crate) const SOURCE_ID: &str = "awareness";

/// The heading the block renders under; fixed, so the prefix bytes are stable.
const HEADING: &str = "Across your activities (loudest first):";

pub struct AwarenessRagSource {
    persona_id: uuid::Uuid,
    /// Her own airc handle, for the one fact the strip needs that her region does not
    /// hold: whether she holds work. `None` in tests of the fit alone.
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

    /// Lines for the turn: the offer first when it applies (her continuation leads the
    /// strip when she has one; the offer stands in its place when she has none).
    async fn lines_for_turn(&self, snapshot: &crate::persona::awareness::AwarenessSnapshot) -> Vec<String> {
        let mut lines = snapshot.render_lines();
        if snapshot.continuation.is_none() {
            if let Some(reader) = &self.claims {
                match reader.active_claims().await {
                    Ok(claims) if !claims.is_empty() => lines.insert(0, offer_line(claims.len())),
                    Ok(_) => {}
                    Err(e) => crate::probe!(
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

    /// The block, fitted to `budget`: the strip is loudest first, so dropping lines
    /// from the END keeps what matters most. `None` = not even the heading and the
    /// loudest line fit.
    fn fit(lines: &[String], budget: u32) -> Option<String> {
        let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
        for line in lines {
            kept.push(line);
            let text = Self::render(&kept);
            if estimate_tokens(&text) > budget {
                kept.pop();
                break;
            }
        }
        if kept.is_empty() {
            return None;
        }
        Some(Self::render(&kept))
    }

    fn render(lines: &[&str]) -> String {
        let mut out = String::from(HEADING);
        for line in lines {
            out.push('\n');
            out.push_str(line);
        }
        out
    }
}

#[async_trait::async_trait]
impl RagSource for AwarenessRagSource {
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }

    fn expand_command(&self) -> Option<&'static str> {
        // The strip is already her whole awareness, reduced; nothing fuller to expand to.
        None
    }

    fn floor_tokens(&self) -> u32 {
        0
    }

    async fn deliver(&self, ctx: &RagContext, budget: u32, resolution: ResolutionPreference) -> RagDelivery {
        let empty = |res| RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items: Vec::new(),
            tokens_used: 0,
            continuation: None,
            resolution_used: res,
        };
        if ctx.persona_id != self.persona_id {
            return empty(ResolutionPreference::Placeholder);
        }
        let Some(snapshot) = crate::persona::perception_feed::awareness_of(self.persona_id, ctx.now_ms) else {
            return empty(resolution);
        };
        let lines = self.lines_for_turn(&snapshot).await;
        if lines.is_empty() {
            return empty(resolution);
        }
        let Some(content) = Self::fit(&lines, budget) else {
            crate::probe!(
                class = "mind.strip.unfit",
                persona = %self.persona_id,
                budget,
                lines = lines.len() as u64,
                "the strip's loudest line did not fit the grounding budget — she turns without her awareness"
            );
            return empty(resolution);
        };
        let tokens = estimate_tokens(&content);
        crate::probe!(
            class = "mind.strip.delivered",
            persona = %self.persona_id,
            lines_total = lines.len() as u64,
            lines_shown = content.lines().count().saturating_sub(1) as u64,
            tokens,
            budget,
            "her awareness strip is in this turn's grounding"
        );
        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items: vec![RagItem {
                content,
                tokens,
                metadata: serde_json::json!({
                    "lines_total": lines.len(),
                    "live_activities": snapshot.load.live_activities,
                    "unread_total": snapshot.load.unread_total,
                }),
            }],
            tokens_used: tokens,
            continuation: None,
            resolution_used: resolution,
        }
    }

    async fn deliver_continuation(&self, _ctx: &RagContext, _cursor: ContinuationCursor, _budget: u32) -> Option<RagDelivery> {
        // One current snapshot; any cursor is stale.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the strip is loudest first, so a budget that holds only part of
    // it keeps the HEAD and drops the tail; a budget that holds not even the loudest line
    // delivers nothing rather than a torn line.
    #[test]
    fn a_tight_budget_keeps_the_loudest_lines_and_never_tears_one() {
        let lines: Vec<String> = (0..6).map(|i| format!("[line {i}] something that happened in activity {i}")).collect();
        let full = AwarenessRagSource::fit(&lines, 10_000).expect("fits");
        assert_eq!(full.lines().count(), 7, "heading + six lines");
        let some = AwarenessRagSource::fit(&lines, estimate_tokens(&full) - 12).expect("fits a prefix");
        assert!(some.lines().count() < 7 && some.lines().nth(1) == Some(&lines[0][..]), "keeps the head: {some}");
        assert!(AwarenessRagSource::fit(&lines, 1).is_none(), "not even the loudest line: nothing, never a torn line");
    }

    // what this catches: the offer names the verb and the count, and is an offer (the
    // word "writes one" — it never claims to have written anything).
    #[test]
    fn the_offer_names_the_verb_and_never_writes() {
        let one = offer_line(1);
        assert!(one.starts_with("[working] you hold 1 card with no note") && one.contains("focus/continue writes one"), "{one}");
        assert!(offer_line(3).contains("3 cards"));
    }
}
