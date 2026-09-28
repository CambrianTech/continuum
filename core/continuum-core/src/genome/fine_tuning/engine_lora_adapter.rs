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

/// Holds on in-engine training, by reason. While any hold stands, every in-engine run on
/// this node is paused at its next training or evaluation window and keeps its context,
/// optimizer, adapter and dataset position on the same resident base (fork #28); when the
/// last hold drops, it resumes where it stopped. This is how a continual mind trains and
/// thinks on one set of weights: learning yields to her turns without being thrown away.
/// One entry per live hold, keyed by a token unique to that hold (Codex on #4485: two holds
/// with the same reason are two holds; dropping one must not release the other). The value
/// is (scope, reason): the scope is a lane's root, or [`EVERY_LANE`]. The node has one set
/// ([`TrainingHolds::node`]); a tuner is handed the set it obeys, so a test owns its own
/// (Cormac on #4485: a process-global set made the tests order-dependent).
#[derive(Clone)]
struct TrainingHolds(Arc<watch::Sender<BTreeMap<u64, (String, String)>>>);

static NODE_HOLDS: std::sync::LazyLock<TrainingHolds> = std::sync::LazyLock::new(TrainingHolds::new);
static NEXT_HOLD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The scope of a hold on every in-engine run of this node.
const EVERY_LANE: &str = "*";

/// A lane's root as a hold scope: one lane, one key, however its url was spelled (Cormac on
/// #4485: a trailing slash made a hold that held nothing).
fn lane_scope(lane: &str) -> String {
    lane.trim_end_matches('/').to_string()
}

impl TrainingHolds {
    fn new() -> Self {
        Self(Arc::new(watch::Sender::new(BTreeMap::new())))
    }

    /// This node's holds: the set every production tuner obeys.
    fn node() -> &'static TrainingHolds {
        &NODE_HOLDS
    }

    /// Pause every in-engine run under this set until the hold drops; every call is its own hold.
    fn hold(&self, reason: &str) -> TrainingHold {
        self.insert(EVERY_LANE.to_string(), reason)
    }

    /// Pause the in-engine run on one lane (its root url) until the hold drops.
    fn hold_on(&self, lane: &str, reason: &str) -> TrainingHold {
        self.insert(lane_scope(lane), reason)
    }

    fn insert(&self, scope: String, reason: &str) -> TrainingHold {
        let token = NEXT_HOLD.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.0.send_modify(|holds| {
            holds.insert(token, (scope, reason.to_string()));
        });
        TrainingHold { holds: self.clone(), token }
    }

    fn subscribe(&self) -> watch::Receiver<BTreeMap<u64, (String, String)>> {
        self.0.subscribe()
    }
}

/// A standing hold on in-engine training; dropping it releases this hold and no other.
pub struct TrainingHold {
    holds: TrainingHolds,
    token: u64,
}

impl Drop for TrainingHold {
    fn drop(&mut self) {
        self.holds.0.send_modify(|holds| {
            holds.remove(&self.token);
        });
    }
}

/// Pause every in-engine training run on this node until the returned hold is dropped. The
/// reason names the hold in probes; every call is its own hold.
pub fn hold_training(reason: &str) -> TrainingHold {
    TrainingHolds::node().hold(reason)
}

/// Pause the in-engine run on one lane of this node (its root url) until the hold is dropped.
pub fn hold_training_on(lane: &str, reason: &str) -> TrainingHold {
    TrainingHolds::node().hold_on(lane, reason)
}

