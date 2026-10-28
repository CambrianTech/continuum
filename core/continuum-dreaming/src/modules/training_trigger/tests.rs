#![cfg(test)]
    use super::*;

    // what this catches (card de9e1342): a capacity refusal decoded as a retry (re-planning
    // every tick) or as recovery (left for a human), or parked with no wake key to wake by.
    #[test]
    fn a_capacity_refusal_decodes_as_parked_only_with_its_wait() {
        use crate::genome::fine_tuning::types::CapacityWait;
        use crate::inference::planning::training_plan::NeedSource;
        let wait = CapacityWait { window: 16_128, need_bytes: 18 << 30, need_source: NeedSource::LowerBound, governed_free: 4 << 30, engine_stamp: Some("abc:cuda".into()) };
        let refused = |wait: Option<&CapacityWait>| {
            let mut v = serde_json::json!({"success": false, "error": "waiting for capacity", "errorKind": "CapacityParked"});
            if let Some(w) = wait {
                v["capacityWait"] = serde_json::to_value(w).expect("test: encode");
            }
            v
        };
        assert!(matches!(decode_job_create(refused(Some(&wait))), Err(DispatchFailure::Parked(w)) if w == wait));
        assert!(matches!(decode_job_create(refused(None)), Err(DispatchFailure::Uncertain(_))), "no wake key, no park");
    }

    // what this catches (Cormac on #4537): an orphan whose engine was silent at boot asked
    // once and dropped, holding serving off its run for the engine's life; or, worse, resumed
    // (a second POST into an engine still training it). Uncertain on the first tick holds it;
    // it is not due again before its wait; on the next ask an Attached answer registers the
    // SAME job. Neither tick is a Resume, the only step that reaches genome/job-create.
    #[test]
    fn an_uncertain_orphan_is_held_then_reattached_and_never_resumed() {
        use crate::genome::fine_tuning::{job_board::OrphanedJob, JobHandle, ReattachOutcome};
        let state = TrainingTriggerState::new();
        let job = Uuid::new_v4();
        let orphan = OrphanedJob {
            local_id: job,
            trigger_dispatch_id: None,
            provider_id: "engine-local".into(),
            persona_id: Uuid::new_v4(),
            persona_name: "Kimi".into(),
            base_model: "qwen3.8-27b".into(),
            trait_kind: "code".into(),
            eval_set: None,
        };
        let t0 = 1_000_000;
        let first = orphan_step(&orphan, Ok(ReattachOutcome::Uncertain { reason: "no answer".into() }));
        assert!(matches!(first, OrphanStep::Hold), "tick 1: uncertain holds");
        state.hold_orphan(orphan, t0);
        assert!(state.take_due_orphans(t0 + 1).is_empty(), "not asked again before its wait");
        let mut due = state.take_due_orphans(t0 + HELD_ORPHAN_REASK_MS);
        assert_eq!(due.len(), 1, "asked again once its wait is up");
        let again = due.remove(0);
        let handle = JobHandle { provider_id: "engine-local".into(), provider_job_id: job.to_string(), local_id: job };
        let OrphanStep::Register(watched) = orphan_step(&again, Ok(ReattachOutcome::Attached { handle })) else {
            panic!("tick 2: an attached run is registered, never resumed");
        };
        assert_eq!(watched.handle.local_id, job, "the same job, watched again");
        assert!(state.take_due_orphans(u64::MAX).is_empty(), "a registered job is no longer held");
        assert!(matches!(orphan_step(&again, Err("executor down".into())), OrphanStep::Hold), "no answer holds too");
        assert!(matches!(orphan_step(&again, Ok(ReattachOutcome::NotResident)), OrphanStep::Resume), "positive control");
    }

    // What this catches (278afa6c): malformed command output cannot turn an
    // uncertain provider creation into a safe retry. Exercise the actual decoder.
    #[test]
    fn job_create_requires_explicit_typed_refusal_evidence() {
        use serde_json::json;
        for response in [
            json!({}),
            json!({"success": "false"}),
            json!({"success": false}),
            json!({"success": false, "error": "bad", "errorKind": 7}),
            json!({"success": false, "error": "bad", "errorKind": null}),
            json!({"success": false, "error": "bad", "errorKind": "Transient"}),
            // A local trainer failure can follow a trainer that started work: never a retry.
            json!({"success": false, "error": "bad", "errorKind": "LocalTrainerFailed"}),
            json!({"success": true}),
        ] {
            assert!(matches!(
                decode_job_create(response),
                Err(DispatchFailure::Uncertain(_))
            ));
        }
        // what this catches (Cormac on #4794): a refusal that created no job is retryable,
        // never Uncertain; Uncertain froze the bucket in RecoveryRequired with no journal
        // row to recover from.
        for kind in [
            None,
            Some("InvalidRequest"),
            Some("MissingCredentials"),
            Some("ProviderRejected"),
            Some(crate::commands::genome::job_create::ERROR_KIND_REUSE_PULL_FAILED),
            Some(crate::commands::genome::job_create::ERROR_KIND_REUSE_ADOPT_FAILED),
            Some(crate::commands::genome::job_create::ERROR_KIND_TRIAL_FILE_UNREADABLE),
            // card a9bc56ea: a trainer that refused before creating anything (IntelMac, a
            // missing MLX interpreter wedged Sigurd's bucket in RecoveryRequired).
            Some(crate::commands::genome::job_create::ERROR_KIND_TRAINER_UNAVAILABLE),
        ] {
            let mut response = json!({"success": false, "error": "explicit refusal"});
            if let Some(kind) = kind {
                response["errorKind"] = json!(kind);
            }
            assert!(matches!(
                decode_job_create(response),
                Err(DispatchFailure::Retryable(_))
            ));
        }
    }

    // what this catches: every migrated verb now fails loud through the legacy
    // path, naming itself + pointing at the typed registry (no silent success that
    // would mask a routing regression). The behavioral tests live in the command
    // files (commands/training_trigger/*).
    #[tokio::test]
    async fn migrated_arms_fail_loud() {
        let module = TrainingTriggerModule::new();
        for command in [
            "genome/training-trigger/submit",
            "genome/training-trigger/flush",
            "genome/training-trigger/status",
        ] {
            let err = module
                .handle_command(command, Value::Null)
                .await
                .expect_err("migrated arm must fail loud");
            assert!(err.contains("migrated"), "for {command}: {err}");
            assert!(err.contains(command), "for {command}: {err}");
        }
    }

    // what this catches: the module contributes the three dep-holding verbs to the
    // typed object map (sharing its one state). A regression that drops the
    // `commands()` override — leaving them unroutable — is caught.
    #[test]
    fn contributes_the_typed_training_trigger_commands() {
        let module = TrainingTriggerModule::new();
        let names: Vec<&str> = module.commands().iter().map(|c| c.name()).collect();
        assert!(names.contains(&"genome/training-trigger/submit"));
        assert!(names.contains(&"genome/training-trigger/flush"));
        assert!(names.contains(&"genome/training-trigger/status"));
    }

    // what this catches: an unmigrated/unknown verb still errors (not a panic, not
    // a silent ok).
    #[tokio::test]
    async fn unknown_command_errors() {
        let module = TrainingTriggerModule::new();
        let result = module
            .handle_command("genome/training-trigger/nope", Value::Null)
            .await;
        assert!(result.is_err());
    }

    // regression for card 244757bc (BigMama, 2026-09-26): three jobs in the ledger ended
    // `killed-by-reboot` — the consumer deploys six times a day — and nothing resumed
    // them, while each job directory still held the full request.json.
    // what this catches: the resumable input is the job directory's own request.json,
    // re-created as job-create params under a fresh dispatch id on the same provider;
    // a directory with nothing in it resumes nothing; the directory rule matches
    // `job_dir_for` for the ledger's fields.
    #[test]
    fn a_dead_jobs_request_json_is_its_resume_and_an_empty_directory_is_none() {
        let dir = tempfile::tempdir().expect("test: tempdir");
        let dispatch = Uuid::new_v4();
        assert!(read_job_request(dir.path()).and_then(|r| resumable_request(r, dir.path(), "cuda-local", dispatch)).is_err(), "nothing on disk");
        // What the CUDA adapter writes: the flattened request plus its admission fields.
        let spec = serde_json::json!({
            "personaId": Uuid::from_u128(7).to_string(),
            "personaName": "Kimi",
            // What the adapter really writes: the TRAINABLE base here, the registry id
            // in canonicalBase. The fixture once wrote the same id in both, so it could
            // not see a resume that fed the HF id back to job-create.
            "baseModel": "Qwen/Qwen3.8-27B",
            "traitKind": "code",
            "dataset": {"examples": [{"prompt": "p", "completion": "c"}], "source": "teacher_synthesized", "validationSplit": 0.1},
            "canonicalBase": "ggml-org/Qwen3.8-27B-GGUF",
            "memoryBytes": 30_000_000_000u64,
            "availableBytes": 6_000_000_000u64,
            "microBatchSize": 1,
            "revision": null
        });
        std::fs::write(dir.path().join("request.json"), spec.to_string()).expect("test: write");
        let params = read_job_request(dir.path()).and_then(|r| resumable_request(r, dir.path(), "cuda-local", dispatch)).expect("test: resumable");
        assert_eq!(params["triggerDispatchId"], serde_json::json!(dispatch.to_string()));
        assert!(
            params.get("preferredProvider").is_none(),
            "a job that never checkpointed is re-chosen, not pinned to the trainer that never ran it"
        );
        assert_eq!(params["personaName"], serde_json::json!("Kimi"));
        assert_eq!(
            params["baseModel"],
            serde_json::json!("ggml-org/Qwen3.8-27B-GGUF"),
            "a resume re-creates the job against the registry id, never the HF base"
        );
        assert_eq!(params["dataset"]["examples"].as_array().map(|e| e.len()), Some(1));
        assert!(params.get("memoryBytes").is_none(), "admission fields are the adapter's, not the request's");
        assert!(params.get("resumeFrom").is_none(), "no checkpoint, no resume point");
        // A checkpoint the dead job finished writing becomes the resume point.
        let ck = dir.path().join("checkpoints").join("step-8");
        std::fs::create_dir_all(&ck).expect("test: checkpoint dir");
        std::fs::write(ck.join("adapter_config.json"), "{}").expect("test: adapter config");
        std::fs::write(dir.path().join("checkpoints").join("LATEST"), "step-8\n").expect("test: pointer");
        let params = read_job_request(dir.path()).and_then(|r| resumable_request(r, dir.path(), "cuda-local", dispatch)).expect("test: resumable");
        assert_eq!(params["resumeFrom"], serde_json::json!(ck.to_string_lossy()), "continues from the latest checkpoint");
        assert_eq!(params["preferredProvider"], serde_json::json!("cuda-local"), "a checkpoint is its trainer's format: pinned");
        let d = default_job_dir("Kimi", "code/owner", Uuid::from_u128(9)).expect("test: a home directory");
        assert!(d.ends_with(std::path::Path::new("Kimi").join("code_owner").join(Uuid::from_u128(9).to_string())), "{d:?}");
    }

    // what this catches (card c2dd200c, Codex on #5550): an orphan whose input could not be read
    // at boot (the trigger's durable state not yet recovered) was dropped as not resumable until
    // the next boot, while a node may deploy rarely. Unreadable-now is Unavailable, held and asked
    // again; no dispatch and no request file is Missing, final.
    #[tokio::test]
    async fn an_orphans_input_unreadable_now_is_unavailable_and_a_missing_one_is_final() {
        let state = TrainingTriggerState::new();
        let dir = tempfile::tempdir().expect("test: tempdir");
        let orphan = crate::genome::fine_tuning::job_board::OrphanedJob {
            local_id: Uuid::from_u128(0x8b46), trigger_dispatch_id: Some(Uuid::from_u128(0xa205)), provider_id: "engine-local".into(),
            persona_id: Uuid::from_u128(7), persona_name: "Kimi".into(), base_model: "ggml-org/Qwen3.8-27B-GGUF".into(),
            trait_kind: "code/owner".into(), eval_set: None,
        };
        assert!(matches!(state.job_input(&orphan, dir.path()).await, Err(JobInputError::Unavailable(_))), "durable state not recovered: asked again");
        let undispatched = crate::genome::fine_tuning::job_board::OrphanedJob { trigger_dispatch_id: None, ..orphan };
        assert!(matches!(state.job_input(&undispatched, dir.path()).await, Err(JobInputError::Missing(_))), "no dispatch and no request file: nothing will ever prove it");
    }
