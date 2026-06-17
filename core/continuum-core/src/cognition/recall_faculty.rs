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

use super::workspace::{Contribution, Faculty, FacultyId, Workspace};
use crate::persona::admission_state::AdmissionState;

/// How many top engrams the faculty surfaces into the workspace per tick. A
/// bounded "spotlight" on memory — the arbiter further bounds what reaches the
/// decider.
const DEFAULT_RECALL_LIMIT: usize = 5;

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
}

impl RecallFaculty {
    /// Construct with the default recall limit and wall clock.
    pub fn new(persona_id: Uuid, admission_state: Arc<AdmissionState>) -> Self {
        Self {
            persona_id,
            admission_state,
            limit: DEFAULT_RECALL_LIMIT,
            clock: wall_clock(),
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
    async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
        let now = (self.clock)();
        // recall_scored ranks by salience-modulated decay AND closes the
        // bidirectional loop: it records a recall hit on each returned engram
        // (uplift + access_count + persistence observe). Recalling into the
        // workspace strengthens the memory — use-it-keeps-it.
        let scored = self.admission_state.recall_scored(now, self.limit);
        if scored.is_empty() {
            return None;
        }

        // The faculty's salience = the best recalled memory's relevance. ML-/
        // algorithm-derived, never a hand-weight; the arbiter integrates it.
        let top_salience = scored[0].1.clamp(0.0, 1.0);

        let content = scored
            .iter()
            .map(|(engram, salience)| format!("- {} (salience {:.2})", engram.content, salience))
            .collect::<Vec<_>>()
            .join("\n");

        let reasoning = format!(
            "recalled {} memor{} via recall_scored (salience-uplifted — the loop is closed)",
            scored.len(),
            if scored.len() == 1 { "y" } else { "ies" }
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
