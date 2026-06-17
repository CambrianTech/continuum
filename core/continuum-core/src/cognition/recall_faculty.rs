//! RecallFaculty — the hippocampus as a Workspace faculty.
//!
//! This is the perception-tier faculty that pulls relevant memory into the
//! Global Workspace (§2 of PERSONA-BRAIN-ARCHITECTURE.md) each cognition tick.
//! It bids context that the deliberation faculty then reasons over in phase 2
//! (staged assembly — "pull relevant memory, THEN decide").
//!
//! ## Why this faculty (killing the recall split)
//!
//! Recall lived in two disconnected places: the RAG `EngramSource`
//! (`persona/engram_source.rs`), which ranks by `salience × recency` but does
//! **not** record recall hits, and `AdmissionState::recall_scored`, which ranks
//! AND closes the bidirectional loop (records the hit → uplifts salience →
//! observes persistence). The RAG path was the *one-way* one. `RecallFaculty`
//! routes recall through `recall_scored` — the loop-closing path — so a memory
//! that gets recalled into the workspace this tick is **strengthened for next
//! tick** (Hebbian rehearsal, use-it-keeps-it). That is the "goes both ways"
//! property: retrieval feeds back into encoding.
//!
//! ## ML-derived salience, not a hand-weight
//!
//! The faculty's bid carries the **top recalled memory's post-decay salience**
//! as its workspace salience — how relevant the hippocampus thinks its best hit
//! is. The arbiter integrates that score; it never invents one. There is no
//! caste, no mention test, no `if` — just the recall score competing for
//! attention.
//!
//! ## Future slice: query-conditioned relevance
//!
//! v1 surfaces the most salient + recent memories (the existing Algorithm-4
//! ranking). Conditioning recall on the *current burst* (the workspace
//! `world_state` as a query — topic similarity over engram embeddings) is the
//! next slice, when embeddings flow through. The faculty seam does not change
//! when that lands: the backend behind `contribute` gets smarter, the brain is
//! unchanged.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use super::embedding::{cosine_similarity, EmbeddingProvider};
use super::workspace::{Contribution, Faculty, FacultyId, Workspace};
use crate::persona::admission_state::AdmissionState;
use crate::persona::engram::Engram;

/// How many top engrams the faculty surfaces into the workspace per tick. A
/// bounded "spotlight" on memory — the arbiter further bounds what reaches the
/// decider.
const DEFAULT_RECALL_LIMIT: usize = 5;

/// When relevance re-ranking is active, over-fetch this many × the surface limit
/// as candidates, then narrow by relevance. Over-fetch so a topically-relevant
/// but lower-salience memory can still enter the running and win the re-rank.
const RERANK_CANDIDATE_MULTIPLIER: usize = 4;

/// Blend weight for relevance (cosine vs the burst) against the memory's
/// salience-decay score: `RELEVANCE_WEIGHT·rel + (1-RELEVANCE_WEIGHT)·salience`.
/// 0.5 = equal voice; tunable, and the replay A/B bench is exactly how we'll
/// tune it with before/after traces instead of guessing.
const RELEVANCE_WEIGHT: f32 = 0.5;

/// Blend cosine-relevance (to the burst) with the memory's salience-decay score.
fn blended_score(
    salience: f32,
    query: &[f32],
    embedder: &dyn EmbeddingProvider,
    content: &str,
) -> f32 {
    let rel = cosine_similarity(query, &embedder.embed(content));
    RELEVANCE_WEIGHT * rel + (1.0 - RELEVANCE_WEIGHT) * salience
}

/// Wall-clock seam — injectable so tests are deterministic. Returns ms since
/// the unix epoch, matching the `now_ms()` convention used across cognition.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// The default wall clock (ms since unix epoch).
fn wall_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    })
}

/// The hippocampus, exposed as a `Faculty`. Persona-scoped: it owns an
/// `Arc<AdmissionState>` (shared with the admission pipeline so encoding and
/// recall see the same store). Perception tier — bids in phase 1.
pub struct RecallFaculty {
    persona_id: Uuid,
    admission_state: Arc<AdmissionState>,
    limit: usize,
    clock: Clock,
    /// Optional relevance re-ranker. When set, recall surfaces the memory most
    /// RELEVANT to the current burst (cosine, blended with salience), not just
    /// the most salient/recent — the "memory works as designed at scale" path.
    /// `None` → pure salience×recency (the backwards-compatible default). The
    /// backend is swappable (lexical bootstrap now; neural local embedder later).
    embedder: Option<Arc<dyn EmbeddingProvider>>,
}

