//! Dream consolidation — distilling episodic memories into durable facts.
//!
//! This is the **consolidation/dream region** — "outlier B" of the
//! RAG-as-persistent-cache architecture (`docs/cognition/RAG-AS-PERSISTENT-CACHE.md`
//! + `docs/cognition/DREAM-CONSOLIDATION.md`). Where the live `ChannelDigestRegion`
//! (outlier A) is an *event-driven, no-LLM* servicer, this one is the most
//! different servicer: *intermittent, LLM-driven*. If both fit the same
//! `BrainRegion`/`ReadyBuffer` interface without forcing, the remaining slices
//! slot in (the methodical-process outlier-validation strategy).
//!
//! ## What this file is (slice 1: the distiller)
//!
//! The smallest, self-contained, deterministically-testable unit: given N
//! related episodic [`Engram`]s and an inference adapter, ask the model to
//! consolidate them into ONE durable semantic fact ([`DistilledFact`]).
//!
//! It deliberately does NOT:
//! - decide WHICH engrams to consolidate (the region's clustering job — a later
//!   slice),
//! - admit the result into the engram store (the self-admission path — a later
//!   slice; the engram store IS the `facts` persistence, not a bespoke buffer).
//!
//! It is pure: engrams in, one distilled fact out, source provenance preserved.
//! Refinement is LEARNED cognition (the model distills), never a hand-written
//! filter that reads the persona's output and puppets it — that would be the
//! exact anti-pattern this codebase forbids
//! (`[[no-hardcoded-heuristics-to-steer-cognition]]`).

use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{ChatMessage, TextGenerationRequest};
use crate::persona::engram::Engram;

/// One durable fact distilled from a cluster of episodic engrams.
///
/// Carries full provenance: `source_ids` is every episodic engram that fed the
/// distillation, in input order. The region that turns this into a `Semantic`
/// `Engram` decides how to record that provenance against the engram model
/// (`EngramOrigin::SelfReflection` carries a single `parent_engram_id` today;
/// multi-source provenance on the engram is a follow-up slice). `tags` is the
/// union of the sources' `recall_keys`, so the distilled fact is retrievable by
/// the same keys its sources were.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistilledFact {
    /// The consolidated fact, as the model wrote it (trimmed).
    pub content: String,
    /// Every source engram id that fed this distillation, in input order.
    pub source_ids: Vec<Uuid>,
    /// Union of the sources' recall keys (first-seen order), so the fact is
    /// retrievable by the same keys its sources were.
    pub tags: Vec<String>,
}

/// Why a distillation could not be produced. Typed + loud — there is no silent
/// "return nothing" path (`[[fallbacks-are-illegal-fail-loud]]`): a caller that
/// gets `Ok` gets a real fact, and any failure names its cause.
#[derive(Debug, Error)]
pub enum DistillError {
    /// The caller passed no source engrams. A distillation of nothing is a
    /// clustering bug in the region, not a runtime condition to paper over.
    #[error("cannot distill: no source engrams provided")]
    NoSources,
    /// The inference adapter returned an error.
    #[error("distillation inference failed: {0}")]
    Inference(String),
    /// The model returned empty text. We do NOT fabricate a fact from the raw
    /// transcript — an empty distillation is surfaced, not hidden.
    #[error("distillation produced empty output")]
    EmptyDistillation,
}

/// Distills clusters of episodic engrams into durable semantic facts via the
/// LLM. Holds an inference adapter; the region (a later slice) owns one of these
/// and feeds it clusters on its idle-tick cadence.
pub struct SemanticDistiller {
    adapter: Arc<dyn AIProviderAdapter>,
}

impl SemanticDistiller {
    /// The consolidation instruction. Frames the persona distilling its OWN
    /// episodic memories into long-term knowledge — faithful, reusable, stated
    /// independently of when/how it was learned.
    const SYSTEM_PROMPT: &'static str = "\
You are consolidating your own episodic memories into long-term knowledge. \
Below are several things you observed or experienced, in order. Distill them \
into a SINGLE durable fact: the general, reusable knowledge they share, stated \
independently of when or how you learned it. Output ONLY the consolidated fact \
as one or two plain sentences — no numbering, no preamble, no commentary, no \
quotes. If the observations share no single consolidatable fact, state the one \
most important durable takeaway.";

    pub fn new(adapter: Arc<dyn AIProviderAdapter>) -> Self {
        Self { adapter }
    }

