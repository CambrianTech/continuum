#![cfg(test)]
use super::*;

    // what this catches (#166): the persona's own near-identical recent turns must
    // collapse to ONE recall slot, not take three — three "I ran `code/tree` to
    // explore the workspace…" with different tails otherwise drown distinct knowledge,
    // and recall-hit uplift then entrenches the restatement. Distinct memories, and
    // ones that merely share a short lead, are NEVER collapsed.
    #[test]
    pub(super) fn near_identical_own_turns_collapse_but_distinct_memories_survive() {
        let a = "I ran `code/tree` to explore the workspace structure, and I see a Rust project";
        let b = "I ran `code/tree`  to explore the workspace structure, and I found several dirs"; // same 48-char head, different tail + whitespace
        let c = "The deploy pipeline went green after the 4pm fix to the auth handler";
        // Same restated thought → near-duplicate.
        assert!(recall_near_duplicate(a, b));
        // Prefix relationship → near-duplicate.
        assert!(recall_near_duplicate(
            "I ran code/tree to explore",
            "I ran code/tree to explore the whole tree"
        ));
        // Genuinely different memory → NOT collapsed.
        assert!(!recall_near_duplicate(a, c));
        // Short shared lead only (< head window, no prefix) → NOT collapsed.
        assert!(!recall_near_duplicate(
            "the cat sat on the mat",
            "the cat ran up the wall today"
        ));
        // Empty never collapses.
        assert!(!recall_near_duplicate("", a));
    }

    // what this catches: bracket-tagged perception annotations ([pattern], [room …])
    // are never the recall query — the query is the newest MESSAGE. Regression for
    // the 2026-07-10 live query-dilution: the [pattern] brick, appended after a
    // directed question, became the query and recall surfaced spiral memories
    // instead of the asked-about fact. [t=…]-stamped messages still qualify.
    #[test]
    pub(super) fn focused_query_skips_annotation_lines() {
        let burst = "[room general]\n\
                     [t=100] Claude: what port does silver-harbor point to?\n\
                     [pattern] Anwen's last 9 messages repeat the same sentiment.";
        assert_eq!(
            focused_query(burst),
            "Claude: what port does silver-harbor point to?"
        );
        // A trailing plain message still wins over everything.
        let burst2 = "[room general]\nAsha: hello\nClaude: the real question";
        assert_eq!(focused_query(burst2), "Claude: the real question");
        // All-annotation burst degrades to the trimmed whole (never panics).
        assert_eq!(focused_query("[room general]"), "[room general]");
    }

    // what this catches: the rendered [recall] line carries STRUCTURAL provenance
    // (who + age from EngramOrigin, never content inspection). Regression for the
    // silver-harbor IDK (2026-07-10): bare unattributed quotes made a taught fact
    // and a bystander's question render identically, and the model completed the
    // "question repeated, no answer visible" pattern with "I don't know" — about
    // a fact two lines below in the same block.
    #[test]
    pub(super) fn provenance_prefix_attributes_who_and_age_structurally() {
        let me = Uuid::new_v4();
        let peer = Uuid::new_v4();
        let now: u64 = 10 * 24 * 60 * 60 * 1000;
        let engram_from = |sender: Uuid, age_ms: u64| Engram {
            context_id: None,
            id: Uuid::new_v4(),
            kind: EngramKind::Episodic,
            content: "staging gateway is on port 58057".to_string(),
            origin: EngramOrigin::Chat(ChatMessageRef {
                message_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: sender,
                posted_at_ms: now - age_ms,
                content_hash: "h".to_string(),
            }),
            recall_keys: Vec::new(),
            admitted_at_ms: now - age_ms,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        };
        // A peer's words render as heard; her own as said — and the age buckets
        // stay coarse and human, never raw milliseconds for the model to parrot.
        assert_eq!(
            provenance_prefix(&engram_from(peer, 3 * 60 * 60 * 1000), me, now),
            "(heard, 2h+ ago) "
        );
        assert_eq!(
            provenance_prefix(&engram_from(me, 5 * 60 * 1000), me, now),
            "(you said, 5m+ ago) "
        );
        assert_eq!(
            provenance_prefix(&engram_from(peer, 30_000), me, now),
            "(heard, moments ago) "
        );
        assert_eq!(
            provenance_prefix(&engram_from(peer, 3 * 24 * 60 * 60 * 1000), me, now),
            "(heard, 2d+ ago) "
        );

        // PEER-HEARD speech renders QUOTED — recalled speech is a quotation
        // (#134 specimen 2: honest provenance alone didn't stop a persona
        // broadcasting a peer's first-person recalled message as her own; quote
        // marks are the strongest structural not-your-words signal). Her OWN
        // words render bare — quoting yourself invites parroting yourself.
        assert_eq!(
            render_memory_line(&engram_from(peer, 3 * 60 * 60 * 1000), me, now),
            "- (heard, 2h+ ago) \u{201c}staging gateway is on port 58057\u{201d}"
        );
        assert_eq!(
            render_memory_line(&engram_from(me, 5 * 60 * 1000), me, now),
            "- (you said, 5m+ ago) staging gateway is on port 58057"
        );
    }

    /// Build a persona-scoped AdmissionState with `count` engrams, each tracked
    /// in the recall registry at a chosen salience. Mirrors the proven fixture
    /// in engram_source.rs. `last_decayed_ms = now` so a same-instant recall
    /// applies a no-op decay (multiplier ≈ 1) and the uplift is observable.
    pub(super) fn fixture(count: usize, now_ms: u64) -> (Uuid, Arc<AdmissionState>, Vec<Uuid>) {
        let persona = Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap();
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        let mut ids = Vec::new();
        for i in 0..count {
            let id = Uuid::new_v4();
            ids.push(id);
            let engram = Engram {
                context_id: None,
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
    pub(super) async fn surfaces_top_salience_memory_as_context() {
        let now = 1_000_000_000;
        let (persona, state, _ids) = fixture(5, now);
        let faculty = RecallFaculty::new(persona, state).with_clock(Arc::new(move || now));
        let c = faculty
            .contribute(&Workspace::new("what's the status?"))
            .await
            .expect("recall should bid when the store is non-empty");
        assert_eq!(c.faculty, FacultyId::Recall);
        assert!(c.decision.is_none(), "recall is context, never a verdict");
        // Regression for the 2026-09-01 KV-prefix churn: recall is re-scored
        // every turn, so it must render TRAILING (nearest generation), never in
        // the cacheable system prefix it would invalidate on every mutation.
        assert!(c.trailing, "recall must ride the volatile tail, not the stable head");
        // The most salient engram (highest index in the fixture) leads.
        assert!(
            c.content.contains("memory body number 4"),
            "top-salience memory should be surfaced, got: {}",
            c.content
        );
        assert!(c.salience > 0.0);
    }

    // what this catches: burst dedup (rung 3c). A memory whose content the
    // current window ALREADY shows is re-served duplication — it must drop
    // from recall (it paid rent twice, and its per-turn novelty as the recall
    // top is why the sticky cache never engaged: 0 fires across two live
    // windows). Memories NOT visible in the burst survive; short bodies are
    // exempt (containment is coincidence-prone).
    #[tokio::test]
    pub(super) async fn burst_dedup_drops_window_visible_memories_but_keeps_the_past() {
        let now = 1_000_000_000u64;
        // Own fixture: bodies long enough to clear the dedup floor (the shared
        // fixture's ~20-char bodies are deliberately exempt as too short).
        let persona = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        let body = |i: usize| {
            format!("long memory number {i} — it stretches well past the dedup floor with detail")
        };
        for i in 0..2usize {
            let id = Uuid::new_v4();
            state.push_for_test(Engram {
                context_id: None,
                id,
                kind: EngramKind::Episodic,
                content: body(i),
                origin: EngramOrigin::Chat(ChatMessageRef {
                    message_id: Uuid::new_v4(),
                    room_id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    posted_at_ms: now,
                    content_hash: format!("hash-{i}"),
                }),
                recall_keys: Vec::new(),
                admitted_at_ms: now,
                trust_state_at_admission: TrustState::ApprovedPeer,
                admission_trace_id: None,
            });
            recall_meta.admit(
                id,
                RecallMetadata {
                    salience: 0.9,
                    access_count: 0,
                    last_accessed_ms: 0,
                    protected_until_ms: 0,
                    last_decayed_ms: now,
                },
            );
        }
        let faculty = RecallFaculty::new(persona, state).with_clock(Arc::new(move || now));
        // Memory 1 appears VERBATIM in the burst: already perceived — recall
        // must not re-serve it. Memory 0 stays recallable.
        let burst = format!("Alice: earlier someone said\n{}\nAlice: next?", body(1));
        let c = faculty
            .contribute(&Workspace::new(burst.as_str()))
            .await
            .expect("the non-visible memory still bids");
        assert!(
            !c.content.contains("long memory number 1"),
            "a memory the window already shows must not be re-served: {}",
            c.content
        );
        assert!(
            c.content.contains("long memory number 0"),
            "the broader past survives dedup: {}",
            c.content
        );
    }

    // what this catches: sticky rendering (rung 3, from the rent ledger's first
    // verdict — warm KV breaks die in the recall run). Re-contributing with the
    // SAME surfaced set must return the byte-identical block even as the clock
    // ticks (age labels frozen — "16h ago" must not become "17h ago" and
    // invalidate the KV tail behind it). Admitting a NEW memory changes the set
    // and re-renders fresh.
    #[tokio::test]
    pub(super) async fn sticky_recall_freezes_bytes_until_the_surfaced_set_changes() {
        use std::sync::atomic::{AtomicU64, Ordering};
        let now = 1_000_000_000u64;
        let (persona, state, _ids) = fixture(3, now);
        let tick = Arc::new(AtomicU64::new(now));
        let t2 = tick.clone();
        let faculty = RecallFaculty::new(persona, state.clone())
            .with_clock(Arc::new(move || t2.load(Ordering::Relaxed)));
        let ws = Workspace::new("what's the status?");
        let first = faculty.contribute(&ws).await.expect("recall bids");
        // The clock advances 2 hours — a naive re-render would tick every age
        // label and mutate the block's bytes.
        tick.store(now + 2 * 3600 * 1000, Ordering::Relaxed);
        let second = faculty.contribute(&ws).await.expect("recall bids again");
        assert_eq!(
            first.content, second.content,
            "unchanged set must re-render BYTE-IDENTICAL (ages frozen) — this is \
             the KV stability the rent ledger attributed warm breaks to"
        );
        assert!(second.trailing, "sticky path preserves trailing placement");
    }

    // what this catches: empty store → abstain (None), not an empty bid.
    #[tokio::test]
    pub(super) async fn abstains_on_empty_store() {
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
    pub(super) async fn recall_closes_the_loop_uplifting_what_it_surfaces() {
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
    pub(super) fn recall_is_perception_tier() {
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
    pub(super) async fn memory_carries_context_across_turns() {
        use crate::persona::engram::AdmissionDecision;
        use crate::cognition::conversation::types::{InboxMessage, SenderType};

        let now1 = 1_000_000_000u64;
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta));

        // TURN 1: a decision worth remembering — stored through the real admission
        // pipeline (not a test back-door push).
        let msg = InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "Operator".to_string(),
            sender_type: SenderType::Human,
            content:
                "We decided to ship the new auth flow behind a feature flag and ramp to 10% first."
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
    pub(super) async fn relevance_beats_recency_with_embedder() {
        use crate::ai::embedding::LexicalEmbedder;

        let now = 1_000_000_000u64;
        let query = "what was our rollout plan for the auth flow again?";

        let seed = || {
            let recall_meta = Arc::new(RecallMetadataRegistry::new());
            let state = Arc::new(AdmissionState::new(recall_meta.clone()));
            let mk = |content: &str, salience: f32, age_ms: u64| {
                let id = Uuid::new_v4();
                state.push_for_test(Engram {
                    context_id: None,
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

    // what this catches: the LIVE recall failure mode the clean-query tests miss.
    // `contribute` embeds the WHOLE `world_state` as the query — and live, that is
    // the full room burst, not a tidy question. So a relevant fact must still
    // surface when the real question is BURIED in unrelated chatter, against
    // high-salience distractors that lexically match the NOISE. Observed live
    // (2026-06-22): taught a deploy codename, then "what's the codename?" surfaced
    // salient-but-irrelevant old prompts instead — the burst noise diluted the
    // query. RED here = recall is not robust to a noisy burst (the fix is to embed
    // a FOCUSED query, the triggering message, not the whole transcript).
    #[tokio::test]
    pub(super) async fn relevance_survives_a_noisy_burst_query() {
        use crate::ai::embedding::LexicalEmbedder;

        let now = 1_000_000_000u64;
        // The LIVE shape: the actual question buried in unrelated room chatter
        // whose words (lunch, game, coffee) match the high-salience distractors.
        // The persona reacts to the NEWEST message, so the triggering question is
        // the last line — preceded by unrelated, more-salient chatter.
        let noisy_burst = "[room 7f3a]\n\
             [t=1] alice: morning all, fresh coffee in the kitchen\n\
             [t=2] bob: did anyone catch the game last night, what a finish\n\
             [t=3] alice: reminder lunch is at noon, i booked the corner table\n\
             [t=4] joel: what's the deploy codename for our next release?";

        let seed = || {
            let recall_meta = Arc::new(RecallMetadataRegistry::new());
            let state = Arc::new(AdmissionState::new(recall_meta.clone()));
            let mk = |content: &str, salience: f32, age_ms: u64| {
                let id = Uuid::new_v4();
                state.push_for_test(Engram {
                    context_id: None,
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
            // The RELEVANT fact — lower salience (it's not the loudest memory):
            mk(
                "the deploy codename for our next release is BLUEHERON-7",
                0.4,
                60_000,
            );
            // HIGH-salience distractors that match the burst's NOISE, not the question:
            mk("lunch is at noon, the corner table is booked", 0.9, 0);
            mk("the game last night was a great finish", 0.9, 0);
            mk("there is fresh coffee in the kitchen", 0.9, 0);
            state
        };

        let persona = Uuid::new_v4();
        let smart = RecallFaculty::new(persona, seed())
            .with_limit(1)
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(LexicalEmbedder::new()));
        let c = smart
            .contribute(&Workspace::new(noisy_burst))
            .await
            .expect("recall should surface a memory");
        assert!(
            c.content.contains("BLUEHERON"),
            "recall must surface the codename memory relevant to the BURIED question, \
             not a salient memory matching the burst's noise; got: {}",
            c.content
        );
    }

    // what this catches: the RANKING clutter seen live (2026-06-22) — even after
    // the focused-query fix, a relevant but lower-salience fact got INTO recall but
    // ranked BELOW a max-salience irrelevant memory (an old prompt rehearsed to
    // salience 1.0 by the recall-hit loop). With `limit 1` that means the wrong
    // memory surfaces. This pins that the relevance blend weight is high enough for
    // relevance to overcome a maxed-out salience when the query clearly matches.
    #[tokio::test]
    pub(super) async fn relevance_outranks_max_salience_noise() {
        use crate::ai::embedding::LexicalEmbedder;

        let now = 1_000_000_000u64;
        let query = "what is the staging server's secret port?";

        let seed = || {
            let recall_meta = Arc::new(RecallMetadataRegistry::new());
            let state = Arc::new(AdmissionState::new(recall_meta.clone()));
            let mk = |content: &str, salience: f32| {
                let id = Uuid::new_v4();
                state.push_for_test(Engram {
                    context_id: None,
                    id,
                    kind: EngramKind::Episodic,
                    content: content.to_string(),
                    origin: EngramOrigin::Chat(ChatMessageRef {
                        message_id: Uuid::new_v4(),
                        room_id: Uuid::new_v4(),
                        sender_id: Uuid::new_v4(),
                        posted_at_ms: now,
                        content_hash: "h".to_string(),
                    }),
                    recall_keys: Vec::new(),
                    admitted_at_ms: now,
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
            // RELEVANT to the query, but LOW salience (a fresh fact, not yet rehearsed):
            mk("the staging server's secret port is 47823", 0.3);
            // IRRELEVANT, but MAXED salience (an old prompt the recall loop over-rehearsed):
            mk("run the ping tool and report the round-trip time", 1.0);
            state
        };

        let persona = Uuid::new_v4();
        let recall = RecallFaculty::new(persona, seed())
            .with_limit(1)
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(LexicalEmbedder::new()));
        let c = recall.contribute(&Workspace::new(query)).await.unwrap();
        assert!(
            c.content.contains("47823"),
            "a clearly-relevant fact must out-rank a maxed-salience irrelevant memory; got: {}",
            c.content
        );
    }

    /// A deliberation faculty that conditions its reply on what recall surfaced.
    pub(super) struct DeliberateOnRecall;

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
                    // Reference the most relevant recalled MEMORY line (each starts with
                    // "- "), skipping the section's memory-frame header.
                    let first_line = mem
                        .content
                        .lines()
                        .find(|l| l.trim_start().starts_with("- "))
                        .unwrap_or("")
                        .to_string();
                    Some(Contribution::verdict(
                        Decision::Speak {
                            text: format!("Picking up the thread — I recall: {first_line}"),
                        },
                        0.92,
                        "decision conditioned on recalled memory (phase-2 over phase-1 context)",
                    ))
                }
                None => Some(Contribution::verdict(
                    Decision::pass(),
                    0.4,
                    "no memory surfaced — nothing to ground a reply on",
                )),
            }
        }
    }

    /// A capture sink that pretty-prints the full tick so we can WATCH the mind.
    pub(super) struct PrintingSink;

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
                println!(
                    "  [{:<12}] {}",
                    c.faculty.as_str(),
                    c.content.replace('\n', " / ")
                );
            }
            println!("\n-- decision (output of deliberation over that context) --");
            println!("  {:?}", t.decision);
            println!("====================================\n");
        }
    }
