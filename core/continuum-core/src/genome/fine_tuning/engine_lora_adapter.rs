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
    ReattachOutcome, TrainerHardware, TrainingArtifact, TrainingExample, TrainingJobRequest, TrainingStatus,
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
    /// The exact walk (fork #47) is its own shape: its chunk graph carries GRAD leaves for the
    /// cached K/V over the gradient horizon, several times the plain walk's. Measured on the
    /// 5090, 2026-10-10 04:34Z (card d6dae498): a plain-walk footprint of 3418 MiB was leased
    /// for Kimi's first exact run, whose chunk graph needed 9236 MiB more, and the job refused.
    #[serde(default)]
    exact: bool,
    /// The training chunk (one batch, one ubatch): the chunk graph's activations scale with
    /// it, so a shape at a smaller chunk is a smaller footprint, and the chunk the core sends
    /// is the one the footprint was measured at.
    #[serde(default = "default_chunk")]
    chunk: u32,
    /// Per-layer recompute (fork #37) is its own shape: the backward pass keeps only the layer
    /// outputs and rebuilds the rest, so a chunk's graph is a fraction of the one that keeps
    /// every layer's activations. Measured on the 5090, 2026-10-10 08:32Z (job 888ce9c6): the
    /// exact walk at chunk 256 without it needed 5.6 GiB more than the lease under every
    /// gradient horizon from 1155 down to 256. A footprint measured without recompute is never
    /// leased for a run with it, nor the reverse.
    #[serde(default)]
    recompute: bool,
}

/// The engine's own default chunk when a caller sends none (`server-train.cpp`,
/// `train_chunk_for(window, 512)`): the chunk every footprint row before `chunk` existed was
/// measured at.
const TRAINING_CHUNK: u32 = 512;
const fn default_chunk() -> u32 {
    TRAINING_CHUNK
}
/// The smallest chunk the engine accepts (`"chunk" >= 256`, a multiple of 256).
const TRAINING_CHUNK_MIN: u32 = 256;

/// The chunk the engine will actually train at for `window` when asked for `asked`: the
/// largest multiple of 256 that divides the window and is at most `asked` — the engine's
/// `train_chunk_for`, mirrored so the shape the core keys its footprint under is the shape
/// the engine runs.
/// A job the engine REFUSED before training: the shape its preflight measured, recorded in the
/// job's own directory (`refusal.json`). The trigger reads it to hand the job's examples back
/// to her bucket so the next dispatch chooses a smaller chunk (card 2dfce676). Without it a
/// refused job's examples sat in its directory until a human returned them, and the chunk
/// ladder #4894 builds never walked on its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Refusal {
    pub window: u32,
    pub chunk: u32,
    pub graph_bytes: u64,
    pub error: String,
}

/// Why a job ended WITHOUT training in a way that hands its examples back to her bucket
/// (card 0b8de4da). Recorded in the job's own directory (`returnable.json`) where it
/// happens; the trigger's tick reads it. A job that failed any other way records nothing
/// and is not returned automatically.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Returnable {
    /// The engine refused the run's graph: the next dispatch steps the chunk down.
    EngineRefused(Refusal),
    /// The lane this run was bound to was replaced while it waited at admission (a
    /// re-home or relaunch): nothing ran, and the next dispatch plans against the new lane.
    /// The 5090, 2026-10-10: a7893e09's 18 examples stranded this way until a human
    /// cancelled and returned it.
    LaneReplaced { error: String },
}

pub(crate) const RETURNABLE_FILE: &str = "returnable.json";
/// #4904's record, before `Returnable` (card 2dfce676): still read, as `EngineRefused`.
const LEGACY_REFUSAL_FILE: &str = "refusal.json";

fn write_returnable(job_dir: &Path, returnable: &Returnable) -> Result<(), String> {
    std::fs::create_dir_all(job_dir).map_err(|e| format!("{}: {e}", job_dir.display()))?;
    let path = job_dir.join(RETURNABLE_FILE);
    let text = serde_json::to_string(returnable).map_err(|e| e.to_string())?; // file on disk: the job dir's returnable.json, read back by the trigger after a restart
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// What `job_dir` records about a return, if anything: `returnable.json`, else #4904's
/// `refusal.json` as `EngineRefused`. A missing or unreadable record is none.
pub(crate) fn read_returnable(job_dir: &Path) -> Option<Returnable> {
    if let Some(r) = std::fs::read(job_dir.join(RETURNABLE_FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()) {
        return Some(r);
    }
    let legacy: Refusal = serde_json::from_slice(&std::fs::read(job_dir.join(LEGACY_REFUSAL_FILE)).ok()?).ok()?;
    Some(Returnable::EngineRefused(legacy))
}

/// PURE: whether a refused job has a smaller rung to step down to. At the smallest chunk a
/// return would only be refused again, forever, so the ladder ends there (the loop guard
/// `genome/training-trigger/return`'s own doc warns about).
pub(crate) fn refusal_has_smaller_rung(refusal: &Refusal) -> bool {
    train_chunk_for(refusal.window, refusal.chunk / 2) < refusal.chunk && refusal.chunk / 2 >= TRAINING_CHUNK_MIN
}

fn train_chunk_for(window: u32, asked: u32) -> u32 {
    let blocks = window / 256;
    let mut g = blocks.min((asked / 256).max(1));
    while g >= 1 {
        if blocks % g == 0 {
            return g * 256;
        }
        g -= 1;
    }
    256
}

/// The chunk a job trains at and the footprint it leases, from the largest chunk down: a
/// chunk whose measured footprint fits the governed VRAM leases that number; a chunk never
/// measured is a calibration run (leases everything governed and free, `None`); a chunk
/// measured too large for what is free is halved and the next shape asked. The smallest
/// chunk's measurement stands even when it does not fit: the governor then waits for the
/// capacity rather than this guessing smaller than the engine can run.
fn choose_chunk(window: u32, available: u64, measured_at: impl Fn(u32) -> Option<u64>) -> (u32, Option<u64>) {
    let mut chunk = train_chunk_for(window, TRAINING_CHUNK);
    loop {
        let measured = measured_at(chunk);
        let next = if chunk / 2 >= TRAINING_CHUNK_MIN { Some(train_chunk_for(window, chunk / 2)) } else { None };
        match (measured, next) {
            (None, _) => return (chunk, None),
            (Some(bytes), _) if bytes <= available => return (chunk, Some(bytes)),
            (Some(bytes), None) => return (chunk, Some(bytes)),
            (Some(_), Some(smaller)) if smaller == chunk => return (chunk, measured),
            (Some(_), Some(smaller)) => chunk = smaller,
        }
    }
}

impl Shape {
    /// Full depth keeps the key every row before depth existed was written under (those
    /// rows were all full-depth runs); a reduced depth adds `|dK`, so a reduced-depth lookup
    /// can never land on a full-depth row, nor a full-depth lookup on a reduced one.
    fn key(&self) -> String {
        let base = format!("{}|w{}|r{}|{}", self.model, self.window, self.rank, self.targets);
        let base = match self.depth {
            Some(k) => format!("{base}|d{k}"),
            None => base,
        };
        // The plain walk at the engine's own chunk for this window keeps the key every row
        // before `chunk`/`exact` existed was written under (all plain, all at the engine's
        // default); a chunk the core shrank and the exact walk add their own suffix, so
        // neither lands on the other's row.
        let base = if self.chunk == train_chunk_for(self.window, TRAINING_CHUNK) { base } else { format!("{base}|c{}", self.chunk) };
        let base = if self.exact { format!("{base}|exact") } else { base };
        if self.recompute { format!("{base}|rc") } else { base }
    }
}

/// A held-out share, exact: parts per million of the examples. Stored as an integer so the
/// record that carries it compares exactly (a residency record is `Eq`) and round-trips
/// deterministically; converted to the engine's `val_split` float only at the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SplitPpm(u32);

impl From<f32> for SplitPpm {
    fn from(share: f32) -> Self {
        Self((share.clamp(0.0, 1.0) * 1_000_000.0).round() as u32)
    }
}

impl From<SplitPpm> for f32 {
    fn from(split: SplitPpm) -> Self {
        split.0 as f32 / 1_000_000.0
    }
}

/// Everything a successor core needs to RE-ATTACH to an in-engine run it did not start, and to
/// finish it (SHARED-RESIDENT-LIFECYCLE.md step 3, card 7bb4e5a2). Recorded with the run's
/// engine binding, inside the admission hold, so a binding that exists always says how to
/// resume watching it. The engine's address is not here: it is the bound incarnation's own
/// ([`crate::inference::engine_residency::EngineIncarnation::root_url`]), one source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineRunSpec {
    /// Where the engine writes the run's adapter, stored whole so finishing never re-derives it
    /// from a train dir that may since have moved (Cormac on the step-3 plan).
    adapter_path: PathBuf,
    /// Where the finished adapter is collected (the job's own directory).
    job_dir: PathBuf,
    epochs: u32,
    /// The held-out share the run asked for: whether an eval loss is believed (`held_out_loss`).
    val_split: SplitPpm,
    /// The artifact's model id (`engine-local:<trait>:<job>`).
    model_id: String,
    /// The footprint key the run was planned under, for filing what it measured.
    shape: Shape,
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
        // a scanner holding the new temp must not lose the measurement (the 5090, 2026-10-10)
        crate::utils::file_replace::replace_file(&tmp, &self.path)
    }
}

