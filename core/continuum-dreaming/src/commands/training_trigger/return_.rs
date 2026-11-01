//! `genome/training-trigger/return` hands the examples of a job that never trained back to
//! her bucket.
//!
//! A job takes its examples out of the bucket when it is dispatched. If it then ends
//! without training a step (killed before it ran, refused at admission, ended by a
//! restart that nothing resumed), those examples sit in its directory and no fill ever
//! sees them again. On 2026-10-05 the 5090 stranded about 500 of Kimi's that way (18
//! jobs). This verb puts them back through the one acceptance path, under an identity
//! derived from the job (`returned_id`), so a second return of the same job is a replay,
//! never a second copy, and never the identity of the submission that filled the job.
//!
//! Deliberately a VERB, not automatic on failure: a job that fails deterministically
//! (out of device memory at its window) would refill, re-dispatch and fail again
//! forever. Returning one is a decision. A held orphan at boot is returned
//! automatically by the trigger, through the same `SubmitParams::returning`.
//!
//! ## Gating
//!
//! `Privileged`, same surface as submit: a return can cross the threshold and dispatch.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use ts_rs::TS;
use uuid::Uuid;

use super::submit::{return_request, ReturnScope};
use crate::genome::training_outcome::SubmitOutcome;
use crate::genome::fine_tuning::TrainingJobBoard;
use crate::genome::fine_tuning::engine_lora_adapter::read_partition;
use crate::genome::fine_tuning::job_dir::{genome_root, job_dir_under, read_job_request};
use crate::modules::training_trigger::TrainingTriggerState;
use crate::sdk_codegen::CommandError;

/// `genome/training-trigger/return` input.
#[derive(Debug, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/ReturnParams.ts")]
pub struct ReturnParams {
    /// The job whose examples go back: its local id, as the job board names it.
    #[ts(type = "string")]
    pub job_id: Uuid,
}

crate::action_command! {
    /// Return the examples of a job that ended without training to its persona's bucket.
    /// Refuses a job that is still open (it trains or resumes itself) and one that trained
    /// (its adapter is its outcome). A distinct identity derives from the job, so returning the same
    /// job twice is a replay. Returns submit's outcome: appended, held behind a job of hers,
    /// or dispatched if the bucket filled.
    pub struct TrainingTriggerReturn {
        state: Arc<TrainingTriggerState>,
    }
    name: "genome/training-trigger/return",
    access: Privileged,
    params: ReturnParams,
    output: SubmitOutcome,
    run(this, _ctx, p) => {
        let root = genome_root().ok_or_else(|| {
            CommandError::Internal("no home directory: job directories cannot be found".into())
        })?;
        return_job(&this.state, TrainingJobBoard::global(), &root, p.job_id).await
    }
}

/// The return itself, over an explicit board and genome root, so it can be exercised
/// without the node's ledger or home.
pub(crate) async fn return_job(
    state: &Arc<TrainingTriggerState>,
    board: &TrainingJobBoard,
    root: &std::path::Path,
    job_id: Uuid,
) -> Result<SubmitOutcome, CommandError> {
    let job = board.registration(job_id).ok_or_else(|| {
        CommandError::NotFound(format!("job {job_id} has no registration on this node's job ledger"))
    })?;
    if !board.is_terminal(job_id) {
        return Err(CommandError::Invalid(format!(
            "job {job_id} is still open: it trains, or the trigger resumes it; only an ended job is returned"
        )));
    }
    if board.completed_training(job_id) {
        return Err(CommandError::Invalid(format!("job {job_id} completed training; its input is not returned")));
    }
    let dir = job_dir_under(root, &job.persona_name, &job.trait_kind, job_id);
    refuse_trained_job(job_id, &dir)?;
    let request = state.job_input(&job, &dir).await.map_err(input_error)?;
    if let Some(root) = &request.local_artifact_dir {
        refuse_trained_job(job_id, &root.join(job_id.to_string()))?;
    }
    // PARKED examples go back too (card e5478bb4; Codex on the 5090 receipt, 2026-10-12): the
    // next dispatch RE-PLANS them against the engine and memory of now, sends only examples that
    // train whole, and a job memory cannot give a useful window parks durably with its wake
    // (#4988). Holding them out was right while a dispatch could only fail or loop; it left Kimi's
    // canonical 680a20 examples in a terminal job's partition with no route back.
    let outcome = return_request(state, request, job_id, ReturnScope::Whole).await?;
    if outcome.success {
        board.journal_returned(job_id, job_id, &serde_json::json!("genome/training-trigger/return"), 0); // not held by a Took: an operator returned it, named by the verb
    }
    Ok(outcome)
}

/// A job's input that cannot be had, as the verb reports it: an unproven dispatch is an invalid
/// request; a job with no dispatch and no request file has nothing to return; a store or file
/// that cannot be read now is an internal failure the caller may retry.
fn input_error(error: crate::modules::training_trigger::JobInputError) -> CommandError {
    match error {
        crate::modules::training_trigger::JobInputError::Unproven(why) => CommandError::Invalid(why),
        crate::modules::training_trigger::JobInputError::Missing(why) => CommandError::NotFound(why),
        crate::modules::training_trigger::JobInputError::Unavailable(why) => CommandError::Internal(why),
    }
}

/// What returning a trained job's parked remainder did.
#[derive(Debug)]
pub(crate) enum Remainder {
    /// The job parked nothing, or keeps no partition: nothing of it is stranded.
    Nothing,
    /// Its receipt stands: the remainder went back before, and is never sent twice.
    AlreadyReturned,
    /// Submitted to her bucket. A refusal (`success: false`) leaves no receipt, so the next
    /// pass retries, and the chunks already accepted replay by their derived identities.
    Submitted(SubmitOutcome),
    /// Not yet: her adopted window (`served`, `None` with no live lane for her base) is below
    /// `needed`, the smallest window that trains one of its parked examples whole. Returned
    /// now, they would only park again in a job that cannot run (card 565b3b64).
    Waiting { needed: u32, served: Option<u32> },
}

