//! Job lifecycle actor — one in-flight training run per
//! [`JobActor`].
//!
//! The substrate's RTOS doctrine (CONCURRENCY-STYLE-GUIDE,
//! `[[rtos-brain-no-region-on-hot-path]]`) says CPU-bound work goes
//! into [`tokio::task::spawn_blocking`]; the foreground tokio
//! runtime stays responsive. Training is CPU-bound (candle's tensor
//! ops are synchronous), so the actor runs the optimizer loop on a
//! blocking thread, publishes status snapshots via a
//! [`tokio::sync::watch`] channel, and watches an
//! [`AtomicBool`] cancel flag the [`JobController`] flips.
//!
//! ## Why an actor at all
//!
//! Without it, `LocalCandleFineTuner::create_job` would have to
//! block until training completes — the [`super::FineTuningAdapter`]
//! contract is `async fn create_job(...) -> JobHandle`, fast-return.
//! The actor decouples submission from progress: `create_job` spawns,
//! returns immediately; `poll` reads the watch channel for the
//! latest [`TrainingStatus`]; `cancel` flips the flag.
//!
//! ## Lifecycle
//!
//! ```text
//!   spawn_job ── publishes ──► Queued
//!       │
//!       │ epoch 1..=N         ── publishes ──► Running { progress, epoch }
//!       │
//!       ├─ cancel flag set?   ── publishes ──► Cancelled  ── exit
//!       │
//!       └─ all epochs done    ── writes safetensors
//!                                publishes ──► Completed { artifact }
//!                                exit
//!
//!   anything else throws      ── publishes ──► Failed { error }
//!                                exit
//! ```
//!
//! Terminal states are sticky — the watch channel keeps the final
//! value forever, so repeated `poll` calls after completion return
//! the same `Completed { artifact }` payload.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use candle_core::{DType, Device, Tensor};
use tokio::sync::watch;

use super::byte_tokenizer::{ByteTokenizer, BYTE_VOCAB};
use super::lora_module::{LoRAError, LoRAModule};
use super::safetensors_io::{write_lora_safetensors, SafetensorsIoError};
use super::training_loop::{DataLoader, LoRATrainer, TrainingError};
use super::types::{
    ArtifactFormat, JobMetrics, LoRAHyperparams, ScheduleParams, TrainingArtifact, TrainingDataset,
    TrainingStatus,
};

// ─── Defaults ────────────────────────────────────────────────────────

/// Defaults for [`LoRAHyperparams`] when the request doesn't carry
/// any. Mirror the values the genome/job-create validator suggests
/// (`alpha = rank * 2`, dropout=0.0, no target modules in standin).
pub(super) fn default_lora() -> LoRAHyperparams {
    LoRAHyperparams {
        rank: 8,
        alpha: 16,
        dropout: 0.0,
        target_modules: vec![],
    }
}

/// Defaults for [`ScheduleParams`] when the request doesn't carry
/// any. Conservative numbers — substrate doesn't know the dataset
/// size yet; 3 epochs × small batch is enough to verify the pipeline
/// end-to-end without burning compute on stand-in math.
pub(super) fn default_schedule() -> ScheduleParams {
    ScheduleParams {
        epochs: 3,
        batch_size: 4,
        sequence_length: 32,
        learning_rate: 1e-4,
    }
}

/// Maximum `sequence_length` the actor will accept from the wire.
///
/// Per Reviewer 3's BLOCK C5: `schedule.sequence_length: u32` flows
/// untouched from the wire to a synchronous `Tensor::rand(BYTE_VOCAB,
/// seq_len)` allocation on the foreground tokio worker. With no
/// cap, a caller sending `sequence_length: 1_000_000` would stall a
/// runtime worker on a ~1 GB sync allocation. This bound is
/// generous enough for any real LoRA training (8192 is past most
/// transformer context lengths in use) but short of DoS territory.
///
/// FUTURE (R2-LGTM2 re-review): when `SubstrateGovernor` grows a
/// `training_policy()` accessor with per-hardware-tier caps
/// (Mac Intel: 2048, MacBook Air M-series: 4096, M5+: 16384,
/// 5090: 32768), this `pub const` becomes a hardware-aware
/// `governor.training_policy().max_sequence_length(tier)` call.
/// DoS protection at the wire IS the right level for v1; tier-aware
/// caps are the v2 follow-up. See INFERENCE-LANES-REALISTIC.md
/// AdaptiveThroughputPlanner for the structural shape.
pub const MAX_SEQUENCE_LENGTH: u32 = 8192;

