//! Built-in `IsMemorable` recipes.
//!
//! Extracted from `admission.rs` (continuum#1208) so the recipe
//! implementations live next to each other and the structural-gate
//! file (`mod.rs`) doesn't carry policy details. The trait itself
//! stays in `mod.rs` since it's the seam every recipe implements;
//! this file is the registry of concrete recipes Continuum ships.
//!
//! Recipe contract (re-stated for skim-readers): each recipe is a
//! pure decision function over a `(candidate, AdmissionContext)`
//! pair returning `Result<AdmissionDecision, AdmissionError>`. The
//! gate runs prereqs (envelope, trust, replay) BEFORE invoking the
//! recipe, so recipes can assume those passed.

use super::{
    build_engram_from_candidate, AdmissionCandidate, AdmissionContext, AdmissionDecision,
    AdmissionDropReason, AdmissionError, IsMemorable,
};

/// Cheap heuristic recipe — the v1 default. Suitable as a starting point
/// for any persona; richer recipes can compose on top.
///
/// Decision logic:
/// 1. **Dedup** — content_hash hit in `seen_content` → `Drop::Duplicate`.
/// 2. **Length** — content shorter than `min_content_length` chars →
///    `Drop::NotMemorable("content too short")`.
/// 3. **Noise phrases** — content (case-insensitive, trimmed) matches a
///    phrase in `noise_phrases` → `Drop::NotMemorable("noise phrase")`.
/// 4. Otherwise → `Admit` with a synthesized `Engram`.
///
/// No `Quarantine` outcome from this recipe — quarantine is for uncertain
/// cases, and this recipe is binary on its inputs. A future
/// `SimilarityIsMemorable` recipe will be the first to use quarantine
/// (for content that's borderline-similar to existing engrams).
pub struct HeuristicIsMemorable {
    /// Minimum content length to consider memorable. Chars, not bytes.
    pub min_content_length: usize,
    /// Phrases that, alone, are noise (e.g., "ack", "ok", "👍"). Stored
    /// pre-normalized (lowercased, trimmed) so the per-call hot path
    /// doesn't repeat the normalization for every candidate. Use
    /// [`HeuristicIsMemorable::with_noise_phrases`] to construct with a
    /// custom set rather than mutating directly.
    pub noise_phrases: Vec<String>,
}

impl HeuristicIsMemorable {
    /// v1 defaults — minimal length 16 chars, common ack phrases as noise.
    /// Tuned for AIRC-style chatter where one-word acks dominate volume.
    pub fn default_v1() -> Self {
        Self::with_noise_phrases(
            16,
            ["ack", "ok", "okay", "thanks", "thx", "got it", "+1", "👍"],
        )
    }

    /// Construct with a custom minimum length + noise-phrase set. Phrases
    /// are normalized once here (lowercased, trimmed) so the per-call
    /// noise check is a plain string comparison — heuristic recipes are
    /// the per-message hot path and re-lowercasing on every candidate
    /// would be wasted work.
    pub fn with_noise_phrases<I, S>(min_content_length: usize, phrases: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let noise_phrases = phrases
            .into_iter()
            .map(|p| p.as_ref().trim().to_lowercase())
            .collect();
        Self {
            min_content_length,
            noise_phrases,
        }
    }
}