/// How much slower her decoding may run while training runs on her lane, parts per million.
/// The bound the substrate means (Joel: "negligible impact on inference latency ... a slowdown
/// if clever can be unnoticed"); a time share only said how often a window ran, and a 25%
/// share took Kimi from 52 to 23 tok/s. The engine (fork #36) measures her decode rate with and
/// without a window running and sizes each yield to hold this. The lane's lease is where it
/// belongs once the lease registry carries it (INFERENCE-LANES-REALISTIC.md); until then ONE
/// value here, sent explicitly so the engine's own default is never load-bearing. Not an env var.
pub const TRAINING_MAX_SLOWDOWN_PPM: u32 = 100_000;

/// The share of the host memory free at admission that the exact walk may keep per training
/// window (its K/V gradient and a recurrent model's state checkpoints, fork #47), as a fraction
/// of `memory_pressure::current_available_bytes()`. The other half stays free for the serving
/// engine's file cache and everyone else: on BigMama (2026-10-06) a compile that evicted the
/// served model's pages from the cache put the engine in a page-fault loop for an hour. The
/// engine checkpoints the state more sparsely to fit, and refuses by name when the K/V gradient
/// alone does not. ONE value here, sent explicitly. Not an env var.
pub const TRAINING_HOST_SHARE: (u64, u64) = (1, 2);

/// The exact walk's host budget, MiB: `TRAINING_HOST_SHARE` of what is free now, never below one
/// MiB (the engine's floor, which it refuses by name when the window needs more). `None` when the
/// monitor has not read the host yet: then the job does not ask for the exact walk at all (the
/// plain walk is chunk-bounded), because an unknown budget must never mean an unbounded one: the
/// tighter memory is, the more the walk must be bounded, never the less (Fable on #4841).
fn exact_walk_host_budget_mib(available_bytes: Option<u64>) -> Option<u64> {
    let (num, den) = TRAINING_HOST_SHARE;
    available_bytes.map(|b| ((b / den * num) >> 20).max(1))
}

