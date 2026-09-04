//! [`MlxLoraFineTuner`] — the Apple-Silicon LoRA trainer behind the
//! [`FineTuningAdapter`] seam.
//!
//! ## Why this adapter exists (the differentiator)
//!
//! unsloth's trainer is the NVIDIA/CUDA path; it does **not** train
//! LoRA on Apple Silicon. Apple's `mlx_lm.lora` does — natively, on the
//! Mac GPU (memory `unsloth-mlx-train-broken-on-mac`: the proven Mac
//! recipe). Registering that path as a first-class `FineTuningAdapter`
//! means the substrate covers a modality unsloth can't: a developer on
//! a MacBook trains a real LoRA locally, and the grid can lease the
//! best node per job (Apple → this adapter, NVIDIA → `local-candle` /
//! the cloud `openai`/unsloth adapter, cross-grid → the airc-routed
//! adapter the module doc anticipates). We serve the Mac + Hermes-local
//! crowd better than the NVIDIA-only trainers serve themselves.
//!
//! ## Where it sits in the seam
//!
//! Third trainer *modality* behind the same trait, proving the
//! interface holds across all three (the outlier-validation method):
//!   - `openai_adapter`        — cloud HTTP trainer (also unsloth /v1)
//!   - `local_candle_adapter`  — in-process Rust/Candle trainer
//!   - **this**                — out-of-process subprocess trainer
//!
//! ## What it does
//!
//! - **`create_job`**: gates on Apple Silicon + an available `mlx_lm`,
//!   materializes the dataset into MLX's `train.jsonl` / `valid.jsonl`
//!   layout, spawns `<python> -m mlx_lm.lora --train …` as a
//!   `tokio::process::Child` (module invocation, not inline python —
//!   the sanctioned subprocess path per `no-python-in-rs-files`),
//!   stores a [`JobSlot`] keyed by `local_id`, and spawns ONE watcher
//!   task that awaits the child and publishes terminal status over a
//!   `watch` channel (the canonical own-task + `watch::Sender` shape).
//! - **`poll`**: reads the slot's `watch::Receiver` — cheap, no work.
//! - **`cancel`**: kills the child; the watcher reports `Cancelled`.
//!
//! On `Completed`, the artifact's `local_path` is the adapter dir
//! holding `adapters.safetensors` + `adapter_config.json` — exactly
//! what `forge-custodian` converts to a GGUF-lora gene (the supply
//! side of task #32). This adapter trains; the custodian converts;
//! genome paging loads. No cloud hop, no unsloth, fully owned.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio::sync::watch;
use uuid::Uuid;

use super::adapter::{FineTuningAdapter, FineTuningCapabilities, FineTuningError, TrainerHardware};
use super::types::{
    ArtifactFormat, JobHandle, JobMetrics, LoRAHyperparams, ScheduleParams, TrainingArtifact,
    TrainingJobRequest, TrainingStatus,
};
use crate::inference_capability::probe_hardware_profile;
use crate::runtime;

/// Stable provider id — matches the model_registry provider convention.
pub const PROVIDER_ID: &str = "mlx-local";

/// Config key (config.env, the single owner) for the python interpreter
/// that can `import mlx_lm`. Defaults to the unsloth-studio env that
/// ships `mlx_lm` on this class of machine; override per host.
const MLX_PYTHON_KEY: &str = "MLX_PYTHON";

/// One in-flight or terminal job. The `watch::Receiver` carries the
/// latest [`TrainingStatus`]; the watcher task owns the matching
/// `Sender`. A terminal status stays latched so repeated `poll`s after
/// completion are idempotent (the reputation/lineage subsystem reads
/// metrics off the terminal status).
struct JobSlot {
    status: watch::Receiver<TrainingStatus>,
    /// Kill switch for `cancel` — `None` once the child has been taken
    /// by the watcher (the watcher owns the wait; cancel kills via this
    /// handle which shares the OS process through `kill_on_drop`).
    cancel: Arc<tokio::sync::Notify>,
}

/// Apple-Silicon `mlx_lm.lora` trainer. Holds a concurrent table of
/// job slots keyed by substrate-side `local_id`.
pub struct MlxLoraFineTuner {
    jobs: Arc<DashMap<Uuid, JobSlot>>,
    python: PathBuf,
}

impl MlxLoraFineTuner {
    /// Construct with the python interpreter resolved from config.env
    /// (`MLX_PYTHON`), falling back to the unsloth-studio env that
    /// ships `mlx_lm` on this machine class.
    pub fn new() -> Self {
        let python = crate::config_env::read(MLX_PYTHON_KEY)
            .map(PathBuf::from)
            .unwrap_or_else(default_mlx_python);
        Self {
            jobs: Arc::new(DashMap::new()),
            python,
        }
    }

