//! `MediaPerceptionSource` — the live-call perception [`RagSource`].
//!
//! Wraps a [`PerceptionBuffer`] and delivers the room AS IT IS NOW — who is visible + a
//! description of what they show — as budgeted [`RagItem`]s. Two disciplines, together:
//! - **NON-BLOCKING**: reads only the cells RESOLVED so far (`current_percepts` → the
//!   `MediaFrame::*_if_ready` twins). A still-warming participant is simply absent this
//!   delivery, present the next — the turn NEVER waits ([[command-async-shape-prefer-stream-never-block]]).
//! - **BUDGETED**: perception must NOT dominate. It competes for context with
//!   engram/airc/roster through the ONE flexbox allocator, stopping when its allotment is
//!   spent ([[perception-feedback-must-not-blow-rag]], [[budget-at-assembly-not-clamp-the-prompt]]).
//!
//! Delivers the TEXT percept (description grounding, available to ANY model). The
//! thumbnail image for a native-vision persona rides the separate render seam (#190) off
//! the SAME cached cells — one describe/scale per frame, shared. Room-as-now, not a
//! backlog ([[perceive-the-room-as-it-is-now]]); never paginated.

use std::sync::Arc;

use async_trait::async_trait;

use super::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};
use crate::cognition::token_budget::estimate_prompt_tokens as estimate_tokens;
use crate::media::{Percept, PerceptionBuffer};
use crate::runtime::SharedCompute;

const SOURCE_ID: &str = "media-perception";

/// A persona's live-call visual perception, delivered as budgeted RAG grounding.
pub struct MediaPerceptionSource {
    persona_id: uuid::Uuid,
    /// The room-as-now frame hold (one latest frame per airc participant).
    buffer: Arc<PerceptionBuffer>,
    /// The ONE runtime shared cache the cells resolve on (read-only here).
    compute: Arc<SharedCompute>,
}

impl MediaPerceptionSource {
    pub fn new(
        persona_id: uuid::Uuid,
        buffer: Arc<PerceptionBuffer>,
        compute: Arc<SharedCompute>,
    ) -> Self {
        Self {
            persona_id,
            buffer,
            compute,
        }
    }

    /// One RESOLVED percept → a grounding [`RagItem`]. `None` when nothing has resolved for
    /// this participant yet — non-blocking: absent this delivery, present the next as its
    /// cells land. A failed cell (`Err`) is treated as not-yet (never a fabricated line).
    fn make_item(p: &Percept) -> Option<RagItem> {
        let has_ok_thumbnail = matches!(p.thumbnail.as_deref(), Some(Ok(_)));
        let content = match p.description.as_deref() {
            Some(Ok(desc)) if !desc.trim().is_empty() => {
                format!("[seeing {}] {desc}", short(&p.participant))
            }
            _ if has_ok_thumbnail => format!(
                "[seeing {}] visible on the call — description still resolving",
                short(&p.participant)
            ),
            _ => return None,
        };
        let tokens = estimate_tokens(&content);
        Some(RagItem {
            content,
            tokens,
            metadata: serde_json::json!({
                "participant": p.participant,
                "content_hash": p.content_hash,
            }),
        })
    }
}

/// First 8 chars of an id for the grounding line (airc peer ids are long; the description
/// carries the "who/what"). The full id rides in item metadata for provenance.
fn short(id: &str) -> &str {
    &id[..8.min(id.len())]
}

fn empty_delivery() -> RagDelivery {
    RagDelivery {
        source_id: SOURCE_ID.to_string(),
        items: Vec::new(),
        tokens_used: 0,
        continuation: None,
        resolution_used: ResolutionPreference::Placeholder,
    }
}