impl IsMemorable for HeuristicIsMemorable {
    fn id(&self) -> &'static str {
        "heuristic.v1"
    }

    fn evaluate(
        &self,
        candidate: &AdmissionCandidate,
        ctx: &AdmissionContext<'_>,
    ) -> Result<AdmissionDecision, AdmissionError> {
        // Dedup first — cheapest check, eliminates the most common drop case.
        if let Some(existing) = ctx
            .seen_content
            .find_by_content_hash(&candidate.content_hash)
        {
            return Ok(AdmissionDecision::Drop {
                reason: AdmissionDropReason::Duplicate {
                    existing_engram_id: existing,
                },
            });
        }

        // Length check
        let char_count = candidate.content.chars().count();
        if char_count < self.min_content_length {
            return Ok(AdmissionDecision::Drop {
                reason: AdmissionDropReason::NotMemorable {
                    explanation: format!(
                        "content too short ({} < {} chars)",
                        char_count, self.min_content_length
                    ),
                },
            });
        }

        // Noise phrase check. `noise_phrases` is pre-normalized
        // (lowercased + trimmed) at construction time, so the per-call
        // hot path is a plain string comparison.
        let normalized = candidate.content.trim().to_lowercase();
        for phrase in &self.noise_phrases {
            if normalized == *phrase {
                return Ok(AdmissionDecision::Drop {
                    reason: AdmissionDropReason::NotMemorable {
                        explanation: format!("matches noise phrase: {phrase:?}"),
                    },
                });
            }
        }

        // Admit
        Ok(AdmissionDecision::Admit {
            engram: build_engram_from_candidate(candidate, ctx),
            why: format!(
                "{} accepted (len={}, no dedup hit, no noise match)",
                self.id(),
                char_count
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        AdmissionConfig, AdmissionContext, AdmissionGate, AircMessageRef, EngramKind, EngramOrigin,
        SeenContentLookup, SeenEventLookup, TrustState,
    };
    use super::*;
    use crate::persona::trace::CognitionTrace;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    const FIXED_NOW_MS: u64 = 1_715_625_600_000;

    // Test fixtures duplicated from `admission/mod.rs::tests` because
    // Rust's `#[cfg(test)] mod` blocks aren't shareable across files.
    // Helpers are tiny and test-only; cost is low.

    #[derive(Default)]
    struct InMemoryContent(Mutex<HashMap<String, Uuid>>);

    impl SeenContentLookup for InMemoryContent {
        fn find_by_content_hash(&self, hash: &str) -> Option<Uuid> {
            self.0.lock().unwrap().get(hash).copied()
        }
    }

    #[derive(Default)]
    struct InMemoryEvents(Mutex<HashMap<String, u64>>);

    impl SeenEventLookup for InMemoryEvents {
        fn first_seen_ms(&self, event_id: &str) -> Option<u64> {
            self.0.lock().unwrap().get(event_id).copied()
        }
    }

    fn airc_ref(message_id: &str) -> AircMessageRef {
        AircMessageRef {
            transport: "airc".to_string(),
            room_id: "cambriantech".to_string(),
            message_id: message_id.to_string(),
            sender_id: "airc-8a5e".to_string(),
            sent_at_ms: FIXED_NOW_MS,
            received_at_ms: FIXED_NOW_MS,
            content_hash: "hash".to_string(),
            signature: "sig".to_string(),
            proof_refs: vec![],
            schema_version: "v1".to_string(),
            client_name: Some("airc-bash".to_string()),
        }
    }

    fn airc_candidate(content: &str, trust: TrustState, message_id: &str) -> AdmissionCandidate {
        AdmissionCandidate {
            content: content.to_string(),
            kind: EngramKind::Episodic,
            origin: EngramOrigin::Airc(airc_ref(message_id)),
            trust_state: trust,
            recall_keys: vec!["test".to_string()],
            content_hash: format!("sha256:fake-{}", content.len()),
        }
    }

    fn permissive_ctx<'a>(
        cfg: &'a AdmissionConfig,
        content: &'a InMemoryContent,
        events: &'a InMemoryEvents,
    ) -> AdmissionContext<'a> {
        AdmissionContext {
            config: cfg,
            seen_content: content,
            seen_events: events,
            now_ms: FIXED_NOW_MS,
        }
    }

    /// What this catches: content shorter than `min_content_length` drops
    /// with `NotMemorable` reason carrying the actual lengths. Operators
    /// debugging admission funnels need the explanation string to be
    /// informative, not opaque.
    #[test]
    fn heuristic_drops_short_content_with_explanation() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = airc_candidate("short", TrustState::ApprovedPeer, "msg-short");

        match AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        )
        .unwrap()
        {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::NotMemorable { explanation },
            } => {
                assert!(
                    explanation.contains("too short"),
                    "explanation: {explanation}"
                );
                assert!(
                    explanation.contains("16"),
                    "must mention threshold: {explanation}"
                );
            }
            other => panic!("expected Drop NotMemorable, got {other:?}"),
        }
    }

    /// What this catches: noise phrase match is case-insensitive and
    /// trim-tolerant, so "  ACK  " drops the same as "ack".
    #[test]
    fn heuristic_drops_noise_phrase_case_insensitive() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        // Pad with whitespace to clear length check; noise check fires after trim.
        let padded = "                ACK                ";
        let cand = airc_candidate(padded, TrustState::ApprovedPeer, "msg-noise");

        match AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        )
        .unwrap()
        {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::NotMemorable { explanation },
            } => {
                assert!(
                    explanation.contains("noise phrase"),
                    "explanation: {explanation}"
                );
            }
            other => panic!("expected Drop NotMemorable for noise phrase, got {other:?}"),
        }
    }

    /// What this catches: dedup hit returns `Drop::Duplicate` with the
    /// existing engram id surfaced. Recall surfaces depend on this id
    /// being present so they can link the new arrival back to the
    /// already-stored memory.
    #[test]
    fn heuristic_drops_duplicate_with_existing_engram_id() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let existing_id = Uuid::new_v4();
        content
            .0
            .lock()
            .unwrap()
            .insert("sha256:fake-29".to_string(), existing_id);
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = airc_candidate(
            "twenty-nine character content",
            TrustState::ApprovedPeer,
            "msg-d",
        );
        assert_eq!(cand.content_hash, "sha256:fake-29");

        match AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        )
        .unwrap()
        {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::Duplicate { existing_engram_id },
            } => {
                assert_eq!(existing_engram_id, existing_id);
            }
            other => panic!("expected Drop Duplicate, got {other:?}"),
        }
    }

    /// What this catches: when the heuristic admits, the synthesized
    /// `Engram` carries the full provenance + trust snapshot. A
    /// regression that drops the trust_state_at_admission would silently
    /// erase forensic context that later introspection needs.
    #[test]
    fn heuristic_admit_synthesizes_engram_with_full_provenance() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = airc_candidate(
            "design discussion about cognitive immune model layers",
            TrustState::IntragridMember,
            "msg-admit-1",
        );

        match AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        )
        .unwrap()
        {
            AdmissionDecision::Admit { engram, why } => {
                assert_eq!(engram.kind, EngramKind::Episodic);
                assert_eq!(engram.trust_state_at_admission, TrustState::IntragridMember);
                assert!(matches!(engram.origin, EngramOrigin::Airc(_)));
                assert_eq!(engram.admitted_at_ms, FIXED_NOW_MS);
                assert!(why.contains("heuristic.v1"), "why: {why}");
            }
            other => panic!("expected Admit, got {other:?}"),
        }
    }
}