    /// Test/observability — number of tracked jobs (in-flight + terminal).
    #[cfg(test)]
    pub(super) fn tracked_job_count(&self) -> usize {
        self.jobs.len()
    }
}

impl Default for MlxLoraFineTuner {
    fn default() -> Self {
        Self::new()
    }
}

/// Default python: the unsloth-studio venv that ships `mlx_lm` on this
/// machine class. Honestly a host-specific default; `MLX_PYTHON`
/// overrides it. (Not a fallback in the forbidden sense — it's the
/// documented default location, and a missing interpreter fails loud in
/// `create_job` rather than silently degrading.)
fn default_mlx_python() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".unsloth/studio/unsloth_studio/bin/python3")
}

/// Can this host actually run `mlx_lm.lora`? MLX targets the Apple GPU,
/// so the gate is the host's probed Metal flag — a deterministic
/// boolean read off the rich [`HardwareProfile`], never a parse of the
/// free-form `platform` string. Keeps the coordinator (and a direct
/// caller) from launching a trainer that would fail; per
/// `no-fallbacks-ever` we fail loud with the reason rather than no-op.
fn host_has_metal() -> bool {
    probe_hardware_profile().has_metal
}

#[async_trait]
impl FineTuningAdapter for MlxLoraFineTuner {
    fn capabilities(&self) -> FineTuningCapabilities {
        FineTuningCapabilities {
            provider_id: PROVIDER_ID.to_string(),
            supports_lora: true,
            supports_validation: true,
            // Trains real weights to a local adapter dir the custodian
            // converts to a GGUF-lora gene.
            produces_local_artifact: true,
            // Wildcard: mlx_lm.lora accepts any HF/MLX-loadable base the
            // caller names. The caller owns knowing the base is
            // MLX-loadable; create_job validates it's non-empty.
            supported_base_model_prefixes: vec![],
            // Apple's MLX path — the coordinator routes here only on a
            // host whose probed HardwareProfile reports a Metal device.
            requires: TrainerHardware::Metal,
        }
    }

    async fn create_job(&self, request: TrainingJobRequest) -> Result<JobHandle, FineTuningError> {
        let log = runtime::logger(PROVIDER_ID);

        // ── Preconditions (fail loud, name the cause) ──────────────
        if !host_has_metal() {
            return Err(FineTuningError::InvalidRequest(
                "mlx-local needs an Apple Silicon Metal device (host has none); \
                 route NVIDIA jobs to a CUDA trainer or a leased GPU node"
                    .into(),
            ));
        }
        if request.base_model.trim().is_empty() {
            return Err(FineTuningError::InvalidRequest(
                "base_model is empty — mlx_lm.lora --model needs an \
                 HF repo id or a local MLX model dir"
                    .into(),
            ));
        }
        // `base_model` is the CANONICAL registry id (GGUF-resolvable for
        // serving/eval). mlx_lm.lora needs the safetensors base instead, so
        // resolve the canonical id → the row's `hf_source` (fail loud if the
        // row declares no trainable base — see resolve_hf_source_for_model_id).
        let train_base =
            crate::model_registry::artifacts::resolve_hf_source_for_model_id(&request.base_model)
                .map_err(FineTuningError::InvalidRequest)?;
        // Prefer a LOCAL 4-bit MLX conversion of the base when one exists
        // (`<genome>/models/mlx-q4/<hf id with '/'→'_'>`). QLoRA on the
        // quantized base is how a 24B trains NEXT TO its own living serving
        // lane on unified memory: the bf16 base (44GB) + the 25GB server
        // SIGABRT'd Metal on the first lived-curriculum train (2026-07-10);
        // the 12GB 4-bit conversion coexists. A deliberate, LOGGED preference
        // for a stronger-fitting artifact — not a fallback: absent the
        // conversion we train the full base exactly as before, and its OOM
        // stays loud.
        let train_base = {
            let q4 = dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".continuum/genome/models/mlx-q4")
                .join(train_base.replace('/', "_"));
            if q4.join("config.json").exists() {
                crate::probe!(
                    class = "genome.train.base",
                    base = %train_base,
                    local_q4 = %q4.display(),
                    "training against the local 4-bit MLX conversion (fits beside the living lane)"
                );
                q4.to_string_lossy().into_owned()
            } else {
                train_base
            }
        };
        if request.dataset.examples.is_empty() {
            return Err(FineTuningError::InvalidRequest(
                "dataset has no examples".into(),
            ));
        }
        if !self.python.exists() {
            return Err(FineTuningError::LocalTrainerFailed(format!(
                "mlx python interpreter not found at {} — set {} in config.env",
                self.python.display(),
                MLX_PYTHON_KEY
            )));
        }

