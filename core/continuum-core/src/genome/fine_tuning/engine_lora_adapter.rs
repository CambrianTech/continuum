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
//!   that shape leases its number. Never a guess: [`crate::forge::training_admission`] refuses unmeasured bytes.
//! - the artifact leaves the lanes' `--train-dir` (`engine-train`, swept of every file whose job
//!   is not live) for the job's own directory BEFORE the job goes terminal (Fable's invariant on
//!   #4438), and is a [`ArtifactFormat::GgufLora`].
use super::native_jobs::{
    default_lora, default_schedule, job_dir_for, Execution, InPlaceEnd, InPlaceRun, NativeJobs, PreparedJob,
    RunProgress,
};
use super::{
    ArtifactFormat, FineTuningAdapter, FineTuningCapabilities, FineTuningError, JobHandle, JobMetrics,
    TrainerHardware, TrainingArtifact, TrainingExample, TrainingJobRequest, TrainingStatus,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use uuid::Uuid;

pub const PROVIDER_ID: &str = "engine-local";

/// How often the run is polled. The engine answers GET /train from memory; this is the
/// cadence of progress and of the VRAM peak sample, not a timeout.
// derived-or-floor: a floor — far below an epoch, far above a spin.
const POLL: Duration = Duration::from_secs(2);

/// How long the lane may stay unreachable before the run is taken to have ended with it. The
/// run lives INSIDE the engine process: a lane that answers nothing for this long has gone
/// (crashed, relaunched), and its training with it. Shorter silences are retried, never read as
/// an end: releasing the job's lease while the engine still trains would let the governor hand
/// that memory to someone else under a live run (Cormac on #4443).
// derived-or-floor: a floor — well past a relaunch's socket gap, well under an epoch.
const LANE_SILENCE: Duration = Duration::from_secs(90);

fn failure(error: impl std::fmt::Display) -> FineTuningError {
    FineTuningError::LocalTrainerFailed(error.to_string())
}

/// HF module names (what [`super::LoRAHyperparams::target_modules`] carries, and what the
/// PyTorch/MLX trainers take) to the GGUF tensor names the engine's adapter writer targets.
/// An unknown name is refused: silently dropping a target trains a different adapter.
fn gguf_targets(modules: &[String]) -> Result<String, String> {
    let mut out: Vec<&str> = Vec::new();
    for m in modules {
        let t = match m.as_str() {
            "q_proj" | "attn_q" => "attn_q",
            "k_proj" | "attn_k" => "attn_k",
            "v_proj" | "attn_v" => "attn_v",
            "o_proj" | "attn_output" => "attn_output",
            "gate_proj" | "ffn_gate" => "ffn_gate",
            "up_proj" | "ffn_up" => "ffn_up",
            "down_proj" | "ffn_down" => "ffn_down",
            other => return Err(format!("LoRA target module {other:?} has no engine (GGUF) equivalent")),
        };
        if !out.contains(&t) {
            out.push(t);
        }
    }
    if out.is_empty() {
        return Err("no LoRA target modules".into());
    }
    Ok(out.join(","))
}

/// The shape a footprint is measured for: the training context's memory is a function of the
/// model, the window, and the adapter's geometry, and of nothing the request can vary besides.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct Shape {
    model: String,
    window: u32,
    rank: u32,
    targets: String,
    /// The blocks adapted when fewer than all (`top_layers`, fork #27); `None` = every block.
    /// Depth drives the graph (~linear), so a reduced-depth run is its own shape.
    #[serde(default)]
    depth: Option<u32>,
}

impl Shape {
    /// Full depth keeps the key every row before depth existed was written under (those
    /// rows were all full-depth runs); a reduced depth adds `|dK`, so a reduced-depth lookup
    /// can never land on a full-depth row, nor a full-depth lookup on a reduced one.
    fn key(&self) -> String {
        let base = format!("{}|w{}|r{}|{}", self.model, self.window, self.rank, self.targets);
        match self.depth {
            Some(k) => format!("{base}|d{k}"),
            None => base,
        }
    }
}

/// The window a run trains at: the engine's training context is n_ctx, which rounds up to a
/// multiple of 256, and a window that is not one reached opt_init's assert and took the
/// serving process down (fork #27 now refuses it). Rounded DOWN, so the lease never grows past
/// what was asked; at least one 256-token context.
fn train_window(sequence_length: u32) -> u32 {
    ((sequence_length / 256).max(1) * 256).min(8192)
}

/// The depth a finished run actually adapted, as the shape it is recorded under: the engine's
/// own `layers_adapted` against its `n_layer`. An engine that reports neither predates
/// `top_layers` and adapted every block, whatever was asked.
fn effective_depth(layers_adapted: Option<u32>, n_layer: Option<u32>) -> Option<u32> {
    match (layers_adapted, n_layer) {
        (Some(k), Some(n)) if k < n => Some(k),
        _ => None,
    }
}

/// Measured engine-training footprints, one row per shape, in one small JSON file (bounded by
/// the number of distinct shapes ever run, not by runs).
struct Footprints {
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FootprintRow {
    bytes: u64,
    measured_at_ms: i64,
    job: String,
}

impl Footprints {
    fn read_all(&self) -> BTreeMap<String, FootprintRow> {
        match std::fs::read_to_string(&self.path) {
            Ok(body) => serde_json::from_str(&body).unwrap_or_default(), // unwrap_or_default: a corrupt file re-measures every shape (a calibration run), never blocks training
            Err(_) => BTreeMap::new(),
        }
    }
    fn get(&self, shape: &Shape) -> Option<u64> {
        self.read_all().get(&shape.key()).map(|r| r.bytes)
    }
    /// Keeps the LARGEST footprint observed for a shape: the peak is sampled on the governor's
    /// scan cadence, which can miss the true peak but never invents one (Cormac on #4443).
    fn record(&self, shape: &Shape, bytes: u64, job: Uuid) -> std::io::Result<()> {
        let mut all = self.read_all();
        if all.get(&shape.key()).is_some_and(|r| r.bytes >= bytes) {
            return Ok(());
        }
        all.insert(
            shape.key(),
            FootprintRow {
                bytes,
                measured_at_ms: chrono::Utc::now().timestamp_millis(),
                job: job.to_string(),
            },
        );
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&all).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, &self.path)
    }
}

