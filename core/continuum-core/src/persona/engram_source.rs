//! EngramSource — the first concrete `RagSource` implementation.
//!
//! Reads from a per-persona `AdmissionState`'s engram store + the
//! shared `RecallMetadataRegistry`, ranks engrams by salience ×
//! recency, packs top-K into `RagItem`s within the budget. Persona-
//! scoped at construction.
//!
//! ### Doctrine alignment
//!
//! Per [[RTOS-brain-no-region-on-hot-path]]: the source's `deliver`
//! does its scoring + selection synchronously inside the call. No
//! I/O, no async wait. The expensive work (admission, decay,
//! consolidation) lives in the hippocampus's own tick — this source
//! just reads pre-staged state.
//!
//! Per the no-clipping doctrine
//! ([[docs/architecture/EVERY-MODEL-INCLUDED-VIA-L1-BUDGET.md]]):
//! atomic unit = one engram. Engrams that don't fit are returned
//! via the continuation cursor for a later turn or operator-driven
//! resume. Mid-engram truncation is structurally impossible.
//!
//! Per [[substrate-is-a-good-citizen-on-the-host]]: the metadata
//! field on every emitted `RagItem` carries provenance — engram_id,
//! kind, admitted_at_ms, score — so prompt assembly + sentinel
//! verifiers + future telemetry can trace what made it in.
//!
//! ### Scoring (slice 10 — simplified Algorithm 1+2)
//!
//! score = 0.6 × salience + 0.4 × recency_normalized
//!
//! - **salience** comes from `RecallMetadata.salience` (admission-
//!   time default 0.5; decays per Algorithm 4; uplifts on recall
//!   hits per slice 5's `record_recall_hit`). Floored at
//!   `SALIENCE_FLOOR` from the anti-amnesia work, so engrams never
//!   drop to invisible.
//! - **recency_normalized** is linear over 24h: engrams admitted
//!   right now score ~1.0, engrams ≥ 24h old score 0.0.
//!
//! Future slices add:
//! - Algorithm 2 channel-bias (`ctx.airc_room` → boost when engram
//!   origin matches the current room)
//! - Algorithm 2 structural relevance (engram graph activation
//!   spreading from query embedding)
//! - Algorithm 2 topic similarity (vector cosine vs query
//!   embedding once embeddings are wired through `RagContext`)
//! - Compressed resolution (engram summary instead of full content)

use std::sync::Arc;

use async_trait::async_trait;

use crate::persona::admission_state::AdmissionState;
use crate::persona::engram::Engram;
use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};

/// 24 hours in ms — the normalization window for the recency
/// score. Engrams older than this contribute 0 to the recency
/// component. Tunable via future `MemoryParameterAdapter`.
const RECENCY_WINDOW_MS: u64 = 24 * 60 * 60 * 1000;

/// Source identifier — referenced by budget presets, telemetry,
/// continuation cursor scope check.
const SOURCE_ID: &str = "engrams";

/// Token estimate — the ONE canonical chars/4 estimator (`cognition::token_budget`),
/// shared by every RAG source so the replay ledger's numbers match. (Was a private
/// copy — converged.)
use crate::cognition::token_budget::estimate_prompt_tokens as estimate_tokens;

/// Linear recency over `RECENCY_WINDOW_MS`. Returns 1.0 for
/// engrams admitted right at `now_ms`, 0.0 for engrams admitted
/// ≥ `RECENCY_WINDOW_MS` ago, linearly interpolated between.
fn recency_score(admitted_at_ms: u64, now_ms: u64) -> f32 {
    if now_ms <= admitted_at_ms {
        return 1.0;
    }
    let age_ms = now_ms - admitted_at_ms;
    if age_ms >= RECENCY_WINDOW_MS {
        return 0.0;
    }
    1.0 - (age_ms as f32 / RECENCY_WINDOW_MS as f32)
}

/// The composite score for ranking. 0.6 × salience + 0.4 × recency.
/// Slice 11+ will add channel-bias, structural relevance, topic
/// similarity.
fn composite_score(salience: f32, admitted_at_ms: u64, now_ms: u64) -> f32 {
    0.6 * salience + 0.4 * recency_score(admitted_at_ms, now_ms)
}

/// Format an engram's content for inclusion in the prompt. Slice 10
/// uses raw `engram.content`; slice 11+ may prefix with provenance
/// markers depending on the prompt-assembly contract.
fn format_engram_content(engram: &Engram, _resolution: ResolutionPreference) -> String {
    engram.content.clone()
}