        let local_id = Uuid::new_v4();

        // ── Materialize the dataset into MLX's data dir layout ─────
        let job_dir = job_dir_for(&request, local_id);
        let data_dir = job_dir.join("data");
        let adapter_dir = job_dir.join("adapters");
        std::fs::create_dir_all(&data_dir).map_err(|e| {
            FineTuningError::LocalTrainerFailed(format!(
                "create data dir {}: {e}",
                data_dir.display()
            ))
        })?;
        std::fs::create_dir_all(&adapter_dir).map_err(|e| {
            FineTuningError::LocalTrainerFailed(format!(
                "create adapter dir {}: {e}",
                adapter_dir.display()
            ))
        })?;
        let chat_template =
            tokenizer_has_chat_template(&self.python, &job_dir, &train_base).await?;
        write_mlx_dataset(&data_dir, &request, chat_template)?;

        // ── Build the mlx_lm.lora --train invocation ───────────────
        let schedule = request.schedule.clone().unwrap_or_else(default_schedule);
        let lora = request.lora.clone().unwrap_or_else(default_lora);
        let iters = iters_for(&request, &schedule);

        // mlx_lm.lora reads rank/scale/dropout ONLY from a `-c` config YAML — there
        // are no CLI flags for them. Emit it (or mlx silently uses scale 20.0; see
        // `lora_config_yaml`) into the already-created adapter dir and pass it below.
        let config_path = adapter_dir.join("mlx_train_config.yaml");
        let scale = lora.alpha as f64 / (lora.rank.max(1) as f64);
        std::fs::write(&config_path, lora_config_yaml(&lora)).map_err(|e| {
            FineTuningError::LocalTrainerFailed(format!(
                "write mlx lora config {}: {e}",
                config_path.display()
            ))
        })?;

        let mut cmd = tokio::process::Command::new(&self.python);
        cmd.arg("-m")
            .arg("mlx_lm.lora")
            .arg("--model")
            .arg(&train_base)
            .arg("--train")
            .arg("--data")
            .arg(&data_dir)
            .arg("--adapter-path")
            .arg(&adapter_dir)
            .arg("--iters")
            .arg(iters.to_string())
            .arg("--batch-size")
            // Clamped to the SMALLEST split: mlx_lm iterates valid with the same
            // batch size and hard-errors when a split has fewer rows than one
            // batch ("Dataset must have at least batch_size=4" — killed the
            // 12-example lived-curriculum train, 2026-07-10). Small datasets are
            // the NORM for the lived loop (a day's corrections, not a corpus);
            // the schedule's batch is a ceiling, the data is the floor.
            .arg(effective_batch_size(&request, &schedule).to_string())
            .arg("--num-layers")
            .arg(lora.target_modules.len().max(8).to_string())
            .arg("--learning-rate")
            .arg(format!("{:e}", schedule.learning_rate))
            // Sequence cap is a MEMORY control, not just quality: activation
            // memory scales with batch × seq × model size, and mlx_lm's silent
            // 2048 default meant the schedule's sequence_length never reached
            // the trainer (found on job a96e2341 — Metal OOM at batch 4 beside
            // a resident llama-server; the operator's 3072 was never applied).
            .arg("--max-seq-length")
            .arg(schedule.sequence_length.to_string())
            .arg("-c")
            .arg(&config_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        log.info(&format!(
            "spawning mlx_lm.lora: model={} (canonical={}) iters={} batch={} \
             rank={} scale={:.1} data={} → adapters={}",
            train_base,
            request.base_model,
            iters,
            schedule.batch_size,
            lora.rank,
            scale,
            data_dir.display(),
            adapter_dir.display()
        ));

        let child = cmd.spawn().map_err(|e| {
            FineTuningError::LocalTrainerFailed(format!(
                "spawn {} -m mlx_lm.lora: {e}",
                self.python.display()
            ))
        })?;

        // ── Watcher: own the child, publish terminal status ────────
        let (tx, rx) = watch::channel(TrainingStatus::Queued);
        let cancel = Arc::new(tokio::sync::Notify::new());
        let model_id = format!("{PROVIDER_ID}:{}:{}", request.trait_kind, local_id);
        spawn_watcher(child, tx, cancel.clone(), adapter_dir, model_id);

        self.jobs.insert(local_id, JobSlot { status: rx, cancel });

        Ok(JobHandle {
            provider_id: PROVIDER_ID.to_string(),
            provider_job_id: local_id.to_string(),
            local_id,
        })
    }