/// `POST /train`'s body: the engine's wire contract (fork `tools/server/server-train.h`), stated
/// ONCE here instead of assembled field by field at the call site.
#[derive(Debug, Clone, Serialize)]
struct TrainRequest {
    examples: Vec<EngineExample>,
    /// a bare `<job>.gguf` name; the engine writes it inside the lane's `--train-dir`
    out: String,
    rank: u32,
    alpha: u32,
    /// GGUF module names, comma-separated
    targets: String,
    window: u32,
    epochs: u32,
    lr: f64,
    val_split: f32,
    seed: u32,
    /// adapt only the last K blocks (fork #27); omitted = every block
    #[serde(skip_serializing_if = "Option::is_none")]
    top_layers: Option<u32>,
    /// what training may add on the GPU: the job's governed lease, which the engine enforces
    /// before allocating (the driver's own free figure is not physical on Windows)
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_budget_mib: Option<u64>,
}

/// One `/train` example: a prompt/completion pair, or a served conversation (OpenAI message
/// shape exactly as serving's `wire_messages` framed it, with the tools she was offered).
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum EngineExample {
    Pair {
        prompt: String,
        completion: String,
    },
    Conversation {
        messages: Vec<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tools: Option<Value>,
    },
}

/// `GET /train`'s run state. A state this core does not know is `Unknown`, never guessed into
/// a known one (it counts as possibly running: see `EngineRun::running_ours`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TrainState {
    Idle,
    Starting,
    Running,
    Done,
    Cancelled,
    Error,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
struct EpochReport {
    train_loss: f64,
    eval_loss: f64,
}

/// `GET /train`'s body. `state` is required: a body without one does not parse, and an
/// unparsable status is retried like a lost one, never read as "not ours".
#[derive(Debug, Clone, Deserialize)]
struct TrainStatus {
    state: TrainState,
    #[serde(default)]
    out: Option<String>,
    #[serde(default)]
    batch: Option<u64>,
    #[serde(default)]
    batch_max: Option<u64>,
    #[serde(default)]
    epochs: Vec<EpochReport>,
    #[serde(default)]
    trainable_tokens: Option<u64>,
    /// the training graph the engine measured before allocating it: this shape's footprint
    #[serde(default)]
    graph_mib: Option<f64>,
    /// the model's block count and the blocks this run adapts (fork #27); absent on an engine
    /// that predates `top_layers`, which adapts every block
    #[serde(default)]
    n_layer: Option<u32>,
    #[serde(default)]
    layers_adapted: Option<u32>,
    #[serde(default)]
    error: Option<String>,
}

/// Where the lane serving `base` answers, if one does on this node.
type LaneResolver = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn live_lane_for(base: &str) -> Option<String> {
    let rec = crate::inference::lane_registry::live_lane()?;
    (rec.model == base).then(|| format!("http://127.0.0.1:{}", rec.port))
}

/// Admission: the governed lease a run holds for its life. `Governed` in production;
/// tests drive the run without a governor.
enum Admission {
    Governed,
    #[cfg(test)]
    Ungoverned,
}

pub struct EngineLoraFineTuner {
    jobs: NativeJobs,
    http: reqwest::Client,
    lane: LaneResolver,
    /// The lanes' `--train-dir` (the engine writes `<job>.gguf` here).
    train_dir: Option<PathBuf>,
    footprints: Footprints,
    admission: Admission,
}

impl Default for EngineLoraFineTuner {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineLoraFineTuner {
    pub fn new() -> Self {
        let footprints = dirs::home_dir()
            .unwrap_or_default() // Without a home the relative path re-measures (a calibration run) — never a guessed footprint.
            .join(".continuum/genome/engine-footprints.json");
        Self {
            jobs: NativeJobs::new(PROVIDER_ID),
            http: reqwest::Client::new(),
            lane: Box::new(live_lane_for),
            train_dir: crate::inference::llama_server::engine_train_dir(),
            footprints: Footprints { path: footprints },
            admission: Admission::Governed,
        }
    }

    #[cfg(test)]
    fn for_test(lane_url: String, train_dir: PathBuf, footprints: PathBuf) -> Self {
        Self {
            jobs: NativeJobs::new(PROVIDER_ID),
            http: reqwest::Client::new(),
            lane: Box::new(move |_| Some(lane_url.clone())),
            train_dir: Some(train_dir),
            footprints: Footprints { path: footprints },
            admission: Admission::Ungoverned,
        }
    }
}

/// The in-place run: POST /train on the lane, poll to its end, stop it on a cancel.
struct EngineRun {
    http: reqwest::Client,
    lane: String,
    body: TrainRequest,
    /// where the engine writes this job's adapter (removed if a cancel races a finish)
    adapter_path: PathBuf,
    epochs: u32,
    /// The run's last status as the engine reported it (finish reads its losses and footprint).
    last: Arc<Mutex<Option<TrainStatus>>>,
    /// The governed lease, held for exactly as long as the engine may be running this job's
    /// training: `run` returns only once it has ended there, and the lease drops with `self`.
    _lease: Option<crate::resources::LeaseGuard>,
}

impl EngineRun {
    async fn status_once(&self) -> Result<TrainStatus, String> {
        let r = self
            .http
            .get(format!("{}/train", self.lane))
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("GET /train on {}: {e}", self.lane))?;
        r.json::<TrainStatus>().await.map_err(|e| format!("GET /train on {}: {e}", self.lane))
    }

    /// The engine's training status, retried through a silence shorter than [`LANE_SILENCE`].
    /// `Err` means the lane has answered nothing for that long: it is gone, and the run with it.
    async fn status(&self) -> Result<TrainStatus, String> {
        let since = tokio::time::Instant::now();
        loop {
            match self.status_once().await {
                Ok(s) => return Ok(s),
                Err(e) if since.elapsed() >= LANE_SILENCE => {
                    return Err(format!("{e} (no answer for {}s: the lane and its run are gone)", LANE_SILENCE.as_secs()))
                }
                Err(_) => tokio::time::sleep(POLL).await,
            }
        }
    }

