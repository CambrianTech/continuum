use super::*;

pub(super) use continuum_engine_protocol::{EngineExample, ExampleFit, Fit, JobRef, MeasureReply, MeasureRequest, MeasuredExample, TrainRequest, TrainState, TrainStatus};

/// Where the lane serving `base` answers, if one does on this node.
/// The live lane serving a base: its url and the per-slot window it was launched with (0 =
/// a record from before that field, which is unknown). One lookup decides both, so the
/// training window can never come from a second source that disagrees with the lane
/// (Cormac on #4498).
pub(super) type LaneResolver = Box<dyn LaneSource>;

/// Where a run's lane comes from: the ONE difference between the in-engine trainers (this
/// node's resident lane, a lane stood up for the run, a grid peer's lane). The training is
/// the same `POST /train` against whatever lane this returns; no trainer is its own process.
#[async_trait]
pub(super) trait LaneSource: Send + Sync {
    /// Whether a lane for `base` is available now; the coordinator asks before it ranks, so a
    /// trainer is never chosen for a job it would refuse.
    fn serves(&self, base: &str) -> bool;
    /// A lane for `base`, alive for as long as the returned lease is held. Typed by cause (Codex
    /// on #5269): an invalid request is `InvalidRequest`, never retried as-is; memory pressure, a
    /// lane that will not start, or a transport error is `Transient`, retried by the dispatch.
    async fn acquire(&self, base: &str) -> Result<LaneLease, FineTuningError>;
    /// The coordinator's provider id for a trainer on these lanes.
    fn provider(&self) -> &'static str;
    /// Where a trainer on these lanes trains ([`TrainingPlace`]).
    fn place(&self) -> crate::genome::fine_tuning::adapter::TrainingPlace;
}

/// A lane a run trains in, and what keeps it alive: `hold` is `None` for a lane that outlives
/// the run (the resident lane), and owns a lane stood up for this run until the run ends.
pub(super) struct LaneLease {
    pub(super) choice: LaneChoice,
    pub(super) hold: Option<Box<dyn std::any::Any + Send + Sync>>,
    /// Whether the lane chosen is still the one at `choice.url`, run inside the admission hold
    /// before the run binds to it. Each source checks its own lane's identity (incarnation,
    /// endpoint, base): a pid alone is not an identity, and a lane stood up for the run is
    /// never the node's live lane, so a re-read of that record cannot answer for it.
    pub(super) still_bound: Box<dyn Fn() -> Result<(), Unbound> + Send + Sync>,
}

/// Why a chosen lane no longer holds the run: it is gone, or something else answers there.
#[derive(Debug)]
pub(super) enum Unbound {
    Gone(String),
    /// A different engine took its place; nothing ran, so her examples go back (card 0b8de4da).
    Replaced(String),
}

/// The live lane serving the base on this node: training on the resident weights.
pub(super) struct ResidentLane;

#[async_trait]
impl LaneSource for ResidentLane {
    fn serves(&self, base: &str) -> bool {
        live_lane_for(base).is_some()
    }

    async fn acquire(&self, base: &str) -> Result<LaneLease, FineTuningError> {
        let choice = live_lane_for(base).ok_or_else(|| {
            FineTuningError::InvalidRequest(format!(
                "no live lane serves {base} on this node: in-engine training runs on the resident weights"
            ))
        })?;
        let (base, chosen, url) = (base.to_owned(), choice.engine, choice.url.clone());
        Ok(LaneLease {
            choice,
            hold: None,
            still_bound: Box::new(move || still_bound_to(crate::inference::lane_registry::live_lane(), &base, chosen, &url)),
        })
    }
    fn provider(&self) -> &'static str {
        PROVIDER_ID
    }

    fn place(&self) -> crate::genome::fine_tuning::adapter::TrainingPlace {
        crate::genome::fine_tuning::adapter::TrainingPlace::ResidentHere
    }
}

