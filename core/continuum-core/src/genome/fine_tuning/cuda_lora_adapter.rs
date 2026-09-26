//! CUDA QLoRA through the same native-job owner as MLX. Python owns tensor
//! kernels; the substrate owns identity, admission, cancellation and receipts.
use super::native_jobs::{default_lora, default_schedule, job_dir_for, NativeJobs, PreparedJob};
use super::{
    ArtifactFormat, FineTuningAdapter, FineTuningCapabilities, FineTuningError, JobHandle,
    JobMetrics, TrainerHardware, TrainingArtifact, TrainingJobRequest, TrainingStatus,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

use super::numerics;

/// What `cuda_train.py` records in `training-provenance.json` as `quantization`, and
/// therefore the numerics every adapter from this adapter is fit against. One name in
/// two files: if the trainer's quantisation changes, this changes with it.
const TRAINER_QUANTIZATION: &str = "nf4-double";

pub const PROVIDER_ID: &str = "cuda-local";

pub struct CudaLoraFineTuner {
    jobs: NativeJobs,
    python: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CudaSpec {
    #[serde(flatten)]
    request: TrainingJobRequest,
    canonical_base: String,
    memory_bytes: u64,
    /// Governed VRAM free NOW — the admission question: the job waits on this.
    available_bytes: u64,
    /// Governed VRAM the job could be GRANTED — the planning question: the window is
    /// decided against this. Capacity minus the residents the period will not release,
    /// i.e. serving's own lane added back (`budget_for_replacing`, the add-back serving
    /// uses for itself). Cormac + Fable on #4396: a window shed against free-now (2 GB
    /// with her lane up) refuses every dispatch made while she serves, and a job that
    /// parks, admits after the unload with 31 GB free, would still train at the shed
    /// window. Free-now admits; grantable plans.
    grantable_bytes: u64,
    micro_batch_size: u32,
    revision: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CudaPlan {
    memory_bytes: u64,
    micro_batch_size: u32,
    /// The token window the planner could AFFORD, which may be shorter than the one
    /// requested: `micro_batch_size` floors at one example, so past that the window is
    /// the only lever left (see `cuda_train.py`'s `MINIMUM_TRAIN_TOKENS`). Absent from
    /// an older planner's output, in which case the requested window stands.
    #[serde(default)]
    sequence_length: Option<u32>,
    /// Present when the budget cannot hold even `MINIMUM_TRAIN_TOKENS` of one example:
    /// the planner does not shed to a floor the card cannot hold (an OOM, or a wait for
    /// capacity that never comes) — it reports the numbers and the owner refuses with
    /// them, the same honest refusal as the numerics check.
    #[serde(default)]
    unaffordable: Option<UnaffordableWindow>,
    revision: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnaffordableWindow {
    affordable_tokens: u64,
    minimum_tokens: u64,
    requested_tokens: u64,
}

fn failure(error: impl std::fmt::Display) -> FineTuningError {
    FineTuningError::LocalTrainerFailed(error.to_string())
}

impl CudaLoraFineTuner {
    pub fn new() -> Self {
        let python = crate::config_env::read("CUDA_TRAIN_PYTHON")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                // An absent explicit override selects the conventional local installation.
                let home = dirs::home_dir().unwrap_or_default(); // Without a home, the relative candidate must still pass is_file before execution.
                let root = home.join(".continuum/tools/cuda-training");
                if cfg!(windows) {
                    root.join("Scripts/python.exe")
                } else {
                    root.join("bin/python")
                }
            });
        Self {
            jobs: NativeJobs::new(PROVIDER_ID),
            python,
        }
    }
}

#[async_trait]
impl FineTuningAdapter for CudaLoraFineTuner {
    fn capabilities(&self) -> FineTuningCapabilities {
        FineTuningCapabilities {
            provider_id: PROVIDER_ID.into(),
            supports_lora: true,
            supports_validation: true,
            produces_local_artifact: true,
            supported_base_model_prefixes: vec![],
            requires: TrainerHardware::Cuda,
        }
    }

    async fn create_job(
        &self,
        mut request: TrainingJobRequest,
    ) -> Result<JobHandle, FineTuningError> {
        if !crate::inference_capability::probe_hardware_profile().has_cuda {
            return Err(FineTuningError::InvalidRequest(
                "cuda-local requires NVIDIA CUDA".into(),
            ));
        }
        let canonical_base = request.base_model.clone();
        request.base_model =
            crate::model_registry::artifacts::resolve_hf_source_for_model_id(&request.base_model)
                .map_err(FineTuningError::InvalidRequest)?;
        let schedule = request.schedule.get_or_insert_with(default_schedule);
        let lora = request.lora.get_or_insert_with(default_lora);
        if request.dataset.examples.is_empty()
            || schedule.epochs == 0
            || schedule.batch_size == 0
            || schedule.sequence_length < 2
            || !schedule.learning_rate.is_finite()
            || schedule.learning_rate <= 0.0
            || lora.rank == 0
            || lora.alpha == 0
            || !lora.dropout.is_finite()
            || !(0.0..1.0).contains(&lora.dropout)
        {
            return Err(FineTuningError::InvalidRequest(
                "invalid CUDA training data/schedule/LoRA geometry".into(),
            ));
        }
        if !self.python.is_file() {
            return Err(failure(format!("CUDA trainer Python absent at {}; provision cuda-training requirements or set CUDA_TRAIN_PYTHON", self.python.display())));
        }
        let id = Uuid::new_v4();
        let python = self.python.clone();
        Ok(self.jobs.prepare(id, move |progress| async move {
            let directory = job_dir_for(&request, id);
            let adapters = directory.join("adapters");
            tokio::fs::create_dir_all(&adapters)
                .await
                .map_err(failure)?;
            let script = directory.join("cuda_train.py");
            tokio::fs::write(&script, include_str!("cuda_train.py"))
                .await
                .map_err(failure)?;
            // THE ADAPTER MUST MEET THE NUMERICS IT WAS FIT AGAINST (Joel, 2026-09-25:
            // "should/does it qlora learn to match the quant level?"). A QLoRA adapter
            // partly learns to compensate its base's quantisation error, so serving it
            // over different numerics answers error that is not there and leaves the
            // error that is. Nothing joined the two before this: the trainer records
            // `nf4-double`, serving loads a GGUF whose quant lives in a filename.
            // Checked HERE — before the planner, the admission and the weights — so a
            // mismatch costs a message instead of an hour of card, and never arrives
            // disguised as "training did not help" at the adoption gate.
            //
            // The served artifact comes from the SAME resolver the lane launches from
            // (`resolve_gguf_for_model`, llama_server.rs:4421), so this reads serving's
            // own choice rather than a second copy of it. LIMIT, stated: under a
            // resident-override pin the loaded file can differ from what the resolver
            // now returns; the durable fix is the loaded path on `ServingSnapshot`, and
            // until then that case reports `Unmeasured` rather than a false match.
            let fit = numerics::WeightNumerics::from_trainer_label(TRAINER_QUANTIZATION);
            let served = crate::inference::llama_server::current_serving()
                .active_model
                .and_then(|id| {
                    crate::model_registry::try_global().and_then(|r| r.model(&id).cloned())
                })
                .and_then(|model| crate::model_registry::artifacts::resolve_gguf_for_model(&model))
                .and_then(|path| numerics::WeightNumerics::from_gguf_path(&path));
            let verdict = numerics::compare(fit.as_ref(), served.as_ref());
            crate::probe!(
                class = "genome.train.numerics",
                fit = fit.as_ref().map_or("unmeasured", |n| n.label.as_str()),
                served = served.as_ref().map_or("unmeasured", |n| n.label.as_str()),
                verdict = ?verdict,
                "the numerics this LoRA is fit against versus the numerics it will be served against"
            );
            if let numerics::NumericsMatch::Incompatible { bits_apart } = verdict {
                return Err(failure(format!(
 "refusing to train: this LoRA would be fit against {} and served against {} — {bits_apart:.2} bits per weight apart, a whole quantisation tier. The adapter would learn to correct base error it never meets, and the adoption gate would report that as absent lift rather than as this mismatch. Serve a quant within {} bits of the trainer's, or train against the served one.",
                    fit.as_ref().map_or("unmeasured".into(), |n| format!("{} ({:.2} bpw)", n.label, n.bits_per_weight)),
                    served.as_ref().map_or("unmeasured".into(), |n| format!("{} ({:.2} bpw)", n.label, n.bits_per_weight)),
                    numerics::REFUSAL_BITS,
                )));
            }
            let config = directory.join("request.json");
            let plan_path = directory.join("plan.json");
            let governor = crate::resources::ResourceDaemon::global()
                .ok_or_else(|| failure("CUDA training requires the resource governor"))?;
            let mut spec = CudaSpec {
                request,
                canonical_base,
                memory_bytes: 0,
                // Driver-free is not admission headroom: on Windows CUDA reported
                // 31.8 GB free while serving occupied 25.9 GiB. The governor
                // accounted for that residency and exposed only ~6 GB to training.
                available_bytes: governor.available_for(
                    &format!("genome-train:{id}"),
                    crate::resources::ResourceKind::Vram,
                ),
                // What the job can be granted once serving steps aside for the period:
                // free-now plus serving's own measured residency, capped at capacity.
                grantable_bytes: governor.budget_for_replacing(
                    crate::modules::serving_consumer::SERVING_CONSUMER_ID,
                    crate::resources::ResourceKind::Vram,
                ),
                micro_batch_size: 0,
                revision: None,
            };
            tokio::fs::write(&config, serde_json::to_vec(&spec).map_err(failure)?) // Disk request.json consumed by the separate Python planner process.
                .await
                .map_err(failure)?;
            // Config-only meta tensors: no model weights or GPU allocation before admission.
            let mut planning = tokio::process::Command::new(&python);
            planning
                .arg(&script)
                .arg("--config")
                .arg(&config)
                .arg("--output")
                .arg(&plan_path)
                .arg("--plan")
                .kill_on_drop(true);
            #[cfg(windows)]
            planning.creation_flags(0x08000000);
            let output = planning.output().await.map_err(failure)?;
            if !output.status.success() {
                return Err(failure(format!(
                    "CUDA training plan failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
            let plan: CudaPlan =
                serde_json::from_slice(&tokio::fs::read(&plan_path).await.map_err(failure)?)
                    .map_err(failure)?;
            if plan.memory_bytes == 0 {
                return Err(failure("CUDA planner returned an empty memory requirement"));
            }
            if let Some(window) = plan.unaffordable {
                crate::probe!(
                    class = "genome.train.unaffordable",
                    job = %id,
                    requested_tokens = window.requested_tokens,
                    affordable_tokens = window.affordable_tokens,
                    minimum_tokens = window.minimum_tokens,
                    floor_memory_bytes = plan.memory_bytes,
                    grantable_bytes = spec.grantable_bytes,
                    available_bytes = spec.available_bytes,
                    "the governed budget holds fewer tokens of one example than the shortest window worth training — refused with the numbers, not queued"
                );
                return Err(failure(format!(
                    "refusing to train: the grantable budget ({} B once serving steps aside; {} B free \
                     now) holds {} tokens of one example, under the {}-token floor a write-error-fix \
                     trajectory needs (requested {}; the floor itself would ask {} B). This card is too \
                     small for the recipe even empty — shorten its window or train it on a bigger node; \
                     waiting would never admit it.",
                    spec.grantable_bytes,
                    spec.available_bytes,
                    window.affordable_tokens,
                    window.minimum_tokens,
                    window.requested_tokens,
                    plan.memory_bytes,
                )));
            }
            // THE VRAM RECEIPT BEFORE STEP 0 (readiness item 1): what the plan asks, what the
            // governor exposes right now, and the headroom between them — a number, not a
            // hope, on the row the room reads before the run is admitted.
            crate::probe!(
                class = "training.job.planned",
                job = %id,
                base = spec.canonical_base.as_str(),
                examples = spec.request.dataset.examples.len() as u64,
                memory_bytes = plan.memory_bytes,
                available_bytes = spec.available_bytes,
                grantable_bytes = spec.grantable_bytes,
                headroom_bytes = spec.available_bytes.saturating_sub(plan.memory_bytes),
                micro_batch = plan.micro_batch_size as u64,
                "CUDA training plan: memory asked vs governed VRAM available before admission"
            );
            let reservation = crate::forge::training_admission::wait_for_training_memory(
                crate::resources::ResourceDaemon::global()
                    .ok_or_else(|| failure("CUDA training requires the resource governor"))?,
                &format!("genome-train:{id}"),
                plan.memory_bytes,
                |available| progress.waiting_for_capacity(plan.memory_bytes, available),
            )
            .await
            .map_err(FineTuningError::Transient)?;
            spec.memory_bytes = plan.memory_bytes;
            spec.micro_batch_size = plan.micro_batch_size;
            // The admitted window goes BACK to the trainer, so the chunker produces
            // exactly what the governor paid for. Without this the planner would fit a
            // shorter window and the run would then chunk at the requested one — an
            // admission for one geometry spent on another.
            if let (Some(admitted), Some(schedule)) =
                (plan.sequence_length, spec.request.schedule.as_mut())
            {
                if admitted < schedule.sequence_length {
                    crate::probe!(
                        class = "genome.train.window_shed",
                        requested = schedule.sequence_length as u64,
                        admitted = admitted as u64,
                        memory_bytes = plan.memory_bytes,
                        "a single example did not fit the governed budget — the token window was shed to what it affords, never the run"
                    );
                    schedule.sequence_length = admitted;
                }
            }
            spec.revision = plan.revision;
            tokio::fs::write(&config, serde_json::to_vec(&spec).map_err(failure)?) // Disk request.json carries the admitted plan to the Python trainer process.
                .await
                .map_err(failure)?;
            let mut command = tokio::process::Command::new(&python);
            command
                .arg(&script)
                .arg("--config")
                .arg(&config)
                .arg("--output")
                .arg(&adapters)
                // The admitted budget is spent on the weights first, so the peak has to
                // come out of what is left — and 1.34 GiB of the 5090's card was held
                // "reserved but unallocated" when Kimi's job b73456e6 died 1.89 GiB
                // short (2026-09-25 00:23Z). Expandable segments give that
                // fragmentation back rather than asking the governor for more card.
                .env("PYTORCH_CUDA_ALLOC_CONF", "expandable_segments:True");
            let model_id = format!("{PROVIDER_ID}:{}:{id}", spec.request.trait_kind);
            Ok(PreparedJob {
                command,
                output: adapters.clone(),
                parser: None,
                finish: Box::new(move |wall_clock_ms| {
                    // Closure ownership retains accounting through kill/reap on every outcome.
                    let _reservation = reservation;
                    let weights = adapters.join("adapter_model.safetensors");
                    if std::fs::metadata(&weights)
                        .map_err(|e| e.to_string())?
                        .len()
                        == 0
                    {
                        return Err("CUDA trainer produced empty adapter weights".into());
                    }
                    if !adapters.join("adapter_config.json").is_file() {
                        return Err("CUDA trainer produced no PEFT configuration".into());
                    }
                    let mut metrics: JobMetrics = serde_json::from_slice(
                        &std::fs::read(adapters.join("metrics.json")).map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?;
                    if metrics.trained_tokens == 0
                        || metrics.final_loss.is_none_or(|loss| !loss.is_finite())
                    {
                        return Err(
                            "CUDA trainer produced no finite measured learning receipt".into()
                        );
                    }
                    metrics.wall_clock_ms = wall_clock_ms;
                    Ok(TrainingArtifact {
                        model_id,
                        local_path: Some(adapters),
                        format: ArtifactFormat::PeftAdapterDir,
                        metrics,
                    })
                }),
            })
        }))
    }

    async fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
        self.jobs.poll(handle)
    }
    async fn cancel(&self, handle: &JobHandle) -> Result<(), FineTuningError> {
        self.jobs.cancel(handle)
    }
}

impl Default for CudaLoraFineTuner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (reviewer on #4396): a plan whose budget holds fewer tokens of
    // one example than MINIMUM_TRAIN_TOKENS is REFUSED with the numbers, never admitted
    // at a window the card cannot hold. The contract is the planner's `unaffordable`
    // object (camelCase) read here; an older planner's output, with neither field, still
    // deserializes as "the requested window stands".
    #[test]
    fn an_unaffordable_plan_carries_its_numbers_and_an_older_plan_still_reads() {
        let plan: CudaPlan = serde_json::from_str(
            r#"{"memoryBytes": 30800000000, "microBatchSize": 1, "sequenceLength": null,
                "unaffordable": {"affordableTokens": 96, "minimumTokens": 512, "requestedTokens": 3348},
                "revision": null}"#,
        )
        .expect("planner output");
        let window = plan.unaffordable.expect("the marker");
        assert_eq!(
            (window.affordable_tokens, window.minimum_tokens, window.requested_tokens),
            (96, 512, 3348)
        );
        assert_eq!(plan.sequence_length, None, "no window is admitted alongside a refusal");
        let shed: CudaPlan = serde_json::from_str(
            r#"{"memoryBytes": 1, "microBatchSize": 1, "sequenceLength": 2000, "unaffordable": null, "revision": null}"#,
        )
        .expect("shed plan");
        assert!(shed.unaffordable.is_none());
        assert_eq!(shed.sequence_length, Some(2000));
        let older: CudaPlan =
            serde_json::from_str(r#"{"memoryBytes": 1, "microBatchSize": 2, "revision": "abc"}"#).expect("older planner");
        assert!(older.unaffordable.is_none() && older.sequence_length.is_none());
    }
}