/// What the job asks the engine for, from the monitor's reading: the exact walk with its host
/// budget when the host is read, the plain walk (no budget) when it is not.
fn walk_request(available_bytes: Option<u64>) -> (bool, Option<u64>) {
    match exact_walk_host_budget_mib(available_bytes) {
        Some(mib) => (true, Some(mib)),
        None => (false, None),
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
    /// The bound on her decode slowdown while training runs, parts per million (fork #36).
    /// Sent INSTEAD of the time share (the engine refuses both). An engine before #36 ignores
    /// the key and paces by its own default share, which is what this core sent before.
    max_slowdown_ppm: u32,
    /// "middle" (fork #29): at the served window a lived example trains whole; only a
    /// conversation longer than serving's own window drops its OLDEST history exchanges, and
    /// always keeps the system and tool head and her reply, the context serving always has
    /// (Cormac on #29). An engine before #29 ignores it.
    fit: &'static str,
    /// The exact walk (fork #47): ONE optimizer step per window, on the gradient of the window's
    /// whole loss carried back through every chunk's cached K/V and a hybrid's recurrent state,
    /// instead of a step per chunk with the gradient stopped at each chunk boundary (the plain
    /// walk's step agreed with the true one at cosine 0.81; the exact walk's at 0.998). The engine
    /// shrinks its gradient horizon to fit the device and reports it. An engine before #47
    /// ignores the key and trains the plain walk, as this core asked before.
    exact: bool,
    /// The exact walk's host memory per window (see `TRAINING_HOST_SHARE`).
    #[serde(skip_serializing_if = "Option::is_none")]
    walk_host_budget_mib: Option<u64>,
    /// The training chunk (one batch, one ubatch), chosen with the lease it fits
    /// ([`choose_chunk`]). An engine before the key ignores it and takes 512.
    #[serde(skip_serializing_if = "Option::is_none")]
    chunk: Option<u32>,
    /// Per-layer recompute (fork #37): the backward pass keeps the layer outputs as checkpoints
    /// and rebuilds each layer's activations from them, so a chunk holds one layer's
    /// activations instead of every layer's. It is the memory a large model's chunk graph is
    /// made of (see [`Shape::recompute`]), and it takes the same step: fork #50 measures the
    /// exact walk with and without it at cosine 1.0000, norm ratio 1.0000. An engine before
    /// #37 ignores the key.
    recompute: bool,
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
    /// null when no held-out token was evaluated (fork #31); an engine before it reports 0.0
    #[serde(default)]
    eval_loss: Option<f64>,
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
    /// the held-out trainable tokens the eval loss is measured over (fork #31); absent on an
    /// engine before it
    #[serde(default)]
    eval_trainable_tokens: Option<u64>,
    /// the training graph the engine measured before allocating it: this shape's footprint
    #[serde(default)]
    graph_mib: Option<f64>,
    /// The chunk the engine trained at (`state["chunk"]`): the shape its footprint is filed
    /// under. An engine before the key reports none and the core mirrors its rule.
    #[serde(default)]
    chunk: Option<u32>,
    /// fork #28: a pause asked for, and one the worker has reached (a pause is real only when
    /// both are true); waiting on serving slots is a separate, automatic yield
    #[serde(default)]
    pause_requested: bool,
    #[serde(default)]
    paused: bool,
    #[serde(default)]
    waiting_for_serving: bool,
    /// the share the run took of the lane (fork #32): windows run, windows taken while a
    /// serving slot was busy, and the time yielded to serving; absent on an engine before it
    #[serde(default)]
    yielded_ms: Option<u64>,
    #[serde(default)]
    windows: Option<u64>,
    #[serde(default)]
    windows_while_busy: Option<u64>,
    #[serde(default)]
    share_ppm: Option<u64>,
    /// the slowdown bound and her measured rates (fork #36): tokens/s with no window running
    /// and while one ran, over the run, and the slowdown they realized; absent before #36
    #[serde(default)]
    max_slowdown_ppm: Option<u64>,
    #[serde(default)]
    decode_tps_no_window: Option<f64>,
    #[serde(default)]
    decode_tps_in_window: Option<f64>,
    #[serde(default)]
    slowdown_ppm_realized: Option<u64>,
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
type LaneResolver = Box<dyn Fn(&str) -> Option<LaneChoice> + Send + Sync>;

/// The lane a run will train in, as the ONE live-lane record names it.
#[derive(Debug, Clone)]
struct LaneChoice {
    url: String,
    /// The per-slot window it was launched with (0 = unknown, refused).
    window: u32,
    /// The engine process hosting it (pid plus OS start time), the identity a run binds to at
    /// admission (SHARED-RESIDENT-LIFECYCLE.md step 1). `None` only in tests with no engine.
    engine: Option<crate::inference::engine_residency::EngineIncarnation>,
}

/// A lane record's incarnation. A record from before `started_s` names the live process by
/// its pid and the start time the table reports now.
fn incarnation_of(rec: &crate::inference::lane_registry::LaneRecord) -> Option<crate::inference::engine_residency::EngineIncarnation> {
    if rec.started_s != 0 {
        Some(rec.incarnation())
    } else {
        crate::inference::engine_residency::EngineIncarnation::of(rec.pid, rec.port)
    }
}

fn live_lane_for(base: &str) -> Option<LaneChoice> {
    let rec = crate::inference::lane_registry::live_lane()?;
    (rec.model == base).then(|| LaneChoice { url: rec.root_url(), window: rec.context_window, engine: incarnation_of(&rec) })
}

/// Admission: the governed lease a run holds for its life. `Governed` in production;
/// tests drive the run without a governor.
enum Admission {
    Governed,
    /// No governor: the free VRAM a chunk is fitted against is what the test says it is
    /// (`u64::MAX` = nothing to fit against, the default chunk).
    #[cfg(test)]
    Ungoverned { vram_free: u64 },
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
    /// Where a run's binding to its engine incarnation is recorded (`None`: no home, or a
    /// test with no engine; an ungoverned run binds nothing).
    residency_store: Option<PathBuf>,
    /// Leases re-taken at boot for runs a previous core bound to a still-live engine
    /// ([`Self::reclaim_resident_leases`]), held here until `reattach` hands each to its run or
    /// the binding is released. The lease was in the dead core's memory; the engine's
    /// allocation was not, so until this re-takes it a second job could be admitted into it.
    resident_leases: Arc<Mutex<BTreeMap<Uuid, crate::resources::LeaseGuard>>>,
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
            residency_store: crate::commands::benchmark::continuum_home()
                .ok()
                .map(|home| crate::inference::engine_residency::store_path(&home)),
            resident_leases: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    #[cfg(test)]
    /// [`Self::for_test`] on a host with `vram_free` bytes governed and free, so a chunk is
    /// chosen against it (card d6dae498).
    #[cfg(test)]
    fn for_test_with_vram(lane_url: String, train_dir: PathBuf, footprints: PathBuf, vram_free: u64) -> Self {
        let mut t = Self::for_test(lane_url, train_dir, footprints);
        t.admission = Admission::Ungoverned { vram_free };
        t
    }

    /// The VRAM a job may fit a chunk into now: the governor's figure for this consumer, or
    /// what an ungoverned (test) host declares.
    fn vram_free_for(&self, consumer: &str) -> u64 {
        match self.admission {
            Admission::Governed => crate::resources::ResourceDaemon::global()
                .map_or(0, |d| d.available_for(consumer, crate::resources::ResourceKind::Vram)), // map_or: a governed host whose governor is gone has nothing free to lease (the governed branch refuses below)
            #[cfg(test)]
            Admission::Ungoverned { vram_free } => vram_free,
        }
    }

    #[cfg(test)]
    fn for_test(lane_url: String, train_dir: PathBuf, footprints: PathBuf) -> Self {
        Self {
            jobs: NativeJobs::new(PROVIDER_ID),
            http: reqwest::Client::new(),
            // a lane launched at 256 per slot: the window every existing test asserts
            lane: Box::new(move |_| Some(LaneChoice { url: lane_url.clone(), window: 256, engine: None })),
            train_dir: Some(train_dir),
            footprints: Footprints { path: footprints },
            admission: Admission::Ungoverned { vram_free: u64::MAX },
            holds: TrainingHolds::new(),
            hold_store: None,
            residency_store: None,
            resident_leases: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}

/// The in-place run: POST /train on the lane, poll to its end, stop it on a cancel.
struct EngineRun {
    http: reqwest::Client,
    lane: String,
    /// The shape this run trains, and where footprints are filed: a REFUSED run files the
    /// graph the engine's preflight measured, under the chunk it reports, so the next
    /// dispatch of this shape chooses a smaller chunk instead of calibrating at the same
    /// one forever (BigMama on #4894: footprints were filed by a finished run only).
    shape: Shape,
    footprints_path: PathBuf,
    /// The run to POST, or `None` for a run this core RE-ATTACHED to: it is already training in
    /// the engine, and POSTing it again would be the duplicate run step 3 exists to prevent.
    start: Option<TrainRequest>,
    /// The engine's name for the run (`/train`'s `out`), what every status is matched on.
    out: String,
    /// where the engine writes this job's adapter (removed if a cancel races a finish)
    adapter_path: PathBuf,
    /// This job's own directory, where a refusal is recorded for the trigger (card 2dfce676).
    job_dir: PathBuf,
    epochs: u32,
    /// The run's last status as the engine reported it (finish reads its losses and footprint).
    last: Arc<Mutex<Option<TrainStatus>>>,
    holds: TrainingHolds,
    /// This job's id (its handle's `local_id`) and the store its persisted pauses live in.
    job: Uuid,
    hold_store: Option<PathBuf>,
    /// Where this run's binding to its engine incarnation is recorded, and that incarnation,
    /// when admission bound it (`None`: an ungoverned test run). Released only when the run's
    /// end is SETTLED: see [`EngineProbe::settled`].
    residency: Option<(PathBuf, crate::inference::engine_residency::ResidentWork)>,
    /// The governed lease, held for exactly as long as the engine may be running this job's
    /// training: `run` returns only once it has ended there, and the lease drops with `self`.
    _lease: Option<crate::resources::LeaseGuard>,
}

/// Just enough of a run to ask the engine, after the run's future has ended, whether this job
/// is still running there.
struct EngineProbe {
    http: reqwest::Client,
    lane: String,
    out: String,
}

impl EngineProbe {
    /// Is this run's end SETTLED (SHARED-RESIDENT-LIFECYCLE.md; Codex on #4531)? Only on
    /// positive evidence:
    /// - the incarnation is VERIFIABLY dead (no such process, or the pid was reused); or
    /// - the engine answers 2xx that THIS run (`out`) is in a terminal state (done, cancelled
    ///   or error), and the incarnation is still the same one after that network round trip.
    /// Everything else is unknown and holds: an unreadable process, no answer, a non-2xx
    /// answer, a foreign or idle answer (not proof about this run).
    async fn settled(&self, engine: &crate::inference::engine_residency::EngineIncarnation) -> bool {
        use crate::inference::engine_residency::Liveness;
        let liveness = || {
            let engine = *engine;
            tokio::task::spawn_blocking(move || engine.liveness())
        };
        match liveness().await {
            Ok(Liveness::Dead) => return true,
            Ok(Liveness::Alive) => {}
            Ok(Liveness::Unknown) | Err(_) => return false,
        }
        let Ok(r) = self.http.get(format!("{}/train", self.lane)).timeout(Duration::from_secs(10)).send().await else {
            return false;
        };
        if !r.status().is_success() {
            return false;
        }
        let Ok(s) = r.json::<TrainStatus>().await else { return false };
        let this_run_ended = s.out.as_deref() == Some(self.out.as_str())
            && matches!(s.state, TrainState::Done | TrainState::Cancelled | TrainState::Error);
        // the answer is only this engine's if the engine is still the same process
        this_run_ended && matches!(liveness().await, Ok(Liveness::Alive))
    }
}

impl EngineRun {
    async fn status_once(&self) -> Result<TrainStatus, String> {
        Self::status_at(&self.http, &self.lane).await
    }

    /// One `GET /train` on `lane`: the engine's training status, or why it could not be read.
    async fn status_at(http: &reqwest::Client, lane: &str) -> Result<TrainStatus, String> {
        let r = http
            .get(format!("{lane}/train"))
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("GET /train on {lane}: {e}"))?;
        if !r.status().is_success() {
            return Err(format!("GET /train on {lane}: HTTP {}", r.status()));
        }
        r.json::<TrainStatus>().await.map_err(|e| format!("GET /train on {lane}: {e}"))
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
        s.out.as_deref() == Some(self.out.as_str())
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
            .json(&json!({ "out": self.out }))
            .timeout(Duration::from_secs(10))
            .send()
            .await; // the status decides whether it took; a failure is retried on the next tick
    }
}

/// The run's HELD-OUT loss, only when held-out tokens were actually evaluated (Codex, attempt 2).
/// With no split, the engine's empty eval result is a loss of 0.0, and read as a validation
/// loss that would claim a perfect held-out score. An engine with fork #31 says how many
/// held-out tokens it evaluated; one before it says nothing, and then only a run that asked for
/// a split is believed to have one.
fn held_out_loss(s: &TrainStatus, asked_split: f32) -> Option<f64> {
    let evaluated = s.eval_trainable_tokens.map_or(asked_split > 0.0, |n| n > 0);
    s.epochs.last().and_then(|e| e.eval_loss).filter(|_| evaluated)
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
        let residency = self.residency.clone();
        let probe_run = EngineProbe { http: self.http.clone(), lane: self.lane.clone(), out: self.out.clone() };
        let mut end = self.run_steered(cancel, progress).await;
        // serving replaced the engine under this run anyway (an emergency): the failure says why
        if let (InPlaceEnd::Failed(why), Some((path, _))) = (&end, &residency) {
            if let Some(reason) = crate::inference::engine_residency::interruption_of(path, job) {
                end = InPlaceEnd::Failed(format!("{why}; serving replaced the engine under this run: {reason}"));
            }
        }
        // THE ENGINE IS RELEASED ONLY WHEN THE END IS SETTLED (SHARED-RESIDENT-LIFECYCLE.md):
        // the engine answered that this job is not running, or its incarnation is verifiably
        // gone. A lane that is alive but unreachable keeps the record: the run may still be
        // training in it, so serving must not replace it on a guess (the explicit recovery act
        // resolves that case, never a timeout).
        if let Some((path, bound)) = residency {
            if probe_run.settled(&bound.engine).await {
                let released = tokio::task::spawn_blocking(move || crate::inference::engine_residency::release(&path, &bound)).await;
                if !matches!(released, Ok(Ok(_))) {
                    crate::probe!(
                        class = "training.residency.release_failed",
                        job = %job,
                        "a settled run's engine binding could not be released; serving stays held on it until it can be"
                    );
                }
            } else {
                crate::probe!(
                    class = "training.residency.kept_uncertain",
                    job = %job,
                    pid = bound.engine.pid as u64,
                    "the run ended here but its engine is alive and did not answer: the binding stays, serving stays off the engine"
                );
            }
        }
        // The job ended (finished, failed or cancelled), and its pauses end with it. A dropped
        // future (a core shutting down) never reaches this line, so its intent stays on disk for
        // the re-attach path that does not exist yet (see training_hold_store).
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
        // a re-attached run is already in the engine: it is watched, never started again
        if let Some(body) = &self.start {
            match self
                .http
                .post(format!("{}/train", self.lane))
                .json(body)
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
                        match super::training_hold_store::live_on(store, self.job, now).await {
                            Ok(persisted) => reasons.extend(persisted.into_iter().map(|h| h.reason)),
                            // unreadable intent holds: a paused job is never resumed on a guess
                            Err(e) => reasons.push(format!("hold store unreadable: {e}")),
                        }
                    }
                    self.steer_pause(&s, !reasons.is_empty()).await;
                    // the worker's own state: paused while it waits at a boundary, whatever was
                    // asked (a resume request clears pause_requested before the worker wakes)
                    let now_paused = s.paused;
                    if now_paused != was_paused {
                        crate::probe!(
                            class = if now_paused { "training.run.paused" } else { "training.run.resumed" },
                            out = self.out.as_str(),
                            holds = reasons.join(",").as_str(),
                            // a pause with no hold is the engine yielding its slots to serving
                            yielding_to_serving = s.waiting_for_serving,
                            pct = pct as f64,
                            "an in-engine run reached a pause at a window boundary, or left one, with its optimizer and adapter kept"
                        );
                        was_paused = now_paused;
                    }
                }
                TrainState::Done => {
                    // THE SHARE, as the engine itself counted it (Cormac on #4798): auditable on
                    // every node from the run's own receipt, no second run. An engine before
                    // fork #32 reports no counts: the probe says so, so "not reported" is
                    // never read as a zero share, and an old engine is told apart from a probe
                    // that never fired.
                    match (s.windows, s.windows_while_busy, s.yielded_ms) {
                        (Some(windows), Some(busy), Some(yielded)) => crate::probe!(
                            class = "training.run.share",
                            out = self.out.as_str(),
                            reported = true,
                            windows,
                            windows_while_busy = busy,
                            yielded_ms = yielded,
                            share_ppm = s.share_ppm.map(|v| v as i64).unwrap_or(-1), // -1 = counts reported, policy not (an engine between the counts and the policy); never a 0 that reads as a zero share
                            max_slowdown_ppm = s.max_slowdown_ppm.map(|v| v as i64).unwrap_or(-1), // -1 = an engine before the bound (fork #36): it paced by the share
                            slowdown_ppm_realized = s.slowdown_ppm_realized.map(|v| v as i64).unwrap_or(-1), // -1 = not measured: no busy stretch both with and without a window, or an engine before #36
                            decode_tps_no_window = s.decode_tps_no_window.unwrap_or(-1.0), // -1 = not measured, never a 0 that reads as her stopping
                            decode_tps_in_window = s.decode_tps_in_window.unwrap_or(-1.0), // -1 = not measured, never a 0 that reads as her stopping
                            "the in-engine run finished: how many windows it took beside busy serving, how long it yielded, and what it cost her decoding against the bound"
                        ),
                        _ => crate::probe!(
                            class = "training.run.share",
                            out = self.out.as_str(),
                            reported = false,
                            "the in-engine run finished on an engine that does not report its share (before fork #32): the counts are not known"
                        ),
                    }
                    return InPlaceEnd::Finished;
                }
                TrainState::Cancelled => return InPlaceEnd::Failed("the engine's run was cancelled by someone else".into()),
                TrainState::Error => {
                    let why = s.error.as_deref().unwrap_or("no error text"); // unwrap_or: the state alone is the failure
                    // THE REFUSAL IS THE MEASUREMENT: the preflight sized the graph before refusing
                    // it, and the engine reports that size on an error too. Filed under the chunk
                    // the engine ran, so `choose_chunk` steps down on the next dispatch.
                    let grown = s.graph_mib.map_or(0, |m| (m * 1024.0 * 1024.0) as u64);
                    if grown > 0 {
                        let ran_window = s.window.unwrap_or(self.shape.window); // unwrap_or: an engine that reports no window ran the one asked
                        let ran_chunk = s.chunk.unwrap_or_else(|| train_chunk_for(ran_window, self.shape.chunk)); // unwrap_or_else: an engine that reports no chunk ran the mirrored rule
                        let refused = Shape { window: ran_window, chunk: ran_chunk, ..self.shape.clone() };
                        // The job's examples belong back in her bucket so the next dispatch can
                        // step down; the trigger reads this to decide (card 2dfce676).
                        if let Err(e) = write_returnable(&self.job_dir, &Returnable::EngineRefused(Refusal { window: ran_window, chunk: ran_chunk, graph_bytes: grown, error: why.to_string() })) {
                            crate::probe!(
                                class = "training.job.refusal_unrecorded",
                                job = %self.job,
                                error = %e,
                                "the refusal could not be recorded in the job's directory; its examples wait for a manual return"
                            );
                        }
                        match (Footprints { path: self.footprints_path.clone() }).record(&refused, grown, self.job) {
                            Ok(()) => crate::probe!(
                                class = "training.job.footprint_from_refusal",
                                job = %self.job,
                                window = ran_window as u64,
                                chunk = ran_chunk as u64,
                                exact = refused.exact,
                                graph_bytes = grown,
                                "the engine refused this shape's graph; its measured size is filed so the next dispatch chooses a smaller chunk"
                            ),
                            Err(e) => crate::probe!(
                                class = "training.job.footprint_unrecorded",
                                job = %self.job,
                                error = %e,
                                "the refused graph's size could not be recorded; the next dispatch calibrates at the same chunk"
                            ),
                        }
                    }
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
fn move_adapter(from: &Path, job_dir: &Path) -> Result<PathBuf, String> {
    let size = std::fs::metadata(from).map_err(|e| format!("the engine wrote no adapter at {}: {e}", from.display()))?.len();
    if size == 0 {
        return Err(format!("the engine's adapter at {} is empty", from.display()));
    }
    let adapters = job_dir.join("adapters");
    std::fs::create_dir_all(&adapters).map_err(|e| format!("{}: {e}", adapters.display()))?;
    let to = adapters.join("adapter.gguf");
    std::fs::rename(from, &to).map_err(|e| format!("moving {} -> {}: {e}", from.display(), to.display()))?;
    Ok(to)
}

/// What re-attaching decides from what it observed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Disposition {
    Attach,
    Gone(String),
    Uncertain(String),
}

/// PURE: the step-3 decision, from what was observed in order: the bound incarnation's liveness
/// BEFORE asking, the engine's answer, and its liveness AFTER (the answer is this engine's only
/// if the process did not change under the round trip). Only positive evidence decides:
/// the same live incarnation answering with THIS run attaches, and a verifiably dead one is
/// gone. An engine that answers with another run, or not at all, is not proof this run ended
/// (Codex on the step-3 plan): that is uncertain, and uncertain never resumes.
fn reattach_disposition(
    before: crate::inference::engine_residency::Liveness,
    has_spec: bool,
    answer: Option<Result<TrainStatus, String>>,
    after: crate::inference::engine_residency::Liveness,
    out: &str,
) -> Disposition {
    use crate::inference::engine_residency::Liveness;
    match before {
        Liveness::Dead => return Disposition::Gone("its bound engine incarnation is verifiably gone".into()),
        Liveness::Unknown => return Disposition::Uncertain("its bound engine's liveness cannot be read".into()),
        Liveness::Alive => {}
    }
    if !has_spec {
        return Disposition::Uncertain("the binding predates job_spec, so the run cannot be watched again here".into());
    }
    match answer {
        None => return Disposition::Uncertain("the engine was not asked".into()),
        Some(Err(e)) => return Disposition::Uncertain(format!("the engine did not answer: {e}")),
        Some(Ok(s)) if s.out.as_deref() != Some(out) => {
            return Disposition::Uncertain(format!("the engine reports run {:?}, not this one: not proof this run ended", s.out))
        }
        Some(Ok(_)) => {}
    }
    match after {
        Liveness::Alive => Disposition::Attach,
        Liveness::Dead => Disposition::Gone("its engine died while it was being asked".into()),
        Liveness::Unknown => Disposition::Uncertain("its engine's liveness could not be re-read after the answer".into()),
    }
}

impl EngineLoraFineTuner {
    /// BOOT, before anything can admit (Cormac on the step-3 plan): re-take the governed lease
    /// of every run a previous core bound to an engine that may still be running it. The lease
    /// lived in the dead core's memory; the engine's allocation did not, so until this runs a
    /// second job could be admitted into memory a surviving run is using. Called once, right
    /// after the adapter is built and before any module ticks; a binding whose engine is
    /// verifiably dead takes no lease (re-attach releases it).
    pub fn reclaim_resident_leases(&self) {
        use crate::inference::engine_residency::Liveness;
        let (Some(store), Some(daemon)) = (self.residency_store.as_ref(), crate::resources::ResourceDaemon::global()) else {
            return;
        };
        let bound = match crate::inference::engine_residency::all(store) {
            Ok(bound) => bound,
            Err(error) => {
                crate::probe!(
                    class = "training.reattach.store_unreadable",
                    error = %error,
                    "engine bindings cannot be read at boot: ownership is unknown, so serving stays off every engine it guards"
                );
                return;
            }
        };
        for work in bound {
            if work.engine.liveness() == Liveness::Dead || work.reserved_bytes == 0 {
                continue;
            }
            let consumer = if work.consumer.is_empty() { format!("genome-train:{}", work.job) } else { work.consumer.clone() };
            match daemon.acquire_guarded(&crate::forge::training_admission::request(&consumer, work.reserved_bytes)) {
                Ok(guard) => {
                    if let Ok(mut leases) = self.resident_leases.lock() {
                        leases.insert(work.job, guard);
                    }
                    crate::probe!(
                        class = "training.reattach.lease_reclaimed",
                        job = %work.job,
                        bytes = work.reserved_bytes,
                        "a run bound to a live engine by a previous core: its lease is re-taken before anything can admit"
                    );
                }
                Err(error) => crate::probe!(
                    class = "training.reattach.lease_refused",
                    job = %work.job,
                    bytes = work.reserved_bytes,
                    error = %format!("{error:?}"),
                    "a surviving run's lease could not be re-taken: its memory is in use but unaccounted"
                ),
            }
        }
    }

    /// Step 3: re-attach to `job` if a previous core bound it to an engine that is still running
    /// it, under the SAME id and without POSTing it again. See [`ReattachOutcome`].
    async fn reattach_job(&self, job: Uuid) -> Result<ReattachOutcome, FineTuningError> {
        let Some(store) = self.residency_store.clone() else {
            return Ok(ReattachOutcome::NotResident);
        };
        let lookup = store.clone();
        let found = tokio::task::spawn_blocking(move || crate::inference::engine_residency::find(&lookup, job))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
        let work = match found {
            Ok(Some(work)) => work,
            Ok(None) => return Ok(ReattachOutcome::NotResident),
            // an unreadable store is unknown ownership: never a reason to start the job again
            Err(error) => return Ok(ReattachOutcome::Uncertain { reason: format!("engine bindings cannot be read: {error}") }),
        };
        let liveness = |engine: crate::inference::engine_residency::EngineIncarnation| async move {
            tokio::task::spawn_blocking(move || engine.liveness())
                .await
                .unwrap_or(crate::inference::engine_residency::Liveness::Unknown) // unwrap_or: a probe that could not run proves nothing
        };
        let lane = work.engine.root_url();
        let before = liveness(work.engine).await;
        let answer = if before == crate::inference::engine_residency::Liveness::Alive && work.job_spec.is_some() {
            let probe = EngineRun::status_at(&self.http, &lane).await;
            Some(probe)
        } else {
            None
        };
        let after = liveness(work.engine).await;
        match reattach_disposition(before, work.job_spec.is_some(), answer, after, &work.out) {
            Disposition::Gone(reason) => {
                let (release_store, bound) = (store.clone(), work.clone());
                let released = tokio::task::spawn_blocking(move || crate::inference::engine_residency::release(&release_store, &bound)).await;
                if let Ok(mut leases) = self.resident_leases.lock() {
                    leases.remove(&job);
                }
                crate::probe!(
                    class = "training.reattach.engine_gone",
                    job = %job,
                    released = matches!(released, Ok(Ok(true))),
                    reason = reason.as_str(),
                    "a previous core's run: its engine is verifiably gone, so its binding is released and the job may resume from its input"
                );
                Ok(ReattachOutcome::EngineGone { reason })
            }
            Disposition::Uncertain(reason) => {
                crate::probe!(
                    class = "training.reattach.uncertain",
                    job = %job,
                    pid = work.engine.pid as u64,
                    reason = reason.as_str(),
                    "a previous core's run may still be training: its binding and lease stay, serving stays off the engine, and it is NOT started again"
                );
                Ok(ReattachOutcome::Uncertain { reason })
            }
            Disposition::Attach => {
                let Some(spec) = work.job_spec.clone() else {
                    return Ok(ReattachOutcome::Uncertain { reason: "the binding carries no job_spec".into() });
                };
                let lease = self.resident_leases.lock().ok().and_then(|mut leases| leases.remove(&job));
                if lease.is_none() {
                    crate::probe!(
                        class = "training.reattach.unleased",
                        job = %job,
                        "re-attached without a reclaimed lease: the run's memory is in use but not on the board"
                    );
                }
                let last = Arc::new(Mutex::new(None));
                let run = EngineRun {
                    shape: spec.shape.clone(),
                    footprints_path: self.footprints.path.clone(),
                    http: self.http.clone(),
                    lane,
                    start: None,
                    out: work.out.clone(),
                    adapter_path: spec.adapter_path.clone(),
                    job_dir: spec.job_dir.clone(),
                    epochs: spec.epochs,
                    last: last.clone(),
                    holds: self.holds.clone(),
                    job,
                    hold_store: self.hold_store.clone(),
                    residency: Some((store, work)),
                    _lease: lease,
                };
                let finish = finish_run(job, spec, self.footprints.path.clone(), last);
                let handle = self.jobs.prepare(job, move |_progress| async move {
                    Ok(PreparedJob { execution: Execution::InPlace(Box::new(run)), finish })
                });
                crate::probe!(
                    class = "training.reattach.attached",
                    job = %job,
                    "a run a previous core started is still this job's in its engine: watched again under the same id, never POSTed again"
                );
                Ok(ReattachOutcome::Attached { handle })
            }
        }
    }
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
            trains_on_resident_weights: true,
        }
    }

    fn serves_base(&self, base_model: &str) -> bool {
        (self.lane)(base_model).is_some()
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
        let LaneChoice { url: lane, window: served_window, engine } = (self.lane)(&request.base_model).ok_or_else(|| {
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
        if let Some(parent) = &request.parent {
            // A fork trains from the base today: the engine's /train has no warm start from
            // an existing adapter (the fork's init file went in #19). The lineage is still
            // recorded on the child's signature; the weights are not inherited. The row
            // that says so is this probe, until the engine takes `init_adapter`.
            crate::probe!(
                class = "genome.fork.cold_start",
                persona = %request.persona_id,
                trait_kind = %request.trait_kind,
                parent = %parent,
                job = %id,
                "a fork trains from the base: the engine has no warm start from the parent yet; lineage recorded, weights not inherited"
            );
        }
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
            max_slowdown_ppm = u64::from(TRAINING_MAX_SLOWDOWN_PPM),
            "the training window: the matched lane's served per-slot window as the CEILING (the engine sizes the context to the longest example, fork #30), rounded to the engine's 256 granularity; the request's length never decides it; and the bound on her decode slowdown while training runs"
        );
        // 0 blocks is no depth at all: every block, as omitted (the engine refuses 0 at parse)
        let depth = lora.top_layers.filter(|&k| k > 0);
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
            max_slowdown_ppm: TRAINING_MAX_SLOWDOWN_PPM,
            fit: "middle",
            exact: false,
            walk_host_budget_mib: None,
            chunk: None,
            recompute: true,
        };
        // THE EXACT WALK ONLY WITH A KNOWN BUDGET: unread memory trains the plain walk
        (body.exact, body.walk_host_budget_mib) =
            walk_request(crate::system_resources::memory_pressure::current_available_bytes());
        if !body.exact {
            crate::probe!(
                class = "training.job.exact_walk_skipped",
                "the host memory monitor has not read the host: the job trains the plain (chunk-bounded) walk rather than an exact walk with no budget"
            );
        }
        // THE CHUNK FITS THE LEASE (card d6dae498): the shape is keyed by the walk it trains
        // and the chunk it trains at, and the chunk is the largest whose measured footprint
        // fits the governed VRAM now; a chunk never measured calibrates. Before this, the
        // engine took 512 for every job and a plain-walk footprint stood in for the exact
        // walk's (the 5090, 2026-10-10: 3418 MiB leased, 12.6 GB needed, refused in 8 s).
        let shape_at = |chunk: u32| Shape {
            model: request.base_model.clone(),
            window,
            rank: lora.rank,
            targets: targets.clone(),
            depth,
            exact: body.exact,
            chunk,
            recompute: body.recompute,
        };
        let governed_free = self.vram_free_for(&format!("genome-train:{id}"));
        let (chunk, measured) = choose_chunk(window, governed_free, |c| self.footprints.get(&shape_at(c)));
        let shape = shape_at(chunk);
        body.chunk = Some(chunk);
        let footprints_path = self.footprints.path.clone();
        let http = self.http.clone();
        let holds = self.holds.clone();
        let hold_store = self.hold_store.clone();
        let residency_store = self.residency_store.clone();
        let base_model = request.base_model.clone();
        let governed = matches!(self.admission, Admission::Governed);
        let job_dir = job_dir_for(&request, id);
        let model_id = format!("{PROVIDER_ID}:{}:{id}", request.trait_kind);
        let epochs = schedule.epochs;
        let spec = EngineRunSpec {
            adapter_path: train_dir.join(&out),
            job_dir,
            epochs,
            val_split: SplitPpm::from(val),
            model_id,
            shape: shape.clone(),
        };
        Ok(self.jobs.prepare(id, move |progress| async move {
            let consumer = format!("genome-train:{id}");
            let mut residency = None;
            let reservation = if governed {
                let daemon = crate::resources::ResourceDaemon::global()
                    .ok_or_else(|| failure("engine training requires the resource governor"))?;
                // A measured shape leases its number; an unmeasured one is a calibration run
                // that leases everything governed and free AT ADMISSION, so nothing grows into
                // it (`TrainingNeed::Calibrate`). `bytes` here is what is free now, for the plan
                // probe and the wait's report; the lease the engine runs under is what was granted.
                let need = match measured {
                    Some(bytes) => crate::forge::training_admission::TrainingNeed::Measured(bytes),
                    None => crate::forge::training_admission::TrainingNeed::Calibrate,
                };
                let bytes = match measured {
                    Some(bytes) => bytes,
                    None => daemon.available_for(&consumer, crate::resources::ResourceKind::Vram),
                };
                crate::probe!(
                    class = "training.job.planned",
                    job = %id,
                    base = shape.model.as_str(),
                    window = shape.window as u64,
                    depth = shape.depth.map_or(0, u64::from), // probe field: 0 = every block
                    exact = shape.exact,
                    chunk = shape.chunk as u64,
                    measured = measured.is_some(),
                    memory_bytes = bytes,
                    "engine training: the measured footprint for this shape, or (unmeasured) all governed \
                     free VRAM for a calibration run"
                );
                let gate = crate::modules::serving_daemon::LifecycleGate::global()
                    .ok_or_else(|| failure("engine training requires the serving lifecycle gate"))?;
                // THE RUN BINDS TO ITS ENGINE INSIDE THE ADMISSION HOLD (SHARED-RESIDENT-LIFECYCLE.md
                // step 1): the lane chosen above must still be the live engine (the same pid and
                // start time), or a relaunch replaced it while this job waited and admission is
                // refused; then the binding is recorded before any relaunch can take the gate.
                let store = residency_store
                    .clone()
                    .ok_or_else(|| failure("engine training requires a home for its engine binding"))?;
                let chosen = engine.ok_or_else(|| failure("the chosen lane has no engine incarnation to bind to"))?;
                let bound = crate::inference::engine_residency::ResidentWork {
                    job: id,
                    out: out.clone(),
                    engine: chosen,
                    base_model: base_model.clone(),
                    created_ms: crate::persona::trace::now_ms(),
                    consumer: consumer.clone(),
                    reserved_bytes: bytes,
                    interrupted: None,
                    job_spec: Some(spec.clone()),
                };
                let (bind_store, mut bind_work) = (store.clone(), bound.clone());
                let bind_job_dir = spec.job_dir.clone();
                let bind = move |granted: u64| -> Result<(), String> {
                    bind_work.reserved_bytes = granted;
                    let now = crate::inference::lane_registry::live_lane()
                        .and_then(|rec| incarnation_of(&rec))
                        .ok_or("no live engine at admission: the lane went away while this job waited")?;
                    if now != chosen {
                        let error = format!(
                            "the lane was replaced while this job waited (pid {} started {} is now pid {} started {}); not training on a different engine",
                            chosen.pid, chosen.started_s, now.pid, now.started_s
                        );
                        // Nothing ran: the examples go back to her bucket and the next dispatch
                        // plans against the new lane (card 0b8de4da).
                        if let Err(e) = write_returnable(&bind_job_dir, &Returnable::LaneReplaced { error: error.clone() }) {
                            crate::probe!(class = "training.job.returnable_unrecorded", error = %e, "the replaced lane could not be recorded; the examples wait for a manual return");
                        }
                        return Err(error);
                    }
                    crate::inference::engine_residency::record(&bind_store, bind_work)
                };
                let reservation = crate::forge::training_admission::wait_for_training_memory_bound(
                    daemon.clone(),
                    &gate,
                    &consumer,
                    need,
                    |available| progress.waiting_for_capacity(bytes, available),
                    bind,
                )
                .await
                .map_err(FineTuningError::Transient)?;
                let granted = reservation.bytes();
                residency = Some((store, crate::inference::engine_residency::ResidentWork { reserved_bytes: granted, ..bound }));
                // the engine refuses a graph over the lease it was GRANTED, before allocating it
                body.memory_budget_mib = Some(granted / (1024 * 1024));
                Some(reservation)
            } else {
                None
            };
            let last = Arc::new(Mutex::new(None));
            let run = EngineRun {
                http,
                lane,
                shape: spec.shape.clone(),
                footprints_path: footprints_path.clone(),
                start: Some(body),
                out: out.clone(),
                adapter_path: spec.adapter_path.clone(),
                job_dir: spec.job_dir.clone(),
                epochs,
                last: last.clone(),
                holds,
                job: id,
                hold_store,
                residency,
                _lease: reservation,
            };
            Ok(PreparedJob {
                execution: Execution::InPlace(Box::new(run)),
                finish: finish_run(id, spec, footprints_path, last),
            })
        }))
    }

    async fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
        self.jobs.poll(handle)
    }

    async fn cancel(&self, handle: &JobHandle) -> Result<(), FineTuningError> {
        self.jobs.cancel(handle)
    }

    async fn reattach(&self, local_id: Uuid) -> Result<ReattachOutcome, FineTuningError> {
        self.reattach_job(local_id).await
    }
}

