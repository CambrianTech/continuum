#![cfg(test)]
use super::*;

    /// What this catches: two personas under the same continuum_root
    /// get fully isolated stores. An engram admitted by Paige does
    /// not appear in Niko's recall, and vice-versa. The first
    /// defense for the chain-of-custody design's per-citizen scoping.
    #[tokio::test]
    pub(super) async fn for_persona_isolates_two_personas_at_the_storage_layer() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paige_home = crate::cognition::home::PersonaHome::for_persona(tmp.path(), "Paige");
        let niko_home = crate::cognition::home::PersonaHome::for_persona(tmp.path(), "Niko");

        // Admit through Paige's home only.
        let paige_id = {
            let registry = Arc::new(crate::cognition::recall_metadata::RecallMetadataRegistry::new());
            let state = AdmissionState::for_persona(&paige_home, registry)
                .await
                .expect("paige setup");
            let decision = state
                .admit(
                    &synthetic_human_message("only paige sees this engram"),
                    None,
                )
                .expect("paige admit");
            match decision {
                AdmissionDecision::Admit { engram, .. } => engram.id,
                other => panic!("expected Admit got {other:?}"),
            }
        };

        // Wait for Paige's fire-and-forget write to land.
        let mut tries = 0;
        loop {
            let paige_registry =
                Arc::new(crate::cognition::recall_metadata::RecallMetadataRegistry::new());
            let paige_state = AdmissionState::for_persona(&paige_home, paige_registry)
                .await
                .expect("paige reload");
            if paige_state.engram_count() == 1 {
                break;
            }
            tries += 1;
            if tries > 100 {
                panic!("paige's engram never landed");
            }
            tokio::task::yield_now().await;
        }

        // Niko's fresh state must NOT see Paige's engram.
        let niko_registry =
            Arc::new(crate::cognition::recall_metadata::RecallMetadataRegistry::new());
        let niko_state = AdmissionState::for_persona(&niko_home, niko_registry)
            .await
            .expect("niko setup");
        assert_eq!(
            niko_state.engram_count(),
            0,
            "Niko's home is independent of Paige's — no cross-persona engram leak"
        );

        // And Niko's scored recall returns nothing.
        let scored = niko_state.recall_scored(10_000, 8);
        assert!(
            scored.is_empty(),
            "Niko's recall is empty: paige's engram {paige_id} stayed scoped to her home"
        );
    }

    // What this catches: the per-task eval rewind invariant (cognition/eval.rs
    // run_pass calls `isolation.rewind()` before EVERY task). An engram admitted
    // while measuring task N — e.g. the act->observe self-observation from a
    // code/search she ran — must NOT survive into task N+1's recall, or the exam
    // bleeds ("based on my earlier code search, SELF_TICK_MS is in…" leaking into
    // an unrelated task, observed live 2026-07-02). checkpoint()/restore() is the
    // mechanism the rewind uses: restoring the pre-task checkpoint discards every
    // engram admitted since, while PRESERVING the baseline the fork was born with
    // (her real engrams via fork_detached) — mirror her reality, drop only the
    // cross-task bleed. See [[mirror-and-challenge-during-training-and-dream]].
    #[test]
    pub(super) fn restore_drops_post_checkpoint_engrams_but_keeps_baseline() {
        let state = AdmissionState::new(Arc::new(
            crate::cognition::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mut trace = CognitionTrace::new();

        // Baseline: one engram present when the "task" begins (stands in for the
        // real engrams the eval fork is born carrying).
        let baseline = "a durable baseline observation the fork was born carrying";
        state
            .admit(&synthetic_human_message(baseline), Some(&mut trace))
            .unwrap();
        assert_eq!(state.engram_count(), 1);

        // Checkpoint = the pre-task frame the rewind restores to.
        let cp = state.checkpoint();

        // Task N admits a fresh engram (the act->observe result-as-engram).
        let task_n = "task N just ran code/search and admitted this result engram";
        state
            .admit(&synthetic_human_message(task_n), Some(&mut trace))
            .unwrap();
        assert_eq!(state.engram_count(), 2);
        assert!(
            state.recall_recent(8).iter().any(|e| e.content == task_n),
            "task N's engram is recallable within task N (sanity)"
        );

        // Rewind before task N+1.
        state.restore(&cp);

        // Task N's engram is gone; the baseline reality survives untouched.
        assert_eq!(
            state.engram_count(),
            1,
            "rewind drops the post-checkpoint engram"
        );
        let recalled = state.recall_recent(8);
        assert!(
            !recalled.iter().any(|e| e.content == task_n),
            "task N's engram must NOT bleed into task N+1"
        );
        assert!(
            recalled.iter().any(|e| e.content == baseline),
            "the pre-eval baseline reality is preserved (mirror, not sterilize)"
        );
    }

    // what this catches: the supersession-review retrieval (#221 slice 2) is
    // RETRIEVAL, not judgment — Semantic-only (episodics are experience, not
    // beliefs), matched by recall-key overlap on content, short keys skipped as
    // noise, capped. If this ever returned episodics, the dream would ask the
    // model to "supersede" raw experience — plasticity eating her history
    // instead of her stale conclusions.
    #[test]
    pub(super) fn semantic_beliefs_matching_is_semantic_only_key_overlap() {
        let state = AdmissionState::new(Arc::new(
            crate::cognition::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mk = |kind: EngramKind, content: &str| Engram {
            id: Uuid::new_v4(),
            context_id: None,
            kind,
            content: content.to_string(),
            origin: EngramOrigin::SelfReflection {
                parent_engram_id: Uuid::new_v4(),
            },
            recall_keys: vec![],
            admitted_at_ms: 1,
            trust_state_at_admission: TrustState::SelfTrust,
            admission_trace_id: None,
        };
        let stale_belief = mk(
            EngramKind::Semantic,
            "You work with main.rs and wordstats.rs",
        );
        let other_belief = mk(EngramKind::Semantic, "The team prefers ranked-choice votes");
        let episode = mk(EngramKind::Episodic, "I edited main.rs and it compiled");
        let stale_id = stale_belief.id;
        for e in [stale_belief, other_belief, episode] {
            state.engrams.lock().unwrap().push(e);
        }
        let hits = state.semantic_beliefs_matching(
            &["main.rs".to_string(), "rs".to_string()], // "rs" = noise, skipped
            8,
        );
        assert_eq!(hits.len(), 1, "semantic-only, key-matched: {hits:?}");
        assert_eq!(hits[0].id, stale_id);
        // Cap respected; empty needles → empty.
        assert!(state.semantic_beliefs_matching(&[], 8).is_empty());
        assert!(state
            .semantic_beliefs_matching(&["main.rs".to_string()], 0)
            .is_empty());

        // Rotating window (#221 slice 2b): oldest-first, Semantic-only,
        // exclusion honored — what guarantees the review EVENTUALLY reaches
        // beliefs lexical overlap can't (the stale-Rust vs python-keys gap).
        let mut exclude = std::collections::HashSet::new();
        let oldest = state.semantic_beliefs_oldest_excluding(&exclude, u64::MAX, 1);
        assert_eq!(oldest.len(), 1);
        assert_eq!(
            oldest[0].content, "You work with main.rs and wordstats.rs",
            "oldest belief first (insertion order)"
        );
        exclude.insert(oldest[0].id);
        let next = state.semantic_beliefs_oldest_excluding(&exclude, u64::MAX, 4);
        assert_eq!(next.len(), 1, "episodics never enter the window: {next:?}");
        assert_eq!(next[0].content, "The team prefers ranked-choice votes");
    }