/// `rec` (the lane serving now) is still the one chosen: the same engine incarnation, at the
/// same endpoint, serving the same base.
pub(super) fn still_bound_to(
    rec: Option<crate::inference::lane_registry::LaneRecord>,
    base: &str,
    chosen: Option<crate::inference::engine_residency::EngineIncarnation>,
    url: &str,
) -> Result<(), Unbound> {
    let gone = || Unbound::Gone("no live engine at admission: the lane went away while this job waited".into());
    let rec = rec.ok_or_else(gone)?;
    let now = incarnation_of(&rec).ok_or_else(gone)?;
    let same_base = crate::model_registry::artifacts::names_the_same_base(&rec.model, base);
    if chosen == Some(now) && rec.root_url() == url && same_base {
        return Ok(());
    }
    let was = chosen.map_or_else(|| "an unidentified engine".to_owned(), |c| format!("pid {} started {}", c.pid, c.started_s));
    Err(Unbound::Replaced(format!(
        "the lane was replaced while this job waited ({was} at {url} serving {base} is now pid {} started {} at {} serving {}); not training on a different engine",
        now.pid,
        now.started_s,
        rec.root_url(),
        rec.model
    )))
}

/// A test's lane: any `Fn(base) -> Option<LaneChoice>`, held by nothing.
#[cfg(test)]
#[async_trait]
impl<F: Fn(&str) -> Option<LaneChoice> + Send + Sync> LaneSource for F {
    fn serves(&self, base: &str) -> bool {
        self(base).is_some()
    }

    async fn acquire(&self, base: &str) -> Result<LaneLease, FineTuningError> {
        let choice = self(base).ok_or_else(|| FineTuningError::InvalidRequest(format!("the test lane serves no {base}")))?;
        Ok(LaneLease { choice, hold: None, still_bound: Box::new(|| Ok(())) })
    }
    fn provider(&self) -> &'static str {
        PROVIDER_ID
    }

    fn place(&self) -> crate::genome::fine_tuning::adapter::TrainingPlace {
        crate::genome::fine_tuning::adapter::TrainingPlace::ResidentHere
    }
}

/// The lane a run will train in, as the ONE live-lane record names it.
#[derive(Debug, Clone)]
pub(super) struct LaneChoice {
    pub(super) url: String,
    /// The per-slot window it was launched with (0 = unknown, refused).
    pub(super) window: u32,
    /// The engine process hosting it (pid plus OS start time), the identity a run binds to at
    /// admission (SHARED-RESIDENT-LIFECYCLE.md step 1). `None` only in tests with no engine.
    pub(super) engine: Option<crate::inference::engine_residency::EngineIncarnation>,
    /// The build of that engine (its slot stamp): what a footprint measured on it is filed under,
    /// so a measurement is never read as evidence about another engine (card de9e1342). `None`
    /// on a pre-slot install and in tests.
    pub(super) build: Option<String>,
    /// The governed pool its bytes live in (`Vram` for a GPU-placed lane, `Ram` for a
    /// CPU-placed one): what a run in it leases and fits its chunk against.
    pub(super) memory: crate::resources::ResourceKind,
    /// The weights file the lane's engine was launched on (its record's stamp): what an adapter
    /// trained in it names as its base (card 3fd98325). `None` for a lane recorded without one.
    pub(super) weights: Option<crate::inference::weights_stamp::WeightsStamp>,
}

/// A test's lane at `url` with a `window`: no engine, no build, on the card.
#[cfg(test)]
impl LaneChoice {
    pub(super) fn at(url: String, window: u32) -> Self {
        Self { url, window, engine: None, build: None, memory: crate::resources::ResourceKind::Vram, weights: None }
    }
}

/// A lane record's incarnation. A record from before `started_s` names the live process by
/// its pid and the start time the table reports now.
pub(super) fn incarnation_of(rec: &crate::inference::lane_registry::LaneRecord) -> Option<crate::inference::engine_residency::EngineIncarnation> {
    if rec.started_s != 0 {
        Some(rec.incarnation())
    } else {
        crate::inference::engine_residency::EngineIncarnation::of(rec.pid, rec.port)
    }
}

pub(super) fn live_lane_for(base: &str) -> Option<LaneChoice> {
    lane_choice(crate::inference::lane_registry::live_lane()?, base)
}

/// The per-slot window the live lane serving `base` ADOPTED (its launch record, not a plan that
/// may not have landed): what a training partition on this node is measured at, so the trigger
/// asks the same number before returning parked examples (card 565b3b64). `None`: no live lane
/// serves `base` here, so nothing of it can train now.
pub fn served_window_for(base: &str) -> Option<u32> {
    live_lane_for(base).map(|lane| lane.window)
}

