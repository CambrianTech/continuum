use super::*;

/// What the trainer is governed to have free for `consumer` in `memory` (its lane's pool): the
/// governor's budget net of every reservation. ONE reader, for the plan here and for the
/// trigger deciding whether a capacity-parked job may re-plan (card de9e1342), so the two can
/// never disagree.
pub fn governed_training_free(consumer: &str, memory: crate::resources::ResourceKind) -> u64 {
    crate::resources::ResourceDaemon::global()
        .map_or(0, |d| d.available_for(consumer, memory)) // map_or: a governed host whose governor is gone has nothing free to lease (the governed branch refuses below)
}

pub const PROVIDER_ID: &str = "engine-local";

/// The provider id of the in-engine trainer on lanes stood up for the run
/// ([`super::engine_lane::EngineLane`]). Its artifacts are the same kind as `engine-local`'s
/// (an engine LoRA, ids `engine-local:<trait>:<job>`); only where it trains differs.
pub const LANE_PROVIDER_ID: &str = "engine-lane";

/// Whether `provider` trains in our engine (on the resident lane or one stood up for the run):
/// the jobs that obey the in-engine holds and pause where they stand.
pub fn trains_in_engine(provider: &str) -> bool {
    provider == PROVIDER_ID || provider == LANE_PROVIDER_ID
}

/// How often the run is polled. The engine answers GET /train from memory; this is the
/// cadence of progress and of the VRAM peak sample, not a timeout.
// derived-or-floor: a floor — far below an epoch, far above a spin.
pub(super) const POLL: Duration = Duration::from_secs(2);

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
pub(super) struct TrainingHolds(pub(super) Arc<watch::Sender<BTreeMap<u64, (String, String)>>>);

pub(super) static NODE_HOLDS: std::sync::LazyLock<TrainingHolds> = std::sync::LazyLock::new(TrainingHolds::new);

pub(super) static NEXT_HOLD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The scope of a hold on every in-engine run of this node.
pub(super) const EVERY_LANE: &str = "*";

/// A lane's root as a hold scope: one lane, one key, however its url was spelled (Cormac on
/// #4485: a trailing slash made a hold that held nothing).
pub(super) fn lane_scope(lane: &str) -> String {
    lane.trim_end_matches('/').to_string()
}

impl TrainingHolds {
    pub(super) fn new() -> Self {
        Self(Arc::new(watch::Sender::new(BTreeMap::new())))
    }

    /// This node's holds: the set every production tuner obeys.
    pub(super) fn node() -> &'static TrainingHolds {
        &NODE_HOLDS
    }

    /// Pause every in-engine run under this set until the hold drops; every call is its own hold.
    pub(super) fn hold(&self, reason: &str) -> TrainingHold {
        self.insert(EVERY_LANE.to_string(), reason)
    }

    pub(super) fn insert(&self, scope: String, reason: &str) -> TrainingHold {
        let token = NEXT_HOLD.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.0.send_modify(|holds| {
            holds.insert(token, (scope, reason.to_string()));
        });
        TrainingHold { holds: self.clone(), token }
    }

    pub(super) fn subscribe(&self) -> watch::Receiver<BTreeMap<u64, (String, String)>> {
        self.0.subscribe()
    }
}

