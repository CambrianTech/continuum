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
                    "an ended job's parked examples went back to her bucket, whole; the next dispatch re-plans them"
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