/// The lane `rec` names as a place to train `base`, priced in the pool its recorded placement
/// keeps its bytes in. A lane recorded before placements were (an older build launched it)
/// is not offered: its pool is unknown, and the current configuration is the NEXT launch's
/// truth, not this process's (Codex on #5264). It is offered again from its next launch.
pub(super) fn lane_choice(rec: crate::inference::lane_registry::LaneRecord, base: &str) -> Option<LaneChoice> {
    if !crate::model_registry::artifacts::names_the_same_base(&rec.model, base) {
        return None;
    }
    let Some(memory) = rec.memory() else {
        continuum_base::probe!(
            class = "training.lane.placement_unrecorded",
            port = rec.port as u64,
            "a lane launched before placements were recorded: not offered for training until it relaunches"
        );
        return None;
    };
    Some(LaneChoice {
        url: rec.root_url(),
        window: rec.context_window,
        engine: incarnation_of(&rec),
        build: rec.engine_bin.as_deref().and_then(crate::inference::engine_slots::stamp_of_bin),
        memory,
        weights: rec.weights.clone(),
    })
}

/// Admission: the governed lease a run holds for its life. `Governed` in production;
/// tests drive the run without a governor.
pub(super) enum Admission {
    Governed,
    /// No governor: the free VRAM a chunk is fitted against is what the test says it is
    /// (`u64::MAX` = nothing to fit against, the default chunk).
    #[cfg(test)]
    Ungoverned { vram_free: u64 },
}

pub struct EngineLoraFineTuner {
    pub(super) jobs: NativeJobs,
    pub(super) http: reqwest::Client,
    pub(super) lane: LaneResolver,
    /// The lanes' `--train-dir` (the engine writes `<job>.gguf` here).
    pub(super) train_dir: Option<PathBuf>,
    pub(super) footprints: Footprints,
    pub(super) admission: Admission,
    /// the holds this tuner's runs obey (the node's, or a test's own)
    pub(super) holds: TrainingHolds,
    /// Where `genome/job-pause` records a job's pause, so it outlives the core (`None`: no
    /// home, so no persisted pauses; the in-memory holds still apply).
    pub(super) hold_store: Option<PathBuf>,
    /// Where a run's binding to its engine incarnation is recorded (`None`: no home, or a
    /// test with no engine; an ungoverned run binds nothing).
    pub(super) residency_store: Option<PathBuf>,
    /// Where each persona's outstanding training demand is recorded for serving to read
    /// ([`crate::genome::fine_tuning::training_demand`]; `None`: no home, nothing recorded).
    pub(super) demand_store: Option<PathBuf>,
    /// Leases re-taken at boot for runs a previous core bound to a still-live engine
    /// ([`Self::reclaim_resident_leases`]), held here until `reattach` hands each to its run or
    /// the binding is released. The lease was in the dead core's memory; the engine's
    /// allocation was not, so until this re-takes it a second job could be admitted into it.
    pub(super) resident_leases: Arc<Mutex<BTreeMap<Uuid, crate::resources::LeaseGuard>>>,
    /// The least a run's POST may wait ([`post_wait_for`]): [`POST_WAIT_FLOOR`], or a test's
    /// shorter one, so a lost POST can be exercised without a two-minute wait.
    pub(super) post_wait_floor: Duration,
}

impl Default for EngineLoraFineTuner {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineLoraFineTuner {
    /// Training on the resident weights: the lane that serves the base on this node.
    pub fn new() -> Self {
        Self::on(Box::new(ResidentLane))
    }

    /// Training on a lane of our own engine stood up for the run ([`EngineLane`]), for a base
    /// no live lane here serves.
    pub fn on_engine_lanes() -> Self {
        Self::on(Box::new(super::engine_lane::EngineLane))
    }