/// Written in a job's directory once its parked remainder is back in her bucket.
pub(crate) const REMAINDER_RECEIPT: &str = "parked-returned.json";

/// The receipt of a returned remainder: which job, exactly which request indices.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemainderReceipt {
    job: Uuid,
    parked: Vec<usize>,
    at_ms: u64,
}

/// The REMAINDER of a job that ended: the whole originals its partition parked (the window
/// couldn't hold them whole, or the run skipped them), back into her bucket, once (card
/// 8a2334d9). Never an example it trained, so a trained job's FULL return stays refused
/// ([`return_job`]). `job.trained` is the sentinel's Completed claim or the ledger's terminal
/// row. A FAILED job is returned here only when it sent nothing (every example parked, so it
/// never ran: card 565b3b64); one that sent examples is the refused-job return's. Its remainder
/// IS its whole input, so it goes back under the whole-job identity ([`ReturnScope::Whole`]), the
/// one [`return_job`] uses: an operator's return, a concurrent pass or a replay after a crash
/// before the receipt are one return by identity, never two appends (Codex on #5563).
///
/// - Only when her adopted window (`served_window` of the job's base) reaches the smallest
///   window that trains one of its parked examples whole ([`Remainder::Waiting`] otherwise): a
///   return before then re-parks them in a job that fails, which is how her reviewed correction
///   stranded. A partition without measured rows (before fork #51) is returned as before.
///
/// - The examples are the job's own input as [`TrainingTriggerState::job_input`] proves it (its durable dispatch,
///   or a legacy request.json): native base, whole payloads, never the partition's measured rows.
/// - The partition is read strictly: only a confirmed-absent file means nothing was parked; an
///   unreadable or corrupt one is an error the next pass retries.
/// - The partition must account for every request example ([`remainder_of`]); one that doesn't
///   is refused before anything is appended.
/// - The identity is the job plus its exact parked set ([`ReturnScope::Parked`]). A
///   [`REMAINDER_RECEIPT`] written atomically after the bucket took it ends the job's remainder;
///   a crash between the two replays by identity, never appends twice.
pub(crate) async fn return_parked_remainder(
    state: &Arc<TrainingTriggerState>,
    board: &TrainingJobBoard,
    root: &std::path::Path,
    job: &crate::genome::fine_tuning::job_board::EndedJob,
    served_window: &(dyn Fn(&str) -> Option<u32> + Sync), // held across awaits: Sync keeps the pass's future Send
    demand_dir: Option<&std::path::Path>,
) -> Result<Remainder, CommandError> {
    let id = job.local_id;
    let dir = job_dir_under(root, &job.persona_name, &job.trait_kind, id);
    let read = |dir: &std::path::Path| read_partition(dir).map_err(|e| CommandError::Internal(format!("job {id}: its partition is unreadable: {e}")));
    let (dir, partition) = match read(&dir)? {
        Some(partition) => (dir, partition),
        // under the request's artifact dir, as `return_job` looks, when a request file names one
        None => match read_job_request(&dir).ok().and_then(|r| r.local_artifact_dir).map(|r| r.join(id.to_string())) {
            Some(alt) => match read(&alt)? {
                Some(partition) => (alt, partition),
                None => return Ok(Remainder::Nothing),
            },
            None => return Ok(Remainder::Nothing), // confirmed: no partition where its records live
        },
    };
    if dir.join(REMAINDER_RECEIPT).try_exists().map_err(|e| CommandError::Internal(e.to_string()))? {
        return Ok(Remainder::AlreadyReturned);
    }
    if partition.parked.is_empty() || (!job.trained && !partition.sent.is_empty()) {
        return Ok(Remainder::Nothing);
    }
    let registration = board
        .registration(id)
        .ok_or_else(|| CommandError::NotFound(format!("job {id} has no registration on this node's job ledger")))?;
    let mut request = state.job_input(&registration, &dir).await.map_err(input_error)?;
    let parked = remainder_of(&partition, request.dataset.examples.len())
        .map_err(|why| CommandError::Invalid(format!("job {id}: {why}; nothing is returned")))?;
    let needed = partition
        .rows
        .iter()
        .filter(|row| partition.parked.contains(&row.index))
        .map(|row| crate::inference::planning::training_plan::window_for_need(std::slice::from_ref(row), u32::MAX))
        .min();
    if let Some(needed) = needed {
        let served = served_window(&registration.base_model);
        if served.is_none_or(|window| window < needed) {
            // It waits only while serving can know what it waits for. Demand is recorded by a
            // partition (#5557); a job partitioned before that has none, so held it would wait
            // forever beside a window that never grows for it (the windows 5090, 2026-10-28:
            // NoDemand, 2 lanes at 51,694 against her 51,712). Such a remainder records its own
            // demand FIRST (each example's window from its measured row, unpriced), then goes back
            // once (its receipt ends it), so the next partition prices it on the one pricing path.
            // Recorded before it goes back, a later all-parked job of the same examples finds it
            // and waits, even if that partition's write fails: no cross-job loop. A store that
            // cannot answer or be written holds the job and is asked again (Codex on #5569).
            use crate::genome::fine_tuning::training_demand::TrainingDemand;
            let demand = TrainingDemand::in_dir(demand_dir, registration.persona_id);
            let keys: Vec<String> = parked.iter().filter_map(|&i| request.dataset.examples.get(i)).map(TrainingDemand::key_of).collect();
            let recorded = demand
                .records_all(&keys)
                .map_err(|why| CommandError::Internal(format!("job {id}: her demand store cannot answer ({why}); held, asked again")))?;
            if recorded {
                return Ok(Remainder::Waiting { needed, served });
            }
            // Every parked example needs exactly one measured row, or only part of the demand is
            // recorded and its successor finds it unrecorded and returns again (Codex on #5569).
            let needs = parked
                .iter()
                .map(|&i| {
                    let mut rows = partition.rows.iter().filter(|row| row.index == i);
                    match (request.dataset.examples.get(i), rows.next(), rows.next()) {
                        (Some(example), Some(row), None) => Ok((
                            TrainingDemand::key_of(example),
                            crate::inference::planning::training_plan::window_for_need(std::slice::from_ref(row), u32::MAX),
                        )),
                        _ => Err(i),
                    }
                })
                .collect::<Result<Vec<(String, u32)>, usize>>()
                .map_err(|i| {
                    CommandError::Invalid(format!(
                        "job {id}: its partition does not hold exactly one measured row for parked example {i}; its demand cannot be recorded whole, so nothing is returned"
                    ))
                })?;
            demand
                .record(&needs, &[], None)
                .map_err(|e| CommandError::Internal(format!("job {id}: its demand could not be recorded ({e}); held, asked again")))?;
            continuum_base::probe!(
                class = "training.job.remainder_unrecorded",
                job = %id,
                needed_window = u64::from(needed),
                served_window = served.map_or(0, u64::from), // probe field: 0 = no live lane serves her base
                "her parked examples wait for a window serving was never told about: returned once, so a partition records and prices their demand"
            );
        }
    }
    request.dataset.examples = parked.iter().map(|&i| request.dataset.examples[i].clone()).collect();
    // a failed job that sent nothing parked every example (`remainder_of` proved the coverage)
    let scope = if job.trained { ReturnScope::Parked(&parked) } else { ReturnScope::Whole };
    let outcome = return_request(state, request, id, scope).await?;
    if outcome.success {
        let receipt = RemainderReceipt { job: id, parked, at_ms: continuum_base::time::EpochMs::now().millis() };
        write_receipt(&dir, &receipt).map_err(|e| {
            CommandError::Internal(format!(
                "job {id}: its remainder is in her bucket, but its receipt could not be written ({e}); the next pass replays it by identity"
            ))
        })?;
    }
    Ok(Remainder::Submitted(outcome))
}