/// The reasons holding the run on `lane`: every node-wide hold and every hold on that lane.
fn holds_on(holds: &BTreeMap<u64, (String, String)>, lane: &str) -> Vec<String> {
    let lane = lane_scope(lane);
    holds
        .values()
        .filter(|(scope, _)| scope == EVERY_LANE || *scope == lane)
        .map(|(_, reason)| reason.clone())
        .collect()
}

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
    (sequence_length / 256).max(1) * 256
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
    /// "middle" (fork #29): at the served window a lived example trains whole; only a
    /// conversation longer than serving's own window drops its OLDEST history exchanges, and
    /// always keeps the system and tool head and her reply, the context serving always has
    /// (Cormac on #29). An engine before #29 ignores it.
    fit: &'static str,
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
    /// fork #28: a pause asked for, and one the worker has reached (a pause is real only when
    /// both are true); waiting on serving slots is a separate, automatic yield
    #[serde(default)]
    pause_requested: bool,
    #[serde(default)]
    paused: bool,
    #[serde(default)]
    waiting_for_serving: bool,
    /// the model's block count and the blocks this run adapts (fork #27); absent on an engine
    /// that predates `top_layers`, which adapts every block
    #[serde(default)]
    n_layer: Option<u32>,
    #[serde(default)]
    layers_adapted: Option<u32>,
    /// examples kept, cut from the front to fit the window, and skipped because her last
    /// reply alone did not fit (fork #29); absent on an engine before it
    #[serde(default)]
    examples: Option<u64>,
    #[serde(default)]
    examples_truncated: Option<u64>,
    #[serde(default)]
    examples_skipped: Option<u64>,
    /// the window the engine's training graph actually used (fork #30: the longest fitted
    /// example rounded up to 256, never above the window sent); absent on an engine before it
    #[serde(default)]
    window: Option<u32>,
    #[serde(default)]
    error: Option<String>,
}

/// Where the lane serving `base` answers, if one does on this node.
/// The live lane serving a base: its url and the per-slot window it was launched with (0 =
/// a record from before that field, which is unknown). One lookup decides both, so the
/// training window can never come from a second source that disagrees with the lane
/// (Cormac on #4498).
type LaneResolver = Box<dyn Fn(&str) -> Option<(String, u32)> + Send + Sync>;