impl RecallFaculty {
    /// Construct with the default recall limit and wall clock, no re-ranker.
    pub fn new(persona_id: Uuid, admission_state: Arc<AdmissionState>) -> Self {
        Self {
            persona_id,
            admission_state,
            limit: DEFAULT_RECALL_LIMIT,
            clock: wall_clock(),
            embedder: None,
        }
    }

    /// Override how many memories are surfaced per tick.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit.max(1);
        self
    }

    /// Inject a deterministic clock (tests / replay).
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// Install a relevance re-ranker (embedding similarity vs the burst). Recall
    /// then surfaces the most relevant memory, not just the most recent.
    pub fn with_embedder(mut self, embedder: Arc<dyn EmbeddingProvider>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    /// The persona this faculty recalls for.
    pub fn persona_id(&self) -> Uuid {
        self.persona_id
    }
}

#[async_trait]
impl Faculty for RecallFaculty {
    fn id(&self) -> FacultyId {
        FacultyId::Recall
    }

    // Perception tier (reacts_to_broadcast() == false, the default): recall
    // reacts to the raw world-state, bidding its memories into phase 1 so the
    // deliberation faculty can condition on them in phase 2.
    async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
        let now = (self.clock)();

        // Fetch candidates WITHOUT recording hits — we record the hit on what we
        // actually SURFACE (below), not on candidates that lose the re-rank.
        // Over-fetch when re-ranking so a relevant-but-lower-salience memory can
        // still win.
        let fetch_n = if self.embedder.is_some() {
            self.limit.saturating_mul(RERANK_CANDIDATE_MULTIPLIER).max(self.limit)
        } else {
            self.limit
        };
        let candidates = self.admission_state.recall_candidates(now, fetch_n);
        if candidates.is_empty() {
            return None;
        }

        // Score: (final_score, engram, salience). With an embedder, final_score
        // blends cosine-relevance-to-the-burst with salience — so recall surfaces
        // the RELEVANT memory, not just the salient/recent one. Without one,
        // final_score IS salience (candidates already in that order).
        let mut scored: Vec<(f32, Engram, f32)> = match &self.embedder {
            Some(embedder) => {
                let query = embedder.embed(&ws.world_state);
                let mut s: Vec<(f32, Engram, f32)> = candidates
                    .into_iter()
                    .map(|(engram, salience)| {
                        let blended =
                            blended_score(salience, &query, embedder.as_ref(), &engram.content);
                        (blended, engram, salience)
                    })
                    .collect();
                s.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                s
            }
            None => candidates
                .into_iter()
                .map(|(engram, salience)| (salience, engram, salience))
                .collect(),
        };
        scored.truncate(self.limit);

        // Close the loop on what we ACTUALLY surface — Hebbian rehearsal on the
        // memories the persona truly used this tick (uplift + persistence).
        let surfaced_ids: Vec<Uuid> = scored.iter().map(|(_, e, _)| e.id).collect();
        self.admission_state.record_recall_hits(&surfaced_ids, now);

