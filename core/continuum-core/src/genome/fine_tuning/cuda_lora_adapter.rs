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
    available_bytes: u64,
    micro_batch_size: u32,
    revision: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CudaPlan {
    memory_bytes: u64,
    micro_batch_size: u32,
    revision: Option<String>,
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
            let config = directory.join("request.json");
            let plan_path = directory.join("plan.json");
            let mut spec = CudaSpec {
                request,
                canonical_base,
                memory_bytes: 0,
                // Driver-free is not admission headroom: on Windows CUDA reported
                // 31.8 GB free while serving occupied 25.9 GiB. The governor
                // accounted for that residency and exposed only ~6 GB to training.
                available_bytes: crate::resources::ResourceDaemon::global()
                    .ok_or_else(|| failure("CUDA training requires the resource governor"))?
                    .available_for(
                        &format!("genome-train:{id}"),
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