    /// Consolidate a cluster of related episodic engrams into one durable fact.
    ///
    /// `persona_id` attributes the inference to its owning persona for
    /// per-persona resource accounting (the dream IS attributable work, not an
    /// ad-hoc probe). The distiller stays persona-agnostic otherwise — it does
    /// not read the persona's store, only the engrams handed to it. Source order
    /// is preserved in [`DistilledFact::source_ids`].
    pub async fn distill(
        &self,
        persona_id: Option<Uuid>,
        sources: &[Engram],
    ) -> Result<DistilledFact, DistillError> {
        if sources.is_empty() {
            return Err(DistillError::NoSources);
        }

        // max_tokens stays None — the adapter owns generation length (#45/#46);
        // no per-call clamp. The distillation's faithfulness is gated by VDD
        // with a real model, not by hand-tuned sampling knobs here.
        let request = TextGenerationRequest {
            messages: vec![ChatMessage::text("user", Self::observations_block(sources))],
            system_prompt: Some(Self::SYSTEM_PROMPT.to_string()),
            model: None,
            provider: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: Some("dream-consolidation".to_string()),
            persona_id: persona_id.map(|id| id.to_string()),
        };

        let response = self
            .adapter
            .generate_text(request)
            .await
            .map_err(DistillError::Inference)?;

        let content = response.text.trim().to_string();
        if content.is_empty() {
            return Err(DistillError::EmptyDistillation);
        }

        Ok(DistilledFact {
            content,
            source_ids: sources.iter().map(|e| e.id).collect(),
            tags: Self::union_recall_keys(sources),
        })
    }

    /// Render the cluster as a numbered observation list for the prompt.
    fn observations_block(sources: &[Engram]) -> String {
        let mut block = String::new();
        for (i, e) in sources.iter().enumerate() {
            block.push_str(&format!("{}. {}\n", i + 1, e.content.trim()));
        }
        block
    }

    /// Union of every source's recall keys, first-seen order preserved.
    fn union_recall_keys(sources: &[Engram]) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for e in sources {
            for k in &e.recall_keys {
                if seen.insert(k.clone()) {
                    out.push(k.clone());
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::persona::engram::{Engram, EngramKind, EngramOrigin, TrustState};

    /// Build an episodic engram with a given id, content, and recall keys.
    fn episodic(id: Uuid, content: &str, recall_keys: &[&str]) -> Engram {
        Engram {
            id,
            context_id: None,
            kind: EngramKind::Episodic,
            content: content.to_string(),
            origin: EngramOrigin::SelfReflection {
                parent_engram_id: Uuid::nil(),
            },
            recall_keys: recall_keys.iter().map(|k| k.to_string()).collect(),
            admitted_at_ms: 1_000,
            trust_state_at_admission: TrustState::SelfTrust,
            admission_trace_id: None,
        }
    }

    // what this catches: the distiller actually invokes the inference adapter,
    // captures its output as the fact content, and preserves full source
    // provenance (ids in input order + the union of recall keys as tags).
    // Regression guard for the dream silently dropping provenance or never
    // calling the model.
    #[tokio::test]
    async fn distill_invokes_adapter_and_preserves_provenance() {
        let id1 = Uuid::from_u128(1);
        let id2 = Uuid::from_u128(2);
        let id3 = Uuid::from_u128(3);
        let sources = vec![
            episodic(id1, "Joel prefers Rust for the core", &["rust", "core"]),
            episodic(id2, "Node is only the shell", &["core", "node"]),
            episodic(id3, "Headless core, many clients", &["node", "clients"]),
        ];

        let distiller = SemanticDistiller::new(Arc::new(HeuristicInferenceAdapter::new()));
        let fact = distiller
            .distill(Some(Uuid::from_u128(99)), &sources)
            .await
            .expect("distillation succeeds against the heuristic adapter");

        // The heuristic adapter echoes the prompt deterministically — its
        // signature in the content proves the model was really called and its
        // output captured (not fabricated locally).
        assert!(
            fact.content.contains("[heuristic:"),
            "fact content should be the adapter's output, got: {}",
            fact.content
        );
        // Source ids preserved, in input order.
        assert_eq!(fact.source_ids, vec![id1, id2, id3]);
        // tags = union of recall keys, first-seen order, deduped.
        assert_eq!(
            fact.tags,
            vec![
                "rust".to_string(),
                "core".to_string(),
                "node".to_string(),
                "clients".to_string(),
            ]
        );
    }

    // what this catches: distilling an empty cluster fails LOUD with NoSources
    // rather than silently returning an empty/fabricated fact
    // ([[fallbacks-are-illegal-fail-loud]]).
    #[tokio::test]
    async fn distill_empty_cluster_fails_loud() {
        let distiller = SemanticDistiller::new(Arc::new(HeuristicInferenceAdapter::new()));
        let err = distiller
            .distill(None, &[])
            .await
            .expect_err("empty cluster must error, not return a fact");
        assert!(matches!(err, DistillError::NoSources));
    }
}