        // The faculty's salience = the top item's final score — relevance-aware
        // when re-ranking, salience otherwise. ML/algorithm-derived, never a
        // hand-weight; the arbiter integrates it.
        let top_salience = scored[0].0.clamp(0.0, 1.0);
        let content = scored
            .iter()
            .map(|(_, engram, salience)| {
                format!("- {} (salience {:.2})", engram.content, salience)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let reasoning = format!(
            "recalled {} memor{} ({}) — salience-uplifted, loop closed",
            scored.len(),
            if scored.len() == 1 { "y" } else { "ies" },
            if self.embedder.is_some() {
                "relevance-ranked vs the burst"
            } else {
                "salience×recency"
            }
        );

        Some(Contribution::context(
            FacultyId::Recall,
            content,
            top_salience,
            reasoning,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::engram::{ChatMessageRef, Engram, EngramKind, EngramOrigin, TrustState};
    use crate::persona::recall_metadata::{RecallMetadata, RecallMetadataRegistry};

    /// Build a persona-scoped AdmissionState with `count` engrams, each tracked
    /// in the recall registry at a chosen salience. Mirrors the proven fixture
    /// in engram_source.rs. `last_decayed_ms = now` so a same-instant recall
    /// applies a no-op decay (multiplier ≈ 1) and the uplift is observable.
    fn fixture(count: usize, now_ms: u64) -> (Uuid, Arc<AdmissionState>, Vec<Uuid>) {
        let persona = Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap();
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        let mut ids = Vec::new();
        for i in 0..count {
            let id = Uuid::new_v4();
            ids.push(id);
            let engram = Engram {
                id,
                kind: EngramKind::Episodic,
                content: format!("memory body number {i}"),
                origin: EngramOrigin::Chat(ChatMessageRef {
                    message_id: Uuid::new_v4(),
                    room_id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    posted_at_ms: now_ms.saturating_sub((i as u64) * 60_000),
                    content_hash: format!("hash-{i}"),
                }),
                recall_keys: Vec::new(),
                admitted_at_ms: now_ms.saturating_sub((i as u64) * 60_000),
                trust_state_at_admission: TrustState::ApprovedPeer,
                admission_trace_id: None,
            };
            state.push_for_test(engram);
            recall_meta.admit(
                id,
                RecallMetadata {
                    // Increasing salience so engram 0 is NOT the most salient —
                    // proves ranking, not insertion order.
                    salience: 0.4 + (i as f32 * 0.1).min(0.5),
                    access_count: 0,
                    last_accessed_ms: 0,
                    protected_until_ms: 0,
                    last_decayed_ms: now_ms,
                },
            );
        }
        (persona, state, ids)
    }

    // what this catches: the faculty surfaces recalled memory as a context
    // Contribution under FacultyId::Recall, with salience = the top hit's score.
    #[tokio::test]
    async fn surfaces_top_salience_memory_as_context() {
        let now = 1_000_000_000;
        let (persona, state, _ids) = fixture(5, now);
        let faculty = RecallFaculty::new(persona, state).with_clock(Arc::new(move || now));
        let c = faculty
            .contribute(&Workspace::new("what's the status?"))
            .await
            .expect("recall should bid when the store is non-empty");
        assert_eq!(c.faculty, FacultyId::Recall);
        assert!(c.decision.is_none(), "recall is context, never a verdict");
        // The most salient engram (highest index in the fixture) leads.
        assert!(
            c.content.contains("memory body number 4"),
            "top-salience memory should be surfaced, got: {}",
            c.content
        );
        assert!(c.salience > 0.0);
    }

    // what this catches: empty store → abstain (None), not an empty bid.
    #[tokio::test]
    async fn abstains_on_empty_store() {
        let now = 1_000_000_000;
        let (persona, state, _ids) = fixture(0, now);
        let faculty = RecallFaculty::new(persona, state).with_clock(Arc::new(move || now));
        assert!(faculty.contribute(&Workspace::new("hi")).await.is_none());
    }

    // what this catches: THE BIDIRECTIONAL LOOP — recalling a memory into the
    // workspace records a recall hit (access_count++, salience uplift). Retrieval
    // feeds back into encoding; the recalled memory is strengthened for next tick.
    // This is the half EngramSource never closed.
    #[tokio::test]
    async fn recall_closes_the_loop_uplifting_what_it_surfaces() {
        let now = 1_000_000_000;
        let (persona, state, ids) = fixture(3, now);
        let recall_meta = state.recall_metadata().clone();

        let before: Vec<(f32, u32)> = ids
            .iter()
            .map(|id| {
                let m = recall_meta.get(*id).unwrap();
                (m.salience, m.access_count)
            })
            .collect();

        let faculty = RecallFaculty::new(persona, state).with_clock(Arc::new(move || now));
        let _ = faculty.contribute(&Workspace::new("status?")).await;

        // Every surfaced engram had its access_count bumped (the hit was
        // recorded) and salience did not fall below where it started (uplift,
        // no net decay at the same instant).
        for (i, id) in ids.iter().enumerate() {
            let m = recall_meta.get(*id).unwrap();
            assert!(
                m.access_count > before[i].1,
                "access_count must rise — the recall hit closes the loop"
            );
            assert!(
                m.salience >= before[i].0,
                "salience must not fall — recall uplifts what it surfaces"
            );
        }
    }

    // what this catches: recall is a PERCEPTION-tier faculty — it bids in phase 1
    // over the raw world-state, so its memories are in the broadcast before the
    // deliberation faculty (phase 2) reasons over them.
    #[test]
    fn recall_is_perception_tier() {
        let now = 1_000_000_000;
        let (persona, state, _ids) = fixture(1, now);
        let faculty = RecallFaculty::new(persona, state);
        assert!(
            !faculty.reacts_to_broadcast(),
            "recall must bid in phase 1, not after the broadcast is assembled"
        );
    }

    // what this catches: MEMORY WORKS AS DESIGNED → coherence across turns. A
    // substantive statement ADMITTED in turn 1 (the real store path, admit()) is
    // RECALLED in a later turn — so the persona carries context forward instead of
    // amnesia each turn. Clock-controlled (recall runs moments after admit) so
    // decay doesn't floor it — the reproducible-clock pattern that the wall-clock
    // live run exposed as needed. This is the store→recall loop the conversation
    // coherence depends on.
    #[tokio::test]
    async fn memory_carries_context_across_turns() {
        use crate::persona::engram::AdmissionDecision;
        use crate::persona::types::{InboxMessage, SenderType};

        let now1 = 1_000_000_000u64;
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta));

        // TURN 1: a decision worth remembering — stored through the real admission
        // pipeline (not a test back-door push).
        let msg = InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "Joel".to_string(),
            sender_type: SenderType::Human,
            content: "We decided to ship the new auth flow behind a feature flag and ramp to 10% first."
                .to_string(),
            timestamp: now1,
            priority: 0.8,
            source_modality: None,
            voice_session_id: None,
        };
        let decision = state.admit(&msg, None).expect("admit should not error");
        assert!(
            matches!(decision, AdmissionDecision::Admit { .. }),
            "a substantive decision must be admitted to memory, got: {decision:?}"
        );

        // TURN 2 (moments later): recall must surface that decision so the persona
        // stays coherent with what was decided.
        let persona = Uuid::new_v4();
        let now2 = now1 + 5_000;
        let recall = RecallFaculty::new(persona, state).with_clock(Arc::new(move || now2));
        let c = recall
            .contribute(&Workspace::new(
                "what was our rollout plan for the auth flow again?",
            ))
            .await
            .expect("recall should surface the stored decision in a later turn");
        assert!(
            c.content.contains("feature flag"),
            "turn-2 recall must carry the turn-1 memory forward (coherence across turns); got: {}",
            c.content
        );
    }

    // what this catches: RELEVANCE BEATS RECENCY — recall with an embedder
    // surfaces the topically-relevant memory even when a MORE-salient, MORE-recent
    // but irrelevant memory exists. Without the embedder, salience wins and the
    // irrelevant memory surfaces. This is "memory works as designed at scale": as
    // memory grows, you need the RIGHT memory, not the latest. The lexical
    // embedder is the bootstrap; a neural one slots in behind the same trait.
    #[tokio::test]
    async fn relevance_beats_recency_with_embedder() {
        use crate::cognition::embedding::LexicalEmbedder;

        let now = 1_000_000_000u64;
        let query = "what was our rollout plan for the auth flow again?";

        let seed = || {
            let recall_meta = Arc::new(RecallMetadataRegistry::new());
            let state = Arc::new(AdmissionState::new(recall_meta.clone()));
            let mut mk = |content: &str, salience: f32, age_ms: u64| {
                let id = Uuid::new_v4();
                state.push_for_test(Engram {
                    id,
                    kind: EngramKind::Episodic,
                    content: content.to_string(),
                    origin: EngramOrigin::Chat(ChatMessageRef {
                        message_id: Uuid::new_v4(),
                        room_id: Uuid::new_v4(),
                        sender_id: Uuid::new_v4(),
                        posted_at_ms: now - age_ms,
                        content_hash: "h".to_string(),
                    }),
                    recall_keys: Vec::new(),
                    admitted_at_ms: now - age_ms,
                    trust_state_at_admission: TrustState::ApprovedPeer,
                    admission_trace_id: None,
                });
                recall_meta.admit(
                    id,
                    RecallMetadata {
                        salience,
                        access_count: 0,
                        last_accessed_ms: 0,
                        protected_until_ms: 0,
                        last_decayed_ms: now,
                    },
                );
            };
            // RELEVANT to the query, but LOWER salience and OLDER:
            mk(
                "we will ship the auth flow behind a feature flag and ramp the rollout to 10%",
                0.4,
                60_000,
            );
            // IRRELEVANT, but HIGHER salience and NEWER:
            mk("lunch is at noon, someone booked the corner table", 0.6, 0);
            state
        };

        let persona = Uuid::new_v4();

        // Without a re-ranker: salience wins → the irrelevant (more salient) memory.
        let plain = RecallFaculty::new(persona, seed())
            .with_limit(1)
            .with_clock(Arc::new(move || now));
        let pc = plain.contribute(&Workspace::new(query)).await.unwrap();
        assert!(
            pc.content.contains("lunch"),
            "salience-only recall surfaces the more-salient-but-irrelevant memory; got: {}",
            pc.content
        );

        // With the relevance re-ranker: the auth-flow memory wins DESPITE lower
        // salience — recall now surfaces what's relevant to the burst.
        let smart = RecallFaculty::new(persona, seed())
            .with_limit(1)
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(LexicalEmbedder::new()));
        let sc = smart.contribute(&Workspace::new(query)).await.unwrap();
        assert!(
            sc.content.contains("feature flag"),
            "relevance recall must surface the auth-flow memory despite lower salience; got: {}",
            sc.content
        );
    }