/// EngramSource — persona-bound, reads from a shared AdmissionState.
///
/// Holds an `Arc<AdmissionState>` so the same admission state is
/// shared with the admission pipeline + future cognition subsystems.
/// The recall metadata comes from `admission_state.recall_metadata()`
/// (a clone of the inner `Arc<RecallMetadataRegistry>`).
pub struct EngramSource {
    persona_id: uuid::Uuid,
    admission_state: Arc<AdmissionState>,
}

impl EngramSource {
    pub fn new(persona_id: uuid::Uuid, admission_state: Arc<AdmissionState>) -> Self {
        Self {
            persona_id,
            admission_state,
        }
    }

    /// Score + sort every engram in the store. Returns
    /// `Vec<(score, engram)>` sorted by score descending. Pure
    /// function over the admission state at a moment in time.
    fn rank_engrams(&self, now_ms: u64) -> Vec<(f32, Engram)> {
        let recall_meta = self.admission_state.recall_metadata().clone();
        let count = self.admission_state.engram_count();
        let mut scored: Vec<(f32, Engram)> = Vec::with_capacity(count);
        for i in 0..count {
            let Some(engram) = self.admission_state.engram_at(i) else {
                continue;
            };
            let salience = recall_meta
                .get(engram.id)
                .map(|m| m.salience)
                .unwrap_or(0.5);
            let score = composite_score(salience, engram.admitted_at_ms, now_ms);
            scored.push((score, engram));
        }
        // Sort by score descending; stable enough — same-score engrams
        // tiebreak on admitted_at_ms descending to favor newer.
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.1.admitted_at_ms.cmp(&a.1.admitted_at_ms))
        });
        scored
    }

    /// Pack ranked engrams into RagItems within budget starting from
    /// the given rank offset. Returns (items, tokens_used,
    /// next_rank_or_done). next_rank is `scored.len()` if the
    /// source delivered everything; otherwise it's the index of the
    /// first engram that didn't fit (cursor for resume).
    fn pack_from_rank(
        &self,
        scored: &[(f32, Engram)],
        start_rank: usize,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> (Vec<RagItem>, u32, usize) {
        let mut items = Vec::new();
        let mut tokens_used: u32 = 0;
        let mut next_rank = start_rank;
        for (idx, (score, engram)) in scored.iter().enumerate().skip(start_rank) {
            let content = format_engram_content(engram, resolution);
            let tokens = estimate_tokens(&content);
            if tokens_used.saturating_add(tokens) > budget {
                next_rank = idx;
                break;
            }
            tokens_used += tokens;
            items.push(RagItem {
                content,
                tokens,
                metadata: serde_json::json!({
                    "engram_id": engram.id.to_string(),
                    "kind": format!("{:?}", engram.kind),
                    "admitted_at_ms": engram.admitted_at_ms,
                    "score": score,
                }),
            });
            next_rank = idx + 1;
        }
        (items, tokens_used, next_rank)
    }

    fn build_delivery(
        &self,
        items: Vec<RagItem>,
        tokens_used: u32,
        next_rank: usize,
        scored_len: usize,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        let continuation = if next_rank < scored_len {
            Some(ContinuationCursor {
                persona_id: self.persona_id,
                source_id: SOURCE_ID.to_string(),
                opaque: serde_json::json!({ "next_rank": next_rank }),
            })
        } else {
            None
        };
        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items,
            tokens_used,
            continuation,
            resolution_used: resolution,
        }
    }
}