    async fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
        match self.jobs.get(&handle.local_id) {
            Some(slot) => Ok(slot.status.borrow().clone()),
            None => Err(FineTuningError::UnknownHandle(handle.clone())),
        }
    }

    async fn cancel(&self, handle: &JobHandle) -> Result<(), FineTuningError> {
        match self.jobs.get(&handle.local_id) {
            Some(slot) => {
                slot.cancel.notify_waiters();
                Ok(())
            }
            None => Err(FineTuningError::UnknownHandle(handle.clone())),
        }
    }
}

/// Spawn the single watcher task for one job. Owns the child process,
/// races completion against the cancel notification, and latches the
/// terminal [`TrainingStatus`] into the watch channel. Canonical
/// own-task + `watch::Sender` shape (CONCURRENCY-STYLE-GUIDE).
fn spawn_watcher(
    mut child: tokio::process::Child,
    tx: watch::Sender<TrainingStatus>,
    cancel: Arc<tokio::sync::Notify>,
    adapter_dir: PathBuf,
    model_id: String,
) {
    tokio::spawn(async move {
        let log = runtime::logger(PROVIDER_ID);
        let _ = tx.send(TrainingStatus::Running {
            progress_pct: 0.0,
            current_epoch: 0,
        });
        let started = Instant::now();

        // Stream the trainer's pipes LIVE instead of buffering to exit
        // (`wait_with_output`), which made a running job unfalsifiable: no loss
        // curve existed anywhere until the process died. Every line lands in
        // `trainer.log`; `Iter N: … loss X` lines additionally parse into
        // `loss.jsonl` (the live learning proof `genome/job-status` and the
        // evidence engine can read). The last stderr lines are kept for the
        // failure message the old path built from the buffered output.
        let stderr_tail: Arc<std::sync::Mutex<std::collections::VecDeque<String>>> =
            Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(stream_trainer_pipe(
                stdout,
                adapter_dir.join("trainer.log"),
                adapter_dir.join("loss.jsonl"),
                None,
            ));
        }
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(stream_trainer_pipe(
                stderr,
                adapter_dir.join("trainer.log"),
                adapter_dir.join("loss.jsonl"),
                Some(stderr_tail.clone()),
            ));
        }

        let outcome = tokio::select! {
            // mlx_lm.lora ran to completion (or failed).
            res = child.wait() => res,
            // Operator cancelled — kill is via kill_on_drop when we
            // drop the child by leaving the select with a Cancelled.
            _ = cancel.notified() => {
                let _ = tx.send(TrainingStatus::Cancelled);
                log.warn("mlx_lm.lora job cancelled by operator");
                return;
            }
        };

        let wall_clock_ms = started.elapsed().as_millis() as u64;
        let status = match outcome {
            Ok(exit) if exit.success() => {
                let safetensors = adapter_dir.join("adapters.safetensors");
                if !safetensors.exists() {
                    TrainingStatus::Failed {
                        error: format!(
                            "mlx_lm.lora exited 0 but {} is missing",
                            safetensors.display()
                        ),
                    }
                } else {
                    log.info(&format!(
                        "mlx_lm.lora completed in {wall_clock_ms}ms → {}",
                        adapter_dir.display()
                    ));
                    TrainingStatus::Completed {
                        artifact: TrainingArtifact {
                            model_id,
                            local_path: Some(adapter_dir),
                            // The MLX `adapters.safetensors` dir — NOT directly
                            // pageable. The completion sentinel dispatches a
                            // `forge/export` (gguf-lora) to the custodian to
                            // convert this into a loadable gene before eval.
                            format: ArtifactFormat::MlxAdapterDir,
                            metrics: JobMetrics {
                                wall_clock_ms,
                                ..Default::default()
                            },
                        },
                    }
                }
            }
            Ok(exit) => {
                let tail: String = stderr_tail
                    .lock()
                    .map(|d| d.iter().rev().take(8).cloned().collect::<Vec<_>>())
                    .unwrap_or_default()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                TrainingStatus::Failed {
                    error: format!("mlx_lm.lora exited {exit}: {tail}"),
                }
            }
            Err(e) => TrainingStatus::Failed {
                error: format!("mlx_lm.lora wait failed: {e}"),
            },
        };
        let _ = tx.send(status);
    });
}

