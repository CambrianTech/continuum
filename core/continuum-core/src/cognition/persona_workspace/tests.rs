#![cfg(test)]
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::time::Duration;

    use tokio::sync::{watch, Notify};
    use tokio::time::timeout;

    // what this catches (card 822c4253): her hands moving to a card's own checkout with no
    // word (Kimi's code/read of career-wrangler/docs/... failing in a per-card worktree),
    // or the notice repeating every turn at the same checkout. Told on a new root, silent
    // on the same one, told again when it moves; the notice names root, branch and the
    // home tree that did not move.
    #[test]
    fn she_is_told_when_her_hands_move_and_only_then() {
        let kimi = Uuid::new_v4();
        let card = std::path::Path::new("/w/worktrees/af4e2cea");
        assert!(!already_told(kimi, card), "first rooting is news");
        assert!(!already_told(kimi, card), "checking does not record: only a pinned line does");
        mark_told(kimi, card);
        assert!(already_told(kimi, card), "the same checkout next turn is not news");
        assert!(!already_told(kimi, std::path::Path::new("/w/worktrees/b0b0b0b0")));
        assert!(!already_told(Uuid::new_v4(), card), "per citizen, never shared");
        let line = rooted_notice("CambrianTech/career-wrangler", card, "af4e2cea/a-real-product");
        for want in ["/w/worktrees/af4e2cea", "af4e2cea/a-real-product", "docs/x.md", "home workspace is unchanged"] {
            assert!(line.contains(want), "{want}: {line}");
        }
    }

    // what this catches (Astra's block on #4315): the wake's verdict must come from the
    // board's holder projection, never from `owner` read as authority. An EXPIRED claim
    // still names its last holder — which may be HER — so "X took it" from `owner` would
    // tell Kimi that Kimi took her own card; a card in Review is absent from her live
    // claims because its life advanced, not because a lease lapsed. Each branch pinned at
    // a fixed clock.
    #[test]
    fn the_wakes_verdict_is_the_boards_never_the_owner_fields() {
        use crate::cognition::working_memory::ClaimGone;
        use airc_core::PeerId;
        use airc_work::{CardState, Priority, RepoId, WorkCard, WorkCardId};
        let now = 1_000_000u64;
        let card = |owner: Option<PeerId>, claimed: bool, expires: Option<u64>, state: CardState| WorkCard {
            card_id: WorkCardId::new(),
            repo: RepoId::new("CambrianTech/continuum").expect("valid repo id in fixture"),
            title: "a card".to_string(),
            body: None,
            priority: Priority::P2,
            lane_id: None,
            state,
            owner,
            claim_provenance: None,
            claim_id: claimed.then(|| airc_work::ClaimId::from_uuid(Uuid::new_v4())),
            claim_expires_at_ms: expires,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: PeerId::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
            reviews: None,
            submissions: Vec::new(),
            last_submission_rejection: None,
        };
        let her = PeerId::new();
        let me = her.as_uuid();
        let peer = PeerId::new();
        // Her own EXPIRED claim: the owner field still says her — the verdict must be Lapsed,
        // never "her took it".
        let lapsed_self = card(Some(her), true, Some(now - 1), CardState::Claimed);
        assert_eq!(claim_gone_verdict(&lapsed_self, now, me), Some(ClaimGone::Lapsed { expired_at_ms: Some(now - 1) }));
        // Her own LIVE hold on the second read (a renewal or re-claim landed between the
        // wake's two board reads): nothing is gone — never "you lost it, <her uuid> took it".
        let held_self = card(Some(her), true, Some(now + 60_000), CardState::Claimed);
        assert_eq!(claim_gone_verdict(&held_self, now, me), None);
        // A peer's LIVE lease: held by them, with the edge.
        let held = card(Some(peer), true, Some(now + 60_000), CardState::Claimed);
        assert_eq!(claim_gone_verdict(&held, now, me), Some(ClaimGone::HeldBy { peer: peer.as_uuid(), expires_at_ms: Some(now + 60_000) }));
        // A peer's EXPIRED lease: lapsed and open, the peer is not named as holder.
        let lapsed_peer = card(Some(peer), true, Some(now - 1), CardState::Claimed);
        assert_eq!(claim_gone_verdict(&lapsed_peer, now, me), Some(ClaimGone::Lapsed { expired_at_ms: Some(now - 1) }));
        // Review / Merged / Closed: the card moved on — whatever the lease fields say, even
        // a live lease in her own name.
        for state in [CardState::Review, CardState::Merged, CardState::Closed] {
            let moved = card(Some(her), true, Some(now + 60_000), state);
            assert!(matches!(claim_gone_verdict(&moved, now, me), Some(ClaimGone::MovedOn { .. })), "{state:?}");
        }
        // Unclaimed and open: lapsed-and-open is the honest word for "nobody holds it".
        let open = card(None, false, None, CardState::Open);
        assert_eq!(claim_gone_verdict(&open, now, me), Some(ClaimGone::Lapsed { expired_at_ms: None }));
    }

    // what this catches (Astra, #4315 round 2): a belief the board has not answered must
    // SURVIVE the next checkpoint. `snapshot()` writes `acting_card: None` and the writer
    // stamps the ambient root; with her hands unheld and the board unseen, that wrote None
    // and a second restart forgot the comparison. Restore → unknown → save → restore must
    // carry the same card; once the board has answered (take), the next save stamps the
    // ambient root again.
    #[test]
    fn an_unanswered_belief_survives_the_next_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let _home = crate::paths::NativeHomeOverride::install(&dir.path().join("native"));
        let persona = Uuid::new_v4();
        let believed = Uuid::new_v4();
        // First lifetime: rooted at `believed` when the checkpoint was written.
        let first = WorkingMemory::new(8);
        let mut snap = first.snapshot();
        snap.acting_card = Some(believed);
        let woke = WorkingMemory::new(8);
        woke.restore(snap);
        assert_eq!(woke.peek_restored_acting_card(), Some(believed));
        // The board is unseen this lifetime (Unknown keeps the belief); hands unheld, so
        // the ambient root is None. The periodic checkpoint runs.
        woke.note_claim_gone(believed, crate::cognition::working_memory::ClaimGone::Unknown);
        assert_eq!(acting_card_of(persona), None, "fixture premise: hands unheld");
        save_volatile(persona, SaveReason::Seam, None, &woke).unwrap();
        let persisted = load_volatile(persona).unwrap().expect("checkpoint written");
        assert_eq!(persisted.wm.acting_card, Some(believed), "the unanswered belief is carried, not settled by forgetting");
        // Second restart: the comparison is still pending.
        let again = WorkingMemory::new(8);
        again.restore(persisted.wm);
        assert_eq!(again.peek_restored_acting_card(), Some(believed));
        // The board answers: consumed. The next checkpoint stamps the ambient root (None here).
        again.take_restored_acting_card();
        save_volatile(persona, SaveReason::Seam, None, &again).unwrap();
        let settled = load_volatile(persona).unwrap().expect("checkpoint written");
        assert_eq!(settled.wm.acting_card, None, "answered beliefs are not resurrected");
    }

    // what this catches: the restart gap (Astra on card 9197cd01). Across the restart a deploy
    // performs, before a mind restores, its references exist only in its persisted checkpoint;
    // the spill guard must read them from there. A newer checkpoint is seen (the cache keys on
    // its modification time), and an unreadable one is an error the guard fails closed on.
    #[test]
    fn a_persisted_checkpoint_s_result_handles_are_readable_before_restore() {
        let dir = std::env::temp_dir().join(format!("volatile-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("volatile.json");
        let write = |handle: &str, age_secs: u64| {
            let memory = WorkingMemory::new(8);
            memory.record_receipt_typed(
                &[],
                "a saved result",
                None,
                crate::ai::types::ResultCustody::Whole { handle: handle.into() },
            );
            let persisted = PersistedVolatile {
                wm: memory.snapshot(),
                own_speech: OwnSpeechPersisted::ByRoom(Vec::new()),
            };
            std::fs::write(&path, serde_json::to_vec(&persisted).unwrap()).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(age_secs))
                .unwrap();
        };
        // A result that collapsed out of the ring is referenced by its POINTER: the checkpoint
        // of a mind with no live working memory still protects it (Astra on acdfb010a).
        {
            let memory = WorkingMemory::new(8);
            memory.set_served_window(16_384);
            let cap = memory.budget().recent_results_chars();
            let saved = |handle: &str| crate::ai::types::ResultCustody::Whole { handle: handle.into() };
            memory.record_receipt_typed(&[], &"c".repeat(cap / 2), None, saved("c011a75ed0000001"));
            for _ in 0..4 {
                memory.record_receipt_typed(&[], &"s".repeat(cap / 2), None, saved("5000000000000002"));
            }
            memory.record_settlement("done", None);
            let persisted = PersistedVolatile {
                wm: memory.snapshot(),
                own_speech: OwnSpeechPersisted::ByRoom(Vec::new()),
            };
            drop(memory);
            std::fs::write(&path, serde_json::to_vec(&persisted).unwrap()).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(200))
                .unwrap();
            assert!(
                result_handles_at(&path).unwrap().contains("c011a75ed0000001"),
                "a collapsed result's handle is protected through its persisted pointer"
            );
        }
        write("0123456789abcdef", 100);
        assert!(result_handles_at(&path).unwrap().contains("0123456789abcdef"));
        write("fedcba9876543210", 10);
        let handles = result_handles_at(&path).unwrap();
        assert!(handles.contains("fedcba9876543210") && !handles.contains("0123456789abcdef"));
        std::fs::write(&path, b"not a checkpoint").unwrap();
        assert!(result_handles_at(&path).is_err(), "unreadable: the guard fails closed");
        assert!(result_handles_at(&dir.join("absent.json")).unwrap().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    // Uses the real persisted schema and WorkingMemory writer, not a second
    // checkpoint model. Explicit legacy root belongs to each temporary fixture.
    fn adoption_source(
        root: &std::path::Path,
        persona: Uuid,
        text: &str,
    ) -> (std::path::PathBuf, Vec<u8>) {
        let memory = WorkingMemory::new(8);
        memory.record_receipt(text);
        let snapshot = PersistedVolatile {
            wm: memory.snapshot(),
            own_speech: OwnSpeechPersisted::ByRoom(vec![(Uuid::new_v4(), vec![text.into()])]),
        };
        let bytes = serde_json::to_vec(&snapshot).expect("test: actual snapshot serializes");
        let path = root.join(persona.to_string()).join("volatile.json");
        std::fs::create_dir_all(path.parent().expect("test: fixture parent"))
            .expect("test: fixture directory");
        std::fs::write(&path, &bytes).expect("test: legacy checkpoint");
        (path, bytes)
    }

    // 9f160b78: explicit selection preserves WHOLE bytes plus both origins,
    // even when the caller chooses an older sequence than the destination.
    #[test]
    fn legacy_adoption_preserves_selected_bytes_and_is_bound_on_replay() {
        use checkpoint_adoption::{adopt, inspect, AdoptionError};
        let dir = tempfile::tempdir().unwrap();
        let _home = crate::paths::NativeHomeOverride::install(&dir.path().join("native"));
        let persona = Uuid::new_v4();
        let memory = WorkingMemory::new(8);
        memory.record_receipt("different previous lifetime");
        memory.record_receipt("higher sequence is not a selection policy");
        save_volatile(persona, SaveReason::Seam, None, &memory).unwrap();
        let destination = volatile_path(persona).unwrap();
        let original = std::fs::read(&destination).unwrap();
        let (source, selected) =
            adoption_source(&dir.path().join("legacy"), persona, "selected work");
        let plan = inspect(&source, persona).unwrap();
        assert!(
            plan.source.summary.next_action_seq
                < plan
                    .prior_destination
                    .as_ref()
                    .unwrap()
                    .summary
                    .next_action_seq
        );
        let receipt = adopt(&plan, || Ok(())).unwrap();
        assert!(!receipt.already_applied);
        assert!(!receipt.legacy_final_flush_acknowledged);
        assert_eq!(std::fs::read(&source).unwrap(), selected);
        assert_eq!(std::fs::read(&destination).unwrap(), selected);
        assert_eq!(std::fs::read(&receipt.source_archive).unwrap(), selected);
        assert_eq!(
            std::fs::read(receipt.prior_destination_archive.as_ref().unwrap()).unwrap(),
            original
        );
        assert!(adopt(&plan, || Ok(())).unwrap().already_applied);
        // The source archive is a distinct inode: an in-place future write to
        // the destination cannot alter the preserved selection.
        std::fs::write(&destination, &original).unwrap();
        assert_eq!(std::fs::read(&receipt.source_archive).unwrap(), selected);
        assert!(matches!(
            adopt(&plan, || Ok(())),
            Err(AdoptionError::DestinationChanged)
        ));
        std::fs::write(&receipt.source_archive, b"conflicting evidence").unwrap();
        assert!(matches!(
            adopt(&plan, || Ok(())),
            Err(AdoptionError::EvidenceConflict(_))
        ));
        assert_eq!(std::fs::read(&destination).unwrap(), original);
    }

    // 9f160b78: both guards are rechecked at the actual publication boundary;
    // late writes are preserved, not silently replaced by the inspected plan.
    #[test]
    fn legacy_adoption_refuses_changed_source_destination_and_offline_state() {
        use checkpoint_adoption::{adopt, inspect, AdoptionError};
        for changed_source in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let _home = crate::paths::NativeHomeOverride::install(&dir.path().join("native"));
            let persona = Uuid::new_v4();
            let (source, selected) =
                adoption_source(&dir.path().join("legacy"), persona, "selected work");
            let plan = inspect(&source, persona).unwrap();
            assert!(plan.prior_destination.is_none());
            let (other, changed) =
                adoption_source(&dir.path().join("other"), persona, "concurrent work");
            let mut checks = 0;
            let outcome = adopt(&plan, || {
                checks += 1;
                if checks == 3 {
                    std::fs::copy(
                        &other,
                        if changed_source {
                            &source
                        } else {
                            &plan.destination_path
                        },
                    )?;
                }
                Ok(())
            });
            assert_eq!(
                checks, 3,
                "test: mutation reached the pre-publication check"
            );
            if changed_source {
                assert!(matches!(outcome, Err(AdoptionError::SourceChanged)));
                assert_eq!(std::fs::read(&source).unwrap(), changed);
                assert!(!plan.destination_path.exists());
            } else {
                assert!(matches!(outcome, Err(AdoptionError::DestinationChanged)));
                assert_eq!(std::fs::read(&plan.destination_path).unwrap(), changed);
                assert_eq!(std::fs::read(&source).unwrap(), selected);
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let _home = crate::paths::NativeHomeOverride::install(&dir.path().join("native"));
        let persona = Uuid::new_v4();
        let (source, _) = adoption_source(&dir.path().join("legacy"), persona, "selected work");
        let plan = inspect(&source, persona).unwrap();
        let mut checks = 0;
        let outcome = adopt(&plan, || {
            checks += 1;
            if checks == 2 {
                Err(std::io::Error::other(
                    "core appeared before lock acquisition",
                ))
            } else {
                Ok(())
            }
        });
        assert!(matches!(outcome, Err(AdoptionError::Io(_))));
        assert_eq!(checks, 2);
        assert!(!plan.destination_path.exists());
    }

    // 9f160b78: inject a real receipt-publication IO failure after the actual
    // checkpoint rename. Retry joins preserved intent, never guesses from a
    // coincidentally matching destination, and never rewrites source bytes.
    #[test]
    fn legacy_adoption_recovers_committed_file_without_receipt() {
        use checkpoint_adoption::{adopt, inspect};
        let dir = tempfile::tempdir().unwrap();
        let _home = crate::paths::NativeHomeOverride::install(&dir.path().join("native"));
        let persona = Uuid::new_v4();
        let (source, selected) =
            adoption_source(&dir.path().join("legacy"), persona, "selected work");
        let plan = inspect(&source, persona).unwrap();
        for _ in 0..2 {
            let mut checks = 0;
            assert!(adopt(&plan, || {
                checks += 1;
                if checks == 3 {
                    Err(std::io::Error::other("core appeared before publication"))
                } else {
                    Ok(())
                }
            })
            .is_err());
            assert!(!plan.destination_path.exists());
            let evidence_root = plan
                .destination_path
                .parent()
                .unwrap()
                .join(".checkpoint-adoptions");
            let evidence = std::fs::read_dir(evidence_root)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            let files = std::fs::read_dir(evidence).unwrap().count();
            assert_eq!(
                files, 3,
                "source, plan and one staged inode; retry must not accumulate payload copies"
            );
        }
        let mut checks = 0;
        let mut blocked_receipt = None;
        let outcome = adopt(&plan, || {
            checks += 1;
            if checks == 3 {
                let archive_root = plan
                    .destination_path
                    .parent()
                    .unwrap()
                    .join(".checkpoint-adoptions");
                let evidence = std::fs::read_dir(archive_root)?.next().unwrap()?.path();
                let path = evidence.join("receipt.json");
                std::fs::create_dir(&path)?;
                blocked_receipt = Some(path);
            }
            Ok(())
        });
        assert!(outcome.is_err());
        assert_eq!(std::fs::read(&plan.destination_path).unwrap(), selected);
        std::fs::remove_dir(blocked_receipt.unwrap()).unwrap();
        let recovered = adopt(&plan, || Ok(())).unwrap();
        assert!(recovered.already_applied);
        assert!(recovered.receipt_path.is_file());
        assert_eq!(std::fs::read(&recovered.source_archive).unwrap(), selected);
        assert!(adopt(&plan, || Ok(())).unwrap().already_applied);
    }

    // 9f160b78: an explicit UUID is a declaration checked against the path,
    // not a replacement identity inserted into arbitrary JSON.
    #[test]
    fn legacy_adoption_inspection_rejects_identity_and_schema_mismatch() {
        use checkpoint_adoption::{inspect, AdoptionError};
        let dir = tempfile::tempdir().unwrap();
        let _home = crate::paths::NativeHomeOverride::install(&dir.path().join("native"));
        let persona = Uuid::new_v4();
        let (source, _) = adoption_source(&dir.path().join("legacy"), persona, "selected work");
        assert!(matches!(
            inspect(&source, Uuid::new_v4()),
            Err(AdoptionError::Invalid(_))
        ));
        std::fs::write(&source, b"{}").unwrap();
        assert!(matches!(
            inspect(&source, persona),
            Err(AdoptionError::Invalid(_))
        ));
        assert!(
            !dir.path().join("native").exists(),
            "inspection is read-only"
        );
        let (source, selected) =
            adoption_source(&dir.path().join("legacy"), persona, "valid selected work");
        let destination = volatile_path(persona).unwrap();
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(&destination, b"corrupt native checkpoint").unwrap();
        assert!(matches!(
            inspect(&source, persona),
            Err(AdoptionError::Invalid(_))
        ));
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"corrupt native checkpoint"
        );
        assert_eq!(std::fs::read(&source).unwrap(), selected);
    }

    // 9f160b78: actual cross-process lock primitive, not a simulated busy flag.
    #[test]
    fn legacy_adoption_refuses_a_checkpoint_owner_already_holding_the_lock() {
        use checkpoint_adoption::{adopt, inspect, lock_checkpoint, AdoptionError};
        let dir = tempfile::tempdir().unwrap();
        let _home = crate::paths::NativeHomeOverride::install(&dir.path().join("native"));
        let persona = Uuid::new_v4();
        let (source, _) = adoption_source(&dir.path().join("legacy"), persona, "selected work");
        let plan = inspect(&source, persona).unwrap();
        let owner = lock_checkpoint(&plan.destination_path, true).unwrap();
        assert!(
            matches!(adopt(&plan, || Ok(())), Err(AdoptionError::Io(error)) if error.kind() == fs2::lock_contended_error().kind())
        );
        assert!(!plan.destination_path.exists());
        drop(owner);
        assert!(!adopt(&plan, || Ok(())).unwrap().already_applied);
    }

    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::cognition::workspace::Decision;
    use crate::persona::engram::{ChatMessageRef, Engram, EngramKind, EngramOrigin, TrustState};
    use crate::persona::recall_metadata::{RecallMetadata, RecallMetadataRegistry};

    // What this catches (f098571b): a native launch must restore the same durable
    // working memory it saved, rather than start a second history under CWD.
    #[test]
    fn native_home_volatile_checkpoint_preserves_the_action_receipt() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let persona = Uuid::new_v4();
        let memory = super::super::working_memory::WorkingMemory::new(8);
        let receipt = "review complete: the caller keeps its room membership";
        memory.record_receipt(receipt);
        save_volatile(persona, SaveReason::Seam, None, &memory).unwrap();
        assert_eq!(
            volatile_path(persona).unwrap(),
            home.path()
                .join(".continuum/personas")
                .join(persona.to_string())
                .join("volatile.json")
        );
        let saved = load_volatile(persona)
            .expect("read the native checkpoint")
            .expect("checkpoint exists");
        assert!(
            saved.wm.last_action.is_some(),
            "the fixture must carry real work"
        );
        assert_eq!(saved.wm.last_action, memory.snapshot().last_action);
        let resumed = super::super::working_memory::WorkingMemory::new(8);
        resumed.restore(saved.wm);
        assert_eq!(
            resumed.snapshot().last_action,
            memory.snapshot().last_action
        );
    }

    // what this catches (card 1e4d8b3b): at a seam, EVERY resident's record measures HER
    // tree — including a file she just created, which is untracked until staged (the
    // "resume the edit, do not re-derive it" case the record exists for; a `-uno` status
    // would drop it, Cormac on #4407). The first cut spent one 400 ms budget sequentially,
    // so on the M5's first deploy seam only the first resident measured and the rest wrote
    // Unmeasured or Periodic-only. Three residents at three cold roots, one seam.
    #[tokio::test]
    async fn every_resident_at_a_seam_measures_her_own_tree_including_untracked_files() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let registry = PersonaWorkspaceRegistry::new();
        let mut residents = Vec::new();
        for i in 0..3 {
            let persona = Uuid::new_v4();
            registry
                .register_from_cfg(cfg_for(persona))
                .expect("test: resident registers");
            let root = home.path().join(format!("work-{i}"));
            std::fs::create_dir_all(&root).unwrap();
            let init = std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(&root)
                .status()
                .expect("git is on PATH");
            assert!(init.success());
            std::fs::write(root.join(format!("new_{i}.py")), "x = 1\n").unwrap();
            note_acting_root_in(persona, None, Some(root.clone()));
            residents.push((persona, root, format!("new_{i}.py")));
        }
        let outcomes = registry.flush_volatile_all();
        assert_eq!(outcomes.len(), 3);
        for (id, r) in &outcomes {
            assert!(r.is_ok(), "{id}: {r:?}");
        }
        for (persona, root, new_file) in &residents {
            let saved = load_volatile(*persona).unwrap().expect("a seam record");
            let handoff = saved
                .wm
                .handoff
                .expect("she stands at a root, so she hands off");
            match handoff.staged.expect("rooted = measured or named") {
                crate::cognition::handoff::Staged::Measured {
                    root: r,
                    dirty_paths,
                    ..
                } => {
                    assert_eq!(r, root.to_string_lossy());
                    assert_eq!(dirty_paths, vec![new_file.clone()], "untracked file kept");
                }
                other => panic!("{persona}: {other:?}"),
            }
            note_acting_root_in(*persona, None, None);
        }
    }

    // What this catches (#3918): replaced cycles and BOTH real eval fork paths
    // used to keep detached writers for the resident's UUID. They must neither
    // restore her scratchpad nor publish over her current action receipt.
    // what this catches: a turn in her mind room running on her LIVE cycle, sharing its
    // working memory with her public turns (PRIVACY-OF-THOUGHT.md §4, build step 2), or her
    // mind cycle reaching the volatile checkpoint and so landing on disk in plaintext.
    #[tokio::test]
    async fn her_mind_room_runs_on_its_own_cycle_never_checkpointed() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let registry = PersonaWorkspaceRegistry::new();
        let her = Uuid::new_v4();
        let live = registry.register_from_cfg(cfg_for(her)).expect("live cycle");
        let mind_room = crate::persona::mind_room::mind_room_id(her);
        let mind = registry.cycle_for_room(&her, mind_room).expect("her mind cycle");
        assert!(!Arc::ptr_eq(&live, &mind), "her mind room runs on its own cycle");
        assert!(
            Arc::ptr_eq(&mind, &registry.cycle_for_room(&her, mind_room).expect("again")),
            "one mind cycle, reused"
        );
        assert!(
            Arc::ptr_eq(&live, &registry.cycle_for_room(&her, Uuid::new_v4()).expect("public")),
            "every other room runs on her live cycle"
        );
        assert_eq!(registry.checkpoint_volatile_all().len(), 1, "only her live cycle is checkpointed");
    }

    #[tokio::test]
    async fn resident_checkpoint_excludes_replaced_cycles_and_eval_forks() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        let old_cycle = registry
            .register_from_cfg(cfg_for(persona))
            .expect("test: resident checkpoint is readable");
        let old_memory = Arc::clone(&registry.cycles.lock()[&persona].working_memory);
        old_memory.record_receipt("previous resident action");
        assert!(registry.checkpoint_volatile_all().remove(0).1.is_ok());
        assert!(registry.flush_volatile_all().remove(0).1.is_ok());
        old_memory.record_receipt("work after an ordinary explicit save");
        assert!(registry.checkpoint_volatile_all().remove(0).1.is_ok());
        assert_eq!(
            load_volatile(persona).unwrap().unwrap().wm.last_action,
            old_memory.snapshot().last_action
        );

        let current = registry
            .register_from_cfg(cfg_for(persona))
            .expect("test: resident checkpoint is readable");
        let current_memory = Arc::clone(&registry.cycles.lock()[&persona].working_memory);
        // cfg_for uses synchronous recall: restore authority must not depend on
        // that scheduling flag, or these residents still wake blank.
        assert_eq!(
            current_memory.snapshot().last_action,
            old_memory.snapshot().last_action
        );
        current_memory.record_receipt("current resident finished the review");
        let expected = current_memory.snapshot().last_action;
        let fork = registry
            .fork_eval_cycle(&persona, false, None, false)
            .unwrap();
        let adapter_fork = registry
            .fork_eval_cycle_with_adapter(
                &persona,
                cfg_for(persona).adapter,
                32_768,
                false,
                None,
                false,
                Vec::new(),
            )
            .unwrap();
        for copy in [&fork, &adapter_fork] {
            let observed = copy.run("teammate: report the review status").await;
            assert!(!observed.perceived().contains("previous resident action"));
            assert!(!observed
                .perceived()
                .contains("work after an ordinary explicit save"));
        }
        old_memory.record_receipt("obsolete cycle must not publish this");
        tokio::task::yield_now().await; // the former saver's immediate first tick
        assert!(registry.stop_volatile_checkpoints().remove(0).1.is_ok());
        assert_eq!(
            load_volatile(persona).unwrap().unwrap().wm.last_action,
            expected
        );
        let checkpoint = std::fs::read(volatile_path(persona).unwrap()).unwrap();
        current_memory.record_receipt("late mutation after shutdown boundary");
        assert!(registry.checkpoint_volatile_all().is_empty());
        assert_eq!(
            std::fs::read(volatile_path(persona).unwrap()).unwrap(),
            checkpoint
        );
        assert!(!Arc::ptr_eq(&old_cycle, &current));
        drop((old_cycle, current, fork, adapter_fork, registry));
        tokio::task::yield_now().await;
        assert_eq!(
            std::fs::read(volatile_path(persona).unwrap()).unwrap(),
            checkpoint
        );
    }

    // What this catches (#3918): a generic/deferred cycle used to read and write
    // the resident checkpoint solely because its recall scheduler was deferred.
    #[tokio::test]
    async fn unregistered_cycle_has_no_persistence_authority() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let persona = Uuid::new_v4();
        let memory = WorkingMemory::new(8);
        memory.record_receipt("resident-only scratchpad");
        save_volatile(persona, SaveReason::Seam, None, &memory).unwrap();
        let before = std::fs::read(volatile_path(persona).unwrap()).unwrap();
        let mut cfg = cfg_for(persona);
        cfg.defer_recall = true;
        let cycle = build_workspace_cycle(cfg);
        let observed = cycle.run("teammate: status?").await;
        assert!(!observed.perceived().contains("resident-only scratchpad"));
        tokio::task::yield_now().await;
        assert_eq!(
            std::fs::read(volatile_path(persona).unwrap()).unwrap(),
            before
        );
    }

    // What this catches (803011ea): one resident's storage failure must be
    // reported without dropping other residents' checkpoints or claiming success.
    #[tokio::test]
    async fn resident_checkpoint_reports_io_failure_and_saves_other_residents() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let registry = PersonaWorkspaceRegistry::new();
        let failed = Uuid::new_v4();
        let healthy = Uuid::new_v4();
        for id in [failed, healthy] {
            registry
                .register_from_cfg(cfg_for(id))
                .expect("test: resident checkpoint is readable");
            registry.cycles.lock()[&id]
                .working_memory
                .record_receipt("completed real work");
        }
        let path = volatile_path(failed).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Deterministic on Unix and Windows: a directory cannot be overwritten
        // by the temporary checkpoint file, regardless of user permissions.
        std::fs::create_dir(path.with_extension("json.tmp")).unwrap();
        let outcomes = registry.flush_volatile_all();
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes
            .iter()
            .find(|(id, _)| *id == failed)
            .unwrap()
            .1
            .is_err());
        assert!(outcomes
            .iter()
            .find(|(id, _)| *id == healthy)
            .unwrap()
            .1
            .is_ok());
        assert!(load_volatile(healthy)
            .unwrap()
            .unwrap()
            .wm
            .last_action
            .is_some());
    }

    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;

        // What this catches (#3918): an admitted old snapshot must finish before
        // final acknowledgement, without holding the resident lookup lock.
        #[tokio::test]
        async fn final_checkpoint_drains_admitted_periodic_writer_before_ack(
        ) -> Result<(), Box<dyn std::error::Error>> {
            let home = Arc::new(tempfile::tempdir()?);
            let _native = crate::paths::NativeHomeOverride::install(home.path());
            let registry = Arc::new(PersonaWorkspaceRegistry::new());
            let persona = Uuid::new_v4();
            registry
                .register_from_cfg(cfg_for(persona))
                .expect("test: resident checkpoint is readable");
            registry.cycles.lock()[&persona]
                .working_memory
                .record_receipt("old resident action");
            let (admitted, ready) = tokio::sync::oneshot::channel();
            let (resume, paused) = std::sync::mpsc::channel();
            *registry.checkpoint_pause.lock() = Some(CheckpointPause {
                admitted,
                resume: paused,
            });
            let owner = Arc::clone(&registry);
            let root = Arc::clone(&home);
            let periodic = tokio::task::spawn_blocking(move || {
                let _native = crate::paths::NativeHomeOverride::install(root.path());
                owner.checkpoint_volatile_all()
            });
            timeout(Duration::from_secs(5), ready).await??;
            let owner = Arc::clone(&registry);
            let root = Arc::clone(&home);
            let replacement = tokio::task::spawn_blocking(move || {
                let _native = crate::paths::NativeHomeOverride::install(root.path());
                let current = owner
                    .register_from_cfg(cfg_for(persona))
                    .expect("test: resident checkpoint is readable");
                assert!(Arc::ptr_eq(&current, &owner.get(&persona).unwrap()));
                let memory = Arc::clone(&owner.cycles.lock()[&persona].working_memory);
                memory.record_receipt("replacement completed the review");
                memory
            });
            let current = timeout(Duration::from_secs(5), replacement).await??;
            let expected = current.snapshot().last_action;
            let (starting, started) = tokio::sync::oneshot::channel();
            let owner = Arc::clone(&registry);
            let root = Arc::clone(&home);
            let mut final_save = tokio::task::spawn_blocking(move || {
                let _native = crate::paths::NativeHomeOverride::install(root.path());
                starting.send(()).unwrap();
                owner.stop_volatile_checkpoints()
            });
            timeout(Duration::from_secs(5), started).await??;
            assert!(
                timeout(Duration::from_millis(100), &mut final_save)
                    .await
                    .is_err(),
                "no final ACK while the admitted writer is paused"
            );
            assert!(
                registry.checkpoint_volatile_all().is_empty(),
                "busy periodic passes coalesce"
            );
            assert!(
                registry.get(&persona).is_some(),
                "lookup remains available during IO"
            );
            resume.send(()).unwrap();
            timeout(Duration::from_secs(5), periodic)
                .await??
                .remove(0)
                .1?;
            timeout(Duration::from_secs(5), final_save)
                .await??
                .remove(0)
                .1?;
            assert_eq!(
                load_volatile(persona).unwrap().unwrap().wm.last_action,
                expected
            );
            let checkpoint = std::fs::read(volatile_path(persona).unwrap()).unwrap();
            current.record_receipt("late action after final ACK");
            assert!(registry.checkpoint_volatile_all().is_empty());
            assert_eq!(
                std::fs::read(volatile_path(persona).unwrap()).unwrap(),
                checkpoint
            );
            Ok(())
        }
    }

    fn seed_admission(now_ms: u64) -> Arc<AdmissionState> {
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        let id = Uuid::new_v4();
        let engram = Engram {
            context_id: None,
            id,
            kind: EngramKind::Episodic,
            content: "the deploy pipeline went green after the 4pm fix".to_string(),
            origin: EngramOrigin::Chat(ChatMessageRef {
                message_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: Uuid::new_v4(),
                posted_at_ms: now_ms,
                content_hash: "h".to_string(),
            }),
            recall_keys: Vec::new(),
            admitted_at_ms: now_ms,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        };
        state.push_for_test(engram);
        recall_meta.admit(
            id,
            RecallMetadata {
                salience: 0.7,
                access_count: 0,
                last_accessed_ms: 0,
                protected_until_ms: 0,
                last_decayed_ms: now_ms,
            },
        );
        state
    }

    // Serving-edge rehome must preserve a resident's binding when its assignment is corrupt.
    #[tokio::test]
    async fn rehome_requires_a_readable_local_assignment() {
        let root = tempfile::tempdir().expect("root");
        let _native = crate::paths::NativeHomeOverride::install(root.path());
        let registry = PersonaWorkspaceRegistry::new();
        let id = Uuid::new_v4();
        let cycle = registry.register_from_cfg(cfg_for(id)).expect("resident");
        let (original, original_model) = cycle.current_model_route().expect("binding");
        let airc = crate::context::citizen_home_path(root.path(), crate::identity::IdentityKind::Persona, None, "Ivar");
        let home = crate::persona::home::PersonaHome::from_root(airc.parent().expect("parent").to_path_buf());
        let remote = crate::persona::PersonaModelOverride::new_remote("remote/model", None, 1, Uuid::new_v4().to_string());
        remote.write(&home).expect("remote override");
        let replacement: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        for corrupt in [false, true] {
            if corrupt { std::fs::write(home.model_override_json(), b"{").expect("corrupt"); }
            assert_eq!(registry.re_home_all_at(root.path(), replacement.clone(), Some("new/local".into()), 4096), 0);
            let (actual, model) = cycle.current_model_route().expect("preserved");
            assert!(Arc::ptr_eq(&original, &actual));
            assert_eq!(model, original_model);
        }
        crate::persona::PersonaModelOverride::new("local/model", None, 2).write(&home).expect("local override");
        assert_eq!(registry.re_home_all_at(root.path(), replacement.clone(), Some("new/local".into()), 4096), 1);
        let (actual, model) = cycle.current_model_route().expect("rebound");
        assert!(Arc::ptr_eq(&replacement, &actual));
        assert_eq!(model.as_deref(), Some("new/local"));
    }

    fn cfg_for(persona_id: Uuid) -> PersonaBrainConfig {
        PersonaBrainConfig {
            persona_id,
            persona_name: "Ivar".to_string(),
            system_prompt: "You are Ivar, an engineer on the grid.".to_string(),
            admission: seed_admission(1_000_000_000),
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            capacity: None,
            grounding_sources: Vec::new(),
            embedder: None,
            tool_executor: None,
            experience: None,
            context_window: crate::inference::planning::MIN_SERVE_CTX,
            // Synchronous recall in the harness: these tests assert recall bids in
            // phase 1 on the same tick, which a deferred (cold-start) worker can't
            // satisfy. Deferral is a live-path concern, tested in deferred_faculty.rs.
            defer_recall: false,
            defer_grounding: false,
            // Harness/test cycles keep recall (they assert on its bids); suppression is a
            // benchmark-reproducibility knob set only by the eval fork.
            suppress_recall: false,
        }
    }

    // what this catches: the assembled cycle runs a FULL persona mind end-to-end —
    // recall (hippocampus) bids in phase 1, deliberation (real adapter) decides in
    // phase 2 over that context — and yields a Decision. This is the production
    // assembly path; swap the adapter for LlamaCppAdapter and it's a live persona.
    #[tokio::test]
    async fn assembled_cycle_produces_a_decision() {
        let persona = Uuid::new_v4();
        let cycle = build_workspace_cycle(cfg_for(persona));
        let ws = cycle.run("teammate: what's the deploy status?").await;
        // The mind reached a participation verdict (heuristic adapter → Speak).
        assert!(matches!(ws.decision(), Some(Decision::Speak { .. })));
    }

    // what this catches: #396 — resolve_persona is the ONE formal identity boundary
    // for eval/benchmark. It must (a) resolve a full UUID, an 8-char short-id, and a
    // case-insensitive persona NAME to the typed id; (b) pass a well-formed full UUID
    // through WITHOUT a liveness check (race safety — a persona still assembling isn't
    // false-rejected; the fork wait absorbs it); (c) fail LOUD on garbage, naming the
    // online roster — never a silent guess. The loose-String id that skipped all of
    // this is exactly what fed a dead reference to a doomed 10s eval.
    #[test]
    fn resolve_persona_takes_uuid_short_id_or_name_and_fails_loud() {
        let registry = PersonaWorkspaceRegistry::new();
        let asha = Uuid::new_v4();
        let atlas = Uuid::new_v4();
        let mut asha_cfg = cfg_for(asha);
        asha_cfg.persona_name = "Asha".to_string();
        let mut atlas_cfg = cfg_for(atlas);
        atlas_cfg.persona_name = "Atlas".to_string();
        registry
            .register_from_cfg(asha_cfg)
            .expect("test: resident checkpoint is readable");
        registry
            .register_from_cfg(atlas_cfg)
            .expect("test: resident checkpoint is readable");

        // full UUID
        assert_eq!(
            registry.resolve_persona(&asha.to_string().into()).unwrap(),
            PeerId::from_uuid(asha)
        );
        // 8-char short-id prefix
        assert_eq!(
            registry
                .resolve_persona(&asha.to_string()[..8].into())
                .unwrap(),
            PeerId::from_uuid(asha)
        );
        // case-insensitive name
        assert_eq!(
            registry.resolve_persona(&"atlas".into()).unwrap(),
            PeerId::from_uuid(atlas)
        );
        assert_eq!(
            registry.resolve_persona(&"ASHA".into()).unwrap(),
            PeerId::from_uuid(asha)
        );

        // (b) a well-formed but NON-live full UUID passes through — race safety. The
        // caller's fork wait, not this boundary, decides liveness.
        let ghost = Uuid::new_v4();
        assert_eq!(
            registry.resolve_persona(&ghost.to_string().into()).unwrap(),
            PeerId::from_uuid(ghost)
        );

        // (c) garbage fails loud AND names the roster so the operator can fix it.
        let err = registry.resolve_persona(&"general".into()).unwrap_err();
        assert!(
            err.contains("Asha") && err.contains("Atlas"),
            "roster hint missing: {err}"
        );
    }

    // what this catches: ONE cycle per persona — get_or_build is idempotent and
    // returns the SAME Arc, so a persona's continuous mind is reused across every
    // room it services (the "one soul, many rooms" / anti-Severance invariant).
    #[tokio::test]
    async fn registry_keeps_one_mind_per_persona() {
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        let first = registry
            .get_or_build(cfg_for(persona))
            .expect("test: resident checkpoint is readable");
        let second = registry
            .get_or_build(cfg_for(persona))
            .expect("test: resident checkpoint is readable");
        assert!(
            Arc::ptr_eq(&first, &second),
            "same persona must resolve to the SAME mind across rooms — not severed per-room"
        );
        assert_eq!(registry.len(), 1);
        // A different persona is a different mind.
        let _ = registry
            .get_or_build(cfg_for(Uuid::new_v4()))
            .expect("test: resident checkpoint is readable");
        assert_eq!(registry.len(), 2);
    }

    // what this catches: RESPAWN must replace the mind, not keep the stale one.
    // A persona can respawn in-process (node resilience) with a fresh admission +
    // adapter; the supervisor uses register() (overwrite), so get() returns the
    // NEW cycle, not the prior lifetime's orphaned one. (get_or_build would have
    // discarded the fresh config — the bug this guards.)
    #[tokio::test]
    async fn register_overwrites_on_respawn() {
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        // register_from_cfg IS the production overwrite path (supervisor.rs spawn);
        // it builds + caches and returns the fresh Arc.
        let first = registry
            .register_from_cfg(cfg_for(persona))
            .expect("test: resident checkpoint is readable");
        let second = registry
            .register_from_cfg(cfg_for(persona))
            .expect("test: resident checkpoint is readable");
        let got = registry.get(&persona).expect("registered");
        assert!(
            Arc::ptr_eq(&got, &second),
            "respawn must resolve to the FRESH mind"
        );
        assert!(
            !Arc::ptr_eq(&got, &first),
            "the prior lifetime's mind is replaced"
        );
        assert_eq!(registry.len(), 1);
    }

    // 9f160b78: unreadable existing memory is not permission to register a blank
    // replacement. Both the live cycle and its fork template survive refusal.
    #[tokio::test(flavor = "current_thread")]
    async fn failed_checkpoint_load_preserves_the_registered_resident() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        assert!(load_volatile(persona).unwrap().is_none());
        let current = registry.register_from_cfg(cfg_for(persona)).unwrap();
        let path = volatile_path(persona).unwrap();
        std::fs::write(&path, br#"{"wm":{},"own_speech":[]}"#).unwrap();
        let mut replacement = cfg_for(persona);
        replacement.persona_name = "must not replace the resident".into();
        let error = match registry.register_from_cfg(replacement) {
            Err(error) => error,
            Ok(_) => panic!("test: malformed checkpoint must refuse registration"),
        };
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(Arc::ptr_eq(&current, &registry.get(&persona).unwrap()));
        assert_eq!(registry.templates.lock()[&persona].persona_name, "Ivar");
        assert!(Arc::ptr_eq(
            &current,
            &registry.get_or_build(cfg_for(persona)).unwrap()
        ));
        assert_eq!(
            std::fs::read(&path).unwrap(),
            br#"{"wm":{},"own_speech":[]}"#
        );
    }

    // 9f160b78: cold lazy registration must preserve lock/read/schema errors;
    // none may insert a cycle or fork template as if no file had existed.
    #[tokio::test(flavor = "current_thread")]
    async fn resident_registration_refuses_checkpoint_and_lock_errors() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let registry = PersonaWorkspaceRegistry::new();
        for fault in ["schema", "read", "lock"] {
            let persona = Uuid::new_v4();
            let path = volatile_path(persona).unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            match fault {
                "schema" => std::fs::write(&path, b"not a checkpoint").unwrap(),
                "read" => std::fs::create_dir(&path).unwrap(),
                "lock" => {
                    std::fs::create_dir(path.parent().unwrap().join(".volatile.lock")).unwrap()
                }
                _ => unreachable!("test: fixed fault cases"),
            }
            assert!(
                registry.get_or_build(cfg_for(persona)).is_err(),
                "{fault} must refuse registration"
            );
            assert!(registry.get(&persona).is_none());
            assert!(!registry.templates.lock().contains_key(&persona));
        }
        assert_eq!(registry.len(), 0);
    }

    // THE LIVE BRING-UP: a persona's mind thinks with the REAL local model.
    // Runs the EXACT production assembly path (build_workspace_cycle → RecallFaculty
    // + LlmDeliberationFaculty) against the real LlamaCppAdapter (qwen3.5-4b-code-
    // forged on disk), over a real consolidated burst, and prints Ivar's actual
    // words. #[ignore] — needs a local GGUF + Metal. Run:
    //   CARGO_TARGET_DIR=$HOME/.continuum/cache/cargo-target \
    //   cargo test -p continuum-core --features metal,accelerate \
    //     cognition::persona_workspace::tests::ivar_thinks_with_the_real_model \
    //     -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "needs local GGUF + Metal; run with --ignored --nocapture"]
    async fn ivar_thinks_with_the_real_model() {
        use crate::ai::adapter::AIProviderAdapter;
        use crate::inference::llamacpp_adapter::LlamaCppAdapter;

        crate::model_registry::init_global().expect("model_registry init");
        // context_length MUST be set explicitly (the backend refuses to silently
        // fall back to n_ctx_train — the 2026-04 Metal-KV-blowup guard). new()
        // doesn't set it; production uses for_persona(profile). 8192 fits Metal.
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(
            LlamaCppAdapter::new()
                .with_context_length(8192)
                .with_n_seq_max(1),
        );
        eprintln!(
            "[live] adapter={} default_model={}",
            adapter.name(),
            adapter.default_model()
        );

        use crate::cognition::llm_deliberation_faculty::LlmDeliberationFaculty;
        use crate::cognition::recall_faculty::RecallFaculty;
        use crate::cognition::workspace::{Faculty, SalienceArbiter, Workspace, WorkspaceCaptureSink, WorkspaceCycle, WorkspaceTrace};

        let persona = Uuid::new_v4();
        let admission = seed_admission(1_718_600_000_000);
        let system_prompt = "You are Ivar, a thoughtful engineer and a citizen on the grid. \
            You speak concisely, and only when you have something worth adding.";

        // Assemble the faculties directly (mirrors build_workspace_cycle) so we
        // keep a typed handle to the deliberation faculty — to introspect the
        // EXACT prompt it feeds the model.
        let delib = Arc::new(LlmDeliberationFaculty::new(
            persona,
            "Ivar",
            system_prompt,
            adapter,
        ));
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(RecallFaculty::new(persona, admission)),
            delib.clone(),
        ];

        // The EXISTING capture harness: a WorkspaceCaptureSink records every phase
        // of the tick (all bids incl. losers, the assembled context, the decision)
        // so we can diagnose cognition at any phase — record + recreate, not guess.
        #[derive(Default)]
        struct CapturingSink(Mutex<Vec<WorkspaceTrace>>);
        impl WorkspaceCaptureSink for CapturingSink {
            fn record(&self, t: &WorkspaceTrace) {
                self.0.lock().push(t.clone());
            }
        }
        let sink = Arc::new(CapturingSink::default());

        let cycle = WorkspaceCycle::new(
            faculties,
            Arc::new(SalienceArbiter),
            DEFAULT_WORKSPACE_CAPACITY,
        )
        .with_capture(sink.clone());

        let burst = "general room:\n\
            Joel: morning all\n\
            teammate: the deploy from yesterday — did we ever figure out what broke it?\n\
            teammate: ivar you were looking at it right?";

        let ws = cycle.run(burst).await;

        // ---- Glass box: diagnose cognition at EVERY phase ----
        let trace = sink.0.lock().pop().expect("a tick was recorded");
        eprintln!("\n================ COGNITION TRACE ================");
        eprintln!("WORLD-STATE (the burst):\n{}\n", trace.world_state);
        eprintln!("PHASE 1 — perception bids (the full competition, incl. losers):");
        for b in &trace.bids {
            eprintln!(
                "  [{:<12} s={:.2}] {}  ({})",
                b.faculty.as_str(),
                b.salience,
                b.content.replace('\n', " / "),
                b.reasoning
            );
        }
        eprintln!("\nASSEMBLED CONTEXT the decider saw (context_broadcast = the RAG):");
        for c in &trace.context_broadcast {
            eprintln!(
                "  [{}] {}",
                c.faculty.as_str(),
                c.content.replace('\n', " / ")
            );
        }

        // ---- EXACTLY what the LLM was fed (reconstruct the pre-deliberation ws) ----
        // Route through the constructor (opaque single-turn burst) so this can't
        // drift as Workspace gains fields; then graft on the recorded context
        // broadcast the decider actually saw. The trace does not yet record the
        // turn's directedness or self-initiation; reconstruct as ambient /
        // message-driven (the live defaults). TODO(#9): carry directedness on the
        // trace so a replayed directed turn shows the silence escape withheld as it was.
        let mut context_ws = Workspace::in_room(burst.to_string(), trace.room_id);
        context_ws.broadcast = trace.context_broadcast.clone();
        let view = delib.prompt_view(&context_ws);
        eprintln!("\n--------------- WHAT THE LLM WAS FED ---------------");
        eprintln!("[SYSTEM]\n{}\n", view.system);
        eprintln!("[CONVERSATION]\n{}", view.user_text());

        eprintln!("\n--------------- Ivar's DECISION ---------------");
        match ws.decision() {
            Some(Decision::Speak { text }) => eprintln!("Ivar SPEAKS:\n{text}"),
            Some(Decision::RaiseUnprompted { text }) => eprintln!("Ivar RAISES:\n{text}"),
            Some(Decision::Act { calls, intent }) => {
                eprintln!("Ivar ACTS ({} call(s)) — intent: {intent}", calls.len())
            }
            Some(Decision::Pass { .. }) | None => eprintln!("Ivar chose silence (PASS)."),
        }
        eprintln!("=================================================\n");

        assert!(
            ws.decision().is_some(),
            "the persona's mind must reach a decision through the real model"
        );
    }

    // A constructible RagSource whose delivery is never exercised — these tests
    // assert the deferrability CLASSIFICATION on GroundingSource, not delivery.
    struct ClassifyStub;
    #[async_trait::async_trait]
    impl RagSource for ClassifyStub {
        fn source_id(&self) -> &'static str {
            "classify-stub"
        }

        fn expand_command(&self) -> Option<&'static str> {
            // Test/stub source — nothing further to fetch.
            None
        }

        /// Test/stub source — floorless, so it never encodes a production floor.
        fn floor_tokens(&self) -> u32 {
            0
        }
        async fn deliver(
            &self,
            _ctx: &crate::persona::rag_budget::RagContext,
            _budget: u32,
            resolution: crate::persona::rag_budget::ResolutionPreference,
        ) -> crate::persona::rag_budget::RagDelivery {
            crate::persona::rag_budget::RagDelivery {
                source_id: "classify-stub".to_string(),
                items: Vec::new(),
                tokens_used: 0,
                continuation: None,
                resolution_used: resolution,
            }
        }
        async fn deliver_continuation(
            &self,
            _ctx: &crate::persona::rag_budget::RagContext,
            _cursor: crate::persona::rag_budget::ContinuationCursor,
            _budget: u32,
        ) -> Option<crate::persona::rag_budget::RagDelivery> {
            None
        }
    }

    // what this catches: deferrability is ORTHOGONAL to salience, and the SAFE
    // DEFAULT is synchronous. framing() is ColdStartCritical — a fresh grounding
    // source runs ON the loop until deliberately opted off, so doctrine (the
    // participation gate) can never silently start serving a cold-start `None`
    // (speaking in a room it shouldn't on turn one). `.defer_tolerant()` flips
    // ONLY the schedule axis, leaving StandingFraming salience untouched. If a
    // future edit folds salience into the deferrability builder, or flips the
    // default to DeferTolerant, this fails — guarding the two-independent-axes
    // invariant the live supervisor wiring depends on.
    #[test]
    fn deferrability_is_orthogonal_to_salience_and_defaults_to_synchronous() {
        let s: Arc<dyn RagSource> = Arc::new(ClassifyStub);

        let critical = GroundingSource::framing(Arc::clone(&s));
        assert_eq!(
            critical.deferrability,
            Deferrability::ColdStartCritical,
            "safe default: a framing source runs synchronously until opted off"
        );
        assert_eq!(critical.policy, SaliencePolicy::StandingFraming);

        let tolerant = GroundingSource::framing(Arc::clone(&s)).defer_tolerant();
        assert_eq!(
            tolerant.deferrability,
            Deferrability::DeferTolerant,
            "opt-in flips ONLY the schedule axis"
        );
        assert_eq!(
            tolerant.policy,
            SaliencePolicy::StandingFraming,
            "orthogonal: deferrability must not touch the salience policy"
        );

        // retrieved() (the other constructor) also defaults to the safe side.
        let retrieved = GroundingSource::retrieved(Arc::clone(&s));
        assert_eq!(retrieved.deferrability, Deferrability::ColdStartCritical);
    }

    // what this catches: capability consistency on the eval fork — grounding
    // marked requires_hands (workspace-map: "drill in with code/list and
    // code/tree") must be DROPPED from a tools-stripped fork and KEPT on a
    // handed fork. Glass-boxed 2026-07-10: every captured spoken-exam prompt
    // (484/484) carried the workspace-map telling her to use tools that were
    // stripped for the exam — the RAG lying to her about her own affordances.
    #[tokio::test]
    async fn eval_fork_drops_hands_describing_grounding_when_tools_stripped() {
        const HANDS_MARKER: &str = "drill in with code/list and code/tree";
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        // SlowGrounding's fixed roster line stands in for capability-NEUTRAL
        // grounding; the hands-describing source is its own tiny stub.
        struct HandsMap;
        #[async_trait::async_trait]
        impl RagSource for HandsMap {
            fn source_id(&self) -> &'static str {
                "workspace-map"
            }

            fn expand_command(&self) -> Option<&'static str> {
                // Test/stub source — nothing further to fetch.
                None
            }

            /// Test/stub source — floorless, so it never encodes a production floor.
            fn floor_tokens(&self) -> u32 {
                0
            }
            async fn deliver(
                &self,
                _ctx: &crate::persona::rag_budget::RagContext,
                _budget: u32,
                resolution: crate::persona::rag_budget::ResolutionPreference,
            ) -> crate::persona::rag_budget::RagDelivery {
                crate::persona::rag_budget::RagDelivery {
                    source_id: "workspace-map".to_string(),
                    items: vec![crate::persona::rag_budget::RagItem {
                        content: format!("workspace layout — {HANDS_MARKER}"),
                        tokens: 12,
                        metadata: serde_json::Value::Null,
                    }],
                    tokens_used: 12,
                    continuation: None,
                    resolution_used: resolution,
                }
            }
            async fn deliver_continuation(
                &self,
                _ctx: &crate::persona::rag_budget::RagContext,
                _cursor: crate::persona::rag_budget::ContinuationCursor,
                _budget: u32,
            ) -> Option<crate::persona::rag_budget::RagDelivery> {
                None
            }
        }
        let mut cfg = cfg_for(persona);
        cfg.grounding_sources = vec![
            GroundingSource::framing(Arc::new(HandsMap)).requires_hands(),
            // A second, hands-agnostic source so the assertion below is about the
            // hands-describing one being stripped, not about grounding being empty.
            // Its gate starts open, so it delivers straight through here.
            GroundingSource::framing(Arc::new(GatedGrounding::new())),
        ];
        registry
            .register_from_cfg(cfg)
            .expect("test: resident checkpoint is readable");

        // Spoken exam (no hands): the workspace-map must not reach her mind.
        let spoken = registry
            .fork_eval_cycle(&persona, false, None, false)
            .expect("template retained");
        let ws = spoken.run("proctor: reverse a string in Rust").await;
        assert!(
            !ws.perceived().contains(HANDS_MARKER),
            "tool-stripped fork must NOT perceive hands-describing grounding:\n{}",
            ws.perceived()
        );
        // Neutral grounding survives the filter — only hands-claims are dropped.
        assert!(
            ws.perceived().contains("roster:"),
            "capability-neutral grounding must survive the hands filter:\n{}",
            ws.perceived()
        );

        // Handed exam: the map is real guidance and must be delivered.
        let handed = registry
            .fork_eval_cycle(&persona, true, None, false)
            .expect("template retained");
        let ws = handed.run("proctor: reverse a string in Rust").await;
        assert!(
            ws.perceived().contains(HANDS_MARKER),
            "handed fork must keep the workspace-map:\n{}",
            ws.perceived()
        );
    }

    // what this catches (#207): the reproducible-absolute benchmark knob. A default fork keeps
    // the recall faculty (memories intact — the natural persona), so its ABSOLUTE score can
    // drift as her durable engram store grows between runs; a `suppress_recall` fork OMITS
    // recall entirely (system + task + grounding only), pinning the absolute baseline. The two
    // forks must differ by exactly the recall faculty — the whole point of the knob.
    #[tokio::test]
    async fn suppress_recall_omits_the_recall_faculty_from_the_eval_fork() {
        use crate::cognition::workspace::FacultyId;
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        registry
            .register_from_cfg(cfg_for(persona))
            .expect("test: resident checkpoint is readable");

        let with_recall = registry
            .fork_eval_cycle(&persona, false, None, false)
            .expect("template retained");
        assert!(
            with_recall.faculty_ids().contains(&FacultyId::Recall),
            "default fork must keep recall (memories intact): {:?}",
            with_recall.faculty_ids()
        );

        let suppressed = registry
            .fork_eval_cycle(&persona, false, None, true)
            .expect("template retained");
        assert!(
            !suppressed.faculty_ids().contains(&FacultyId::Recall),
            "suppress_recall fork must OMIT recall for a reproducible absolute baseline: {:?}",
            suppressed.faculty_ids()
        );
    }

    /// A grounding RagSource whose `deliver()` the TEST holds the door on: it
    /// cannot return until the gate is opened. Models the real I/O cost (roster
    /// query / workspace-map scan) the slice moves off the hot path — but as a
    /// *control-flow* hold rather than a `sleep`, so "is this deliver on the
    /// critical path?" is answered by whether the tick completes, never by a
    /// wall-clock threshold a loaded CI runner can forge. Exercised through the
    /// LIVE grounding path (GroundingSource → RagSourceFaculty → DeferredFaculty),
    /// not a hand-built faculty.
    struct GatedGrounding {
        /// `false` = held shut. Every `deliver()` parks here until it flips true.
        gate: watch::Sender<bool>,
        /// Bumped on ENTRY to `deliver()`, before parking — so the test can wait
        /// for "the tick reached the source" without polling a clock.
        entries: AtomicUsize,
        /// Fires on each entry, so that wait is event-driven (system law: no
        /// condition-polling).
        entered: Notify,
    }

    impl GatedGrounding {
        /// Starts OPEN — the cold tick must be able to complete.
        fn new() -> Self {
            let (gate, _rx) = watch::channel(true);
            Self {
                gate,
                entries: AtomicUsize::new(0),
                entered: Notify::new(),
            }
        }
        /// `send_replace`, not `send`: with no receiver parked at this instant
        /// `send` fails and silently leaves the value UNCHANGED, which quietly
        /// turns the whole gate into a no-op. Same reason `DashboardCaptureSink`
        /// publishes with `send_replace`.
        fn open_gate(&self) {
            self.gate.send_replace(true);
        }
        fn shut_gate(&self) {
            self.gate.send_replace(false);
        }
        fn deliver_entries(&self) -> usize {
            self.entries.load(AtomicOrdering::SeqCst)
        }
        /// Resolves once `deliver()` has been ENTERED beyond `baseline`. Loops on
        /// the counter because a stale `notify_one` permit from an earlier entry
        /// may wake us spuriously.
        async fn entered_beyond(&self, baseline: usize) {
            while self.deliver_entries() == baseline {
                self.entered.notified().await;
            }
        }
    }

    #[async_trait::async_trait]
    impl RagSource for GatedGrounding {
        fn source_id(&self) -> &'static str {
            "gated-grounding"
        }

        fn expand_command(&self) -> Option<&'static str> {
            // Test/stub source — nothing further to fetch.
            None
        }

        /// Test/stub source — floorless, so it never encodes a production floor.
        fn floor_tokens(&self) -> u32 {
            0
        }
        async fn deliver(
            &self,
            _ctx: &crate::persona::rag_budget::RagContext,
            _budget: u32,
            resolution: crate::persona::rag_budget::ResolutionPreference,
        ) -> crate::persona::rag_budget::RagDelivery {
            self.entries.fetch_add(1, AtomicOrdering::SeqCst);
            self.entered.notify_one();
            // Park until the test opens the gate. `wait_for` returns immediately
            // when it is already open, so an open gate costs nothing.
            let _ = self.gate.subscribe().wait_for(|open| *open).await;
            crate::persona::rag_budget::RagDelivery {
                source_id: "gated-grounding".to_string(),
                items: vec![crate::persona::rag_budget::RagItem {
                    content: "roster: Ivar [persona], win-claude [claude]".to_string(),
                    tokens: 12,
                    metadata: serde_json::Value::Null,
                }],
                tokens_used: 12,
                continuation: None,
                resolution_used: resolution,
            }
        }
        async fn deliver_continuation(
            &self,
            _ctx: &crate::persona::rag_budget::RagContext,
            _cursor: crate::persona::rag_budget::ContinuationCursor,
            _budget: u32,
        ) -> Option<crate::persona::rag_budget::RagDelivery> {
            None
        }
    }

    // what this catches: that deferring grounding takes the slow source OFF the
    // perception barrier — the structural claim, proven by control flow instead of
    // by a stopwatch. Model LOCKED (same HeuristicInferenceAdapter both forks), ONE
    // variable changed (defer_grounding). With the source's deliver held SHUT:
    //   * the DEFERRED fork's warm tick still completes — it can only do that by
    //     serving reprojected last-good, i.e. never awaiting deliver; and
    //   * the SYNCHRONOUS fork's warm tick runs INTO deliver and parks there —
    //     the control that proves the hold is real and the completion above means
    //     something.
    // A future parked on a shut gate cannot complete no matter how loaded the
    // machine is, so neither assertion has a timing tolerance to blow. The COST
    // this buys (that removing it from the barrier removes exactly its deliver
    // from the turn's wait) is the arithmetic half, table-tested clock-free in
    // `workspace_dashboard::tests`.
    //
    // This replaces a version that asserted `deferred_critical_path < 30ms` on the
    // live cycle. It measured wall clock through `cargo test`'s parallelism, so on
    // a loaded CI runner (a 60s CPU-hog test running alongside it) the deferred
    // fork clocked 99,798µs against the synchronous fork's 62,427µs — the fork
    // that pays NO grounding cost measuring higher than the one that pays 60ms.
    // The number was scheduler stall, not grounding; the invariant was never
    // temporal. Never re-express it as a duration.
    #[tokio::test]
    async fn deferring_grounding_takes_the_slow_source_off_the_perception_barrier() {
        // A LIVENESS bound, never a performance threshold: every assertion here is
        // about control flow, so this only fires if a tick genuinely hangs.
        const LIVENESS: Duration = Duration::from_secs(30);
        const WARM_BURST: &str = "teammate: and the rollback plan?";

        // Build a cycle with ONE gated grounding source and run the COLD tick with
        // the gate OPEN, so the deferred lane publishes its last-good finding and
        // both forks reach the steady state a live persona actually runs at.
        async fn warmed(defer_grounding: bool) -> (WorkspaceCycle, Arc<GatedGrounding>, Uuid) {
            let persona = Uuid::new_v4();
            let room = Uuid::new_v4();
            let gate = Arc::new(GatedGrounding::new());
            let source: Arc<dyn RagSource> = Arc::clone(&gate) as Arc<dyn RagSource>;
            let mut cfg = cfg_for(persona);
            cfg.grounding_sources = vec![GroundingSource::framing(source).defer_tolerant()];
            cfg.defer_grounding = defer_grounding;

            let cycle = build_workspace_cycle(cfg);
            timeout(
                LIVENESS,
                cycle.run_in_room("teammate: where are we on the deploy?", room),
            )
            .await
            .expect("cold tick completes with the grounding gate open");
            (cycle, gate, room)
        }

        // DEFERRED fork: with deliver held shut, the warm tick must STILL complete.
        let (cycle, gate, room) = warmed(true).await;
        gate.shut_gate();
        timeout(LIVENESS, cycle.run_in_room(WARM_BURST, room))
            .await
            .expect(
                "deferred fork's warm tick never completed with the grounding deliver held \
                 shut — the deferred source is back ON the perception barrier",
            );
        // Release the background worker (parked on the gate) before teardown.
        gate.open_gate();
        drop(cycle);

        // SYNCHRONOUS fork — the control. Same gate, same hold: this tick must run
        // INTO deliver rather than complete, which is what makes the deferred
        // fork's completion above evidence of anything.
        let (cycle, gate, room) = warmed(false).await;
        gate.shut_gate();
        let before = gate.deliver_entries();
        let mut tick = std::pin::pin!(cycle.run_in_room(WARM_BURST, room));
        tokio::select! {
            _ = tick.as_mut() => panic!(
                "synchronous fork's warm tick completed while the grounding deliver was held \
                 shut — the source is not on the perception barrier at all, so this test can \
                 no longer tell deferral apart from nothing happening"
            ),
            reached = timeout(LIVENESS, gate.entered_beyond(before)) => {
                reached.expect(
                    "synchronous fork's warm tick never reached the grounding deliver — the \
                     source is not being delivered on the barrier"
                );
            }
        }
        // …and it finishes the moment the door opens, proving the hold — not a
        // deadlock elsewhere — is what kept it parked.
        gate.open_gate();
        timeout(LIVENESS, tick)
            .await
            .expect("synchronous fork's warm tick finishes once the gate opens");
    }

    mod hands_isolation {
        use super::*;
        use crate::cognition::tool_executor::CommandToolExecutor;
        use crate::runtime::{CommandExecutor, ModuleRegistry};
        use dashmap::DashMap;

        /// One core with the REAL `CodeModule` — the single process-global engine map
        /// every caller's workspace lives in, which is exactly why the leak is possible.
        fn hands_for(persona: Uuid) -> ActingHands {
            let (executor, _) = core_with_code();
            hands_over(Arc::new(CommandToolExecutor::for_persona(executor, persona)), persona)
        }

        /// A core with the real `CodeModule`, and its state, so a test can see whose hands
        /// the engine map holds.
        fn core_with_code() -> (Arc<CommandExecutor>, Arc<CodeState>) {
            let registry = Arc::new(ModuleRegistry::new());
            let state = Arc::new(CodeState::new(
                Arc::new(DashMap::new()),
                Arc::new(DashMap::new()),
                tokio::runtime::Handle::current(),
            ));
            registry.register(Arc::new(CodeModule::new(state.clone())));
            // The PRODUCTION authorization policy, not an AllowAll fixture: the fixture hid
            // code/release-hands being refused for a local persona (Astra's review of #5005).
            let executor = Arc::new(
                CommandExecutor::new(registry)
                    .with_policy(Arc::new(GridTrustAuthPolicy::new()))
                    .with_message_bus(Arc::new(crate::runtime::MessageBus::new())),
            );
            (executor, state)
        }

        fn hands_over(executor: Arc<dyn crate::cognition::tool_executor::ToolExecutor>, persona: Uuid) -> ActingHands {
            ActingHands {
                persona_id: persona,
                persona_name: "Anwen".to_string(),
                context: executor.hands_context(),
                executor,
                working_memory: Arc::new(crate::cognition::working_memory::WorkingMemory::new(8)),
            }
        }

        /// What her hands can SEE right now — the honest probe, because "where is the
        /// engine rooted" is only interesting insofar as it changes what she can read.
        async fn listing(hands: &ActingHands) -> String {
            let ctx = crate::cognition::tool_executor::ToolExecutionContext {
                persona_id: hands.persona_id,
                persona_name: hands.persona_name.clone(),
                session_id: Uuid::new_v4(),
                context_id: Uuid::new_v4(),
                caller_context: serde_json::Value::Null,
                persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
                    auto_load_media: false,
                    supported_media_types: vec![],
                },
            };
            let call = crate::ai::types::ToolCall {
                id: "ls".to_string(),
                name: "code/list".to_string(),
                input: serde_json::json!({ "path": "." }),
            };
            let out = hands
                .executor
                .execute_native_batch(std::slice::from_ref(&call), &ctx, 8000)
                .await
                .expect("code/list runs");
            out.results[0].content.clone()
        }

        // Regression: foreground and background work both record against the card the
        // CALL's own hands are bound to (Astra on #5026), never her current focus: a turn
        // moving her focus to another claim before the task's first poll cannot move it.
        #[tokio::test]
        async fn command_work_stays_with_its_card_across_background_dispatch() {
            use crate::persona::cognition_pulse::work_idle_ms;
            let home = tempfile::TempDir::new().unwrap();
            std::fs::write(home.path().join("work.txt"), "work").unwrap();
            let persona = Uuid::new_v4();
            let live = hands_for(persona);
            let (card_a, card_b) = (Uuid::new_v4(), Uuid::new_v4());
            let claim_a = crate::code::hands::HandsContext::for_claim(persona, card_a, Uuid::new_v4());
            let hands = live.in_context(claim_a).expect("claim A's hands");
            drive_create_workspace(&hands, &home.path().to_string_lossy(), &[], "root", false)
                .await
                .unwrap();
            note_acting_root_in(persona, Some(claim_a), Some(home.path().to_path_buf()));
            note_acting_card_in(persona, Some(claim_a), card_a);
            assert!(listing(&hands).await.contains("work.txt"));
            assert!(work_idle_ms(persona, Some(card_a), EpochMs::now().millis()).is_some());

            let exec = hands.executor.command_executor().unwrap();
            let mut rx = exec.message_bus().unwrap().receiver();
            let handle = exec.dispatch_background(
                "code/list",
                serde_json::json!({"path": "."}),
                Some(CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona)).in_hands(claim_a)),
            );
            // Current-thread test: the spawned task has not been polled yet. Her focus moves on.
            let claim_b = crate::code::hands::HandsContext::for_claim(persona, card_b, Uuid::new_v4());
            note_acting_root_in(persona, Some(claim_b), Some(home.path().to_path_buf()));
            note_acting_card_in(persona, Some(claim_b), card_b);
            set_focus(persona, Some(claim_b));
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    let event = rx.recv().await.unwrap();
                    if event.name == "command:completed"
                        && event.payload["handle"] == handle.to_string()
                    {
                        assert_eq!(event.payload["success"], true);
                        break;
                    }
                }
            })
            .await
            .expect("background completion");
            let now = EpochMs::now().millis();
            assert!(work_idle_ms(persona, Some(card_a), now).is_some(), "the work is claim A's");
            assert_eq!(work_idle_ms(persona, Some(card_b), now), None, "never the focus it moved to");
            set_focus(persona, None);
            note_acting_root_in(persona, Some(claim_a), None);
            note_acting_root_in(persona, Some(claim_b), None);
        }

        // what this catches (Astra on #5026): admission living in helpers, so a command that
        // started after its hands were released (a background command first polled after the
        // release) still ran in them; and a release that ran as ordinary admitted work, waiting
        // on itself. The DISPATCHER admits every command bound to a context; a release is the
        // kernel's, marked as one, and closes the hands to everything after it.
        #[tokio::test]
        async fn a_command_after_its_hands_were_released_is_refused_at_dispatch() {
            let dir = tempfile::TempDir::new().expect("a workspace");
            let persona = Uuid::new_v4();
            let (executor, _state) = core_with_code();
            let bound = CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona))
                .in_hands(crate::code::hands::HandsContext::fresh());
            executor
                .execute_with_caller("code/create-workspace", serde_json::json!({"workspace_root": dir.path().to_string_lossy()}), Some(bound.clone()))
                .await
                .expect("the hands are bound");
            executor
                .execute_with_caller("code/list", serde_json::json!({"path": "."}), Some(bound.clone()))
                .await
                .expect("open hands admit a command");

            let unmarked = CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona))
                .in_hands(crate::code::hands::HandsContext::fresh());
            let refused = executor.execute_with_caller("code/release-hands", serde_json::json!({}), Some(unmarked)).await;
            assert!(refused.is_err(), "a release run as ordinary work is refused: {refused:?}");

            executor
                .execute_with_caller("code/release-hands", serde_json::json!({}), Some(bound.clone().releasing_hands()))
                .await
                .expect("the kernel's release ends the hands");
            let late = executor.execute_with_caller("code/list", serde_json::json!({"path": "."}), Some(bound)).await;
            assert!(late.as_ref().is_err_and(|e| e.contains("released")), "a command after the release is refused at dispatch: {late:?}");
        }

        // what this catches (card fc9e9fd7; Kimi #11697, #312): one set of hands moved by
        // every turn and measurement that needed another root. A card claim's hands are their
        // own: rooted at the card, they never move her default hands, every turn of the claim
        // finds them where they were rooted, and a re-acquired claim is NEW hands that don't
        // inherit the old one's place. Her default hands can't be rooted or ended at all.
        #[tokio::test]
        async fn a_claims_hands_are_its_own_and_her_default_hands_never_move() {
            let card_dir = tempfile::TempDir::new().expect("card checkout");
            let home = tempfile::TempDir::new().expect("home dir");
            std::fs::write(card_dir.path().join("CARD_ONLY.rs"), "// card\n").unwrap();
            std::fs::write(home.path().join("HER_OWN_notes.md"), "# mine\n").unwrap();
            let persona = Uuid::new_v4();
            let live = hands_for(persona);
            drive_create_workspace(&live, &home.path().to_string_lossy(), &[], "home", false).await.expect("her home");

            let (card, claim) = (Uuid::new_v4(), Uuid::new_v4());
            let context = crate::code::hands::HandsContext::for_claim(persona, card, claim);
            let claim_hands = live.in_context(context).expect("her executor acts in a claim's context");
            drive_create_workspace(&claim_hands, &card_dir.path().to_string_lossy(), &[], "bind", false).await.expect("rooted once");
            assert!(listing(&claim_hands).await.contains("CARD_ONLY.rs"), "the claim's hands stand in the card");
            let at_home = listing(&live).await;
            assert!(at_home.contains("HER_OWN_notes.md") && !at_home.contains("CARD_ONLY.rs"), "her default hands never moved: {at_home}");

            let next_turn = live.in_context(crate::code::hands::HandsContext::for_claim(persona, card, claim)).expect("same claim");
            assert!(listing(&next_turn).await.contains("CARD_ONLY.rs"), "the next turn of the claim finds its hands where they were rooted");
            let reacquired = crate::code::hands::HandsContext::for_claim(persona, card, Uuid::new_v4());
            assert_ne!(reacquired, context, "a re-acquired claim is new hands, never the old one's");

            assert!(end_acting_context(&live).await.is_err(), "her default hands can't be ended");

            // A no-setup gym still needs bound hands. Previously its claim staged
            // Ordinary and the resolver searched only SWE checkouts, so valid
            // code/write and code/shell calls were refused as unrooted.
            let _home = crate::test_env::HomeGuard::set(home.path()).await;
            std::env::set_var("CONTINUUM_HOME", home.path());
            let gym_card = airc_lib::WorkCard {
                card_id: airc_work::WorkCardId::new(),
                repo: airc_work::RepoId::new("CambrianTech/continuum").unwrap(),
                title: "[bench coder-write-eval] sum_evens: write and verify".into(),
                body: None, priority: airc_work::Priority::P2, lane_id: None,
                state: airc_work::CardState::Claimed,
                owner: Some(airc_core::PeerId::from_uuid(persona)),
                claim_provenance: None,
                claim_id: Some(airc_work::ClaimId::from_uuid(Uuid::new_v4())),
                claim_expires_at_ms: None, last_heartbeat_at_ms: None,
                pull_request: None, created_by: airc_core::PeerId::new(),
                created_at_ms: 0, updated_at_ms: 0, reviews: None,
                submissions: Vec::new(), last_submission_rejection: None,
            };
            let workspace = card_staging::gym_workspace(home.path(), persona);
            assert!(card_staging::checkout_path_for(&persona, &gym_card).is_none());
            assert_eq!(
                card_staging::stage_for_card(home.path(), persona, &gym_card).await,
                card_staging::Staging::Ready { path: workspace.clone() },
            );
            let cycle = WorkspaceCycle::new(vec![], Arc::new(crate::cognition::workspace::SalienceArbiter), 4)
                .with_acting(Arc::new(ActingBody {
                    persona_id: persona, persona_name: live.persona_name.clone(),
                    executor: live.executor.clone(), working_memory: live.working_memory.clone(),
                    admission: Arc::new(AdmissionState::new(Arc::new(RecallMetadataRegistry::new()))),
                }));
            let bound = bind_card(&cycle, persona, &gym_card).await;
            let gym = &bound.hands.as_ref().expect("a claimed no-setup gym has real hands").hands;
            let (written, failed) = call(gym, "code/write", serde_json::json!({"file_path":"binding-proof.txt", "content":"gym-owned\n"})).await;
            assert!(!failed, "actual write is admitted: {written}");
            assert_eq!(std::fs::read_to_string(workspace.join("binding-proof.txt")).unwrap(), "gym-owned\n");
            let (ran, failed) = call(gym, "code/shell", serde_json::json!({"cmd":"cat binding-proof.txt", "wait_ms":20000})).await;
            assert!(!failed && ran.contains("gym-owned"), "actual shell sees the same artifact: {ran}");
            assert!(!home.path().join("binding-proof.txt").exists());
            assert!(!card_dir.path().join("binding-proof.txt").exists());
            assert!(listing(&claim_hands).await.contains("CARD_ONLY.rs"), "the prior claim's hands never moved");
            assert!(listing(&live).await.contains("HER_OWN_notes.md"), "resident hands never moved");
        }


        /// One tool call through these hands: its content, and whether it was an error.
        async fn call(hands: &ActingHands, name: &str, input: serde_json::Value) -> (String, bool) {
            let ctx = crate::cognition::tool_executor::ToolExecutionContext {
                persona_id: hands.persona_id,
                persona_name: hands.persona_name.clone(),
                session_id: Uuid::new_v4(),
                context_id: Uuid::new_v4(),
                caller_context: serde_json::Value::Null,
                persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
                    auto_load_media: false,
                    supported_media_types: vec![],
                },
            };
            let call = crate::ai::types::ToolCall { id: name.to_string(), name: name.to_string(), input };
            let out = hands.executor.execute_native_batch(std::slice::from_ref(&call), &ctx, 8000).await.expect("the call runs");
            (out.results[0].content.clone(), out.results[0].is_error.is_some())
        }

        /// Root a set of hands the way `root_acting_workspace` does: the workspace, then its
        /// place in the acting-root record.
        async fn root(hands: &ActingHands, dir: &std::path::Path) {
            drive_create_workspace(hands, &dir.to_string_lossy(), &[], "root", false).await.expect("roots");
            note_acting_root_in(hands.persona_id, hands.context, Some(dir.to_path_buf()));
        }

        // what this catches: an eval, a solve or a replay moving or borrowing the LIVING
        // persona's hands (#312, and Astra's review of #5005). A fork used to clone her live
        // executor, so its rooting moved her live tools, its restore moved them home, and a
        // default eval left her live hands at a deleted clone. Under the PRODUCTION policy, a
        // fork's own hands stand in an exam while hers stand at home, at the same time:
        // listing, Python execution and her live acting place all stay hers; a context never
        // rooted gets no workspace (never her desk); a running operation is killed and
        // confirmed by the awaited close; and an unclosed fork is still released on drop.
        #[tokio::test]
        async fn a_forks_own_hands_never_move_her_live_hands_and_end_with_the_fork() {
            let exam = tempfile::TempDir::new().expect("exam dir");
            let home = tempfile::TempDir::new().expect("home dir");
            std::fs::write(exam.path().join("EXAM_ONLY_blueprints.py"), "# flask\n").unwrap();
            std::fs::write(home.path().join("HER_OWN_notes.md"), "# mine\n").unwrap();
            let persona = Uuid::new_v4();
            let (executor, state) = core_with_code();
            let live_exec = CommandToolExecutor::for_persona(executor, persona);
            let live = hands_over(Arc::new(live_exec.clone()), persona);
            root(&live, home.path()).await;
            let live_card = Uuid::new_v4();
            note_acting_card_in(persona, None, live_card);

            let fork_exec = crate::cognition::tool_executor::ToolExecutor::in_fresh_hands(&live_exec, persona)
                .expect("a live persona's hands can open a fork's own");
            let fork = hands_over(fork_exec, persona);
            assert!(fork.context.is_some() && live.context.is_none(), "the fork's hands are a context of their own");

            // Fail closed: an acting context that was never rooted has no workspace, never hers.
            let (unrooted, refused) = call(&fork, "code/list", serde_json::json!({"path": "."})).await;
            assert!(refused, "an unrooted context must be refused, not handed her desk: {unrooted}");

            root(&fork, exam.path()).await;
            let (seen_live, _) = call(&live, "code/list", serde_json::json!({"path": "."})).await;
            let (seen_fork, _) = call(&fork, "code/list", serde_json::json!({"path": "."})).await;
            assert!(
                seen_live.contains("HER_OWN_notes.md") && !seen_live.contains("EXAM_ONLY_blueprints.py"),
                "rooting the fork moved HER live hands into the exam (#312): {seen_live}"
            );
            assert!(seen_fork.contains("EXAM_ONLY_blueprints.py"), "the fork's hands must stand in the exam: {seen_fork}");

            // Python runs where THESE hands stand: the fork's in the exam, hers at home.
            let listdir = serde_json::json!({"lang": "python", "code": "import os; print(sorted(os.listdir('.')))"});
            let (py_fork, _) = call(&fork, "code/run", listdir.clone()).await;
            let (py_live, _) = call(&live, "code/run", listdir).await;
            assert!(py_fork.contains("EXAM_ONLY_blueprints.py") && !py_fork.contains("HER_OWN_notes.md"), "the fork's Python must run in the exam: {py_fork}");
            assert!(py_live.contains("HER_OWN_notes.md") && !py_live.contains("EXAM_ONLY_blueprints.py"), "her Python must run at home: {py_live}");

            // Provenance: the fork's rooting is its own; her live place and card are unchanged.
            assert_eq!(acting_root_of(persona), Some(home.path().to_path_buf()), "her live place is still home");
            assert_eq!(acting_card_of(persona), Some(live_card), "her receipts still belong to her live card");
            assert_eq!(acting_card_in(persona, fork.context), None, "the fork never inherits her card");

            // A running operation in the fork is ended by the awaited close, under the
            // production policy, and confirmed.
            let (started, _) = call(&fork, "code/shell", serde_json::json!({"cmd": "sleep 600", "wait_ms": 100})).await;
            assert!(started.contains("running") || started.contains("Running"), "the fork has a running operation: {started}");
            close_acting_context(&fork).await.expect("the fork's context closes, with its kill confirmed");
            assert_eq!(state.file_engines.len(), 1, "only her live engine remains");
            assert_eq!(acting_root_in(persona, fork.context), None, "the fork's place is forgotten");
            let (after, _) = call(&live, "code/list", serde_json::json!({"path": "."})).await;
            assert!(after.contains("HER_OWN_notes.md"), "ending the fork leaves her hands as they were: {after}");

            // An unclosed fork (an abnormal exit) is still released when its hands drop.
            let dropped = hands_over(
                crate::cognition::tool_executor::ToolExecutor::in_fresh_hands(&live_exec, persona).expect("another fork"),
                persona,
            );
            root(&dropped, exam.path()).await;
            assert_eq!(state.file_engines.len(), 2);
            let dropped_context = dropped.context;
            drop(dropped);
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                while state.file_engines.len() != 1 || acting_root_in(persona, dropped_context).is_some() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("an unclosed fork is released, and its place forgotten, when its hands drop");
            note_acting_root_in(persona, None, None);
        }

        // what this catches (Astra's re-review of #5005): an eval cancelled while closing its
        // forks' hands deleting the world they still run in, while their release was merely
        // scheduled. After the cancel the world, the fork's engine and its place must all
        // remain; once the owned cleanup confirms the kill, all three go.
        #[tokio::test]
        async fn a_cancelled_eval_keeps_its_world_until_its_forks_are_confirmed_ended() {
            use futures::FutureExt;
            let parent = tempfile::TempDir::new().expect("parent dir");
            let world_dir = parent.path().join("run");
            std::fs::create_dir_all(&world_dir).unwrap();
            std::fs::write(world_dir.join("EXAM_ONLY_blueprints.py"), "# flask\n").unwrap();
            let persona = Uuid::new_v4();
            let (executor, state) = core_with_code();
            let live_exec = CommandToolExecutor::for_persona(executor, persona);
            let fork = hands_over(
                crate::cognition::tool_executor::ToolExecutor::in_fresh_hands(&live_exec, persona).expect("a fork's own hands"),
                persona,
            );
            let context = fork.context;
            let mut world = crate::cognition::eval::EvalWorld::new(Some(crate::cognition::eval::EphemeralEvalRoot::new(world_dir.clone())));
            world.hold(Some(fork.clone()));
            root(&fork, &world_dir).await;
            let (started, _) = call(&fork, "code/shell", serde_json::json!({"cmd": "sleep 600", "wait_ms": 100})).await;
            assert!(started.contains("running") || started.contains("Running"), "the fork has a running operation: {started}");
            drop(fork);

            // Poll the close once (it starts the kill, then waits on the reap this
            // single-threaded runtime can't have run yet), and cancel it there.
            assert!(world.finish().now_or_never().is_none(), "the close can't confirm before the reap runs");
            assert!(world_dir.is_dir(), "a cancelled close must not delete the world its fork still runs in");
            assert_eq!(state.file_engines.len(), 1, "the fork's engine is kept until its kill is confirmed");
            assert!(acting_root_in(persona, context).is_some(), "the fork's place is kept until its kill is confirmed");

            // The owned cleanup outlives the cancel: it confirms the kill, then cleans up.
            // 10 s is this test's bound on a SIGKILLed `sleep` being reaped, not a product timeout.
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                while world_dir.exists() || !state.file_engines.is_empty() || acting_root_in(persona, context).is_some() {
                    tokio::time::sleep(crate::code::shell_session::KILL_POLL).await;
                }
            })
            .await
            .expect("once the kill is confirmed, the world, the engine and the place all go");
        }

        // what this catches: a pure-cognition persona has no hands, so there is nothing to
        // root and nothing to restore. `of` must say so rather than have the caller invent
        // a handle — the fail-loud in `root_acting_workspace` depends on this being None.
        #[test]
        fn a_handless_cycle_yields_no_hands() {
            let cycle = crate::cognition::workspace::WorkspaceCycle::new(
                vec![],
                Arc::new(crate::cognition::workspace::SalienceArbiter),
                4,
            );
            assert!(ActingHands::of(&cycle).is_none());
        }
    }