#[async_trait]
impl RagSource for EngramSource {
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }

    fn expand_command(&self) -> Option<&'static str> {
        Some("cognition/recall")
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        // Defense-in-depth: refuse calls with the wrong persona ctx.
        if ctx.persona_id != self.persona_id {
            return RagDelivery {
                source_id: SOURCE_ID.to_string(),
                items: Vec::new(),
                tokens_used: 0,
                continuation: None,
                resolution_used: ResolutionPreference::Placeholder,
            };
        }
        let scored = self.rank_engrams(ctx.now_ms);
        let scored_len = scored.len();
        let (items, tokens_used, next_rank) =
            self.pack_from_rank(&scored, 0, budget, resolution);
        self.build_delivery(items, tokens_used, next_rank, scored_len, resolution)
    }

    async fn deliver_continuation(
        &self,
        ctx: &RagContext,
        cursor: ContinuationCursor,
        budget: u32,
    ) -> Option<RagDelivery> {
        if ctx.persona_id != self.persona_id {
            return None;
        }
        if cursor.persona_id != self.persona_id {
            return None;
        }
        if cursor.source_id != SOURCE_ID {
            return None;
        }
        let next_rank: usize = cursor.opaque.get("next_rank")?.as_u64()? as usize;
        let scored = self.rank_engrams(ctx.now_ms);
        if next_rank >= scored.len() {
            return None;
        }
        let scored_len = scored.len();
        let (items, tokens_used, new_next_rank) =
            self.pack_from_rank(&scored, next_rank, budget, ResolutionPreference::Raw);
        Some(self.build_delivery(
            items,
            tokens_used,
            new_next_rank,
            scored_len,
            ResolutionPreference::Raw,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::admission_state::EngramOriginKind;
    use crate::persona::engram::{ChatMessageRef, Engram, EngramKind, EngramOrigin, TrustState};
    use crate::persona::recall_metadata::{RecallMetadata, RecallMetadataRegistry};
    use uuid::Uuid;

    /// Build an AdmissionState wrapped in Arc, with `count` engrams
    /// admitted via the raw store accessor + each tracked in the
    /// recall metadata registry with a chosen salience.
    fn fixture(count: usize, base_now_ms: u64) -> (uuid::Uuid, Arc<AdmissionState>) {
        let persona = Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap();
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));

        // Push N engrams directly. We bypass `admit` (which runs the
        // full admission pipeline) to keep the test isolated to the
        // source's scoring + packing behavior.
        for i in 0..count {
            let engram = Engram {
                context_id: None,
                id: Uuid::new_v4(),
                kind: EngramKind::Episodic,
                content: format!("engram body number {i}"),
                origin: EngramOrigin::Chat(ChatMessageRef {
                    message_id: Uuid::new_v4(),
                    room_id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    posted_at_ms: base_now_ms.saturating_sub((i as u64) * 60_000),
                    content_hash: format!("hash-{i}"),
                }),
                recall_keys: Vec::new(),
                admitted_at_ms: base_now_ms.saturating_sub((i as u64) * 60_000),
                trust_state_at_admission: TrustState::ApprovedPeer,
                admission_trace_id: None,
            };
            // Test-only access: push through the engram_count-incrementing
            // path. We can't easily push directly into the private store,
            // so use admit_via_test_pushback (a test-only API) — except
            // that doesn't exist. We'll use the admit() pipeline by
            // constructing inbox messages... that's too complex for slice
            // 10's purposes.
            //
            // Pragmatic alternative: add a test-only accessor on
            // AdmissionState that lets tests push engrams directly. Done
            // below — see admission_state.rs:`pub fn _push_for_test_only`.
            state.push_for_test(engram.clone());
            recall_meta.admit(
                engram.id,
                RecallMetadata {
                    salience: 0.5 + (i as f32 * 0.05).min(0.5),
                    access_count: 0,
                    last_accessed_ms: 0,
                    protected_until_ms: 0,
                    last_decayed_ms: base_now_ms,
                },
            );
        }
        // Suppress unused warning — fixture pattern uses kind for future tests.
        let _ = EngramOriginKind::Chat;
        (persona, state)
    }

    fn ctx_for(persona_id: uuid::Uuid, now_ms: u64) -> RagContext {
        RagContext::for_persona(persona_id, now_ms)
    }

    #[tokio::test]
    async fn empty_store_delivers_nothing() {
        let (persona, state) = fixture(0, 1_000_000_000);
        let source = EngramSource::new(persona, state);
        let delivery = source
            .deliver(&ctx_for(persona, 1_000_000_000), 1000, ResolutionPreference::Raw)
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
        assert!(delivery.continuation.is_none());
    }

    #[tokio::test]
    async fn single_engram_delivered_when_fits() {
        let (persona, state) = fixture(1, 1_000_000_000);
        let source = EngramSource::new(persona, state);
        let delivery = source
            .deliver(&ctx_for(persona, 1_000_000_000), 1000, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 1);
        assert!(delivery.tokens_used > 0);
        assert!(delivery.continuation.is_none());
        // Metadata carries the engram id.
        assert!(delivery.items[0]
            .metadata
            .get("engram_id")
            .is_some());
    }

    #[tokio::test]
    async fn oversized_engram_returns_continuation_with_zero_items() {
        let (persona, state) = fixture(1, 1_000_000_000);
        let source = EngramSource::new(persona, state);
        // Budget of 0 tokens — the (small but nonzero) engram can't
        // fit. Source returns 0 items + continuation so the caller
        // can retry with more budget OR drop the source.
        let delivery = source
            .deliver(&ctx_for(persona, 1_000_000_000), 0, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 0);
        assert_eq!(delivery.tokens_used, 0);
        assert!(delivery.continuation.is_some());
    }

    #[tokio::test]
    async fn multi_engram_ranked_by_salience_descending() {
        // 5 engrams with increasing salience (per fixture builder).
        // Smallest budget that fits 2 engrams → top 2 by score should
        // come out, in descending order.
        let (persona, state) = fixture(5, 1_000_000_000);
        let source = EngramSource::new(persona, state.clone());
        let delivery = source
            .deliver(
                &ctx_for(persona, 1_000_000_000),
                100, // enough for a couple
                ResolutionPreference::Raw,
            )
            .await;
        // Score is descending across items.
        let scores: Vec<f64> = delivery
            .items
            .iter()
            .map(|i| i.metadata.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0))
            .collect();
        for w in scores.windows(2) {
            assert!(w[0] >= w[1], "scores not descending: {scores:?}");
        }
        assert!(!delivery.items.is_empty());
    }

    #[tokio::test]
    async fn continuation_resumes_from_next_rank() {
        let (persona, state) = fixture(4, 1_000_000_000);
        let source = EngramSource::new(persona, state);
        // Budget tight enough to force continuation — each engram body
        // is ~6 tokens, so budget 12 fits 2 of 4 and forces a cursor.
        let first = source
            .deliver(&ctx_for(persona, 1_000_000_000), 12, ResolutionPreference::Raw)
            .await;
        assert!(!first.items.is_empty());
        let cursor = first.continuation.expect("expected continuation");
        // Resume with large budget — should get the rest.
        let second = source
            .deliver_continuation(&ctx_for(persona, 1_000_000_000), cursor, 10_000)
            .await
            .expect("continuation should yield");
        // Total items across both calls = all 4 engrams.
        assert_eq!(first.items.len() + second.items.len(), 4);
        // No duplicate engram ids across the two calls.
        let mut seen_ids = std::collections::HashSet::new();
        for item in first.items.iter().chain(second.items.iter()) {
            let id = item
                .metadata
                .get("engram_id")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_string();
            assert!(seen_ids.insert(id), "duplicate engram across calls");
        }
    }

    #[tokio::test]
    async fn cross_persona_ctx_returns_empty() {
        let (persona, state) = fixture(3, 1_000_000_000);
        let source = EngramSource::new(persona, state);
        let other = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let delivery = source
            .deliver(&ctx_for(other, 1_000_000_000), 1_000, ResolutionPreference::Raw)
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.resolution_used, ResolutionPreference::Placeholder);
    }

    #[tokio::test]
    async fn cross_persona_cursor_refused() {
        let (persona, state) = fixture(3, 1_000_000_000);
        let source = EngramSource::new(persona, state);
        let other = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let alien_cursor = ContinuationCursor {
            persona_id: other,
            source_id: SOURCE_ID.to_string(),
            opaque: serde_json::json!({ "next_rank": 0 }),
        };
        let result = source
            .deliver_continuation(&ctx_for(persona, 1_000_000_000), alien_cursor, 1_000)
            .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn wrong_source_id_cursor_refused() {
        let (persona, state) = fixture(3, 1_000_000_000);
        let source = EngramSource::new(persona, state);
        let alien = ContinuationCursor {
            persona_id: persona,
            source_id: "memories".to_string(),
            opaque: serde_json::json!({ "next_rank": 0 }),
        };
        let result = source
            .deliver_continuation(&ctx_for(persona, 1_000_000_000), alien, 1_000)
            .await;
        assert!(result.is_none());
    }

    #[test]
    fn recency_score_at_now_is_one() {
        assert_eq!(recency_score(1_000_000_000, 1_000_000_000), 1.0);
    }

    #[test]
    fn recency_score_at_window_or_older_is_zero() {
        let now = 24 * 60 * 60 * 1000_u64;
        assert_eq!(recency_score(0, now), 0.0);
        // older than the window — also 0.
        assert_eq!(recency_score(0, now * 2), 0.0);
    }

    #[test]
    fn recency_score_halfway_is_half() {
        let now = 24 * 60 * 60 * 1000_u64;
        let half_window_ago = now / 2;
        let score = recency_score(half_window_ago, now);
        assert!((score - 0.5).abs() < 0.001, "got {score}");
    }

    #[test]
    fn composite_score_weights_salience_more() {
        // Same recency, higher salience → higher score.
        let high = composite_score(1.0, 1_000_000_000, 1_000_000_000);
        let low = composite_score(0.0, 1_000_000_000, 1_000_000_000);
        assert!(high > low);
        // Specifically, weight ratio should be 0.6 : 0.4.
        // pure salience 1.0 at recency 1.0 = 0.6 * 1.0 + 0.4 * 1.0 = 1.0
        assert!((high - 1.0).abs() < 0.001);
        // pure salience 0.0 at recency 1.0 = 0.0 + 0.4 = 0.4
        assert!((low - 0.4).abs() < 0.001);
    }
}