/// Stream one trainer pipe: every line appends to `log_path`; `Iter N: … loss X`
/// lines additionally append a `{"iter","kind","loss","atMs"}` row to `loss_path`;
/// when `tail` is given (the stderr pipe) the last ~40 lines are retained for the
/// failure message. Line-buffered writes — the trainer emits a handful of lines
/// per minute, so this costs nothing.
async fn stream_trainer_pipe(
    pipe: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    log_path: PathBuf,
    loss_path: PathBuf,
    tail: Option<Arc<std::sync::Mutex<std::collections::VecDeque<String>>>>,
) {
    use tokio::io::AsyncBufReadExt;
    let mut lines = tokio::io::BufReader::new(pipe).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            use std::io::Write;
            let _ = writeln!(f, "{line}");
        }
        if let Some((iter, kind, loss)) = parse_loss_line(&line) {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&loss_path)
            {
                use std::io::Write;
                let _ = writeln!(
                    f,
                    "{}",
                    serde_json::json!({
                        "iter": iter,
                        "kind": kind,
                        "loss": loss,
                        "atMs": chrono::Utc::now().timestamp_millis(),
                    })
                );
            }
        }
        if let Some(t) = &tail {
            if let Ok(mut d) = t.lock() {
                d.push_back(line);
                while d.len() > 40 {
                    d.pop_front();
                }
            }
        }
    }
}

/// Parse an mlx_lm loss line — `Iter 100: Train loss 1.234, …` /
/// `Iter 200: Val loss 1.5, …` → `(100, "train", 1.234)`. Hand-rolled (no regex
/// dep): anything that isn't exactly this shape returns `None` and stays a plain
/// log line.
fn parse_loss_line(line: &str) -> Option<(u64, &'static str, f64)> {
    let rest = line.strip_prefix("Iter ")?;
    let (iter_s, rest) = rest.split_once(':')?;
    let iter: u64 = iter_s.trim().parse().ok()?;
    let kind = if rest.contains("Train loss") {
        "train"
    } else if rest.contains("Val loss") {
        "val"
    } else {
        return None;
    };
    let after = rest.split("loss").nth(1)?.trim_start();
    let num: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    let loss: f64 = num.parse().ok()?;
    Some((iter, kind, loss))
}