/// PURE: the request indices to return from `partition`, a job of `examples` examples, or why
/// the record can't be trusted. Both lists strictly ascending and in range; together they cover
/// every example. One in both was sent and then skipped by the run (the only way the two
/// overlap, `with_run_skipped`): the measure must have kept it, so its row, when rows were
/// recorded, says Kept. An example sent and not parked trained, and is never returned.
fn remainder_of(partition: &crate::inference::planning::training_plan::Partition, examples: usize) -> Result<Vec<usize>, String> {
    use continuum_engine_protocol::ExampleFit;
    for (name, set) in [("sent", &partition.sent), ("parked", &partition.parked)] {
        if let Some(i) = set.iter().copied().find(|&i| i >= examples) {
            return Err(format!("its partition's {name} names request index {i}, but its request holds {examples} examples"));
        }
        if !set.windows(2).all(|pair| pair[0] < pair[1]) {
            return Err(format!("its partition's {name} indices {set:?} are not unique and ascending"));
        }
    }
    let mut covered = vec![false; examples];
    for &i in partition.sent.iter().chain(&partition.parked) {
        covered[i] = true;
    }
    if let Some(missing) = covered.iter().position(|c| !c) {
        return Err(format!("its partition accounts for no request index {missing}: neither sent nor parked"));
    }
    if !partition.rows.is_empty() {
        for i in partition.parked.iter().filter(|i| partition.sent.contains(i)) {
            let kept = partition.rows.iter().find(|r| r.index == *i).is_some_and(|r| r.fit == ExampleFit::Kept);
            if !kept {
                return Err(format!("request index {i} is both sent and parked, but its measure didn't keep it, so the run can't have skipped it"));
            }
        }
    }
    Ok(partition.parked.clone())
}

/// Write the receipt whole or not at all: to a sibling, then renamed over.
fn write_receipt(dir: &std::path::Path, receipt: &RemainderReceipt) -> Result<(), String> {
    let body = serde_json::to_vec(receipt).map_err(|e| e.to_string())?; // file on disk: the job dir's remainder receipt, read by the next pass after a restart
    let staged = dir.join(format!("{REMAINDER_RECEIPT}.partial"));
    std::fs::write(&staged, body).map_err(|e| e.to_string())?;
    std::fs::rename(&staged, dir.join(REMAINDER_RECEIPT)).map_err(|e| e.to_string())
}