/// A run's finish: collect its adapter into the job directory, read the engine's receipts, and
/// file the footprint under the geometry that ran. ONE body for a run this core started and one
/// it re-attached to (step 3), because both finish from the same [`EngineRunSpec`].
fn finish_run(
    job: Uuid,
    spec: EngineRunSpec,
    footprints_path: PathBuf,
    last: Arc<Mutex<Option<TrainStatus>>>,
) -> Box<dyn FnOnce(u64) -> Result<TrainingArtifact, String> + Send> {
    Box::new(move |wall_clock_ms| {
        let adapter = move_adapter(&spec.adapter_path, &spec.job_dir)?;
        let status = last
            .lock()
            .ok()
            .and_then(|l| l.clone())
            .ok_or("the engine's final status was never read")?;
        let final_loss = status.epochs.last().map(|e| e.train_loss);
        let final_validation_loss = held_out_loss(&status, f32::from(spec.val_split));
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
            job = %job,
            kept = status.examples.unwrap_or(0), // probe field: 0 = an engine that does not report it
            truncated = status.examples_truncated.unwrap_or(0), // probe field: as above
            skipped = status.examples_skipped.unwrap_or(0), // probe field: as above
            "how her examples met the window: kept whole, fitted by dropping their oldest history, or skipped"
        );
        let adapted = effective_depth(status.layers_adapted, status.n_layer);
        if adapted != spec.shape.depth {
            crate::probe!(
                class = "training.job.depth_differs",
                job = %job,
                asked = spec.shape.depth.map_or(0, u64::from), // probe field: 0 = every block
                adapted = adapted.map_or(0, u64::from), // probe field: 0 = every block
                "the engine adapted a different depth than was asked (an engine without top_layers adapts every block)"
            );
        }
        // The footprint is filed under the geometry that RAN (fork #30 sizes the graph
        // to the data, the served window is only its ceiling): a graph measured at a
        // 15k window must never be leased for a 61k one (Codex on #4498).
        let ran_window = status.window.filter(|w| *w > 0).unwrap_or(spec.shape.window);
        if ran_window != spec.shape.window {
            crate::probe!(
                class = "training.job.window_used",
                job = %job,
                sent = u64::from(spec.shape.window),
                used = u64::from(ran_window),
                "the engine sized its training graph to the data: the footprint is filed under the window that ran"
            );
        }
        // …and under the chunk the engine ran at that window: the engine's rule applied to
        // the chunk the core asked for (`train_chunk_for`), never the asked number as sent.
        let ran_chunk = status.chunk.unwrap_or_else(|| train_chunk_for(ran_window, spec.shape.chunk)); // unwrap_or_else: an engine that reports no chunk ran the mirrored rule
        let measured_shape = Shape { depth: adapted, window: ran_window, chunk: ran_chunk, ..spec.shape.clone() };
        // A depth equal to the model's block count IS full depth (fork #27 refuses one
        // past it): filed under the asked key too, or that request would calibrate on
        // every run (Cormac on #4472).
        let asked_full = matches!((spec.shape.depth, status.n_layer), (Some(k), Some(n)) if k >= n);
        if grown > 0 {
            let store = Footprints { path: footprints_path };
            if asked_full {
                // the asked depth, at the window that ran
                let asked_ran = Shape { window: ran_window, chunk: ran_chunk, ..spec.shape.clone() };
                let _ = store.record(&asked_ran, grown, job); // best effort: the full-depth row below is the one that matters
            }
            if let Err(e) = store.record(&measured_shape, grown, job) {
                crate::probe!(
                    class = "training.job.footprint_unrecorded",
                    job = %job,
                    error = %e,
                    "the measured engine-training footprint could not be recorded; the next run of \
                     this shape calibrates again"
                );
            }
        }
        Ok(TrainingArtifact {
            model_id: spec.model_id.clone(),
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
    })
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

    #[test]
    fn a_refused_job_records_its_shape_and_the_ladder_ends_at_the_smallest_chunk() {
        // what this catches (card 2dfce676): the record the trigger returns a refused job by.
        // It round-trips through the job's directory; a directory without one is no refusal;
        // and a return happens only while a smaller chunk is left, so a job refused even at
        // the smallest is never returned into the same refusal forever.
        let dir = tempfile::tempdir().expect("test: tempdir");
        assert_eq!(read_returnable(dir.path()), None, "no record, no return");
        let refused = Refusal { window: 1536, chunk: 512, graph_bytes: 12_654 << 20, error: "over budget".into() };
        // #4904's legacy refusal.json is still read, as EngineRefused
        std::fs::write(dir.path().join(LEGACY_REFUSAL_FILE), serde_json::to_string(&refused).expect("test: json")).expect("test: legacy");
        assert_eq!(read_returnable(dir.path()), Some(Returnable::EngineRefused(refused.clone())));
        let replaced = Returnable::LaneReplaced { error: "the lane was replaced while this job waited".into() };
        write_returnable(dir.path(), &replaced).expect("test: write");
        assert_eq!(read_returnable(dir.path()), Some(replaced), "returnable.json wins over the legacy record");
        assert!(refusal_has_smaller_rung(&refused), "512 steps down to 256");
        assert!(!refusal_has_smaller_rung(&Refusal { chunk: 256, ..refused.clone() }), "256 is the floor");
        assert!(!refusal_has_smaller_rung(&Refusal { window: 1280, chunk: 256, ..refused }), "a 5x256 window's only chunk is 256");
    }

    // what this catches (card d6dae498; the 5090, 2026-10-10 04:34Z): a plain-walk footprint
    // leased for an exact run, or a footprint measured at one chunk leased for another; the
    // chunk the core keys under differing from the chunk the engine runs; and a lease that
    // never shrinks the chunk when the measured graph does not fit what is free.
    #[test]
    fn the_walk_and_its_chunk_are_the_footprints_shape_and_the_chunk_fits_the_lease() {
        // the engine's rule, mirrored: the largest multiple of 256 dividing the window
        assert_eq!(train_chunk_for(1536, 512), 512);
        assert_eq!(train_chunk_for(1536, 256), 256);
        assert_eq!(train_chunk_for(1280, 512), 256, "1280 = 5 x 256: 512 does not divide it");
        assert_eq!(train_chunk_for(66_560, 512), 512);
        assert_eq!(train_chunk_for(256, 512), 256);
        // the key: a plain walk at the default chunk is the row every older core wrote;
        // the exact walk and any other chunk never land on it
        let plain = Shape { model: "m".into(), window: 1536, rank: 8, targets: "attn_q".into(), depth: None, exact: false, chunk: TRAINING_CHUNK, recompute: false };
        assert_eq!(plain.key(), "m|w1536|r8|attn_q");
        let exact = Shape { exact: true, ..plain.clone() };
        assert_eq!(exact.key(), "m|w1536|r8|attn_q|exact");
        let small = Shape { chunk: 256, ..exact.clone() };
        assert_eq!(small.key(), "m|w1536|r8|attn_q|c256|exact");
        let rebuilt = Shape { recompute: true, ..small.clone() };
        assert_eq!(rebuilt.key(), "m|w1536|r8|attn_q|c256|exact|rc", "a footprint that keeps every layer's activations is never leased for recompute, nor the reverse");
        assert_ne!(exact.key(), plain.key(), "a plain-walk footprint is never leased for the exact walk");
        let one_chunk = Shape { window: 256, chunk: 256, ..plain.clone() };
        assert_eq!(one_chunk.key(), "m|w256|r8|attn_q", "a 256 window's only chunk is the engine's default there: the old key");
        // the chunk: unmeasured calibrates at the default; a measurement that fits leases;
        // one that does not fit halves the chunk and asks that shape; the smallest stands
        let gib = 1024u64 * 1024 * 1024;
        assert_eq!(choose_chunk(1536, 6 * gib, |_| None), (512, None), "never measured: calibrate at the default");
        assert_eq!(choose_chunk(1536, 6 * gib, |c| (c == 512).then_some(3 * gib)), (512, Some(3 * gib)));
        let measured = |c: u32| match c { 512 => Some(12 * gib), 256 => Some(5 * gib), _ => None };
        assert_eq!(choose_chunk(1536, 6 * gib, measured), (256, Some(5 * gib)), "the 5090: 12.6 GB at 512 does not fit 6.2 GB free; 256 does");
        assert_eq!(choose_chunk(1536, 6 * gib, |c| (c == 512).then_some(12 * gib)), (256, None), "512 too big, 256 never measured: calibrate at 256");
        assert_eq!(choose_chunk(1536, 1 * gib, measured), (256, Some(5 * gib)), "nothing fits: the smallest chunk's number stands and the governor waits");
        assert_eq!(choose_chunk(256, 1 * gib, |_| Some(5 * gib)), (256, Some(5 * gib)), "a 256 window has one chunk");
    }
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
            parent: None,
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
                    // the 5090, 2026-10-10 04:34Z: the device budget refused the chunk graph, and the
                    // engine reports the size its preflight measured, the chunk and the window it ran
                    if mode == "refused" {
                        let body = l.body.clone().unwrap_or(Value::Null);
                        return axum::Json(json!({"state": "error", "out": out,
                            "error": "the graph needs 9236.1 MiB more on CUDA0, over the 3418.0 MiB it may add: not allocating it",
                            "graph_mib": 12_654.1, "chunk": body.get("chunk").cloned().unwrap_or(json!(512)), "window": body.get("window").cloned().unwrap_or(Value::Null)}));
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
        t.lane = Box::new(move |_| Some(LaneChoice { url: served.clone(), window: 61_696, engine: None }));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        r.schedule.as_mut().expect("test: schedule").sequence_length = 1024;
        let h = t.create_job(r).await.expect("test: create");
        let _ = wait_terminal(&t, &h).await;
        let body = seen.lock().unwrap().clone().expect("test: /train was posted");
        assert_eq!(body["window"].as_u64(), Some(61_696), "the lane's served window, not the request's 1024");

        for (served, why) in [(0, "an unknown served window is refused, never guessed"), (200, "a window under 256 is refused, never rounded up past serving")] {
            let lane_url = url.clone();
            t.lane = Box::new(move |_| Some(LaneChoice { url: lane_url.clone(), window: served, engine: None }));
            let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
            r.local_artifact_dir = Some(jobs.path().to_path_buf());
            assert!(t.create_job(r).await.is_err(), "{why}");
        }
        server.abort();
    }

    // what this catches (BigMama on #4894; the 5090, 2026-10-10 04:34Z): a refused calibration
    // filing nothing, so every later dispatch calibrates at the same chunk and is refused the
    // same way. The refusal files the graph the engine measured under the chunk it ran; the
    // next dispatch of the shape chooses the next chunk down against what is free.
    #[tokio::test]
    async fn a_refused_run_files_its_graph_and_the_next_dispatch_steps_the_chunk_down() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "refused").await;
        let footprints = jobs.path().join("footprints.json");
        let gib = 1024u64 * 1024 * 1024;
        let mut t = EngineLoraFineTuner::for_test_with_vram(url.clone(), train.path().to_path_buf(), footprints.clone(), 6 * gib);
        t.lane = Box::new(move |_| Some(LaneChoice { url: url.clone(), window: 1536, engine: None }));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r.clone()).await.expect("test: create");
        let TrainingStatus::Failed { error } = wait_terminal(&t, &h).await else {
            panic!("test: the refused run must fail");
        };
        assert!(error.contains("it may add"), "the engine's refusal, by name: {error}");
        assert_eq!(seen.lock().unwrap().clone().expect("test: posted")["chunk"].as_u64(), Some(512), "the first dispatch calibrates at the engine's default chunk");
        let rows: Value = serde_json::from_slice(&std::fs::read(&footprints).expect("test: the refusal filed a footprint")).unwrap();
        let (key, row) = rows.as_object().unwrap().iter().next().expect("test: one row");
        assert_eq!(key, "ggml-org/Qwen3.8-27B-GGUF|w1536|r8|attn_q,attn_v|rc", "filed under the shape and the chunk that ran (512 is 1536's default: no chunk suffix), with recompute");
        assert_eq!(row["bytes"].as_u64(), Some((12_654.1f64 * 1024.0 * 1024.0) as u64), "the graph the preflight measured");
        let h2 = t.create_job(r).await.expect("test: create again");
        let _ = wait_terminal(&t, &h2).await;
        assert_eq!(seen.lock().unwrap().clone().expect("test: posted")["chunk"].as_u64(), Some(256), "12.4 GB at 512 does not fit 6 GB free: the next dispatch asks for 256");
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
        t.lane = Box::new(move |_| Some(LaneChoice { url: url.clone(), window: 61_696, engine: None }));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        // the trigger's dispatch and `return` carry no schedule: the engine runs the default
        r.schedule = None;
        let h = t.create_job(r).await.expect("test: create");
        let TrainingStatus::Completed { .. } = wait_terminal(&t, &h).await else {
            panic!("test: not completed");
        };
        let posted = seen.lock().unwrap().clone().expect("test: posted");
        assert_eq!(posted["window"].as_u64(), Some(61_696), "the served window is sent as the ceiling");
        // what this catches (the 5090, 2026-10-10, job ff186697): every trigger-dispatched gene
        // trained at a second, disagreeing default of 1e-5 and barely moved (-0.2% per epoch)
        assert_eq!(posted["lr"].as_f64(), Some(super::super::native_jobs::DEFAULT_LEARNING_RATE), "a run with no schedule trains at the one default learning rate");
        let rows: Value = serde_json::from_slice(&std::fs::read(&footprints).expect("test: footprint filed")).unwrap();
        let keys: Vec<&String> = rows.as_object().unwrap().keys().collect();
        // …and under the chunk that ran: 61,696 is 241 blocks, prime, so the engine's chunk was
        // 256, not the 512 a 15,360 window would take by default; a 15,360 run at 256 is its own
        // (smaller) graph and must never lease a default-chunk row's number (card d6dae498).
        assert_eq!(keys, vec!["ggml-org/Qwen3.8-27B-GGUF|w15360|r8|attn_q,attn_v|c256|rc"], "filed under the window and the chunk that ran");
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
        // what this catches (fork #36): the bound on her slowdown goes out INSTEAD of the time
        // share; the engine refuses a body carrying both, so sending both fails every run
        assert_eq!(body["max_slowdown_ppm"].as_u64(), Some(u64::from(TRAINING_MAX_SLOWDOWN_PPM)));
        assert!(body.get("share_ppm").is_none(), "the share and the slowdown bound pace the same windows: one is sent");
        // what this catches (fork #47, Fable on #4841): the walk the job asks for follows the
        // monitor; here no monitor runs, so the plain walk, with no unbounded exact walk
        assert_eq!(body["exact"], false, "an unread host trains the plain walk");
        assert!(body.get("walk_host_budget_mib").is_none(), "and sends no budget");
        // what this catches (Kimi's first exact run, the 5090 2026-10-10: refused at chunk 256
        // needing 5.6 GiB more): fork #37's recompute was built and never asked for
        assert_eq!(body["recompute"], true, "every run keeps the layer outputs, not every layer's activations");
        let path = artifact.local_path.expect("test: path");
        assert_eq!(artifact.format, ArtifactFormat::GgufLora);
        assert!(path.starts_with(jobs.path()) && path.is_file(), "adapter in the job dir: {}", path.display());
        assert_eq!(std::fs::read_dir(train.path()).unwrap().count(), 0, "nothing left in engine-train");
        assert_eq!(artifact.metrics.final_loss, Some(2.1));
        assert_eq!(artifact.metrics.trained_tokens, 80);
        server.abort();
    }

    // what this catches: the exact walk's host budget reading the monitor as anything but
    // "half of what is free" (the other half is the serving engine's file cache), and an unread
    // monitor (0 / None) inventing a budget instead of sending none
    #[test]
    fn the_exact_walk_keeps_half_the_free_host_memory() {
        assert_eq!(super::exact_walk_host_budget_mib(Some(36 << 30)), Some(18 << 10));
        // regression for Fable on #4841: tighter memory must bound the walk MORE, never unbound it
        assert_eq!(super::exact_walk_host_budget_mib(Some(1 << 20)), Some(1), "the floor, which the engine refuses by name");
        assert_eq!(super::exact_walk_host_budget_mib(Some(0)), Some(1));
        assert_eq!(super::exact_walk_host_budget_mib(None), None, "unread: no exact walk (the caller sends exact: false)");
        // both arms of the request the job sends
        assert_eq!(super::walk_request(Some(36 << 30)), (true, Some(18 << 10)), "a read host: the exact walk, bounded");
        assert_eq!(super::walk_request(None), (false, None), "an unread host: the plain walk");
    }

    // what this catches (SHARED-RESIDENT-LIFECYCLE.md step 3, Codex and Cormac on the plan):
    // re-attach deciding on anything but positive evidence. Only the SAME live incarnation
    // answering with THIS run attaches; a verifiably dead one (including a new engine that took
    // the port: the record's own start time no longer matches) is gone; an engine answering
    // with another run, not answering, or unreadable, is uncertain and never resumes.
    #[test]
    fn re_attach_decides_only_on_positive_evidence() {
        use crate::inference::engine_residency::{EngineIncarnation, Liveness, ProcessProbe};
        let ours = |out: &str| -> TrainStatus {
            serde_json::from_value(json!({"state": "running", "out": out})).expect("test: status")
        };
        let (a, d, u) = (Liveness::Alive, Liveness::Dead, Liveness::Unknown);
        assert_eq!(reattach_disposition(a, true, Some(Ok(ours("j.gguf"))), a, "j.gguf"), Disposition::Attach);
        for (before, spec, answer, after, why) in [
            (a, true, Some(Ok(ours("other.gguf"))), a, "another run is not proof this one ended"),
            (a, true, Some(Err("refused".to_string())), a, "no answer is not proof"),
            (a, false, None, a, "a binding from before job_spec cannot be watched again"),
            (u, true, None, u, "unreadable liveness"),
            (a, true, Some(Ok(ours("j.gguf"))), u, "the answer is this engine's only if it is still the same process"),
        ] {
            assert!(matches!(reattach_disposition(before, spec, answer, after, "j.gguf"), Disposition::Uncertain(_)), "{why}");
        }
        assert!(matches!(reattach_disposition(d, true, None, d, "j.gguf"), Disposition::Gone(_)));
        assert!(matches!(reattach_disposition(a, true, Some(Ok(ours("j.gguf"))), d, "j.gguf"), Disposition::Gone(_)));
        // Cormac's (c): after a restart a NEW engine holds the same port; the old record's own
        // incarnation reads dead, so the new engine is never asked and never attached to.
        let old = EngineIncarnation { pid: 4242, started_s: 1_000, port: 58057 };
        assert_eq!(old.liveness_with(|_| ProcessProbe::Present(2_000)), Liveness::Dead);
        assert!(!ReattachOutcome::Uncertain { reason: String::new() }.permits_resume());
        assert!(ReattachOutcome::EngineGone { reason: String::new() }.permits_resume());
    }

    // what this catches (step 3's whole point): a core-only restart losing a run that is still
    // training, or starting it AGAIN. A successor tuner that never created the job finds its
    // binding, watches the run in the engine under the SAME id WITHOUT a POST, finishes it into
    // the job directory from the recorded spec, and releases the binding once the end is settled.
    #[tokio::test]
    async fn a_successor_core_reattaches_a_live_run_under_the_same_id_without_posting_it() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let home = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "normal").await;
        let job = Uuid::new_v4();
        let out = format!("{job}.gguf");
        // the previous core's POST: the run is in the engine before this core exists
        reqwest::Client::new().post(format!("{url}/train")).json(&json!({"out": out})).send().await.expect("test: post");
        *seen.lock().unwrap() = None;
        let port: u16 = url.rsplit(':').next().and_then(|p| p.parse().ok()).expect("test: port");
        let engine = crate::inference::engine_residency::EngineIncarnation::of(std::process::id(), port).expect("test: this process has a start time");
        let store = crate::inference::engine_residency::store_path(home.path());
        let job_dir = jobs.path().join(job.to_string());
        let spec = EngineRunSpec {
            adapter_path: train.path().join(&out),
            job_dir: job_dir.clone(),
            epochs: 2,
            val_split: SplitPpm::from(0.1),
            model_id: format!("{PROVIDER_ID}:code:{job}"),
            shape: Shape { model: "m".into(), window: 256, rank: 8, targets: "attn_q,attn_v".into(), depth: None, exact: false, chunk: TRAINING_CHUNK, recompute: false },
        };
        let bound = crate::inference::engine_residency::ResidentWork {
            job,
            out: out.clone(),
            engine,
            base_model: "m".into(),
            created_ms: 1,
            consumer: format!("genome-train:{job}"),
            reserved_bytes: 0,
            interrupted: None,
            job_spec: Some(spec),
        };
        crate::inference::engine_residency::record(&store, bound).expect("test: record");
        let mut successor = EngineLoraFineTuner::for_test(url, train.path().to_path_buf(), jobs.path().join("footprints.json"));
        successor.residency_store = Some(store.clone());

        let ReattachOutcome::Attached { handle } = successor.reattach(job).await.expect("test: reattach") else {
            panic!("test: a live run of this job must re-attach");
        };
        assert_eq!(handle.local_id, job, "the same job, not a new one");
        let TrainingStatus::Completed { artifact } = wait_terminal(&successor, &handle).await else {
            panic!("test: the re-attached run must finish");
        };
        assert!(seen.lock().unwrap().is_none(), "a re-attached run is never POSTed again");
        let path = artifact.local_path.expect("test: path");
        assert!(path.starts_with(&job_dir) && path.is_file(), "finished into the recorded job dir: {}", path.display());
        assert_eq!(artifact.metrics.final_validation_loss, Some(2.4), "the recorded split decides the held-out loss");
        assert!(
            crate::inference::engine_residency::find(&store, job).expect("test: read").is_none(),
            "the settled end released the binding"
        );
        assert!(matches!(successor.reattach(job).await.expect("test: again"), ReattachOutcome::NotResident));
        server.abort();
    }

    // what this catches (Codex on #4472): the finish path filing a measured graph under the
    // depth that was ASKED rather than the depth the engine ADAPTED. An engine without
    // top_layers ignores it and measures a full-depth graph; filed under |d8, that number
    // would later be leased for a K=8 run as if it were one. And the gene must carry the
    // depth the engine reported, never the request's.
    #[tokio::test]
    async fn a_finished_run_files_its_graph_and_its_gene_under_the_depth_the_engine_adapted() {
        for (mode, filed, gene) in [("normal", "ggml-org/Qwen3.8-27B-GGUF|w256|r8|attn_q,attn_v|rc", None), ("depth", "ggml-org/Qwen3.8-27B-GGUF|w256|r8|attn_q,attn_v|d8|rc", Some(8))] {
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
    // job; the run steers to it every tick; releasing resumes and
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
                assert!(store::live_on(&holds, h.local_id, store::now_ms()).await.unwrap().is_empty(), "the job's end took its pause with it");
            }
            server.abort();
        }
        let left = store::live_on(&holds, bystander.job, store::now_ms()).await.unwrap();
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
        let s = Shape { model: "m".into(), window: 256, rank: 8, targets: "attn_q".into(), depth: None, exact: false, chunk: TRAINING_CHUNK, recompute: false };
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
        let full = Shape { model: "m".into(), window: 1536, rank: 8, targets: "attn_q".into(), depth: None, exact: false, chunk: TRAINING_CHUNK, recompute: false };
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

    // what this catches (Codex on attempt 2): a run with no held-out split reporting a
    // validation loss. The engine's empty eval result is 0.0, and read as a validation loss it
    // claims a perfect held-out score. With fork #31 the count decides; before it, a run that
    // asked for no split has no held-out loss, and a null eval loss is none either way.
    #[test]
    fn a_run_with_no_held_out_tokens_has_no_validation_loss() {
        let st = |v: Value| serde_json::from_value::<TrainStatus>(v).expect("test: status");
        let old = |eval| st(json!({"state": "done", "epochs": [{"train_loss": 2.0, "eval_loss": eval}]}));
        assert_eq!(held_out_loss(&old(0.0), 0.0), None, "an old engine, no split asked: its 0.0 is no measurement");
        assert_eq!(held_out_loss(&old(2.4), 0.1), Some(2.4), "an old engine, a split asked: believed");
        let new = |eval: Value, n| st(json!({"state": "done", "eval_trainable_tokens": n, "epochs": [{"train_loss": 2.0, "eval_loss": eval}]}));
        assert_eq!(held_out_loss(&new(Value::Null, 0), 0.1), None, "a split that rounded to no held-out tokens");
        assert_eq!(held_out_loss(&new(json!(2.4), 512), 0.0), Some(2.4), "the engine's count decides, not the request");
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
