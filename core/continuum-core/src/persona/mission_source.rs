//! MissionSource — a measurement drive's task brief as STANDING FRAMING.
//!
//! ### The defect this kills (#390 write-avoidance, glass-boxed 2026-08-12)
//!
//! `agent/solve` delivered the task ONCE, as the drive's opening burst. Every
//! later iteration rebuilt the window from accumulating tool receipts, and the
//! brief aged out: on pytest-5221 (Asha, 24 acts, patch 0), the issue text was
//! present at act 0 and ABSENT from every captured prompt from act ~6 on. Her
//! late-run turn asked, verbatim, "Could you please provide additional details
//! or describe the symptoms of the issue?" — a mind whose exam question was
//! evicted. With no live anchor for WHAT to change, reading is the only
//! rational act, and the run settles patchless. That anchor loss — not tool
//! reluctance — is the dominant capability-zero shape.
//!
//! The fix is the same shape as #347 (the board pinned as standing framing):
//! the mission rides as a `[mission]` grounding block with a
//! `SaliencePolicy::StandingFraming` floor, so attention pressure can never
//! evict the one thing the whole drive exists to do.
//!
//! Fixed text, no I/O, all-or-nothing: a truncated mission is a different
//! (wrong) mission, so `floor_tokens == full size` and an under-budget call
//! delivers nothing — which the allocator surfaces as a dropped source rather
//! than silently grading against half a task.

use async_trait::async_trait;
use uuid::Uuid;

use crate::cognition::token_budget::estimate_prompt_tokens;
use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};

/// Source identifier — renders as the `[mission]` grounding block via the
/// generic `[<source_id>]` projection.
const SOURCE_ID: &str = "mission";

pub struct MissionSource {
    persona_id: Uuid,
    text: String,
    tokens: u32,
}

impl MissionSource {
    pub fn new(persona_id: Uuid, text: impl Into<String>) -> Self {
        let text = text.into();
        let tokens = estimate_prompt_tokens(&text);
        Self {
            persona_id,
            text,
            tokens,
        }
    }
}

#[async_trait]
impl RagSource for MissionSource {
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }

    /// Nothing more to show — the mission IS its full content.
    fn expand_command(&self) -> Option<&'static str> {
        None
    }

    /// All-or-nothing: half a mission is a wrong mission.
    fn floor_tokens(&self) -> u32 {
        self.tokens
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        _resolution: ResolutionPreference,
    ) -> RagDelivery {
        let empty = RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items: Vec::new(),
            tokens_used: 0,
            continuation: None,
            resolution_used: ResolutionPreference::Raw,
        };
        // Persona-scoped like the sibling sources (defense in depth).
        if ctx.persona_id != self.persona_id || budget < self.tokens {
            return empty;
        }
        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items: vec![RagItem {
                content: self.text.clone(),
                tokens: self.tokens,
                metadata: serde_json::json!({}),
            }],
            tokens_used: self.tokens,
            continuation: None,
            resolution_used: ResolutionPreference::Raw,
        }
    }

    async fn deliver_continuation(
        &self,
        _ctx: &RagContext,
        _cursor: ContinuationCursor,
        _budget: u32,
    ) -> Option<RagDelivery> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the #390 anchor-loss regression — the mission must deliver
    // its FULL text when budgeted, refuse a partial render (a truncated mission is a
    // wrong mission), and stay persona-scoped like every sibling source.
    #[tokio::test]
    async fn mission_is_all_or_nothing_and_persona_scoped() {
        let me = Uuid::new_v4();
        let src = MissionSource::new(me, "Fix the bug in swe/x. Deliver an edit.");
        let ctx = RagContext::for_persona(me, 0);

        let full = src.deliver(&ctx, u32::MAX, ResolutionPreference::Raw).await;
        assert_eq!(full.items.len(), 1);
        assert!(full.items[0].content.contains("Deliver an edit"));

        let starved = src
            .deliver(&ctx, src.floor_tokens() - 1, ResolutionPreference::Raw)
            .await;
        assert!(
            starved.items.is_empty(),
            "under-budget must deliver NOTHING, never a truncation"
        );

        let other = RagContext::for_persona(Uuid::new_v4(), 0);
        let cross = src
            .deliver(&other, u32::MAX, ResolutionPreference::Raw)
            .await;
        assert!(
            cross.items.is_empty(),
            "a mission never leaks across personas"
        );
    }
}