fn refuse_trained_job(job: Uuid, dir: &std::path::Path) -> Result<(), CommandError> {
    for artifact in ["checkpoints/LATEST", "adapters/adapter_config.json", "adapters/adapter.gguf"] {
        if dir.join(artifact).try_exists().map_err(|e| CommandError::Internal(e.to_string()))? {
            return Err(CommandError::Invalid(format!(
                "job {job} has a checkpoint or adapter in {}: its outcome is that adapter, not its examples", dir.display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::training_trigger::submit::{submit_batch, SubmitParams};
    use crate::commands::training_trigger::test_support::{build_runtime_trigger_only, ex};
    use crate::genome::fine_tuning::job_board::WatchedJob;
    use crate::genome::fine_tuning::types::{JobHandle, TrainingSource, TrainingStatus};

    // what this catches (card e5478bb4; the 5090 receipt, 2026-10-12): a terminal job whose
    // examples were all parked (Kimi's canonical 680a20) refused as Parked, leaving them in its
    // partition with no route back. Astra's concern on 2026-10-11 (parked examples refilling
    // her bucket and being skipped every tick) is now the dispatch's job: it re-plans whole-only
    // and a job memory cannot serve parks durably with its wake (#4988). So every example returns.
    #[tokio::test]
    async fn a_return_hands_back_parked_examples_for_the_next_plan() {
        let (trigger, _executor, _db) = build_runtime_trigger_only().await;
        let dir = tempfile::tempdir().expect("test: tempdir");
        let board = TrainingJobBoard::with_ledger(Some(dir.path().join("jobs-ledger.jsonl")));
        let root = dir.path().join("genome");
        let persona = Uuid::from_u128(11);
        let ended = |job: Uuid| {
            board.register(WatchedJob {
                trigger_dispatch_id: None,
                handle: JobHandle { provider_id: "engine-local".into(), provider_job_id: "x".into(), local_id: job },
                persona_id: persona,
                persona_name: "Kimi".into(),
                base_model: "ggml-org/Qwen3.8-27B-GGUF".into(),
                trait_kind: "code/owner".into(),
                eval_set: None,
                signature: None,
                decision: None,
            });
            board.claim(job, &TrainingStatus::Failed { error: "ended before training".into() });
        };
        let with_parked = |job: Uuid, parked: Vec<usize>| {
            let jd = job_dir_under(&root, "Kimi", "code/owner", job);
            std::fs::create_dir_all(&jd).expect("test: job dir");
            std::fs::write(jd.join("request.json"), serde_json::json!({
                "personaId": persona.to_string(), "personaName": "Kimi",
                "baseModel": "ggml-org/Qwen3.8-27B-GGUF", "traitKind": "code/owner",
                "dataset": {"examples": [ex("p0", "c"), ex("p1", "c"), ex("p2", "c")], "source": TrainingSource::TeacherSynthesized, "validationSplit": 0.0}
            }).to_string()).expect("test: request");
            std::fs::write(jd.join("partition.json"), serde_json::json!({
                "window": 16896, "sent": [], "parked": parked, "rows": []
            }).to_string()).expect("test: partition");
        };
        let key = crate::modules::training_trigger::BucketKey {
            persona_id: persona,
            trait_kind: "code/owner".into(),
            base_model: "ggml-org/Qwen3.8-27B-GGUF".into(),
        };
        // A job whose examples were ALL parked (Kimi's canonical 680a20 shape) returns them all:
        // the next dispatch re-plans them, never this record.
        let all = Uuid::new_v4();
        ended(all);
        with_parked(all, vec![0, 1, 2]);
        assert!(return_job(&trigger.state, &board, &root, all).await.expect("test: returned").success);
        let held: Vec<String> = trigger.state.buckets.get(&key).map(|b| b.examples.iter().map(|e| e.prompt.clone()).collect()).expect("test: bucket");
        assert_eq!(held, vec!["p0".to_string(), "p1".to_string(), "p2".to_string()], "every example goes back, parked ones too");

        // what this catches (Codex on #4992): a MIXED job returned before this change carried
        // only its unparked examples under the job's returned id. Returning it again, now whole,
        // must be refused explicitly (the id already binds different content), never accepted as
        // already-returned while its parked rows are silently lost. Such a job needs its own
        // named recovery; this change does not claim the history is repaired.
        let earlier = Uuid::new_v4();
        ended(earlier);
        with_parked(earlier, vec![0, 2]);
        let jd = job_dir_under(&root, "Kimi", "code/owner", earlier);
        let mut unparked_only = read_job_request(&jd).expect("test: request");
        unparked_only.dataset.examples.retain(|e| e.prompt == "p1");
        assert!(return_request(&trigger.state, unparked_only, earlier, ReturnScope::Whole).await.expect("test: the old return").success);
        let before = trigger.state.buckets.get(&key).map(|b| b.examples.len());
        let again = return_job(&trigger.state, &board, &root, earlier).await.expect("test: answered");
        assert!(!again.success, "a different payload under the same returned id is refused: {again:?}");
        assert!(again.error_kind.is_some() && again.error.is_some(), "the refusal is typed and says why: {again:?}");
        assert_eq!(trigger.state.buckets.get(&key).map(|b| b.examples.len()), before, "nothing was added");
    }

    // what this catches (card 8a2334d9; Kimi's 55f2e6de on the 5090, 2026-10-13): a job that
    // COMPLETED training kept the examples its partition parked with no route back, since the
    // one return refuses a trained job. Its remainder returns whole from request.json, never an
    // example it trained, including one the run skipped (in `sent` and parked); once, by its
    // receipt; a full return stays refused; and a partition that doesn't match its request is
    // refused with nothing added.
    #[tokio::test]
    async fn a_completed_job_returns_only_its_parked_remainder_once() {
        use crate::genome::fine_tuning::job_board::EndedJob;
        let (trigger, _executor, _db) = build_runtime_trigger_only().await;
        let dir = tempfile::tempdir().expect("test: tempdir");
        let board = TrainingJobBoard::with_ledger(Some(dir.path().join("jobs-ledger.jsonl")));
        let root = dir.path().join("genome");
        let persona = Uuid::from_u128(12);
        let completed = |job: Uuid, sent: Vec<usize>, parked: Vec<usize>| {
            board.register(WatchedJob {
                trigger_dispatch_id: None,
                handle: JobHandle { provider_id: "engine-local".into(), provider_job_id: "x".into(), local_id: job },
                persona_id: persona,
                persona_name: "Kimi".into(),
                base_model: "ggml-org/Qwen3.8-27B-GGUF".into(),
                trait_kind: "code/owner".into(),
                eval_set: None,
                signature: None,
                decision: None,
            });
            board.claim(job, &TrainingStatus::Completed {
                artifact: crate::genome::fine_tuning::types::TrainingArtifact {
                    model_id: "gene".into(),
                    local_path: None,
                    format: Default::default(),
                    metrics: Default::default(),
                },
            });
            let jd = job_dir_under(&root, "Kimi", "code/owner", job);
            std::fs::create_dir_all(&jd).expect("test: job dir");
            std::fs::write(jd.join("request.json"), serde_json::json!({
                "personaId": persona.to_string(), "personaName": "Kimi",
                "baseModel": "ggml-org/Qwen3.8-27B-GGUF", "traitKind": "code/owner",
                "dataset": {"examples": [ex("p0", "c"), ex("p1", "c"), ex("p2", "c"), ex("p3", "c")], "source": TrainingSource::TeacherSynthesized, "validationSplit": 0.0}
            }).to_string()).expect("test: request");
            std::fs::write(jd.join("partition.json"), serde_json::json!({
                "window": 16896, "sent": sent, "parked": parked, "rows": []
            }).to_string()).expect("test: partition");
            std::fs::create_dir_all(jd.join("adapters")).expect("test: adapters");
            std::fs::write(jd.join("adapters/adapter.gguf"), b"gene").expect("test: its adapter");
            EndedJob { local_id: job, persona_name: "Kimi".into(), trait_kind: "code/owner".into(), trained: true }
        };
        let key = crate::modules::training_trigger::BucketKey {
            persona_id: persona,
            trait_kind: "code/owner".into(),
            base_model: "ggml-org/Qwen3.8-27B-GGUF".into(),
        };
        let held = || trigger.state.buckets.get(&key).map(|b| b.examples.iter().map(|e| e.prompt.clone()).collect::<Vec<_>>()).unwrap_or_default();

        // Sent 0, 1 and 2; the run skipped 1, so 1 is parked too; 3 never fit. 0 and 2 trained.
        let trained = Uuid::new_v4();
        let job = completed(trained, vec![0, 1, 2], vec![1, 3]);
        assert_eq!(board.take_ended(), vec![job.clone()], "the sentinel's Completed claim queues it for the trigger");
        let first = return_parked_remainder(&trigger.state, &board, &root, &job, &|_: &str| None, None).await.expect("test: returned");
        assert!(matches!(&first, Remainder::Submitted(o) if o.success), "{first:?}");
        assert_eq!(held(), vec!["p1".to_string(), "p3".to_string()], "only what it parked goes back, whole, never what it trained");
        assert!(job_dir_under(&root, "Kimi", "code/owner", trained).join(REMAINDER_RECEIPT).is_file(), "the receipt stands");
        let again = return_parked_remainder(&trigger.state, &board, &root, &job, &|_: &str| None, None).await.expect("test: answered");
        assert!(matches!(again, Remainder::AlreadyReturned), "{again:?}");
        assert_eq!(held().len(), 2, "returned once, never twice");
        let whole = return_job(&trigger.state, &board, &root, trained).await;
        assert!(whole.is_err(), "a trained job's FULL return stays refused: {whole:?}");

        // A partition naming an example its request doesn't hold is refused; nothing is added.
        let broken = completed(Uuid::new_v4(), vec![0], vec![1, 9]);
        let refused = return_parked_remainder(&trigger.state, &board, &root, &broken, &|_: &str| None, None).await;
        assert!(matches!(refused, Err(CommandError::Invalid(_))), "{refused:?}");
        assert_eq!(held().len(), 2, "nothing was added");

        // A partition that leaves an example unaccounted for (neither sent nor parked) is refused.
        let gap = completed(Uuid::new_v4(), vec![0], vec![2, 3]);
        let refused = return_parked_remainder(&trigger.state, &board, &root, &gap, &|_: &str| None, None).await;
        assert!(matches!(refused, Err(CommandError::Invalid(ref why)) if why.contains("index 1")), "{refused:?}");
        // A corrupt partition is an error the next pass retries, never "nothing parked".
        let torn = completed(Uuid::new_v4(), vec![0], vec![1, 2, 3]);
        std::fs::write(job_dir_under(&root, "Kimi", "code/owner", torn.local_id).join("partition.json"), b"{ torn").expect("test: corrupt");
        let refused = return_parked_remainder(&trigger.state, &board, &root, &torn, &|_: &str| None, None).await;
        assert!(matches!(refused, Err(CommandError::Internal(_))), "{refused:?}");
        assert_eq!(held().len(), 2, "nothing was added");

        // A job that parked nothing has no remainder.
        let whole_fit = completed(Uuid::new_v4(), vec![0, 1, 2, 3], vec![]);
        assert!(matches!(return_parked_remainder(&trigger.state, &board, &root, &whole_fit, &|_: &str| None, None).await, Ok(Remainder::Nothing)));
    }

    // what this catches (card 565b3b64; Kimi's reviewed correction 2de02f40 on the windows 5090,
    // 2026-10-28): a job whose examples ALL parked never runs, ends FAILED, and its examples had no
    // route back (the remainder route took completed jobs only). Returned at once they would only
    // re-park in another job that cannot run. They wait while her adopted window is below the
    // smallest one that trains one of them whole, and go back once it reaches it, under the
    // whole-job identity: the operator's return, a replay after a crash before the receipt, and
    // both routes at once are each one return (Codex on #5563). A failed job that sent examples
    // stays the refused-job return's.
    #[tokio::test]
    async fn an_all_parked_failed_job_returns_when_her_window_can_train_one_and_only_once() {
        use crate::genome::fine_tuning::job_board::EndedJob;
        let (trigger, _executor, _db) = build_runtime_trigger_only().await;
        let dir = tempfile::tempdir().expect("test: tempdir");
        let board = TrainingJobBoard::with_ledger(Some(dir.path().join("jobs-ledger.jsonl")));
        let root = dir.path().join("genome");
        let persona = Uuid::from_u128(0x2de0);
        let row = |index: usize, tokens: u64| serde_json::json!({"index": index, "tokens_full": tokens, "tokens_min": tokens, "tokens_trained": 0, "fit": "skipped"});
        let failed = |job: Uuid, sent: Vec<usize>, parked: Vec<usize>| {
            board.register(WatchedJob {
                trigger_dispatch_id: None,
                handle: JobHandle { provider_id: "engine-local".into(), provider_job_id: "x".into(), local_id: job },
                persona_id: persona,
                persona_name: "Kimi".into(),
                base_model: "ggml-org/Qwen3.8-27B-GGUF".into(),
                trait_kind: "code/owner".into(),
                eval_set: None,
                signature: None,
                decision: None,
            });
            board.claim(job, &TrainingStatus::Failed { error: "none of the 3 examples trains whole at window 49664".into() });
            let jd = job_dir_under(&root, "Kimi", "code/owner", job);
            std::fs::create_dir_all(&jd).expect("test: job dir");
            std::fs::write(jd.join("request.json"), serde_json::json!({
                "personaId": persona.to_string(), "personaName": "Kimi",
                "baseModel": "ggml-org/Qwen3.8-27B-GGUF", "traitKind": "code/owner",
                "dataset": {"examples": [ex("p0", "c"), ex("p1", "c"), ex("p2", "c")], "source": TrainingSource::TeacherSynthesized, "validationSplit": 0.0}
            }).to_string()).expect("test: request");
            std::fs::write(jd.join("partition.json"), serde_json::json!({
                "window": 49664, "sent": sent, "parked": parked, "rows": [row(0, 51_670), row(1, 64_041), row(2, 60_000)]
            }).to_string()).expect("test: partition");
            EndedJob { local_id: job, persona_name: "Kimi".into(), trait_kind: "code/owner".into(), trained: false }
        };
        let key = crate::modules::training_trigger::BucketKey {
            persona_id: persona,
            trait_kind: "code/owner".into(),
            base_model: "ggml-org/Qwen3.8-27B-GGUF".into(),
        };
        let held = || trigger.state.buckets.get(&key).map(|b| b.examples.len()).unwrap_or(0);
        // serving was told what her parked examples need (a partition recorded it, #5557)
        let demand_dir = dir.path().join("training-demand");
        let told = crate::genome::fine_tuning::training_demand::TrainingDemand::in_dir(Some(&demand_dir), persona);
        let recorded: Vec<(String, u32)> = ["p0", "p1", "p2"]
            .iter()
            .map(|p| (crate::genome::fine_tuning::training_demand::TrainingDemand::key_of(&ex(p, "c")), 64_256))
            .collect();
        told.record(&recorded, &[], None).expect("test: demand recorded");
        let all = Uuid::new_v4();
        let job = failed(all, vec![], vec![0, 1, 2]);
        assert_eq!(board.take_ended(), vec![job.clone()], "the sentinel's Failed claim lists it for the trigger, untrained");
        // her smallest parked example (51,670 tokens) trains whole at 51,712
        for (served, why) in [(None, "no live lane serves her base"), (Some(49_664), "her old window")] {
            let waiting = return_parked_remainder(&trigger.state, &board, &root, &job, &|_: &str| served, Some(&demand_dir)).await.expect("test: answered");
            assert!(matches!(waiting, Remainder::Waiting { needed: 51_712, served: s } if s == served), "{why}: {waiting:?}");
            assert_eq!(held(), 0, "{why}: nothing goes back while it would only park again");
        }
        let grown = |_: &str| Some(64_256);
        let returned = return_parked_remainder(&trigger.state, &board, &root, &job, &grown, Some(&demand_dir)).await.expect("test: returned");
        assert!(matches!(&returned, Remainder::Submitted(o) if o.success), "{returned:?}");
        assert_eq!(held(), 3, "every parked example goes back whole, the longest included: the next partition sends or parks it");
        let again = return_parked_remainder(&trigger.state, &board, &root, &job, &grown, Some(&demand_dir)).await.expect("test: answered");
        assert!(matches!(again, Remainder::AlreadyReturned), "{again:?}");
        // the operator's whole return of the same job is the same return by identity: a replay
        let whole = return_job(&trigger.state, &board, &root, all).await.expect("test: replay");
        assert!(whole.success, "{whole:?}");
        assert_eq!(held(), 3, "returned once across both routes");

        // a crash after her bucket took it, before the receipt: the next pass replays by identity
        let crashed = failed(Uuid::new_v4(), vec![], vec![0, 1, 2]);
        let first = return_parked_remainder(&trigger.state, &board, &root, &crashed, &grown, Some(&demand_dir)).await.expect("test: returned");
        assert!(matches!(&first, Remainder::Submitted(o) if o.success), "{first:?}");
        std::fs::remove_file(job_dir_under(&root, "Kimi", "code/owner", crashed.local_id).join(REMAINDER_RECEIPT)).expect("test: lose the receipt");
        let replayed = return_parked_remainder(&trigger.state, &board, &root, &crashed, &grown, Some(&demand_dir)).await.expect("test: replayed");
        assert!(matches!(&replayed, Remainder::Submitted(o) if o.success), "{replayed:?}");
        assert_eq!(held(), 6, "the crashed job's three, once");

        // both routes at once, neither past its receipt: one return by identity
        let raced = failed(Uuid::new_v4(), vec![], vec![0, 1, 2]);
        let (by_pass, by_operator) = tokio::join!(
            return_parked_remainder(&trigger.state, &board, &root, &raced, &grown, Some(&demand_dir)),
            return_job(&trigger.state, &board, &root, raced.local_id),
        );
        assert!(by_pass.is_ok() && by_operator.is_ok(), "{by_pass:?} {by_operator:?}");
        assert_eq!(held(), 9, "the raced job's three, once");

        // what this catches (card a9e94342): a stranded job whose demand serving was never told
        // (partitioned before #5557) waiting forever beside a window that never grows for it.
        // Below its window it goes back ONCE, so a partition records and prices it.
        let untold = failed(Uuid::new_v4(), vec![], vec![0, 1, 2]);
        let nowhere = dir.path().join("no-demand-recorded");
        let back = return_parked_remainder(&trigger.state, &board, &root, &untold, &|_: &str| Some(49_664), Some(&nowhere)).await.expect("test: returned");
        assert!(matches!(&back, Remainder::Submitted(o) if o.success), "unrecorded demand goes back to be recorded: {back:?}");
        assert_eq!(held(), 12, "the untold job's three, once");
        let again = return_parked_remainder(&trigger.state, &board, &root, &untold, &|_: &str| Some(49_664), Some(&nowhere)).await.expect("test: answered");
        assert!(matches!(again, Remainder::AlreadyReturned), "once, by its receipt: {again:?}");
        // ...and recorded its demand before going back: the next all-parked job of the same
        // examples (its partition's write failed, say) WAITS, so no cross-job loop (Codex on #5569)
        let next = failed(Uuid::new_v4(), vec![], vec![0, 1, 2]);
        let waits = return_parked_remainder(&trigger.state, &board, &root, &next, &|_: &str| Some(49_664), Some(&nowhere)).await.expect("test: answered");
        assert!(matches!(waits, Remainder::Waiting { needed: 51_712, .. }), "the successor job waits: {waits:?}");
        assert_eq!(held(), 12, "nothing added by the successor");
        // a store that cannot answer, or cannot be written, holds the job and adds nothing
        let corrupt = dir.path().join("corrupt-demand");
        std::fs::create_dir_all(&corrupt).expect("test: dir");
        std::fs::write(corrupt.join(format!("{persona}.json")), b"{ torn").expect("test: corrupt");
        let torn = failed(Uuid::new_v4(), vec![], vec![0, 1, 2]);
        let held_up = return_parked_remainder(&trigger.state, &board, &root, &torn, &|_: &str| Some(49_664), Some(&corrupt)).await;
        assert!(matches!(held_up, Err(CommandError::Internal(_))), "a corrupt store is not 'unrecorded': {held_up:?}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let sealed = dir.path().join("sealed-demand");
            std::fs::create_dir_all(&sealed).expect("test: dir");
            std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o555)).expect("test: read-only");
            // as root a read-only dir is still writable: only a sealed one tests the refusal
            let really_sealed = std::fs::write(sealed.join("probe"), b"x").is_err();
            if really_sealed {
                let unwritable = return_parked_remainder(&trigger.state, &board, &root, &torn, &|_: &str| Some(49_664), Some(&sealed)).await;
                assert!(matches!(unwritable, Err(CommandError::Internal(_))), "a demand it cannot record is never returned: {unwritable:?}");
            }
            std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o755)).expect("test: restore");
        }
        assert_eq!(held(), 12, "nothing added by a store that cannot answer or be written");

        // a partition missing a parked example's measured row can't have its demand recorded
        // whole, so it is refused and nothing returns (Codex on #5569: a partial record loops)
        let partial = failed(Uuid::new_v4(), vec![], vec![0, 1, 2]);
        let pd = job_dir_under(&root, "Kimi", "code/owner", partial.local_id);
        std::fs::write(pd.join("partition.json"), serde_json::json!({
            "window": 49664, "sent": [], "parked": [0, 1, 2], "rows": [row(0, 51_670), row(1, 64_041)]
        }).to_string()).expect("test: partition missing row 2");
        let fresh = dir.path().join("fresh-demand");
        let refused_partial = return_parked_remainder(&trigger.state, &board, &root, &partial, &|_: &str| Some(49_664), Some(&fresh)).await;
        assert!(matches!(refused_partial, Err(CommandError::Invalid(ref why)) if why.contains("parked example 2")), "{refused_partial:?}");
        assert_eq!(held(), 12, "nothing added by a partially measured partition");

        // a failed job that SENT examples is the refused-job return's, never this route's
        let refused = failed(Uuid::new_v4(), vec![0], vec![1, 2]);
        assert!(matches!(return_parked_remainder(&trigger.state, &board, &root, &refused, &grown, Some(&demand_dir)).await, Ok(Remainder::Nothing)));
        assert_eq!(held(), 12, "nothing added");
    }

    // what this catches (the 5090, 2026-10-05; and 2026-10-07, a return wearing its submission's id): ~500 of Kimi's examples stranded in the
    // directories of 18 jobs that ended without training, with no path back to her bucket.
    // A returned job's examples land in her bucket ONCE (a second return is a replay), and
    // a job still open, or one that trained, is refused rather than returned.
    #[tokio::test]
    async fn a_job_that_never_trained_returns_its_examples_once_and_a_trained_or_open_one_is_refused() {
        let (trigger, _executor, _db) = build_runtime_trigger_only().await;
        let dir = tempfile::tempdir().expect("test: tempdir");
        let board = TrainingJobBoard::with_ledger(Some(dir.path().join("jobs-ledger.jsonl")));
        let root = dir.path().join("genome");
        let persona = Uuid::from_u128(7);
        let watched = |job: Uuid| WatchedJob {
            trigger_dispatch_id: None,
            handle: JobHandle { provider_id: "engine-local".into(), provider_job_id: "x".into(), local_id: job },
            persona_id: persona,
            persona_name: "Kimi".into(),
            base_model: "ggml-org/Qwen3.8-27B-GGUF".into(),
            trait_kind: "code/owner".into(),
            eval_set: None,
            signature: None,
            decision: None,
        };
        let write_request = |job: Uuid| {
            let jd = job_dir_under(&root, "Kimi", "code/owner", job);
            std::fs::create_dir_all(&jd).expect("test: job dir");
            // what the trigger writes for a job: its request, examples inline
            let request = serde_json::json!({
                "personaId": persona.to_string(),
                "personaName": "Kimi",
                "baseModel": "ggml-org/Qwen3.8-27B-GGUF",
                "traitKind": "code/owner",
                "dataset": {
                    "examples": [ex("p1", "c1"), ex("p2", "c2"), ex("p3", "c3")],
                    "source": TrainingSource::TeacherSynthesized,
                    "validationSplit": 0.0
                }
            });
            std::fs::write(jd.join("request.json"), serde_json::to_vec(&request).expect("test: json")).expect("test: write");
            jd
        };
        let ended_for = |job: Uuid, persona: Uuid, name: &str| {
            let mut w = watched(job);
            w.persona_id = persona;
            w.persona_name = name.into();
            board.register(w);
            board.claim(job, &TrainingStatus::Failed { error: "ended before training".into() });
        };
        let ended = |job: Uuid| ended_for(job, persona, "Kimi");

        let stranded = Uuid::new_v4();
        ended(stranded);
        write_request(stranded);
        let first = return_job(&trigger.state, &board, &root, stranded).await.expect("test: returned");
        assert!(first.success, "{first:?}");
        let second = return_job(&trigger.state, &board, &root, stranded).await.expect("test: replay");
        assert!(second.success, "{second:?}");
        let key = crate::modules::training_trigger::BucketKey {
            persona_id: persona,
            trait_kind: "code/owner".into(),
            base_model: "ggml-org/Qwen3.8-27B-GGUF".into(),
        };
        let held = trigger.state.buckets.get(&key).map(|b| b.examples.len());
        assert_eq!(held, Some(3), "the three examples are in her bucket once, not twice");

        // Fable on #4800: a return into a bucket that already holds a producer's batch
        // (code traits carry the gym's evalSet; the adapter wrote a default lora into the
        // job's request.json) joins that bucket instead of being refused InconsistentBucket.
        let other = Uuid::from_u128(8);
        let producer = SubmitParams {
            submission_id: None,
            persona_id: other,
            persona_name: "Sahar".into(),
            base_model: "ggml-org/Qwen3.8-27B-GGUF".into(),
            trait_kind: "code/owner".into(),
            examples: vec![ex("q", "a")],
            source: TrainingSource::TeacherSynthesized,
            eval_set: Some("docs/genome/coder-eval.jsonl".into()),
            lora: None,
            schedule: None,
            local_artifact_dir: None,
            preferred_provider: None,
            min_examples: Some(50),
            validation_split: None,
        };
        assert!(submit_batch(&trigger.state, producer).await.expect("test: producer").success);
        let joining = Uuid::new_v4();
        ended_for(joining, other, "Sahar");
        let jd = job_dir_under(&root, "Sahar", "code/owner", joining);
        std::fs::create_dir_all(&jd).expect("test: dir");
        std::fs::write(jd.join("request.json"), serde_json::json!({
            "personaId": other.to_string(), "personaName": "Sahar",
            "baseModel": "ggml-org/Qwen3.8-27B-GGUF", "traitKind": "code/owner",
            "dataset": {"examples": [ex("r1", "s1"), ex("r2", "s2")], "source": TrainingSource::TeacherSynthesized, "validationSplit": 0.1},
            "evalSet": "docs/genome/coder-eval.jsonl",
            "lora": {"rank": 8, "alpha": 16, "dropout": 0.0, "targetModules": ["q_proj", "v_proj"]}
        }).to_string()).expect("test: write");
        let joined = return_job(&trigger.state, &board, &root, joining).await.expect("test: joined");
        assert!(joined.success, "a return joins the bucket's policy: {joined:?}");
        let other_key = crate::modules::training_trigger::BucketKey {
            persona_id: other,
            trait_kind: "code/owner".into(),
            base_model: "ggml-org/Qwen3.8-27B-GGUF".into(),
        };
        assert_eq!(trigger.state.buckets.get(&other_key).map(|b| b.examples.len()), Some(3), "1 produced + 2 returned");

        // regression for the 5090, 2026-10-07 (34 of Kimi's examples): a job the trigger
        // dispatched is registered under the id of the submission that filled it. The return
        // keyed its batch by that id, so the store read it as the original submission
        // (AlreadyAccepted, nothing appended). The return lands under its own identity.
        let third = Uuid::from_u128(9);
        let filled_by = Uuid::new_v4();
        let original = SubmitParams {
            submission_id: Some(filled_by),
            persona_id: third,
            persona_name: "Aris".into(),
            base_model: "ggml-org/Qwen3.8-27B-GGUF".into(),
            trait_kind: "code/owner".into(),
            examples: vec![ex("o1", "p1")],
            source: TrainingSource::TeacherSynthesized,
            eval_set: None,
            lora: None,
            schedule: None,
            local_artifact_dir: None,
            preferred_provider: None,
            min_examples: Some(50),
            validation_split: None,
        };
        assert!(submit_batch(&trigger.state, original).await.expect("test: original").success);
        ended_for(filled_by, third, "Aris");
        let jd = job_dir_under(&root, "Aris", "code/owner", filled_by);
        std::fs::create_dir_all(&jd).expect("test: dir");
        std::fs::write(jd.join("request.json"), serde_json::json!({
            "personaId": third.to_string(), "personaName": "Aris",
            "baseModel": "ggml-org/Qwen3.8-27B-GGUF", "traitKind": "code/owner",
            "dataset": {"examples": [ex("t1", "u1"), ex("t2", "u2")], "source": TrainingSource::TeacherSynthesized, "validationSplit": 0.1}
        }).to_string()).expect("test: write");
        // A legacy row without a dispatch receipt is ambiguous: it may itself be an
        // earlier successful return. The durable owner's regression covers proven
        // dispatches with both identical and changed payloads across restart.
        let back = return_job(&trigger.state, &board, &root, filled_by).await.expect("test: legacy ownership diagnostic");
        assert_eq!(back.error_kind.as_deref(), Some("RecoveryRequired"), "{back:?}");
        let third_key = crate::modules::training_trigger::BucketKey {
            persona_id: third,
            trait_kind: "code/owner".into(),
            base_model: "ggml-org/Qwen3.8-27B-GGUF".into(),
        };
        assert_eq!(trigger.state.buckets.get(&third_key).map(|b| b.examples.len()), Some(1), "ambiguous legacy ownership never duplicates examples");

        let open = Uuid::new_v4();
        board.register(watched(open));
        write_request(open);
        assert!(matches!(return_job(&trigger.state, &board, &root, open).await, Err(CommandError::Invalid(_))), "an open job trains or resumes itself");

        let trained = Uuid::new_v4();
        ended(trained);
        let jd = write_request(trained);
        for artifact in ["checkpoints/LATEST", "adapters/adapter_config.json", "adapters/adapter.gguf"] {
            let path = jd.join(artifact);
            std::fs::create_dir_all(path.parent().expect("test: parent")).expect("test: artifact dir");
            std::fs::write(&path, "trained").expect("test: artifact");
            assert!(matches!(return_job(&trigger.state, &board, &root, trained).await, Err(CommandError::Invalid(_))), "a trained job's outcome is its adapter: {artifact}");
            std::fs::remove_file(path).expect("test: next artifact format");
        }
        let completed = Uuid::new_v4();
        board.register(watched(completed));
        board.claim(completed, &TrainingStatus::Completed {
            artifact: crate::genome::fine_tuning::types::TrainingArtifact {
                model_id: "already-trained".into(), local_path: None,
                format: crate::genome::fine_tuning::types::ArtifactFormat::ProviderHosted,
                metrics: Default::default(),
            },
        });
        assert!(matches!(return_job(&trigger.state, &board, &root, completed).await, Err(CommandError::Invalid(_))), "completed receipt refuses even when artifacts are absent");
    }
}
