//! Returning what ended jobs parked (cards 8a2334d9, 565b3b64): one pass of the trigger, moved
//! out of the module file so it stays under the size gate.
use std::sync::Arc;

use super::{BucketKey, DispatchResult, TrainingTriggerState};
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
                Ok(Remainder::Submitted(outcome)) if outcome.success => {
                    continuum_base::probe!(
                        class = "training.job.remainder_returned",
                        job = %job.local_id,
                        persona = %job.persona_name,
                        "an ended job's parked examples went back to her bucket, whole; the next dispatch re-plans them"
                    );
                    match board.registration(id) {
                        Some(r) => self.dispatch_returned(BucketKey { persona_id: r.persona_id, trait_kind: r.trait_kind, base_model: r.base_model }, id).await,
                        None => continuum_base::probe!(class = "training.job.returned_unkeyed", job = %id,
                            "an ended job's examples went back but the board holds no registration naming her bucket; they dispatch at its threshold"),
                    }
                }
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

    /// Dispatch the bucket a return just refilled, below its threshold too: the returned examples
    /// met a threshold when they were first dispatched, and held to it again a quiet persona's
    /// returned correction waits for examples that never come (the windows 5090 2026-10-28
    /// 20:16Z: her canonical return held at 11 of 16). The flush verb's own dispatch, once per
    /// return; a pass returns only what her window can train now (#5563) or a smaller chunk
    /// can (`refusal_has_smaller_rung`), so this never re-dispatches into the same refusal.
    pub(crate) async fn dispatch_returned(self: &Arc<Self>, key: BucketKey, job: uuid::Uuid) {
        let dispatched = self.run_owned(key, |state, key| async move { state.dispatch_pending(&key).await }).await;
        match dispatched {
            Ok(DispatchResult::Dispatched { examples, handle, provider }) => continuum_base::probe!(
                class = "training.job.returned_dispatched", job = %job, examples = examples as u64, provider = %provider, next = %handle.local_id,
                "her returned examples dispatched at once, below the bucket's threshold"),
            Ok(DispatchResult::Empty | DispatchResult::Held { .. }) => {}
            Ok(DispatchResult::Failed { kind, error }) => continuum_base::probe!(
                class = "training.job.returned_dispatch_failed", job = %job, kind = kind, error = %error,
                "her returned examples stay owned by her bucket; the dispatch's own recovery retries them"),
            Err(error) => continuum_base::probe!(
                class = "training.job.returned_dispatch_failed", job = %job, kind = "Unowned", error = %error,
                "her returned examples stay in her bucket; they dispatch at its threshold or the next return"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::training_trigger::submit::{return_request, ReturnScope};
    use crate::commands::training_trigger::test_support::{build_runtime_with_registry, ex};
    use crate::genome::fine_tuning::{FineTuningRegistry, RecordingFineTuningAdapter, RECORDING_BASE_PREFIX};

    // what this catches (the windows 5090 2026-10-28 20:16Z): her canonical returned examples
    // landed in a bucket below its threshold (11 of 16) and waited for unrelated examples that a
    // quiet persona never produces, so a reviewed correction never trained. A return the pass
    // makes dispatches its bucket at once, whole, and only once.
    #[tokio::test]
    async fn a_returned_batch_below_its_threshold_dispatches_once() {
        let (adapter, _db) = crate::orm::store::fresh_adapter().await;
        let recorder = Arc::new(RecordingFineTuningAdapter::new());
        let providers = Arc::new(FineTuningRegistry::new());
        providers.register(recorder.clone());
        let (trigger, _executor) = build_runtime_with_registry(adapter, Some(providers)).await;
        let (persona, job) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let base = format!("{RECORDING_BASE_PREFIX}-returned");
        let request: crate::genome::fine_tuning::types::TrainingJobRequest = serde_json::from_value(serde_json::json!({
            "personaId": persona, "personaName": "Kimi", "baseModel": base, "traitKind": "code/owner",
            "dataset": {"examples": [ex("p0", "c"), ex("p1", "c"), ex("p2", "c")], "source": "operator_curated", "validationSplit": 0.0}
        }))
        .expect("test: request");
        let landed = return_request(&trigger.state, request, job, ReturnScope::Whole).await.expect("test: returned");
        assert!(landed.success && landed.job_handle.is_none(), "below the default threshold the return alone holds: {landed:?}");
        let key = BucketKey { persona_id: persona, trait_kind: "code/owner".into(), base_model: base };
        trigger.state.dispatch_returned(key.clone(), job).await;
        let prompts: Vec<Vec<String>> = recorder
            .captures()
            .lock()
            .expect("test: captures")
            .iter()
            .map(|r| r.dataset.examples.iter().map(|e| e.prompt.clone()).collect())
            .collect();
        assert_eq!(prompts, vec![vec!["p0".to_string(), "p1".into(), "p2".into()]], "dispatched at once, whole, in order");
        assert_eq!(trigger.state.bucket_example_count(persona, "code/owner", &key.base_model), None, "nothing left waiting");
        trigger.state.dispatch_returned(key, job).await;
        assert_eq!(recorder.captured_job_count(), 1, "a second pass dispatches nothing twice");
    }
}
