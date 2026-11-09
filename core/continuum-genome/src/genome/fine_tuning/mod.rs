//! The vocabulary of learning a LoRA, and the forge's offline MLX run.
//!
//! A persona learns in place, inside the serving engine, on the weights she is served on
//! (`genome::learner`; ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md). What remains here is what
//! that learner and its readers share: the typed example and dataset (`types`), the shape and
//! schedule a learner takes when its request names none (`lora_defaults`), where a gene's
//! provenance is written (`job_dir`), how much she still has to learn (`training_demand`), and
//! the forge's MLX trainer (`mlx_train`), which builds published artifacts, never a resident's
//! gene.

pub mod batch_wind_down;
pub mod job_dir;
mod lora_defaults;
pub mod mlx_train;
pub mod training_demand;
pub mod types;

pub(crate) fn native_jobs_defaults() -> (types::LoRAHyperparams, types::ScheduleParams) {
    (lora_defaults::default_lora(), lora_defaults::default_schedule())
}

pub use types::{
    LivedCall, LoRAHyperparams, ScheduleParams, TrainingDataset, TrainingExample, TrainingJobRequest, TrainingSource,
};