    fn on(lane: LaneResolver) -> Self {
        // Without a home nothing persists: every shape re-measures (a calibration run), never a guessed footprint.
        let footprints = match crate::paths::ContinuumLayout::home() {
            Ok(layout) => Footprints::at(layout.genome().join("engine-footprints.json")),
            Err(_) => Footprints::unrecorded(),
        };
        Self {
            jobs: NativeJobs::new(lane.provider()),
            http: reqwest::Client::new(),
            lane,
            train_dir: crate::inference::llama_server::engine_train_dir(),
            footprints,
            admission: Admission::Governed,
            holds: TrainingHolds::node().clone(),
            hold_store: crate::paths::continuum_home()
                .ok()
                .map(|home| super::super::training_hold_store::store_path(&home)),
            residency_store: crate::paths::continuum_home()
                .ok()
                .map(|home| crate::inference::engine_residency::store_path(&home)),
            demand_store: crate::paths::ContinuumLayout::home()
                .ok()
                .map(|layout| crate::genome::fine_tuning::training_demand::TrainingDemand::store_dir(&layout.genome())),
            resident_leases: Arc::new(Mutex::new(BTreeMap::new())),
            post_wait_floor: POST_WAIT_FLOOR,
        }
    }

    #[cfg(test)]
    /// [`Self::for_test`] on a host with `vram_free` bytes governed and free, so a chunk is
    /// chosen against it (card d6dae498).
    #[cfg(test)]
    pub(super) fn for_test_with_vram(lane_url: String, train_dir: PathBuf, footprints: PathBuf, vram_free: u64) -> Self {
        let mut t = Self::for_test(lane_url, train_dir, footprints);
        t.admission = Admission::Ungoverned { vram_free };
        t
    }

    /// The memory a job may fit a chunk into now, in its lane's pool: the governor's figure
    /// for this consumer, or what an ungoverned (test) host declares.
    pub(super) fn free_for(&self, consumer: &str, memory: crate::resources::ResourceKind) -> u64 {
        match self.admission {
            Admission::Governed => governed_training_free(consumer, memory),
            #[cfg(test)]
            Admission::Ungoverned { vram_free } => vram_free,
        }
    }

    #[cfg(test)]
    pub(super) fn for_test(lane_url: String, train_dir: PathBuf, footprints: PathBuf) -> Self {
        Self {
            jobs: NativeJobs::new(PROVIDER_ID),
            http: reqwest::Client::new(),
            // a lane launched at 256 per slot: the window every existing test asserts
            lane: Box::new(move |_: &str| Some(LaneChoice::at(lane_url.clone(), 256))),
            train_dir: Some(train_dir),
            footprints: Footprints::at(footprints),
            admission: Admission::Ungoverned { vram_free: u64::MAX },
            holds: TrainingHolds::new(),
            hold_store: None,
            residency_store: None,
            demand_store: None,
            resident_leases: Arc::new(Mutex::new(BTreeMap::new())),
            post_wait_floor: Duration::from_secs(1),
        }
    }
}

/// The in-place run: POST /train on the lane, poll to its end, stop it on a cancel.
pub(super) struct EngineRun {
    pub(super) http: reqwest::Client,
    pub(super) lane: String,
    /// The shape this run trains, and where footprints are filed: a REFUSED run files the
    /// graph the engine's preflight measured, under the chunk it reports, so the next
    /// dispatch of this shape chooses a smaller chunk instead of calibrating at the same
    /// one forever (BigMama on #4894: footprints were filed by a finished run only).
    pub(super) shape: Shape,
    pub(super) footprints: Footprints,
    /// The run to POST, or `None` for a run this core RE-ATTACHED to: it is already training in
    /// the engine, and POSTing it again would be the duplicate run step 3 exists to prevent.
    pub(super) start: Option<TrainRequest>,
    /// How long the POST of `start` may take: the engine prepares (renders and tokenizes) every
    /// example before it answers, so the wait derives from this job's measured render time
    /// ([`post_wait_for`]), never a constant that a larger batch outlasts.
    pub(super) post_wait: Duration,
    /// The engine's name for the run (`/train`'s `out`), what every status is matched on.
    pub(super) out: String,
    /// where the engine writes this job's adapter (removed if a cancel races a finish)
    pub(super) adapter_path: PathBuf,
    /// This job's own directory, where a refusal is recorded for the trigger (card 2dfce676).
    pub(super) job_dir: PathBuf,
    pub(super) epochs: u32,
    /// The run's last status as the engine reported it (finish reads its losses and footprint).
    pub(super) last: Arc<Mutex<Option<TrainStatus>>>,
    pub(super) holds: TrainingHolds,
    /// This job's id (its handle's `local_id`) and the store its persisted pauses live in.
    pub(super) job: Uuid,
    pub(super) hold_store: Option<PathBuf>,
    /// Where this run's binding to its engine incarnation is recorded, and that incarnation,
    /// when admission bound it (`None`: an ungoverned test run). Released only when the run's
    /// end is SETTLED: see [`EngineProbe::settled`].
    pub(super) residency: Option<(PathBuf, crate::inference::engine_residency::ResidentWork)>,
    /// What keeps a lane stood up for this run alive ([`LaneLease::hold`]): dropped with the run.
    pub(super) _lane_hold: Option<Box<dyn std::any::Any + Send + Sync>>,
    /// The governed lease, held for exactly as long as the engine may be running this job's
    /// training: `run` returns only once it has ended there, and the lease drops with `self`.
    pub(super) _lease: Option<crate::resources::LeaseGuard>,
}

/// Just enough of a run to ask the engine, after the run's future has ended, whether this job
/// is still running there.
pub(super) struct EngineProbe {
    pub(super) http: reqwest::Client,
    pub(super) lane: String,
    pub(super) out: String,
}

impl EngineProbe {
    /// The end of a run the engine provably never started (card 26df): its documented refusal
    /// of the POST, and nothing else. `settled` can't decide this case,
    /// since it waits for THIS run's terminal state and a refused run never has one. The
    /// binding is settled when the bound incarnation is verifiably dead, or is still the same
    /// process, which owns the lane that answered. An unreadable process keeps it.
    pub(super) async fn never_started(&self, engine: &crate::inference::engine_residency::EngineIncarnation) -> bool {
        use crate::inference::engine_residency::Liveness;
        let engine = *engine;
        matches!(
            tokio::task::spawn_blocking(move || engine.liveness()).await,
            Ok(Liveness::Dead | Liveness::Alive)
        )
    }

