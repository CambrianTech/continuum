//! `genome/training-trigger/return` hands the examples of a job that never trained back to
//! her bucket.
//!
//! A job takes its examples out of the bucket when it is dispatched. If it then ends
//! without training a step (killed before it ran, refused at admission, ended by a
//! restart that nothing resumed), those examples sit in its directory and no fill ever
//! sees them again. On 2026-10-05 the 5090 stranded about 500 of Kimi's that way (18
//! jobs). This verb puts them back through the one acceptance path, keyed by the job's
//! id so a second return of the same job is a replay, never a second copy.
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

use super::submit::{return_request, SubmitOutcome};
use crate::genome::fine_tuning::TrainingJobBoard;
use crate::modules::training_trigger::{genome_root, job_dir_under, read_job_request, TrainingTriggerState};
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
    /// (its adapter is its outcome). The job id is the batch identity, so returning the same
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
    let dir = job_dir_under(root, &job.persona_name, &job.trait_kind, job_id);
    if dir.join("checkpoints").join("LATEST").is_file() || dir.join("adapters").join("adapter_config.json").is_file() {
        return Err(CommandError::Invalid(format!(
            "job {job_id} trained (its checkpoint or adapter is in {}): its outcome is that adapter, not its examples",
            dir.display()
        )));
    }
    let request = read_job_request(&dir).map_err(CommandError::NotFound)?;
    let outcome = return_request(state, request, job_id).await?;
    if outcome.success {
        board.journal_returned(job_id, job_id, &serde_json::json!("genome/training-trigger/return"), 0); // not held by a Took: an operator returned it, named by the verb
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::training_trigger::submit::{submit_batch, SubmitParams};
    use crate::commands::training_trigger::test_support::{build_runtime_trigger_only, ex};
    use crate::genome::fine_tuning::job_board::WatchedJob;
    use crate::genome::fine_tuning::types::{JobHandle, TrainingSource, TrainingStatus};

    // what this catches (the 5090, 2026-10-05): ~500 of Kimi's examples stranded in the
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

        let open = Uuid::new_v4();
        board.register(watched(open));
        write_request(open);
        assert!(matches!(return_job(&trigger.state, &board, &root, open).await, Err(CommandError::Invalid(_))), "an open job trains or resumes itself");

        let trained = Uuid::new_v4();
        ended(trained);
        let jd = write_request(trained);
        std::fs::create_dir_all(jd.join("checkpoints")).expect("test: ck");
        std::fs::write(jd.join("checkpoints").join("LATEST"), "step-8").expect("test: latest");
        assert!(matches!(return_job(&trigger.state, &board, &root, trained).await, Err(CommandError::Invalid(_))), "a trained job's outcome is its adapter");
    }
}