    // ---- The mind in action: real hippocampus → workspace → informed decision ----

    use super::super::workspace::{Decision, NoopWorkspaceCaptureSink, WorkspaceCaptureSink, WorkspaceCycle, WorkspaceTrace};

    /// A deliberation faculty that conditions its reply on what recall surfaced.
    struct DeliberateOnRecall;
    #[async_trait]
    impl Faculty for DeliberateOnRecall {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
            match ws.broadcast.iter().find(|c| c.faculty == FacultyId::Recall) {
                Some(mem) => {
                    // Reference the most relevant recalled line.
                    let first_line = mem.content.lines().next().unwrap_or("").to_string();
                    Some(Contribution::verdict(
                        Decision::Speak {
                            text: format!("Picking up the thread — I recall: {first_line}"),
                        },
                        0.92,
                        "decision conditioned on recalled memory (phase-2 over phase-1 context)",
                    ))
                }
                None => Some(Contribution::verdict(
                    Decision::Pass,
                    0.4,
                    "no memory surfaced — nothing to ground a reply on",
                )),
            }
        }
    }

    /// A capture sink that pretty-prints the full tick so we can WATCH the mind.
    struct PrintingSink;
    impl WorkspaceCaptureSink for PrintingSink {
        fn record(&self, t: &WorkspaceTrace) {
            println!("\n========== WORKSPACE TICK ==========");
            println!("world_state (the consolidated burst):\n  {}", t.world_state);
            println!("\n-- phase 1: all faculty bids (the full competition) --");
            for b in &t.bids {
                println!(
                    "  [{:<12}] salience {:.2}  {}  (why: {})",
                    b.faculty.as_str(),
                    b.salience,
                    b.content.replace('\n', " / "),
                    b.reasoning
                );
            }
            println!("\n-- assembled context the decider SAW (context_broadcast) --");
            for c in &t.context_broadcast {
                println!("  [{:<12}] {}", c.faculty.as_str(), c.content.replace('\n', " / "));
            }
            println!("\n-- decision (output of deliberation over that context) --");
            println!("  {:?}", t.decision);
            println!("====================================\n");
        }
    }

    // what this catches: END-TO-END — a real AdmissionState hippocampus bids
    // recalled memory into phase 1; the arbiter routes it into the broadcast;
    // the deliberation faculty in phase 2 reads it and produces a Decision that
    // REFERENCES the recalled memory. Prints the full trace (run with
    // `--nocapture` to watch). This is the coherence claim, demonstrated.
    #[tokio::test]
    async fn mind_in_action_recall_informs_the_decision() {
        let now = 1_000_000_000;
        let (persona, state, _ids) = fixture(3, now);

        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(RecallFaculty::new(persona, state).with_clock(Arc::new(move || now))),
            Arc::new(DeliberateOnRecall),
        ];
        let ws = WorkspaceCycle::new(faculties, Arc::new(super::super::workspace::SalienceArbiter), 5)
            .with_capture(Arc::new(PrintingSink))
            .run("teammate asks: where did we land on the deploy?")
            .await;

        match ws.decision() {
            Some(Decision::Speak { text }) => assert!(
                text.contains("memory body"),
                "the spoken decision must be grounded in recalled memory, got: {text}"
            ),
            other => panic!("expected a recall-grounded Speak, got {other:?}"),
        }

        // Silence the unused-import lint when this module's other helpers vary.
        let _ = NoopWorkspaceCaptureSink;
    }
}
