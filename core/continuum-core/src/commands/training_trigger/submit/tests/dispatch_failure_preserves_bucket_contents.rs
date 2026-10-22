#![cfg(test)]
use super::*;

    // what this catches: dispatch failure preserves the bucket contents — the worst
    // regression in this module would be a failed dispatch silently dropping curated
    // examples. A runtime with NO genome module → genome/job-create unregistered →
    // dispatch fails loud at the executor; the bucket must survive intact so the next
    // submit retains ownership; an untyped execution error stays uncertain.
    #[tokio::test]
    pub(super) async fn dispatch_failure_preserves_bucket_contents() {
        let (trigger, executor, _dir) = build_runtime_trigger_only().await;
        let persona = Uuid::new_v4();

        let json = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(
                    persona,
                    "test-trait",
                    vec![ex("a", "b"), ex("c", "d")],
                    Some(2),
                ),
            )
            .await
            .unwrap();
        assert_eq!(json["success"], false);
        assert_eq!(json["errorKind"], "RecoveryRequired");
        // The two examples must STILL be in the bucket.
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            Some(2)
        );
    }

    // what this catches: different personas have isolated buckets. A regression that
    // keyed on trait_kind alone would mix multiple personas' curricula into one
    // training run.
    #[tokio::test]
    pub(super) async fn different_personas_have_isolated_buckets() {
        let (trigger, executor, _dir) = build_runtime_with_trigger_and_genome().await;
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        let _ = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(a, "shared-trait", vec![ex("a1", "b1")], Some(5)),
            )
            .await
            .unwrap();
        let _ = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(
                    b,
                    "shared-trait",
                    vec![ex("a2", "b2"), ex("c2", "d2")],
                    Some(5),
                ),
            )
            .await
            .unwrap();

        assert_eq!(
            trigger
                .state
                .bucket_example_count(a, "shared-trait", "synthetic"),
            Some(1)
        );
        assert_eq!(
            trigger
                .state
                .bucket_example_count(b, "shared-trait", "synthetic"),
            Some(2)
        );
        assert_eq!(trigger.state.pending_bucket_count(), 2);
    }

    /// VDD — validation-driven tests verifying the conservation invariant: every
    /// example a producer submits ends up in the dispatched training job EXACTLY once,
    /// in original order, with no duplicates or drops across the accumulate-fire
    /// boundary. TDD pins lifecycle; VDD pins example conservation.
    pub(super) mod vdd {
        use super::super::*;
        use crate::genome::fine_tuning::types::TrainingExample;
        use crate::genome::fine_tuning::{
            FineTuningRegistry, RecordingFineTuningAdapter, RECORDING_BASE_PREFIX,
        };
        use crate::modules::training_trigger::TrainingTriggerModule;
        use crate::runtime::CommandExecutor;

        /// Build a runtime where genome/job-create routes to a
        /// RecordingFineTuningAdapter (the substrate's canonical fixture for capturing
        /// dispatched TrainingJobRequests). Returns the trigger + executor + the
        /// shared captures handle so the test body can compare dispatched-vs-submitted
        /// at exact example granularity.
        async fn build_recording_runtime() -> (
            Arc<TrainingTriggerModule>,
            Arc<CommandExecutor>,
            Arc<RecordingFineTuningAdapter>,
            tempfile::TempDir,
        ) {
            let (adapter, dir) = crate::orm::store::fresh_adapter().await;
            let ft_registry = Arc::new(FineTuningRegistry::new());
            let recorder = Arc::new(RecordingFineTuningAdapter::new());
            ft_registry.register(recorder.clone());
            let (trigger, executor) =
                crate::commands::training_trigger::test_support::build_runtime_with_registry(
                    adapter,
                    Some(ft_registry),
                )
                .await;
            (trigger, executor, recorder, dir)
        }

        // What this catches (Kimi received-lesson audit): accepted ownership survives provider refusal, restart/tick, and explicit producer replay.
        #[tokio::test]
        async fn received_lesson_dispatch_failure_restart_and_retry_preserve_one_batch() {
            use crate::commands::training_trigger::test_support::build_runtime_with_registry;
            use crate::genome::fine_tuning::RECORDING_PROVIDER_ID;
            use crate::persona::domain_classifier::DomainClassifier;
            use crate::persona::training_producer::{
                received_submission_id, received_submission_params,
            };
            use crate::runtime::ServiceModule;

            let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
            let providers = Arc::new(FineTuningRegistry::new());
            let (first, executor) =
                build_runtime_with_registry(adapter.clone(), Some(providers)).await;
            let recorder = Arc::new(RecordingFineTuningAdapter::new());
            let persona = Uuid::from_u128(73);
            let base_model = format!("{RECORDING_BASE_PREFIX}-received-retry");
            let classifier = DomainClassifier::new();
            let record: crate::memory::MemoryRecord = serde_json::from_value(serde_json::json!({
                "id": "lesson-73", "persona_id": persona.to_string(), "memory_type": "shared",
                "content": "Read Rust compiler diagnostics and verify the correction with cargo check.",
                "context": { "scope": "rust code", "shared_by": "teacher-9", "original_author": "teacher-9" },
                "timestamp": "2026-09-23T00:00:00Z", "importance": 0.6, "access_count": 0,
                "tags": [], "related_to": [], "source": "shared:teacher-9"
            })).expect("test: persisted received lesson");
            let request = || {
                let mut params = received_submission_params(
                    persona,
                    "recipient",
                    &base_model,
                    &classifier,
                    &record,
                );
                params["minExamples"] = serde_json::json!(1);
                params["preferredProvider"] = serde_json::json!(RECORDING_PROVIDER_ID);
                params
            };
            let original = request();
            let submission_id = received_submission_id(persona, &base_model, &record.id);
            let accepted = executor
                .execute_json("genome/training-trigger/submit", original.clone())
                .await
                .expect("test: accepted then refused dispatch");
            assert_eq!(accepted["success"], false);
            assert_eq!(accepted["errorKind"], "DispatchFailed", "{accepted}");
            assert_eq!(
                accepted["acceptance"]["submissionId"],
                submission_id.to_string()
            );
            assert_eq!(accepted["acceptance"]["replayed"], false);
            assert!(accepted.get("jobHandle").is_none());
            let status = executor
                .execute_json("genome/training-trigger/status", serde_json::json!({}))
                .await
                .expect("test: refused dispatch status");
            assert_eq!(
                status["buckets"]
                    .as_array()
                    .expect("test: bucket rows")
                    .len(),
                1
            );
            let dispatch_id = status["buckets"][0]["dispatchId"]
                .as_str()
                .expect("test: durable dispatch identity")
                .to_string();
            assert_eq!(status["buckets"][0]["dispatch"]["state"], "retryable");
            assert_eq!(status["buckets"][0]["examplesPending"], 1);
            first.shutdown().await.expect("test: owner shutdown");
            drop(executor);
            drop(first);

            let providers = Arc::new(FineTuningRegistry::new());
            let (next, executor) =
                build_runtime_with_registry(adapter, Some(providers.clone())).await;
            next.tick()
                .await
                .expect("test: restore and retry owned dispatch");
            let status = executor
                .execute_json("genome/training-trigger/status", serde_json::json!({}))
                .await
                .expect("test: recovered status");
            assert_eq!(
                status["buckets"]
                    .as_array()
                    .expect("test: bucket rows")
                    .len(),
                1
            );
            assert_eq!(status["buckets"][0]["dispatchId"], dispatch_id);
            assert_eq!(status["buckets"][0]["dispatch"]["state"], "retryable");
            assert_eq!(status["buckets"][0]["examplesPending"], 1);
            // Submit and tick attempted dispatch; neither reached a provider or executed training.
            assert_eq!(recorder.captured_job_count(), 0);

            providers.register(recorder.clone());
            let replay = request();
            assert_eq!(
                replay, original,
                "restart reconstructs the same immutable payload"
            );
            let dispatched = executor
                .execute_json("genome/training-trigger/submit", replay.clone())
                .await
                .expect("test: explicit received replay");
            assert_eq!(dispatched["success"], true, "{dispatched}");
            assert_eq!(dispatched["outcome"], "JobDispatched");
            assert_eq!(
                dispatched["acceptance"]["submissionId"],
                submission_id.to_string()
            );
            assert_eq!(dispatched["acceptance"]["replayed"], true);
            let again = executor
                .execute_json("genome/training-trigger/submit", replay)
                .await
                .expect("test: replay after successful dispatch");
            assert_eq!(again["outcome"], "AlreadyAccepted", "{again}");
            assert_eq!(
                again["acceptance"]["submissionId"],
                submission_id.to_string()
            );
            assert_eq!(again["acceptance"]["replayed"], true);
            assert_eq!(next.state.pending_bucket_count(), 0);
            assert_eq!(
                recorder.captured_job_count(),
                1,
                "one successful fixture dispatch, not one attempt"
            );
            assert_eq!(recorder.captured_example_count(), 1);
            {
                let captures = recorder.captures();
                let captured = captures.lock().expect("test: captured request");
                assert_eq!(captured[0].persona_id, persona);
                assert_eq!(
                    serde_json::to_value(&captured[0].dataset.examples)
                        .expect("test: example receipt"),
                    original["examples"]
                );
                let metadata = captured[0].dataset.examples[0]
                    .metadata
                    .as_ref()
                    .expect("test: provenance");
                assert_eq!(metadata["memoryRecordId"], record.id);
                assert_eq!(metadata["shared_by"], "teacher-9");
                assert!(metadata.get("cardId").is_none());
            }
            next.shutdown()
                .await
                .expect("test: recovered owner shutdown");
        }

        // what this VDD catches: every example submitted across N submits appears
        // EXACTLY ONCE in a dispatched job — no duplicates, no drops, in submission
        // order. The RecordingFineTuningAdapter captures the dispatched TrainingDataset
        // so we compare prompt-by-prompt against what was submitted.
        #[tokio::test]
        async fn submitted_examples_flow_through_dispatch_intact() {
            let (trigger, executor, recorder, _dir) = build_recording_runtime().await;
            let persona = Uuid::new_v4();
            let n = 8;

            let examples: Vec<TrainingExample> = (0..n)
                .map(|i| ex(&format!("prompt-{i}"), &format!("completion-{i}")))
                .collect();
            let mut params = submit_params(persona, "vdd-trait", examples.clone(), Some(n as u32));
            // Route deterministically to the recorder (matches base_model prefix).
            params.as_object_mut().unwrap().insert(
                "baseModel".into(),
                Value::String(format!("{RECORDING_BASE_PREFIX}-vdd")),
            );

            let json = executor
                .execute_json("genome/training-trigger/submit", params)
                .await
                .unwrap();
            assert_eq!(
                json["success"], true,
                "VDD: submit must succeed; got {json}"
            );
            assert_eq!(
                json["examplesUsed"], n,
                "VDD: every submitted example must be in the dispatched job"
            );

            // Bucket cleared exactly once — zero leftover.
            assert_eq!(
                trigger.state.bucket_example_count(
                    persona,
                    "vdd-trait",
                    &format!("{RECORDING_BASE_PREFIX}-vdd")
                ),
                None,
                "VDD: bucket must be fully drained, no leftover"
            );
            assert_eq!(trigger.state.pending_bucket_count(), 0);

            // CONSERVATION CHECK — exactly one job dispatched, exactly n examples
            // captured, examples match submitted set in ORDER.
            assert_eq!(
                recorder.captured_job_count(),
                1,
                "VDD: exactly one job dispatched"
            );
            assert_eq!(
                recorder.captured_example_count(),
                n,
                "VDD: dispatched dataset must carry exactly the {n} examples submitted"
            );

            let captures = recorder.captures();
            let guard = captures.lock().unwrap();
            let dispatched_examples = &guard[0].dataset.examples;
            assert_eq!(dispatched_examples.len(), examples.len());
            for (i, (sub, disp)) in examples.iter().zip(dispatched_examples.iter()).enumerate() {
                assert_eq!(
                    sub.prompt, disp.prompt,
                    "VDD: example {i} prompt mismatch — submission order violated or example replaced"
                );
                assert_eq!(sub.completion, disp.completion);
            }
        }

        // what this VDD catches: conservation across MULTIPLE submits to the same
        // bucket. Accumulated submits, when finally drained-and-dispatched, carry every
        // example in INSERTION ORDER.
        #[tokio::test]
        async fn multi_submit_accumulation_preserves_order_through_dispatch() {
            let (_trigger, executor, recorder, _dir) = build_recording_runtime().await;
            let persona = Uuid::new_v4();
            let trait_kind = "multi-submit-vdd";
            let base_model = format!("{RECORDING_BASE_PREFIX}-multi");

            // 4 submits × 3 examples = 12. Threshold 12 → fires only at the last submit.
            let mut all_submitted: Vec<TrainingExample> = Vec::new();
            for batch in 0..4 {
                let exs: Vec<TrainingExample> = (0..3)
                    .map(|i| ex(&format!("b{batch}-p{i}"), &format!("b{batch}-c{i}")))
                    .collect();
                all_submitted.extend(exs.clone());
                let mut params = submit_params(persona, trait_kind, exs, Some(12));
                params
                    .as_object_mut()
                    .unwrap()
                    .insert("baseModel".into(), Value::String(base_model.clone()));
                let outcome = executor
                    .execute_json("genome/training-trigger/submit", params)
                    .await
                    .unwrap();
                assert_eq!(outcome["success"], true, "batch {batch}: {outcome}");
                assert!(outcome["acceptance"]["submissionId"].is_string());
                assert_eq!(
                    outcome["outcome"],
                    if batch == 3 {
                        "JobDispatched"
                    } else {
                        "BatchAppended"
                    },
                    "only the fourth accepted batch crosses the threshold"
                );
            }

            // Exactly one job dispatched (only the 4th submit crossed threshold).
            assert_eq!(recorder.captured_job_count(), 1);

            let captures = recorder.captures();
            let guard = captures.lock().unwrap();
            let dispatched = &guard[0].dataset.examples;
            assert_eq!(
                dispatched.len(),
                12,
                "VDD: dispatched dataset must carry all 12 accumulated examples"
            );
            for (i, (sub, disp)) in all_submitted.iter().zip(dispatched.iter()).enumerate() {
                assert_eq!(
                    sub.prompt, disp.prompt,
                    "VDD: position {i} reordering — accumulator violated submission order"
                );
            }
        }
    }

    /// Stress / concurrency tests — gated behind the `stress-tests` feature per
    /// CLAUDE.md's test-discipline doctrine. Default `cargo test` does NOT compile
    /// these. Multi-thread tokio runtime + a yielding stub adapter exercise the race
    /// windows the gate protects against.
    #[cfg(feature = "stress-tests")]
    pub(super) mod stress {
        use super::super::*;
        use crate::genome::fine_tuning::adapter::{
            FineTuningAdapter, FineTuningCapabilities, FineTuningError, TrainerHardware,
        };
        use crate::genome::fine_tuning::types::{
            ArtifactFormat, JobHandle, JobMetrics, TrainingArtifact, TrainingExample,
            TrainingJobRequest, TrainingStatus,
        };
        use crate::genome::fine_tuning::FineTuningRegistry;
        use crate::modules::training_trigger::TrainingTriggerModule;
        use crate::runtime::CommandExecutor;
        use async_trait::async_trait;
        use serde_json::json;
        use std::sync::Mutex as StdMutex;
        use std::time::Duration;

        /// Test-only adapter that captures dispatched `TrainingJobRequest`s and yields
        /// cooperatively (+ a short sleep) before returning, so concurrent submit tasks
        /// WILL interleave at the gate's `.await` boundary under a multi-thread runtime.
        struct YieldingRecordingAdapter {
            captured_requests: Arc<StdMutex<Vec<TrainingJobRequest>>>,
        }

        impl YieldingRecordingAdapter {
            fn new() -> Self {
                Self {
                    captured_requests: Arc::new(StdMutex::new(Vec::new())),
                }
            }
            fn captures(&self) -> Arc<StdMutex<Vec<TrainingJobRequest>>> {
                self.captured_requests.clone()
            }
        }

        #[async_trait]
        impl FineTuningAdapter for YieldingRecordingAdapter {
            fn capabilities(&self) -> FineTuningCapabilities {
                FineTuningCapabilities {
                    provider_id: "stress-yielding-recorder".to_string(),
                    supports_lora: true,
                    supports_validation: false,
                    produces_local_artifact: true,
                    supported_base_model_prefixes: vec!["stress".to_string()],
                    requires: TrainerHardware::Any,
                    place: crate::genome::fine_tuning::adapter::TrainingPlace::SeparateCopyHere,
                }
            }

            async fn create_job(
                &self,
                request: TrainingJobRequest,
            ) -> Result<JobHandle, FineTuningError> {
                // WIDE race window: multiple yields + a long-enough sleep open the
                // window for contending submits to interleave reliably across runs.
                for _ in 0..8 {
                    tokio::task::yield_now().await;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
                self.captured_requests.lock().unwrap().push(request.clone());
                Ok(JobHandle {
                    provider_id: "stress-yielding-recorder".into(),
                    provider_job_id: format!("stress-{}", uuid::Uuid::new_v4()),
                    local_id: uuid::Uuid::new_v4(),
                })
            }

            async fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
                Ok(TrainingStatus::Completed {
                    artifact: TrainingArtifact {
                        model_id: handle.provider_job_id.clone(),
                        local_path: None,
                        format: ArtifactFormat::ProviderHosted,
                        metrics: JobMetrics::default(),
                    },
                })
            }

            async fn cancel(&self, _handle: &JobHandle) -> Result<(), FineTuningError> {
                Ok(())
            }
        }

        async fn build_stress_runtime() -> (
            Arc<TrainingTriggerModule>,
            Arc<CommandExecutor>,
            Arc<StdMutex<Vec<TrainingJobRequest>>>,
            tempfile::TempDir,
        ) {
            let (adapter, dir) = crate::orm::store::fresh_adapter().await;
            let ft_registry = Arc::new(FineTuningRegistry::new());
            let recorder = Arc::new(YieldingRecordingAdapter::new());
            let captures = recorder.captures();
            ft_registry.register(recorder);
            let (trigger, executor) =
                crate::commands::training_trigger::test_support::build_runtime_with_registry(
                    adapter,
                    Some(ft_registry),
                )
                .await;
            (trigger, executor, captures, dir)
        }

        fn stress_submit_params(
            persona_id: Uuid,
            trait_kind: &str,
            examples: Vec<TrainingExample>,
            min_examples: u32,
        ) -> Value {
            json!({
                "personaId": persona_id,
                "personaName": "stress-p",
                "baseModel": "stress-test",
                "traitKind": trait_kind,
                "examples": examples,
                "source": "operator_curated",
                "minExamples": min_examples,
            })
        }

        // what this catches: a SMOKE-LEVEL conservation check under multi-thread
        // concurrent submits. The YieldingRecordingAdapter captures dispatched datasets
        // so we sum-vs-submitted (true conservation). HONESTY NOTE: exercises the
        // multi-thread dispatch path with yields, but does not deterministically force
        // the C1/C2 race window (a Notify-barrier test is the deterministic exercise).
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_submits_to_same_key_serialize_without_loss_stress() {
            let (trigger, executor, captures, _dir) = build_stress_runtime().await;
            let persona = Uuid::new_v4();

            // Mix FIRE-LOAD submits (threshold=5, immediately fires) with ACCUMULATOR
            // submits (threshold=100, accumulates 1 example at a time). WITHOUT the gate
            // the fire-load's remove-on-success deletes the accumulator's appended
            // examples; WITH the gate accumulators block behind the fire-load.
            const N_FIRE: usize = 10;
            const FIRE_EXAMPLES: usize = 5;
            const N_ACCUM: usize = 50;
            let total_examples = N_FIRE * FIRE_EXAMPLES + N_ACCUM;

            let mut handles = Vec::with_capacity(N_FIRE + N_ACCUM);
            for accum in 0..N_ACCUM {
                let exec = executor.clone();
                handles.push(tokio::spawn(async move {
                    exec.execute_json(
                        "genome/training-trigger/submit",
                        stress_submit_params(
                            persona,
                            "race-trait",
                            vec![ex(&format!("acc{accum}-p"), &format!("acc{accum}-c"))],
                            100, // never fires alone
                        ),
                    )
                    .await
                }));
            }
            for fire in 0..N_FIRE {
                let exec = executor.clone();
                handles.push(tokio::spawn(async move {
                    exec.execute_json(
                        "genome/training-trigger/submit",
                        stress_submit_params(
                            persona,
                            "race-trait",
                            (0..FIRE_EXAMPLES)
                                .map(|i| {
                                    ex(&format!("fire{fire}-p{i}"), &format!("fire{fire}-c{i}"))
                                })
                                .collect(),
                            5,
                        ),
                    )
                    .await
                }));
            }
            for h in handles {
                let json = h.await.unwrap().expect("submit");
                assert_eq!(
                    json["success"], true,
                    "stress: every submit must succeed; got {json}"
                );
            }

            // Sum examples in dispatched datasets.
            let dispatched: usize = captures
                .lock()
                .unwrap()
                .iter()
                .map(|req| req.dataset.examples.len())
                .sum();
            let pending = trigger
                .state
                .bucket_example_count(persona, "race-trait", "stress-test")
                .unwrap_or(0);
            let total = dispatched + pending;
            assert_eq!(
                total, total_examples,
                "STRESS conservation: dispatched={dispatched} + pending={pending} = {total}, \
                 expected {total_examples}. A drop or duplicate means the gate failed under concurrency."
            );

            // No duplicate prompts in the dispatched datasets.
            let mut seen_prompts = std::collections::HashSet::new();
            for req in captures.lock().unwrap().iter() {
                for ex in &req.dataset.examples {
                    assert!(
                        seen_prompts.insert(ex.prompt.clone()),
                        "STRESS: duplicate prompt {:?} in dispatched datasets — \
                         gate failed to prevent double-drain",
                        ex.prompt
                    );
                }
            }
        }
    }