#[async_trait]
impl RagSource for MediaPerceptionSource {
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }

    fn expand_command(&self) -> Option<&'static str> {
        Some("perception/observe")
    }

    /// One resolved perception cell — who is visible / what is shown. A single
    /// cell is a complete visual fact.
    fn floor_tokens(&self) -> u32 {
        48
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        // Persona-scoped: a cross-persona ctx gets nothing (defense in depth, same shape as
        // the roster/airc sources).
        if ctx.persona_id != self.persona_id {
            return empty_delivery();
        }

        // NON-BLOCKING read of the room as it is NOW — only cells resolved this tick.
        let percepts = self.buffer.current_percepts(&self.compute);
        let mut items: Vec<RagItem> = Vec::new();
        let mut tokens_used: u32 = 0;
        for p in &percepts {
            let Some(item) = Self::make_item(p) else {
                continue;
            };
            // BUDGET: perception must NOT dominate — stop when the allotment is spent.
            // A truncated view is still truthful for the participants it names.
            if tokens_used.saturating_add(item.tokens) > budget {
                break;
            }
            tokens_used += item.tokens;
            items.push(item);
        }

        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items,
            tokens_used,
            // Room-as-now, not a paginated history.
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
        // Live perception is the CURRENT tail, not a backlog — no pagination.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::frame::{FrameDescriber, MediaFrame};
    use crate::media::image_ops::DestSize;
    use async_trait::async_trait;
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;
    use uuid::Uuid;

    const AMBIENT: DestSize = DestSize { width: 32, height: 24 };

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, _| {
            if x < w / 2 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    struct StubDescriber;
    #[async_trait]
    impl FrameDescriber for StubDescriber {
        async fn describe(&self, source: &[u8], mime: &str) -> Result<String, String> {
            Ok(format!("a {mime} image of {} bytes", source.len()))
        }
    }

    /// Store `alice` with her cells RESOLVED on `compute` (deterministic; in prod the
    /// observe spawn warms them async). Returns (source, ctx, compute).
    async fn source_with_resolved_alice(
        pid: Uuid,
    ) -> (MediaPerceptionSource, Arc<SharedCompute>) {
        let compute = Arc::new(SharedCompute::new());
        let buffer = Arc::new(PerceptionBuffer::new(AMBIENT));
        let describer: Arc<dyn FrameDescriber> = Arc::new(StubDescriber);
        let frame = MediaFrame::from_bytes(png(60, 40));
        frame.scaled(&compute, None, AMBIENT).await;
        frame.description(&compute, &StubDescriber, "image/png").await;
        buffer.observe("alice-peer".into(), frame, compute.clone(), describer, "image/png", 0);
        (
            MediaPerceptionSource::new(pid, buffer, compute.clone()),
            compute,
        )
    }

    // what this catches: a resolved percept becomes a budgeted "[seeing …] <desc>" grounding
    // item; wrong persona gets nothing; a zero budget yields nothing (perception must not
    // dominate). The core deliver contract.
    #[tokio::test]
    async fn delivers_resolved_percepts_as_budgeted_grounding() {
        let pid = Uuid::new_v4();
        let (src, _compute) = source_with_resolved_alice(pid).await;
        let ctx = RagContext::for_persona(pid, 0);

        let d = src.deliver(&ctx, 10_000, ResolutionPreference::Raw).await;
        assert_eq!(d.items.len(), 1, "one resolved participant → one item");
        assert!(d.items[0].content.contains("seeing"), "grounds who is seen: {}", d.items[0].content);
        assert!(d.tokens_used > 0);
        assert!(d.continuation.is_none(), "room-as-now, never paginated");

        // Cross-persona → empty (defense in depth).
        let other = RagContext::for_persona(Uuid::new_v4(), 0);
        assert!(src.deliver(&other, 10_000, ResolutionPreference::Raw).await.items.is_empty());

        // Zero budget → nothing (must NOT dominate context).
        assert!(src.deliver(&ctx, 0, ResolutionPreference::Raw).await.items.is_empty());
    }

    // what this catches: NON-BLOCKING — a participant whose cells haven't resolved yet is
    // simply ABSENT from the delivery (the turn never waits on them).
    #[tokio::test]
    async fn unresolved_participants_are_absent_not_awaited() {
        let compute = Arc::new(SharedCompute::new());
        let buffer = Arc::new(PerceptionBuffer::new(AMBIENT));
        let pid = Uuid::new_v4();
        // Store a frame WITHOUT warming its cells → nothing resolved.
        let frame = MediaFrame::from_bytes(png(20, 20));
        buffer.observe(
            "bob-peer".into(),
            frame,
            compute.clone(),
            Arc::new(StubDescriber),
            "image/png",
            0,
        );
        // (observe spawns a warm task, but we don't await it — read immediately.)
        let src = MediaPerceptionSource::new(pid, buffer, compute);
        let ctx = RagContext::for_persona(pid, 0);
        let d = src.deliver(&ctx, 10_000, ResolutionPreference::Raw).await;
        assert!(d.items.is_empty(), "unresolved participant is absent this tick, not awaited");
    }
}