/// `~/.continuum/genome/<persona>/<trait_kind>/<job_uuid>/` — honors an
/// explicit `local_artifact_dir` override when the caller set one.
fn job_dir_for(request: &TrainingJobRequest, local_id: Uuid) -> PathBuf {
    if let Some(dir) = &request.local_artifact_dir {
        return dir.join(local_id.to_string());
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".continuum/genome")
        .join(request.persona_name.replace(['/', ' '], "_"))
        .join(sanitize(&request.trait_kind))
        .join(local_id.to_string())
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Does `train_base`'s tokenizer carry a chat template? Decides the dataset
/// schema: mlx_lm renders `{"prompt","completion"}` rows THROUGH
/// `tokenizer.apply_chat_template`, so a template-less base (Devstral/Tekken —
/// its template lives outside tokenizer_config) exits 1 mid-train ("Cannot use
/// chat template functions"; killed the first lived-curriculum train,
/// 2026-07-10). Probed under the SAME interpreter that runs mlx_lm.lora, from
/// the authoritative source (the tokenizer itself — the #74 doctrine), never
/// guessed from the model name. Probe failure is loud: a base whose tokenizer
/// can't even load will not train either.
async fn tokenizer_has_chat_template(
    python: &Path,
    job_dir: &Path,
    train_base: &str,
) -> Result<bool, FineTuningError> {
    let probe_path = job_dir.join("probe_chat_template.py");
    std::fs::write(&probe_path, include_str!("probe_chat_template.py")).map_err(|e| {
        FineTuningError::LocalTrainerFailed(format!("write chat-template probe: {e}"))
    })?;
    let out = tokio::process::Command::new(python)
        .arg(&probe_path)
        .arg(train_base)
        .output()
        .await
        .map_err(|e| FineTuningError::LocalTrainerFailed(format!("spawn probe: {e}")))?;
    if !out.status.success() {
        return Err(FineTuningError::LocalTrainerFailed(format!(
            "chat-template probe failed for {train_base}: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim() == "yes")
}

/// Write the dataset into MLX's expected `train.jsonl` / `valid.jsonl`.
/// `chat_template: true` → the `{"prompt","completion"}` schema (maps 1:1 from
/// [`super::types::TrainingExample`]; mlx_lm renders it through the tokenizer's
/// template). `false` → the raw `{"text"}` schema (prompt + completion joined),
/// the only schema a template-less tokenizer can train on — whole-text LM loss
/// instead of completion-masked, the honest capability of such a base. The
/// validation split is honored; with too few examples for a split we
/// still emit a (possibly 1-line) valid file so mlx_lm doesn't error on
/// a missing file.
fn write_mlx_dataset(
    data_dir: &Path,
    request: &TrainingJobRequest,
    chat_template: bool,
) -> Result<(), FineTuningError> {
    let examples = &request.dataset.examples;
    let split = request.dataset.validation_split.clamp(0.0, 0.5);
    let n_valid = ((examples.len() as f32) * split).floor() as usize;
    // Keep at least 1 training example; cap valid so train is non-empty.
    let n_valid = n_valid.min(examples.len().saturating_sub(1));
    let (valid, train) = examples.split_at(n_valid);

    let to_jsonl = |rows: &[super::types::TrainingExample]| -> Result<String, FineTuningError> {
        let mut s = String::new();
        for ex in rows {
            let line = if chat_template {
                serde_json::json!({
                    "prompt": ex.prompt,
                    "completion": ex.completion,
                })
            } else {
                serde_json::json!({
                    "text": format!("{}\n{}", ex.prompt, ex.completion),
                })
            };
            s.push_str(
                &serde_json::to_string(&line)
                    .map_err(|e| FineTuningError::LocalTrainerFailed(format!("encode row: {e}")))?,
            );
            s.push('\n');
        }
        Ok(s)
    };

    // mlx_lm always needs a non-empty train file; mirror train into
    // valid when the split rounded to zero so --train doesn't choke on
    // an empty valid.jsonl.
    let train_rows = if train.is_empty() {
        examples.as_slice()
    } else {
        train
    };
    let valid_rows = if valid.is_empty() {
        &train_rows[..1]
    } else {
        valid
    };

    std::fs::write(data_dir.join("train.jsonl"), to_jsonl(train_rows)?)
        .map_err(|e| FineTuningError::LocalTrainerFailed(format!("write train.jsonl: {e}")))?;
    std::fs::write(data_dir.join("valid.jsonl"), to_jsonl(valid_rows)?)
        .map_err(|e| FineTuningError::LocalTrainerFailed(format!("write valid.jsonl: {e}")))?;
    Ok(())
}

/// mlx_lm.lora counts iterations, not epochs. Derive iters from the
/// The batch size mlx_lm can actually run: the schedule's ask, clamped to the
/// smallest dataset split (mlx iterates BOTH splits at this batch size and
/// hard-errors on a split smaller than one batch), floored at 1. Split sizing
/// mirrors [`write_mlx_dataset`] exactly — one arithmetic, two readers.
fn effective_batch_size(request: &TrainingJobRequest, schedule: &ScheduleParams) -> u32 {
    let n = request.dataset.examples.len();
    let split = request.dataset.validation_split.clamp(0.0, 0.5);
    let n_valid = (((n as f32) * split).floor() as usize).min(n.saturating_sub(1));
    let n_train = n - n_valid;
    // A zero-row valid file is mirrored from train by the writer, so the
    // effective smallest split is never zero.
    let smallest = n_train.min(n_valid.max(1)).max(1) as u32;
    schedule.batch_size.max(1).min(smallest)
}

/// schedule's epochs × example count ÷ batch — a reasonable mapping
/// from the substrate's epoch-shaped schedule to MLX's iter knob.
fn iters_for(request: &TrainingJobRequest, schedule: &ScheduleParams) -> u32 {
    let n = request.dataset.examples.len().max(1) as u32;
    let batch = schedule.batch_size.max(1);
    (schedule.epochs.max(1) * n / batch).max(1)
}

fn default_schedule() -> ScheduleParams {
    ScheduleParams {
        epochs: 3,
        batch_size: 4,
        sequence_length: 2048,
        learning_rate: 1e-5,
    }
}

fn default_lora() -> LoRAHyperparams {
    LoRAHyperparams {
        rank: 8,
        alpha: 16,
        dropout: 0.0,
        target_modules: vec!["q_proj".into(), "v_proj".into()],
    }
}

/// The `-c` config YAML carrying the LoRA knobs mlx_lm.lora has NO CLI flag for
/// (`rank`/`scale`/`dropout`). This is LOAD-BEARING: without it, mlx_lm silently
/// falls back to its built-in defaults — rank 8 **and scale 20.0** — over-baking
/// every gene ~10× past the substrate-intended scale (`== alpha/rank`, ≈2.0 for the
/// rank-8/alpha-16 default). A scale-20 LoRA serves as gibberish at request-scale
/// 1.0, so the genome A/B measures a real-but-overdriven gene as a loss and rejects
/// an adapter that lifts cleanly at the intended strength. Rendered through the ONE
/// canonical encoder the forge train primitive uses
/// ([[fallbacks-are-illegal-fail-loud]] — fix the trained scale at its source, never
/// dial around it at serve time).
///
/// `keys` is left EMPTY here, leaving mlx_lm on its own default target set. The
/// genome adapter's convert-safe MLP-only targeting (the forge path's hard-won
/// invariant) is the #52 convergence follow-up; this fix isolates the proven scale
/// variable and changes nothing else about what the trainer touches.
fn lora_config_yaml(lora: &LoRAHyperparams) -> String {
    let scale = lora.alpha as f64 / (lora.rank.max(1) as f64);
    crate::forge::mlx_train::render_lora_parameters_yaml(lora.rank, scale, lora.dropout as f64, &[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::fine_tuning::types::{TrainingDataset, TrainingExample, TrainingSource};

    // what this catches: the live-learning-proof parser — mlx_lm's `Iter N: Train/Val
    // loss X` lines become loss.jsonl rows, and everything else (tqdm bars, checkpoint
    // saves, blank lines) stays a plain log line. A parser that mis-reads a bar as a
    // loss point poisons the curve the evidence engine renders.
    #[test]
    fn loss_lines_parse_and_noise_does_not() {
        assert_eq!(
            parse_loss_line("Iter 100: Train loss 1.234, Learning Rate 1e-05, It/sec 0.12"),
            Some((100, "train", 1.234))
        );
        assert_eq!(
            parse_loss_line("Iter 200: Val loss 0.987, Val took 30.1s"),
            Some((200, "val", 0.987))
        );
        for noise in [
            "Calculating loss...:  12%|█▎        | 2/16 [00:50<06:19, 27.11s/it]",
            "Iter 300: Saved adapter weights to adapters/0000300_adapters.safetensors",
            "Loading pretrained model",
            "",
        ] {
            assert_eq!(
                parse_loss_line(noise),
                None,
                "noise parsed as loss: {noise:?}"
            );
        }
    }

    fn req(examples: Vec<(&str, &str)>) -> TrainingJobRequest {
        TrainingJobRequest {
            persona_id: Uuid::nil(),
            persona_name: "asha".into(),
            base_model: "Qwen/Qwen2.5-Coder-3B-Instruct".into(),
            trait_kind: "coder-test".into(),
            dataset: TrainingDataset {
                examples: examples
                    .into_iter()
                    .map(|(p, c)| TrainingExample {
                        prompt: p.into(),
                        completion: c.into(),
                        metadata: None,
                    })
                    .collect(),
                source: TrainingSource::OperatorCurated,
                validation_split: 0.2,
            },
            eval_set: None,
            lora: None,
            schedule: None,
            local_artifact_dir: None,
        }
    }

    // what this catches: the adapter advertises itself honestly as the
    // mlx-local LoRA-capable, local-artifact-producing trainer. The
    // coordinator routes by these; a regression that flips
    // produces_local_artifact would silently break the custodian
    // hand-off (it reads adapters.safetensors off the local path).
    #[test]
    fn capabilities_describe_mlx_local() {
        let caps = MlxLoraFineTuner::new().capabilities();
        assert_eq!(caps.provider_id, "mlx-local");
        assert!(caps.supports_lora);
        assert!(caps.produces_local_artifact);
    }

    // what this catches: the `-c` config the adapter emits carries the
    // substrate-intended LoRA scale (== alpha/rank, ≈2.0 for the rank-8/alpha-16
    // default), NOT mlx_lm's built-in default 20.0. Without this config mlx silently
    // over-bakes every gene ~10×, serving gibberish at request-scale 1.0 and getting
    // a good gene wrongly rejected by the genome A/B. The scale line is the proven
    // root cause of the positive-control "failure"; this pins it at the source.
    #[test]
    fn lora_config_carries_intended_scale_two() {
        let cfg = lora_config_yaml(&default_lora());
        assert!(cfg.contains("rank: 8"), "config: {cfg}");
        assert!(cfg.contains("scale: 2.0"), "config: {cfg}");
        // 20.0 is exactly the mlx default this config exists to override.
        assert!(
            !cfg.contains("scale: 20"),
            "leaked mlx default scale: {cfg}"
        );
    }

    // what this catches: empty base / empty dataset are caller errors
    // (InvalidRequest), not silent no-ops. Per no-fallbacks-ever the
    // adapter refuses rather than spawning a doomed subprocess.
    #[tokio::test]
    async fn rejects_empty_base_and_dataset() {
        let tuner = MlxLoraFineTuner::new();

        let mut bad_base = req(vec![("hi", "there")]);
        bad_base.base_model = "  ".into();
        match tuner.create_job(bad_base).await {
            Err(FineTuningError::InvalidRequest(_)) => {}
            other => panic!("expected InvalidRequest for empty base, got {other:?}"),
        }

        let empty = req(vec![]);
        match tuner.create_job(empty).await {
            Err(FineTuningError::InvalidRequest(_)) => {}
            other => panic!("expected InvalidRequest for empty dataset, got {other:?}"),
        }

        // A rejected request must not leave a phantom job slot behind —
        // poll/cancel would otherwise see a job that never spawned.
        assert_eq!(tuner.tracked_job_count(), 0);
    }

    // what this catches: the dataset materializer writes MLX's
    // train.jsonl + valid.jsonl with the {"prompt","completion"} schema,
    // honors the split, and never leaves train empty (mlx_lm chokes on
    // an empty train file). Pure I/O — no subprocess, runs everywhere.
    #[test]
    fn writes_mlx_dataset_split() {
        let dir = std::env::temp_dir().join(format!("mlx_ds_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let r = req(vec![
            ("p1", "c1"),
            ("p2", "c2"),
            ("p3", "c3"),
            ("p4", "c4"),
            ("p5", "c5"),
        ]);
        write_mlx_dataset(&dir, &r, true).unwrap();

        let train = std::fs::read_to_string(dir.join("train.jsonl")).unwrap();
        let valid = std::fs::read_to_string(dir.join("valid.jsonl")).unwrap();
        let train_lines: Vec<_> = train.lines().filter(|l| !l.is_empty()).collect();
        let valid_lines: Vec<_> = valid.lines().filter(|l| !l.is_empty()).collect();

        // 5 examples × 0.2 split → 1 valid, 4 train.
        assert_eq!(valid_lines.len(), 1);
        assert_eq!(train_lines.len(), 4);
        // Schema is prompt/completion, not the bare {"text"} form.
        let first: serde_json::Value = serde_json::from_str(train_lines[0]).unwrap();
        assert!(first.get("prompt").is_some());
        assert!(first.get("completion").is_some());

        std::fs::remove_dir_all(&dir).ok();
    }

    // what this catches: a template-less base (Devstral/Tekken) gets the raw
    // {"text"} schema — the only one its tokenizer can train on. Regression for
    // the 2026-07-10 recall-trust train that died mid-job on
    // "Cannot use chat template functions".
    #[test]
    fn template_less_base_writes_text_schema() {
        let dir = std::env::temp_dir().join(format!("mlx_ds_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let r = req(vec![("the prompt", "the completion"), ("p2", "c2")]);
        write_mlx_dataset(&dir, &r, false).unwrap();
        let train = std::fs::read_to_string(dir.join("train.jsonl")).unwrap();
        let first: serde_json::Value = serde_json::from_str(train.lines().next().unwrap()).unwrap();
        assert!(
            first.get("prompt").is_none(),
            "no chat schema without a template"
        );
        let text = first["text"].as_str().unwrap();
        assert!(text.contains("the prompt") && text.contains("the completion"));
        std::fs::remove_dir_all(&dir).ok();
    }

    // what this catches: epochs→iters mapping never returns 0 (mlx_lm
    // --iters 0 is a no-op train). Floors at 1 even for tiny corpora.
    #[test]
    fn iters_never_zero() {
        let r = req(vec![("p", "c")]);
        let s = default_schedule();
        assert!(iters_for(&r, &s) >= 1);
    }

    // what this catches: end-to-end real training on this Mac. Proves
    // mlx_lm.lora spawns, trains, and writes adapters.safetensors that
    // the custodian can convert. Gated #[ignore] — needs the base model
    // downloaded + minutes of GPU. Run:
    //   cargo test -p continuum-core --features metal,accelerate,test-fixtures \
    //     --lib genome::fine_tuning::mlx_lora_adapter::tests::real_mlx -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "real mlx_lm.lora training on Apple Silicon; run explicitly"]
    async fn real_mlx_train_produces_safetensors() {
        if !host_has_metal() {
            eprintln!("skip: host has no Metal device");
            return;
        }
        let tuner = MlxLoraFineTuner::new();
        if !tuner.python.exists() {
            eprintln!("skip: {} not present", tuner.python.display());
            return;
        }
        let mut r = req(vec![
            (
                "Write a Rust function that adds two i32.",
                "fn add(a: i32, b: i32) -> i32 { a + b }",
            ),
            (
                "Reverse a string in Rust.",
                "fn rev(s: &str) -> String { s.chars().rev().collect() }",
            ),
        ]);
        r.schedule = Some(ScheduleParams {
            epochs: 1,
            batch_size: 1,
            sequence_length: 512,
            learning_rate: 1e-4,
        });

        let handle = tuner.create_job(r).await.expect("spawn mlx job");
        // Poll until terminal (bounded — real training is minutes).
        for _ in 0..600 {
            match tuner.poll(&handle).await.unwrap() {
                TrainingStatus::Completed { artifact } => {
                    let st = artifact.local_path.unwrap().join("adapters.safetensors");
                    assert!(st.exists(), "adapters.safetensors must exist");
                    eprintln!("OK: {}", st.display());
                    return;
                }
                TrainingStatus::Failed { error } => panic!("mlx training failed: {error}"),
                _ => tokio::time::sleep(std::time::Duration::from_secs(2)).await,
            }
        }
        panic!("mlx training did not finish in the polling window");
    }
}
