//! Returning what ended jobs parked (cards 8a2334d9, 565b3b64): one pass of the trigger, moved
//! out of the module file so it stays under the size gate.
use std::sync::Arc;

use super::TrainingTriggerState;
use crate::genome::fine_tuning::job_dir::genome_root;

impl TrainingTriggerState {
    /// Hand back what each ENDED job parked (cards 8a2334d9, 565b3b64): the whole originals its
    /// window couldn't hold, or its run skipped, which no route returned before, including a
    /// failed job whose examples all parked. Through `return_parked_remainder`: never an example
    /// the job trained, once per job by its receipt, only once her adopted window can train one
    /// of them (a waiting job is held, not dropped), and a refused or failed return stays for the
    /// next pass. The serving watch wakes this pass when her window changes; boot runs it once.
    pub(crate) async fn return_parked_remainders(self: &Arc<Self>) {
        use crate::commands::training_trigger::return_::{return_parked_remainder, Remainder};
        let board = crate::genome::fine_tuning::job_board::TrainingJobBoard::global();
        let ended = board.take_ended();
        if ended.is_empty() {
            return;
        }
        let served = |base: &str| crate::genome::fine_tuning::engine_lora_adapter::served_window_for(base);
        let Some(root) = genome_root() else {
            continuum_base::probe!(class = "training.job.remainder_unrooted", jobs = ended.len() as u64, "no home: completed jobs cannot be found to return what they parked");
            return;
        };
        let demand_dir = crate::genome::fine_tuning::training_demand::TrainingDemand::store_dir(&root);
        let mut retry = Vec::new();
        for job in ended {
            let id = job.local_id;
            match return_parked_remainder(self, board, &root, &job, &served, Some(&demand_dir)).await {
                Ok(Remainder::Nothing | Remainder::AlreadyReturned) => {}
                Ok(Remainder::Waiting { needed, served }) => {
                    let changed = self.remainder_waits.lock().is_ok_and(|mut waits| waits.insert(job.local_id, (needed, served)) != Some((needed, served)));
                    if changed {
                        continuum_base::probe!(
                            class = "training.job.remainder_waits",
                            job = %job.local_id,
                            persona = %job.persona_name,
                            needed_window = u64::from(needed),
                            served_window = served.map_or(0, u64::from), // probe field: 0 = no live lane serves her base
                            "her parked examples wait for a window that can train one of them whole; returned now they would only park again in a job that cannot run"
                        );
                    }
                    retry.push(job);
                    continue;
                }
                Ok(Remainder::Submitted(outcome)) if outcome.success => continuum_base::probe!(
                    class = "training.job.remainder_returned",
                    job = %job.local_id,
                    persona = %job.persona_name,
                    "an ended job's parked examples went back to her bucket, whole; the recovery pass dispatches them (a return is ready below its threshold)"
                ),
                Ok(Remainder::Submitted(outcome)) => {
                    continuum_base::probe!(
                        class = "training.job.remainder_refused",
                        job = %job.local_id,
                        error = ?outcome.error,
                        "the return of an ended job's parked examples was refused; they stay in its directory and are tried again"
                    );
                    retry.push(job);
                }
                // A malformed record is refused loudly and not retried: retrying reads the same bytes.
                Err(error @ crate::sdk_codegen::CommandError::Invalid(_)) => continuum_base::probe!(
                    class = "training.job.remainder_invalid",
                    job = %job.local_id,
                    error = %error,
                    "an ended job's partition does not match its request; its parked examples stay in its directory for a human"
                ),
                Err(error) => {
                    continuum_base::probe!(
                        class = "training.job.remainder_failed",
                        job = %job.local_id,
                        error = %error,
                        "the return of an ended job's parked examples failed; they stay in its directory and are tried again"
                    );
                    retry.push(job);
                }
            }
            // returned, nothing to return, or refused: no longer waiting on her window
            if let Ok(mut waits) = self.remainder_waits.lock() {
                waits.remove(&id);
            }
        }
        board.requeue_ended(retry);
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::training_trigger::submit::{return_request, submit_batch, ReturnScope, SubmitParams};
    use crate::commands::training_trigger::test_support::{build_runtime_with_registry, ex};
    use crate::genome::fine_tuning::{FineTuningRegistry, RecordingFineTuningAdapter, RECORDING_BASE_PREFIX};
    use crate::runtime::ServiceModule;
    use std::sync::Arc;

    // what this catches (the windows 5090 2026-10-28 20:16Z; Codex on #5575): her canonical
    // returned examples landed in a bucket below its threshold (11 of 16) and waited for examples
    // a quiet persona never produces, so a reviewed correction never trained. A return is ready
    // below its threshold BY ITS DURABLE ACCEPTANCE: the core dies after the return and before any
    // dispatch, the next owner's recovery dispatches it whole, and a third owner dispatches nothing
    // again. A plain submit below its threshold stays held: historical buckets are never flushed.
    #[tokio::test]
    async fn a_return_dispatches_below_its_threshold_once_across_restarts() {
        let (adapter, _db) = crate::orm::store::fresh_adapter().await;
        let recorder = Arc::new(RecordingFineTuningAdapter::new());
        let providers = Arc::new(FineTuningRegistry::new());
        providers.register(recorder.clone());
        let base = format!("{RECORDING_BASE_PREFIX}-returned");
        let (returned_to, plain) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let (first, executor) = build_runtime_with_registry(adapter.clone(), Some(providers.clone())).await;
        let request: crate::genome::fine_tuning::types::TrainingJobRequest = serde_json::from_value(serde_json::json!({
            "personaId": returned_to, "personaName": "Kimi", "baseModel": base, "traitKind": "code/owner",
            "dataset": {"examples": [ex("p0", "c"), ex("p1", "c"), ex("p2", "c")], "source": "operator_curated", "validationSplit": 0.0}
        }))
        .expect("test: request");
        let landed = return_request(&first.state, request, uuid::Uuid::new_v4(), ReturnScope::Whole).await.expect("test: returned");
        assert!(landed.success && landed.job_handle.is_none(), "the return lands and holds; the pass dispatches it: {landed:?}");
        let held = submit_batch(&first.state, SubmitParams {
            submission_id: None, persona_id: plain, persona_name: "Sahar".into(), base_model: base.clone(), trait_kind: "code/owner".into(),
            examples: vec![ex("q0", "c")], source: crate::genome::fine_tuning::types::TrainingSource::OperatorCurated, eval_set: None,
            lora: None, schedule: None, local_artifact_dir: None, preferred_provider: None, min_examples: None, validation_split: None,
        }).await.expect("test: submitted");
        assert!(held.success, "{held:?}");
        // the core dies after the acceptance, before any pass ran
        first.shutdown().await.expect("test: shutdown");
        drop((first, executor));
        let dispatched = |persona: uuid::Uuid| -> Vec<Vec<String>> {
            recorder.captures().lock().expect("test: captures").iter()
                .filter(|r| r.persona_id == persona)
                .map(|r| r.dataset.examples.iter().map(|e| e.prompt.clone()).collect())
                .collect()
        };
        for owner in 0..2 {
            let (next, _executor) = build_runtime_with_registry(adapter.clone(), Some(providers.clone())).await;
            next.state.recover_tick().await.expect("test: recovery");
            assert_eq!(dispatched(returned_to), vec![vec!["p0".to_string(), "p1".into(), "p2".into()]], "owner {owner}: the return dispatched whole, exactly once");
            assert!(dispatched(plain).is_empty(), "owner {owner}: a plain submit below its threshold still waits for it");
            assert_eq!(next.state.bucket_example_count(plain, "code/owner", &base), Some(1));
            next.shutdown().await.expect("test: shutdown");
        }
    }
}
