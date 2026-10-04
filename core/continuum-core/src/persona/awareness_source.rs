//! The awareness strip as a grounding source: what she perceives across ALL her
//! activities, loudest first, with her continuation and her dial (EVENT-MIND §6, the
//! strip). One `RagSource` like the roster and the doctrine, so it rides the same
//! budgeter, the same capture and the same prefix order as every other grounding.
//! Without it (found 2026-10-04 by BigMama while building the Wake consumer) the fold
//! was published on a watch nobody held: a Resume turn could not show her own note.
use std::sync::Arc;

use crate::persona::rag_budget::{ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference};
use crate::persona::viewstate_rag::estimate_tokens;

pub(crate) const SOURCE_ID: &str = "awareness";

/// The heading the block renders under; fixed, so the prefix bytes are stable.
const HEADING: &str = "Across your activities (loudest first):";

pub struct AwarenessRagSource {
    persona_id: uuid::Uuid,
}

impl AwarenessRagSource {
    pub fn new(persona_id: uuid::Uuid) -> Self {
        Self { persona_id }
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
        let lines = snapshot.render_lines();
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

/// Boxed for the brain's source list, like the others.
pub fn boxed(persona_id: uuid::Uuid) -> Arc<dyn RagSource> {
    Arc::new(AwarenessRagSource::new(persona_id))
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
}
