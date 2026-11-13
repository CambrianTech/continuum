#![cfg(test)]
use super::*;

    // what this catches: END-TO-END — a real AdmissionState hippocampus bids
    // recalled memory into phase 1; the arbiter routes it into the broadcast;
    // the deliberation faculty in phase 2 reads it and produces a Decision that
    // REFERENCES the recalled memory. Prints the full trace (run with
    // `--nocapture` to watch). This is the coherence claim, demonstrated.
    #[tokio::test]
    pub(super) async fn mind_in_action_recall_informs_the_decision() {
        let now = 1_000_000_000;
        let (persona, state, _ids) = fixture(3, now);

        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(RecallFaculty::new(persona, state).with_clock(Arc::new(move || now))),
            Arc::new(DeliberateOnRecall),
        ];
        let ws = WorkspaceCycle::new(
            faculties,
            Arc::new(super::super::super::workspace::SalienceArbiter),
        )
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

    // what this catches: THE CAESAR-PROMPT CONTAMINATION BUG (2026-06-27). Live,
    // 5 memories rehearsed to salience 1.0 by the recall-hit loop but topically
    // UNRELATED to the task ("caesar cipher") polluted every prompt — blending to
    // ~0.5 (0.5·0 rel + 0.5·1.0 sal), well above any blended floor, so they slipped
    // through. The closest-match floor gates on the RELEVANCE component, not the
    // blend, so a cosine-~0 nag is dropped no matter how salient. RED before the
    // floor: the nags surface and crowd out / dilute the one relevant memory.
    #[tokio::test]
    pub(super) async fn recall_drops_topically_irrelevant_high_salience_nags() {
        use crate::ai::embedding::LexicalEmbedder;
        let now = 1_000_000_000u64;
        let query = "write a caesar cipher function";

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
        // The one RELEVANT memory, modest salience.
        mk("caesar cipher shifts each letter by fixed amount", 0.4);
        // Five high-salience nags, each topically orthogonal (no shared tokens with
        // the task) — mirrors the live contamination: code prompts ("nth prime",
        // "fibonacci") rehearsed to salience 1.0 but unrelated to the caesar task.
        mk("lunchtime sandwiches arrived early", 1.0);
        mk("soccer scores updated overnight", 1.0);
        mk("espresso machine needs descaling", 1.0);
        mk("office ferns require watering", 1.0);
        mk("garage shutters lock midnight", 1.0);

        let recall = RecallFaculty::new(Uuid::new_v4(), state)
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(LexicalEmbedder::new()))
            .with_context_window(8192);
        let c = recall
            .contribute(&Workspace::new(query))
            .await
            .expect("the relevant memory should surface");
        assert!(
            c.content.contains("caesar cipher shifts"),
            "the topically-relevant memory must surface; got: {}",
            c.content
        );
        for nag in ["lunch", "game", "coffee", "plants", "parking"] {
            assert!(
                !c.content.contains(nag),
                "the irrelevant high-salience nag '{nag}' must be dropped by the closest-match floor; got: {}",
                c.content
            );
        }
    }

    // what this catches: the recall COUNT scales with the served model's
    // capability (proxied by context window) — a tight 4B window surfaces fewer
    // memories than a cloud-class one, so a small model isn't buried under memory
    // it can't juggle. `0` (window unknown) keeps the historical default of 5.
    #[test]
    pub(super) fn recall_count_scales_with_context_window() {
        assert_eq!(
            recall_count_for_window(0),
            5,
            "unknown window → historical default"
        );
        assert_eq!(
            recall_count_for_window(4096),
            3,
            "tight 4B window → fewer memories"
        );
        assert_eq!(recall_count_for_window(16384), 5);
        assert_eq!(recall_count_for_window(65536), 8);
        assert_eq!(
            recall_count_for_window(262144),
            12,
            "cloud-class window → more memories"
        );
        // Monotonic non-decreasing across KNOWN windows (0 is the unknown sentinel,
        // excluded — it deliberately returns the historical default, not the floor).
        let windows = [4096u32, 8192, 32768, 131072, 262144];
        for pair in windows.windows(2) {
            assert!(
                recall_count_for_window(pair[1]) >= recall_count_for_window(pair[0]),
                "count must not shrink as the window grows: {pair:?}"
            );
        }
    }

    // what this catches: the capability count BOUNDS how many memories surface even
    // when many are relevant — a tight 4B window (4096) caps recall at 3 though 10
    // equally-relevant memories qualify. This is the "limited to a reasonable
    // number by the model metric" half of the budget.
    #[tokio::test]
    pub(super) async fn recall_count_is_bounded_by_a_tight_window() {
        use crate::ai::embedding::LexicalEmbedder;
        let now = 1_000_000_000u64;
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        for i in 0..10 {
            let id = Uuid::new_v4();
            state.push_for_test(Engram {
                context_id: None,
                id,
                kind: EngramKind::Episodic,
                content: format!("auth flow rollout note {i}"),
                origin: EngramOrigin::Chat(ChatMessageRef {
                    message_id: Uuid::new_v4(),
                    room_id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    posted_at_ms: now,
                    content_hash: format!("h{i}"),
                }),
                recall_keys: Vec::new(),
                admitted_at_ms: now,
                trust_state_at_admission: TrustState::ApprovedPeer,
                admission_trace_id: None,
            });
            recall_meta.admit(
                id,
                RecallMetadata {
                    salience: 0.5,
                    access_count: 0,
                    last_accessed_ms: 0,
                    protected_until_ms: 0,
                    last_decayed_ms: now,
                },
            );
        }
        let recall = RecallFaculty::new(Uuid::new_v4(), state)
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(LexicalEmbedder::new()))
            .with_context_window(4096);
        let c = recall
            .contribute(&Workspace::new("auth flow rollout"))
            .await
            .expect("relevant memories should surface");
        assert_eq!(
            // Count MEMORY lines (each starts with "- "), not the section frame header.
            c.content
                .lines()
                .filter(|l| l.trim_start().starts_with("- "))
                .count(),
            3,
            "a 4096-token window caps recall at 3 memories; got:\n{}",
            c.content
        );
    }

    // what this catches: the recency→semantic handoff. The persona's own just-happened
    // act lands in BOTH tiers by design (an 800-char head in working memory, the full
    // result as an engram — act_observe::apply_act). Without the dedup, one prompt shows
    // the act twice: its head in [working-memory] AND its full body in [recall], burning
    // prefill on a byte-overlapping block. With the working-memory Arc shared in, recall
    // drops the engram whose content the working-memory head already carries (exact
    // prefix match), while an UNRELATED memory still surfaces. Regression for
    // [[act-results-need-a-recency-channel-not-semantic-recall]].
    #[tokio::test]
    pub(super) async fn recall_drops_an_act_the_recency_channel_already_carries() {
        let now = 1_000_000_000u64;
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));

        // The persona's own action observation (the exact string apply_act admits) and
        // an unrelated long-term memory. Same salience so ordering can't hide the drop.
        let own_act =
            "I ran code/run(source=fn is_prime) because I must verify. Result: 2 prime, 4 not";
        let other = "The team agreed to ship the auth flow on Friday.";
        for content in [own_act, other] {
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
                    content_hash: format!("h-{}", content.len()),
                }),
                recall_keys: Vec::new(),
                admitted_at_ms: now,
                trust_state_at_admission: TrustState::ApprovedPeer,
                admission_trace_id: None,
            });
            recall_meta.admit(
                id,
                RecallMetadata {
                    salience: 0.7,
                    access_count: 0,
                    last_accessed_ms: 0,
                    protected_until_ms: 0,
                    last_decayed_ms: now,
                },
            );
        }

        // The recency channel carries the SAME act (its head, stamped), exactly as
        // apply_act records it after executing the Decision::Act.
        let wm = Arc::new(WorkingMemory::new(3));
        wm.record_receipt(own_act);

        // No embedder → pure salience×recency, relevance floor disabled → both would
        // surface if not for the working-memory dedup. Large window so count is not the
        // limiter.
        let recall = RecallFaculty::new(Uuid::new_v4(), state)
            .with_clock(Arc::new(move || now))
            .with_context_window(131_072)
            .with_working_memory(Arc::clone(&wm));
        let c = recall
            .contribute(&Workspace::new("is_prime check"))
            .await
            .expect("the unrelated memory still surfaces");
        assert!(
            !c.content.contains("code/run(source=fn is_prime)"),
            "recall must NOT re-surface an act the recency channel already carries; got:\n{}",
            c.content
        );
        assert!(
            c.content.contains("ship the auth flow on Friday"),
            "an unrelated long-term memory must still surface; got:\n{}",
            c.content
        );
    }

    // what this catches: with NO working memory shared in (harness / backward-compatible
    // path), recall is unchanged — the act engram still surfaces. Guards against the
    // dedup firing when it has no recency channel to defer to.
    #[tokio::test]
    pub(super) async fn recall_keeps_the_act_when_no_recency_channel_is_shared() {
        let now = 1_000_000_000u64;
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        let own_act =
            "I ran code/run(source=fn is_prime) because I must verify. Result: 2 prime, 4 not";
        let id = Uuid::new_v4();
        state.push_for_test(Engram {
            context_id: None,
            id,
            kind: EngramKind::Episodic,
            content: own_act.to_string(),
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
                salience: 0.7,
                access_count: 0,
                last_accessed_ms: 0,
                protected_until_ms: 0,
                last_decayed_ms: now,
            },
        );
        // No .with_working_memory(...) — the dedup has nothing to defer to.
        let recall = RecallFaculty::new(Uuid::new_v4(), state)
            .with_clock(Arc::new(move || now))
            .with_context_window(131_072);
        let c = recall
            .contribute(&Workspace::new("is_prime check"))
            .await
            .expect("the act memory surfaces with no recency channel");
        assert!(
            c.content.contains("code/run(source=fn is_prime)"),
            "without a working-memory channel, recall must keep the act; got:\n{}",
            c.content
        );
    }

    /// Test-only anisotropic embedding space: unrelated pairs baseline at a
    /// NONZERO cosine (like the neural Qwen3 space, measured ≈0.25–0.3), with a
    /// calibrated null. `embed` keys canned contents to fixed 2-d unit vectors —
    /// a geometry fixture, not matching logic.
    pub(super) struct AnisotropicStub;

    #[async_trait]
    impl crate::ai::embedding::EmbeddingProvider for AnisotropicStub {
        fn id(&self) -> &str {
            "test-anisotropic"
        }
        fn dim(&self) -> usize {
            2
        }
        async fn embed(&self, text: &str) -> Vec<f32> {
            if text.contains("BLUEHERON") {
                vec![0.8, 0.6] // cos vs query = 0.8 → z = (0.8−0.27)/0.04 ≈ 13 — significant
            } else if text.contains("deploy codename") {
                vec![1.0, 0.0] // the query
            } else {
                vec![0.28, 0.96] // cos vs query = 0.28 ≈ the null — NOT significant
            }
        }
        fn unrelated_null(&self) -> Option<(f32, f32)> {
            Some((0.27, 0.04)) // the measured anisotropy of this space
        }
    }

    // what this catches: the live 2026-07-10 hippocampus bug — under an
    // ANISOTROPIC space (unrelated baseline ≈0.27, NOT 0) the old absolute floor
    // (0.15 < baseline) filtered nothing, so a salience-saturated room nag
    // surfaced identically on 11/11 unrelated coding tasks. The calibrated
    // SIGNIFICANCE gate must (a) DROP null-scoring junk even at salience 0.99,
    // (b) SURFACE a true match (several σ above the null) despite lower salience,
    // and (c) surface NOTHING when no memory rejects the null — never pad the
    // prompt with the least-irrelevant memory.
    #[tokio::test]
    pub(super) async fn significance_gate_beats_saturated_salience_under_anisotropic_space() {
        let now = 1_000_000_000u64;
        let query = "what was the deploy codename again?";
        let seed = |with_match: bool| {
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
            // Rehearsal-saturated junk (the "enough goodbyes" class): max salience,
            // cosine AT the null.
            mk("enough goodbyes, the board has real work", 0.99);
            mk("hello, how can I assist you today", 0.99);
            mk("let me check the current task at hand", 0.99);
            if with_match {
                // The genuinely relevant memory — LOWER salience.
                mk("the codename is BLUEHERON-7", 0.4);
            }
            state
        };

        // (a)+(b): the true match surfaces; every saturated-junk memory is dropped.
        let recall = RecallFaculty::new(Uuid::new_v4(), seed(true))
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(AnisotropicStub));
        let c = recall
            .contribute(&Workspace::new(query))
            .await
            .expect("the significant memory must surface");
        assert!(
            c.content.contains("BLUEHERON"),
            "the true match must surface despite lower salience; got:\n{}",
            c.content
        );
        assert!(
            !c.content.contains("goodbyes") && !c.content.contains("assist you"),
            "null-scoring junk must be dropped even at salience 0.99; got:\n{}",
            c.content
        );

        // (c): with NO memory rejecting the null, recall surfaces NOTHING.
        let recall = RecallFaculty::new(Uuid::new_v4(), seed(false))
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(AnisotropicStub));
        assert!(
            recall.contribute(&Workspace::new(query)).await.is_none(),
            "an all-junk pool must surface nothing, never the least-irrelevant memory"
        );
    }