/// Maximum `batch_size` the actor will accept. Same rationale as
/// `MAX_SEQUENCE_LENGTH` — combined with seq_len, `batch * seq *
/// vocab * 4` is the per-batch allocation. 256 × 8192 × 257 × 4 ≈
/// 2 GB which is still beyond Mac Intel viability but well below
/// the wire-controlled-DoS threshold the cap is meant to prevent.
pub const MAX_BATCH_SIZE: u32 = 256;

// ─── Errors ──────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum JobActorError {
    #[error("dataset has no examples; nothing to train on")]
    EmptyDataset,

    #[error("schedule epochs must be > 0")]
    InvalidEpochs,

    #[error("schedule sequence_length must be in (0, {MAX_SEQUENCE_LENGTH}], got {0}")]
    InvalidSequenceLength(u32),

    #[error("schedule batch_size must be in (0, {MAX_BATCH_SIZE}], got {0}")]
    InvalidBatchSize(u32),

    #[error("output path is required for local-candle jobs")]
    MissingOutputPath,

    #[error("LoRA module construction: {0}")]
    LoRA(#[from] LoRAError),

    #[error("data loader: {0}")]
    Training(#[from] TrainingError),

    #[error("safetensors writeout: {0}")]
    Safetensors(#[from] SafetensorsIoError),

    #[error("candle tensor op: {0}")]
    Candle(#[from] candle_core::Error),
}

// ─── Controller ──────────────────────────────────────────────────────

/// Handle returned by [`spawn_job`]. Owns the cancel flag + the
/// watch receiver. Cloning is by `Clone` on `Arc`s + `watch::Receiver`.
#[derive(Debug, Clone)]
pub struct JobController {
    status_rx: watch::Receiver<TrainingStatus>,
    cancel_flag: Arc<AtomicBool>,
}

impl JobController {
    /// Latest published status. Cheap clone — TrainingStatus is small.
    pub fn current_status(&self) -> TrainingStatus {
        self.status_rx.borrow().clone()
    }

    /// Flip the cancel flag. The actor honors it at the next epoch
    /// boundary; the watch channel will then publish
    /// [`TrainingStatus::Cancelled`].
    ///
    /// Cancellation is idempotent — flipping a flag that's already
    /// set is a no-op.
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::Release);
    }

    /// True if the cancel flag has been set. Test-only inspection;
    /// the watch channel is the source of truth for status.
    #[cfg(test)]
    pub(super) fn is_cancel_requested(&self) -> bool {
        self.cancel_flag.load(Ordering::Acquire)
    }
}

// ─── Spawn ───────────────────────────────────────────────────────────

/// Inputs that flow into `spawn_job`. Bundled into a struct so the
/// `create_job` call site stays readable as the field count grows
/// (artifact-id / persona-id / device-pin all land here in
/// follow-ups).
#[derive(Debug, Clone)]
pub struct SpawnJobRequest {
    pub persona_name: String,
    pub base_model: String,
    pub trait_kind: String,
    pub dataset: TrainingDataset,
    pub schedule: Option<ScheduleParams>,
    pub lora: Option<LoRAHyperparams>,
    pub output_path: PathBuf,
}

/// Spawn the lifecycle actor. Validates the request synchronously,
/// constructs the LoRAModule + DataLoader on the calling task (cheap;
/// tokenization + batching are fast), then hands the trainer + loader
/// off to a [`tokio::task::spawn_blocking`] thread that runs the
/// optimizer loop.
///
/// On success returns a [`JobController`] whose watch channel is
/// pre-published with [`TrainingStatus::Queued`] before this
/// function returns — so the first `poll` from the adapter never
/// races the actor's first publish.
pub fn spawn_job(req: SpawnJobRequest) -> Result<JobController, JobActorError> {
    // Validation — fast, synchronous, BEFORE any tokio resource is
    // allocated. The contract: a bad request fails synchronously
    // here, not after spawning a doomed task.
    if req.dataset.examples.is_empty() {
        return Err(JobActorError::EmptyDataset);
    }
    let schedule = req.schedule.unwrap_or_else(default_schedule);
    if schedule.epochs == 0 {
        return Err(JobActorError::InvalidEpochs);
    }
    // Cap caller-controlled schedule dims BEFORE allocating the
    // synthetic base weight. Per Reviewer 3's BLOCK C5, an uncapped
    // sequence_length would let a wire caller stall a tokio worker
    // on a multi-GB synchronous Tensor::rand alloc.
    if schedule.sequence_length == 0 || schedule.sequence_length > MAX_SEQUENCE_LENGTH {
        return Err(JobActorError::InvalidSequenceLength(
            schedule.sequence_length,
        ));
    }
    if schedule.batch_size == 0 || schedule.batch_size > MAX_BATCH_SIZE {
        return Err(JobActorError::InvalidBatchSize(schedule.batch_size));
    }
    let lora = req.lora.unwrap_or_else(default_lora);

    // Stand-in module construction. Production wiring (next slice)
    // loads `req.base_model` from disk and injects A/B at each
    // `lora.target_modules` projection. Here we materialize a single
    // synthetic linear layer the trainer's gradient path exercises
    // end-to-end. Shape: [vocab, sequence_length] so cross-entropy
    // sees logits over the byte vocab from the standin input cast.
    let device = Device::Cpu;
    let base_weight = Tensor::rand(
        -0.1f32,
        0.1f32,
        (BYTE_VOCAB as usize, schedule.sequence_length as usize),
        &device,
    )?;
    let module = LoRAModule::new(base_weight, lora.rank, lora.alpha, DType::F32, &device)?;
    let trainer = LoRATrainer::new(module, schedule.learning_rate)?;

    // Pre-tokenize + batch the dataset. Building the loader on the
    // calling thread (not inside spawn_blocking) lets us surface
    // validation errors synchronously.
    let tokenizer = ByteTokenizer::new();
    let loader = DataLoader::new(
        &req.dataset.examples,
        &tokenizer,
        schedule.batch_size,
        schedule.sequence_length,
        &device,
    )?;

    // Watch channel pre-published with Queued. The actor's first
    // act inside spawn_blocking is to flip it to Running { 0%, 0 }.
    let (status_tx, status_rx) = watch::channel(TrainingStatus::Queued);
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_actor = cancel_flag.clone();

    let output_path = req.output_path.clone();
    let persona_name = req.persona_name.clone();
    let trait_kind = req.trait_kind.clone();

    tokio::task::spawn_blocking(move || {
        let started = Instant::now();
        run_actor(
            trainer,
            loader,
            schedule,
            output_path,
            persona_name,
            trait_kind,
            status_tx,
            cancel_flag_actor,
            started,
        );
    });

    Ok(JobController {
        status_rx,
        cancel_flag,
    })
}

// ─── Actor body ──────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn run_actor(
    mut trainer: LoRATrainer,
    loader: DataLoader,
    schedule: ScheduleParams,
    output_path: PathBuf,
    persona_name: String,
    trait_kind: String,
    status_tx: watch::Sender<TrainingStatus>,
    cancel_flag: Arc<AtomicBool>,
    started: Instant,
) {
    // Open with Running { 0%, 0 } so a poll right after spawn sees a
    // started job, not Queued.
    let _ = status_tx.send(TrainingStatus::Running {
        progress_pct: 0.0,
        current_epoch: 0,
    });

    let mut final_train_loss: Option<f32> = None;
    for epoch in 1..=schedule.epochs {
        if cancel_flag.load(Ordering::Acquire) {
            let _ = status_tx.send(TrainingStatus::Cancelled);
            return;
        }

        match trainer.train_epoch(&loader) {
            Ok(avg) => {
                final_train_loss = Some(avg);
                let pct = (epoch as f32) * 100.0 / (schedule.epochs as f32);
                let _ = status_tx.send(TrainingStatus::Running {
                    progress_pct: pct,
                    current_epoch: epoch,
                });
            }
            Err(e) => {
                let _ = status_tx.send(TrainingStatus::Failed {
                    error: format!("train_epoch (epoch {epoch}): {e}"),
                });
                return;
            }
        }
    }

    // Write safetensors. Failure here is a terminal Failed — the
    // training succeeded but the artifact isn't durable, so a
    // subsequent `poll` reporting Completed would be a lie per
    // [[no-fallbacks-ever]].
    if let Err(e) = write_lora_safetensors(trainer.module(), &output_path) {
        let _ = status_tx.send(TrainingStatus::Failed {
            error: format!("safetensors write: {e}"),
        });
        return;
    }

    let metrics = trainer.metrics();
    let wall_clock_ms = started.elapsed().as_millis() as u64;
    // Honest count of gradient-bearing tokens consumed. Per
    // Reviewer 1's BLOCK M1: the previous `steps × batch × seq_len`
    // formula inflated this by ~seq_len× in the stand-in path
    // because the stand-in trains on one target per sample, not
    // seq_len. The actor now reads the count accumulated inside
    // `LoRATrainer::train_step` (which counts non-pad first-targets
    // in the standin; will become attention_mask.sum() in
    // production wiring). The metric thus reflects what actually
    // flowed through gradient, not a schedule-derived guess. This
    // feeds the alloy provenance unmodified — honest provenance
    // per [[forge-alloy-secures-commodity-zero-trust-plus-reputation]].
    let trained_tokens: u64 = metrics.gradient_tokens_consumed;

    let artifact = TrainingArtifact {
        model_id: format!(
            "local-candle:{persona_name}:{trait_kind}:{}",
            output_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        ),
        local_path: Some(output_path),
        // The Candle skeleton (#231-#233) writes a synthetic-base LoRA
        // safetensors — not yet a gene loadable against a real base, so the
        // completion sentinel will refuse to page it in until that path is real.
        format: ArtifactFormat::CandleSafetensors,
        metrics: JobMetrics {
            trained_tokens,
            final_loss: final_train_loss.map(|v| v as f64),
            final_validation_loss: None,
            wall_clock_ms,
            cost_usd: None,
        },
    };
    let _ = status_tx.send(TrainingStatus::Completed { artifact });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::fine_tuning::types::{TrainingExample, TrainingSource};
    use tempfile::tempdir;

    fn example(p: &str, c: &str) -> TrainingExample {
        TrainingExample {
            prompt: p.into(),
            completion: c.into(),
            metadata: None,
        }
    }

    fn small_request(output_path: PathBuf) -> SpawnJobRequest {
        SpawnJobRequest {
            persona_name: "test-persona".into(),
            base_model: "synthetic".into(),
            trait_kind: "stand-in".into(),
            dataset: TrainingDataset {
                examples: vec![
                    example("hello", "world"),
                    example("foo", "bar"),
                    example("ping", "pong"),
                    example("aa", "bb"),
                ],
                source: TrainingSource::OperatorCurated,
                validation_split: 0.0,
            },
            schedule: Some(ScheduleParams {
                epochs: 2,
                batch_size: 2,
                sequence_length: 8,
                learning_rate: 1e-3,
            }),
            lora: Some(LoRAHyperparams {
                rank: 2,
                alpha: 4,
                dropout: 0.0,
                target_modules: vec![],
            }),
            output_path,
        }
    }

    async fn poll_until_terminal(controller: &JobController) -> TrainingStatus {
        for _ in 0..200 {
            let status = controller.current_status();
            if matches!(
                status,
                TrainingStatus::Completed { .. }
                    | TrainingStatus::Failed { .. }
                    | TrainingStatus::Cancelled
            ) {
                return status;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!(
            "actor never reached terminal state within timeout; last status: {:?}",
            controller.current_status()
        );
    }

    // what this catches: empty dataset rejected synchronously before
    // any tokio resource is allocated. A future refactor that
    // deferred validation into the actor would burn a spawn_blocking
    // thread on a doomed run.
    #[test]
    fn empty_dataset_rejected_synchronously() {
        let dir = tempdir().unwrap();
        let req = SpawnJobRequest {
            persona_name: "p".into(),
            base_model: "b".into(),
            trait_kind: "t".into(),
            dataset: TrainingDataset {
                examples: vec![],
                source: TrainingSource::OperatorCurated,
                validation_split: 0.0,
            },
            schedule: None,
            lora: None,
            output_path: dir.path().join("layer.safetensors"),
        };
        let err = spawn_job(req).err().expect("must reject");
        assert!(matches!(err, JobActorError::EmptyDataset));
    }

    // what this catches: 0 epochs rejected synchronously. Otherwise
    // the actor would publish Running once and skip straight to
    // safetensors write — the loop body would never execute.
    #[test]
    fn zero_epochs_rejected_synchronously() {
        let dir = tempdir().unwrap();
        let mut req = small_request(dir.path().join("layer.safetensors"));
        req.schedule = Some(ScheduleParams {
            epochs: 0,
            batch_size: 2,
            sequence_length: 8,
            learning_rate: 1e-3,
        });
        let err = spawn_job(req).err().expect("must reject");
        assert!(matches!(err, JobActorError::InvalidEpochs));
    }

    // what this catches: caller-controlled `sequence_length` is
    // capped before it reaches the synchronous `Tensor::rand` alloc.
    // Per Reviewer 3's BLOCK C5: an uncapped `u32` would let a wire
    // caller stall a tokio worker on a multi-GB sync allocation.
    // 0 and `MAX_SEQUENCE_LENGTH + 1` both reject; values up to the
    // cap pass.
    #[test]
    fn sequence_length_above_cap_rejected_synchronously() {
        let dir = tempdir().unwrap();
        let mut req = small_request(dir.path().join("layer.safetensors"));
        req.schedule = Some(ScheduleParams {
            epochs: 1,
            batch_size: 2,
            sequence_length: MAX_SEQUENCE_LENGTH + 1,
            learning_rate: 1e-3,
        });
        let err = spawn_job(req).err().expect("must reject");
        assert!(matches!(err, JobActorError::InvalidSequenceLength(_)));
    }

    #[test]
    fn sequence_length_zero_rejected_synchronously() {
        let dir = tempdir().unwrap();
        let mut req = small_request(dir.path().join("layer.safetensors"));
        req.schedule = Some(ScheduleParams {
            epochs: 1,
            batch_size: 2,
            sequence_length: 0,
            learning_rate: 1e-3,
        });
        let err = spawn_job(req).err().expect("must reject");
        assert!(matches!(err, JobActorError::InvalidSequenceLength(0)));
    }

    // what this catches: caller-controlled `batch_size` is capped
    // for the same DoS-protection reason as `sequence_length`. With
    // `batch_size = u32::MAX` and a small `sequence_length`, the
    // batch grouping in the data loader would attempt to materialize
    // a single batch larger than the dataset, but the per-batch
    // tensor allocation in candle uses batch_size as a dimension —
    // an unbounded value would corrupt downstream sizing logic.
    #[test]
    fn batch_size_above_cap_rejected_synchronously() {
        let dir = tempdir().unwrap();
        let mut req = small_request(dir.path().join("layer.safetensors"));
        req.schedule = Some(ScheduleParams {
            epochs: 1,
            batch_size: MAX_BATCH_SIZE + 1,
            sequence_length: 8,
            learning_rate: 1e-3,
        });
        let err = spawn_job(req).err().expect("must reject");
        assert!(matches!(err, JobActorError::InvalidBatchSize(_)));
    }

    // what this catches: parallel to sequence_length_zero — a
    // regression dropping `batch_size == 0` from the guard would
    // let a wire caller submit batch_size: 0, which would panic
    // downstream in candle's tensor dim allocation. Reviewer R1's
    // BLOCK on the asymmetric coverage: sequence_length had a
    // zero-rejected test but batch_size didn't.
    #[test]
    fn batch_size_zero_rejected_synchronously() {
        let dir = tempdir().unwrap();
        let mut req = small_request(dir.path().join("layer.safetensors"));
        req.schedule = Some(ScheduleParams {
            epochs: 1,
            batch_size: 0,
            sequence_length: 8,
            learning_rate: 1e-3,
        });
        let err = spawn_job(req).err().expect("must reject");
        assert!(matches!(err, JobActorError::InvalidBatchSize(0)));
    }

    // what this catches: BOUNDARY pin. The cap guards use `>` (the
    // value MAX_SEQUENCE_LENGTH itself is ACCEPTED, MAX+1 rejected).
    // A regression flipping `>` to `>=` would silently break callers
    // submitting exactly 8192 — and the existing
    // `_above_cap_rejected` test would still pass because MAX+1 is
    // rejected under either semantic. Pin both sides per Reviewer R1's
    // BLOCK on cap boundary asymmetry.
    //
    // These tests are `#[tokio::test]` (not `#[test]`) because
    // spawn_job internally calls `tokio::task::spawn_blocking`,
    // which requires a tokio runtime context. The synchronous-rejection
    // tests don't reach that call site (they error before spawn).
    #[tokio::test]
    async fn sequence_length_at_cap_accepted() {
        let dir = tempdir().unwrap();
        let mut req = small_request(dir.path().join("layer.safetensors"));
        // Need >= batch_size examples for DataLoader to form a batch
        // (partial last batch is dropped).
        req.dataset.examples = (0..4)
            .map(|i| example(&format!("p-{i}"), &format!("c-{i}")))
            .collect();
        req.schedule = Some(ScheduleParams {
            epochs: 1,
            batch_size: 2,
            sequence_length: MAX_SEQUENCE_LENGTH,
            learning_rate: 1e-3,
        });
        spawn_job(req).expect("spawn_job must accept sequence_length == MAX_SEQUENCE_LENGTH");
    }

    #[tokio::test]
    async fn batch_size_at_cap_accepted() {
        let dir = tempdir().unwrap();
        let mut req = small_request(dir.path().join("layer.safetensors"));
        req.dataset.examples = (0..(MAX_BATCH_SIZE as usize))
            .map(|i| example(&format!("p-{i}"), &format!("c-{i}")))
            .collect();
        req.schedule = Some(ScheduleParams {
            epochs: 1,
            batch_size: MAX_BATCH_SIZE,
            sequence_length: 8,
            learning_rate: 1e-3,
        });
        spawn_job(req).expect("spawn_job must accept batch_size == MAX_BATCH_SIZE");
    }

    // what this catches: happy-path lifecycle — actor reaches
    // Completed, writes the file, the artifact carries a non-None
    // local_path matching the requested location. A regression that
    // skipped the safetensors write but still published Completed
    // would be the worst kind of [[no-fallbacks-ever]] violation;
    // this test pins the write-then-publish ordering.
    #[tokio::test]
    async fn happy_path_writes_artifact_and_publishes_completed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("layer.safetensors");
        let controller = spawn_job(small_request(path.clone())).expect("spawn");

        let terminal = poll_until_terminal(&controller).await;
        match terminal {
            TrainingStatus::Completed { artifact } => {
                assert_eq!(artifact.local_path.as_deref(), Some(path.as_path()));
                assert!(path.exists(), "safetensors file must exist on completion");
                assert!(artifact.metrics.wall_clock_ms > 0);
                assert!(artifact.metrics.trained_tokens > 0);
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    // what this catches: post-terminal stickiness. After Completed
    // is published, polling again must return the same Completed
    // (the watch channel retains its last value). A regression that
    // dropped the sender mid-loop would surface here as a default
    // TrainingStatus value at the receiver.
    #[tokio::test]
    async fn terminal_state_is_sticky() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("layer.safetensors");
        let controller = spawn_job(small_request(path)).expect("spawn");

        let first = poll_until_terminal(&controller).await;
        let second = controller.current_status();
        assert!(matches!(first, TrainingStatus::Completed { .. }));
        assert!(matches!(second, TrainingStatus::Completed { .. }));
    }

    // what this catches: cancellation reaches Cancelled state and
    // the actor exits without writing the safetensors file. A
    // refactor that ignored the cancel flag would keep training and
    // eventually publish Completed — silently wasting compute.
    #[tokio::test]
    async fn cancel_before_first_epoch_yields_cancelled() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("layer.safetensors");
        // Bump epochs high so we have time to cancel before
        // training finishes on this small dataset.
        let mut req = small_request(path.clone());
        req.schedule = Some(ScheduleParams {
            epochs: 100,
            batch_size: 2,
            sequence_length: 8,
            learning_rate: 1e-4,
        });
        let controller = spawn_job(req).expect("spawn");
        controller.cancel();
        assert!(controller.is_cancel_requested());

        let terminal = poll_until_terminal(&controller).await;
        assert!(
            matches!(terminal, TrainingStatus::Cancelled),
            "expected Cancelled, got {terminal:?}"
        );
        assert!(
            !path.exists(),
            "cancelled run must NOT have written the safetensors file"
        );
    }
}
