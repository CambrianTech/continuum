//! In-engine LoRA training on the RESIDENT weights: the dream's trainer (charter
//! ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md, S3/S4). The lane that serves the base already
//! holds its weights at their served quantization; its `POST /train` (fork 1d028f43b) trains a
//! fresh LoRA in a second context beside the serving slots, yielding between batches while any
//! slot is busy. No second copy of the model, no trainer process, no PyTorch: the adapter is fit
//! against exactly the numerics it will be served over (QLoRA by construction), and it comes out
//! as a GGUF-lora the genome pages in directly.
//!
//! This backend drives that run IN PLACE through the shared job owner ([`NativeJobs`] with
//! [`Execution::InPlace`]): the same handles, probes, status and cancellation as the process
//! trainers. What is its own:
//! - the lane: the live lane whose recorded model IS the request's base; none = refused.
//! - the examples: a lived call (card ad107e18) is sent as the conversation it was served,
//!   with its tools and her reply's reasoning and tool calls, and loss on that reply only;
//!   any other example is `{prompt, completion}`. The engine renders both through the served
//!   chat template, one window each, with loss only on the trained assistant turns.
//! - admission: a MEASURED footprint per (model, window, rank, targets). A shape never run
//!   before is admitted by leasing ALL the governed VRAM free right now (nothing else can grow
//!   into the run); the lease goes to the engine as its memory budget, the engine measures the
//!   training graph before allocating it, and that measurement is recorded, so the next run of
//!   that shape leases its number. Never a guess: [`crate::inference::training_admission`] refuses unmeasured bytes.
//! - the artifact leaves the lanes' `--train-dir` (`engine-train`, swept of every file whose job
//!   is not live) for the job's own directory BEFORE the job goes terminal (Fable's invariant on
//!   #4438), and is a [`ArtifactFormat::GgufLora`].
use super::native_jobs::{default_lora, default_schedule, job_dir_for, Execution, InPlaceEnd, InPlaceRun, NativeJobs, PreparedJob, RunProgress};
use super::{ArtifactFormat, FineTuningAdapter, FineTuningCapabilities, FineTuningError, JobHandle, JobMetrics, ReattachOutcome, TrainerHardware, TrainingArtifact, TrainingExample, TrainingJobRequest, TrainingStatus};
use crate::inference::planning::training_plan::{calibration_floor, choose_chunk, choose_depth, choose_window, shallower_depths, default_chunk, exact_ruled_out, partition_of, refit_middle, train_chunk_for, train_window, window_for_need, with_run_skipped, GraphModel, need, Need, NeedSource, Partition, TRAINING_CHUNK, TRAINING_CHUNK_MIN, TRAINING_WINDOW_MIN};

use async_trait::async_trait;

use serde::{Deserialize, Serialize};

use serde_json::{json, Value};

use std::collections::BTreeMap;

use std::path::{Path, PathBuf};

use std::sync::{Arc, Mutex};

use std::time::Duration;

use tokio::sync::watch;

use uuid::Uuid;

mod governed_training_free;
pub use governed_training_free::*;
mod train_status;
mod engine_lane;
pub use train_status::*;
mod move_adapter;
use move_adapter::*;
mod engine_lora_fine_tuner;
use engine_lora_fine_tuner::*;

#[cfg(test)]
mod tests;