fn live_lane_for(base: &str) -> Option<(String, u32)> {
    let rec = crate::inference::lane_registry::live_lane()?;
    (rec.model == base).then(|| (format!("http://127.0.0.1:{}", rec.port), rec.context_window))
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
    /// the holds this tuner's runs obey (the node's, or a test's own)
    holds: TrainingHolds,
    /// Where `genome/job-pause` records a job's pause, so it outlives the core (`None`: no
    /// home, so no persisted pauses; the in-memory holds still apply).
    hold_store: Option<PathBuf>,
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
            holds: TrainingHolds::node().clone(),
            hold_store: crate::commands::benchmark::continuum_home()
                .ok()
                .map(|home| super::training_hold_store::store_path(&home)),
        }
    }

    #[cfg(test)]
    fn for_test(lane_url: String, train_dir: PathBuf, footprints: PathBuf) -> Self {
        Self {
            jobs: NativeJobs::new(PROVIDER_ID),
            http: reqwest::Client::new(),
            // a lane launched at 256 per slot: the window every existing test asserts
            lane: Box::new(move |_| Some((lane_url.clone(), 256))),
            train_dir: Some(train_dir),
            footprints: Footprints { path: footprints },
            admission: Admission::Ungoverned,
            holds: TrainingHolds::new(),
            hold_store: None,
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
    holds: TrainingHolds,
    /// This job's id (its handle's `local_id`) and the store its persisted pauses live in.
    job: Uuid,
    hold_store: Option<PathBuf>,
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

    /// Steer the engine's pause toward `want` (fork #28's contract): ask when the status says
    /// otherwise, and let the next status tell whether it took. A refused or lost request is
    /// asked again next tick; uncertainty is never turned into a cancel. An engine without the
    /// routes (404) cannot pause: the run keeps its automatic serving yield.
    async fn steer_pause(&self, s: &TrainStatus, want: bool) {
        if s.pause_requested == want {
            return;
        }
        let verb = if want { "pause" } else { "resume" };
        let _ = self
            .http
            .post(format!("{}/train/{verb}", self.lane))
            .json(&json!({ "out": self.body.out }))
            .timeout(Duration::from_secs(10))
            .send()
            .await; // the status decides whether it took; a failure is retried on the next tick
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

    async fn run(self: Box<Self>, cancel: watch::Receiver<bool>, progress: RunProgress) -> InPlaceEnd {
        let (store, job) = (self.hold_store.clone(), self.job);
        let end = self.run_steered(cancel, progress).await;
        // The job ended (finished, failed or cancelled), and its pauses end with it. A dropped
        // future (a core shutting down) never reaches this line, so a run the next core adopts
        // keeps its pauses.
        if let Some(store) = store {
            let released = tokio::task::spawn_blocking(move || {
                super::training_hold_store::release_job(&store, job, super::training_hold_store::now_ms())
            })
            .await;
            if !matches!(released, Ok(Ok(_))) {
                crate::probe!(
                    class = "training.hold.release_failed",
                    job = %job,
                    "a finished job's persisted pauses could not be released; they expire on their TTL"
                );
            }
        }
        end
    }
}

impl EngineRun {
    async fn run_steered(self: Box<Self>, mut cancel: watch::Receiver<bool>, progress: RunProgress) -> InPlaceEnd {
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
        let mut holds = self.holds.subscribe();
        let mut was_paused = false;
        loop {
            tokio::select! {
                changed = cancel.changed(), if !cancelling => {
                    if changed.is_err() || *cancel.borrow() {
                        cancelling = true;
                    }
                }
                _ = holds.changed() => {}
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
                    // held: pause at the next window, keeping everything; released: resume. The
                    // lease stays held throughout, since the paused run keeps its allocation.
                    let mut reasons = holds_on(&holds.borrow(), &self.lane);
                    // a pause asked through genome/job-pause, read every tick: it outlives the core
                    if let Some(store) = &self.hold_store {
                        let now = super::training_hold_store::now_ms();
                        let persisted = super::training_hold_store::live_on(store, self.job, now).await;
                        reasons.extend(persisted.into_iter().map(|h| h.reason));
                    }
                    self.steer_pause(&s, !reasons.is_empty()).await;
                    // the worker's own state: paused while it waits at a boundary, whatever was
                    // asked (a resume request clears pause_requested before the worker wakes)
                    let now_paused = s.paused;
                    if now_paused != was_paused {
                        crate::probe!(
                            class = if now_paused { "training.run.paused" } else { "training.run.resumed" },
                            out = self.body.out.as_str(),
                            holds = reasons.join(",").as_str(),
                            // a pause with no hold is the engine yielding its slots to serving
                            yielding_to_serving = s.waiting_for_serving,
                            pct = pct as f64,
                            "an in-engine run reached a pause at a window boundary, or left one, with its optimizer and adapter kept"
                        );
                        was_paused = now_paused;
                    }
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
            || schedule.sequence_length < 16
            || !schedule.learning_rate.is_finite()
            || schedule.learning_rate <= 0.0
            || schedule.learning_rate > 1.0
            || !(1..=256).contains(&lora.rank)
            || lora.alpha == 0
        {
            return Err(FineTuningError::InvalidRequest(
                "invalid engine training data/schedule/LoRA geometry (epochs 1-100, sequence_length >= 16, lr (0,1], rank 1-256)".into(),
            ));
        }
        let targets = gguf_targets(&lora.target_modules).map_err(FineTuningError::InvalidRequest)?;
        let (lane, served_window) = (self.lane)(&request.base_model).ok_or_else(|| {
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
        // LEARNING SEES WHAT SERVING SEES (Joel, 2026-09-28: "stupidly low token sizes are
        // idiotic ... the same as inference"; "you're not supposed to make learning so different
        // from reality"). The window is the per-slot window the matched lane was LAUNCHED with,
        // from the same record that chose the lane: her turns run in it, so a lived example
        // trains whole with its system and tool head. No request sets it (§9.1): a record that
        // predates the field is refused, never trained at a guessed window (Cormac on #4498:
        // a fallback to the request's length is attempt #1 again). The engine measures the
        // training graph at this window before allocating and refuses past the lease: a window
        // that does not fit is an engineering problem, never a smaller window.
        if served_window == 0 {
            return Err(FineTuningError::InvalidRequest(format!(
                "the live lane serving {} has no recorded served window (a record from before the field): not training at a guessed window",
                request.base_model
            )));
        }
        // the engine's context granularity is 256; rounding a smaller served window UP would
        // train on more context than serving holds (Codex on #4498)
        if served_window < 256 {
            return Err(FineTuningError::InvalidRequest(format!(
                "the live lane serving {} holds {served_window} tokens a slot, under the engine's 256-token training granularity: not training past what serving holds",
                request.base_model
            )));
        }
        let window = train_window(served_window);
        crate::probe!(
            class = "training.job.window",
            requested = schedule.sequence_length as u64,
            served = u64::from(served_window),
            sent = window as u64,
            "the training window: the matched lane's served per-slot window, rounded to the engine's 256 granularity; the request's length never decides it"
        );
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
            fit: "middle",
        };
        let measured = self.footprints.get(&shape);
        let footprints_path = self.footprints.path.clone();
        let http = self.http.clone();
        let holds = self.holds.clone();
        let hold_store = self.hold_store.clone();
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
                holds,
                job: id,
                hold_store,
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
                    crate::probe!(
                        class = "training.job.examples_fit",
                        job = %id,
                        kept = status.examples.unwrap_or(0), // probe field: 0 = an engine that does not report it
                        truncated = status.examples_truncated.unwrap_or(0), // probe field: as above
                        skipped = status.examples_skipped.unwrap_or(0), // probe field: as above
                        "how her examples met the window: kept whole, fitted by dropping their oldest history, or skipped"
                    );
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
                    // The footprint is filed under the geometry that RAN (fork #30 sizes the graph
                    // to the data, the served window is only its ceiling): a graph measured at a
                    // 15k window must never be leased for a 61k one (Codex on #4498).
                    let ran_window = status.window.filter(|w| *w > 0).unwrap_or(shape.window);
                    if ran_window != shape.window {
                        crate::probe!(
                            class = "training.job.window_used",
                            job = %id,
                            sent = u64::from(shape.window),
                            used = u64::from(ran_window),
                            "the engine sized its training graph to the data: the footprint is filed under the window that ran"
                        );
                    }
                    let measured_shape = Shape { depth: adapted, window: ran_window, ..shape.clone() };
                    // A depth equal to the model's block count IS full depth (fork #27 refuses one
                    // past it): filed under the asked key too, or that request would calibrate on
                    // every run (Cormac on #4472).
                    let asked_full = matches!((shape.depth, status.n_layer), (Some(k), Some(n)) if k >= n);
                    if grown > 0 {
                        let store = Footprints { path: footprints_path };
                        if asked_full {
                            // the asked depth, at the window that ran
                            let asked_ran = Shape { window: ran_window, ..shape.clone() };
                            let _ = store.record(&asked_ran, grown, id); // best effort: the full-depth row below is the one that matters
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
            pause_requested: bool,
            pauses_seen: u32,
        }
        let lane = Arc::new(Mutex::new(Lane::default()));
        let seen = Arc::new(Mutex::new(None));
        let (l1, l2, l3, seen1) = (lane.clone(), lane.clone(), lane.clone(), seen.clone());
        let (l4, l5, l6) = (lane.clone(), lane.clone(), lane.clone());
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
                    // fork #28: a paused run reports it and does not advance
                    if l.pause_requested {
                        l.polls -= 1;
                        return axum::Json(json!({"state": "running", "out": out, "batch": 1, "batch_max": 4, "epochs": [],
                            "pause_requested": true, "paused": true}));
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
                    // fork #30: the engine sized its graph to the data and reports the window it used
                    if mode == "sized" {
                        done["window"] = json!(15_360);
                        done["window_asked"] = l.body.as_ref().and_then(|b| b.get("window")).cloned().unwrap_or(Value::Null);
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
            }))
            .route("/train/pause", post(move |axum::Json(b): axum::Json<Value>| {
                let lane = l4.clone();
                async move {
                    let mut l = lane.lock().unwrap();
                    assert_eq!(b["out"].as_str(), l.out.as_deref(), "pause names this job");
                    l.pause_requested = true;
                    l.pauses_seen += 1;
                    axum::Json(json!({"ok": true, "pause_requested": true, "paused": false}))
                }
            }))
            .route("/test/pause", get(move || {
                // the test's view of the pause, without advancing the run as GET /train does
                let lane = l6.clone();
                async move {
                    let l = lane.lock().unwrap();
                    axum::Json(json!({"pause_requested": l.pause_requested, "pauses_seen": l.pauses_seen}))
                }
            }))
            .route("/train/resume", post(move |axum::Json(b): axum::Json<Value>| {
                let lane = l5.clone();
                async move {
                    let mut l = lane.lock().unwrap();
                    assert_eq!(b["out"].as_str(), l.out.as_deref(), "resume names this job");
                    l.pause_requested = false;
                    axum::Json(json!({"ok": true, "pause_requested": false, "paused": true}))
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

    // what this catches (Joel, 2026-09-28; Cormac on #4498): a training window set by the
    // REQUEST instead of the lane. Her lane serves 61,696 tokens a slot; the request carries a
    // plan's 1024; the run trains at 61,696, from the same record that chose the lane. A lane
    // record with no served window is refused, never trained at a guessed one.
    #[tokio::test]
    async fn the_training_window_is_the_lanes_served_window_never_the_requests() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "normal").await;
        let mut t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("footprints.json"));
        let served = url.clone();
        t.lane = Box::new(move |_| Some((served.clone(), 61_696)));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        r.schedule.as_mut().expect("test: schedule").sequence_length = 1024;
        let h = t.create_job(r).await.expect("test: create");
        let _ = wait_terminal(&t, &h).await;
        let body = seen.lock().unwrap().clone().expect("test: /train was posted");
        assert_eq!(body["window"].as_u64(), Some(61_696), "the lane's served window, not the request's 1024");

        for (served, why) in [(0, "an unknown served window is refused, never guessed"), (200, "a window under 256 is refused, never rounded up past serving")] {
            let lane_url = url.clone();
            t.lane = Box::new(move |_| Some((lane_url.clone(), served)));
            let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
            r.local_artifact_dir = Some(jobs.path().to_path_buf());
            assert!(t.create_job(r).await.is_err(), "{why}");
        }
        server.abort();
    }

    // what this catches (Codex on #4498): a footprint filed under the window SENT when the
    // engine ran a smaller graph sized to the data. The lane serves 61,696 a slot; the engine
    // trains at 15,360; the measured graph is filed under w15360, so it can never be leased for
    // a real 61,696-token graph.
    #[tokio::test]
    async fn a_footprint_is_filed_under_the_window_the_engine_ran() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "sized").await;
        let footprints = jobs.path().join("footprints.json");
        let mut t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), footprints.clone());
        t.lane = Box::new(move |_| Some((url.clone(), 61_696)));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r).await.expect("test: create");
        let TrainingStatus::Completed { .. } = wait_terminal(&t, &h).await else {
            panic!("test: not completed");
        };
        assert_eq!(seen.lock().unwrap().clone().expect("test: posted")["window"].as_u64(), Some(61_696), "the served window is sent as the ceiling");
        let rows: Value = serde_json::from_slice(&std::fs::read(&footprints).expect("test: footprint filed")).unwrap();
        let keys: Vec<&String> = rows.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["ggml-org/Qwen3.8-27B-GGUF|w15360|r8|attn_q,attn_v"], "filed under the window that ran");
        server.abort();
    }

    // what this catches: the dispatch end to end against an engine-shaped lane — the request
    // reaches /train as examples with GGUF targets and the lane's window, the request's epochs/rank; the
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
        // what this catches (Kimi's first dream, 2026-09-28): a lived example longer than the
        // window refused the whole run; the engine is asked to keep each example's tail
        assert_eq!(body["fit"], "middle", "a conversation longer than the served window drops its oldest history, never its head");
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

    // what this catches (Codex on #4485): two holds sharing a reason collapsing into one, so
    // dropping either resumed training while the other still stood; and a hold on one lane
    // reaching another; and (Cormac) a lane spelled with a trailing slash holding nothing.
    // Each hold is its own; the run stays held until the last one drops.
    #[test]
    fn each_hold_is_its_own_and_the_run_stays_held_until_the_last_drops() {
        let set = TrainingHolds::new();
        let lane = "http://127.0.0.1:9001";
        let on = |l: &str| holds_on(&set.subscribe().borrow(), l).len();
        let a = set.hold_on(lane, "a directed turn is waiting");
        let b = set.hold_on(lane, "a directed turn is waiting");
        let c = set.hold_on(&format!("{lane}/"), "a lifecycle drain");
        assert_eq!(on(lane), 3, "same reason twice is two holds; a trailing slash is the same lane");
        assert_eq!(on("http://127.0.0.1:9002"), 0, "another lane is not held");
        drop(a);
        assert_eq!(on(&lane), 2, "the same-reason hold still stands");
        drop(c);
        assert_eq!(on(&lane), 1);
        drop(b);
        assert_eq!(on(&lane), 0, "released only when the last hold drops");
    }

    // what this catches (fork #28, Joel: necessary for continual minds): a training hold that
    // does not pause the engine's run, a paused run the job gives up on (the lease must stay
    // held and the run must survive), or a resume that never reaches the engine. Held: the
    // engine is asked to pause and reports it, and the job stays running; released: it
    // resumes and finishes with its adapter.
    #[tokio::test]
    async fn a_held_run_pauses_in_the_engine_and_finishes_after_the_hold_drops() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, _) = fake_lane(train.path().to_path_buf(), "normal").await;
        let t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("footprints.json"));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let hold = t.holds.hold_on(&url, "test: a directed turn is waiting");
        let h = t.create_job(r).await.expect("test: create");
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let engine: Value = reqwest::get(format!("{url}/test/pause")).await.unwrap().json().await.unwrap();
                if engine["pause_requested"] == true {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("test: the engine never paused under the hold");
        assert!(matches!(t.poll(&h).await.unwrap(), TrainingStatus::Running { .. }), "a paused run is still running");
        drop(hold);
        let TrainingStatus::Completed { artifact } = wait_terminal(&t, &h).await else {
            panic!("test: the run did not finish after the hold dropped");
        };
        assert!(artifact.local_path.expect("test: path").is_file(), "resumed and finished with its adapter");
        server.abort();
    }

    // what this catches (Joel's continual minds; Codex and Cormac on the hold design): a pause
    // asked through genome/job-pause that does not reach the engine, a release that leaves the
    // run paused, or a pause that outlives its job. The pause is a persisted fact keyed by the
    // job, so a relaunched core reads it; the run steers to it every tick; releasing resumes and
    // the run finishes; and a job that ends (here cancelled while paused) takes its pauses with
    // it, while another job's pause stands.
    #[tokio::test]
    async fn a_persisted_pause_steers_its_job_and_ends_with_it() {
        use super::super::training_hold_store as store;
        let paused = |url: String| async move {
            tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let engine: Value = reqwest::get(format!("{url}/test/pause")).await.unwrap().json().await.unwrap();
                    if engine["pause_requested"] == true {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await
            .expect("test: the engine never paused under the persisted hold");
        };
        let state = tempfile::tempdir().expect("test: dir");
        let holds = store::store_path(state.path());
        let bystander = store::add(&holds, Uuid::from_u128(9), "another job's pause", 60_000, store::now_ms()).expect("test: add");
        for mode in ["release", "cancel"] {
            let train = tempfile::tempdir().expect("test: dir");
            let jobs = tempfile::tempdir().expect("test: dir");
            let (url, server, _) = fake_lane(train.path().to_path_buf(), if mode == "cancel" { "cancel_only" } else { "normal" }).await;
            let mut t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("footprints.json"));
            t.hold_store = Some(holds.clone());
            let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
            r.local_artifact_dir = Some(jobs.path().to_path_buf());
            let h = t.create_job(r).await.expect("test: create");
            store::add(&holds, h.local_id, "test: an operator paused it", 60_000, store::now_ms()).expect("test: add");
            paused(url.clone()).await;
            assert!(matches!(t.poll(&h).await.unwrap(), TrainingStatus::Running { .. }), "{mode}: a paused run is still running");
            if mode == "release" {
                assert_eq!(store::release_job(&holds, h.local_id, store::now_ms()).unwrap(), 1);
                let TrainingStatus::Completed { .. } = wait_terminal(&t, &h).await else {
                    panic!("test: the run did not finish after its pause was released");
                };
            } else {
                t.cancel(&h).await.expect("test: cancel");
                assert!(matches!(wait_terminal(&t, &h).await, TrainingStatus::Cancelled));
                assert!(store::live_on(&holds, h.local_id, store::now_ms()).await.is_empty(), "the job's end took its pause with it");
            }
            server.abort();
        }
        let left = store::live_on(&holds, bystander.job, store::now_ms()).await;
        assert_eq!(left.len(), 1, "another job's pause stands");
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
        assert_eq!((train_window(1536), train_window(1600), train_window(100), train_window(61_696)), (1536, 1536, 256, 61_696), "no fixed ceiling: the served window is the window");
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