    /// True while the engine MAY be running this job's training (it runs one at a time): the
    /// status is ours and not a known end. An unknown state counts as running: releasing the
    /// lease on a state nobody here understands is exactly the unsafe case.
    fn running_ours(&self, s: &TrainStatus) -> bool {
        self.ours(s) && !matches!(s.state, TrainState::Done | TrainState::Cancelled | TrainState::Error)
    }

    /// The one way out of a run that did not finish: make sure the engine is not still training
    /// this job before the lease is let go. Cancels (retrying the POST) and waits until the
    /// engine's status is no longer this job's live run, or the lane is gone.
    async fn stop_ours(&self) {
        loop {
            match self.status().await {
                Ok(s) if self.running_ours(&s) => {
                    let _ = self
                        .http
                        .post(format!("{}/train/cancel", self.lane))
                        .timeout(Duration::from_secs(10))
                        .send()
                        .await; // a refused or lost cancel is retried by the next pass; the status decides
                    tokio::time::sleep(POLL).await;
                }
                _ => return, // not ours, not running, or the lane is gone: nothing of ours trains
            }
        }
    }

    async fn failed(&self, why: String) -> InPlaceEnd {
        self.stop_ours().await;
        InPlaceEnd::Failed(why)
    }

    /// The engine runs one training at a time; a status for a different `out` is not ours.
    fn ours(&self, s: &TrainStatus) -> bool {
        s.out.as_deref() == Some(self.body.out.as_str())
    }
}

/// Percent done and the current epoch from an engine status: completed epochs plus the current
/// epoch's batch fraction.
fn progress_of(s: &TrainStatus, epochs: u32) -> (f32, u32) {
    let done = s.epochs.len() as f32;
    let batch = s.batch.unwrap_or(0) as f32; // unwrap_or: absent before the first batch — zero of this epoch is the truth
    let max = s.batch_max.unwrap_or(0) as f32; // unwrap_or: absent before the first batch
    let frac = if max > 0.0 { (batch / max).min(1.0) } else { 0.0 };
    let pct = if epochs == 0 { 0.0 } else { ((done + frac) / epochs as f32 * 100.0).min(100.0) };
    (pct, done as u32)
}

#[async_trait]
impl InPlaceRun for EngineRun {
    fn place(&self) -> String {
        self.lane.clone()
    }

    async fn run(self: Box<Self>, mut cancel: watch::Receiver<bool>, progress: RunProgress) -> InPlaceEnd {
        match self
            .http
            .post(format!("{}/train", self.lane))
            .json(&self.body)
            .timeout(Duration::from_secs(120))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => {}
            Ok(r) => {
                // refused before any thread started: nothing of ours runs
                let why = r.text().await.unwrap_or_default(); // unwrap_or_default: an unreadable refusal body still fails the job
                return InPlaceEnd::Failed(format!("the engine refused the training run: {why}"));
            }
            // lost on the way back: the run may have started, so it is stopped before failing
            Err(e) => return self.failed(format!("POST /train on {}: {e}", self.lane)).await,
        }
        let mut tick = tokio::time::interval(POLL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut cancelling = false;
        loop {
            tokio::select! {
                changed = cancel.changed(), if !cancelling => {
                    if changed.is_err() || *cancel.borrow() {
                        cancelling = true;
                    }
                }
                _ = tick.tick() => {}
            }
            if cancelling {
                // stops at the next training window; Cancelled only once the engine says so. A run
                // that reached done in the meantime wrote its adapter: a cancelled job keeps none.
                self.stop_ours().await;
                let _ = std::fs::remove_file(&self.adapter_path); // absent unless the run finished first
                return InPlaceEnd::Cancelled;
            }
            let s = match self.status().await {
                Ok(s) => s,
                Err(e) => return InPlaceEnd::Failed(e), // the lane is gone, and its run with it
            };
            if !self.ours(&s) {
                // one run at a time: another run in the engine means ours has ended
                return InPlaceEnd::Failed(format!("the engine's training run is no longer this job's (out {:?})", s.out));
            }
            if let Ok(mut last) = self.last.lock() {
                *last = Some(s.clone());
            }
            match s.state {
                TrainState::Starting | TrainState::Running => {
                    let (pct, epoch) = progress_of(&s, self.epochs);
                    progress.running(pct, epoch);
                }
                TrainState::Done => return InPlaceEnd::Finished,
                TrainState::Cancelled => return InPlaceEnd::Failed("the engine's run was cancelled by someone else".into()),
                TrainState::Error => {
                    let why = s.error.as_deref().unwrap_or("no error text"); // unwrap_or: the state alone is the failure
                    return InPlaceEnd::Failed(format!("the engine's training run failed: {why}"));
                }
                // idle while ours is named, or a state this core does not know: not a known end
                TrainState::Idle | TrainState::Unknown => {
                    return self.failed(format!("the engine reports training state {:?} for this job", s.state)).await
                }
            }
        }
    }
}

/// The adapter leaves the lanes' train dir for the job's own directory. Runs in `finish`, i.e.
/// BEFORE the job goes terminal: the engine-train sweep deletes every file whose job is not live,
/// so a finished gene left there would be deleted.
fn move_adapter(train_dir: &Path, out: &str, job_dir: &Path) -> Result<PathBuf, String> {
    let from = train_dir.join(out);
    let size = std::fs::metadata(&from).map_err(|e| format!("the engine wrote no adapter at {}: {e}", from.display()))?.len();
    if size == 0 {
        return Err(format!("the engine's adapter at {} is empty", from.display()));
    }
    let adapters = job_dir.join("adapters");
    std::fs::create_dir_all(&adapters).map_err(|e| format!("{}: {e}", adapters.display()))?;
    let to = adapters.join("adapter.gguf");
    std::fs::rename(&from, &to).map_err(|e| format!("moving {} -> {}: {e}", from.display(), to.display()))?;
    Ok(to)
}

