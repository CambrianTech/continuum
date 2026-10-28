#![cfg(test)]
    use super::*;

    // what this catches (the 5090, 2026-10-05): a second process reading a RUNNING core's
    // ledger journaled its open jobs killed-by-reboot. `cargo test` reached the home ledger
    // at 13:32Z and 16:31Z, and 18 of Kimi's jobs died with no reboot. A board that does not
    // own the ledger reconciles nothing; once the owner dies, the next owner reconciles.
    #[test]
    fn only_the_ledgers_owner_reconciles_and_a_dead_owners_jobs_become_orphans() {
        let dir = tempfile::tempdir().expect("test: tempdir");
        let ledger = dir.path().join("jobs-ledger.jsonl");
        let core = TrainingJobBoard::with_ledger(Some(ledger.clone()));
        let open = Uuid::new_v4();
        core.register(watched(open, "engine-local"));

        let reader = TrainingJobBoard::with_ledger(Some(ledger.clone()));
        assert!(reader.reconcile_orphans().is_empty(), "a live core's open job is not an orphan to a reader");
        drop(reader);
        let text = std::fs::read_to_string(&ledger).expect("test: ledger");
        assert!(!text.contains("killed-by-reboot"), "nothing was journaled dead while the core lived");

        drop(core); // the core dies; the OS releases its lock
        let next = TrainingJobBoard::with_ledger(Some(ledger));
        let orphaned: Vec<Uuid> = next.reconcile_orphans().iter().map(|o| o.local_id).collect();
        assert_eq!(orphaned, vec![open], "the next owner reconciles the dead core's open job");
    }

    fn watched(local_id: Uuid, provider: &str) -> WatchedJob {
        WatchedJob {
            trigger_dispatch_id: None,
            handle: JobHandle {
                provider_id: provider.to_string(),
                provider_job_id: "job-x".to_string(),
                local_id,
            },
            persona_id: Uuid::new_v4(),
            persona_name: "asha".to_string(),
            base_model: "qwen3-coder".to_string(),
            trait_kind: "code".to_string(),
            eval_set: Some("docs/genome/coder-eval.jsonl".to_string()),
            signature: None,
            decision: None,
        }
    }

    // What this catches (278afa6c): a concrete correlated handle survives loss
    // of the live board; earlier/unrelated records and partial scans cannot be
    // mistaken for proof that the dispatch did not happen.
    #[test]
    fn trigger_evidence_survives_restart_and_pages_without_false_absence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("jobs.jsonl");
        let board = TrainingJobBoard::with_ledger(Some(path.clone()));
        let dispatch_id = Uuid::new_v4();
        let local_id = Uuid::new_v4();
        for _ in 0..512 {
            board.register(watched(Uuid::new_v4(), "unrelated-provider"));
            if std::fs::metadata(&path).unwrap().len() > 2 * DISPATCH_JOURNAL_PAGE_BYTES {
                break;
            }
        }
        let target_offset = std::fs::metadata(&path).unwrap().len();
        assert!(
            target_offset > 2 * DISPATCH_JOURNAL_PAGE_BYTES,
            "actual journal bytes must force multiple pages before the target"
        );
        let mut target = watched(local_id, "local-candle");
        target.trigger_dispatch_id = Some(dispatch_id);
        board.register(target);
        assert!(
            matches!(board.lookup_trigger_dispatch(dispatch_id, 0).unwrap(),
            DispatchLookup::Observed(handle) if handle.local_id == local_id)
        );
        drop(board);
        let recovered = TrainingJobBoard::with_ledger(Some(path.clone()));
        let mut cursor = match recovered.lookup_trigger_dispatch(dispatch_id, 0).unwrap() {
            DispatchLookup::Incomplete { next_offset } => next_offset,
            _ => panic!("fixture must cross the bounded journal page"),
        };
        assert!(cursor > 0 && cursor <= DISPATCH_JOURNAL_PAGE_BYTES);
        assert!(cursor < target_offset);
        loop {
            match recovered
                .lookup_trigger_dispatch(dispatch_id, cursor)
                .unwrap()
            {
                DispatchLookup::Incomplete { next_offset } => {
                    assert!(next_offset > cursor);
                    cursor = next_offset;
                }
                DispatchLookup::Observed(handle) => {
                    assert_eq!(handle.local_id, local_id);
                    assert_eq!(handle.provider_id, "local-candle");
                    break;
                }
                DispatchLookup::NotObserved => {
                    panic!("lost correlated evidence after a partial scan")
                }
                DispatchLookup::Corrupt { .. } => panic!("clean fixture reported corruption"),
            }
        }
        let end = std::fs::metadata(&path).unwrap().len();
        assert!(matches!(
            recovered
                .lookup_trigger_dispatch(Uuid::new_v4(), end)
                .unwrap(),
            DispatchLookup::NotObserved
        ));
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{\"event\":")
            .unwrap();
        assert!(
            matches!(recovered.lookup_trigger_dispatch(dispatch_id, end).unwrap(),
            DispatchLookup::Incomplete { next_offset } if next_offset == end)
        );
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"bad}\n")
            .unwrap();
        assert!(
            matches!(recovered.lookup_trigger_dispatch(dispatch_id, end).unwrap(),
            DispatchLookup::Corrupt { observed: None, malformed: 1, next_offset } if next_offset > end)
        );
        assert!(recovered
            .lookup_trigger_dispatch(dispatch_id, u64::MAX)
            .is_err());
    }

    // what this catches (card 2dfce676): a refused training job's examples stranded because
    // the trigger is never told its job failed. A FAILED job the trigger dispatched is handed
    // to the trigger exactly once; a completed one, or one the trigger didn't dispatch (an
    // operator's), is not.
    #[test]
    fn a_failed_trigger_dispatched_job_is_handed_to_the_trigger_once() {
        let board = TrainingJobBoard::default();
        let (failed, done, operators) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        for (id, dispatched) in [(failed, true), (done, true), (operators, false)] {
            let mut job = watched(id, "engine-local");
            job.trigger_dispatch_id = dispatched.then(Uuid::new_v4);
            board.register(job);
        }
        board.claim(failed, &TrainingStatus::Failed { error: "the engine's training run failed: over budget".into() });
        board.claim(operators, &TrainingStatus::Failed { error: "x".into() });
        let taken: Vec<Uuid> = board.take_failed().iter().map(|j| j.handle.local_id).collect();
        assert_eq!(taken, vec![failed], "only the trigger's own failed job");
        assert!(board.take_failed().is_empty(), "handed over once");
        assert!(board.claim(done, &TrainingStatus::Cancelled).is_some());
        assert!(board.take_failed().is_empty(), "a cancelled job is not a refusal to return");
    }

    // what this catches: the claim-once contract. A registered job is visible in the
    // snapshot, claim() returns it exactly once and removes it, and a second claim
    // yields None — the invariant that prevents the L3 sentinel from running the
    // eval chain twice for one completed job.
    #[test]
    fn claim_returns_a_job_exactly_once() {
        let board = TrainingJobBoard::default();
        let id = Uuid::new_v4();
        board.register(watched(id, "local-candle"));

        assert_eq!(board.len(), 1, "registered job must be in flight");
        assert_eq!(
            board.snapshot().len(),
            1,
            "snapshot sees the registered job"
        );

        assert!(board.claim(id, &TrainingStatus::Queued).is_none());
        assert!(board
            .claim(
                id,
                &TrainingStatus::WaitingForCapacity {
                    required_bytes: 1024,
                    available_bytes: 512
                }
            )
            .is_none());
        assert_eq!(
            board.len(),
            1,
            "a nonterminal observation cannot retire a job"
        );
        let claimed = board
            .claim(id, &TrainingStatus::Cancelled)
            .expect("first claim returns the job");
        assert_eq!(claimed.handle.local_id, id);
        assert!(board.is_empty(), "claim removes the job from the board");
        assert!(
            board.claim(id, &TrainingStatus::Cancelled).is_none(),
            "a second claim of the same job must be None — no double-processing"
        );
    }

    // what this catches: re-register replaces (keyed by local_id), so a duplicate
    // dispatch of the same correlation id never inflates the in-flight count.
    #[test]
    fn register_is_keyed_by_local_id() {
        let board = TrainingJobBoard::default();
        let id = Uuid::new_v4();
        board.register(watched(id, "openai"));
        board.register(watched(id, "openai"));
        assert_eq!(board.len(), 1, "same local_id must not duplicate");
    }

    // what this catches: the severed-flywheel-tail bug (glass-boxed 2026-07-11 —
    // 41 submissions, zero recorded outcomes). A job registered by a "previous
    // core" (same ledger file, fresh board) but never claimed MUST be reconciled
    // as killed-by-reboot at the next boot — loud, journaled, and idempotent —
    // while a claimed job reconciles clean. // regression for task #137
    #[test]
    fn orphaned_jobs_reconcile_as_killed_by_reboot() {
        let dir = std::env::temp_dir().join(format!("job-ledger-test-{}", Uuid::new_v4()));
        let ledger = dir.join("jobs-ledger.jsonl");

        // "Previous core": registers two jobs, claims one, then dies (dropped).
        let previous = TrainingJobBoard::with_ledger(Some(ledger.clone()));
        let done = Uuid::new_v4();
        let orphan = Uuid::new_v4();
        previous.register(watched(done, "mlx"));
        previous.register(watched(orphan, "mlx"));
        previous
            .claim(
                done,
                &TrainingStatus::Failed {
                    error: "capacity refused before weights".into(),
                },
            )
            .expect("claim the finished job");
        drop(previous);

        // "Next boot": replay finds exactly the unclaimed job.
        let next = TrainingJobBoard::with_ledger(Some(ledger.clone()));
        assert_eq!(
            next.reconcile_orphans().len(),
            1,
            "exactly the never-terminal job is orphaned"
        );
        let text = std::fs::read_to_string(&ledger).expect("ledger exists");
        let terminal: serde_json::Value = text
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .find(|row| row["event"] == "terminal" && row["local_id"] == done.to_string())
            .expect("completed job has a durable outcome");
        assert_eq!(terminal["status"]["state"], "failed");
        assert_eq!(
            terminal["status"]["error"],
            "capacity refused before weights"
        );
        assert_eq!(terminal["handle"]["localId"], done.to_string());
        let handle = watched(done, "mlx").handle;
        assert!(matches!(next.lookup_terminal(&handle, 0).unwrap(),
            JournalLookup::Observed(TrainingStatus::Failed { error })
                if error == "capacity refused before weights"));
        let foreign = JobHandle {
            provider_id: "another-provider".into(),
            ..handle.clone()
        };
        assert!(matches!(
            next.lookup_terminal(&foreign, 0).unwrap(),
            JournalLookup::NotObserved
        ));
        assert!(
            text.contains("killed-by-reboot") && text.contains(&orphan.to_string()),
            "the orphan's death is journaled: {text}"
        );
        // Idempotent: the terminal line just written closes the id.
        assert_eq!(
            next.reconcile_orphans().len(),
            0,
            "a second replay sees the orphan as closed"
        );
        // Corruption between valid receipts must not hide either receipt or
        // imply clean absence. Keep the bad bytes and report their page count.
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&ledger)
            .unwrap();
        // Older cores recorded a reboot interruption without a typed status or
        // handle. Recover the failure only with its exact registration evidence;
        // a continuation starting at the terminal row alone cannot prove identity.
        let legacy = Uuid::new_v4();
        let legacy_handle = watched(legacy, "mlx").handle;
        next.register(watched(legacy, "mlx"));
        let terminal_offset = std::fs::metadata(&ledger).unwrap().len();
        writeln!(file, "{}", serde_json::json!({
            "event": "terminal", "local_id": legacy, "reason": "killed-by-reboot"
        })).unwrap();
        assert!(matches!(next.lookup_terminal(&legacy_handle, 0).unwrap(),
            JournalLookup::Observed(TrainingStatus::Failed { error })
                if error == "killed-by-reboot (legacy journal receipt)"));
        for foreign in [
            JobHandle { provider_id: "foreign".into(), ..legacy_handle.clone() },
            JobHandle { provider_job_id: "foreign".into(), ..legacy_handle.clone() },
        ] {
            assert!(matches!(next.lookup_terminal(&foreign, 0).unwrap(),
                JournalLookup::NotObserved));
        }
        assert!(matches!(next.lookup_terminal(&legacy_handle, terminal_offset).unwrap(),
            JournalLookup::NotObserved));
        writeln!(file, "not-json").unwrap();
        let later = Uuid::new_v4();
        let dispatch = Uuid::new_v4();
        let mut job = watched(later, "mlx");
        job.trigger_dispatch_id = Some(dispatch);
        let later_handle = job.handle.clone();
        next.register(job);
        next.claim(later, &TrainingStatus::Cancelled).unwrap();
        let reread = TrainingJobBoard::with_ledger(Some(ledger.clone()));
        for expected in [&handle, &later_handle, &legacy_handle] {
            assert!(matches!(
                reread.lookup_terminal(expected, 0).unwrap(),
                JournalLookup::Corrupt {
                    observed: Some(_),
                    malformed: 1,
                    ..
                }
            ));
        }
        assert!(
            matches!(reread.lookup_trigger_dispatch(dispatch, 0).unwrap(),
            JournalLookup::Corrupt { observed: Some(found), malformed: 1, .. }
                if found.local_id == later)
        );
        assert!(matches!(
            reread.lookup_terminal(&foreign, 0).unwrap(),
            JournalLookup::Corrupt {
                observed: None,
                malformed: 1,
                ..
            }
        ));
        assert_eq!(
            reread.quarantined.len(),
            1,
            "repeat scans report the offset once"
        );
        assert!(std::fs::read_to_string(&ledger)
            .unwrap()
            .contains("not-json"));
        let _ = std::fs::remove_dir_all(&dir);
    }
    // what this catches (card faed9279): two tasks journaling at once zipped their
    // rows together byte-wise — `writeln!` on a raw File was one write(2) per Display
    // fragment, and O_APPEND made each fragment, not each line, the atomic unit. 136 of
    // 701 lines on the M5 were unreadable, in two shapes (interleaved, and "torn" — a
    // row whose tail landed after another writer's newline). One serialized line, one
    // write_all: every row lands whole. Two threads × 1,000 rows → 2,000 rows, every one
    // parses, every one carries exactly its own writer's id. PROVEN RED before the fix:
    // with the writer reverted to `writeln!(f, "{line}")`, run 0 line 0 was already
    // unreadable ("expected `:` at line 1 column 9" — a fragment boundary inside the
    // first key) on the IntelMac, 2026-09-22. Five runs so a lucky schedule cannot pass
    // a broken writer.
    #[test]
    fn two_writers_never_zip_their_rows_together() {
        for run in 0..5 {
            let dir = std::env::temp_dir().join(format!("faed9279-{}-{run}", Uuid::new_v4()));
            let board = std::sync::Arc::new(TrainingJobBoard::with_ledger(Some(dir.join("jobs-ledger.jsonl"))));
            let writers: Vec<_> = ["aaaaaaaa", "bbbbbbbb"]
                .into_iter()
                .map(|who| {
                    let board = board.clone();
                    std::thread::spawn(move || {
                        for n in 0..1_000u32 {
                            board.journal(&serde_json::json!({
                                "event": "registered",
                                "who": who,
                                "n": n,
                                // long enough that Display would split it across many
                                // fragments — the shape that interleaved in production
                                "pad": "x".repeat(200),
                            }));
                        }
                    })
                })
                .collect();
            for w in writers {
                w.join().expect("writer thread");
            }
            let text = std::fs::read_to_string(dir.join("jobs-ledger.jsonl")).expect("ledger");
            let lines: Vec<&str> = text.lines().collect();
            assert_eq!(lines.len(), 2_000, "run {run}: every row landed, none merged");
            for (i, line) in lines.iter().enumerate() {
                let v: serde_json::Value = serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("run {run} line {i} unreadable: {e}\n{line}"));
                let who = v["who"].as_str().expect("who");
                assert!(who == "aaaaaaaa" || who == "bbbbbbbb", "run {run} line {i}: {who}");
                assert_eq!(v["pad"].as_str().map(str::len), Some(200), "run {run} line {i}: pad intact");
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    // regression for card 244757bc: the boot replay must HAND BACK what it found, not
    // only count it — and a lineage's resumes are counted from the ledger so a job that
    // dies at every boot stops at MAX_RESUMES.
    #[test]
    fn the_boot_replay_hands_orphans_back_and_counts_a_lineages_resumes() {
        let dir = tempfile::tempdir().expect("test: tempdir");
        let ledger = dir.path().join("jobs-ledger.jsonl");
        let board = TrainingJobBoard::with_ledger(Some(ledger.clone()));
        let dead = Uuid::from_u128(0xdead);
        board.journal(&serde_json::json!({
            "event": "registered", "local_id": dead.to_string(), "provider_id": "cuda-local",
            "persona_id": Uuid::from_u128(7).to_string(), "persona_name": "Kimi",
            "base_model": "ggml-org/Qwen3.8-27B-GGUF", "trait_kind": "code", "at_ms": 1
        }));
        drop(board); // the core that registered it dies; only then is it an orphan
        let replay = TrainingJobBoard::with_ledger(Some(ledger.clone()));
        let orphans = replay.reconcile_orphans();
        assert_eq!(orphans.len(), 1);
        assert_eq!((orphans[0].local_id, orphans[0].provider_id.as_str(), orphans[0].trait_kind.as_str()), (dead, "cuda-local", "code"));
        assert_eq!(replay.take_orphans().len(), 1, "handed to the trigger once");
        assert!(replay.take_orphans().is_empty(), "…and only once");
        // The lineage: dead → r1 → r2; attempts count from the origin.
        let r1 = Uuid::from_u128(0x11);
        let r2 = Uuid::from_u128(0x22);
        replay.journal_resumed(dead, dead, r1, 1);
        replay.journal_resumed(dead, r1, r2, 2);
        assert_eq!(replay.resume_origin(r2), dead);
        assert_eq!(replay.resume_attempts(dead), 2);
        assert_eq!(replay.resume_origin(Uuid::from_u128(0x99)), Uuid::from_u128(0x99), "a job with no lineage is its own origin");
        // The dead one is journaled dead too: the NEXT owner finds no open registration
        // (dropping `replay` first, or the check would pass only because it is not the owner).
        drop(replay);
        assert!(TrainingJobBoard::with_ledger(Some(ledger)).reconcile_orphans().is_empty());
    }

    // what this catches (cards 8a2334d9, 565b3b64): a job that ended before a restart (Kimi's
    // 55f2e6de completed; her 2de02f40 correction's all-parked job failed) losing its only route
    // to returning what it parked, because the runtime list dies with the core. The next owner
    // lists every ended job from the ledger, marked trained or not, a completion outranking a
    // failure row of the same job; a killed-by-reboot row (no status) is not an ending this
    // lists; a reader that doesn't own the ledger lists none.
    #[test]
    fn the_boot_replay_lists_every_ended_job_for_its_remainder() {
        let dir = tempfile::tempdir().expect("test: tempdir");
        let ledger = dir.path().join("jobs-ledger.jsonl");
        let board = TrainingJobBoard::with_ledger(Some(ledger.clone()));
        let registered = |id: Uuid| serde_json::json!({
            "event": "registered", "local_id": id.to_string(), "provider_id": "engine-local",
            "persona_id": Uuid::from_u128(7).to_string(), "persona_name": "Kimi",
            "base_model": "ggml-org/Qwen3.8-27B-GGUF", "trait_kind": "code/owner", "at_ms": 1
        });
        let (done, failed, killed) = (Uuid::from_u128(0xd0), Uuid::from_u128(0xfa), Uuid::from_u128(0x4b));
        for id in [done, failed, killed] {
            board.journal(&registered(id));
        }
        let artifact = super::super::types::TrainingArtifact { model_id: "gene".into(), local_path: None, format: Default::default(), metrics: Default::default() };
        board.journal(&serde_json::json!({"event": "terminal", "reason": "claimed", "local_id": done.to_string(),
            "status": TrainingStatus::Failed { error: "an earlier row".into() }, "at_ms": 2}));
        board.journal(&serde_json::json!({"event": "terminal", "reason": "claimed", "local_id": done.to_string(),
            "status": TrainingStatus::Completed { artifact }, "at_ms": 3}));
        board.journal(&serde_json::json!({"event": "terminal", "reason": "claimed", "local_id": failed.to_string(),
            "status": TrainingStatus::Failed { error: "none of the 11 examples trains whole".into() }, "at_ms": 2}));
        board.journal(&serde_json::json!({"event": "terminal", "reason": "killed-by-reboot", "local_id": killed.to_string(), "at_ms": 2}));
        let reader = TrainingJobBoard::with_ledger(Some(ledger.clone()));
        reader.reconcile_ended();
        assert!(reader.take_ended().is_empty(), "a board that doesn't own the ledger lists nothing");
        drop((board, reader));
        let next = TrainingJobBoard::with_ledger(Some(ledger));
        next.reconcile_ended();
        let job = |local_id, trained| EndedJob { local_id, persona_name: "Kimi".into(), trait_kind: "code/owner".into(), trained };
        assert_eq!(next.take_ended(), vec![job(done, true), job(failed, false)], "the completed job as trained, the failed one as not, the killed one not at all");
    }
