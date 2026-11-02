#![cfg(test)]
use super::*;

    // what this catches (Astra's block on #4315): the wake's verdict must come from the
    // board's holder projection, never from `owner` read as authority. An EXPIRED claim
    // still names its last holder — which may be HER — so "X took it" from `owner` would
    // tell Kimi that Kimi took her own card; a card in Review is absent from her live
    // claims because its life advanced, not because a lease lapsed. Each branch pinned at
    // a fixed clock.
    #[test]
    pub(super) fn the_wakes_verdict_is_the_boards_never_the_owner_fields() {
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

    // what this catches: Kimi locked out of 0c69aa07 for hours (2026-10-21). Her claim's hands
    // were closed while the board still showed the claim hers. Closed hands never reopen, so
    // only an END may close them: a released claim, a card that moved on, or a different claim
    // on the card. A lease that LAPSED keeps her claim id and is renewed by her next
    // heartbeat, and a row the board couldn't give says nothing; neither closes her hands.
    #[test]
    pub(super) fn a_claims_hands_close_only_when_the_claim_ended_never_on_a_lapsed_lease() {
        use airc_core::PeerId;
        use airc_work::{CardState, ClaimId, Priority, RepoId, WorkCard, WorkCardId};
        let her = PeerId::new();
        let me = her.as_uuid();
        let card_id = WorkCardId::new();
        let mine = ClaimId::from_uuid(Uuid::new_v4());
        let context = crate::identity::hands_context::HandsContext::for_claim(me, card_id.as_uuid(), mine.as_uuid());
        let row = |owner: Option<PeerId>, claim: Option<ClaimId>, expires: Option<u64>, state: CardState| WorkCard {
            card_id,
            repo: RepoId::new("CambrianTech/continuum").expect("valid repo id in fixture"),
            title: "a card".to_string(),
            body: None,
            priority: Priority::P1,
            lane_id: None,
            state,
            owner,
            claim_provenance: None,
            claim_id: claim,
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
        // Her lease lapsed: same claim id, renewable. NOT an end.
        assert!(!claim_has_ended(context, Some(&row(Some(her), Some(mine), Some(1), CardState::InProgress)), me));
        // Her live claim: not an end.
        assert!(!claim_has_ended(context, Some(&row(Some(her), Some(mine), Some(u64::MAX), CardState::InProgress)), me));
        // The board could not give the row: unknown, never an end.
        assert!(!claim_has_ended(context, None, me));
        // Released (the board clears claim_id): ended.
        assert!(claim_has_ended(context, Some(&row(None, None, None, CardState::Open)), me));
        // A different claim on the card, hers re-claimed or another holder's: this claim ended.
        let other = ClaimId::from_uuid(Uuid::new_v4());
        assert!(claim_has_ended(context, Some(&row(Some(her), Some(other), Some(u64::MAX), CardState::Claimed)), me));
        assert!(claim_has_ended(context, Some(&row(Some(PeerId::new()), Some(other), Some(u64::MAX), CardState::Claimed)), me));
        // Terminal, even under her own claim id: ended.
        for state in [CardState::Merged, CardState::Closed] {
            assert!(claim_has_ended(context, Some(&row(Some(her), Some(mine), Some(u64::MAX), state)), me), "{state:?}");
        }
        // Review and Blocked keep her claim (a send-back returns the card to InProgress under the
        // SAME claim): not an end, or her first submission would lock her out (Codex on #5247).
        for state in [CardState::Review, CardState::Blocked] {
            assert!(!claim_has_ended(context, Some(&row(Some(her), Some(mine), Some(u64::MAX), state)), me), "{state:?}");
        }
    }

    // what this catches (Astra, #4315 round 2): a belief the board has not answered must
    // SURVIVE the next checkpoint. `snapshot()` writes `acting_card: None` and the writer
    // stamps the ambient root; with her hands unheld and the board unseen, that wrote None
    // and a second restart forgot the comparison. Restore → unknown → save → restore must
    // carry the same card; once the board has answered (take), the next save stamps the
    // ambient root again.
    #[test]
    pub(super) fn an_unanswered_belief_survives_the_next_checkpoint() {
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
    pub(super) fn a_persisted_checkpoint_s_result_handles_are_readable_before_restore() {
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
            let cap = 8_192; // a test body size; a shed below it collapses the older ones
            let saved = |handle: &str| crate::ai::types::ResultCustody::Whole { handle: handle.into() };
            memory.record_receipt_typed(&[], &"c".repeat(cap / 2), None, saved("c011a75ed0000001"));
            for _ in 0..4 {
                memory.record_receipt_typed(&[], &"s".repeat(cap / 2), None, saved("5000000000000002"));
            }
            memory.record_settlement("done", None);
            memory.evict_resident_results_over(0, &|_, _| true);
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
    pub(super) fn adoption_source(
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
    pub(super) fn legacy_adoption_preserves_selected_bytes_and_is_bound_on_replay() {
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
    pub(super) fn legacy_adoption_refuses_changed_source_destination_and_offline_state() {
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
    pub(super) fn legacy_adoption_recovers_committed_file_without_receipt() {
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
    pub(super) fn legacy_adoption_inspection_rejects_identity_and_schema_mismatch() {
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
    pub(super) fn legacy_adoption_refuses_a_checkpoint_owner_already_holding_the_lock() {
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

    // What this catches (f098571b): a native launch must restore the same durable
    // working memory it saved, rather than start a second history under CWD.
    #[test]
    pub(super) fn native_home_volatile_checkpoint_preserves_the_action_receipt() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let persona = Uuid::new_v4();
        let memory = super::super::super::working_memory::WorkingMemory::new(8);
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
        let resumed = super::super::super::working_memory::WorkingMemory::new(8);
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
    pub(super) async fn every_resident_at_a_seam_measures_her_own_tree_including_untracked_files() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let registry = Arc::new(PersonaWorkspaceRegistry::new());
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
    pub(super) async fn her_mind_room_runs_on_its_own_cycle_never_checkpointed() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let registry = Arc::new(PersonaWorkspaceRegistry::new());
        let her = Uuid::new_v4();
        let live = registry.register_from_cfg(cfg_for(her)).expect("live cycle");
        let mind_room = crate::cognition::mind_room::mind_room_id(her);
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
    pub(super) async fn resident_checkpoint_excludes_replaced_cycles_and_eval_forks() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let registry = Arc::new(PersonaWorkspaceRegistry::new());
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
    pub(super) async fn unregistered_cycle_has_no_persistence_authority() {
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
    pub(super) async fn resident_checkpoint_reports_io_failure_and_saves_other_residents() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let registry = Arc::new(PersonaWorkspaceRegistry::new());
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
        std::fs::create_dir(&path).expect("test: obstruct only the failed resident checkpoint");
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