#[async_trait]
impl FineTuningAdapter for EngineLoraFineTuner {
    fn capabilities(&self) -> FineTuningCapabilities {
        FineTuningCapabilities {
            provider_id: PROVIDER_ID.into(),
            supports_lora: true,
            supports_validation: true,
            produces_local_artifact: true,
            // any base a live lane on this node serves; create_job checks the lane
            supported_base_model_prefixes: vec![],
            requires: TrainerHardware::Any,
        }
    }

    async fn create_job(&self, mut request: TrainingJobRequest) -> Result<JobHandle, FineTuningError> {
        let schedule = request.schedule.get_or_insert_with(default_schedule).clone();
        let lora = request.lora.get_or_insert_with(default_lora).clone();
        if request.dataset.examples.is_empty()
            || schedule.epochs == 0
            || schedule.epochs > 100
            || !(16..=8192).contains(&schedule.sequence_length)
            || !schedule.learning_rate.is_finite()
            || schedule.learning_rate <= 0.0
            || schedule.learning_rate > 1.0
            || !(1..=256).contains(&lora.rank)
            || lora.alpha == 0
        {
            return Err(FineTuningError::InvalidRequest(
                "invalid engine training data/schedule/LoRA geometry (epochs 1-100, sequence_length 16-8192, lr (0,1], rank 1-256)".into(),
            ));
        }
        let targets = gguf_targets(&lora.target_modules).map_err(FineTuningError::InvalidRequest)?;
        let lane = (self.lane)(&request.base_model).ok_or_else(|| {
            FineTuningError::InvalidRequest(format!(
                "no live lane serves {} on this node: in-engine training runs on the resident weights",
                request.base_model
            ))
        })?;
        let train_dir = self
            .train_dir
            .clone()
            .ok_or_else(|| failure("no engine train dir (no home directory): /train is off on every lane"))?;
        let id = Uuid::new_v4();
        let out = format!("{id}.gguf");
        let window = train_window(schedule.sequence_length);
        if window != schedule.sequence_length {
            crate::probe!(
                class = "training.job.window_rounded",
                asked = schedule.sequence_length as u64,
                sent = window as u64,
                "the training window rounded down to a multiple of 256 (the engine's context granularity)"
            );
        }
        // 0 blocks is no depth at all: every block, as omitted (the engine refuses 0 at parse)
        let depth = lora.top_layers.filter(|&k| k > 0);
        let shape = Shape {
            model: request.base_model.clone(),
            window,
            rank: lora.rank,
            targets: targets.clone(),
            depth,
        };
        let val = request.dataset.validation_split.clamp(0.0, 0.5);
        let mut body = TrainRequest {
            examples: request.dataset.examples.iter().map(engine_example).collect(),
            out: out.clone(),
            rank: lora.rank,
            alpha: lora.alpha,
            targets: targets.clone(),
            window,
            epochs: schedule.epochs,
            lr: schedule.learning_rate,
            val_split: val,
            seed: 42,
            top_layers: depth,
            memory_budget_mib: None,
        };
        let measured = self.footprints.get(&shape);
        let footprints_path = self.footprints.path.clone();
        let http = self.http.clone();
        let governed = matches!(self.admission, Admission::Governed);
        let job_dir = job_dir_for(&request, id);
        let model_id = format!("{PROVIDER_ID}:{}:{id}", request.trait_kind);
        let epochs = schedule.epochs;
        Ok(self.jobs.prepare(id, move |progress| async move {
            let consumer = format!("genome-train:{id}");
            let reservation = if governed {
                let daemon = crate::resources::ResourceDaemon::global()
                    .ok_or_else(|| failure("engine training requires the resource governor"))?;
                // A measured shape leases its number; an unmeasured one is a calibration run
                // that leases everything governed and free, so nothing grows into it.
                let bytes = match measured {
                    Some(bytes) => bytes,
                    None => daemon.available_for(&consumer, crate::resources::ResourceKind::Vram),
                };
                if bytes == 0 {
                    return Err(FineTuningError::Transient(
                        "no governed VRAM is free to calibrate this engine training shape on".into(),
                    ));
                }
                crate::probe!(
                    class = "training.job.planned",
                    job = %id,
                    base = shape.model.as_str(),
                    window = shape.window as u64,
                    depth = shape.depth.map_or(0, u64::from), // probe field: 0 = every block
                    measured = measured.is_some(),
                    memory_bytes = bytes,
                    "engine training: the measured footprint for this shape, or (unmeasured) all governed \
                     free VRAM for a calibration run"
                );
                let gate = crate::modules::serving_daemon::LifecycleGate::global()
                    .ok_or_else(|| failure("engine training requires the serving lifecycle gate"))?;
                let reservation = crate::forge::training_admission::wait_for_training_memory(
                    daemon.clone(),
                    &gate,
                    &consumer,
                    bytes,
                    |available| progress.waiting_for_capacity(bytes, available),
                )
                .await
                .map_err(FineTuningError::Transient)?;
                // the engine refuses a graph over the lease before allocating it
                body.memory_budget_mib = Some(bytes / (1024 * 1024));
                Some(reservation)
            } else {
                None
            };
            let last = Arc::new(Mutex::new(None));
            let run = EngineRun {
                http,
                lane,
                body,
                adapter_path: train_dir.join(&out),
                epochs,
                last: last.clone(),
                _lease: reservation,
            };
            Ok(PreparedJob {
                execution: Execution::InPlace(Box::new(run)),
                finish: Box::new(move |wall_clock_ms| {
                    let adapter = move_adapter(&train_dir, &out, &job_dir)?;
                    let status = last
                        .lock()
                        .ok()
                        .and_then(|l| l.clone())
                        .ok_or("the engine's final status was never read")?;
                    let final_loss = status.epochs.last().map(|e| e.train_loss);
                    let final_validation_loss = status.epochs.last().map(|e| e.eval_loss);
                    let trainable = status
                        .trainable_tokens
                        .ok_or("the engine reported no trainable-token count")?;
                    if trainable == 0 || final_loss.is_none_or(|l| !l.is_finite()) {
                        return Err("the engine produced no finite measured learning receipt".into());
                    }
                    // the footprint this shape needs: the training graph the engine measured before
                    // allocating it (exact, where a VRAM sample could miss the peak)
                    let grown = status.graph_mib.map_or(0, |m| (m * 1024.0 * 1024.0) as u64);
                    // recorded under the depth the engine ACTUALLY adapted: an engine that
                    // ignored top_layers measured a full-depth graph, and that number must never
                    // be leased for a reduced-depth run (Codex on #27)
                    let adapted = effective_depth(status.layers_adapted, status.n_layer);
                    if adapted != shape.depth {
                        crate::probe!(
                            class = "training.job.depth_differs",
                            job = %id,
                            asked = shape.depth.map_or(0, u64::from), // probe field: 0 = every block
                            adapted = adapted.map_or(0, u64::from), // probe field: 0 = every block
                            "the engine adapted a different depth than was asked (an engine without top_layers adapts every block)"
                        );
                    }
                    let measured_shape = Shape { depth: adapted, ..shape.clone() };
                    // A depth equal to the model's block count IS full depth (fork #27 refuses one
                    // past it): filed under the asked key too, or that request would calibrate on
                    // every run (Cormac on #4472).
                    let asked_full = matches!((shape.depth, status.n_layer), (Some(k), Some(n)) if k >= n);
                    if grown > 0 {
                        let store = Footprints { path: footprints_path };
                        if asked_full {
                            let _ = store.record(&shape, grown, id); // best effort: the full-depth row below is the one that matters
                        }
                        if let Err(e) = store.record(&measured_shape, grown, id) {
                            crate::probe!(
                                class = "training.job.footprint_unrecorded",
                                job = %id,
                                error = %e,
                                "the measured engine-training footprint could not be recorded; the next run of \
                                 this shape calibrates again"
                            );
                        }
                    }
                    Ok(TrainingArtifact {
                        model_id,
                        local_path: Some(adapter),
                        format: ArtifactFormat::GgufLora,
                        metrics: JobMetrics {
                            trained_tokens: trainable * status.epochs.len() as u64,
                            final_loss,
                            final_validation_loss,
                            wall_clock_ms,
                            layers_adapted: status.layers_adapted.or(status.n_layer),
                            ..Default::default()
                        },
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

/// One example as `/train` reads it. A lived call goes as its served conversation: the
/// system prompt and every served message framed exactly as serving framed them
/// ([`crate::inference::request_body::wire_messages`]: her earlier tool calls kept, tool
/// results as `role: tool`), then her reply with its reasoning and tool calls. Earlier
/// assistant turns in that history are context (`"train": false`), not this lesson. The tools she was offered ride along so the rendered prompt has the tool
/// block she saw. Anything else goes as `{prompt, completion}`.
fn engine_example(e: &TrainingExample) -> EngineExample {
    let Some(call) = e.lived.as_ref() else {
        return EngineExample::Pair {
            prompt: e.prompt.clone(),
            completion: e.completion.clone(),
        };
    };
    use crate::inference::request_body::{close_trailing_assistant, wire_messages, wire_tool_call};
    // The history exactly as serving framed it; images drop (the trainer is text-only,
    // and a text model was served the description bridge anyway).
    let mut messages = wire_messages(
        &call.request.messages,
        call.request.system_prompt.as_deref(),
        false,
        PROVIDER_ID,
    );
    // serving closes a history that ends in her own turn before she replies (a self-tick's
    // continuation); without it the trained render has two assistant turns in a row
    close_trailing_assistant(&mut messages);
    for m in messages.iter_mut().filter(|m| m["role"] == "assistant") {
        m["train"] = json!(false);
    }
    let r = &call.response;
    let calls: Vec<Value> = r
        .tool_calls
        .iter()
        .flatten()
        .map(|c| wire_tool_call(&c.id, &c.name, &c.input))
        .collect();
    let mut reply = json!({"role": "assistant", "content": r.text});
    if let Some(reasoning) = r.reasoning.as_deref().filter(|s| !s.is_empty()) {
        reply["reasoning_content"] = json!(reasoning);
    }
    if !calls.is_empty() {
        reply["tool_calls"] = json!(calls);
    }
    messages.push(reply);
    EngineExample::Conversation {
        messages,
        tools: call
            .request
            .tools
            .as_deref()
            .filter(|t| !t.is_empty())
            .map(|tools| json!(crate::inference::request_body::openai_tools(tools))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::fine_tuning::{LoRAHyperparams, ScheduleParams, TrainingDataset, TrainingSource};

    fn request(base: &str) -> TrainingJobRequest {
        TrainingJobRequest {
            persona_id: Uuid::nil(),
            persona_name: "Kimi".into(),
            base_model: base.into(),
            trait_kind: "code".into(),
            dataset: TrainingDataset {
                examples: vec![TrainingExample { prompt: "p".into(), completion: "c".into(), metadata: None, lived: None }],
                source: TrainingSource::OperatorCurated,
                validation_split: 0.0,
            },
            eval_set: None,
            lora: Some(LoRAHyperparams { rank: 8, alpha: 16, dropout: 0.0, target_modules: vec!["q_proj".into(), "v_proj".into()], top_layers: None }),
            schedule: Some(ScheduleParams { epochs: 2, batch_size: 1, sequence_length: 256, learning_rate: 1e-5 }),
            local_artifact_dir: None,
            resume_from: None,
        }
    }

    /// A lane that answers /train like the engine: a run goes starting -> running (batches) ->
    /// done (writing `<out>` into `dir`), or -> cancelled after POST /train/cancel.
    async fn fake_lane(dir: PathBuf, mode: &'static str) -> (String, tokio::task::JoinHandle<()>, Arc<Mutex<Option<Value>>>) {
        use axum::routing::{get, post};
        #[derive(Default)]
        struct Lane {
            out: Option<String>,
            polls: u32,
            cancelled: bool,
            body: Option<Value>,
        }
        let lane = Arc::new(Mutex::new(Lane::default()));
        let seen = Arc::new(Mutex::new(None));
        let (l1, l2, l3, seen1) = (lane.clone(), lane.clone(), lane.clone(), seen.clone());
        let app = axum::Router::new()
            .route("/train", post(move |axum::Json(b): axum::Json<Value>| {
                let lane = l1.clone();
                let seen = seen1.clone();
                async move {
                    let mut l = lane.lock().unwrap();
                    l.out = b.get("out").and_then(Value::as_str).map(str::to_owned);
                    *seen.lock().unwrap() = Some(b.clone());
                    l.body = Some(b);
                    axum::Json(json!({"ok": true}))
                }
            }))
            .route("/train", get(move || {
                let lane = l2.clone();
                let dir = dir.clone();
                async move {
                    let mut l = lane.lock().unwrap();
                    l.polls += 1;
                    let out = l.out.clone().unwrap_or_default();
                    if l.cancelled {
                        return axum::Json(json!({"state": "cancelled", "out": out}));
                    }
                    if mode == "flaky" && l.polls <= 2 {
                        return axum::Json(json!("not a status object"));
                    }
                    if mode == "unknown" {
                        return axum::Json(json!({"state": "paused", "out": out}));
                    }
                    if mode == "cancel_only" || l.polls < 3 {
                        return axum::Json(json!({"state": "running", "out": out, "batch": 1, "batch_max": 4, "epochs": []}));
                    }
                    std::fs::write(dir.join(&out), b"GGUF-lora").unwrap();
                    let mut done = json!({"state": "done", "out": out, "trainable_tokens": 40, "adapter": out, "graph_mib": 5.0,
                        "epochs": [{"epoch": 0, "train_loss": 2.5, "eval_loss": 2.6}, {"epoch": 1, "train_loss": 2.1, "eval_loss": 2.4}]});
                    // an engine with fork #27 reports the depth it adapted; one without says nothing
                    if mode == "depth" {
                        let asked = l.body.as_ref().and_then(|b| b.get("top_layers")).and_then(Value::as_u64).unwrap_or(64);
                        done["n_layer"] = json!(64);
                        done["layers_adapted"] = json!(asked.min(64));
                    }
                    axum::Json(done)
                }
            }))
            .route("/train/cancel", post(move || {
                let lane = l3.clone();
                async move {
                    lane.lock().unwrap().cancelled = true;
                    axum::Json(json!({"ok": true}))
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
        let url = format!("http://{}", listener.local_addr().expect("test: addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test: serve");
        });
        (url, server, seen)
    }

    async fn wait_terminal(t: &EngineLoraFineTuner, h: &JobHandle) -> TrainingStatus {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let s = t.poll(h).await.expect("test: poll");
                if matches!(s, TrainingStatus::Completed { .. } | TrainingStatus::Failed { .. } | TrainingStatus::Cancelled) {
                    return s;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("test: the job never ended")
    }

    // what this catches: a lived call reaching /train as anything but the conversation she
    // was served (card ad107e18): her reasoning and tool calls must stay on the trained reply,
    // her earlier replies in the history must be context and not trained again, and the tools
    // she was offered must ride along so the prompt renders with its tool block.
    #[test]
    fn a_lived_call_is_sent_as_its_served_conversation_with_only_her_reply_trained() {
        use crate::ai::types::{ChatMessage, ContentPart, MessageContent, NativeToolSpec, TextGenerationRequest, TextGenerationResponse};
        let request = TextGenerationRequest {
            system_prompt: Some("you are Kimi".into()),
            messages: vec![
                ChatMessage::text("user", "the build is red"),
                ChatMessage {
                    role: "assistant".into(),
                    content: MessageContent::Parts(vec![ContentPart::ToolUse {
                        id: "t0".into(),
                        name: "code/run".into(),
                        input: json!({"cmd": "cargo test"}),
                    }]),
                    name: None,
                },
                ChatMessage {
                    role: "user".into(),
                    content: MessageContent::Parts(vec![ContentPart::ToolResult {
                        tool_use_id: "t0".into(),
                        content: "1 failed".into(),
                        is_error: None,
                    }]),
                    name: None,
                },
                ChatMessage::text("user", "card: fix it"),
            ],
            tools: Some(vec![serde_json::from_value::<NativeToolSpec>(json!({
                "name": "code/read", "description": "read a file",
                "input_schema": {"type": "object", "properties": {}}
            }))
            .expect("test: tool spec")]),
            ..Default::default()
        };
        let response = TextGenerationResponse {
            text: "reading it".into(),
            finish_reason: crate::ai::FinishReason::ToolUse,
            model: "m".into(),
            provider: "p".into(),
            usage: crate::ai::UsageMetrics::default(),
            response_time_ms: 0,
            request_id: "r".into(),
            content: None,
            tool_calls: Some(vec![crate::ai::ToolCall { id: "t1".into(), name: "code/read".into(), input: json!({"path": "a.rs"}) }]),
            reasoning: Some("read before guessing".into()),
            routing: None,
            error: None,
            timing: None,
        };
        let lived = TrainingExample {
            prompt: "card: fix it".into(),
            completion: "reading it".into(),
            metadata: None,
            lived: Some(super::super::LivedCall { capture: "c".into(), request, response }),
        };
        let e = serde_json::to_value(engine_example(&lived)).expect("test: wire");
        let m = e["messages"].as_array().expect("test: messages");
        assert_eq!(m.len(), 6);
        assert_eq!((m[0]["role"].as_str(), m[0]["content"].as_str()), (Some("system"), Some("you are Kimi")));
        // the history is the one serving sent: her earlier act keeps its tool call, and its
        // result is a tool message bound to that call (Cormac on #4445)
        assert_eq!(m[2]["tool_calls"][0]["function"]["name"], "code/run");
        assert_eq!(m[2]["train"], json!(false), "her earlier act is context, not this lesson");
        assert_eq!((m[3]["role"].as_str(), m[3]["tool_call_id"].as_str()), (Some("tool"), Some("t0")));
        let reply = &m[5];
        assert!(reply.get("train").is_none(), "the reply is trained");
        assert_eq!(reply["reasoning_content"], "read before guessing");
        assert_eq!(reply["tool_calls"][0]["function"]["name"], "code/read");
        assert_eq!(reply["tool_calls"][0]["function"]["arguments"], "{\"path\":\"a.rs\"}");
        assert_eq!(e["tools"][0]["function"]["name"], "code/read");
        // a self-tick: the history ends in her own turn, and serving closed it before her reply
        let mut tick = lived.clone();
        let call = tick.lived.as_mut().expect("test: lived");
        call.request.messages = vec![ChatMessage::text("user", "go"), ChatMessage::text("assistant", "thinking it over")];
        let e = serde_json::to_value(engine_example(&tick)).expect("test: wire");
        let m = e["messages"].as_array().expect("test: messages");
        let roles: Vec<&str> = m.iter().filter_map(|x| x["role"].as_str()).collect();
        assert!(
            !roles.windows(2).any(|w| w == ["assistant", "assistant"]),
            "never two assistant turns in a row, as serving never sends them: {roles:?}"
        );
        let plain = TrainingExample { prompt: "p".into(), completion: "c".into(), metadata: None, lived: None };
        assert_eq!(serde_json::to_value(engine_example(&plain)).expect("test: wire"), json!({"prompt": "p", "completion": "c"}));
    }

    // what this catches: the dispatch end to end against an engine-shaped lane — the request
    // reaches /train as examples with GGUF targets and the request's window/epochs/rank; the
    // finished adapter LEAVES the lanes' train dir for the job dir before the job is terminal
    // (Fable's invariant: the engine-train sweep deletes files of jobs that are not live); the
    // artifact is a GgufLora with the engine's losses and trainable-token count.
    #[tokio::test]
    async fn a_finished_run_moves_its_adapter_out_of_the_train_dir_and_reports_the_engines_losses() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "normal").await;
        let t = EngineLoraFineTuner::for_test(url, train.path().to_path_buf(), jobs.path().join("footprints.json"));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r).await.expect("test: create");
        let TrainingStatus::Completed { artifact } = wait_terminal(&t, &h).await else {
            panic!("test: not completed");
        };
        let body = seen.lock().unwrap().clone().expect("test: /train was posted");
        assert_eq!(body["targets"], "attn_q,attn_v");
        assert_eq!((body["window"].as_u64(), body["epochs"].as_u64(), body["rank"].as_u64()), (Some(256), Some(2), Some(8)));
        assert_eq!(body["examples"][0]["completion"], "c");
        assert!(body.get("text").is_none(), "examples, never a text corpus (the engine masks the prompts)");
        let path = artifact.local_path.expect("test: path");
        assert_eq!(artifact.format, ArtifactFormat::GgufLora);
        assert!(path.starts_with(jobs.path()) && path.is_file(), "adapter in the job dir: {}", path.display());
        assert_eq!(std::fs::read_dir(train.path()).unwrap().count(), 0, "nothing left in engine-train");
        assert_eq!(artifact.metrics.final_loss, Some(2.1));
        assert_eq!(artifact.metrics.trained_tokens, 80);
        server.abort();
    }

    // what this catches (Codex on #4472): the finish path filing a measured graph under the
    // depth that was ASKED rather than the depth the engine ADAPTED. An engine without
    // top_layers ignores it and measures a full-depth graph; filed under |d8, that number
    // would later be leased for a K=8 run as if it were one. And the gene must carry the
    // depth the engine reported, never the request's.
    #[tokio::test]
    async fn a_finished_run_files_its_graph_and_its_gene_under_the_depth_the_engine_adapted() {
        for (mode, filed, gene) in [("normal", "ggml-org/Qwen3.8-27B-GGUF|w256|r8|attn_q,attn_v", None), ("depth", "ggml-org/Qwen3.8-27B-GGUF|w256|r8|attn_q,attn_v|d8", Some(8))] {
            let train = tempfile::tempdir().expect("test: dir");
            let jobs = tempfile::tempdir().expect("test: dir");
            let (url, server, seen) = fake_lane(train.path().to_path_buf(), mode).await;
            let footprints = jobs.path().join("footprints.json");
            let t = EngineLoraFineTuner::for_test(url, train.path().to_path_buf(), footprints.clone());
            let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
            r.local_artifact_dir = Some(jobs.path().to_path_buf());
            r.lora.as_mut().unwrap().top_layers = Some(8);
            let h = t.create_job(r).await.expect("test: create");
            let TrainingStatus::Completed { artifact } = wait_terminal(&t, &h).await else {
                panic!("test: {mode}: not completed");
            };
            let body = seen.lock().unwrap().clone().expect("test: /train was posted");
            assert_eq!(body["top_layers"], 8, "{mode}: the asked depth reaches /train");
            let rows: Value = serde_json::from_slice(&std::fs::read(&footprints).expect("test: footprint filed")).unwrap();
            let keys: Vec<&String> = rows.as_object().unwrap().keys().collect();
            assert_eq!(keys, vec![filed], "{mode}: filed under the depth the engine adapted");
            assert_eq!(artifact.metrics.layers_adapted, gene, "{mode}: the gene's depth is the engine's report");
            server.abort();
        }
    }

    // what this catches: a cancel that reports Cancelled while the engine is still training
    // (the lease would be released under a live run). The job must call /train/cancel and
    // report Cancelled only after the engine says the run is cancelled; no adapter survives.
    #[tokio::test]
    async fn a_cancel_stops_the_run_on_the_engine_before_the_job_reports_cancelled() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, _) = fake_lane(train.path().to_path_buf(), "cancel_only").await;
        let t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("footprints.json"));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r).await.expect("test: create");
        tokio::time::timeout(Duration::from_secs(10), async {
            while !matches!(t.poll(&h).await.unwrap(), TrainingStatus::Running { .. }) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("test: never running");
        t.cancel(&h).await.expect("test: cancel");
        assert!(matches!(wait_terminal(&t, &h).await, TrainingStatus::Cancelled));
        let engine: Value = reqwest::get(format!("{url}/train")).await.unwrap().json().await.unwrap();
        assert_eq!(engine["state"], "cancelled", "the engine's run was stopped, not abandoned");
        server.abort();
    }

    // what this catches: a failure path that lets go of the lease while the engine still trains
    // (Cormac on #4443). A status the job cannot read is retried, not taken as the end; a state
    // it does not know makes it STOP the engine's run first, and only then fail.
    #[tokio::test]
    async fn no_failure_path_ends_the_job_while_the_engine_still_trains_it() {
        let jobs = tempfile::tempdir().expect("test: dir");
        // flaky: the first two GET /train answers are unreadable, then the run finishes
        let train = tempfile::tempdir().expect("test: dir");
        let (url, server, _) = fake_lane(train.path().to_path_buf(), "flaky").await;
        let t = EngineLoraFineTuner::for_test(url, train.path().to_path_buf(), jobs.path().join("f.json"));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r).await.expect("test: create");
        assert!(matches!(wait_terminal(&t, &h).await, TrainingStatus::Completed { .. }), "a transient read is retried");
        server.abort();
        // unknown: the engine reports a state the job does not know; it must cancel and wait
        let train = tempfile::tempdir().expect("test: dir");
        let (url, server, _) = fake_lane(train.path().to_path_buf(), "unknown").await;
        let t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("f.json"));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r).await.expect("test: create");
        assert!(matches!(wait_terminal(&t, &h).await, TrainingStatus::Failed { .. }));
        let engine: Value = reqwest::get(format!("{url}/train")).await.unwrap().json().await.unwrap();
        assert_eq!(engine["state"], "cancelled", "failed only after the engine's run was stopped");
        server.abort();
    }

    // what this catches: a footprint that shrinks because one run's sampling missed its peak.
    #[test]
    fn a_footprint_keeps_the_largest_observation() {
        let dir = tempfile::tempdir().expect("test: dir");
        let f = Footprints { path: dir.path().join("f.json") };
        let s = Shape { model: "m".into(), window: 256, rank: 8, targets: "attn_q".into(), depth: None };
        f.record(&s, 900, Uuid::nil()).unwrap();
        f.record(&s, 700, Uuid::nil()).unwrap();
        assert_eq!(f.get(&s), Some(900));
        f.record(&s, 1200, Uuid::nil()).unwrap();
        assert_eq!(f.get(&s), Some(1200));
    }

    // what this catches (fork #27, Codex's cases): a footprint leased at the wrong depth. The
    // graph is ~linear in the blocks adapted, so a K=8 number leased for a full-depth run
    // under-reserves (the engine then refuses or the node overcommits), and a full-depth number
    // leased for K=8 locks out a run that fits. A persisted reduced-depth row and a legacy
    // depthless (full-depth) row must each answer only their own depth, across a reload.
    #[test]
    fn a_footprint_answers_only_the_depth_it_was_measured_at() {
        let dir = tempfile::tempdir().expect("test: dir");
        let path = dir.path().join("f.json");
        let full = Shape { model: "m".into(), window: 1536, rank: 8, targets: "attn_q".into(), depth: None };
        let top8 = Shape { depth: Some(8), ..full.clone() };
        // a row written before depth existed: the key full depth still reads
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({ "m|w1536|r8|attn_q": { "bytes": 41_500, "measuredAtMs": 0, "job": "legacy" } })).unwrap(),
        )
        .unwrap();
        let f = Footprints { path: path.clone() };
        assert_eq!(f.get(&full), Some(41_500), "the legacy row is full depth");
        assert_eq!(f.get(&top8), None, "a reduced-depth lookup never falls back to the legacy row");
        f.record(&top8, 5_200, Uuid::nil()).unwrap();
        let reloaded = Footprints { path };
        assert_eq!(reloaded.get(&top8), Some(5_200));
        assert_eq!(reloaded.get(&full), Some(41_500), "the reduced row never answers for full depth");
        // what the engine reported decides the recorded depth: an engine that says nothing
        // (it predates top_layers) adapted every block, and K = n_layer is every block too
        assert_eq!(effective_depth(Some(8), Some(64)), Some(8));
        assert_eq!(effective_depth(Some(64), Some(64)), None);
        assert_eq!(effective_depth(None, None), None);
        // the window is a multiple of 256, rounded down, never under one context
        assert_eq!((train_window(1536), train_window(1600), train_window(100), train_window(9000)), (1536, 1536, 256, 8192));
    }

    // what this catches: a request the engine cannot run is refused before any job exists:
    // a target with no GGUF equivalent (silently dropping it trains a different adapter), and a
    // base no live lane serves (in-engine training has nothing to train on).
    #[tokio::test]
    async fn unknown_targets_and_unserved_bases_are_refused_up_front() {
        assert_eq!(gguf_targets(&["q_proj".into(), "attn_v".into(), "q_proj".into()]).unwrap(), "attn_q,attn_v");
        assert!(gguf_targets(&["lm_head".into()]).is_err());
        let dir = tempfile::tempdir().expect("test: dir");
        let mut t = EngineLoraFineTuner::for_test(String::new(), dir.path().into(), dir.path().join("f.json"));
        t.lane = Box::new(|_| None);
        assert!(matches!(t.create_job(request("some/other-base")).await, Err(FineTuningError::InvalidRequest(_))));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.lora.as_mut().unwrap().target_modules = vec!["lm_head".into()];
        assert!(matches!(t.create_job(r).await, Err(FineTuningError::InvalidRequest(_))));
    }

    // what this catches: progress that jumps or overflows — completed epochs plus the current
    // epoch's batch fraction, capped at 100.
    #[test]
    fn progress_is_completed_epochs_plus_the_current_batch_fraction() {
        let st = |v: Value| serde_json::from_value::<TrainStatus>(v).expect("test: status");
        let e = json!({"train_loss": 2.0, "eval_loss": 2.0});
        assert_eq!(progress_of(&st(json!({"state": "running", "epochs": [e], "batch": 2, "batch_max": 4})), 2), (75.0, 1));
        assert_eq!(progress_of(&st(json!({"state": "starting"})), 2), (0.0, 0));
        assert_eq!(progress_of(&st(json!({"state": "done", "epochs": [e, e, e]})), 2).0, 100.0);
        // the schema: a status without a state does not parse (retried, never read as "not ours"),
        // and a state this core does not know is Unknown, not a known one
        assert!(serde_json::from_value::<TrainStatus>(json!({"out": "x.gguf"})).is_err());
        assert_eq!(st(json!({"state": "paused"})).state, TrainState::Unknown);
    }
}