    /// Is this run's end SETTLED (SHARED-RESIDENT-LIFECYCLE.md; Codex on #4531)? Only on
    /// positive evidence:
    /// - the incarnation is VERIFIABLY dead (no such process, or the pid was reused); or
    /// - the engine answers 2xx that THIS run (`out`) is in a terminal state (done, cancelled
    ///   or error), and the incarnation is still the same one after that network round trip.
    /// Everything else is unknown and holds: an unreadable process, no answer, a non-2xx
    /// answer, a foreign or idle answer (not proof about this run).
    pub(super) async fn settled(&self, engine: &crate::inference::engine_residency::EngineIncarnation) -> bool {
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
    pub(super) async fn status_once(&self) -> Result<TrainStatus, String> {
        Self::status_at(&self.http, &self.lane).await
    }

    /// One `GET /train` on `lane`: the engine's training status, or why it could not be read.
    pub(super) async fn status_at(http: &reqwest::Client, lane: &str) -> Result<TrainStatus, String> {
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
    pub(super) async fn status(&self) -> Result<TrainStatus, String> {
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
    pub(super) fn running_ours(&self, s: &TrainStatus) -> bool {
        self.ours(s) && !matches!(s.state, TrainState::Done | TrainState::Cancelled | TrainState::Error)
    }

    /// The one way out of a run that did not finish: make sure the engine is not still training
    /// this job before the lease is let go. Cancels (retrying the POST) and waits until the
    /// engine's status is no longer this job's live run, or the lane is gone.
    pub(super) async fn stop_ours(&self) {
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

    pub(super) async fn failed(&self, why: String) -> InPlaceEnd {
        self.stop_ours().await;
        InPlaceEnd::Failed(why)
    }

    /// The engine runs one training at a time; a status for a different `out` is not ours.
    pub(super) fn ours(&self, s: &TrainStatus) -> bool {
        s.out.as_deref() == Some(self.out.as_str())
    }

    /// Steer the engine's pause toward `want` (fork #28's contract): ask when the status says
    /// otherwise, and let the next status tell whether it took. A refused or lost request is
    /// asked again next tick; uncertainty is never turned into a cancel. An engine without the
    /// routes (404) cannot pause: the run keeps its automatic serving yield.
    pub(super) async fn steer_pause(&self, s: &TrainStatus, want: bool) {
        if s.pause_requested == want {
            return;
        }
        let verb = if want { "pause" } else { "resume" };
        let _ = self
            .http
            .post(format!("{}/train/{verb}", self.lane))
            .json(&JobRef { out: self.out.clone() })
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
pub(super) fn held_out_loss(s: &TrainStatus, asked_split: f32) -> Option<f64> {
    let evaluated = s.eval_trainable_tokens.map_or(asked_split > 0.0, |n| n > 0);
    s.epochs.last().and_then(|e| e.eval_loss).filter(|_| evaluated)
}

/// Percent done and the current epoch from an engine status: completed epochs plus the current
/// epoch's batch fraction.
pub(super) fn progress_of(s: &TrainStatus, epochs: u32) -> (f32, u32) {
    let done = s.epochs.len() as f32;
    let batch = s.batch.unwrap_or(0) as f32; // unwrap_or: absent before the first batch — zero of this epoch is the truth
    let max = s.batch_max.unwrap_or(0) as f32; // unwrap_or: absent before the first batch
    // A batch past the epoch's last is that epoch done; completed epochs past the run's are the run done.
    let frac = if max > 0.0 { batch.min(max) / max } else { 0.0 };
    let pct = if epochs == 0 { 0.0 } else { (done + frac).min(epochs as f32) / epochs as f32 * 100.0 };
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
        let (mut end, never_started) = match self.begin().await {
            Begin::NeverStarted(end) => (end, true),
            Begin::Unproven(end) => (end, false),
            Begin::Running(run) => (run.run_steered(cancel, progress).await, false),
        };
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
            let settled = if never_started {
                probe_run.never_started(&bound.engine).await
            } else {
                probe_run.settled(&bound.engine).await
            };
            if settled {
                let released = tokio::task::spawn_blocking(move || crate::inference::engine_residency::release(&path, &bound)).await;
                if !matches!(released, Ok(Ok(_))) {
                    continuum_base::probe!(
                        class = "training.residency.release_failed",
                        job = %job,
                        "a settled run's engine binding could not be released; serving stays held on it until it can be"
                    );
                }
            } else {
                continuum_base::probe!(
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
                super::super::training_hold_store::release_job(&store, job, continuum_base::time::EpochMs::now().millis())
            })
            .await;
            if !matches!(released, Ok(Ok(_))) {
                continuum_base::probe!(
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
    /// Start the run in the engine, or say provably that it never started there. Split from
    /// the watch so a refused start settles its engine binding as "never started" (card 26df),
    /// not by waiting on a terminal state the engine will never report for this run.
    pub(super) async fn begin(self: Box<Self>) -> Begin {
        // a re-attached run is already in the engine: it is watched, never started again
        if let Some(body) = &self.start {
            match self
                .http
                .post(format!("{}/train", self.lane))
                .json(body)
                .timeout(self.post_wait)
                .send()
                .await
            {
                Ok(r) if r.status().is_success() => {}
                Ok(r) => {
                    let status = r.status();
                    let why = r.text().await.unwrap_or_default(); // unwrap_or_default: an unreadable refusal body still fails the job
                    let end = InPlaceEnd::Failed(format!("the engine refused the training run: HTTP {status}: {why}"));
                    // Only the engine's documented refusal proves nothing of ours runs (Codex on
                    // #5001): any other non-2xx (a 500, a proxy's answer) proves nothing.
                    return if is_documented_train_refusal(status, &why) { Begin::NeverStarted(end) } else { Begin::Unproven(end) };
                }
                // LOST ON THE WAY BACK, RECONCILED BY THE RUN'S ID (card 93ffc555): the engine may
                // have the run (fork #55 claims and shows it 'preparing' before its long prepare),
                // so its status decides, never a clock: this job's run, in ANY state including its
                // error, is handed to the watch loop below, whose arms already file a refused
                // graph's footprint and its returnable record (Cormac on #4946: a shortcut here
                // dropped both, which is 97e081e2 again); anything else means it never started
                // there, and it fails as before.
                Err(e) => {
                    let status = self.status().await.ok();
                    match after_lost_post(status.as_ref(), |s| self.ours(s)) {
                        LostPost::Watch => continuum_base::probe!(
                            class = "training.job.post_reconciled",
                            job = %self.job,
                            error = %e,
                            "the POST was lost on the way back but the engine reports this job's run: it is watched to its end, not failed"
                        ),
                        // A lost POST may still be queued or in flight when the status is read, so a
                        // status naming another run or none does not prove it will never start
                        // (Codex on #5001). Its binding settles only the ordinary way.
                        LostPost::NeverStarted => {
                            return Begin::Unproven(self.failed(format!("POST /train on {}: {e}", self.lane)).await);
                        }
                    }
                }
            }
        }
        Begin::Running(self)
    }

    pub(super) async fn run_steered(self: Box<Self>, mut cancel: watch::Receiver<bool>, progress: RunProgress) -> InPlaceEnd {
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
                TrainState::Preparing | TrainState::Starting | TrainState::Running => {
                    let (pct, epoch) = progress_of(&s, self.epochs);
                    progress.running(pct, epoch);
                    // held: pause at the next window, keeping everything; released: resume. The
                    // lease stays held throughout, since the paused run keeps its allocation.
                    let mut reasons = holds_on(&holds.borrow(), &self.lane);
                    // a pause asked through genome/job-pause, read every tick: it outlives the core
                    if let Some(store) = &self.hold_store {
                        // Holds are created and expire in wall time, which a core restart keeps. A pre-1970
                        // clock reads 0 here: every hold reads as live, never as expired early.
                        let now = continuum_base::time::EpochMs::now().millis();
                        match super::super::training_hold_store::live_on(store, self.job, now).await {
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
                        continuum_base::probe!(
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
                        (Some(windows), Some(busy), Some(yielded)) => continuum_base::probe!(
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
                        _ => continuum_base::probe!(
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
                    let grown = s.run_bytes();
                    if grown > 0 {
                        let ran_window = s.window.unwrap_or(self.shape.window); // unwrap_or: an engine that reports no window ran the one asked
                        let ran_chunk = s.chunk.unwrap_or_else(|| train_chunk_for(ran_window, self.shape.chunk)); // unwrap_or_else: an engine that reports no chunk ran the mirrored rule
                        // the depth the engine adapted (it reports it before the preflight), as a finish files it
                        let depth = effective_depth(s.layers_adapted, s.n_layer);
                        let refused = Shape { window: ran_window, chunk: ran_chunk, depth, ..self.shape.clone() };
                        // The job's examples belong back in her bucket so the next dispatch can
                        // step down; the trigger reads this to decide (card 2dfce676).
                        if let Err(e) = write_returnable(&self.job_dir, &Returnable::EngineRefused(Refusal { window: ran_window, chunk: ran_chunk, depth, layers: s.n_layer, graph_bytes: grown, error: why.to_string() })) {
                            continuum_base::probe!(
                                class = "training.job.refusal_unrecorded",
                                job = %self.job,
                                error = %e,
                                "the refusal could not be recorded in the job's directory; its examples wait for a manual return"
                            );
                        }
                        // file I/O that may wait out a Windows sharing violation: off the runtime
                        let store = self.footprints.clone();
                        let (shape, job) = (refused.clone(), self.job);
                        // A LOWER BOUND under its own key: the preflight sized a graph that never ran,
                        // so it rules this shape out, never trains the fit, and never replaces or hides
                        // a run's row (card 0d21e644)
                        let layers = s.n_layer;
                        let recorded = tokio::task::spawn_blocking(move || store.record_refusal(&shape, grown, layers, job))
                            .await
                            .unwrap_or_else(|e| Err(std::io::Error::other(e.to_string())));
                        match recorded {
                            Ok(()) => continuum_base::probe!(
                                class = "training.job.footprint_from_refusal",
                                job = %self.job,
                                window = ran_window as u64,
                                chunk = ran_chunk as u64,
                                exact = refused.exact,
                                graph_bytes = grown,
                                "the engine refused this shape's graph; its size is filed as a lower bound so the next dispatch steps a rung down (chunk, then depth, then window)"
                            ),
                            Err(e) => continuum_base::probe!(
                                class = "training.job.footprint_unrecorded",
                                job = %self.job,
                                error = %e,
                                "the refused graph's size could not be recorded; the next dispatch calibrates at the same chunk"
                            ),
                        }
                    }
                    return InPlaceEnd::Failed(format!("the engine's training run failed: {why}"));
                }
                // idle while ours is named, a server with no trainer, or a state this core does not
                // know: not a known end
                TrainState::Idle | TrainState::Unavailable | TrainState::Unknown => {
                    return self.failed(format!("the engine reports training state {:?} for this job", s.state)).await
                }
            }
        }
    }
}