/// A standing hold on in-engine training; dropping it releases this hold and no other.
pub struct TrainingHold {
    pub(super) holds: TrainingHolds,
    pub(super) token: u64,
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

/// The reasons holding the run on `lane`: every node-wide hold and every hold on that lane.
pub(super) fn holds_on(holds: &BTreeMap<u64, (String, String)>, lane: &str) -> Vec<String> {
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
pub(super) const LANE_SILENCE: Duration = Duration::from_secs(90);

pub(super) fn failure(error: impl std::fmt::Display) -> FineTuningError {
    FineTuningError::LocalTrainerFailed(error.to_string())
}

/// HF module names (what [`super::LoRAHyperparams::target_modules`] carries, and what the
/// PyTorch/MLX trainers take) to the GGUF tensor names the engine's adapter writer targets.
/// An unknown name is refused: silently dropping a target trains a different adapter.
pub(super) fn gguf_targets(modules: &[String]) -> Result<String, String> {
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
pub(super) struct Shape {
    pub(super) model: String,
    pub(super) window: u32,
    pub(super) rank: u32,
    pub(super) targets: String,
    /// The blocks adapted when fewer than all (`top_layers`, fork #27); `None` = every block.
    /// Depth drives the graph (~linear), so a reduced-depth run is its own shape.
    #[serde(default)]
    pub(super) depth: Option<u32>,
    /// The exact walk (fork #47) is its own shape: its chunk graph carries GRAD leaves for the
    /// cached K/V over the gradient horizon, several times the plain walk's. Measured on the
    /// 5090, 2026-10-10 04:34Z (card d6dae498): a plain-walk footprint of 3418 MiB was leased
    /// for Kimi's first exact run, whose chunk graph needed 9236 MiB more, and the job refused.
    #[serde(default)]
    pub(super) exact: bool,
    /// The training chunk (one batch, one ubatch): the chunk graph's activations scale with
    /// it, so a shape at a smaller chunk is a smaller footprint, and the chunk the core sends
    /// is the one the footprint was measured at.
    #[serde(default = "default_chunk")]
    pub(super) chunk: u32,
    /// Per-layer recompute (fork #37) is its own shape: the backward pass keeps only the layer
    /// outputs and rebuilds the rest, so a chunk's graph is a fraction of the one that keeps
    /// every layer's activations. Measured on the 5090, 2026-10-10 08:32Z (job 888ce9c6): the
    /// exact walk at chunk 256 without it needed 5.6 GiB more than the lease under every
    /// gradient horizon from 1155 down to 256. A footprint measured without recompute is never
    /// leased for a run with it, nor the reverse.
    #[serde(default)]
    pub(super) recompute: bool,
}

/// A job the engine REFUSED before training: the shape its preflight measured, recorded in the
/// job's own directory (`refusal.json`). The trigger reads it to hand the job's examples back
/// to her bucket so the next dispatch chooses a smaller chunk (card 2dfce676). Without it a
/// refused job's examples sat in its directory until a human returned them, and the chunk
/// ladder #4894 builds never walked on its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Refusal {
    pub window: u32,
    pub chunk: u32,
    /// The blocks the refused run adapted (`None`: every block), and the model's block count as
    /// the engine reported it: whether a shallower depth is left to step to. Both `None` on a
    /// record from before them.
    #[serde(default)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub layers: Option<u32>,
    pub graph_bytes: u64,
    pub error: String,
}

/// Why a job ended WITHOUT training in a way that hands its examples back to her bucket
/// (card 0b8de4da). Recorded in the job's own directory (`returnable.json`) where it
/// happens; the trigger's tick reads it. A job that failed any other way records nothing
/// and is not returned automatically.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Returnable {
    /// The engine refused the run's graph: the next dispatch steps the chunk down.
    EngineRefused(Refusal),
    /// The lane this run was bound to was replaced while it waited at admission (a
    /// re-home or relaunch): nothing ran, and the next dispatch plans against the new lane.
    /// The 5090, 2026-10-10: a7893e09's 18 examples stranded this way until a human
    /// cancelled and returned it.
    LaneReplaced { error: String },
}

pub(crate) const RETURNABLE_FILE: &str = "returnable.json";

pub(crate) const PARTITION_FILE: &str = "partition.json";

pub(super) fn write_partition(job_dir: &Path, partition: &Partition) -> Result<(), String> {
    std::fs::create_dir_all(job_dir).map_err(|e| format!("{}: {e}", job_dir.display()))?;
    let path = job_dir.join(PARTITION_FILE);
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_vec(partition).map_err(|e| e.to_string())?; // file on disk: the job dir's partition.json, read back by `return` after a restart
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    continuum_host::durable::replace(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
}

/// What `job_dir` records about its partition. `Ok(None)` only when the file is confirmed absent
/// (a job from before the record: every example it holds is returnable, as before). A file that
/// can't be read or isn't a partition is an ERROR, never read as "nothing parked" (Astra on #5009).
pub fn read_partition(job_dir: &Path) -> Result<Option<Partition>, String> {
    let path = job_dir.join(PARTITION_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    serde_json::from_slice(&bytes).map(Some).map_err(|e| format!("{}: not a partition: {e}", path.display()))
}

/// Examples per `/train/measure` call: ONE, so a slow example can never fail a batch's wait.
/// MEASURED on the 5090, 2026-10-11 07:20Z: the engine renders and tokenizes a lived turn
/// (~48k tokens, 36 tools) at its window, and again with its history dropped, in ~5 s each
/// (1 example 4.8 s, 4 in 20.9 s). The first release batched 16 under a 60 s wait, on a guess of
/// milliseconds each, so every dispatch's measure timed out at 60 s and failed its job (jobs
/// f2a2f89e and cc1583a2, with their examples left untrained and returnable). One example per call
/// costs one more request each, and keeps every wait proportional to one example.
pub(super) const MEASURE_BATCH: usize = 1;

/// The wait for one example's measurement. derived-or-floor: a floor, ~24x the measured ~5 s for
/// a 48k-token lived turn, so an example twice that long still answers; short enough that a dead
/// lane fails the job within minutes, not hours.
pub(super) const MEASURE_TIMEOUT: Duration = Duration::from_secs(120);

/// The floor on the wait for `/train` to accept a run: what the core waited before this was
/// derived, kept so a small batch on a fast engine is never given less.
pub(super) const POST_WAIT_FLOOR: Duration = Duration::from_secs(120);

/// PURE: how long the POST of a run may take. `/train` renders and tokenizes every example it
/// is sent before it answers, the same work `/train/measure` just did for each, so the wait is
/// the measured per-example render time times the examples sent, doubled for headroom, at least
/// [`POST_WAIT_FLOOR`]. MEASURED on the 5090, 2026-10-11 10:06Z: 16 lived turns measured in
/// 205 s (~13 s each beside serving), and the 11 sent outlasted the old fixed 120 s wait, so the
/// core failed job 97e081e2 while the engine trained it, unadoptable (card 93ffc555).
pub(super) fn post_wait_for(render_per_example: Duration, sent: usize, floor: Duration) -> Duration {
    render_per_example.mul_f64(sent as f64 * 2.0).max(floor)
}

/// What starting a run in the engine came to.
pub(super) enum Begin {
    /// accepted (or re-attached): the watch loop takes it
    Running(Box<EngineRun>),
    /// the engine provably never started it: its documented refusal (see
    /// [`is_documented_train_refusal`])
    NeverStarted(InPlaceEnd),
    /// it ended without starting, and nothing proves the engine won't run it: a lost POST
    /// (it may still be queued), or any other non-2xx. Its binding settles only the ordinary way.
    Unproven(InPlaceEnd),
}

/// The fork's documented pre-acceptance refusal of `POST /train`: HTTP 400 with an
/// `invalid_request_error` body. `server_trainer::start` (tools/server/server-train.cpp) is
/// synchronous and answers `ok: false` only before claiming the run, or after releasing its
/// claim (`running = false`). The server then maps it to exactly this, so when the answer
/// arrives nothing of this run is running or will run.
pub(super) fn is_documented_train_refusal(status: reqwest::StatusCode, body: &str) -> bool {
    status == reqwest::StatusCode::BAD_REQUEST
        && serde_json::from_str::<Value>(body)
            .ok()
            .is_some_and(|b| b.pointer("/error/type").and_then(Value::as_str) == Some("invalid_request_error"))
}

/// What a lost POST /train turns out to have been, from the engine's status afterwards.
#[derive(Debug, PartialEq)]
pub(super) enum LostPost {
    /// the engine has this job's run, in any state but cancelled (an error included): the watch
    /// loop takes it, and handles its end as it would any other
    Watch,
    /// no status, or the engine shows another run or none: it never started there
    NeverStarted,
}

/// PURE: [`LostPost`] for `status` (None: unreadable), where `ours` says whether a status is
/// this job's run (its `out`).
pub(super) fn after_lost_post(status: Option<&TrainStatus>, ours: impl Fn(&TrainStatus) -> bool) -> LostPost {
    match status {
        Some(s) if ours(s) && s.state != TrainState::Cancelled => LostPost::Watch,
        _ => LostPost::NeverStarted,
    }
}

/// Why the engine's measure of her examples failed.
#[derive(Debug, thiserror::Error)]
pub(super) enum MeasureError {
    /// The engine is not ready to answer: still loading its model (HTTP 503), or not yet
    /// listening. Nothing about her examples is known, so nothing is decided from it (the
    /// 5090 2026-10-28 20:16Z: a re-dispatch 2 ms after a handoff failed its job on
    /// "503 Loading model" and stranded the examples it had just been returned).
    #[error("{0}")]
    NotReady(String),
    /// The engine answered, and the measure failed.
    #[error("{0}")]
    Failed(String),
}

/// Every example measured at `window` by the engine's own fit rule, by request index.
pub(super) async fn measure_examples(
    http: &reqwest::Client,
    lane: &str,
    examples: &[EngineExample],
    fit: Fit,
    window: u32,
) -> Result<Vec<MeasuredExample>, MeasureError> {
    use MeasureError::{Failed, NotReady};
    let mut rows = Vec::with_capacity(examples.len());
    for (batch_index, batch) in examples.chunks(MEASURE_BATCH).enumerate() {
        let r = http
            .post(format!("{lane}/train/measure"))
            .json(&MeasureRequest { examples: batch.to_vec(), window, fit: Some(fit) })
            .timeout(MEASURE_TIMEOUT)
            .send()
            .await
            .map_err(|e| {
                let why = format!("POST /train/measure on {lane}: {e}");
                if e.is_connect() { NotReady(why) } else { Failed(why) }
            })?;
        let code = r.status();
        if !code.is_success() {
            let why = r.text().await.unwrap_or_default(); // unwrap_or_default: an unreadable refusal body still fails the measure
            let why = format!("POST /train/measure on {lane}: HTTP {code}: {why}");
            return Err(if code == reqwest::StatusCode::SERVICE_UNAVAILABLE { NotReady(why) } else { Failed(why) });
        }
        let reply: MeasureReply = r.json().await.map_err(|e| Failed(format!("POST /train/measure on {lane}: {e}")))?;
        if reply.examples.len() != batch.len() {
            return Err(Failed(format!(
                "POST /train/measure on {lane}: {} rows for {} examples",
                reply.examples.len(),
                batch.len()
            )));
        }
        rows.extend(reply.examples.into_iter().map(|mut row| {
            row.index += batch_index * MEASURE_BATCH;
            row
        }));
    }
    Ok(rows)
}

/// #4904's record, before `Returnable` (card 2dfce676): still read, as `EngineRefused`.
pub(super) const LEGACY_REFUSAL_FILE: &str = "refusal.json";

pub(super) fn write_returnable(job_dir: &Path, returnable: &Returnable) -> Result<(), String> {
    std::fs::create_dir_all(job_dir).map_err(|e| format!("{}: {e}", job_dir.display()))?;
    let path = job_dir.join(RETURNABLE_FILE);
    let text = serde_json::to_string(returnable).map_err(|e| e.to_string())?; // file on disk: the job dir's returnable.json, read back by the trigger after a restart
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// What `job_dir` records about a return, if anything: `returnable.json`, else #4904's
/// `refusal.json` as `EngineRefused`. A missing or unreadable record is none; a caller that must
/// tell the two apart reads [`read_returnable_strict`].
pub fn read_returnable(job_dir: &Path) -> Option<Returnable> {
    read_returnable_strict(job_dir).ok().flatten()
}

/// [`read_returnable`], telling a confirmed absence (`Ok(None)`: neither file exists) from a record
/// that exists and cannot be read or parsed, or a directory that cannot be checked (`Err`).
pub fn read_returnable_strict(job_dir: &Path) -> Result<Option<Returnable>, String> {
    let read = |name: &str| -> Result<Option<Vec<u8>>, String> {
        let path = job_dir.join(name);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    };
    if let Some(bytes) = read(RETURNABLE_FILE)? {
        return serde_json::from_slice(&bytes).map(Some).map_err(|e| format!("{}: not a returnable record: {e}", job_dir.join(RETURNABLE_FILE).display()));
    }
    match read(LEGACY_REFUSAL_FILE)? {
        Some(bytes) => serde_json::from_slice::<Refusal>(&bytes)
            .map(|legacy| Some(Returnable::EngineRefused(legacy)))
            .map_err(|e| format!("{}: not a refusal record: {e}", job_dir.join(LEGACY_REFUSAL_FILE).display())),
        None => Ok(None),
    }
}

/// PURE: whether a refused job has a smaller rung to step down to: a smaller chunk, a shallower
/// depth (the engine reported its block count, and the run adapted more than one block), or a
/// smaller window (the refusal filed its graph as a lower bound, so the next plan steps past it
/// instead of repeating it). At the smallest chunk, depth AND window a return would only be
/// refused again, forever, so the ladder ends there (the loop guard
/// `genome/training-trigger/return`'s own doc warns about).
pub fn refusal_has_smaller_rung(refusal: &Refusal) -> bool {
    let smaller_chunk = train_chunk_for(refusal.window, refusal.chunk / 2) < refusal.chunk && refusal.chunk / 2 >= TRAINING_CHUNK_MIN;
    let shallower = refusal.layers.is_some_and(|n| !shallower_depths(refusal.depth, n).is_empty());
    smaller_chunk || shallower || refusal.window > TRAINING_WINDOW_MIN
}

impl Shape {
    /// Full depth keeps the key every row before depth existed was written under (those
    /// rows were all full-depth runs); a reduced depth adds `|dK`, so a reduced-depth lookup
    /// can never land on a full-depth row, nor a full-depth lookup on a reduced one.
    pub(super) fn key(&self) -> String {
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
pub struct SplitPpm(pub(super) u32);

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
    pub(super) adapter_path: PathBuf,
    /// Where the finished adapter is collected (the job's own directory).
    pub(super) job_dir: PathBuf,
    pub(super) epochs: u32,
    /// The held-out share the run asked for: whether an eval loss is believed (`held_out_loss`).
    pub(super) val_split: SplitPpm,
    /// The artifact's model id (`engine-local:<trait>:<job>`).
    pub(super) model_id: String,
    /// The footprint key the run was planned under, for filing what it measured.
    pub(super) shape: Shape,
    /// Which walk the run trains by, and why: carried to the finished artifact's metrics.
    #[serde(default)]
    pub(super) walk: Option<super::super::types::TrainingWalk>,
    /// The characters of example payload the run trains (`training_rate::example_chars`), so its
    /// finish files the measured wall seconds per character the next dispatch is sized by.
    #[serde(default)]
    pub(super) corpus_chars: u64,
    /// The weights file the run's lane was launched on, carried to the finished artifact's
    /// metrics so a re-attached run names the same base (card 3fd98325).
    #[serde(default)]
    pub(super) trained_on: Option<crate::inference::weights_stamp::WeightsStamp>,
    /// Where her training demand is recorded ([`crate::genome::fine_tuning::training_demand`]):
    /// a successful finish clears the examples the run trained. `None` on a spec persisted before
    /// demand existed, when no example of its run could have been recorded as demand.
    #[serde(default)]
    pub(super) demand_store: Option<PathBuf>,
}

/// The depth a finished run actually adapted, as the shape it is recorded under: the engine's
/// own `layers_adapted` against its `n_layer`. An engine that reports neither predates
/// `top_layers` and adapted every block, whatever was asked.
pub(super) fn effective_depth(layers_adapted: Option<u32>, n_layer: Option<u32>) -> Option<u32> {
    match (layers_adapted, n_layer) {
        (Some(k), Some(n)) if k < n => Some(k),
        _ => None,
    }
}

/// Measured engine-training footprints, one row per shape, in one small JSON file (bounded by
/// the number of distinct shapes ever run, not by runs).
#[derive(Clone)]
pub(super) struct Footprints {
    /// The store's file; `None` with no continuum home, where nothing persists and every shape
    /// is measured again (a calibration run), never written into the working directory.
    pub(super) path: Option<PathBuf>,
    /// The engine build this view reads and writes for (`LaneChoice::build`). Lower bounds and
    /// leases are evidence only about the engine that measured them; whole-run rows of any
    /// engine still fit the model, which is a prediction. `None` reads every row (no identity
    /// to tell engines apart) (Codex on #4988).
    pub(super) engine: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FootprintRow {
    /// The build of the engine it was measured on (`LaneChoice::build`). `None` on rows from
    /// before it and on pre-slot installs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) engine: Option<String>,
    pub(super) bytes: u64,
    pub(super) measured_at_ms: u64,
    pub(super) job: String,
    /// A RUN's row is the graph AND its context (fork #52's context_mib), the whole of what a lease
    /// must cover and what the model is fitted to. A run row from before it is the graph alone: it
    /// under-states a lease by the context's K/V, and mixed into one fit with whole-run rows it bends
    /// the model, so it is kept as a bound but never leased or fitted (Cormac on #4939). A refusal
    /// row ([`Provenance::Refusal`]) is never whole. Stored under its first name, so every row on a
    /// node reads the same.
    #[serde(default, rename = "includesContext")]
    pub(super) whole_run: bool,
    /// The model's block count as the measuring engine reported it (`n_layer`): what a plan's depth
    /// rungs are halved from. Merged on every write whatever the bytes ([`Footprints::upsert`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) layers: Option<u32>,
    /// What measured it. `None` only on rows from before it, classified once by
    /// [`Footprints::classify_legacy`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) provenance: Option<Provenance>,
}

/// What a footprint row's number is (card 0d21e644).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Provenance {
    /// A run that trained: its measured footprint.
    Run,
    /// The engine's preflight refused the shape: the graph it sized, a LOWER BOUND that never ran.
    /// Filed under its own key ([`REFUSED`]) so it neither replaces nor hides a run's row, and it
    /// never trains the fit (Codex's gate on #5548).
    Refusal,
}

/// What [`Footprints::classify_legacy`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Classified {
    /// Legacy rows moved to their refusal keys.
    pub(super) refusals: usize,
    /// Why each row left unclassified could not be classified.
    pub(super) unknown: Vec<String>,
}

/// What a legacy row's own job directory says about it.
enum Evidence {
    Refused,
    /// Every place its job could be was read, and none records a refusal of its size.
    NotRefused,
    Unknown(String),
}

/// The evidence `roots` hold for `row`'s job, in every directory it can have
/// ([`job_dirs_strict`](crate::genome::fine_tuning::job_dir::job_dirs_strict)): any listing,
/// check or record that errs makes it unknown, never "not refused".
fn refusal_evidence(row: &FootprintRow, roots: &[PathBuf]) -> Evidence {
    let Ok(job) = Uuid::parse_str(&row.job) else {
        return Evidence::NotRefused; // no job id (a test's or a hand's row): no record can name it
    };
    let mut unknown = None;
    for root in roots {
        let dirs = match crate::genome::fine_tuning::job_dir::job_dirs_strict(root, job) {
            Ok(dirs) => dirs,
            Err(why) => {
                unknown = Some(why);
                continue;
            }
        };
        for dir in dirs {
            match read_returnable_strict(&dir) {
                Ok(Some(Returnable::EngineRefused(refusal))) if refusal.graph_bytes == row.bytes => return Evidence::Refused,
                Ok(_) => {}
                Err(why) => unknown = Some(why),
            }
        }
    }
    unknown.map_or(Evidence::NotRefused, Evidence::Unknown)
}


/// The suffix a refusal's row is keyed under, beside the run row of the same shape.
const REFUSED: &str = "|refused";

impl Footprints {
    pub(super) fn at(path: PathBuf) -> Self {
        Footprints { path: Some(path), engine: None }
    }
    /// No store: with no continuum home there is nowhere to keep a measurement.
    pub(super) fn unrecorded() -> Self {
        Footprints { path: None, engine: None }
    }
    /// This store, as evidence about the engine `build`.
    pub(super) fn for_engine(&self, build: Option<String>) -> Self {
        Footprints { path: self.path.clone(), engine: build }
    }
    /// Whether `row` is evidence about this view's engine.
    pub(super) fn measured_here(&self, row: &FootprintRow) -> bool {
        self.engine.is_none() || row.engine == self.engine
    }
    pub(super) fn read_all(&self) -> BTreeMap<String, FootprintRow> {
        let Some(path) = &self.path else {
            return BTreeMap::new();
        };
        match std::fs::read_to_string(path) {
            Ok(body) => serde_json::from_str(&body).unwrap_or_default(), // unwrap_or_default: a corrupt file re-measures every shape (a calibration run), never blocks training
            Err(_) => BTreeMap::new(),
        }
    }
    fn write_all(&self, all: &BTreeMap<String, FootprintRow>) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(()); // unrecorded: nowhere to keep it, so the next run measures again
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(all).map_err(std::io::Error::other)?)?;
        // a scanner holding the new temp must not lose the measurement (the 5090, 2026-10-10)
        continuum_host::durable::replace(&tmp, path)
    }
    pub(super) fn get(&self, shape: &Shape) -> Option<u64> {
        self.read_all().get(&shape.key()).filter(|r| r.whole_run && self.measured_here(r)).map(|r| r.bytes)
    }
    /// Every measured `(window, chunk, bytes)` of `shape`'s family (the same model, rank, targets,
    /// depth, walk and recompute, at any window and chunk): what [`choose_window`] fits. A key is
    /// read back by rebuilding the shape it names and comparing keys, so the parse can never
    /// accept a row of a different family.
    pub(super) fn rows_like(&self, shape: &Shape) -> Vec<(u32, u32, u64)> {
        self.family_rows(shape, true)
    }
    /// The whole-run rows of `shape`'s family measured on THIS engine: what a hard floor reads
    /// (a calibration grant's minimum, the exact walk ruled out). Another engine's rows predict,
    /// through the model, and never set a floor here (Codex on #4988).
    pub(super) fn measured_rows_like(&self, shape: &Shape) -> Vec<(u32, u32, u64)> {
        self.family_rows(shape, true)
            .into_iter()
            .filter(|&(window, chunk, _)| self.get(&Shape { window, chunk, ..shape.clone() }).is_some())
            .collect()
    }
    /// Every measured row of `shape`'s family, refusals and graph-only rows included: LOWER bounds
    /// on what each shape needs. They cap a plan (a graph alone over what is free means the whole
    /// run is over it too) but never fit the model, which reads whole-run rows only (#4939).
    pub(super) fn bound_rows_like(&self, shape: &Shape) -> Vec<(u32, u32, u64)> {
        self.family_rows(shape, false)
    }
    /// A shape's measured lower bound: the larger of its run row and its refusal.
    pub(super) fn bound(&self, shape: &Shape) -> Option<u64> {
        let all = self.read_all();
        [shape.key(), format!("{}{REFUSED}", shape.key())].iter().filter_map(|k| all.get(k)).filter(|r| self.measured_here(r)).map(|r| r.bytes).max()
    }
    /// What a shape needs ([`need`] over this store's rows): a refusal at a chunk still rules it
    /// out, so the ladder steps down instead of calibrating the refused shape again.
    pub(super) fn need(&self, shape: &Shape, predicted: Option<u64>) -> Option<Need> {
        need(self.get(shape), self.bound(shape), predicted)
    }
    /// `model`'s block count as an engine here last reported it with a measurement: what the depth
    /// rungs are halved from ([`choose_depth`]). `None` until a run on this engine has reported it.
    pub(super) fn layers(&self, model: &str) -> Option<u32> {
        self.read_all()
            .into_iter()
            .filter(|(key, row)| key.split('|').next() == Some(model) && self.measured_here(row) && row.layers.is_some())
            .max_by_key(|(_, row)| row.measured_at_ms)
            .and_then(|(_, row)| row.layers)
    }
    pub(super) fn family_rows(&self, shape: &Shape, whole_only: bool) -> Vec<(u32, u32, u64)> {
        self.read_all()
            .into_iter()
            .filter_map(|(key, row)| {
                let (named, refused) = key.strip_suffix(REFUSED).map_or((key.as_str(), false), |named| (named, true));
                let num = |prefix: char| {
                    named
                        .split('|')
                        .find_map(|seg| seg.strip_prefix(prefix).filter(|v| v.chars().all(|c| c.is_ascii_digit())).and_then(|v| v.parse::<u32>().ok()))
                };
                let window = num('w')?;
                let chunk = num('c').unwrap_or_else(|| train_chunk_for(window, TRAINING_CHUNK));
                let candidate = Shape { window, chunk, ..shape.clone() };
                // the fit reads whole runs of any engine (a prediction); bounds only this engine's
                let usable = if whole_only { !refused && row.whole_run } else { self.measured_here(&row) };
                (candidate.key() == named && usable).then_some((window, chunk, row.bytes))
            })
            .collect()
    }
    /// A RUN's footprint. Keeps the LARGEST observed for a shape: the peak is sampled on the
    /// governor's scan cadence, which can miss the true peak but never invents one (Cormac on
    /// #4443). A whole-run row always replaces a graph-only one, whatever their sizes: they
    /// measure different things.
    pub(super) fn record_with(&self, shape: &Shape, bytes: u64, whole_run: bool, layers: Option<u32>, job: Uuid) -> std::io::Result<()> {
        // a whole-run row is never replaced by a graph-only one (an engine rolled back before
        // #52), whatever its size; otherwise the larger measurement stands (Cormac on #4939)
        // within ONE engine; another engine's measurement is not evidence about this one, so a
        // measurement on this engine always replaces it
        let row = self.row(bytes, whole_run, layers, job, Provenance::Run);
        self.upsert(shape.key(), row, |old, new| (old.whole_run && !new.whole_run) || (old.whole_run == new.whole_run && old.bytes >= new.bytes))
    }
    /// A REFUSAL's graph, a lower bound, under the shape's refusal key: the largest stands.
    pub(super) fn record_refusal(&self, shape: &Shape, bytes: u64, layers: Option<u32>, job: Uuid) -> std::io::Result<()> {
        let row = self.row(bytes, false, layers, job, Provenance::Refusal);
        self.upsert(format!("{}{REFUSED}", shape.key()), row, |old, new| old.bytes >= new.bytes)
    }
    fn row(&self, bytes: u64, whole_run: bool, layers: Option<u32>, job: Uuid, provenance: Provenance) -> FootprintRow {
        let measured_at_ms = continuum_base::time::EpochMs::now().millis();
        FootprintRow { engine: self.engine.clone(), bytes, measured_at_ms, job: job.to_string(), whole_run, layers, provenance: Some(provenance) }
    }
    /// Write `new` under `key` unless this engine's row there is to be `kept` over it; either way
    /// the engine's reported block count lands, so a kept row never hides a newer count (Codex on
    /// #5548: an equal or smaller run must still attach it).
    fn upsert(&self, key: String, new: FootprintRow, kept: impl Fn(&FootprintRow, &FootprintRow) -> bool) -> std::io::Result<()> {
        let mut all = self.read_all();
        match all.get_mut(&key) {
            Some(old) if old.engine == self.engine && kept(old, &new) => {
                if new.layers.is_none() || old.layers == new.layers {
                    return Ok(());
                }
                old.layers = new.layers;
            }
            _ => {
                all.insert(key, new);
            }
        }
        self.write_all(&all)
    }
    /// Classify every row from before [`Provenance`]: a whole-run row whose own job recorded an
    /// engine refusal of exactly its size (`returnable.json`, or #4904's `refusal.json`, in its
    /// directory under one of `roots`) WAS a refusal, filed as whole before card 0d21e644 (the
    /// windows 5090's row for 598e183c at window 45,056). It moves to the refusal key, out of the
    /// fit. A row whose job is confirmed to record no such refusal is stamped a run. A row whose
    /// evidence cannot be read (a root, a directory or a record that errs) stays unclassified and is
    /// tried again on the next dispatch: a transient failure never stamps a refusal as a run (Codex
    /// on #5548). Returns how many were reclassified as refusals and how many stay unknown.
    pub(super) fn classify_legacy(&self, roots: &[PathBuf]) -> std::io::Result<Classified> {
        let mut all = self.read_all();
        let legacy: Vec<String> = all.iter().filter(|(k, r)| r.provenance.is_none() && !k.ends_with(REFUSED)).map(|(k, _)| k.clone()).collect();
        let mut classified = Classified::default();
        if legacy.is_empty() {
            return Ok(classified);
        }
        let mut changed = false;
        for key in legacy {
            let Some(mut row) = all.remove(&key) else { continue };
            let evidence = if row.whole_run { refusal_evidence(&row, roots) } else { Evidence::NotRefused };
            match evidence {
                Evidence::Refused => {
                    classified.refusals += 1;
                    changed = true;
                    row.whole_run = false;
                    row.provenance = Some(Provenance::Refusal);
                    let refused_key = format!("{key}{REFUSED}");
                    if all.get(&refused_key).is_none_or(|kept| kept.bytes < row.bytes) {
                        all.insert(refused_key, row);
                    }
                }
                Evidence::NotRefused => {
                    changed = true;
                    row.provenance = Some(Provenance::Run);
                    all.insert(key, row);
                }
                Evidence::Unknown(why) => {
                    classified.unknown.push(why);
                    all.insert(key, row); // as it was: tried again on the next dispatch
                }
            }
        }
        if changed {
            self.write_all(&all)?;
        }
        Ok(classified)
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
pub(super) fn exact_walk_host_budget_mib(available_bytes: Option<u64>) -> Option<u64> {
    let (num, den) = TRAINING_HOST_SHARE;
    available_bytes.map(|b| ((b / den * num) >> 20).max(1))
}

/// What the job asks the engine for, from the monitor's reading: the exact walk with its host
/// budget when the host is read, the plain walk (no budget) when it is not.
pub(super) fn walk_request(available_bytes: Option<u64>) -> (bool, Option<u64>) {
    match exact_walk_host_budget_mib(available_bytes) {
        Some(mib) => (true, Some(mib)),
        None => (false, None),
    }
}

#[cfg(test)]
impl TrainingHolds {
    /// Pause the in-engine run on one lane (its root url) until the hold drops.
    pub(super) fn hold_on(&self, lane: &str, reason: &str) -> TrainingHold {
        self.insert(lane_scope(lane), reason)
    }
}
