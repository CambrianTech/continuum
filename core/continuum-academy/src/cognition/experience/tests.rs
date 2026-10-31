#![cfg(test)]
    use super::*;
    use crate::cognition::act_observe::SettleOutcome;
    use crate::cognition::workspace::{Decision, TurnMetrics};

    /// Build a SettleOutcome standing in for a lived trajectory — a settled Speak
    /// with an answer longer than the report's 200-char cap, so we can prove the
    /// record keeps it whole.
    fn settled_speaking(answer: &str, acts: usize, world_state: &str) -> SettleOutcome {
        SettleOutcome {
            decision: Decision::Speak {
                text: answer.to_string(),
            },
            spoken: Some(answer.to_string()),
            acts,
            world_state: world_state.to_string(),
            room_updates: Default::default(),
            metrics: TurnMetrics::default(),
            failure: None,
            deferred_before_dispatch: false,
        capacity_refusal: None,
            touched_paths: Vec::new(),
            room: uuid::Uuid::from_u128(7),
            generation_receipts: Vec::new(),
            turn_acts: Vec::new(),
        }
    }

    fn eval_task(id: &str, with_test: bool) -> EvalTask {
        EvalTask {
            id: id.to_string(),
            prompt: "write a function that reverses a string".to_string(),
            test: with_test.then(|| "assert_eq!(rev(\"ab\"), \"ba\");".to_string()),
            lang: Some("rust".to_string()),
            ..Default::default()
        }
    }

    // What this catches: ErrorSalience is the HONEST selector — it fires on a
    // failed episode (naming the grade as the weakness to remediate) and stays
    // silent on a passed one, so consolidation targets the fitness gap and never
    // wastes a class on what she already knows. Guards the doc's "we consolidate
    // near where she fails" discipline. See [[attention-salience-selects-what-becomes-curriculum]].
    #[test]
    fn error_salience_fires_on_failure_and_ignores_success() {
        let detector = ErrorSalience;

        let failed = ExperienceRecord {
            task: eval_task("rev-1", true),
            ok: false,
            grade: "error[E0308]: mismatched types — expected String, found &str".to_string(),
            answer: "fn rev(s: &str) -> &str { s }".to_string(),
            world_state: String::new(),
            room_updates: Default::default(),
            acts: 3,
            source: ExperienceSource::Eval,
            room: Some(uuid::Uuid::from_u128(7)),
            teammates: Vec::new(),
            team_role: None,
        };
        let signal = detector
            .assess(&failed)
            .expect("a failed episode must be salient");
        assert_eq!(signal.kind, SalienceKind::Error);
        assert!(
            signal.weakness.contains("E0308"),
            "the weakness must carry the REAL grader error (what genome/teach reads back), got: {}",
            signal.weakness
        );

        let passed = ExperienceRecord {
            ok: true,
            grade: "tests passed".to_string(),
            ..failed.clone()
        };
        assert!(
            detector.assess(&passed).is_none(),
            "a passed episode is not salient — don't spend a class on what she already knows"
        );
    }

    // What this catches: the afferent→efferent nerve projects ONLY salient,
    // test-graded episodes onto the remediation teach set — a passed task is
    // dropped (not a gap), and a failed-but-untestable task is dropped from
    // REMEDIATION (its correction couldn't be validated — that path is expansion,
    // not genome/teach). This is the exact filter that turns "she failed these"
    // into "teach corrections for these".
    #[test]
    fn salient_teach_set_keeps_only_failed_and_testable() {
        let failed_testable = ExperienceRecord {
            task: eval_task("keep-me", true),
            ok: false,
            grade: "no match".to_string(),
            answer: String::new(),
            world_state: String::new(),
            room_updates: Default::default(),
            acts: 8,
            source: ExperienceSource::Eval,
            room: Some(uuid::Uuid::from_u128(7)),
            teammates: Vec::new(),
            team_role: None,
        };
        let passed_testable = ExperienceRecord {
            task: eval_task("passed", true),
            ok: true,
            grade: "tests passed".to_string(),
            answer: "ok".to_string(),
            world_state: String::new(),
            room_updates: Default::default(),
            acts: 1,
            source: ExperienceSource::Eval,
            room: Some(uuid::Uuid::from_u128(7)),
            teammates: Vec::new(),
            team_role: None,
        };
        let failed_untestable = ExperienceRecord {
            task: eval_task("no-test", false),
            ok: false,
            grade: "no match".to_string(),
            answer: String::new(),
            world_state: String::new(),
            room_updates: Default::default(),
            acts: 2,
            source: ExperienceSource::Eval,
            room: Some(uuid::Uuid::from_u128(7)),
            teammates: Vec::new(),
            team_role: None,
        };

        let set = salient_teach_set(
            &[failed_testable, passed_testable, failed_untestable],
            &ErrorSalience,
        );

        assert_eq!(
            set.len(),
            1,
            "only the failed, test-graded task feeds remediation"
        );
        assert_eq!(set[0].id, "keep-me");
    }

    // What this catches: the UNIFICATION — a lived turn enters the SAME experience
    // stream as an eval episode (untruncated answer, world-state, effort all
    // retained, one being), yet because it carries NO ground-truth `test`, it can
    // never leak into the objective-remediation set. A cleanly-settled lived turn
    // is ok=true; an infra-faulted one is ok=false — but NEITHER becomes a
    // remediation teach task (no test to validate a correction against). Lived
    // experience grows the being via the expansion path, never by corrupting the
    // grade-driven loop. Regression for the "one being, one stream" spine
    // ([[lived-and-eval-experience-are-one-stream-one-being]]).
    #[test]
    fn from_lived_turn_joins_the_stream_but_never_the_remediation_set() {
        let long_answer = "y".repeat(500); // same anti-truncation guarantee as eval
        let settled = settled_speaking(&long_answer, 5, "answered in #general after code/search");

        let lived =
            ExperienceRecord::from_lived_turn("what does build_workspace_cycle do?", &settled);

        // Same rich record shape as from_eval — the being is not stripped in either context.
        assert_eq!(
            lived.answer.len(),
            500,
            "lived answer survives whole, like eval"
        );
        assert_eq!(lived.acts, 5, "lived effort is retained");
        assert!(
            lived.world_state.contains("code/search"),
            "lived trajectory retained"
        );
        assert!(
            lived.ok,
            "a cleanly-settled lived turn is ok (no infra fault) — NOT a correctness claim"
        );
        assert_eq!(lived.task.prompt, "what does build_workspace_cycle do?");
        assert!(
            lived.task.test.is_none(),
            "a lived turn has no objective grader"
        );

        // ── The PRODUCER, which is the half that was dead ────────────────────────
        // `from_lived_turn` had zero production callers, so the experience stream was
        // fed ONLY by graded bench cards: a citizen could hold a thousand real
        // conversations and her stream stayed empty. This asserts the append actually
        // reaches HER OWN stream at the canonical citizen path — the wire, not just
        // the record's shape (the shape was always fine; nothing called it).
        let root = tempfile::tempdir().expect("tempdir");
        let peer = crate::identity::PeerId::from_u128(0x90e758b2_0000_4000_8000_000000000002);
        assert!(
            load_experiences(&crate::identity::citizen_peer_dir(root.path(), peer)).is_empty(),
            "a fresh citizen's stream starts empty"
        );

        record_lived_turn(root.path(), peer, &settled);

        let stream = load_experiences(&crate::identity::citizen_peer_dir(root.path(), peer));
        assert_eq!(stream.len(), 1, "the lived turn reached her stream");
        // The stimulus is the outcome's OWN world_state, never a separately-passed
        // string: the record therefore cannot describe a stimulus the turn did not
        // actually perceive. (The first wiring passed only the inbound message text
        // while the mind had settled over the whole rendered burst — the two could
        // disagree, and did.)
        assert_eq!(
            stream[0].task.prompt, settled.world_state,
            "the recorded stimulus IS what she deliberated over"
        );
        assert!(
            stream[0].task.test.is_none(),
            "still no objective grader — the append must not invent one"
        );
        // It lands where the DRAIN reads, keyed by her identity — producer and consumer
        // cannot disagree about the path because both go through `citizen_peer_dir`.
        assert!(
            experience_stream_path(&crate::identity::citizen_peer_dir(root.path(), peer)).exists()
        );

        // Two turns append, never overwrite — a stream, not a slot.
        record_lived_turn(root.path(), peer, &settled);
        assert_eq!(
            load_experiences(&crate::identity::citizen_peer_dir(root.path(), peer)).len(),
            2
        );

        // A lived turn that died on a serving fault: ok=false, honest grade — but STILL untestable.
        let faulted =
            SettleOutcome::infra_failure(uuid::Uuid::from_u128(7), "lane 58057 refused qwen3");
        let lived_fault = ExperienceRecord::from_lived_turn("ping", &faulted);
        assert!(
            !lived_fault.ok,
            "an inference-faulted lived turn is not ok (same honesty as eval)"
        );
        assert!(
            lived_fault.grade.contains("inference fault"),
            "the fault cause is named in the grade"
        );

        // NON-CONVERGENCE: the drive-loop ran out its action budget mid-Act and never
        // reached a verdict — the lived analog of the eval's "did not finish". A
        // structural fact of the outcome (decision is still Act), not a threshold —
        // ok=false with NO magic-number struggle constant. ErrorSalience selects it.
        let unconverged = SettleOutcome {
            decision: Decision::Act {
                calls: Vec::new(),
                intent: "re-run the failing test".into(),
            },
            spoken: None,
            acts: 8,
            world_state: "budget exhausted after 8 acts".into(),
            room_updates: Default::default(),
            metrics: TurnMetrics::default(),
            failure: None,
            deferred_before_dispatch: false,
        capacity_refusal: None,
            touched_paths: Vec::new(),
            room: uuid::Uuid::from_u128(7),
            generation_receipts: Vec::new(),
            turn_acts: Vec::new(),
        };
        let lived_stuck = ExperienceRecord::from_lived_turn("fix the build", &unconverged);
        assert!(
            !lived_stuck.ok,
            "a lived turn that never converged is not ok — the honest struggle signal"
        );
        assert!(
            lived_stuck.grade.contains("did not converge"),
            "non-convergence is named, not faked as a threshold"
        );
        assert!(
            ErrorSalience.assess(&lived_stuck).is_some(),
            "ErrorSalience selects the non-converged lived turn with NO new detector — one detector, honest data"
        );

        // THE SAFETY GUARANTEE: none of the salient lived records can reach remediation
        // curriculum, because salient_teach_set gates on test.is_some() and a lived turn
        // has none. Salient lived experience feeds EXPANSION only.
        let set = salient_teach_set(&[lived, lived_fault, lived_stuck], &ErrorSalience);
        assert!(
            set.is_empty(),
            "lived turns feed EXPANSION, never grade-driven remediation — the objective loop stays uncorrupted"
        );
    }

    /// A shared lesson as BigMama's `memory/share` (#2025) builds it: a MemoryRecord
    /// with `memory_type: "shared"`, `context.shared_by`, `source: "shared:<from>"`.
    fn shared_lesson(from: &str, scope: &str, content: &str) -> MemoryRecord {
        MemoryRecord {
            id: "lesson-1".to_string(),
            persona_id: "recipient".to_string(),
            memory_type: "shared".to_string(),
            content: content.to_string(),
            context: serde_json::json!({ "shared_by": from, "scope": scope }),
            timestamp: "2026-07-26T00:00:00Z".to_string(),
            importance: 0.7,
            access_count: 0,
            tags: vec![scope.to_string(), format!("shared-from:{from}")],
            related_to: Vec::new(),
            source: Some(format!("shared:{from}")),
            last_accessed_at: None,
            layer: None,
            relevance_score: None,
            origin_node: None,
            origin_seq: None,
        }
    }

    // What this catches: THE THIRD AXIS. A lesson another agent hands over (BigMama's
    // memory/share #2025) enters the SAME experience stream as lived + eval — one
    // being that learns from everything it DOES and everything it's TOLD. Its salience
    // is NOT ErrorSalience (a received lesson is knowledge, ok=true, not a gap) but
    // ReceivedSalience, where provenance IS the signal (no proxy, no threshold). And by
    // the SAME test.is_some() gate as a lived turn, a received lesson has no ground-truth
    // test → it feeds expansion, never grade-driven remediation. Producer lane (share)
    // meets consumer lane (this) — the convergence.
    #[test]
    fn from_shared_lesson_is_the_third_axis_received_salience_never_remediation() {
        let record = shared_lesson(
            "BigMama",
            "continuum",
            "the call room IS the airc room — never mint a rogue call_id",
        );
        let lesson = ExperienceRecord::from_shared_lesson(&record);

        // Same stream, tagged Received — the being learns from what it's told, not just what it does.
        assert_eq!(lesson.source, ExperienceSource::Received);
        assert!(
            lesson.ok,
            "a received lesson is knowledge to integrate, not a failure"
        );
        assert_eq!(
            lesson.answer, record.content,
            "the lesson content is the teaching material"
        );
        assert!(
            lesson.grade.contains("BigMama"),
            "the teacher is named — received, not derived"
        );
        assert!(
            lesson.task.test.is_none(),
            "a received lesson has no ground-truth test"
        );

        // ReceivedSalience selects it (provenance IS the signal); ErrorSalience does NOT
        // (it isn't a gap — ok=true). Two complementary detectors, one stream.
        let recv = ReceivedSalience
            .assess(&lesson)
            .expect("a shared lesson is salient by provenance");
        assert_eq!(recv.kind, SalienceKind::Received);
        assert!(
            ErrorSalience.assess(&lesson).is_none(),
            "a received lesson is not an error/gap"
        );

        // A lived/eval failure is NOT received — ReceivedSalience must stay silent on it
        // (the axis tag is honest, not a catch-all).
        let failed_eval = ExperienceRecord {
            task: eval_task("rev-x", true),
            ok: false,
            grade: "no match".to_string(),
            answer: String::new(),
            world_state: String::new(),
            room_updates: Default::default(),
            acts: 2,
            source: ExperienceSource::Eval,
            room: Some(uuid::Uuid::from_u128(7)),
            teammates: Vec::new(),
            team_role: None,
        };
        assert!(
            ReceivedSalience.assess(&failed_eval).is_none(),
            "ReceivedSalience fires ONLY on received provenance"
        );

        // SAFETY GUARANTEE holds for the third axis too: no test → never remediation.
        let set = salient_teach_set(&[lesson], &ReceivedSalience);
        assert!(
            set.is_empty(),
            "received lessons feed EXPANSION, never grade-driven remediation"
        );
    }

    // What this catches: the composite selects the whole being's curriculum from all
    // three axes in ONE pass — an eval failure (Error), a received lesson (Received),
    // and it drops what nobody flags (a passed eval). This is the single detector the
    // dream consolidation runs over the unified stream: gather lived+eval+received,
    // sweep once, get everything worth learning from. Priority order means Error wins
    // the naming on a record that is both a gap and received.
    #[test]
    fn any_salience_selects_the_whole_being_curriculum_in_one_pass() {
        let detector = AnySalience::all_axes();

        let eval_failure = ExperienceRecord {
            task: eval_task("rev-9", true),
            ok: false,
            grade: "error[E0308]".to_string(),
            answer: String::new(),
            world_state: String::new(),
            room_updates: Default::default(),
            acts: 3,
            source: ExperienceSource::Eval,
            room: Some(uuid::Uuid::from_u128(7)),
            teammates: Vec::new(),
            team_role: None,
        };
        let received = ExperienceRecord::from_shared_lesson(&shared_lesson(
            "BigMama",
            "continuum",
            "provenance IS the salience signal",
        ));
        let passed_eval = ExperienceRecord {
            ok: true,
            grade: "tests passed".to_string(),
            ..eval_failure.clone()
        };

        // Error wins the kind (priority), Received fires for the lesson, pass is dropped.
        assert_eq!(
            detector.assess(&eval_failure).unwrap().kind,
            SalienceKind::Error
        );
        assert_eq!(
            detector.assess(&received).unwrap().kind,
            SalienceKind::Received
        );
        assert!(
            detector.assess(&passed_eval).is_none(),
            "a passed eval is nobody's gap and nobody's lesson"
        );

        // Empty composite is never salient (honest degenerate case).
        assert!(AnySalience::of(vec![]).assess(&eval_failure).is_none());
    }

    // What this catches: the DREAM'S ROUTING — one salient sweep of the unified stream
    // splits cleanly into two efferent organs. A testable eval failure → remediation
    // (test-validated teacher); an untestable lived turn AND an untestable received
    // lesson → expansion (benchmark-validated teacher); a passed eval → neither. The two
    // partitions are complementary and exhaustive over the salient set — no salient
    // episode is dropped, none is double-routed. This is the routing that lets the dream
    // consolidate the whole being's experience without corrupting the graded loop.
    #[test]
    fn remediation_and_expansion_partition_the_salient_stream_exhaustively() {
        let any = AnySalience::all_axes();

        let testable_failure = ExperienceRecord {
            task: eval_task("exam-fail", true),
            ok: false,
            grade: "error[E0308]".to_string(),
            answer: String::new(),
            world_state: String::new(),
            room_updates: Default::default(),
            acts: 4,
            source: ExperienceSource::Eval,
            room: Some(uuid::Uuid::from_u128(7)),
            teammates: Vec::new(),
            team_role: None,
        };
        let stuck = SettleOutcome {
            decision: Decision::Act {
                calls: Vec::new(),
                intent: "keep trying".into(),
            },
            spoken: None,
            acts: 8,
            world_state: String::new(),
            room_updates: Default::default(),
            metrics: TurnMetrics::default(),
            failure: None,
            deferred_before_dispatch: false,
        capacity_refusal: None,
            touched_paths: Vec::new(),
            room: uuid::Uuid::from_u128(7),
            generation_receipts: Vec::new(),
            turn_acts: Vec::new(),
        };
        let lived = ExperienceRecord::from_lived_turn("a hard live question", &stuck);
        let received = ExperienceRecord::from_shared_lesson(&shared_lesson(
            "BigMama",
            "continuum",
            "the being learns from what it's told",
        ));
        let passed = ExperienceRecord {
            ok: true,
            grade: "tests passed".to_string(),
            ..testable_failure.clone()
        };

        let batch = [testable_failure, lived, received, passed];

        // Remediation: ONLY the testable failure (objective grader can validate the fix).
        let remediation = salient_teach_set(&batch, &any);
        assert_eq!(
            remediation.len(),
            1,
            "only the testable failure is remediation-eligible"
        );
        assert_eq!(remediation[0].id, "exam-fail");

        // Expansion: the untestable-but-salient lived + received (benchmark-validated).
        let expansion = expansion_teach_set(&batch, &any);
        assert_eq!(
            expansion.len(),
            2,
            "the lived turn and the received lesson are expansion-eligible"
        );
        assert!(expansion
            .iter()
            .any(|r| r.source == ExperienceSource::Lived));
        assert!(expansion
            .iter()
            .any(|r| r.source == ExperienceSource::Received));

        // Exhaustive + disjoint over the salient set: 1 + 2 = 3 salient, the pass in neither.
        assert_eq!(
            remediation.len() + expansion.len(),
            3,
            "every salient episode is routed exactly once; the pass is dropped"
        );
    }

    // What this catches: the lived record keeps EVERY room update the act driver perceived
    // (teaching a failed episode with the same complete input) and still reads legacy JSON
    // that predates `room_updates`. Moved here from act_observe's credit test when learning
    // became its own crate above cognition; the driver half stays there.
    #[test]
    fn lived_record_keeps_every_room_update_and_reads_legacy_json() {
        use crate::cognition::conversation::IncomingMessage;
        use crate::cognition::curriculum::LivedExpansionSynthesizer;
        use std::sync::Arc;
        use uuid::Uuid;

        let room = Uuid::new_v4();
        let colleague = Uuid::new_v4();
        let update = |lamport: u64, room_id: Uuid, text: &str| IncomingMessage {
            work: None,
            media: Vec::new(),
            event_id: Uuid::new_v4(),
            lamport,
            peer_id: colleague,
            text: text.into(),
            room_id,
        };
        let updates = vec![
            update(1, room, "Asha, the review target moved to src/new.rs; preserve the regression."),
            update(1, Uuid::new_v4(), "Another activity has a question when you have attention available."),
            update(2, Uuid::new_v4(), "Asha, newer evidence arrived during your second read; inspect src/final.rs."),
        ];
        let task = "Review the original src/lib.rs change and report a reproducible defect.";
        let mut outcome = settled_speaking("done", 2, task);
        outcome.room = room;
        outcome.room_updates = Arc::new(updates.iter().cloned().map(Arc::new).collect());
        let lived = ExperienceRecord::from_lived_turn(task, &outcome);
        let encoded = serde_json::to_string(&lived).unwrap();
        let mut restored: ExperienceRecord = serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored.task.prompt, task);
        assert_eq!(restored.world_state, task);
        assert_eq!(restored.room, Some(room));
        for update in &updates {
            assert!(restored.room_updates.contains(update));
            assert!(restored
                .training_prompt()
                .contains(&update.render_room_update()));
        }
        // A later failed episode is taught with the same complete input,
        // through the ordinary lived-curriculum selector.
        restored.ok = false;
        let teaching = LivedExpansionSynthesizer::new().select(std::slice::from_ref(&restored));
        assert_eq!(teaching, vec![restored.training_prompt()]);
        let mut legacy: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        legacy.as_object_mut().unwrap().remove("room_updates");
        assert!(serde_json::from_value::<ExperienceRecord>(legacy)
            .unwrap()
            .room_updates
            .is_empty());
    }

    mod durable_stream {
        use super::*;

        // what this catches: the #319 spine — a kanban-graded outcome appended to
        // the per-persona stream must survive a write→load roundtrip INTACT (her
        // artifact, the real check output, the verdict, the source tag), and a
        // corrupt line must cost exactly itself, never the rest of the stream.
        #[test]
        fn append_load_roundtrip_survives_a_corrupt_line() {
            let dir = std::env::temp_dir().join(format!("exp-stream-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();

            let task = EvalTask {
                id: "sum_evens".into(),
                prompt: "Write fn sum_evens…".into(),
                test: Some("assert_eq!(sum_evens(&[2,3]), 2);".into()),
                ..EvalTask::default()
            };
            let fail = ExperienceRecord::from_kanban_grade(
                &task,
                "fn sum_evens(n:&[i32])->i32{0}",
                false,
                "assertion failed: left 0, right 2",
            );
            let pass = ExperienceRecord::from_kanban_grade(
                &task,
                "fn sum_evens…correct",
                true,
                "ALL ASSERTIONS PASSED",
            );
            append_experience(&dir, &fail).unwrap();
            // A partial write / schema drift in the middle of the stream:
            {
                use std::io::Write;
                let mut f = std::fs::OpenOptions::new()
                    .append(true)
                    .open(experience_stream_path(&dir))
                    .unwrap();
                writeln!(f, "{{\"not\": \"an experience record\"").unwrap();
            }
            append_experience(&dir, &pass).unwrap();

            let loaded = load_experiences(&dir);
            assert_eq!(
                loaded.len(),
                2,
                "both real records load; the corrupt line costs only itself"
            );
            assert!(!loaded[0].ok && loaded[0].grade.contains("assertion failed"));
            assert_eq!(loaded[0].answer, "fn sum_evens(n:&[i32])->i32{0}");
            assert!(matches!(loaded[0].source, ExperienceSource::Eval));
            assert!(loaded[1].ok);
            let _ = std::fs::remove_dir_all(&dir);
        }

        // what this catches: a kanban-grade record must be REMEDIATION-ELIGIBLE —
        // its task keeps the held-out test (so the teacher can re-pose + objectively
        // re-grade), and a FAILED grade is selected by the error detector while a
        // PASS is not. This is the exact contract that lets graded board work feed
        // the existing salience→curriculum seam with zero changes to it.
        #[test]
        fn kanban_grade_failure_enters_the_teach_set() {
            let task = EvalTask {
                id: "fib".into(),
                prompt: "Write fn fib…".into(),
                test: Some("assert_eq!(fib(10), 55);".into()),
                ..EvalTask::default()
            };
            let fail = ExperienceRecord::from_kanban_grade(
                &task,
                "fn fib(n:u32)->u64{n as u64}",
                false,
                "left 10, right 55",
            );
            let pass = ExperienceRecord::from_kanban_grade(
                &task,
                "fn fib…",
                true,
                "ALL ASSERTIONS PASSED",
            );
            let teach = salient_teach_set(&[fail, pass], &ErrorSalience);
            assert_eq!(
                teach.len(),
                1,
                "the failure is remediable; the pass teaches nothing"
            );
            assert_eq!(teach[0].id, "fib");
        }

        // what this catches: the drain's dedup contract — a later PASS retires the
        // task's earlier failure (no re-teaching solved gaps), a later failure
        // supersedes an earlier one, and only the LATEST-failed tasks survive into
        // the teach set. Without this, every historical failure re-enters training
        // forever, even after the citizen learned it.
        #[test]
        fn a_later_pass_retires_the_failure_from_the_teach_set() {
            let t = |id: &str| EvalTask {
                id: id.into(),
                prompt: format!("do {id}"),
                test: Some("assert!(true);".into()),
                ..EvalTask::default()
            };
            let records = vec![
                ExperienceRecord::from_kanban_grade(&t("fib"), "v1", false, "wrong"),
                ExperienceRecord::from_kanban_grade(&t("sum"), "v1", false, "wrong"),
                ExperienceRecord::from_kanban_grade(&t("fib"), "v2", true, "ALL PASSED"),
            ];
            let latest = latest_per_task(&records);
            assert_eq!(latest.len(), 2, "one record per task survives");
            let teach = salient_teach_set(&latest, &ErrorSalience);
            assert_eq!(teach.len(), 1, "fib was learned; only sum still teaches");
            assert_eq!(teach[0].id, "sum");
        }

        // what this catches: a missing stream is an EMPTY stream (a fresh mind),
        // never an error — the drain must not fail a persona who has no history.
        #[test]
        fn missing_stream_is_empty_not_an_error() {
            let dir = std::env::temp_dir().join(format!("exp-none-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            assert!(load_experiences(&dir).is_empty());
        }
    }
