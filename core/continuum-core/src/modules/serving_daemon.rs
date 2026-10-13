//! ServingDaemonModule — the ever-present ServiceModule that decides, and
//! continuously re-decides, how THIS host serves persona inference.
//!
//! It is the control loop around the pure [`crate::cognition::serving_plan`]
//! classifier. On each tick it takes an honest snapshot of the host — the
//! GPU/UMA serving budget plus the model footprints actually on disk — runs
//! [`plan_serving`], and publishes the resulting [`ServingPlan`] (which base
//! model, how many continuous-batching lanes, how many models to keep warm) on
//! a `watch` channel for the scheduler + spawner to read.
//!
//! This is the "ebb and flow" seam Joel describes: as pressure shifts and
//! demand changes, the plan is recomputed and republished — a huge MoE coder
//! pages in and the general model drops to one lane; demand falls, it flows
//! back. Driving the scheduler/spawner from the published plan and reacting to
//! the `PressureBroker` snapshot are the NEXT slices; this slice establishes
//! the loop, the published decision, and the `serving/plan` query command so
//! personas + operators can see what the substrate decided and why.
//!
//! Shape per docs/architecture/CONCURRENCY-STYLE-GUIDE.md: a ServiceModule with
//! a `tick_interval` that the runtime drives via `tick()`. No bespoke thread,
//! no lock held across await — the `watch::Sender` is the only shared state and
//! its `send` takes `&self`. cbar's pipeline-stage pattern in Rust dress.

pub(crate) mod academy_batch;
pub(crate) mod lane_demand;
use crate::time::EpochMs;
use lane_demand::*;
pub(crate) mod gates;
use gates::*;
pub(crate) mod host_budget;
use host_budget::*;
pub(crate) mod vram;
use vram::*;
pub(crate) mod prompt_cache;
use prompt_cache::*;
pub(crate) mod candidates;
use candidates::*;
pub(crate) mod snapshot;
use snapshot::*;
pub(crate) mod wiring;
pub(crate) mod intent;
pub(crate) mod plan;
pub(crate) mod measure;
pub(crate) mod reconcile;
pub(crate) mod health;
pub(crate) mod publish;

use super::serving_consumer::{FootprintFn, ServingConsumer, SERVING_CONSUMER_ID};
use crate::capacity::placement::PlacementRequest;
use crate::cognition::model_resolver::types::HwCapabilityTier;
use crate::cognition::serving_plan::footprint::{footprint_for, footprint_from_parts};
use crate::cognition::serving_plan::{
    plan_serving, plan_serving_at_rest, plan_serving_stable, HostBudget, ModelFootprint,
    ServingDemand, ServingPlan,
};
use crate::gpu::GpuMemoryManager;
use crate::inference::lane_registry::LaneRecord;
use crate::inference::llama_server::{
    ensure_model_serving_if_current, serving_v1_url, AdapterEntry, EnsureOutcome,
    LlamaServerControl, LlamaServerProcess, ServingSnapshot, ServingTarget, READY_TIMEOUT,
};
use crate::model_registry::live::{Availability, CatalogSnapshot, ModelCatalog};
use crate::model_registry::types::Model;
use crate::persona::hw_tier_descriptor::HwTierCategory;
use crate::resources::{LeaseBoard, ResourceDaemon, ResourceKind};
use crate::runtime::message_bus::MessageBus;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::system_resources::SystemResourceMonitor;
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// How often the daemon re-evaluates the serving plan. 5s matches the other
/// pressure-class ticks (pressure-broker, ai_provider) in the runtime. The
/// next slice makes the re-plan also fire on PressureBroker watch edges so it
/// reacts faster than the cadence under sudden pressure.
const TICK: Duration = Duration::from_secs(5);

/// The margin, in percent, by which the plan must exceed the LIVE lane before a
/// re-home is worth killing in-flight turns for.
///
/// RELATIVE, not absolute: the same 2048-token shortfall is a third of a small
/// lane and noise on a large one, so an absolute bar means something different at
/// every window size.
///
/// MEASURED against 6,322 live `serving.plan` samples (probes.jsonl, 2026-08-06),
/// after BigMama flagged that she had proposed 15% from the shape of the problem
/// with no data behind it: consecutive-sample plan jitter runs p50 0%, p90 1%, with
/// a fat tail (p99 22%, max 106%). 15% sits an order of magnitude above the p90
/// noise floor while staying far under the 60% shortfall of the real incident.
///
/// Note which parameter is actually load-bearing, because it is NOT this one: at a
/// 10%, 15% OR 20% bar the longest run of consecutive qualifying rises in the whole
/// sample was 2. The jitter tail is a SPIKE, not a sustained climb, so
/// [`REHOME_SUSTAINED_TICKS`] is what rejects it and this margin only has to clear
/// the p90. Widen it only with evidence; loosening the tick count is the change that
/// would actually let noise through.
const REHOME_MIN_GAIN_PCT: u32 = 15;
/// A steady hold (an eval, a detached solve) suppresses the OPTIONAL grow-back — a
/// re-home that would buy a few percent is not worth dropping a held slot for. It does
/// not get to pin a lane serving at a FRACTION of its plan: measured 2026-09-17 07:5xZ
/// on the M5, the boot plan came up 8 lanes × 12,800 under the outgoing server's memory
/// (card 40f53419), the plan wanted 3 × 67,340, and four re-fired control solves held
/// the lane steady for 255 ticks in a row — every one of the 16 minds, the solves
/// included, on a 12.8k window because the solves were "protecting" it. Past this gain
/// the re-home goes ahead and the probe says so; the held work retries its flickered
/// act (the deliberation retry budget exists for exactly one relaunch).
const REHOME_HOLD_OVERRIDE_PCT: u32 = 75;
/// A hold DELAYS a smaller re-home, it never denies it: after this many consecutive
/// held ticks (5 s each — about one act of held work) a sustained re-home goes ahead
/// anyway. Measured 2026-09-17 08:3xZ: with the window restored the knee moved 3 → 4
/// and the plan wanted a fourth lane (+33%, under the override), and the re-fired
/// solves' hold kept 16 minds on 3 lanes — the health line read STARVED for an hour
/// while the planner "owed lanes" it had already planned.
const REHOME_HOLD_DEFER_TICKS: u32 = 24;

/// How many consecutive ticks the plan must exceed the lane by
/// [`REHOME_MIN_GAIN_PCT`] before a re-home is justified.
///
/// Sustained-ness is the anti-thrash property the old single-sample `live * 2 <=
/// plan` ratio was reaching for, stated directly — and it is deliberately SHORT.
/// It is not what limits the relaunch RATE (that is [`REHOME_COOLDOWN_TICKS`],
/// enforced independently below); it only has to outlast jitter.
///
/// THE load-bearing parameter, and measured as such: across 6,322 live
/// `serving.plan` samples (probes.jsonl, 2026-08-06) the longest run of consecutive
/// qualifying rises was **2**, at a 10%, 15% or 20% margin alike — zero runs of 3 in
/// the entire sample. Real plan jitter spikes for a sample or two and falls back; it
/// does not climb. So 3 is the first value that provably rejects every observed
/// noise event, and 2 would not have. A dip RESETS the streak, so noise cannot
/// accumulate, while a genuine capacity change is honoured in 15s instead of being
/// stranded forever.
///
/// Lowering this to 2 re-admits the entire measured jitter tail. Do not, without
/// re-running that measurement against fresher receipts.
const REHOME_SUSTAINED_TICKS: u32 = 3;

/// One line per 5 s tick makes the decline probe the defect at the other end, so
/// it speaks on ENTRY and roughly once a minute thereafter.
///
/// `ticks` MUST be a counter that advances for each consecutive declined tick.
/// Returning false for 0 is deliberate: it makes "the clock never advanced" mean
/// SILENCE rather than "log forever", which is the exact failure #3751 shipped.
const DECLINE_LOG_EVERY_TICKS: u32 = 12; // 5 s ticks -> ~1/min
fn decline_should_log(ticks: u32) -> bool {
    ticks == 1 || (ticks > 0 && ticks % DECLINE_LOG_EVERY_TICKS == 0)
}

/// Re-home evidence: `(live_space, plan_space, gain, worth_it)` in TOTAL
/// token-space (window × lanes) — never the per-slot window alone.
///
/// Measured live 2026-09-01: the plan moved to 4 lanes × 47,426 while the
/// running server held 2 × 57,600 — 65% more total space and two more warm
/// slots, but the per-slot window SHRANK, so a window-only gain was negative
/// and the relaunch never fired. Five residents thrashed two slots at
/// hit_rate 0.0 until an operator cycled the server by hand. Lane count must
/// be part of the evidence
/// ([[the-same-bug-at-two-sites-is-a-missing-constraint-not-two-bugs]]).
///
/// Integer arithmetic that cannot overflow: u64 space on a 1M-token × 8-lane
/// server is ~8e6; ×100 still fits comfortably.
/// A steady hold yields when the plan's space exceeds the live space by
/// [`REHOME_HOLD_OVERRIDE_PCT`] or more — the hold protects a slot, never a starvation.
fn hold_overridden_by_shortfall(live_space: u64, gain: u64) -> bool {
    live_space > 0 && gain.saturating_mul(100) >= live_space.saturating_mul(REHOME_HOLD_OVERRIDE_PCT as u64)
}

fn rehome_gain_evidence(
    live_window: u32,
    live_lanes: u32,
    plan_window: u32,
    plan_lanes: u32,
) -> (u64, u64, u64, bool) {
    let live_space = (live_window as u64).saturating_mul(live_lanes.max(1) as u64);
    let plan_space = (plan_window as u64).saturating_mul(plan_lanes.max(1) as u64);
    let gain = plan_space.saturating_sub(live_space);
    let worth_it = live_space > 0
        && (gain.saturating_mul(100) >= live_space.saturating_mul(REHOME_MIN_GAIN_PCT as u64)
            || lane_over_its_knee(live_window, live_lanes, plan_window, plan_lanes));
    (live_space, plan_space, gain, worth_it)
}

/// The plan trades LANES for WIDTH: fewer slots, each at least as wide. That is the
/// decode knee clamping the roster (`inference::decode_knee`) — the live lane is
/// serving MORE streams than the box decodes at speed, every stream is slow, and the
/// only cure is a relaunch at the knee. Token-space says such a plan is "not a gain"
/// (8 × 51,712 → 7 × 67,340 is +14%, under the margin; → 4 × 67,340 is a LOSS), so
/// without this arm the knee only ever took effect at boot: measured 2026-09-17, the
/// M5 sat at 8 lanes / 7.9 t/s per stream all morning while the knee read 7 and the
/// reconcile declined every tick on the margin. The sustain streak and the cooldown
/// still gate the relaunch; this only makes a knee shrink COUNT as evidence.
fn lane_over_its_knee(live_window: u32, live_lanes: u32, plan_window: u32, plan_lanes: u32) -> bool {
    live_lanes > 0 && plan_lanes > 0 && plan_lanes < live_lanes && plan_window >= live_window
}

/// The knee's mirror: the plan wants MORE warm slots than the live lane serves. The plan
/// never asks for more lanes than the roster demands (`lane_cap` is the demand, then the
/// fit), so a plan with more lanes than are live is minds WITHOUT a warm slot — the #266
/// shape: every turn of an unslotted mind re-prefills cold. Token-space calls such a plan
/// a wash or a loss (measured 2026-09-19 15:4xZ, the M5: a cold boot launched 1 × 161,792
/// under a transient 44 GB budget; the steady plan said 2 × 67,340 — 17% LESS space —
/// and the reconcile declined it every tick, so a second coder lane could never appear
/// at runtime, and the settled 1-lane geometry then capped the next boot: a one-way
/// ratchet with no mover the other way). The sustain streak, the flat-plan check and the
/// cooldown still gate the relaunch; this only makes a slot shortfall COUNT as evidence.
/// A plan may shrink while the host pages out; it never grows into it. A grow is more
/// KV space than the lane holds; paging out is swap rising since the last decision.
/// `swap_prev == u64::MAX` is "no previous reading" and never reads as rising. The swap
/// LEVEL is not a term (Cormac on #4415): a page in swap already gave up its RAM, so
/// `available` reflects it, and macOS keeps idle pages swapped for hours after the
/// pressure is gone — subtracting the level would under-plan by the whole residue.
fn grow_refused_while_paging(
    live_space: u64,
    plan_space: u64,
    swap_prev: u64,
    swap_now: u64,
) -> bool {
    plan_space > live_space && swap_prev != u64::MAX && swap_now > swap_prev
}

fn roster_short_of_slots(live_lanes: u32, plan_lanes: u32) -> bool {
    live_lanes > 0 && plan_lanes > live_lanes
}

/// Consecutive plan ticks a base-model DOWNSHIFT must persist before it is
/// adopted. #368 (2nd occurrence, 2026-08-08): a ~6-second RAM transient at
/// agent/solve launch collapsed the planner's budget to zero for ONE tick, the
/// plan flipped to the 0.5B, reconcile actuated the flip (tore down Devstral,
/// re-homed every citizen), and the civilization lost 47 minutes to a phantom
/// model. The tear-down/spin-up/re-home cycle costs minutes, so reacting to a
/// squeeze shorter than [`TICK`]×this is ALWAYS a net loss — the highest-value
/// activity in the system must never be evicted by its own setup ripple (Joel:
/// "system working against rather than for the best activity work").
/// [[never-thrash-sticky-hysteresis-on-every-lane]]
const DOWNSHIFT_SUSTAINED_TICKS: u32 = 3;

/// Ticks a re-home must wait before another may fire, enforced INDEPENDENTLY of
/// the sustained-delta test above.
///
/// The two guards answer different questions and must not be collapsed into one:
/// sustained-delta asks "is this gain real?", the cooldown asks "may we pay for it
/// yet?". A relaunch kills every in-flight turn on the lane, so the rate limit has
/// to hold even when the evidence is perfect. Anchored to a lane's own readiness
/// budget ([`READY_TIMEOUT`], 90s) in ticks: never start a second re-home before
/// the first could possibly have come up and proven itself.
/// [[never-thrash-sticky-hysteresis-on-every-lane]]
const REHOME_COOLDOWN_TICKS: u32 = (READY_TIMEOUT.as_secs() / TICK.as_secs()) as u32;

/// Slow liveness HEARTBEAT cadence (#175 self-heal): reconcile ticks between decode
/// smoke-probes of the LIVE lane. The reconcile fast-path trusts the published `ready`
/// snapshot and never re-probes a child it owns ([`crate::inference::llama_server`] —
/// "trusted thereafter, no per-tick decode load"), so a Metal-GPU-OOM-POISONED backend —
/// which answers every control-plane read (`/health`, `/v1/models`, `/props`) with 200
/// while every `llama_decode` 500s (`kIOGPUCommandBufferCallbackErrorOutOfMemory`) — stays
/// `ready:true` FOREVER and the persona substrate is bricked until a human reboots. This
/// heartbeat re-verifies the actual COMPUTE path on a cadence far slower than [`TICK`], so
/// it costs one tiny generation per minute, not one per tick.
const HEALTH_PROBE_EVERY_TICKS: u64 = 12; // 12 × 5s ≈ 60s

/// Consecutive failed heartbeats before the lane is declared WEDGED and flipped
/// not-ready (which the reconcile turns into a kill+respawn). Pure hysteresis: ONE failed
/// probe can be a merely-BUSY lane (a saturated slot times out the 10s decode probe during
/// a wake burst), and reaping a healthy-but-busy lane is the exact thrash that got the
/// prior watchdog reverted (commit 93753d812). Two consecutive failures ≈ 2 minutes of
/// sustained no-decode = genuinely poisoned, not transiently loaded.
const HEALTH_FAILS_TO_RELAUNCH: u8 = 2;

/// Bus topic the live [`ServingSnapshot`] is emitted on whenever it changes.
/// Subscribers (embedding, supervisor, inference_session, ai_provider) declare
/// this in their `event_subscriptions` and cache the latest in `handle_event`
/// instead of probing the process — the cbar pipeline-stage shape: one organ
/// emits its state, everything that needs it subscribes. Because the bus spans
/// the grid, a remote lease allocator subscribes to the SAME topic. Routed by
/// name (no body parse in middleware), payload fans out as a shared pointer,
/// emitted only on a rare state change — never on the token hot path.
const SERVING_SNAPSHOT_EVENT: &str = "serving.snapshot";

/// VRAM headroom held below the governed serving budget when fitting expert LAYERS for K3
/// placement. `layer_bytes` sizes the stacked expert blob, but the `-ot`'d layer also carries
/// its router + norms and the per-layer size can drift; this margin keeps the fit honest. 512 MiB.
const EXPERT_PLACEMENT_MARGIN_BYTES: u64 = 512 * 1024 * 1024;
/// Hot-layer churn (symmetric difference vs the served set) that justifies a llama-server
/// RESPAWN. A respawn reloads every weight (seconds), so a 1–2 layer flap must not trigger one;
/// only a real residency shift (a task change moving > 2 expert layers) does.
const RELAUNCH_LAYER_CHURN_THRESHOLD: usize = 2;
/// Sticky-lease hysteresis band for the host-cache budget: 1/8 of the published
/// value (move only on >12.5% change). Same band shape as the plan hysteresis —
/// KV-sample flutter must not churn the plan file's mtime under her per-token poll.
const HOST_CACHE_LEASE_BAND_DIVISOR: u64 = 8;
/// Recency-window tokens carried on the published plan document. Inert while
/// `pin_list` is empty (v1 publishes budget-only); matches the golden v1 wire
/// fixture so the C++ consumer's parse sees the shape it was validated against.
// context-budget-exempt: the EXPERT PAGER's recency window (thousands of tokens of routing history for the pin list), part of the published plan wire format the C++ consumer validates against — not the prompt
const MOE_PLAN_WINDOW_K: u32 = 8;

/// Resolves a base-model id (as named in a [`ServingPlan`]) back to its full
/// [`Model`] struct. Production resolves through the global registry; tests
/// inject a fake so the reconcile WIRING can be exercised without a populated
/// registry. The resolved `Model` is carried straight onto the [`ServingTarget`]
/// — resolve once, pass the struct, never re-fetch ([[pass-the-model-struct-no-param-hell]]).
type ModelResolver = Arc<dyn Fn(&str) -> Option<Model> + Send + Sync>;

/// How the planner learns about a lane inherited from a PREVIOUS GENERATION of this core —
/// a past form of ourself, still resident, still holding its weights + KV.
///
/// This exists because of a measured boot defect (#438, glass-boxed 2026-08-19 from the probe
/// ledger). A core crashed with a healthy Qwen3.8-27B serving 4 lanes at a 25,075 window
/// (`serving.grow served_lanes=4 target_window=25075`). The successor booted while that lane was
/// STILL ALIVE holding ~45 GB, so the very first plan sampled `usable_gb=6` — an entirely HONEST
/// reading of free VRAM — and, seeing 6 GB, selected a 7B, then a 14B at `MIN_SERVE_CTX`. The old
/// lane was reclaimed 1.2 SECONDS LATER (`serving.lane_reclaim outcome=Reclaimed { pid: 11341 }`),
/// freeing all 45 GB; by then the downshift had already been actuated and the 14B was resident.
/// Ten seconds on, the planner was asking for the 27B again at `usable_gb=29..47` and could not
/// get back — a model swap needs the relaunch the plan had already spent.
///
/// The mechanism is NOT a bogus sample and NOT a reap-ordering race in the reclaim itself. It is
/// that BOTH anti-thrash defenses key on an incumbent, and boot has none:
///   * [`plan_serving_stable`]'s at-rest credit (which exists precisely so "a model's OWN
///     load/residency can never flap it out") never fires, because it credits back the
///     INCUMBENT's weights and `plan_tx` is empty on a fresh boot;
///   * [`downshift_gate`]'s sustained-ticks debounce (#368) is a no-op for the same reason —
///     with no incumbent there is nothing to debounce against.
///
/// So the one moment when a transient squeeze is GUARANTEED — the successor overlapping its
/// predecessor — is the exact moment every defense against transients is disabled. The comment
/// on [`ServingDaemonModule::publish_plan`] stated this as intent ("Boot's first plan has no
/// incumbent → plain selection"); it is the bug.
///
/// The fix is Joel's own framing of the governor: know our footprint, the system with us removed,
/// and **any past forms of ourself** — then work within that. An inherited lane is the third
/// category. It is ours, its memory is ours to reclaim, and it is therefore the incumbent the
/// first plan should reason against, not an external squeeze to flee from.
///
/// A seam and not a direct call to [`crate::inference::lane_registry::live_lane`] for the same
/// reason [`DecodeAgeSource`] is one: that function reads a filesystem singleton (the pidfile
/// plus `~/.continuum`), so under `cargo test` — one process, shared HOME — it would report the
/// OPERATOR's real 27B into unrelated unit tests. That exact leak was caught by measurement in
/// `serving_consumer` and fixed the same way.
type InheritedLaneSource = Arc<dyn Fn() -> Option<LaneRecord> + Send + Sync>;

/// Which model the plan should treat as the incumbent, given what we have already PUBLISHED and
/// what we INHERITED from a previous generation of this core.
///
/// A published plan always wins: once this core has decided, its own decision is the incumbent
/// and the inherited lane is either already adopted or already reclaimed. The inherited lane
/// only speaks for the window where `plan_tx` is still empty — the first plan after boot — which
/// is precisely the window the #438 downshift lives in.
///
/// Deliberately does NOT verify that the inherited model is still on disk or still servable:
/// [`plan_serving_stable`] already refuses an incumbent that is not among the candidates
/// (falling straight through to a plain plan), so a stale record degrades to today's behaviour
/// instead of pinning a model that no longer exists.
fn incumbent_for_plan(published: Option<String>, inherited: Option<&LaneRecord>) -> Option<String> {
    published.or_else(|| inherited.map(|rec| rec.model.clone()))
}

/// How the heartbeat learns whether a REAL generation delivered tokens recently — the input
/// to the "trust busy lanes, probe quiet ones" short-circuit in
/// [`ServingDaemonModule::spawn_health_heartbeat_if_due`]. Milliseconds since the last real
/// decode, or `None` for "no decode observed yet, go probe".
///
/// This is a seam and not a direct call to
/// [`crate::inference::llama_server::ms_since_real_decode`] because that function reads a
/// PROCESS-GLOBAL atomic. Under `cargo test` the whole crate shares one process, so an
/// unrelated test that stamps the global (`llama_server`'s own `note_real_decode` coverage
/// does exactly that) silently converts every later heartbeat test into a short-circuit —
/// a red that a filtered local run can never reproduce, because the filter excludes the
/// test doing the stamping. Owning the evidence source per-daemon makes each test's answer
/// its own; production still reads the global, through the default below.
type DecodeAgeSource = Arc<dyn Fn() -> Option<u64> + Send + Sync>;
type LeasedInSentSource = Arc<dyn Fn() -> Vec<u32> + Send + Sync>;

/// Where the heartbeat reads "consecutive REAL generations that failed on the live lane
/// since the last success" (#363). Defaults to the llama-server process-global stamped by
/// the adapter's local-lane error paths; tests inject their own so a parallel test that
/// stamps the global cannot leak order-dependence in
/// ([[a-process-global-read-inside-a-decision-makes-tests-order-dependent]]).
type RealFailsSource = Arc<dyn Fn() -> u64 + Send + Sync>;

/// The verdict for force-serving one specific model on this host RIGHT NOW —
/// what [`ServingDaemonModule::pin_fit_checker`] returns and `serving/pin` gates
/// on. Carries the numbers so the command can fail loud with a NAMED shortfall
/// ("needs ~XGB, host budget is ~YGB") rather than a bare refusal.
pub struct PinFit {
    /// The host-fit plan for the model alone. `None` when the model has no GGUF
    /// on disk (not downloaded → not servable). `Some` with `fits_on_gpu = false`
    /// when it is on disk but won't fit a lane in the current budget.
    pub plan: Option<ServingPlan>,
    /// The model's on-disk weight bytes (0 when not downloaded).
    pub weights_bytes: u64,
    /// The host's usable serving budget right now, in bytes.
    pub budget_bytes: u64,
}

/// The cloneable synchronous fit-gate the `serving/pin` command holds — given a
/// candidate [`Model`], can this host force-serve it right now. Built by
/// [`ServingDaemonModule::pin_fit_checker`] over the live budget so it agrees
/// with the autonomic planner by construction.
pub type PinFitChecker = Arc<dyn Fn(&Model) -> PinFit + Send + Sync>;

/// A coherent process-local intent snapshot. Revision is not persisted across boots.
#[derive(Clone, Debug)]
pub struct ServingIntentSnapshot {
    pub revision: u64,
    pub pinned: Option<String>,
    pub suppressed: Arc<HashSet<String>>,
}

/// Mutation capability for the serving daemon's single intent watch.
#[derive(Clone)]
pub struct ServingIntent {
    state: watch::Sender<ServingIntentSnapshot>,
}

impl ServingIntent {
    pub fn new(pinned: Option<String>) -> Self {
        let (state, _) = watch::channel(ServingIntentSnapshot {
            revision: 0,
            pinned,
            suppressed: Arc::new(HashSet::new()),
        });
        Self { state }
    }

    pub fn snapshot(&self) -> ServingIntentSnapshot {
        self.state.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<ServingIntentSnapshot> {
        self.state.subscribe()
    }

    /// Explicit accepted requests count even when reaffirming the current value.
    /// Pressure retries only count when their effective value changes.
    pub fn set_pin(&self, pinned: Option<String>, explicit: bool) -> Option<String> {
        let mut previous = None;
        self.state.send_if_modified(|state| {
            previous = state.pinned.clone();
            if !explicit && previous == pinned {
                return false;
            }
            state.revision = state
                .revision
                .checked_add(1)
                .expect("serving intent revision exhausted"); // JUSTIFIED: refuse overflow rather than authorize an old revision again.
            state.pinned = pinned;
            true
        });
        previous
    }

    pub fn set_suppressed(&self, model: &str, suppressed: bool, explicit: bool) -> bool {
        let mut previous = false;
        self.state.send_if_modified(|state| {
            previous = state.suppressed.contains(model);
            if !explicit && previous == suppressed {
                return false;
            }
            state.revision = state
                .revision
                .checked_add(1)
                .expect("serving intent revision exhausted"); // JUSTIFIED: refuse overflow rather than authorize an old revision again.
            if suppressed {
                Arc::make_mut(&mut state.suppressed).insert(model.to_string());
            } else {
                Arc::make_mut(&mut state.suppressed).remove(model);
            }
            true
        });
        previous
    }
}

/// Internal provenance for the plan. The command still returns only `plan`.
#[derive(Clone, Debug)]
struct ServingPlanSnapshot {
    pub intent_revision: u64,
    pub plan: Option<ServingPlan>,
}

/// Exact local launch provenance acknowledged under one serving intent revision.
/// Eligibility is queried from the daemon; this receipt alone is not a loan or lease.
#[derive(Clone)]
pub struct VerifiedServingCapture {
    pub launch: crate::inference::llama_server::OwnedServingTarget,
    pub intent_revision: u64,
}

/// The existing reconcile gate, armed before spawning so abort-before-first-poll
/// cannot strand it. Every lifecycle operation, including empty-plan idle, owns it.
/// Where an in-flight reconcile operation is waiting — named, so a tick that skips past
/// it can say WHICH await holds the daemon (card c3c50e0d: the M5 went dark for six
/// minutes at boot beside a surviving engine; every tick and the planner skipped on the
/// gate in silence, and the evidence died with the kill that restored it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum ReconcileStep {
    None = 0,
    /// Waiting on the external endpoint's reachability probe.
    External = 1,
    /// Retiring the owned engine for an empty plan (`idle_if_current`).
    Retire = 2,
    /// `ensure_model_serving_if_current`: adopt the running engine or spawn and wait ready.
    Ensure = 3,
    /// Reading the ready engine's own per-slot window (`/props`).
    VerifyWindow = 4,
    /// Reading the ready engine's own slot count (`/props`).
    VerifyLanes = 5,
    /// Holding the gate for an academy teacher batch on the lane.
    AcademyBatch = 6,
    /// Training admission reading governed memory under the gate (aae8af55).
    TrainingAdmission = 7,
}
impl ReconcileStep {
    fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::External => "external_probe",
            Self::Retire => "retire_owned_engine",
            Self::Ensure => "ensure_serving",
            Self::VerifyWindow => "verify_window",
            Self::VerifyLanes => "verify_lanes",
            Self::AcademyBatch => "academy_batch",
            Self::TrainingAdmission => "training_admission",
        }
    }
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::External,
            2 => Self::Retire,
            3 => Self::Ensure,
            4 => Self::VerifyWindow,
            5 => Self::VerifyLanes,
            6 => Self::AcademyBatch,
            7 => Self::TrainingAdmission,
            _ => Self::None,
        }
    }
}

/// An in-flight reconcile past this age is WEDGED: named on the gate-skip probe with the
/// step it is in, so the next sample of the core knows which await to read. Twice the
/// engine's ready timeout: the longest single wait a healthy reconcile can hold.
// derived-or-floor: derived — 2 × READY_TIMEOUT (a launch may wait ready once, then verify).
const RECONCILE_WEDGE_BOUND: Duration = Duration::from_secs(2 * READY_TIMEOUT.as_secs());

/// Ticks a LIVE lane may sit with no published plan before it is said: the boot plan
/// lands within the first tick or two; three is past any honest transient.
// derived-or-floor: a floor — three ticks at TICK is well past the boot's own first plan.
const PLAN_NONE_LIVE_LANE_TICKS: u64 = 3;

/// A gate skip says something once per this interval, never per tick.
// derived-or-floor: a floor — one line per half minute is readable in a log and loud enough.
const RECONCILE_BUSY_SAY_EVERY: Duration = Duration::from_secs(30);

/// True when an operation of this age has outlived the bound a healthy reconcile can hold.
fn reconcile_wedged(age: Duration) -> bool {
    age > RECONCILE_WEDGE_BOUND
}

/// The bound is PER STEP (Cormac on #4421): an academy teacher batch holds the gate for
/// as long as its corpus takes — minutes, legitimately — so the single serving bound
/// would read every long batch as a wedge. A batch is `busy`, never `wedged`; the batch
/// has its own deadline and receipt. Every other step is bounded as a reconcile.
fn reconcile_wedged_at(step: ReconcileStep, age: Duration) -> bool {
    match step {
        ReconcileStep::AcademyBatch => false,
        _ => reconcile_wedged(age),
    }
}

struct ServingOperation {
    gate: Arc<AtomicBool>,
    started_ms: Arc<AtomicU64>,
    step: Arc<AtomicU8>,
}
impl ServingOperation {
    fn acquire(
        gate: Arc<AtomicBool>,
        started_ms: Arc<AtomicU64>,
        step: Arc<AtomicU8>,
    ) -> Option<Self> {
        gate.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        started_ms.store(EpochMs::now().millis(), Ordering::Release);
        step.store(ReconcileStep::None as u8, Ordering::Release);
        Some(Self {
            gate,
            started_ms,
            step,
        })
    }
    /// Name the await this operation is entering.
    fn step(&self, step: ReconcileStep) {
        self.step.store(step as u8, Ordering::Release);
    }
}
impl ServingDaemonModule {
    /// This daemon's lifecycle gate, for the boot path to publish (`LifecycleGate::set_global`).
    pub(crate) fn lifecycle_gate(&self) -> LifecycleGate {
        LifecycleGate {
            gate: self.reconciling.clone(),
            started_ms: self.reconcile_started_ms.clone(),
            step: self.reconcile_step.clone(),
            settled: self.plan_published.clone(),
        }
    }
}
impl Drop for ServingOperation {
    fn drop(&mut self) {
        self.step.store(ReconcileStep::None as u8, Ordering::Release);
        self.started_ms.store(0, Ordering::Release);
        self.gate.store(false, Ordering::Release);
    }
}

/// The serving lifecycle gate as a handle another owner can take (card aae8af55).
/// Training admission reads governed memory UNDER this gate: a relaunch or a placement
/// move holds it from the moment it frees the engine's memory until the new engine is
/// resident, so a free-memory read inside that window is refused rather than admitting
/// a trainer at a time nobody called (the 9/16 class: a 22 GB sidecar admitted inside a
/// relaunch). The gate is held only across the admission, not the run: once granted,
/// the training lease is what keeps serving out of that memory, and holding the gate
/// for hours would stop every relaunch on a node where training and serving co-reside.
#[derive(Clone)]
pub(crate) struct LifecycleGate {
    gate: Arc<AtomicBool>,
    started_ms: Arc<AtomicU64>,
    step: Arc<AtomicU8>,
    settled: Arc<AtomicBool>,
}

/// Why an admission may not decide now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GateRefusal {
    /// Serving has not published its first plan on this core: the authority may not
    /// hold a physical reading yet, so free memory is a boot artefact, not capacity.
    Unsettled,
    /// A serving operation (relaunch, placement move, teacher batch) holds the gate:
    /// free memory now is its window, not capacity.
    Operation,
}
impl GateRefusal {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Unsettled => "serving_unsettled",
            Self::Operation => "serving_operation",
        }
    }
}
static LIFECYCLE_GATE: OnceLock<LifecycleGate> = OnceLock::new();
impl LifecycleGate {
    /// Publish THE serving daemon's gate (first writer wins — the boot path). One serving
    /// lifecycle per core, same precedent as `ResourceDaemon::set_global`.
    pub(crate) fn set_global(gate: LifecycleGate) {
        let _ = LIFECYCLE_GATE.set(gate);
    }
    /// The published gate, if boot installed a serving daemon (None in bare unit tests).
    pub(crate) fn global() -> Option<LifecycleGate> {
        LIFECYCLE_GATE.get().cloned()
    }
    /// A gate no serving daemon owns, for admission tests.
    /// A gate no serving daemon owns, for admission tests; `settled` as its planner would be.
    #[cfg(test)]
    pub(crate) fn unowned(settled: bool) -> Self {
        Self {
            gate: Arc::new(AtomicBool::new(false)),
            started_ms: Arc::new(AtomicU64::new(0)),
            step: Arc::new(AtomicU8::new(0)),
            settled: Arc::new(AtomicBool::new(settled)),
        }
    }
    /// What a test's planner does on its first publish.
    #[cfg(test)]
    pub(crate) fn settle(&self) {
        self.settled.store(true, Ordering::Release);
    }
    /// Take the gate for one admission decision, or say why it cannot decide now.
    pub(crate) fn hold_for_admission(&self) -> Result<AdmissionHold, GateRefusal> {
        if !self.settled.load(Ordering::Acquire) {
            return Err(GateRefusal::Unsettled);
        }
        let operation = ServingOperation::acquire(
            self.gate.clone(),
            self.started_ms.clone(),
            self.step.clone(),
        )
        .ok_or(GateRefusal::Operation)?;
        operation.step(ReconcileStep::TrainingAdmission);
        Ok(AdmissionHold {
            _operation: operation,
        })
    }
}

/// The gate, held while one admission reads and leases; dropping it releases the gate.
pub(crate) struct AdmissionHold {
    _operation: ServingOperation,
}

/// A forced health check stays owed if its task is cancelled or superseded.
/// Completion never clears a newer report raised while this claim was in flight.
struct ForcedProbe {
    flag: Arc<AtomicBool>,
    pending: bool,
}
impl ForcedProbe {
    fn claim(flag: Arc<AtomicBool>) -> Self {
        let pending = flag.swap(false, Ordering::AcqRel);
        Self { flag, pending }
    }
}
impl Drop for ForcedProbe {
    fn drop(&mut self) {
        if self.pending {
            self.flag.store(true, Ordering::Release);
        }
    }
}

fn verified_capture(
    server: &dyn LlamaServerControl,
    snapshot: &ServingSnapshot,
    intent_revision: u64,
) -> Option<VerifiedServingCapture> {
    if !snapshot.ready || server.paging_recovery_required() {
        return None;
    }
    let launch = server.owned_serving_target()?;
    if snapshot.active_model.as_deref() != Some(launch.target.model.id.as_str())
        || snapshot.adapters != launch.target.adapter_paths()
        || snapshot.served_context_window != launch.observed_context_window
        || snapshot.lanes != launch.observed_lanes
        || snapshot.host_prompt_cache_mib != launch.launched_host_prompt_cache_mib
        || server.owned_engine()? != launch.identity
    {
        return None;
    }
    Some(VerifiedServingCapture {
        launch,
        intent_revision,
    })
}

pub struct ServingDaemonModule {
    gpu: Arc<GpuMemoryManager>,
    /// Live system memory monitor — the budget comes from what's actually FREE
    /// right now (`available_bytes`), not total capacity. On unified memory
    /// this drops when anything else grabs memory (a game, a build), so the
    /// plan ebbs and flows organically.
    system: Arc<SystemResourceMonitor>,
    /// The ONE per-machine resource authority (#56). Serving reads the GOVERNED
    /// VRAM headroom — capacity net of every external consumer (Bevy, LiveKit)
    /// and every outstanding lease — from this board, instead of a fraction of
    /// *total* VRAM blind to those consumers. That blindness was the
    /// `host_budget()` OOM bug; the board's `available(Vram)` is the precise
    /// net-of-everyone ceiling that fixes it. Mandatory (no Option): a host
    /// with no governor has no honest VRAM ceiling, and serving must fail
    /// closed rather than over-commit.
    resource_daemon: Arc<ResourceDaemon>,
    /// The published decision. `None` until the first successful plan. Held as
    /// the module's only shared state; `send` takes `&self` so `tick()` can
    /// publish without interior-mutability gymnastics.
    plan_tx: watch::Sender<ServingPlanSnapshot>,
    /// Existing read-only wire/grid projection; never used to authorize reconciliation.
    plan_view_tx: watch::Sender<Option<ServingPlan>>,
    /// The serving-control leaf: owns the supervised `llama-server` child and
    /// reconciles it to the plan. A trait object so tests inject a fake; in
    /// production it is a `LlamaServerProcess` (which kills its child on Drop —
    /// so the daemon owning it means the daemon owns the server's lifetime).
    server: Arc<dyn LlamaServerControl>,
    /// The published serving state — which model is live, is it ready, on what
    /// `/v1` url. Adapters read THIS instead of probing the process; a grid
    /// allocator reads it to contract `(model, genome)` leases. Node down →
    /// empties; node up → republished. The grid seam.
    serving_tx: watch::Sender<ServingSnapshot>,
    /// Gate so at most one reconcile (which may spawn + wait for model load) is
    /// in flight. A tick that finds a reconcile already running skips — no
    /// stacked relaunches thrashing the GPU.
    reconciling: Arc<AtomicBool>,
    /// When the in-flight reconcile began (ms; 0 = none) and which await it is in
    /// (`ReconcileStep`), written by the operation, read by whoever skips on the gate.
    reconcile_started_ms: Arc<AtomicU64>,
    reconcile_step: Arc<AtomicU8>,
    /// When the gate skip last spoke (ms), so a busy reconcile is said once per
    /// [`RECONCILE_BUSY_SAY_EVERY`], never per tick.
    reconcile_busy_said_ms: Arc<AtomicU64>,
    /// Set once this core's planner has published its first decision (a plan or an honest
    /// none). Until then the authority may not yet hold a physical reading — a source
    /// with no reading reports 0 used, so the whole card reads free — and training
    /// admission is refused (card aae8af55, the boot-empty window).
    plan_published: Arc<AtomicBool>,
    academy_batch: parking_lot::Mutex<Option<academy_batch::BatchSlot>>,
    verified_target: watch::Sender<Option<VerifiedServingCapture>>,
    /// Reconcile-tick counter driving the slow liveness HEARTBEAT (fires when
    /// `% HEALTH_PROBE_EVERY_TICKS == 0`). See [`Self::spawn_health_heartbeat_if_due`].
    health_ticks: Arc<AtomicU64>,
    /// Consecutive ticks with NO published plan while a lane is LIVE (owned or inherited)
    /// — the 2026-09-26 18:22Z shape: a refused demotion left plan None beside a healthy
    /// engine, and no receipt could see it (the reconcile receipt needs an operation in
    /// flight; there was none). Said once per [`RECONCILE_BUSY_SAY_EVERY`] after
    /// [`PLAN_NONE_LIVE_LANE_TICKS`]; reset the tick a plan exists.
    plan_none_live_lane_ticks: Arc<AtomicU64>,
    plan_none_said_ms: Arc<AtomicU64>,
    /// Consecutive failed decode heartbeats — the hysteresis counter that keeps a
    /// merely-BUSY lane from being reaped. Reset to 0 on any passing probe or when the
    /// lane isn't believed-ready. Relaunch only once it reaches [`HEALTH_FAILS_TO_RELAUNCH`].
    health_fails: Arc<AtomicU8>,
    /// At most one heartbeat decode-probe in flight (a probe can take up to
    /// `DECODE_SMOKE_TIMEOUT` under load, longer than one [`TICK`]); a tick that finds one
    /// running skips, exactly like `reconciling`.
    health_probing: Arc<AtomicBool>,
    /// A prefill-knee `/slots` read is in flight ([`Self::spawn_prefill_knee_read`]).
    prefill_knee_reading: Arc<AtomicBool>,
    /// Set by the liveness heartbeat when it declares the live lane WEDGED, read+cleared by
    /// the next [`Self::reconcile_to_plan`]. It forces `ensure_model_serving`'s decode probe
    /// even on a child we own — otherwise the "trusted thereafter" short-circuit would
    /// re-adopt the poisoned-but-alive lane forever instead of relaunching it (#175).
    force_relaunch: Arc<AtomicBool>,
    /// The message bus, captured at `initialize`. Set once; `None` in tests
    /// constructed via `with_control` without a live runtime, so the emit is a
    /// silent no-op there. The daemon publishes [`ServingSnapshot`] changes on
    /// `SERVING_SNAPSHOT_EVENT` so any subscriber (local or grid) gets the live
    /// serving state pushed — no point-to-point receiver plumbing.
    bus: OnceLock<Arc<MessageBus>>,
    /// Maps a planned base-model id → its full [`Model`] for the reconcile to
    /// carry onto the [`ServingTarget`]. Defaults to the global registry; tests
    /// override it ([`Self::set_model_resolver`]).
    model_resolver: ModelResolver,
    /// Where the liveness heartbeat gets "how long since a real decode" from. Defaults to
    /// the llama-server process-global; tests own it ([`Self::set_decode_age_source`]).
    decode_age: DecodeAgeSource,
    /// The prompt sizes of the generates this seat served for OTHER nodes — the
    /// leased-in sample ring. Production reads `resource_admission`'s process-global ring;
    /// tests own it ([`Self::set_leased_in_sent_source`]): a ring shared by every test in
    /// the binary made one test's 56k samples another test's requirement (CI, 2026-09-20).
    leased_in_sent: LeasedInSentSource,
    /// The `/slots` activity fingerprint observed at the LAST missed smoke probe (L11).
    /// A changed fingerprint at the next miss proves the serve loop advanced between the
    /// two looks — work the adapter stamps can't see (ghost turns from a dead core's
    /// clients, a fresh boot's un-stamped atomics) — so the miss is queue contention,
    /// not wedge evidence. Cleared on every smoke success so a stale value can never
    /// exonerate a later freeze. `std::sync::Mutex` — held only for a copy, never
    /// across an await.
    last_miss_slots_fp: Arc<std::sync::Mutex<Option<u64>>>,
    /// Where the first plan after boot learns about a PAST FORM OF OURSELF still holding VRAM.
    /// Defaults to the lane registry; tests own it ([`Self::set_inherited_lane`]). See
    /// [`InheritedLaneSource`] for the measured defect this closes (#438).
    inherited_lane: InheritedLaneSource,
    /// Where the heartbeat reads the real-turn failure streak (#363). Sustained real
    /// failure is wedge evidence that OUTRANKS a passing smoke probe — the undersized-slot
    /// wedge class rejects the fleet's real prompts while a tiny probe still succeeds.
    real_fails: RealFailsSource,
    /// The last HEALTHY lane's (window, lanes) — the #363 prevention half. The #175
    /// sticky floor reads the live snapshot, but declaring a wedge EMPTIES that snapshot
    /// first, so every wedge-heal relaunch bypassed the floor by construction and spawned
    /// at whatever the teardown-transient plan said (the dying predecessor still holds
    /// its memory when the successor's budget samples free RAM — 2026-08-07: plan dipped
    /// to 9984 mid-teardown, successor spawned undersized, every 12k prompt rejected).
    /// These survive the empty snapshot; 0 = no healthy lane observed yet. Memory-safe:
    /// the predecessor's window IS the memory the successor inherits, and the existing
    /// overflow protection remains the backstop if it genuinely no longer fits.
    last_healthy_window: Arc<AtomicU32>,
    /// The discrete footprint arm's baseline: the TROUGH of device VRAM in use between
    /// the predecessor's reclaim and the successor's load. Armed to `u64::MAX` at spawn,
    /// lowered to the minimum sampled value on every tick until the lane is ready; the
    /// delta at a later tick is then the lane's own bytes. A single read at the spawn
    /// instant was the first cut and it read the DYING predecessor's ~24 GB off the
    /// board's sampled figure (2026-09-17 13:1xZ: delta 0) — a stale sample, not a
    /// baseline. `MAX` = armed but never lowered; 0 = no spawn observed (an adopted
    /// lane); both make the arm say could-not-look rather than charge the desktop.
    spawn_baseline_vram: Arc<AtomicU64>,
    /// What the serving engine reports it holds on a discrete GPU, recorded on the footprint
    /// tick and credited to serving on the memory board (card 741f5eec). Shared with the
    /// `ServingConsumer` this daemon registers.
    engine_device_credit: Arc<crate::modules::serving_consumer::EngineDeviceCredit>,
    last_healthy_lanes: Arc<AtomicU32>,
    /// The LIVE model universe — the SAME `Arc<ModelCatalog>` the `models/*`
    /// command surface mutates. The daemon plans off this snapshot, NOT the
    /// immutable seed registry, so a model acquired at runtime (`models/pull`
    /// flips it to [`Availability::Ready`] with its real on-disk path) becomes a
    /// serving candidate on the very next tick — no reboot. This is the consumer
    /// side of the rich API: serving reacts to the universe changing.
    catalog: Arc<ModelCatalog>,
    /// The persisted operator pin, as a handle this daemon and its `serving/pin` ·
    /// `serving/unpin` commands share. Handed in at construction so a test daemon
    /// owns a tempdir store and can never read or write the machine's real pin.
    pin_store: crate::modules::serving_pin_store::ServingPinStore,
    /// K3 expert-residency state for the CURRENTLY-served MoE model, `(model_id, context)`.
    /// Built lazily on the first reconcile that serves a given MoE model and rebuilt when the
    /// served model changes; `None` for a dense model or before the first MoE reconcile. A
    /// `std::sync::Mutex` because [`Self::reconcile_to_plan`] is synchronous and this is never
    /// held across an await. The pager inside owns the live-hit observer + the gate seed; each
    /// reconcile ticks it to decide the served expert-layer placement.
    moe_serving:
        std::sync::Mutex<Option<(String, crate::capacity::moe_serving::MoeServingContext)>>,
    /// Sticky publisher state for the governed host-cache lease (#287 slice 2) — the
    /// never-thrash layer between the per-tick raw derivation
    /// ([`host_cache_lease_bytes`](crate::capacity::host_cache_lease::host_cache_lease_bytes))
    /// and the plan-file write her per-token mtime poll consumes. Sub-band KV jitter
    /// never republishes; a material shrink publishes NOW; grow-back publishes past
    /// the band (#214). Same sync-Mutex discipline as `moe_serving` (never across await).
    host_cache_lease: std::sync::Mutex<crate::capacity::host_cache_lease::StickyLease>,
    /// Sticky publisher state for the DEVICE-side expert-slot budget (#305;
    /// BigMama's measured ask): free-VRAM-after-device-fit, from the live
    /// resource board (the serve's own residency is already a registered
    /// consumer per #79, so the board's available IS post-fit), less the
    /// placement margin. Same band + same never-thrash discipline as the
    /// host lease — raw VRAM flutters every tick; the published budget
    /// moves only on material change. Warm coverage scales directly with
    /// this (measured 2026-08-02: 13.8% @ 1.9 GiB → 65.7% @ 30 GiB).
    device_budget_lease: std::sync::Mutex<crate::capacity::host_cache_lease::StickyLease>,
    /// The artifact path currently registered in the process-wide
    /// [`serving_active_artifacts`](crate::system_resources::serving_active_artifacts)
    /// set (#302 invariant 1: the NvmeServingTierPool must never migrate the
    /// resident model out from under the engine). Tracked so a model change
    /// releases the old registration exactly once.
    active_artifact: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
    /// The pin actuator's observation state (#281): tails the fork's
    /// routed-expert trace, owns the bandit, and remembers the last
    /// published pin list (the write-churn gate). `None` until the first
    /// MoE lease publish; rebuilt on geometry change. Same sync-Mutex
    /// discipline as `moe_serving` (never held across await).
    moe_trace_tail: std::sync::Mutex<Option<crate::capacity::trace_tail::MoeTraceTail>>,
    /// Division actuator (#2 of the resident/cache split, contract 2026-08-03): tier
    /// catalog discovered from the `--resident-only` manifests + warm-started
    /// `DivisionBandit` + the trace-tail reward watermark, for the ACTIVE MoE serve.
    /// Chooses which RESIDENT precision the next relaunch should load (`resident_tier`
    /// on the governed plan); the LIVE device budget stays #305's board-derived axis —
    /// one budget authority, one tier authority. Rebuilt on model change; `None` until
    /// a MoE serve exists. Same sync-Mutex discipline (never held across await).
    division: Arc<std::sync::Mutex<Option<crate::capacity::division_actuation::DivisionActuator>>>,
    /// The resident-override path the LAST reconcile applied to the spawn — ground truth
    /// for which tier the RUNNING serve actually loaded. Division rewards credit this
    /// tier, never the bandit's latest (unlaunched) choice — two-speed honesty.
    served_resident: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
    /// MEASUREMENT-ONLY, off by default: an explicit forced VRAM budget for K3 expert
    /// placement, read ONCE at construction from `K3_MEASURE_FORCE_EXPERT_BUDGET_BYTES`. When
    /// `Some`, it OVERRIDES the governed ceiling so a model that would otherwise fit is driven
    /// to spill expert layers onto CPU — the only way to measure the B-gate (does cold-expert
    /// CPU-compute dominate decode?) without a model that genuinely overflows. It is an
    /// experimental control, NOT a substrate threshold: `None` in all normal operation, and
    /// every placement made under it is flagged on the `serving.k3_placement` probe so the
    /// numbers are never mistaken for real capacity. [[k3-slice2-A-vs-B-decision]]
    measure_force_expert_budget_bytes: Option<u64>,
    /// One coherent revisioned pin/suppression authority, shared with commands and
    /// pressure reclaim. Readers never reconstruct intent from separate watches.
    intent: ServingIntent,
    /// How many minds actually need a concurrent serving lane — the persona
    /// floor, set by the boot wiring BEFORE the first plan and updated if the
    /// population changes. Lanes come from DEMAND ([`plan_serving`] docs):
    /// llama-server splits `-c` evenly across slots, so every lane nobody
    /// asked for divides every mind's window for nothing (the 4-slots-for-2-
    /// personas starvation, 2026-07-10). Default 1 — window-first.
    lane_demand: Arc<std::sync::atomic::AtomicU32>,
    /// Consecutive ticks the PLAN has exceeded the live lane by at least
    /// [`REHOME_MIN_GAIN_PCT`]. Reset the moment it does not. Sustained-ness is
    /// what separates a real capacity change from memory jitter — see
    /// [`REHOME_SUSTAINED_TICKS`].
    rehome_streak: Arc<std::sync::atomic::AtomicU32>,
    /// Consecutive ticks a sustained re-home has been deferred by a steady hold.
    rehome_held_ticks: Arc<std::sync::atomic::AtomicU32>,
    /// The host's swap-in-use at the last grow decision (card 628dc958): a grow-back
    /// re-home is refused while this is rising. `u64::MAX` = no reading yet.
    last_swap_used: Arc<std::sync::atomic::AtomicU64>,

    /// How many CONSECUTIVE ticks the lane has been below plan while declining to
    /// re-home — a clock for the decline probe's cadence and nothing else.
    ///
    /// It exists because #3751 tried to throttle that probe off `rehome_streak`
    /// and silently did not throttle at all. `rehome_streak` counts consecutive
    /// ticks of a QUALIFYING GAIN; the declined state is precisely the state where
    /// no gain qualifies, so it is pinned at 0 for the whole episode and
    /// `streak <= 1` stayed true forever. Measured after that "fix" merged: 411
    /// identical lines, one per 5 s tick. A domain counter is not a logging clock —
    /// its reset rule belongs to the domain, and here the domain resets it exactly
    /// when we wanted it to count.
    decline_log_ticks: Arc<std::sync::atomic::AtomicU32>,
    /// The highest plan window seen in the current re-home streak (0 = none yet).
    /// "Sustained" requires the plan to have stopped CLIMBING: a new high resets
    /// the streak, so the boot staircase coalesces into one relaunch at its top.
    /// A plan that only alternates below a high it already reached is settled
    /// enough: both values want the bigger lane (2026-10-07: a lane at 2,304 under
    /// a plan alternating 33k/70k never re-homed for an hour, because every rise
    /// reset the streak).
    rehome_plan_high: Arc<std::sync::atomic::AtomicU64>,
    /// L10 (#438): consecutive plan ticks wanting a DIFFERENT base model than the
    /// ready incumbent, and which model that was. A model swap re-homes every
    /// persona to different weights, so it earns the same sustained-streak bar as
    /// the window re-home above — a one-tick nothing-fits fallback (bogus
    /// usable_bytes sample) must never actuate a swap.
    model_change_streak: Arc<std::sync::atomic::AtomicU32>,
    pending_model_change: Arc<std::sync::Mutex<Option<String>>>,
    /// Consecutive ticks the fresh plan has wanted a LESS capable base model
    /// than the incumbent (a DOWNSHIFT). Reset the moment it stops. See
    /// [`DOWNSHIFT_SUSTAINED_TICKS`] — #368's second occurrence: a ~6-second
    /// RAM transient at solve launch read as usable_gb 17→0, one depressed tick
    /// flipped the PLAN to the 0.5B, and from then on hysteresis defended the
    /// WRONG incumbent. A brain flip must outlast jitter to be believed.
    downshift_streak: Arc<std::sync::atomic::AtomicU32>,
    /// (decision-fingerprint, served_window) of the last `serving.plan` probe
    /// actually emitted. The plan recomputes every tick, but the PROBE fires
    /// only when a DECISION changes or the window moves past a relative
    /// deadband (event-based law: emit on change, never per tick). Measured
    /// 2026-08-14: per-tick emission put 1,585 identical rows into 600s of
    /// probe stream — 51% of ALL rows — which rotated real history away in
    /// minutes and made every incident archeological (#399). Steady state is
    /// now silence; the live value stays queryable on demand via `serving/plan`.
    last_plan_probe: Arc<std::sync::Mutex<Option<(String, u32)>>>,
    /// Ticks remaining before another window re-home may fire. Charged to
    /// [`REHOME_COOLDOWN_TICKS`] when one fires, decremented every reconcile.
    /// Counted in ticks rather than wall-clock deliberately: the rate limit is on
    /// the daemon's own decision cadence, so it stays exact under a stopped clock,
    /// a suspended host, or a test that drives ticks by hand.
    rehome_cooldown: Arc<std::sync::atomic::AtomicU32>,
    /// The `ready_verified_at_ms` stamp of the launch the reconcile last settled on. A
    /// stamp it has not seen is a lane that just came READY, and a launch settles AFTER
    /// it is ready — see [`ready_edge`]. 0 = none yet.
    settled_ready_at_ms: Arc<std::sync::atomic::AtomicU64>,
    /// Per-persona MEASURED turn demand — the window half of `serving_demand()`.
    /// Personas write (their deliberation faculty records every turn's unclamped
    /// cost); this daemon reads the ceiling when it plans. Replaces the
    /// `BOOTSTRAP_WORKING_SET` constant that used to stand in for this measurement
    /// and capped every citizen at 8192 tokens of a 128k-capable model.
    working_set: crate::cognition::working_set::WorkingSetRegistry,
    /// The long-lived vision SIDECAR lane (#106, `inference::vision_sidecar`):
    /// a small VL model serving beside a text-only mind so every persona has
    /// eyes. Owned here so it lives across reconciles and its child dies with
    /// the daemon (`EphemeralServingLane` Drop-kills). A tokio Mutex because
    /// [`vision_sidecar::ensure_sidecar`] awaits (spawn + `/props` verify)
    /// inside the reconcile task while holding the slot.
    vision_sidecar:
        Arc<tokio::sync::Mutex<Option<crate::inference::llama_server::EphemeralServingLane>>>,
}

/// L10 (#438) pure gate: may the reconcile commit a BASE-MODEL swap this tick?
///
/// A swap tears down the lane and re-homes every persona to different weights, so a
/// plan that names a different model than the READY incumbent must persist for
/// `needs` consecutive ticks. Changing the *target* mid-streak restarts the count
/// (a plan flapping 0.5B->27B->0.5B never accumulates); a tick where plan == live
/// clears any pending change. Boot / wedge-recovery (no ready incumbent) commits
/// immediately — recovery must not wait out a streak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelChangeGate {
    /// Not a ready-lane model change (same model, or no ready incumbent) — proceed.
    NotAChange,
    /// A change, but not yet sustained — do nothing this tick.
    Defer { streak: u32 },
    /// Sustained disagreement — commit the swap (streak consumed).
    Commit { streak: u32 },
}

fn model_change_gate(
    live_ready: bool,
    live_model: Option<&str>,
    desired: &str,
    pending: &mut Option<String>,
    streak: &std::sync::atomic::AtomicU32,
    needs: u32,
) -> ModelChangeGate {
    use std::sync::atomic::Ordering;
    if !(live_ready && live_model.is_some() && live_model != Some(desired)) {
        streak.store(0, Ordering::Relaxed);
        *pending = None;
        return ModelChangeGate::NotAChange;
    }
    let n = if pending.as_deref() == Some(desired) {
        streak.fetch_add(1, Ordering::Relaxed).saturating_add(1)
    } else {
        *pending = Some(desired.to_string());
        streak.store(1, Ordering::Relaxed);
        1
    };
    if n < needs {
        ModelChangeGate::Defer { streak: n }
    } else {
        streak.store(0, Ordering::Relaxed);
        *pending = None;
        ModelChangeGate::Commit { streak: n }
    }
}

impl ServingDaemonModule {
    pub fn new(
        gpu: Arc<GpuMemoryManager>,
        system: Arc<SystemResourceMonitor>,
        resource_daemon: Arc<ResourceDaemon>,
        catalog: Arc<ModelCatalog>,
        pin_store: crate::modules::serving_pin_store::ServingPinStore,
    ) -> Self {
        Self::with_control(
            gpu,
            system,
            resource_daemon,
            Arc::new(LlamaServerProcess::new()),
            catalog,
            pin_store,
        )
    }

    /// Construct with an injected serving control. Production uses
    /// [`Self::new`] (real `LlamaServerProcess`); tests inject a fake to drive
    /// the reconcile decision without a live process. `catalog` is the SAME live
    /// universe the `models/*` commands mutate — the daemon plans off its
    /// snapshot so runtime acquisitions become servable without a reboot.
    pub fn with_control(
        gpu: Arc<GpuMemoryManager>,
        system: Arc<SystemResourceMonitor>,
        resource_daemon: Arc<ResourceDaemon>,
        server: Arc<dyn LlamaServerControl>,
        catalog: Arc<ModelCatalog>,
        pin_store: crate::modules::serving_pin_store::ServingPinStore,
    ) -> Self {
        let (plan_tx, _rx) = watch::channel(ServingPlanSnapshot {
            intent_revision: 0,
            plan: None,
        });
        let (plan_view_tx, _) = watch::channel(None);
        let (serving_tx, _srx) = watch::channel(ServingSnapshot::empty());
        let (verified_target, _) = watch::channel(None);
        // The operator's pin is durable intent: seed the channel from the store so the
        // FIRST plan is computed under it (cards 3160b3d0 / 9552a01e — every reboot
        // used to lose the pin and the boot planner served whatever fit).
        // A RESTORED PIN IS VERIFIED BEFORE IT IS HONOURED.
        //
        // `serving/pin` fit-gates at set time, so a pin to an undownloaded model cannot
        // be created through the command. A pin RESTORED from disk has had no such
        // check — the world moved since it was written (weights evicted, a drive
        // unmounted, the file edited, a downgrade that wrote a row this build reads
        // differently), and the planner honours it absolutely: every downloaded
        // candidate is refused as "another model is pinned" and NOTHING loads.
        //
        // Measured twice on this host (2026-09-09/10, #3955): a pin to
        // `AtomicChat/Qwen3.8-Flash-Next-GGUF`, `availability: not_downloaded`, took the
        // node to zero lanes and zero citizens with NOTHING red — the planner correctly
        // honoured the pin, correctly refused non-pinned candidates, correctly declined
        // to load absent weights, and the machine was useless. Every component behaved
        // as designed. Only `serving/unpin` by hand recovered it.
        //
        // The pin FILE is deliberately left alone: it is durable operator intent, and
        // the model may be pulled later. It is simply not honoured this boot. So the
        // node self-heals to autonomic serving, says loudly why, and re-honours the pin
        // on the first boot after the weights arrive.
        let stored_pin = pin_store.load();
        let honoured = match stored_pin {
            None => {
                crate::probe!(class = "serving.pin.none_stored", "no stored serving pin — the planner chooses");
                None
            }
            Some(pin) => {
                let snapshot = catalog.snapshot();
                let servable = snapshot
                    .get(&pin.model_id)
                    .is_some_and(|entry| entry.status.availability == Availability::Ready);
                if servable {
                    crate::probe!(
                        class = "serving.pin.restored",
                        model_id = %pin.model_id,
                        set_at_ms = pin.set_at_ms,
                        "serving pin restored from the state dir before the first plan"
                    );
                    Some(pin.model_id)
                } else {
                    crate::probe!(
                        class = "serving.pin.unservable_ignored",
                        model_id = %pin.model_id,
                        set_at_ms = pin.set_at_ms,
                        "the stored serving pin names a model this host cannot serve (absent                          from the catalog, or catalogued but not downloaded) — IGNORING it for                          this boot so the planner can serve something real. The pin file is                          kept: pull the weights and it is honoured again on the next boot, or                          run serving/unpin to drop the intent."
                    );
                    None
                }
            }
        };
        let intent = ServingIntent::new(honoured);
        Self {
            verified_target,
            gpu,
            system,
            resource_daemon,
            plan_tx,
            plan_view_tx,
            server,
            serving_tx,
            reconciling: Arc::new(AtomicBool::new(false)),
            reconcile_started_ms: Arc::new(AtomicU64::new(0)),
            reconcile_step: Arc::new(AtomicU8::new(0)),
            reconcile_busy_said_ms: Arc::new(AtomicU64::new(0)),
            plan_published: Arc::new(AtomicBool::new(false)),
            academy_batch: parking_lot::Mutex::new(None),
            health_ticks: Arc::new(AtomicU64::new(0)),
            plan_none_live_lane_ticks: Arc::new(AtomicU64::new(0)),
            plan_none_said_ms: Arc::new(AtomicU64::new(0)),
            health_fails: Arc::new(AtomicU8::new(0)),
            health_probing: Arc::new(AtomicBool::new(false)),
            prefill_knee_reading: Arc::new(AtomicBool::new(false)),
            force_relaunch: Arc::new(AtomicBool::new(false)),
            bus: OnceLock::new(),
            // Production resolver: the global registry. `try_global` (not the
            // panicking `global`) so a not-yet-initialized registry resolves to
            // None → honest empty snapshot, never a panic in the reconcile path.
            model_resolver: Arc::new(|id: &str| {
                crate::model_registry::try_global().and_then(|r| r.model(id).cloned())
            }),
            decode_age: Arc::new(crate::inference::llama_server::ms_since_real_work),
            leased_in_sent: Arc::new(crate::cognition::resource_admission::leased_in_sent_samples),
            last_miss_slots_fp: Arc::new(std::sync::Mutex::new(None)),
            inherited_lane: Arc::new(crate::inference::lane_registry::live_lane),
            real_fails: Arc::new(crate::inference::llama_server::consecutive_real_decode_failures),
            last_healthy_window: Arc::new(AtomicU32::new(0)),
            spawn_baseline_vram: Arc::new(AtomicU64::new(0)),
            engine_device_credit: Arc::new(crate::modules::serving_consumer::EngineDeviceCredit::default()),
            last_healthy_lanes: Arc::new(AtomicU32::new(0)),
            catalog,
            pin_store,
            intent,
            lane_demand: Arc::new(std::sync::atomic::AtomicU32::new(1)),
            rehome_streak: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            rehome_held_ticks: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            last_swap_used: Arc::new(std::sync::atomic::AtomicU64::new(u64::MAX)), // MAX = no reading yet: unknown is never "rising"
            decline_log_ticks: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            rehome_plan_high: Arc::new(std::sync::atomic::AtomicU64::new(0)), // 0 = no plan seen in this streak yet
            model_change_streak: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            pending_model_change: Arc::new(std::sync::Mutex::new(None)),
            rehome_cooldown: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            settled_ready_at_ms: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            downshift_streak: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            last_plan_probe: Arc::new(std::sync::Mutex::new(None)),
            working_set: crate::cognition::working_set::global(),
            moe_serving: std::sync::Mutex::new(None),
            active_artifact: Arc::new(std::sync::Mutex::new(None)),
            moe_trace_tail: std::sync::Mutex::new(None),
            division: Arc::new(std::sync::Mutex::new(None)),
            served_resident: Arc::new(std::sync::Mutex::new(None)),
            host_cache_lease: std::sync::Mutex::new(
                crate::capacity::host_cache_lease::StickyLease::new(HOST_CACHE_LEASE_BAND_DIVISOR),
            ),
            device_budget_lease: std::sync::Mutex::new(
                crate::capacity::host_cache_lease::StickyLease::new(HOST_CACHE_LEASE_BAND_DIVISOR),
            ),
            // Read ONCE here (single config entry point, no per-tick I/O). Off by default.
            measure_force_expert_budget_bytes: crate::config_env::read(
                "K3_MEASURE_FORCE_EXPERT_BUDGET_BYTES",
            )
            .and_then(|s| s.trim().parse::<u64>().ok()),
            vision_sidecar: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Set the lane DEMAND (how many minds need a concurrent lane — the
    /// persona floor). The boot wiring calls this before the first
    /// [`Self::compute_plan`]; the next tick replans if it changes.
    pub fn set_lane_demand(&self, demand: u32) {
        // Register the demand state process-globally the first time boot sets it, so a
        // measurement preemption lease (quiesce_all / quiesce_others) can drop the
        // warm-slot demand to the ACTIVE (non-quiesced) count for its duration and
        // release on drop — without threading a ServingDaemon handle into the persona
        // registry. Idempotent: `set` after the first call is a no-op.
        let state = LANE_DEMAND.get_or_init(|| LaneDemandState {
            cell: self.lane_demand.clone(),
            base: std::sync::Mutex::new(demand.max(1)),
            overrides: std::sync::Mutex::new(Vec::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        });
        *state.base.lock().expect("lane-demand base lock poisoned") = demand.max(1);
        state.recompute();
    }
}


#[async_trait]
impl ServiceModule for ServingDaemonModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "serving-daemon",
            priority: ModulePriority::Normal,
            command_prefixes: &["serving/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(TICK),
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Capture the bus so snapshot changes fan out to subscribers (the
        // cbar-stage shape). Set-once; ignore a re-init.
        let _ = self.bus.set(ctx.bus.clone());
        // Install our serving-state watch as the process-wide readable seam so
        // free functions + adapters read "what's live" as a pointer instead of
        // each probing /v1/models. Set-once (singleton daemon).
        let _ = crate::inference::llama_server::install_serving_state(self.subscribe_serving());
        // THE DEVICE-BUDGET AUTHORITY, wired here rather than promised in a doc comment.
        // Without it `EphemeralServingLane::spawn` admits every engine with only a
        // free-port check — which is how three sidecars once shared one GPU. Set-once
        // (singleton daemon); `the_serving_daemon_installs_the_admission_authority`
        // pins that this call exists.
        let _ = crate::inference::llama_server::install_lane_admission(
            self.lane_admission_authority(),
        );
        // NOTE: the VRAM prior is seeded and serving is registered as a ResourceConsumer at
        // WIRING time ([`declare_to_memory_authority`], called from `ipc/mod.rs` before
        // `register_planner_on_authority_tick`) — NOT here. `initialize` runs after the module
        // is handed to the runtime, and the authority's tick can fire a plan in between; a plan
        // computed while serving is not yet a registered consumer gets NO add-back for its own
        // residency, which is the #438 `usable_gb = 0` window. Both calls are idempotent, so
        // this is an ordering constraint, not a duplication hazard.
        // Reap orphaned llama-server lanes left by a crashed/SIGKILLed predecessor
        // BEFORE reconciling to the new plan — a mid-eval crash leaves ephemeral
        // lanes (their own scanned ports, no pidfile) holding ~6 GB each with zero
        // reclaim record but the registry. Sweeping first frees that VRAM so the
        // new live lane comes up without competing against dead siblings.
        for outcome in crate::inference::lane_registry::sweep_orphans() {
            crate::probe!(
                class = "serving.lane_registry.sweep",
                outcome = format!("{outcome:?}").as_str(),
                "boot lane-registry sweep",
            );
        }
        // Same reclaim, one level up: a detached benchmark run is a tokio task INSIDE the core,
        // so a restart kills it with no child process to find and no error to report. Any run
        // still marked `running` belonged to the core we just replaced — journal it as
        // killed-by-restart so a poller sees a cause instead of waiting forever on a file that
        // will never appear (#137's train-job shape, applied to benchmarks).
        // NAME THE INSTANCE, not just the run. A reap that reports only an opaque run id is
        // unactionable: recovering the round means hand-opening ledger files to learn WHICH
        // task needs re-measuring. Measured 2026-08-21 — 8 of 20 runs on this box carried
        // `killed by a core restart`, 40% of every run ever attempted here, and the ledger
        // knew each instance the whole time.
        //
        // THIS STILL DOES NOT RE-DISPATCH, and says so rather than implying it did. The
        // instance goes back on the board through `benchmark/dispatch` — the ONE adapter into
        // kanban — never from here: this is the SERVING daemon, and a serving daemon that
        // posts work cards is the parallel-runner shape
        // (docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md). Auto-re-post belongs on
        // the benchmark side, consuming this report.
        for (run_id, instance) in crate::cognition::swe_bench::reap_orphaned_solve_runs() {
            crate::probe!(
                class = "benchmark.swe.orphan_reaped",
                run_id = run_id.as_str(),
                instance = instance.as_str(),
                needs_redispatch = true,
                "benchmark run orphaned by a core restart — {instance} was NOT measured; \
                 the benchmark-side boot resume rejoins its round and re-fires it",
            );
        }
        // The ROUND half is now CONTINUITY (plan A5): a Working round SURVIVES the
        // restart — the benchmark-side boot resume (modules::benchmark_resume, which
        // owns re-firing per BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER; never this daemon)
        // rejoins it. Only TTL-expired abandoned rounds are swept here.
        for (benchmark, remaining) in crate::cognition::bench_round::reap_orphaned_rounds() {
            crate::probe!(
                class = "bench.round.expired",
                benchmark = benchmark.as_str(),
                remaining = remaining,
                "benchmark round exceeded its TTL with {remaining} card(s) unworked — \
                 expired, not resumed; a fresh benchmark/dispatch re-measures it",
            );
        }
        // Plan once at boot so the decision is published before the first tick,
        // then kick the first reconcile so the server comes up promptly rather
        // than waiting a full tick interval. The reconcile runs detached.
        self.recompute();
        let _ = self.reconcile_to_plan();
        Ok(())
    }

    async fn tick(&self) -> Result<(), String> {
        crate::inference::llama_server::collect_retired_engines();
        self.poll_teacher_batch();
        self.note_plan_none_with_live_lane();
        // The plan is DECIDED on the memory authority's tick now (MEMORY-AUTHORITY-DAEMON:
        // `register_planner_on_authority_tick` runs `recompute()` as an `on_tick` observer,
        // publishing to `plan_tx`) — serving no longer samples memory on its own tick. This
        // tick only RECONCILES: bring the running server in line with the authority's
        // published plan. Fast-to-decide; the slow relaunch spawns off the tick.
        // BEFORE the reconcile, so a wedge reported since the last tick is already published
        // not-ready when the reconcile reads the snapshot — otherwise the escalation waits a
        // full tick behind the very guard it exists to defeat.
        self.take_reported_wedge();
        let _ = self.reconcile_to_plan();
        // Liveness heartbeat (#175 self-heal): on a slow cadence, re-verify that the lane
        // we believe is `ready` can ACTUALLY decode — the reconcile trusts the published
        // `ready` forever and would never notice an OOM-poisoned backend otherwise. Off the
        // tick, hysteresis-gated; a sustained failure flips not-ready and the next reconcile
        // respawns the lane.
        let _ = self.spawn_health_heartbeat_if_due();
        // The cheap knob on the reaction ladder (#56): re-derive the concurrent-prefill
        // grant from the board's LIVE VRAM availability (lock-free watch read, already net
        // of external pressure + grants). Flexes both directions every tick — the instant
        // valve for the 2026-07-16 compute-buffer OOM; re-planning above stays the slow,
        // hysteresis-guarded knob. Ungoverned VRAM (no monitor) → no live number → the
        // throttle holds at the lane count rather than guessing.
        if let Some(available) = governed_vram_ceiling(&self.resource_daemon) {
            // Probes on CHANGE only, inside the throttle — a steady grant is silence.
            let _granted = crate::cognition::prefill_throttle::reconcile(available);
        }
        // #287: for a ready MoE serve, re-derive the governed host-cache lease from the
        // LIVE working set (KV grows as slots fill) and publish it through the sticky
        // band to the per-port plan file — the actuator her ResidencyCache polls.
        self.publish_moe_host_cache_lease();
        self.lower_spawn_baseline_to_the_trough();
        self.sample_lane_footprint().await;
        Ok(())
    }

    async fn drain(&self) -> Result<u32, String> {
        self.interrupt_teacher_batch()
    }

    async fn shutdown(&self) -> Result<(), String> {
        self.interrupt_teacher_batch().map(|_| ())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // The full `serving/*` surface (plan · status · load · unload) is migrated to
        // the typed registry (`commands/serving/*`, wired via `commands()`). Fail loud
        // — no silent fallback.
        Err(format!(
            "serving-daemon command surface is migrated to the typed registry; \
             '{command}' has no legacy handler"
        ))
    }

    /// The typed `serving/*` family on the ONE command registry, so a persona /
    /// operator / grid peer is OFFERED serving control + inspection as real tools:
    /// the VRAM-axis deallocation pair (`serving/unload` frees a lane, `serving/load`
    /// permits it again) and the two read surfaces (`serving/plan` = intent,
    /// `serving/status` = reality). They share the daemon's suppress-set writer, its
    /// published serving snapshot, and its serving-plan receiver; the daemon's own
    /// plan/reconcile loop turns the suppress-set edits into actual (un)loads.
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::serving::command_objects(
            self.intent.clone(),
            self.pin_fit_checker(),
            self.subscribe_serving(),
            self.subscribe(),
            self.catalog.clone(),
            self.pin_store.clone(),
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
pub(crate) mod tests {
    // what this catches (card c3c50e0d): a refused demotion with NO previous plan must
    // still produce a plan for the inherited engine — the incumbent alone, at its running
    // geometry, against the physical budget — never nothing. An incumbent that is not a
    // candidate, or cannot fit one lane physically, is a named None.
    #[test]
    fn a_refused_demotion_with_no_plan_adopts_the_inherited_engine_at_its_geometry() {
        use super::incumbent_adoption_plan;
        use crate::cognition::serving_plan::{HostBudget, ModelFootprint, ServingDemand};
        const GB: u64 = 1_000_000_000;
        let qwen27b = ModelFootprint {
            model_id: "ggml-org/Qwen3.8-27B-GGUF".into(),
            weights_bytes: 17 * GB,
            kv_per_token: 55_000,
            context_window: 131_072,
            capability_rank: 18,
            fixed_per_lane_bytes: 0,
        };
        let small = ModelFootprint {
            model_id: "continuum-ai/qwen2.5-coder-14b-instruct-GGUF".into(),
            weights_bytes: 9 * GB,
            kv_per_token: 30_000,
            context_window: 32_768,
            capability_rank: 11,
            fixed_per_lane_bytes: 0,
        };
        let physical = HostBudget { usable_bytes: 44 * GB, perf_cores: 12 };
        let demand = ServingDemand::new(2, Some(30_000));
        let plan = incumbent_adoption_plan(
            &physical,
            &[small.clone(), qwen27b.clone()],
            &qwen27b.model_id,
            &demand,
            Some((41_728, 2)),
        )
        .expect("the incumbent fits physically");
        assert_eq!(plan.base_model.model_id, qwen27b.model_id, "the incumbent, never the smaller candidate");
        assert!(plan.lanes >= 1 && plan.served_context_window >= 8_192);
        assert!(
            incumbent_adoption_plan(&physical, &[small], &qwen27b.model_id, &demand, None).is_none(),
            "an incumbent that is not a candidate is a named None"
        );
    }

    // what this catches (card c3c50e0d): a reconcile in flight is NAMED — its step and age
    // — and past the bound it is a wedge, never a silent skip. The gate, the start and the
    // step are one operation: acquire sets them, drop clears them, a second acquire fails.
    #[test]
    fn a_reconcile_in_flight_names_its_step_and_a_wedge_is_declared_past_the_bound() {
        use super::{reconcile_wedged, ReconcileStep, ServingOperation, RECONCILE_WEDGE_BOUND};
        use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
        use std::sync::Arc;
        let gate = Arc::new(AtomicBool::new(false));
        let started = Arc::new(AtomicU64::new(0));
        let step = Arc::new(AtomicU8::new(0));
        let op = ServingOperation::acquire(gate.clone(), started.clone(), step.clone())
            .expect("first acquire");
        assert!(ServingOperation::acquire(gate.clone(), started.clone(), step.clone()).is_none());
        assert!(started.load(Ordering::Acquire) > 0, "the start is stamped");
        op.step(ReconcileStep::Ensure);
        assert_eq!(ReconcileStep::from_u8(step.load(Ordering::Acquire)), ReconcileStep::Ensure);
        assert_eq!(ReconcileStep::from_u8(step.load(Ordering::Acquire)).name(), "ensure_serving");
        drop(op);
        assert!(!gate.load(Ordering::Acquire) && started.load(Ordering::Acquire) == 0);
        assert_eq!(ReconcileStep::from_u8(step.load(Ordering::Acquire)), ReconcileStep::None);
        assert!(!reconcile_wedged(RECONCILE_WEDGE_BOUND));
        assert!(reconcile_wedged(RECONCILE_WEDGE_BOUND + std::time::Duration::from_secs(1)));
        // Per step (Cormac on #4421): a teacher batch is busy for its own reasons, never wedged.
        let long = RECONCILE_WEDGE_BOUND * 10;
        assert!(super::reconcile_wedged_at(ReconcileStep::Ensure, long));
        assert!(!super::reconcile_wedged_at(ReconcileStep::AcademyBatch, long));
    }

    // what this catches (card 628dc958): the plan grew into free memory while the host
    // was pushing other processes' pages out to make it, then fed on the memory its own
    // relaunch freed — six relaunches in 75 min on the M5 at HIGH. Pure decision.
    #[test]
    fn a_plan_never_grows_while_the_host_pages_out() {
        use super::grow_refused_while_paging;
        let gib = |n: u64| n << 30;
        // paging out + a bigger plan = refused; flat or falling swap, a shrink, or no
        // previous reading (boot on a node that always has swap) flows.
        assert!(!grow_refused_while_paging(80_000, 120_000, u64::MAX, gib(2)));
        assert!(grow_refused_while_paging(80_000, 120_000, gib(1), gib(2)));
        assert!(!grow_refused_while_paging(80_000, 120_000, gib(2), gib(2)));
        assert!(!grow_refused_while_paging(80_000, 120_000, gib(2), gib(1)));
        assert!(
            !grow_refused_while_paging(120_000, 80_000, gib(1), gib(2)),
            "a shrink while paging is the remedy, never refused"
        );
    }

    // what this catches (Cormac on #4298): a receipt that fires on PERSISTENCE rather than
    // CHANGE. `serving.prompt_cache.divergence` sits on the 5-second ready path, and the M5
    // gap lasted three hours — firing per tick is ~2,160 identical lines carrying the same
    // two numbers, a rising count that is not rising evidence and drowns the very ledger the
    // card asks a reader to open. Both EDGES owe a receipt; the middle owes silence.
    #[test]
    fn a_divergence_speaks_when_it_appears_and_when_it_closes_never_while_it_persists() {
        use super::say_on_change;
        let mut cell: Option<(&str, u32, u32)> = None;

        // Quiet stays quiet — a node whose engine already holds what the decision wants
        // must never emit a line just for being healthy.
        assert_eq!(say_on_change(&mut cell, None), None);

        // APPEARS: speak, and the previous state is "nothing had been said".
        let m5 = ("qwen27b", 480u32, 2_517u32);
        assert_eq!(say_on_change(&mut cell, Some(m5)), Some(None));

        // PERSISTS: three hours of identical ticks, one receipt total.
        for _ in 0..2_160 {
            assert_eq!(say_on_change(&mut cell, Some(m5)), None);
        }

        // MOVES: the target drifted (3889 -> 2916 -> 2517 happens within minutes), so the
        // fact changed and owes a new receipt carrying the old numbers.
        let moved = ("qwen27b", 480u32, 3_889u32);
        assert_eq!(say_on_change(&mut cell, Some(moved)), Some(Some(m5)));

        // A MODEL SWITCH that happens to produce the same pair is a different fact — this is
        // why the cell is keyed by model id and not by the numbers alone.
        let other = ("ornith35b", 480u32, 3_889u32);
        assert_eq!(say_on_change(&mut cell, Some(other)), Some(Some(moved)));

        // CLOSES: one receipt at the far edge, carrying what it was, then silence again.
        assert_eq!(say_on_change(&mut cell, None), Some(Some(other)));
        assert_eq!(say_on_change(&mut cell, None), None);
    }

    // what this catches (2026-09-19, the M5 pinned at one lane across boots): the store is
    // written on the tick a launch SETTLES — cooldown 1 → 0 with the lane serving — and on
    // no other tick: not at spawn (cooling = full), not while cooling, not after (0), not
    // for a lane that is not ready, not for zero lanes.
    #[test]
    fn only_the_tick_a_launch_settles_on_remembers_its_geometry() {
        use super::{settles_this_tick, REHOME_COOLDOWN_TICKS};
        assert!(settles_this_tick(1, true, 2));
        assert!(!settles_this_tick(REHOME_COOLDOWN_TICKS, true, 2), "spawn tick: cooling is full");
        assert!(!settles_this_tick(2, true, 2), "still cooling");
        assert!(!settles_this_tick(0, true, 2), "already settled — no re-save every tick");
        assert!(!settles_this_tick(1, false, 2), "not ready is not settled");
        assert!(!settles_this_tick(1, true, 0), "no lanes is nothing to remember");
    }

    // what this catches (the M5, 2026-09-19 23:13–23:15Z): the spawn cooldown (18 ticks) was
    // spent DURING the 27B's 90 s cold load — the old lane kept serving, so the ticks
    // counted — and the new lane came ready with zero settle left; its predecessor's freed
    // memory read as a 32 GB budget for a minute, the plan said 3 lanes, and a second
    // relaunch fired 15 s after the first came ready (12 settles in the hour, acts 5). A
    // launch settles AFTER it is ready: a ready stamp not yet settled on is the edge; the
    // same stamp again is not; a not-ready snapshot never is.
    #[test]
    fn a_launch_settles_after_it_is_ready_not_after_it_was_spawned() {
        assert!(ready_edge(0, Some(1_000)), "the first ready stamp: settle from here");
        assert!(!ready_edge(1_000, Some(1_000)), "the same stamp is the same launch");
        assert!(ready_edge(1_000, Some(2_000)), "a new stamp is a new launch: settle again");
        assert!(!ready_edge(1_000, None), "not ready: nothing to settle on");
    }

    // what this catches: the cache is sized to the ACTIVE roster's states at the served
    // window, never to every seed on disk at its unclamped peak. Regression for the M5
    // 2026-09-20 (card 7b01ed65): 23 seeded minds × 225k–505k peaks made the want
    // astronomical, so the grant was always "everything affordable" (26,985 MiB on a
    // fresh 2 × 67k engine), the cache filled the box into swap, the plan shed a lane and
    // every deploy went dark. Two lanes → four states of at most the window; the largest
    // are kept; a mind that never sent falls back to its peak; an unknown bound keeps all.
    #[test]
    fn the_cache_holds_the_active_rosters_states_not_every_seed_on_disk() {
        // 23 minds: sent ~30k, unclamped peaks 225k–505k, one that never sent.
        let mut population: Vec<(u32, u32)> = (0..22).map(|i| (28_000 + i * 500, 225_000 + i * 12_000)).collect();
        population.push((0, 505_000));
        let active = crate::persona::spawner_module::bounded_by_warm_slots(population.len(), Some(2));
        assert_eq!(active, 4, "two lanes seat four active minds");
        let states = active_cache_states(&population, 67_072, active);
        assert_eq!(states.len(), 4, "one state per active mind, not one per seed");
        assert!(states.iter().all(|t| *t <= 67_072), "a state never exceeds the served window: {states:?}");
        assert_eq!(states[0], 67_072, "the never-sent mind's peak, clamped to the window, is the largest kept");
        assert_eq!(states[1], 38_500, "then the largest sent prompts");
        let want: u64 = states.iter().map(|t| 32_888u64 * *t as u64).sum();
        assert!(want < 8 * 1024 * 1024 * 1024, "four states at 32.9 KB/token stay under 8 GiB: {want}");
        // No bound known (a derivation before any plan) keeps the FLOOR, never everything:
        // unknown is not "all 23 at their peaks" — that is the want this law deletes.
        assert_eq!(
            active_cache_states(&population, 67_072, 0).len(),
            crate::modules::citizen_health::MINDLESS_RESIDENT_FLOOR as usize
        );
        // Through the decision: the same population on a 2-lane serve asks for a
        // demand-sized cache, not the whole afford.
        let fp = crate::cognition::serving_plan::ModelFootprint {
            model_id: "cache-fixture".into(),
            weights_bytes: 19 * 1024 * 1024 * 1024,
            kv_per_token: 32_888,
            context_window: 262_144,
            capability_rank: 1,
            fixed_per_lane_bytes: 0,
        };
        let physical = 64u64 << 30;
        let decision = prompt_cache_decision(Some(&fp), &states, 23, 67_072, 2, physical, fp.peak_resident_bytes(67_072, 2), 40u64 << 30);
        assert_eq!(decision.reason, "resident_demand_kv_estimate");
        let afford_mib = decision.affordable_bytes.expect("afford") / (1024 * 1024);
        assert!(
            (decision.desired_mib as u64) < afford_mib && decision.desired_mib < 9_000,
            "demand-sized ({} MiB), not the afford ({afford_mib} MiB)",
            decision.desired_mib
        );
    }

    // what this catches (the 5090, 2026-10-11: --cache-ram 4288 against hybrid-27B states of
    // 4.3-5.8 GB; 104 states skipped and 220 evicted over the lane's life, two minds on one
    // slot found nothing): a host prompt cache sized from the KV estimate alone, when a
    // saved state also carries the recurrent state and the context checkpoints. Each mind's
    // demand is raised to the measured state, so the cache holds every rotating mind's
    // state; with nothing measured the KV estimate stands unchanged.
    #[test]
    fn the_prompt_cache_holds_each_rotating_minds_measured_state() {
        let kv = 32_768u64;
        let mib = 1024 * 1024;
        let measured = Some(5_511 * mib);
        let demands = vec![41_318u32, 21_525];
        assert_eq!(with_measured_state(demands.clone(), Some(kv), None), demands, "nothing measured: the estimate stands");
        assert_eq!(with_measured_state(demands.clone(), None, measured), demands, "no KV rate: nothing to convert");
        let raised = with_measured_state(demands.clone(), Some(kv), measured);
        let plenty = 64 * 1024 * mib;
        let sized = u64::from(crate::inference::lane_args::host_prompt_cache_mib(&raised, kv, plenty)) * mib;
        assert!(sized >= 2 * 5_511 * mib, "both minds' measured states fit: {} MiB", sized / mib);
        let before = u64::from(crate::inference::lane_args::host_prompt_cache_mib(&demands, kv, plenty)) * mib;
        assert!(before < 5_511 * mib, "the KV estimate alone held less than ONE measured state: {} MiB", before / mib);
        let afford = 8 * 1024 * mib;
        assert!(u64::from(crate::inference::lane_args::host_prompt_cache_mib(&raised, kv, afford)) * mib <= afford, "still capped by what the host can afford");
    }

    // what this catches (the M5, 2026-10-11 01:41Z: the plan moved 2,2,1,1,1,1 lanes in 40 s,
    // same engine pid, and the admission gate followed every move): admission sized by a plan
    // the engine never took. A serving engine's own lane count is what the gate counts; the
    // plan only stands in before an engine is up.
    #[test]
    fn the_admission_gate_counts_the_engines_lanes_not_the_plans() {
        assert_eq!(admitted_lanes(true, 2, 1), 2, "a 2-lane engine is admitted as two lanes whatever the plan says");
        assert_eq!(admitted_lanes(true, 1, 2), 1, "and a 1-lane engine as one, until the reconcile relaunches it");
        assert_eq!(admitted_lanes(false, 0, 2), 2, "cold boot: the plan is all there is");
        assert_eq!(admitted_lanes(true, 0, 2), 2, "a ready snapshot without a lane count is no evidence");
    }

    // what this catches (the 5090, 2026-10-11: launched --cache-ram 4288 against ~5.5 GB
    // measured states, 114 skips, 76% of restores cold, and nothing ever relaunched for it):
    // the agreed rule. A cache that holds no measured state is evidence for a re-home when a
    // relaunch would hold one; a cache that holds at least one state is never a reason,
    // however far below the derived target; nothing measured is never a reason; and a host
    // that cannot afford one state does not relaunch for nothing.
    #[test]
    fn a_lane_relaunches_for_cache_size_only_when_its_cache_holds_no_state() {
        let mib = 1024u64 * 1024;
        let state = Some(5_511 * mib);
        assert!(cache_relaunch_needed(4_288, 11_022, state), "the 5090: holds nothing, a relaunch holds two");
        assert!(!cache_relaunch_needed(6_000, 40_000, state), "holds one state: waits for the next relaunch, however far below target");
        assert!(!cache_relaunch_needed(4_288, 11_022, None), "nothing measured: no relaunch on a guess");
        assert!(!cache_relaunch_needed(4_288, 5_000, state), "a relaunch would hold no state either: no futile relaunch");
        assert!(!cache_relaunch_needed(0, 11_022, state), "an unrecorded launch size is not a measurement");
        assert!(launched_cache_holds_no_state(4_288, state) && !launched_cache_holds_no_state(5_512, state));
    }

    // what this catches (card 58ba9752): the cache sized for lanes x 2 while more minds
    // rotate. The IntelMac, 2026-10-10: 7 minds turning through 2 lanes against 4 states,
    // so every turn re-prefilled cold. Minds seen within the rotation window count; the
    // M5's dormant seeds (never seen, or seen hours ago) do not, so 7b01ed65 stays shut.
    #[test]
    fn the_cache_counts_the_minds_that_rotate_not_the_seeds_on_disk() {
        let hour = 60 * 60 * 1000u64;
        let minute = 60_000u64;
        let now = 1000 * hour;
        // The IntelMac (card 56d3c76f): residents last seen 6, 41, 77, 225, 246, 250 and
        // 366 minutes ago, each returning every 4-7 hours, plus one never seen.
        let intelmac: Vec<(u64, u64)> = [6u64, 41, 77, 225, 246, 250, 366]
            .iter()
            .zip([4u64, 5, 6, 4, 5, 6, 7])
            .map(|(ago, every)| (now - ago * minute, now - ago * minute - every * hour))
            .chain([(0, 0)])
            .collect();
        let window = rotation_window_ms(&intelmac).expect("seven measured returns");
        assert_eq!(window, 7 * hour, "the p90 return interval");
        assert_eq!(rotating_minds(&intelmac, now, Some(window)), 7, "the whole rotation is held; the never-seen mind is not");
        assert_eq!(rotating_minds(&intelmac, now, Some(hour)), 2, "a declared hour counted two and lost to the floor of four: the no-op");
        // The M5 shape: 23 seeds, three returning every 19 minutes, twenty dormant for days.
        let m5: Vec<(u64, u64)> = (0..23)
            .map(|i| if i < 3 { (now - minute, now - 20 * minute) } else { (now - 72 * hour, 0) })
            .collect();
        let w5 = rotation_window_ms(&m5);
        assert_eq!(w5, Some(19 * minute));
        assert_eq!(rotating_minds(&m5, now, w5), 3, "the dormant seeds stay out: 7b01ed65 stays shut");
        let floor = crate::persona::spawner_module::bounded_by_warm_slots(23, Some(2));
        assert_eq!(rotating_minds(&m5, now, w5).max(floor), floor, "the warm-slot floor still holds");
        // Nothing measured yet: nothing counts, and the floor alone sizes the cache.
        assert_eq!(rotation_window_ms(&[(now, 0)]), None);
        assert_eq!(rotating_minds(&[(now, 0)], now, None), 0);
    }

    // what this catches: c8a8829b / the 4096 MiB incident hid all prior branches
    // and treated a desired KV estimate as observed serialized cache capacity.
    #[test]
    fn prompt_cache_diagnostics_preserve_priors_and_distinguish_measurements() {
        use super::*;
        let fp = crate::cognition::serving_plan::ModelFootprint {
            model_id: "cache-fixture".into(),
            weights_bytes: 1024 * 1024 * 1024,
            kv_per_token: 65536,
            context_window: 65536,
            capability_rank: 1,
            fixed_per_lane_bytes: 0,
        };
        let missing = prompt_cache_decision(None, &[60000], 1, 65536, 1, 32 << 30, 0, 0);
        assert_eq!(missing.reason, "prior_missing_footprint");
        assert_eq!(missing.estimated_kv_bytes, None);
        assert_eq!(missing.affordable_bytes, None);
        // Empty demand is TWO states now, and they must not share a name. A cold node
        // (no seeds) and a node whose residents will resume but have never recorded
        // demand are different facts; the old single reason read benign for both — and
        // for a third state, "residents not yet up at launch", that the caller no longer
        // produces (it sizes from the resume set). Regression for the 5-minute turn.
        let cold = prompt_cache_decision(Some(&fp), &[], 0, 65536, 1, 32 << 30, fp.peak_resident_bytes(65536, 1), 0);
        assert_eq!(cold.reason, "prior_no_seeds_on_disk");
        let empty = prompt_cache_decision(Some(&fp), &[], 2, 65536, 1, 32 << 30, fp.peak_resident_bytes(65536, 1), 0);
        assert_eq!(empty.reason, "prior_seeds_without_recorded_demand");
        assert_eq!(empty.citizens, 0, "planned is not demand — citizens counts demand rows");
        let unknown = prompt_cache_decision(Some(&fp), &[60000], 1, 65536, 1, 0, fp.peak_resident_bytes(65536, 1), 0);
        assert_eq!(unknown.reason, "prior_unknown_physical_memory");
        assert_eq!(missing.desired_mib, crate::inference::lane_args::CACHE_RAM_MIB);
        for prior in [&missing, &cold, &empty, &unknown] {
            assert_eq!(prior.affordable_bytes, None);
        }
        // what this catches: a prior that knows the geometry must hold ONE FULL STATE.
        // The fixture above sits on the crossover (65536 × 65536 B = exactly 4096 MiB),
        // where the constant and the floor coincide and an `assert_eq!(…, CACHE_RAM_MIB)`
        // could not tell them apart — the coincidence that let a first-ever boot ship an
        // undersized cache (Cormac, #4128). So drive the priors across geometries with
        // one full state BELOW the constant and ABOVE it, and assert the invariant.
        let mib = 1024u64 * 1024;
        for (kv_per_token, served_ctx) in [(16_384u64, 65_536u32), (27_360, 172_436), (65_536, 262_144)] {
            let geo = crate::cognition::serving_plan::ModelFootprint {
                kv_per_token,
                context_window: served_ctx,
                ..fp.clone()
            };
            let one_state_bytes = kv_per_token * served_ctx as u64;
            let one_state_mib = one_state_bytes.div_ceil(mib) as u32;
            let constant = crate::inference::lane_args::CACHE_RAM_MIB;
            for (name, prior) in [
                ("cold", prompt_cache_decision(Some(&geo), &[], 0, served_ctx, 1, 32 << 30, geo.peak_resident_bytes(served_ctx, 1), 0)),
                ("seeds", prompt_cache_decision(Some(&geo), &[], 2, served_ctx, 1, 32 << 30, geo.peak_resident_bytes(served_ctx, 1), 0)),
                ("unknown", prompt_cache_decision(Some(&geo), &[60_000], 1, served_ctx, 1, 0, geo.peak_resident_bytes(served_ctx, 1), 0)),
            ] {
                // The engine's check is in BYTES (`state_size > limit_size`), so the
                // invariant is asserted in bytes: a limit floored to the MiB below a
                // full state still refuses that state.
                assert!(
                    (prior.desired_mib as u64) * mib >= one_state_bytes,
                    "{name} prior at kv {kv_per_token} × ctx {served_ctx}: {} MiB < one full state {} B",
                    prior.desired_mib,
                    one_state_bytes
                );
                assert_eq!(prior.desired_mib, constant.max(one_state_mib), "{name}: the floor lifts the prior, never lowers it");
            }
            // The one prior that cannot know the geometry keeps the constant.
            assert_eq!(prompt_cache_decision(None, &[], 2, served_ctx, 1, 32 << 30, 0, 0).desired_mib, constant);
        }
        let measured = prompt_cache_decision(Some(&fp), &[32768, 32768], 2, 65536, 1, 32 << 30, fp.peak_resident_bytes(65536, 1), 0);
        // Same 4096 MiB result as the prior, but materially different evidence.
        assert_eq!(measured.desired_mib, missing.desired_mib);
        assert_ne!(measured, missing);
        assert_eq!(measured.estimated_kv_bytes, Some(4 << 30));
        assert_eq!(measured.reason, "resident_demand_kv_estimate");
        let above_floor =
            (32u64 << 30) - fp.peak_resident_bytes(65536, 1) - host_os_floor_bytes(32 << 30);
        // The grant leaves the planner's co-consumer headroom — a cache fills to its
        // grant, so a grant of everything above the floor ends at the floor (the M5,
        // 2026-09-19: 6 GB of swap under a 19.5 GB grant).
        let afford = (above_floor as f64
            * (1.0 - crate::cognition::serving_plan::CACHE_GRANT_HEADROOM)) as u64;
        assert!(afford < above_floor, "the cache never gets everything above the floor");
        assert_eq!(measured.affordable_bytes, Some(afford));
        assert_eq!(
            measured.desired_mib,
            crate::inference::lane_args::host_prompt_cache_mib(
                &[32768, 32768],
                fp.kv_per_token,
                afford
            )
        );
        let squeezed = prompt_cache_decision(Some(&fp), &[60000], 1, 65536, 1, 1, fp.peak_resident_bytes(65536, 1), 0);
        assert_eq!(squeezed.affordable_bytes, Some(0));
        assert_eq!(
            squeezed.desired_mib,
            crate::inference::lane_args::host_prompt_cache_mib(&[60000], fp.kv_per_token, 0)
        );
    }

    // what this catches (2026-09-16, the 5090): the affordability term charged the
    // serve's on-device working set to HOST RAM regardless of topology. On a discrete
    // GPU the 27B's weights + a 144k KV window live in VRAM, yet `physical − peak −
    // floor` went to 0 and the cache to its 256 MiB hard floor while 49 GB of host RAM
    // sat free — every turn a full re-prefill. Same footprint, same demand, same box:
    // unified pays the working set, discrete does not, and the discrete answer is the
    // demand-sized one.
    /// The grant the cache may be promised out of a ceiling: the ceiling less the
    /// cache's grant headroom (#4216 — a cache fills to its grant).
    fn grantable(ceiling: u64) -> u64 {
        (ceiling as f64 * (1.0 - crate::cognition::serving_plan::CACHE_GRANT_HEADROOM)) as u64
    }

    /// The grant out of a known `available` reading: the smaller of "above the OS
    /// floor" and "above the Eco exit band", less the co-consumer share.
    fn grantable_from_available(physical: u64, available: u64) -> u64 {
        grantable(
            available
                .saturating_sub(host_os_floor_bytes(physical))
                .min(available.saturating_sub(crate::provisioning::model_catalog::ECO_EXIT_BYTES)),
        )
    }

    #[test]
    fn a_discrete_gpu_does_not_charge_vram_residency_against_host_ram() {
        use super::*;
        use crate::gpu::monitor::MemoryMode;
        let fp = crate::cognition::serving_plan::ModelFootprint {
            model_id: "27b-on-a-5090".into(),
            weights_bytes: 17 << 30,
            kv_per_token: 262_144, // ~256 KiB/token: a 144k window is ~36 GiB of KV
            context_window: 262_144,
            capability_rank: 1,
            fixed_per_lane_bytes: 0,
        };
        let physical = 63u64 << 30;
        let (ctx, lanes) = (144_530u32, 1u32);
        let demands = [43_000u32, 12_000];
        let unified = serve_host_bytes(Some(&fp), ctx, lanes, Some(MemoryMode::Unified));
        let discrete = serve_host_bytes(Some(&fp), ctx, lanes, Some(MemoryMode::Discrete));
        let cpu_only = serve_host_bytes(Some(&fp), ctx, lanes, None);
        assert_eq!(unified, fp.peak_resident_bytes(ctx, lanes));
        assert_eq!(cpu_only, unified, "no GPU = everything host-resident");
        assert_eq!(discrete, 0, "VRAM residency costs the host nothing");
        assert!(
            unified + host_os_floor_bytes(physical) > physical,
            "the fixture reproduces the squeeze"
        );

        let on_unified =
            prompt_cache_decision(Some(&fp), &demands, 2, ctx, lanes, physical, unified, 0);
        let on_discrete =
            prompt_cache_decision(Some(&fp), &demands, 2, ctx, lanes, physical, discrete, 0);
        assert_eq!(
            on_unified.affordable_bytes,
            Some(0),
            "unified: honestly nothing left"
        );
        assert_eq!(
            on_unified.desired_mib,
            crate::inference::lane_args::CACHE_RAM_HARD_FLOOR_MIB,
            "the 256 MiB the 5090 actually launched with"
        );
        let want_bytes = demands.iter().map(|t| fp.kv_per_token * *t as u64).sum::<u64>();
        assert_eq!(
            on_discrete.affordable_bytes,
            Some(grantable(physical - host_os_floor_bytes(physical)))
        );
        assert_eq!(
            on_discrete.desired_mib as u64,
            want_bytes / (1024 * 1024),
            "discrete: sized from the citizens' demand, not squeezed to the floor"
        );
        assert_eq!(on_discrete.serve_host_bytes, 0);
    }

    // what this catches (2026-09-17, the 5090): the discrete footprint arm. WDDM
    // attributes VRAM to no process, so the lane's bytes beyond its weights are the
    // device delta across the spawn. The 5090's two points: 752 MiB idle → 31,914 MiB
    // at a 144k window and 22,078 MiB at 9.4k, 17 GB of weights; and an adopted lane
    // (no baseline) must say could-not-look, never charge the desktop to the model.
    #[test]
    fn the_discrete_footprint_arm_is_the_device_delta_beyond_the_weights() {
        use super::*;
        const MIB: u64 = 1024 * 1024;
        let weights = 17 << 30;
        let big = device_delta_beyond_weights(752 * MIB, 31_914 * MIB, weights).expect("read");
        let small = device_delta_beyond_weights(752 * MIB, 22_078 * MIB, weights).expect("read");
        assert!(big > small, "a bigger window costs more beyond the weights");
        // Through the ONE consumer: per-token from the plan's own decomposition.
        let per_token_big =
            crate::inference::lane_footprint::per_token_from(big, 1, 144_640, 0, 0).expect("rate");
        assert!(
            per_token_big > 60_000 && per_token_big < 110_000,
            "≈74 KiB/token measured on the 5090, got {per_token_big}"
        );
        assert_eq!(
            device_delta_beyond_weights(0, 31_914 * MIB, weights),
            None,
            "no baseline = could not look"
        );
        assert_eq!(
            device_delta_beyond_weights(752 * MIB, 0, weights),
            None,
            "no reading = could not look"
        );
        assert_eq!(
            device_delta_beyond_weights(u64::MAX, 31_914 * MIB, weights),
            None,
            "armed but never lowered = could not look"
        );
        // card b5230492 (the 5090 2026-10-11: anon_bytes=0 per_token_measured=0 every minute
        // for a lane serving 67,584 tokens): a trough baseline that already held the weights
        // makes the delta saturate. Zero or less beyond the weights is not a reading; it
        // falls through to the engine's own breakdown.
        assert_eq!(
            device_delta_beyond_weights(18_000 * MIB, 31_914 * MIB, weights),
            None,
            "the baseline held the weights: the delta is below them, so it is no reading"
        );
        assert_eq!(
            device_delta_beyond_weights(752 * MIB, 752 * MIB + weights, weights),
            None,
            "exactly the weights beyond the baseline: nothing measured beyond them"
        );
    }

    // what this catches (2026-09-17, the 5090 after #4144): with VRAM residency no
    // longer charged to the host, afford read 55 GB on a 63 GB box that had 31 GB
    // free — a `--cache-ram` that fills to a pagefile thrash once two 115k-token
    // states are saved. The second ceiling is what is AVAILABLE now, less the floor.
    // And it is not the commit charge: the first cut used it and read 75.5 GB
    // committed on that same box with 43 GB free → afford 0 → the 256 MiB floor again.
    // what this catches (2026-09-19, the M5 relaunched at one lane): a grant sized to
    // "everything above the OS floor" fills into the Eco pressure band and steps the
    // serving mode down — the cache throttles its own planner. For every available
    // reading the box can report, what is left after the grant fills stays above the
    // Eco EXIT band, so a full cache never enters Eco by itself; and the grant is still
    // a real cache where there is room (the 46 GB reading gets more than the 20 GB one).
    #[test]
    fn a_full_cache_never_steps_the_serving_mode_down_by_itself() {
        use crate::provisioning::model_catalog::ECO_EXIT_BYTES;
        let fp = crate::cognition::serving_plan::ModelFootprint {
            model_id: "eco-fixture".into(),
            weights_bytes: 16 << 30,
            kv_per_token: 32_768,
            context_window: 67_340,
            capability_rank: 1,
            fixed_per_lane_bytes: 0,
        };
        let physical = 64u64 << 30;
        let demands = [67_340u32; 16]; // sixteen seeds on disk: the want is far past any box
        let mut last = 0u64;
        for available_gib in [13u64, 20, 29, 46] {
            let available = available_gib << 30;
            let d = prompt_cache_decision(Some(&fp), &demands, 16, 67_340, 2, physical, fp.peak_resident_bytes(67_340, 2), available);
            let grant = (d.desired_mib as u64) << 20;
            assert!(
                available - grant >= ECO_EXIT_BYTES,
                "available {available_gib} GiB: a full {} MiB cache would leave {} GiB — inside the Eco band",
                d.desired_mib,
                (available - grant) >> 30
            );
            assert!(grant >= last, "more room, no smaller grant: {available_gib} GiB → {} MiB", d.desired_mib);
            last = grant;
        }
        assert!(last > (8u64 << 30), "with 46 GiB free the cache is still a real cache: {} MiB", last >> 20);
    }

    #[test]
    fn the_cache_never_promises_past_what_is_available_now() {
        use super::*;
        let fp = crate::cognition::serving_plan::ModelFootprint {
            model_id: "27b-on-a-5090".into(),
            weights_bytes: 17 << 30,
            kv_per_token: 262_144,
            context_window: 262_144,
            capability_rank: 1,
            fixed_per_lane_bytes: 0,
        };
        let physical = 63u64 << 30;
        let demands = [115_000u32, 115_000]; // ~58 GB wanted
        let unknown = prompt_cache_decision(Some(&fp), &demands, 2, 144_640, 1, physical, 0, 0);
        assert_eq!(
            unknown.affordable_bytes,
            Some(grantable(physical - host_os_floor_bytes(physical))),
            "no availability reading: headroom alone, less the co-consumer share"
        );
        let available = 31u64 << 30;
        let bound = prompt_cache_decision(
            Some(&fp),
            &demands,
            2,
            144_640,
            1,
            physical,
            0,
            available,
        );
        assert_eq!(
            bound.affordable_bytes,
            Some(grantable_from_available(physical, available)),
            "bounded by what the box can give now, less the Eco exit band and the co-consumer share"
        );
        assert!(bound.desired_mib < unknown.desired_mib);
        assert_eq!(bound.available_bytes, available);
        // Less available than the floor → nothing to promise → the hard floor, no wrap.
        let starved = prompt_cache_decision(Some(&fp), &demands, 2, 144_640, 1, physical, 0, 1);
        assert_eq!(starved.affordable_bytes, Some(0));
        assert_eq!(
            starved.desired_mib,
            crate::inference::lane_args::CACHE_RAM_HARD_FLOOR_MIB
        );
    }

    // what this catches: #3751 threw this throttle at `rehome_streak` — the
    // RE-HOME GAIN counter — which is stored to 0 for exactly as long as the lane
    // is below plan with no qualifying gain. `streak <= 1` was therefore true on
    // every tick of the declined episode and the probe throttled NOTHING: 411
    // identical lines at one per 5 s measured on BigMama 2026-09-05, burying a
    // persona's first_token underneath them. The clock a cadence uses must be
    // owned by the cadence; a domain counter's reset rule belongs to the domain.
    #[test]
    fn decline_probe_speaks_on_entry_then_about_once_a_minute() {
        assert!(
            super::decline_should_log(1),
            "entry into the declined state must always speak — the silence was the original defect"
        );
        for quiet in [2, 3, 7, 11, 13, 23] {
            assert!(!super::decline_should_log(quiet), "tick {quiet} adds nothing the 1st line lacked");
        }
        for loud in [12, 24, 120] {
            assert!(super::decline_should_log(loud), "tick {loud} is the ~1/min heartbeat");
        }

        // A ten-minute episode is 120 ticks: entry plus one per minute, not 120.
        let spoken = (1..=120u32).filter(|&t| super::decline_should_log(t)).count();
        assert_eq!(spoken, 11, "entry + 10 minutes of heartbeat over 120 ticks");

        // The defect itself, stated as a property: a clock that never advances
        // must be SILENT, not infinitely chatty. `0 % 12 == 0` would make the
        // naive form log forever, which is how the first fix passed review.
        assert!(
            !super::decline_should_log(0),
            "a counter that never advanced means we have not entered the state, not that we should shout"
        );
    }


    // what this catches: a self-heal that renders as a dead node. `declare_lane_wedged`
    // TAKES the reason as a parameter, logged it, then published `ServingSnapshot::empty()`
    // — which hardcodes `degraded_reason: None`. For the whole kill+respawn window
    // `serving/status` showed `ready:false, active_model:null, lanes:0,
    // degraded_reason:null`: absence with no stated cause, byte-identical to a node that
    // never had a brain. Measured 2026-09-05 on BigMama, where a wedge-heal recovering by
    // itself inside 120s was read three times as a possible serving-is-dead defect, and
    // the pre-flight guard's `degraded=` probe field (added 2026-08-15 precisely to carry
    // this string) was handed "" on every wait. The reason is IN HAND at the call site;
    // dropping it is the silent-failure class, not a formatting nit.
    #[test]
    fn declaring_a_wedge_publishes_the_reason_not_a_bare_empty_snapshot() {
        use super::*;

        let (tx, _rx) = watch::channel(ServingSnapshot::empty());
        let force = Arc::new(AtomicBool::new(false));

        ServingDaemonModule::declare_lane_wedged(
            &force,
            &tx,
            None,
            "consecutive REAL generations failed on the live lane",
        );

        let snap = tx.borrow();
        // the heal still fires: not-ready, nothing active, relaunch armed
        assert!(!snap.ready, "a declared wedge must flip the lane not-ready");
        assert!(snap.active_model.is_none());
        assert!(
            force.load(Ordering::Relaxed),
            "the relaunch flag is what makes this a HEAL — populating the reason must \
             not change the control path"
        );
        // ...and the operator is told WHY, verbatim
        assert_eq!(
            snap.degraded_reason.as_deref(),
            Some("consecutive REAL generations failed on the live lane"),
            "the wedge reason must reach the snapshot, not only the log"
        );
    }

    // what this catches (2026-09-01 live): the re-home guard measured only the
    // per-slot window, so a plan that GREW total token-space by adding lanes
    // while trimming per-slot (2×57,600 → 4×47,426, +65% space, +2 warm slots)
    // computed a negative gain and never relaunched — five residents thrashed
    // two slots at hit_rate 0.0 until an operator cycled the server by hand.
    // The evidence currency is window × lanes.
    #[test]
    fn rehome_evidence_counts_lanes_not_just_per_slot_window() {
        use super::rehome_gain_evidence;
        // The live incident shape: more lanes, smaller per-slot, bigger total.
        let (live, plan, gain, worth_it) = rehome_gain_evidence(57_600, 2, 47_426, 4);
        assert_eq!(live, 115_200);
        assert_eq!(plan, 189_704);
        assert_eq!(gain, 74_504);
        assert!(worth_it, "65% more total space must qualify as re-home evidence");
        // Same total space re-sliced: not evidence (a relaunch kills in-flight turns).
        let (_, _, _, worth_it) = rehome_gain_evidence(40_000, 2, 20_000, 4);
        assert!(!worth_it, "re-slicing the same space is not a gain");
        // Sub-threshold growth (<15%): not evidence.
        let (_, _, _, worth_it) = rehome_gain_evidence(40_000, 2, 44_000, 2);
        assert!(!worth_it, "10% is under the 15% margin");
        // A dead lane (0 window) never qualifies — recovery is a different path.
        let (_, _, _, worth_it) = rehome_gain_evidence(0, 1, 40_000, 2);
        assert!(!worth_it, "live_space=0 is the recovery arm's job, not re-home");
    }

    // what this catches (2026-09-17, the M5 at 8 lanes / 7.9 t/s all morning): a plan
    // that trades lanes for width is the decode knee clamping the roster, and it is
    // evidence even when token-space calls it a wash or a loss — otherwise the knee
    // only ever applies at boot. Re-slicing the same space at the SAME lane count, or a
    // plan with more lanes, still needs the space margin.
    #[test]
    fn a_lane_over_its_knee_is_rehome_evidence_whatever_the_token_space_says() {
        use super::{lane_over_its_knee, rehome_gain_evidence};
        // The live shape: 8 × 51,712 → 7 × 67,340 (+14%, under the margin) qualifies.
        let (_, _, _, worth_it) = rehome_gain_evidence(51_712, 8, 67_340, 7);
        assert!(worth_it, "one lane over the knee is a relaunch, not a rounding error");
        // The knee at 4: 8 × 51,712 → 4 × 67,340 is a LOSS of space and still qualifies.
        let (_, _, _, worth_it) = rehome_gain_evidence(51_712, 8, 67_340, 4);
        assert!(worth_it, "fewer, wider lanes is the knee — space is not the currency here");
        // Fewer lanes but NARROWER is a pressure shrink, not a knee: not this arm.
        assert!(!lane_over_its_knee(51_712, 8, 40_000, 4));
        // Same lanes / more lanes: the space margin still rules.
        assert!(!lane_over_its_knee(40_000, 2, 44_000, 2));
        assert!(!lane_over_its_knee(57_600, 2, 47_426, 4));
        let (_, _, _, worth_it) = rehome_gain_evidence(40_000, 2, 44_000, 2);
        assert!(!worth_it, "10% at the same lane count is still under the margin");
    }

    // what this catches (card 29e4ab34, the M5 2026-09-19 17:20:35Z): the thirteen-second
    // boot window in which no lane has reported a model. The knee lookup must key on the
    // remembered model then — otherwise the roster (17 runtimes) goes unclamped into the
    // demand, the plan says 8 lanes under the transient budget, and the warm adopted lane
    // is relaunched twice (8 × 49k, then 2 × 67k). A reported model still wins; a box
    // that remembers nothing plans the roster, as before.
    #[test]
    fn a_boot_before_any_lane_reports_its_model_reads_the_remembered_models_knee() {
        use crate::inference::decode_knee::knee_lanes;
        let remembered = Some("ggml-org/Qwen3.8-27B-GGUF".to_string());
        assert_eq!(knee_model(None, || remembered.clone()), remembered, "the boot window plans the last model");
        let read = std::cell::Cell::new(false);
        assert_eq!(
            knee_model(Some("other".into()), || { read.set(true); remembered.clone() }).as_deref(),
            Some("other"),
            "a serving lane's model wins"
        );
        assert!(!read.get(), "with a lane serving, the remembered geometry (a file) is never read");
        assert_eq!(knee_model(None, || None), None, "nothing remembered: the roster rules, as before");
        // With the remembered model's knee (2 on the M5) the roster of 17 is clamped at boot;
        // with none it is not — the first stair of the staircase.
        assert_eq!(knee_lanes(17, Some(2)), 2);
        assert_eq!(knee_lanes(17, None), 17);
    }

    // what this catches (2026-09-19 15:4xZ, the M5 stuck at one lane): the knee's mirror.
    // A cold boot launched 1 × 161,792 under a transient budget; the steady plan said
    // 2 × 67,340 — a 17% LOSS of token-space — and token-space alone declined it every
    // tick, so the roster's second coder lane could never appear at runtime, and the
    // settled 1-lane geometry then capped every later boot. A plan with more lanes than
    // are live is minds without a warm slot; that is evidence whatever the space says.
    // Fewer or equal lanes is not this arm (the knee and the margin rule those), and a
    // dead lane (0 live lanes) is recovery's job, never re-home's.
    #[test]
    fn a_roster_short_of_warm_slots_is_rehome_evidence_whatever_the_token_space_says() {
        use super::{rehome_gain_evidence, roster_short_of_slots};
        assert!(roster_short_of_slots(1, 2), "1 × 161k → 2 × 67k: a second warm slot for the roster");
        let (_, _, _, by_space) = rehome_gain_evidence(161_792, 1, 67_340, 2);
        assert!(!by_space, "token-space alone calls the second lane a loss — the arm above is what carries it");
        assert!(roster_short_of_slots(2, 4), "2 × 40k → 4 × 20k: two more slots the roster asked for");
        assert!(!roster_short_of_slots(2, 2), "the same count is the margin's business");
        assert!(!roster_short_of_slots(8, 4), "fewer lanes is the knee's business");
        assert!(!roster_short_of_slots(0, 2), "a dead lane is recovery, not re-home");
    }

    // what this catches (2026-08-15 14:13, round-killer L10 / #438 live): a plan tick
    // whose usable_bytes sample was transiently bogus named the SMALLEST candidate
    // (qwen2.5-0.5B) via the nothing-fits arm, and the reconcile swapped the READY
    // 27B lane on that single tick — citizens spoke template-token salad into the
    // bench room as durable messages every healthy peer then perceived as
    // conversation. A base-model swap re-homes minds; it must earn the same
    // sustained streak as the (milder) window re-home, target changes must restart
    // the count, and recovery (no ready incumbent) must never wait.
    #[test]
    fn model_swap_on_a_ready_lane_needs_a_sustained_streak() {
        use super::{model_change_gate, ModelChangeGate};
        use std::sync::atomic::AtomicU32;
        let streak = AtomicU32::new(0);
        let mut pending = None;
        // One bogus tick wanting the 0.5B: deferred, never actuates.
        assert_eq!(
            model_change_gate(true, Some("27b"), "0.5b", &mut pending, &streak, 3),
            ModelChangeGate::Defer { streak: 1 }
        );
        // Next tick the sample healed and the plan wants the incumbent again:
        // pending clears, nothing happened.
        assert_eq!(
            model_change_gate(true, Some("27b"), "27b", &mut pending, &streak, 3),
            ModelChangeGate::NotAChange
        );
        assert!(pending.is_none());
        // Sustained genuine disagreement commits on the Nth consecutive tick.
        assert_eq!(
            model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
            ModelChangeGate::Defer { streak: 1 }
        );
        assert_eq!(
            model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
            ModelChangeGate::Defer { streak: 2 }
        );
        assert_eq!(
            model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
            ModelChangeGate::Commit { streak: 3 }
        );
        // A flapping target restarts the count — 0.5b/48b alternation never commits.
        assert_eq!(
            model_change_gate(true, Some("27b"), "0.5b", &mut pending, &streak, 3),
            ModelChangeGate::Defer { streak: 1 }
        );
        assert_eq!(
            model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
            ModelChangeGate::Defer { streak: 1 }
        );
        // No ready incumbent (boot / post-wedge): commit immediately — recovery
        // must not wait out a streak.
        let mut p2 = None;
        assert_eq!(
            model_change_gate(false, Some("27b"), "48b", &mut p2, &streak, 3),
            ModelChangeGate::NotAChange
        );
        assert_eq!(
            model_change_gate(true, None, "48b", &mut p2, &streak, 3),
            ModelChangeGate::NotAChange
        );
    }
    use super::*;
    use crate::cognition::serving_plan::footprint::apply_kv_quantization;

    const GB: u64 = 1_000_000_000;

    // what this catches (#287 slice 2): the governed host-cache lease inputs are
    // assembled from the HOST-RESIDENT mass — expert bytes (cache-funded, or hot in
    // VRAM) are subtracted BEFORE any weight-scaled heuristic runs, KV tracks the
    // LIVE window × lanes, and the OS floor derives from the box. K3-on-63GB
    // incident shape: full-file mass would derive a permanent ZERO lease (68GB KV
    // + 41GB buffer estimates off 663GB); the split derives a real budget.
    #[test]
    fn moe_lease_inputs_use_host_resident_mass_not_file_mass() {
        let file = 663 * GB;
        let experts = 640 * GB;
        let physical = 63 * GB;
        let inputs =
            moe_host_cache_lease_inputs("k3", file, experts, 262_144, 4096, 2, physical, None, 4096)
                .expect("host share is real");
        assert_eq!(
            inputs.weights_host_bytes,
            23 * GB,
            "dense/attention share only"
        );
        assert_eq!(
            inputs.live_kv_bytes,
            (23 * GB / 80_000) * 4096 * 2,
            "KV law over the HOST mass × live window × lanes"
        );
        assert_eq!(
            inputs.os_floor_bytes,
            physical / 8,
            "floor derives from the box"
        );
        let lease = crate::capacity::host_cache_lease::host_cache_lease_bytes(&inputs);
        assert!(
            lease > 20 * GB && lease < 30 * GB,
            "governed lease lands in a sane band on the incident shape, got {lease}"
        );

        // Blind to the expert split, the working set "costs" the whole file → the
        // lease saturates to zero and the cache is permanently off. The subtraction
        // is load-bearing, not cosmetic.
        let blind =
            moe_host_cache_lease_inputs("k3", file, 0, 262_144, 4096, 2, physical, None, 4096).unwrap();
        assert_eq!(
            crate::capacity::host_cache_lease::host_cache_lease_bytes(&blind),
            0,
            "full-file mass must starve the lease — proves why the split exists"
        );

        // Degenerate all-expert read → no honest host footprint → publish nothing.
        assert!(
            moe_host_cache_lease_inputs("k3", file, file, 262_144, 4096, 2, physical, None, 4096)
                .is_none()
        );
    }

    // what this catches: a pin SWAPS — serve() kills the incumbent llama-server
    // child, THEN launches the candidate (never co-resident) — so the fit-check must
    // credit the outgoing model's weights back into the budget. Without it, swapping
    // DOWN to a model that fits ALONE but not ALONGSIDE the incumbent is falsely
    // denied (glass-boxed 2026-07-21: pin Devstral 14GB refused while a 20GB 32B
    // teacher was resident, though evicting it frees enough). Regression for the
    // stronger-teacher swap-and-back the Academy needs.
    // what this catches: the swap/restore interleaving bug in the measurement demand
    // override (regression for the #2238 follow-up). With TWO overlapping quiesce
    // leases, releasing in ACQUISITION order under the old swap/restore scheme first
    // restored the pre-lease base over the still-held second lease, then restored the
    // FIRST lease's override — leaving the fleet's warm-slot demand stuck at the
    // measurement value with nobody quiesced. The authority must recompute from what
    // remains: any-order release, base restored only when the LAST override lifts.
    #[test]
    // what this catches: 16 resident minds planned for a boot floor of 2 (3 lanes with
    // 40 GB idle, 2026-09-14) — the roster IS the demand; the floor only bounds it
    // from below; a measurement lease still holds it down while held.
    fn the_lane_demand_follows_the_live_roster_above_the_boot_floor() {
        assert_eq!(resident_lane_demand(2, 16, false), 16);
        assert_eq!(resident_lane_demand(16, 5, false), 16, "the floor is a lower bound");
        assert_eq!(resident_lane_demand(2, 0, false), 2, "no residents yet = the floor");
        assert_eq!(resident_lane_demand(2, 16, true), 2, "a held quiesce lease wins");
        assert_eq!(resident_lane_demand(0, 0, false), 1);
    }

    #[test]
    fn overlapping_demand_overrides_release_in_any_order() {
        let cell = Arc::new(AtomicU32::new(0));
        let state = LaneDemandState {
            cell: cell.clone(),
            base: std::sync::Mutex::new(4),
            overrides: std::sync::Mutex::new(Vec::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        };
        state.recompute();
        assert_eq!(
            cell.load(Ordering::Relaxed),
            4,
            "base floor before any lease"
        );

        // Overlap: eval quiesce_all (active 0 → floored 1), then a solve's
        // quiesce_others (active 1) while the first is still held.
        let eval = state.acquire(0);
        assert_eq!(cell.load(Ordering::Relaxed), 1);
        let solve = state.acquire(1);
        assert_eq!(cell.load(Ordering::Relaxed), 1);

        // The killer interleaving: release in ACQUISITION order. The solve still
        // holds its lease, so demand must STAY at the measurement value...
        state.release(eval);
        assert_eq!(
            cell.load(Ordering::Relaxed),
            1,
            "first release must not restore the base over a still-held lease"
        );
        // ...and only the LAST release restores the base floor.
        state.release(solve);
        assert_eq!(
            cell.load(Ordering::Relaxed),
            4,
            "last release restores the base — never a stale override"
        );

        // Idempotent: releasing an unknown/stale id changes nothing.
        state.release(solve);
        assert_eq!(cell.load(Ordering::Relaxed), 4);

        // Concurrent measurements with different needs: never starve the larger one.
        let a = state.acquire(2);
        let b = state.acquire(1);
        assert_eq!(cell.load(Ordering::Relaxed), 2, "max of held overrides");
        state.release(a);
        assert_eq!(cell.load(Ordering::Relaxed), 1);
        state.release(b);
        assert_eq!(cell.load(Ordering::Relaxed), 4);
    }

    // what this catches (Cormac's review of #4197, 2026-09-18): a burst of leased-in calls
    // on ONE tick must not move the lane count. `--parallel` is a launch arg — a lane-count
    // change is a new llama-server and every in-flight generation dies with the old one —
    // so a raw per-tick peak would relaunch the seat and kill the very calls that raised
    // the count, then downshift, then repeat: the 718 flap through a new input. The
    // planner may act only on a load sustained for DOWNSHIFT_SUSTAINED_TICKS.
    #[test]
    fn a_burst_of_leased_in_calls_on_one_tick_does_not_move_the_lane_count() {
        let w = DOWNSHIFT_SUSTAINED_TICKS as usize;
        assert!(w >= 2, "the guard only means something with a window");
        assert_eq!(sustained_leased_in(&[3]), 0, "a fresh gauge never relaunches");
        assert_eq!(sustained_leased_in(&[3, 0, 0]), 0, "a burst that vanished is not demand");
        assert_eq!(sustained_leased_in(&[3, 3, 0]), 0, "two ticks is still a burst");
        assert_eq!(sustained_leased_in(&[3, 3, 3]), 3, "three consecutive ticks: a load");
        assert_eq!(sustained_leased_in(&[0, 3, 3, 3]), 3, "the window is the LAST w ticks");
        assert_eq!(sustained_leased_in(&[2, 3, 3]), 2, "the minimum, not the max: a dip is honored");
        assert_eq!(sustained_leased_in(&[3, 3, 3, 1]), 1, "a falling load falls at once");
    }

    #[test]
    fn pin_swap_down_credits_the_evicted_incumbents_weights() {
        let base = HostBudget {
            usable_bytes: 12 * GB,
            perf_cores: 10,
        };
        let footprint = |id: &str, weights_gb: u64, rank: u8| ModelFootprint {
            model_id: id.into(),
            weights_bytes: weights_gb * GB,
            kv_per_token: 100_000, // ~0.2GB KV at 2048 ctx — small, not the binding term
            context_window: 32768,
            capability_rank: rank,
            fixed_per_lane_bytes: 0,
        };
        let candidate = footprint("devstral-24b", 14, 8);
        let incumbent = footprint("qwen-32b", 20, 10);

        // No incumbent credited → the 14GB candidate does NOT fit the raw 12GB budget.
        let no_credit = pin_fit_decision(base, Some(candidate.clone()), None);
        assert!(
            no_credit.plan.map(|p| !p.fits_on_gpu).unwrap_or(true),
            "candidate must not fit the raw 12GB budget"
        );

        // Crediting the evicted 20GB incumbent lifts the budget to 32GB → it fits.
        let credited = pin_fit_decision(base, Some(candidate), Some(&incumbent));
        assert_eq!(
            credited.budget_bytes,
            12 * GB + 20 * GB,
            "the evicted incumbent's weights are credited back into the pin budget"
        );
        assert!(
            credited.plan.expect("a plan is produced").fits_on_gpu,
            "candidate must fit its lane once the incumbent is evicted"
        );
    }

    // what this catches: the ServingSteadyHold RAII gauge — acquiring sets
    // serving_held_steady() true; dropping clears it; nesting is reference-counted so a
    // concurrent second eval doesn't release the first's hold. This is the gate that stops
    // the grow-back re-home from relaunching the lane under a running eval (hard-rs 0/8).
    // regression for the 2026-07-20 shared-lane bounce.
    // what this catches (2026-09-17, the M5): a steady hold pinning a boot shape of
    // 8 × 12,800 against a plan of 3 × 67,340 for 255 ticks. The hold yields to a
    // shortfall of REHOME_HOLD_OVERRIDE_PCT or more; a few-percent grow-back stays held.
    // what this catches (2026-09-19 17:56–18:20Z, the M5 at 3 lanes then 1): on unified
    // memory a plan filled to Comfort's fraction sat inside the Eco band, the third lane
    // went in with zero slack, the compressor's response read as co-consumer growth, and
    // the plan collapsed. The fill on unified memory stops ECO_EXIT_BYTES short of the
    // budget: the M5's 40.3 GB budget plans 2 × 67k (27.4 GB) and never 3; Eco's own
    // fraction still rules when it is tighter; a small box keeps the mode alone; discrete
    // VRAM is unchanged.
    #[test]
    fn a_unified_plan_never_fills_within_the_eco_band() {
        use crate::cognition::serving_plan::ModelFootprint;
        use crate::provisioning::model_catalog::{PowerMode, ECO_EXIT_BYTES};
        let live = 40_300_000_000u64; // the M5 at rest: available 13.4 GB + serving 26.9 GB
        let comfort = PowerMode::Comfort.serving_fraction();
        let fill = plan_fill(live, comfort, true);
        assert_eq!(fill, live - ECO_EXIT_BYTES, "the band, not the fraction, is the ceiling on the M5");
        assert!(fill < (live as f64 * comfort) as u64);
        assert_eq!(plan_fill(live, comfort, false), (live as f64 * comfort) as u64, "discrete VRAM: the mode alone");
        let eco = PowerMode::Eco.serving_fraction();
        assert_eq!(plan_fill(live, eco, true), (live as f64 * eco) as u64, "Eco is tighter than the band: Eco rules");
        let small = 20_000_000_000u64;
        assert_eq!(plan_fill(small, comfort, true), (small as f64 * comfort) as u64, "a small box: the band would leave less than Eco does; the mode alone");
        // The M5's own fit at that ceiling: two 67k lanes of the 27B, never three.
        let m5 = ModelFootprint {
            model_id: "qwen-27b".into(), weights_bytes: 19_000_000_000, kv_per_token: 32_768, context_window: 262_144, capability_rank: 42, fixed_per_lane_bytes: 0,
        };
        let floor = 64_052; // typical prompt 51,242 × 1.25
        assert!(m5.window_within(fill, 2) >= floor, "two lanes clear the floor: {}", m5.window_within(fill, 2));
        assert!(m5.window_within(fill, 3) < floor, "three do not — the zero-slack third lane is what collapsed the box");
        let by_mode = (live as f64 * comfort) as u64;
        assert!(m5.window_within(by_mode, 3) >= floor, "under the bare fraction the third lane fit, with nothing to spare");
    }

    // what this catches (#4230, BigMama's condition): with the plan's own reserve gone, a
    // foundry's headroom of 1.0 must still stop at what Performance keeps — the serving
    // process's fixed overhead is not in the footprint, and 100% of the ceiling is an OOM.
    #[test]
    fn a_foundry_headroom_of_one_still_keeps_the_process_overhead() {
        use crate::provisioning::model_catalog::PowerMode;
        assert_eq!(plan_fraction(1.0), PowerMode::Performance.serving_fraction());
        assert!(plan_fraction(1.0) < 1.0);
        assert_eq!(plan_fraction(0.8), 0.8, "the everyday default is the operator's own number");
    }

    #[test]
    fn a_steady_hold_yields_to_a_large_shortfall_and_keeps_a_small_one() {
        let (live, _plan, gain, _) = rehome_gain_evidence(12_800, 8, 67_340, 3);
        assert!(hold_overridden_by_shortfall(live, gain), "8 × 12.8k vs 3 × 67k is starvation, not a slot to protect");
        let (live, _plan, gain, worth) = rehome_gain_evidence(56_000, 5, 67_000, 5);
        assert!(worth, "a ~20% grow-back is worth a re-home in general");
        assert!(!hold_overridden_by_shortfall(live, gain), "but not worth dropping a held slot for");
        assert!(!hold_overridden_by_shortfall(0, 1), "no live space = nothing to compare");
    }

    #[test]
    fn serving_steady_hold_is_refcounted_and_raii() {
        assert!(!serving_held_steady(), "no hold at rest");
        {
            let _h1 = ServingSteadyHold::acquire("test");
            assert!(serving_held_steady(), "one hold ⇒ steady");
            {
                let _h2 = ServingSteadyHold::acquire("test");
                assert!(serving_held_steady(), "two holds ⇒ still steady");
            }
            assert!(
                serving_held_steady(),
                "inner drop must not release the outer hold"
            );
        }
        assert!(
            !serving_held_steady(),
            "all holds dropped ⇒ grow-back resumes"
        );
    }

    // what this catches: the ServingLudicrousHold RAII gauge — while held, serving plans at
    // PowerMode::Performance (the whole GPU, biggest window), overriding the timid pressure
    // read; refcounted so concurrent benchmarks compose; RAII-released so serving reverts to
    // the live pressure-adaptive mode. This is the "extreme mode for benchmarks" gate that
    // stops a starved eco boot-window from being what the exam is measured on.
    // [[serving-mode-follows-activity-ludicrous-to-dream]]
    #[test]
    fn serving_ludicrous_hold_is_refcounted_and_raii() {
        assert!(!serving_ludicrous_active(), "no ludicrous demand at rest");
        {
            let _h1 = ServingLudicrousHold::acquire();
            assert!(
                serving_ludicrous_active(),
                "one hold ⇒ Performance override active"
            );
            {
                let _h2 = ServingLudicrousHold::acquire();
                assert!(serving_ludicrous_active(), "two holds ⇒ still active");
            }
            assert!(
                serving_ludicrous_active(),
                "inner drop must not release the outer hold"
            );
        }
        assert!(
            !serving_ludicrous_active(),
            "all holds dropped ⇒ back to pressure-adaptive mode"
        );
    }

    // what this catches (card 48f5438a): a below-floor plan is refused ONLY when there
    // is an incumbent to preserve. A COLD BOOT (no incumbent) must SERVE the plan rather
    // than refuse into a gap and boot the node DARK — measured on the Intel tier
    // 2026-09-17: #4169 without #4174 sized a CPU host below its capability floor, the
    // gate refused into "the previous plan stands" with no previous plan, and the node
    // reported resident 0 / active_model None while a servable 1.5B sat on disk. The
    // floor guards SINKING from an incumbent; on a cold boot there is nothing to sink
    // from. Against the pre-fix unconditional refuse the cold-boot assert fails.
    #[test]
    fn a_cold_boot_below_the_floor_serves_rather_than_booting_dark() {
        let below = FloorVerdict::Below {
            floor: "qwen2.5-coder-4b".to_string(),
            wanted: "qwen2.5-coder-1.5b".to_string(),
        };
        use std::time::Duration;
        let grace = COLD_BOOT_BELOW_FLOOR_GRACE;
        // Below the floor WITH an incumbent → refuse, however long (a host never sinks).
        assert!(below_floor_refuses(&below, true, Duration::ZERO), "with an incumbent, hold the floor");
        assert!(below_floor_refuses(&below, true, grace * 10), "…for as long as it takes");
        // Below the floor with NO incumbent: a fresh reading is a teardown transient →
        // refuse (the 5090's 53 GB card must not latch a 4B on one 6 GB reading)…
        assert!(
            below_floor_refuses(&below, false, Duration::ZERO),
            "a first below-floor reading with nothing serving is a transient, not a shortfall"
        );
        // …and a shortfall that has outlived the grace → SERVE, never stay dark.
        assert!(
            !below_floor_refuses(&below, false, grace),
            "a cold boot held below the floor past the grace serves the viable plan (48f5438a)"
        );
        // At or above the floor never refuses, incumbent or not.
        assert!(!below_floor_refuses(&FloorVerdict::AtOrAbove, true, Duration::ZERO));
        assert!(!below_floor_refuses(&FloorVerdict::AtOrAbove, false, grace));
    }

    // what this catches: regression #4146 on the Intel tier — a CPU-placed host
    // (`--n-gpu-layers 0`) sized against its decorative GPU instead of the system RAM
    // the KV actually lives in, which bound the kept window to MIN_SERVE_CTX (2,048),
    // hard-refused every ~2,300-token prompt and produced 36/36 inference_failed with
    // ZERO acts across eight citizens. The CPU arm must read RAM and the GPU arm must
    // read VRAM; a VRAM-only implementation (the pre-fix code) fails the first assert.
    #[test]
    fn physical_capacity_follows_the_placement_not_the_installed_device() {
        use crate::inference::llama_server::LanePlacement;
        // The measured Intel tier: 3.89 GB Radeon Pro 560X at used_mb 0.0, 32 GB RAM.
        let vram = 3_891_000_000u64;
        let ram = 32 * GB;

        let cpu = physical_usable_bytes(LanePlacement::Cpu, vram, ram, 0.80);
        let gpu = physical_usable_bytes(LanePlacement::Gpu, vram, ram, 0.80);

        // THE REGRESSION ASSERT: a CPU-placed host must not be bounded by a card that
        // holds zero bytes. Pre-fix both arms returned the VRAM figure, so cpu == gpu.
        assert!(
            cpu > gpu,
            "CPU placement must size against RAM, not the idle card: cpu={cpu} gpu={gpu}"
        );
        assert_eq!(cpu, (ram as f64 * 0.80) as u64, "CPU arm reads system RAM");
        assert_eq!(gpu, (vram as f64 * 0.80) as u64, "GPU arm still reads VRAM");

        // And the fix must not hand a GPU host its (often larger) RAM figure — that
        // would over-commit the card, which is the 2026-07-20 error in reverse.
        assert!(
            gpu < ram,
            "GPU placement must stay bounded by the device: {gpu} vs {ram} RAM"
        );
    }

    // what this catches: the budget is LIVE (tracks free memory, not capacity),
    // capped at physical VRAM, with headroom, cores floored at 1 — the organic
    // "free memory drops → budget drops" behavior.
    #[test]
    fn host_budget_tracks_live_free_memory() {
        // 40GB free on a 53GB device → budget from the 40, with headroom.
        let b = host_budget_from(&HostBudgetInputs {
            available_bytes: 40 * GB,
            total_vram_bytes: 53 * GB,
            perf_cores: 6,
            budget_fraction: 0.80,
        });
        assert!(b.usable_bytes < 40 * GB, "must reserve headroom");
        assert!(
            b.usable_bytes >= 30 * GB,
            "but most of free is ours: {}",
            b.usable_bytes
        );

        // Organic: less free memory → smaller budget (a game grabbed memory).
        let busy = host_budget_from(&HostBudgetInputs {
            available_bytes: 6 * GB,
            total_vram_bytes: 53 * GB,
            perf_cores: 6,
            budget_fraction: 0.80,
        });
        assert!(
            busy.usable_bytes < b.usable_bytes,
            "less free → smaller budget"
        );

        // Never plan above physical VRAM even if the OS reports more free RAM
        // (unified memory: free RAM can exceed the VRAM serving ceiling).
        let capped = host_budget_from(&HostBudgetInputs {
            available_bytes: 100 * GB,
            total_vram_bytes: 53 * GB,
            perf_cores: 6,
            budget_fraction: 0.80,
        });
        assert!(capped.usable_bytes <= 53 * GB, "capped at physical VRAM");

        assert_eq!(
            host_budget_from(&HostBudgetInputs {
                available_bytes: 8 * GB,
                total_vram_bytes: 8 * GB,
                perf_cores: 0,
                budget_fraction: 0.80,
            })
            .perf_cores,
            1,
            "cores floored at 1"
        );
    }

    // what this catches: an M5 Pro (or any capable silicon) must NOT classify
    // as Compat (which would force n_gpu_layers=0 = CPU); unknown hardware
    // stays on the safe LCD/CPU fallback.
    #[test]
    fn detect_tier_classifies_silicon() {
        use crate::persona::hw_tier_descriptor::HwTierCategory;
        assert_eq!(detect_tier("Apple M5 Pro").1, HwTierCategory::MSeriesPro);
        assert_eq!(detect_tier("Apple M2").1, HwTierCategory::MSeries);
        assert_eq!(
            detect_tier("NVIDIA GeForce RTX 5090").1,
            HwTierCategory::Cuda
        );
        assert_eq!(detect_tier("llvmpipe").1, HwTierCategory::Compat);
    }

    // what this catches: the daemon publishes the classifier's decision to its
    // watch channel — Some(plan) with the most-capable fitting model when there
    // are candidates, None (no silent serve) when there are none.
    #[tokio::test]
    async fn publish_plan_drives_the_watch() {
        let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
        let system = Arc::new(SystemResourceMonitor::new());
        let mut daemon = ServingDaemonModule::new(gpu, system, test_resource_daemon(), test_catalog(), test_pin_store());
        daemon.working_set = crate::cognition::working_set::WorkingSetRegistry::new();
        daemon.set_leased_in_sent_source(Arc::new(Vec::new));
        let rx = daemon.subscribe();
        assert!(rx.borrow().is_none(), "starts unpublished");
        // A measured demand, as every live node has (the seat's leased-in sample ring is
        // process-global across tests; a requirement it carries needs a ceiling above it).
        daemon.working_set.record(uuid::Uuid::new_v4(), 200_000, 1);

        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        // The candidates carry real trained windows (2eec3977: a persona seat holds what the
        // residents require; an 8k-window model cannot).
        let candidates = vec![
            footprint_from_parts("small", GB, 4096, false, None).unwrap(),
            footprint_from_parts("coder-14b", 9 * GB, 262_144, true, None).unwrap(),
        ];
        daemon.publish_plan(budget, &candidates, &candidates);
        let plan = rx.borrow().clone().expect("plan published");
        assert_eq!(
            plan.base_model.model_id, "coder-14b",
            "most capable that fits"
        );
        assert!(plan.fits_on_gpu);

        // No candidates → None published (no silent serve).
        daemon.publish_plan(budget, &[], &[]);
        assert!(rx.borrow().is_none(), "empty candidates → no plan");
        // What this catches: publication must retain the revision used to select
        // candidates, never borrow newer intent and falsely stamp the old plan.
        let selected_revision = daemon.intent.snapshot().revision;
        daemon.intent.set_pin(Some("new-user-choice".into()), true);
        daemon.publish_plan_for_intent(budget, &candidates, &candidates, selected_revision);
        assert_eq!(daemon.plan_tx.borrow().intent_revision, selected_revision);
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "stale plan cannot dispatch"
        );
        assert_ne!(
            daemon.plan_tx.borrow().intent_revision,
            daemon.intent.snapshot().revision
        );
    }

    // what this catches: a plan whose window is below the residents' REQUIREMENT is never PUBLISHED —
    // the previous plan stands. Regression for the 5090 (2026-09-20, 2 × 2,048 all
    // afternoon, 2,932 prompts refused) and the M5's 1 × 2,048 boot: a starved geometry
    // must not become the served target, whatever the planner's arithmetic says.
    #[tokio::test]
    async fn a_plan_below_one_coding_turn_is_not_published_and_the_previous_stands() {
        let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
        let system = Arc::new(SystemResourceMonitor::new());
        let mut daemon = ServingDaemonModule::new(gpu, system, test_resource_daemon(), test_catalog(), test_pin_store());
        // This test OWNS its demand pool: a private working set and an empty leased-in ring.
        // Both are process globals in production, shared by every test in the binary.
        daemon.working_set = crate::cognition::working_set::WorkingSetRegistry::new();
        daemon.set_leased_in_sent_source(Arc::new(Vec::new));
        let rx = daemon.subscribe();
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 262_144, true, None).unwrap()];
        // The residents' requirement: a 56k typical prompt (the 5090's p50), with headroom.
        // No residents are live in this harness, so the seat's leased-in samples carry it —
        // the same ring #4256 votes refused prompts into.
        // …in the daemon's OWN working set, never the process-global leased-in ring: that
        // ring is shared by every test in the binary, and eight 56k samples there re-homed
        // a 65k lane in `a_jittering_gain_never_accumulates_into_a_re_home` (CI, 2026-09-20).
        let resident = uuid::Uuid::new_v4();
        for i in 0..8 {
            daemon.working_set.record_sent(resident, 56_057, 1 + i);
        }
        // …and a measured demand ceiling above it (the assembled context a turn wanted),
        // as every live node has — without it the demand cap is the bootstrap prior and no
        // plan can hold a 56k requirement, which is a harness artifact, not a law.
        daemon.working_set.record(uuid::Uuid::new_v4(), 200_000, 1);
        // A roomy budget: a real window is published.
        daemon.publish_plan(HostBudget { usable_bytes: 45 * GB, perf_cores: 6 }, &candidates, &candidates);
        let good = rx.borrow().clone().expect("a real plan is published");
        assert!(
            good.served_context_window >= 70_071,
            "the roomy plan holds the residents' requirement (56k × 1.25): {}",
            good.served_context_window
        );
        // A squeezed budget: the planner's arithmetic yields a window below one coding
        // turn — the daemon refuses to publish it and the good plan stands.
        daemon
            .intent
            .set_pin(Some(good.base_model.model_id.clone()), true);
        daemon.publish_plan(HostBudget { usable_bytes: 9 * GB + 200 * 1_000_000, perf_cores: 6 }, &candidates, &candidates);
        let after = rx.borrow().clone().expect("the previous plan still stands");
        assert_eq!(
            daemon.plan_tx.borrow().intent_revision,
            daemon.intent.snapshot().revision,
            "held geometry is revalidated under same-value explicit intent"
        );
        assert_eq!(after.served_context_window, good.served_context_window, "a 2k plan never replaces a real one");
        assert_eq!(after.lanes, good.lanes);
        let held_revision = daemon.plan_tx.borrow().intent_revision;
        daemon.intent.set_pin(Some("different-model".into()), true);
        let different =
            vec![footprint_from_parts("different-model", GB, 8192, true, None).unwrap()];
        daemon.publish_plan_for_intent(
            HostBudget {
                usable_bytes: 2 * GB,
                perf_cores: 6,
            },
            &different,
            &different,
            daemon.intent.snapshot().revision,
        );
        assert_eq!(
            daemon.plan_tx.borrow().intent_revision,
            held_revision,
            "refusing a new candidate cannot relabel the excluded old plan as current intent"
        );
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "excluded retained model cannot dispatch"
        );
        // With NO plan published (a cold boot) and a requirement the box cannot hold, the
        // best runnable plan is still published — a dark node is worse than a starved seat
        // (Cormac's condition on #4257: IntelMac's 32k-trained 1.5B against a 58k typical).
        let mut cold = ServingDaemonModule::new(
            Arc::new(GpuMemoryManager::simulated("Intel", 16 * GB)),
            Arc::new(SystemResourceMonitor::new()),
            test_resource_daemon(),
            test_catalog(),
            test_pin_store(),
        );
        cold.working_set = crate::cognition::working_set::WorkingSetRegistry::new();
        cold.set_leased_in_sent_source(Arc::new(Vec::new));
        cold.working_set.record(uuid::Uuid::new_v4(), 200_000, 1);
        let rx_cold = cold.subscribe();
        let small = vec![footprint_from_parts("coder-1.5b", GB, 32_768, true, None).unwrap()];
        cold.publish_plan(HostBudget { usable_bytes: 12 * GB, perf_cores: 4 }, &small, &small);
        let starved = rx_cold.borrow().clone().expect("a cold boot publishes the best runnable plan, starved or not");
        assert!(starved.served_context_window <= 32_768);
        assert!(
            !crate::cognition::serving_plan::persona_lane_holds(starved.served_context_window, Some(70_071)),
            "it IS below the requirement — published and said, never dark"
        );
    }

    // what this catches: the `serving/*` surface (plan · status · load · unload) is
    // migrated to the typed registry; the legacy handle_command is gone, so for any
    // command name it must fail loud — never silently fall back to an empty result.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
        let system = Arc::new(SystemResourceMonitor::new());
        let daemon = ServingDaemonModule::new(
            gpu,
            system,
            test_resource_daemon(),
            test_catalog(),
            test_pin_store(),
        );
        let err = daemon
            .handle_command("serving/plan", serde_json::json!({}))
            .await
            .expect_err("legacy handler must fail loud after migration");
        assert!(
            err.contains("migrated to the typed registry"),
            "error must name the migration: {err}"
        );
    }

    // what this catches: the daemon contributes the full typed serving surface,
    // sharing its OWN watch receivers + catalog (so the read surfaces report the
    // daemon's live decision/snapshot). A regression that drops a verb is caught.
    #[tokio::test]
    async fn contributes_the_full_serving_surface() {
        let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
        let system = Arc::new(SystemResourceMonitor::new());
        let daemon = ServingDaemonModule::new(
            gpu,
            system,
            test_resource_daemon(),
            test_catalog(),
            test_pin_store(),
        );
        let mut names: Vec<&str> = daemon.commands().iter().map(|c| c.name()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            vec![
                "serving/cache-probe",
            "serving/load",
                "serving/pin",
                "serving/plan",
                "serving/status",
                "serving/unload",
                "serving/unpin"
            ]
        );
    }

    use crate::inference::llama_server::LlamaServerError;
    use std::sync::atomic::AtomicUsize;

    /// Fake serving control: counts serve() calls and reports nothing live
    /// (Unreachable), so reconcile always decides to (re)serve. A daemon-level
    /// stub for the reconcile WIRING; the reconcile DECISION itself is tested in
    /// `inference::llama_server` against its own fake.
    struct FakeServer {
        serves: Arc<AtomicUsize>,
        idles: Arc<AtomicUsize>,
        ok: bool,
        /// Drives [`LlamaServerControl::decode_smoke_ok`] so a test can wedge the lane's
        /// COMPUTE path (probe → false) independently of its control plane. Defaults true
        /// (a healthy lane decodes). See [`FakeServer::healthy`].
        smoke_ok: Arc<AtomicBool>,
        /// The stderr watcher's report channel, as the real `LlamaServerProcess` exposes it.
        /// A test raises this to stand in for a slot printing impossible progress.
        wedge: crate::inference::wedge::WedgeFlag,
        /// Drives [`LlamaServerControl::slots_activity_fingerprint`] (L11). 0 = the
        /// endpoint is unreadable (`None`); any other value is the fingerprint. A test
        /// bumps it to stand in for the serve loop advancing between smoke misses.
        slots_fp: Arc<AtomicU64>,
        paging_endpoint: Option<Arc<crate::inference::slots::EndpointSlots>>,
        verified_reads: Arc<AtomicUsize>,
        probe_effect: Option<Arc<dyn Fn() + Send + Sync>>,
        active: Option<String>,
        serve_started: Option<Arc<tokio::sync::Notify>>,
        probe_started: Option<Arc<tokio::sync::Notify>>,
        /// `/slots` reads served ([`LlamaServerControl::slots_body`]): the prefill knee's.
        slots_reads: Arc<AtomicUsize>,
    }

    impl FakeServer {
        /// A healthy fake: serve() outcome = `ok`, decode heartbeat passes.
        fn healthy(serves: Arc<AtomicUsize>, ok: bool) -> Self {
            Self {
                serves,
                ok,
                idles: Arc::new(AtomicUsize::new(0)),
                smoke_ok: Arc::new(AtomicBool::new(true)),
                wedge: Default::default(),
                slots_fp: Default::default(),
                paging_endpoint: None,
                verified_reads: Arc::new(AtomicUsize::new(0)),
                probe_effect: None,
                active: None,
                serve_started: None,
                probe_started: None,
                slots_reads: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    #[async_trait]
    impl LlamaServerControl for FakeServer {
        fn paging_recovery_required(&self) -> bool {
            self.paging_endpoint
                .as_ref()
                .is_some_and(|endpoint| endpoint.paging_recovery_required())
        }

        fn wedge_flag(&self) -> Option<crate::inference::wedge::WedgeFlag> {
            Some(self.wedge.clone())
        }

        fn owned_serving_target(
            &self,
        ) -> Option<crate::inference::llama_server::OwnedServingTarget> {
            self.verified_reads.fetch_add(1, Ordering::SeqCst);
            None
        }
        async fn active_model(&self) -> Result<Option<String>, LlamaServerError> {
            if let Some(effect) = &self.probe_effect {
                effect();
            }
            if let Some(started) = &self.probe_started {
                started.notify_one();
                std::future::pending::<()>().await;
            }
            match &self.active {
                Some(active) => Ok(Some(active.clone())),
                None => Err(LlamaServerError::Unreachable("test: nothing up".into())),
            }
        }
        async fn active_adapters(&self) -> Result<Vec<String>, LlamaServerError> {
            Ok(Vec::new())
        }
        async fn idle(&self) -> Result<(), LlamaServerError> {
            self.idles.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn serve(&self, _target: &ServingTarget) -> Result<(), LlamaServerError> {
            self.serves.fetch_add(1, Ordering::SeqCst);
            if let Some(started) = &self.serve_started {
                started.notify_one();
                std::future::pending::<()>().await;
            }
            if self.ok {
                Ok(())
            } else {
                Err(LlamaServerError::Spawn("test boom".into()))
            }
        }
        async fn served_context_window(&self) -> Result<u32, LlamaServerError> {
            // After a successful (test) serve the daemon reads the real per-slot
            // window; a fixed non-zero value stands in for the live `/props` read
            // so the reconcile publishes a READY snapshot carrying a real window.
            Ok(11008)
        }
        async fn served_lanes(&self) -> Result<u32, LlamaServerError> {
            // 0 = "this fake's /props names no slot count", i.e. nothing to compare
            // on the lane axis. These daemon tests assert the window/readiness path,
            // so the lane operand stays inert exactly as it was before it was probed.
            Ok(0)
        }
        async fn decode_smoke_ok(&self) -> bool {
            // Driven by `smoke_ok` so a test can wedge the COMPUTE path (the #175
            // liveness-heartbeat tests) independently of the control plane; defaults
            // true (a healthy fake decodes).
            self.smoke_ok.load(Ordering::Relaxed)
        }
        async fn slots_body(&self) -> Option<serde_json::Value> {
            self.slots_reads.fetch_add(1, Ordering::SeqCst);
            Some(serde_json::json!([]))
        }

        async fn slots_activity_fingerprint(&self) -> Option<u64> {
            match self.slots_fp.load(Ordering::Relaxed) {
                0 => None,
                v => Some(v),
            }
        }
    }

    /// A minimal [`Model`] for reconcile-wiring tests — only `id` is load-bearing
    /// (the FakeServer ignores the rest); the planned served window rides on the
    /// `ServingTarget`, not here.
    // what this catches (card af635057, the 5090 after a reboot): an embedding or
    // reranking row — Ready, with a real GGUF — chosen as the base model. A mind needs
    // text generation; a row without it is never a candidate, pinned or not.
    #[test]
    fn a_model_that_cannot_speak_is_never_a_base_candidate() {
        use crate::model_registry::types::Capability;
        let mut speaks = fake_model("chat/model");
        speaks.capabilities = [Capability::Chat, Capability::TextGeneration].into_iter().collect();
        assert!(can_serve_minds(&speaks));
        let mut embeds = fake_model("embed/model");
        embeds.capabilities = [Capability::Embedding].into_iter().collect();
        assert!(!can_serve_minds(&embeds), "an embedding-only row cannot host a mind");
        let mut mute = fake_model("mute/model");
        mute.capabilities.clear();
        assert!(!can_serve_minds(&mute), "no capabilities = no candidate, never a guess");
    }

    fn fake_model(id: &str) -> Model {
        use crate::model_registry::types::{Arch, MultiPartyChatStrategy};
        Model {
            weights_bytes: None,
            mmproj_bytes: None,
            id: id.to_string(),
            name: None,
            provider: "llamacpp-local".to_string(),
            arch: Arch::Qwen2,
            context_window: 262_144,
            max_output_tokens: 4096,
            tokens_per_second: 0.0,
            // A fixture row that can SPEAK: the base-model candidate set now requires
            // TextGeneration or Chat (can_serve_minds, card af635057), and every real
            // catalog row carries them; an empty set here would model an embedding row.
            capabilities: [
                crate::model_registry::types::Capability::Chat,
                crate::model_registry::types::Capability::TextGeneration,
            ]
            .into_iter()
            .collect(),
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: None,
            hf_source: None,
            gguf_local_path: None,
            chat_template: None,
            stop_sequences: Vec::new(),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            mmproj_local_path: None,
            parameter_count: 0,
            sampling: crate::model_registry::types::ModelSampling::default(),
            persona_serving_eligible: true,
            serving: Default::default(), // test/fixture literal: substrate defaults (text-only main lane, unverified kv-shift)
        }
    }

    /// A live catalog seeded from the real registry — the shared universe the
    /// daemon plans off. Tests that drive `publish_plan` directly pass candidates
    /// explicitly and never touch it; the reconcile-wiring tests just need it to
    /// exist so the daemon can be constructed.
    fn test_catalog() -> Arc<ModelCatalog> {
        let reg = crate::model_registry::catalog::registry().expect("Rust catalog must validate");
        Arc::new(ModelCatalog::from_registry(&reg))
    }

    /// A resource authority for daemon construction in tests — primed with a
    /// generous mock VRAM ceiling so that if a test ever reaches `host_budget()`
    /// it reads a real governed number rather than the fail-closed 0. Calls
    /// `ResourceDaemon::start`, which spawns its tick task, so every test that
    /// constructs a daemon through `daemon_with` must be `#[tokio::test]`.
    /// A pin store under a tempdir the process owns for its lifetime: a test daemon
    /// must never read the machine's real pin (it would plan under whatever the box
    /// last pinned) nor write it (2026-09-16: `cargo test` re-pinned every dev box).
    fn test_pin_store() -> crate::modules::serving_pin_store::ServingPinStore {
        static DIR: std::sync::LazyLock<tempfile::TempDir> =
            std::sync::LazyLock::new(|| tempfile::tempdir().expect("tempdir"));
        crate::modules::serving_pin_store::ServingPinStore::under_home(DIR.path())
    }

    fn test_resource_daemon() -> Arc<ResourceDaemon> {
        use crate::resources::{DaemonConfig, MockCapacitySource};
        ResourceDaemon::start(
            vec![Arc::new(MockCapacitySource::new(
                ResourceKind::Vram,
                53 * GB,
            ))],
            vec![],
            DaemonConfig::default(),
        )
    }

    // what this catches (5090, 2026-09-06 — BigMama's third bug): an empty plan
    // publishing "idle" while the llama-server stays alive. At the live → empty
    // transition the daemon takes the lane down exactly once; a second tick with
    // the plan still empty does not idle again.
    #[tokio::test]
    async fn an_empty_plan_takes_the_live_lane_down_once() {
        let serves = Arc::new(AtomicUsize::new(0));
        let server = Arc::new(FakeServer::healthy(serves, true));
        let idles = server.idles.clone();
        // An EMPTY catalog, so the plan is None on every host — the real catalog would
        // find this Mac's own GGUFs and plan a model, which is not this test's question.
        let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
        let system = Arc::new(SystemResourceMonitor::new());
        let empty = crate::model_registry::Registry::from_catalog(vec![], vec![]).expect("empty registry");
        let daemon = ServingDaemonModule::with_control(
            gpu,
            system,
            test_resource_daemon(),
            server,
            Arc::new(ModelCatalog::from_registry(&empty)),
            test_pin_store(),
        );
        let mut live = ServingSnapshot::empty();
        live.active_model = Some("ornith-ai/Ornith-1.5-35B-A3B-GGUF".to_string());
        live.ready = true;
        let _ = daemon.serving_tx.send_replace(live);
        // Nothing on disk is servable in a test → the reconcile's empty arm.
        daemon.recompute();
        let queued = daemon.reconcile_to_plan().expect("empty-plan operation");
        assert!(daemon.reconcile_to_plan().is_none(), "one operation owner");
        queued.abort();
        assert!(queued.await.unwrap_err().is_cancelled());
        assert!(
            !daemon.reconciling.load(Ordering::Acquire),
            "abort before first poll releases gate"
        );
        assert_eq!(idles.load(Ordering::SeqCst), 0);
        let superseded = daemon.reconcile_to_plan().expect("queued idle");
        daemon.intent.set_pin(None, true);
        superseded.await.unwrap();
        assert_eq!(
            idles.load(Ordering::SeqCst),
            0,
            "newer intent supersedes queued idle"
        );
        daemon.recompute();
        daemon
            .reconcile_to_plan()
            .expect("current idle")
            .await
            .unwrap();
        assert_eq!(
            idles.load(Ordering::SeqCst),
            1,
            "the lane is taken down at live → empty"
        );
        daemon.recompute();
        assert!(daemon.reconcile_to_plan().is_none());
        assert_eq!(
            idles.load(Ordering::SeqCst),
            1,
            "still empty: no second teardown"
        );
        daemon
            .serving_tx
            .send_modify(|snapshot| snapshot.loading_model = Some("cancelled-boot".into()));
        daemon
            .reconcile_to_plan()
            .expect("empty intent retires an interrupted load")
            .await
            .unwrap();
        assert_eq!(idles.load(Ordering::SeqCst), 2);
        assert!(daemon.serving_tx.borrow().loading_model.is_none());
    }

    // What this catches: the explicit command's real owner must retain a shared
    // generation after caller cancellation, reject overlapping batches, and return
    // the existing graded corpus/actual receipt without retiring resident weights.
    #[tokio::test]
    async fn explicit_teacher_batch_keeps_shared_generation_owned_until_terminal() {
        // The real shared adapter consumes the process-wide serving watch. Run
        // this boot-wiring fixture in a fresh copy of the existing test binary,
        // so its OnceLock cannot capture or contaminate parallel test daemons.
        const CHILD_ENV: &str = "CONTINUUM_SHARED_TEACHER_FIXTURE_CHILD";
        if std::env::var_os(CHILD_ENV).is_none() {
            let mut child = tokio::process::Command::new(std::env::current_exe().unwrap());
            child.args([
                "--exact",
                "modules::serving_daemon::tests::explicit_teacher_batch_keeps_shared_generation_owned_until_terminal",
                "--nocapture",
            ]).env(CHILD_ENV, "1").kill_on_drop(true);
            #[cfg(windows)]
            child.creation_flags(0x08000000); // CREATE_NO_WINDOW: no interactive fixture console.
            let output = child.output().await.unwrap();
            assert!(
                output.status.success(),
                "isolated shared teacher fixture failed:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        crate::model_registry::init_global().expect("model registry for real teacher adapter");
        use crate::cognition::eval::EvalTask;
        use crate::modules::serving_daemon::academy_batch::TeacherBatchRequest;
        use axum::{routing::post, Router};
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Semaphore::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let app = Router::new().route("/v1/chat/completions", post({
            let entered = entered.clone(); let release = release.clone(); let calls = calls.clone();
            move || {
                let entered = entered.clone(); let release = release.clone(); let calls = calls.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    entered.notify_one();
                    release.acquire().await.unwrap().forget();
                    let chunk = serde_json::json!({
                        "id":"owned-teacher-fixture", "model":"teacher-fixture",
                        "choices":[{"index":0,"delta":{"content":"```rust\nfn answer() -> i32 { 2 }\n```"},"finish_reason":null}]
                    });
                    let end = serde_json::json!({"id":"owned-teacher-fixture","model":"teacher-fixture",
                        "choices":[{"index":0,"delta":{},"finish_reason":"stop"}],
                        "usage":{"prompt_tokens":10,"completion_tokens":12,"total_tokens":22}});
                    ([("content-type", "text/event-stream")], format!("data: {chunk}\n\ndata: {end}\n\ndata: [DONE]\n\n"))
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let root = format!("http://{}", listener.local_addr().unwrap());
        let http = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let serves = Arc::new(AtomicUsize::new(0));
        let mut configured = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));
        configured.set_model_resolver(Arc::new(|id| {
            let mut model = fake_model(id);
            model.provider = crate::inference::llama_server::PROVIDER_ID.into();
            Some(model)
        }));
        let daemon = Arc::new(configured);
        let mut snapshot = ServingSnapshot::empty();
        snapshot.active_model = Some("teacher-fixture".into());
        snapshot.base_url = format!("{root}/v1");
        snapshot.ready = true;
        snapshot.served_context_window = 32768;
        snapshot.lanes = 2;
        daemon.serving_tx.send_replace(snapshot);
        assert!(
            crate::inference::llama_server::install_serving_state(daemon.subscribe_serving()),
            "fresh child must install this fixture daemon's serving authority"
        );
        crate::inference::llama_server::mark_first_reconcile();
        let task: EvalTask = serde_json::from_value(serde_json::json!({
            "id":"owned-task", "prompt":"Write answer returning two", "lang":"rust", "test":"assert_eq!(answer(), 2);"
        })).unwrap();
        let request = |tasks| TeacherBatchRequest {
            tasks,
            teacher_model: "teacher-fixture".into(),
            temperature: 0.0,
            max_fix_iters: 0,
        };
        let first_owner = daemon.clone();
        let first_request = request(vec![task.clone()]);
        let mut first =
            tokio::spawn(async move { first_owner.run_teacher_batch(first_request).await });
        tokio::time::timeout(Duration::from_secs(5), async {
            tokio::select! {
                _ = entered.notified() => {},
                result = &mut first => match result {
                    Ok(Err(error)) => panic!("teacher returned before HTTP generation: {error}"),
                    Ok(Ok(corpus)) => panic!("teacher returned {} examples before HTTP generation", corpus.examples.len()),
                    Err(error) => panic!("teacher task ended before HTTP generation: {error}"),
                },
            }
        }).await.expect("teacher must reach fixture HTTP within its existing budget");
        assert!(daemon.reconciling.load(Ordering::Acquire));
        release.add_permits(1);
        let corpus = tokio::time::timeout(Duration::from_secs(20), first)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(corpus.examples.len(), 1);
        assert!(
            corpus.examples[0]["metadata"]["teacherGenerations"]
                .as_array()
                .unwrap()
                .len()
                == 1
        );
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut tick = tokio::time::interval(Duration::from_millis(10));
            while daemon.reconciling.load(Ordering::Acquire) {
                tick.tick().await;
                daemon.poll_teacher_batch();
            }
        })
        .await
        .unwrap();
        let next_owner = daemon.clone();
        let next_request = request(vec![task.clone(), task]);
        let mut next =
            tokio::spawn(async move { next_owner.run_teacher_batch(next_request).await });
        tokio::time::timeout(Duration::from_secs(5), async {
            tokio::select! {
                _ = entered.notified() => {},
                result = &mut next => match result {
                    Ok(Err(error)) => panic!("teacher returned before HTTP generation: {error}"),
                    Ok(Ok(corpus)) => panic!("teacher returned {} examples before HTTP generation", corpus.examples.len()),
                    Err(error) => panic!("teacher task ended before HTTP generation: {error}"),
                },
            }
        }).await.expect("teacher must reach fixture HTTP within its existing budget");
        next.abort();
        assert!(next.await.err().expect("cancelled command").is_cancelled());
        assert!(
            daemon.reconciling.load(Ordering::Acquire),
            "caller cancellation does not cancel shared decoding"
        );
        assert!(daemon.run_teacher_batch(request(vec![])).await.is_err());
        release.add_permits(1);
        tokio::time::timeout(Duration::from_secs(20), async {
            let mut tick = tokio::time::interval(Duration::from_millis(10));
            while daemon.reconciling.load(Ordering::Acquire) {
                tick.tick().await;
                daemon.poll_teacher_batch();
            }
        })
        .await
        .unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "cancelled batch schedules no second task"
        );
        assert_eq!(
            serves.load(Ordering::SeqCst),
            0,
            "same-model sharing does not retire or spawn"
        );
        assert!(daemon.serving_tx.borrow().ready);
        http.abort();
    }

    // Called by the existing real-child fixture, which supplies original validated
    // inputs, a real child, and the actual HTTP checkpoint acknowledgement.
    #[cfg(windows)]
    pub(crate) async fn exercise_cancelled_exclusive_teacher(
        server: Arc<dyn LlamaServerControl>,
        generation: crate::inference::slots::EngineGeneration,
        root: &str,
        checkpoint_http: JoinHandle<()>,
        saves: Arc<AtomicUsize>,
    ) {
        use crate::modules::serving_daemon::academy_batch::TeacherBatchRequest;
        let original = server.owned_serving_target().unwrap();
        let mut teacher = original.target.model.clone();
        teacher.id = "exclusive-teacher-fixture".into();
        teacher.provider = crate::inference::llama_server::PROVIDER_ID.into();
        let mut configured = daemon_with(server.clone());
        configured.set_model_resolver(Arc::new(move |_| Some(teacher.clone())));
        let daemon = Arc::new(configured);
        let mut prior = ServingSnapshot::empty();
        prior.active_model = Some(original.target.model.id.clone());
        prior.adapters = original.target.adapter_paths();
        prior.base_url = format!("{root}/v1");
        prior.ready = true;
        prior.served_context_window = original.observed_context_window;
        prior.lanes = original.observed_lanes;
        prior.host_prompt_cache_mib = original.launched_host_prompt_cache_mib;
        daemon.serving_tx.send_replace(prior.clone());
        daemon.acknowledge_verified_target(&prior, daemon.intent.snapshot().revision);
        assert!(daemon.verified_serving_target().is_some());
        let available = daemon
            .resource_daemon
            .board()
            .kinds
            .iter()
            .find(|kind| kind.kind == ResourceKind::Vram)
            .unwrap()
            .available_bytes;
        let pressure = daemon
            .resource_daemon
            .acquire_guarded(&crate::resources::LeaseRequest {
                consumer_id: "exclusive-fixture-pressure".into(),
                kind: ResourceKind::Vram,
                bytes: available,
                ttl_ms: u64::MAX,
                reclaim_policy: crate::resources::ReclaimPolicy::Pinned,
            })
            .unwrap();
        let owner = daemon.clone();
        let caller = tokio::spawn(async move {
            owner
                .run_teacher_batch(TeacherBatchRequest {
                    tasks: vec![],
                    teacher_model: "exclusive-teacher-fixture".into(),
                    temperature: 0.0,
                    max_fix_iters: 0,
                })
                .await
        });
        tokio::time::timeout(Duration::from_secs(15), async {
            let mut tick = tokio::time::interval(Duration::from_millis(10));
            while !generation.has_exited() {
                assert!(
                    !caller.is_finished(),
                    "checkpoint/retirement must be reached through the owner"
                );
                tick.tick().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            saves.load(Ordering::SeqCst),
            1,
            "save confirmed before observed original exit"
        );
        assert!(daemon.reconciling.load(Ordering::Acquire));
        caller.abort();
        assert!(caller
            .await
            .err()
            .expect("cancelled command")
            .is_cancelled());
        checkpoint_http.abort();
        let _ = checkpoint_http.await;
        // Only now make capacity available. Caller cancellation cannot surrender
        // the cleanup obligation or turn actual exit into a resource grant.
        drop(pressure);
        tokio::time::timeout(Duration::from_secs(20), async {
            let mut tick = tokio::time::interval(Duration::from_millis(10));
            while server.owned_engine().is_none() {
                tick.tick().await;
                daemon.poll_teacher_batch();
            }
        })
        .await
        .unwrap();
        assert!(server.owned_engine().unwrap() != original.identity);
        assert!(
            daemon
                .resource_daemon
                .board()
                .leases
                .iter()
                .any(|lease| lease.consumer_id == SERVING_CONSUMER_ID && lease.bytes > 0),
            "shared restore spawn must own an actual serving grant"
        );
        assert!(
            !daemon.serving_tx.borrow().ready,
            "non-model child cannot fabricate restore success"
        );
        daemon.intent.set_pin(None, true);
        tokio::time::timeout(Duration::from_secs(20), async {
            let mut tick = tokio::time::interval(Duration::from_millis(10));
            while daemon.reconciling.load(Ordering::Acquire) {
                tick.tick().await;
                daemon.poll_teacher_batch();
            }
        })
        .await
        .unwrap();
        assert!(
            server.owned_engine().is_none(),
            "failed restore child must exit before releasing operation"
        );
        assert!(!daemon
            .resource_daemon
            .board()
            .leases
            .iter()
            .any(|lease| lease.consumer_id == SERVING_CONSUMER_ID));
        assert!(
            !daemon.serving_tx.borrow().ready,
            "new intent is left to normal reconcile"
        );
    }

    fn daemon_with(server: Arc<dyn LlamaServerControl>) -> ServingDaemonModule {
        daemon_on_card(server, 53 * GB)
    }

    /// The same daemon on a card of a chosen size — the host floor is derived from
    /// PHYSICAL VRAM, so a test about "a genuinely small box" must give it one.
    fn daemon_on_card(server: Arc<dyn LlamaServerControl>, vram_bytes: u64) -> ServingDaemonModule {
        let gpu = Arc::new(GpuMemoryManager::simulated("test card", vram_bytes));
        let system = Arc::new(SystemResourceMonitor::new());
        let mut daemon = ServingDaemonModule::with_control(
            gpu,
            system,
            test_resource_daemon(),
            server,
            test_catalog(),
            test_pin_store(),
        );
        // Every harness daemon OWNS its demand pool: a private working set and an empty
        // leased-in ring. Both are process globals in production and shared by every test
        // in the binary — one test's samples became another test's requirement (CI,
        // 2026-09-20, three runs).
        daemon.working_set = crate::cognition::working_set::WorkingSetRegistry::new();
        daemon.set_leased_in_sent_source(Arc::new(Vec::new));
        // Resolve any planned id to a fake Model so reconcile can build a
        // ServingTarget without a populated global registry.
        daemon.set_model_resolver(Arc::new(|id: &str| Some(fake_model(id))));
        // NO real decode observed — the fresh-boot state every heartbeat test below assumes.
        // Left to the default this reads a process-global that ANY other test in this binary
        // can stamp (`llama_server`'s `note_real_decode` coverage does), which silently turns
        // the trust short-circuit on and reds these tests by scheduling order alone.
        daemon.set_decode_age_source(Arc::new(|| None));
        // NO inherited lane — the fresh-machine state every plan test below assumes. Left to the
        // default this reads the OPERATOR's real pidfile + lane registry, so a developer with a
        // 27B serving would silently hand every planning test an incumbent it never declared.
        // Same class of leak as the decode-age global above; caught by measurement in
        // `serving_consumer` before it could ever be caught by a red.
        daemon.set_inherited_lane(Arc::new(|| None));
        daemon
    }

    // what this catches: THE slice-6 no-reboot servability contract. A model that
    // is NotDownloaded is NOT offered as a serving candidate; the instant a pull
    // flips it Ready with a real on-disk path (`attach_local_artifact`), the SAME
    // live snapshot yields it as a candidate. Proves serving plans off the LIVE
    // catalog, not the immutable seed — a runtime acquisition becomes servable on
    // the next tick with no restart. Regresses the convergence gap where serving
    // re-derived candidates from `model_registry::global()` and never saw a pull.
    #[test]
    fn pulled_model_becomes_a_candidate_without_reboot() {
        use crate::model_registry::live::ModelStatus;
        use std::io::Write;

        let catalog = test_catalog();
        let id = "slice6-convergence-probe";
        // A fresh local model with no artifact on disk → NotDownloaded.
        catalog.register(
            fake_model(id),
            ModelStatus {
                availability: Availability::NotDownloaded,
                verified: None,
            },
        );

        // Before the pull: not on disk ⇒ never offered for serving.
        let before = candidates_from_snapshot(&catalog.snapshot());
        assert!(
            !before.iter().any(|f| f.model_id == id),
            "a NotDownloaded model must never be a serving candidate"
        );

        // The pull lands the artifact: a real, non-empty GGUF on disk.
        let mut gguf = tempfile::Builder::new()
            .suffix(".gguf")
            .tempfile()
            .expect("temp gguf");
        gguf.write_all(&[0u8; 4096]).expect("write weights");
        gguf.flush().expect("flush");
        assert!(
            catalog.attach_local_artifact(id, gguf.path().to_path_buf(), None),
            "model is present, artifact attaches + flips Ready"
        );

        // After the pull: the SAME live snapshot now yields it as a candidate,
        // with the footprint reflecting the real on-disk bytes — no reboot.
        let after = candidates_from_snapshot(&catalog.snapshot());
        let found = after.iter().find(|f| f.model_id == id).expect(
            "a freshly-pulled Ready model becomes a serving candidate on the next snapshot",
        );
        assert_eq!(
            found.weights_bytes, 4096,
            "footprint counts the real bytes that will be loaded"
        );
    }

    // what this catches: the VRAM-axis deallocation. `serving/unload` inserts a
    // model id into the daemon's suppress-set; `live_candidates` must then EXCLUDE
    // that model even though it is Ready on disk, so the planner can no longer pick
    // it and the next reconcile frees its lane. `serving/load` (un-suppress)
    // restores it as a candidate. Without this filter an unloaded model would be
    // re-planned on the very next tick and its VRAM would never free — the
    // imperative-suppress contract the operator chose.
    #[tokio::test]
    async fn live_candidates_honors_the_suppress_set() {
        use crate::model_registry::live::ModelStatus;
        use std::io::Write;

        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves, true)));
        let id = "suppress-probe";

        // Land a Ready model so it IS a candidate to begin with.
        let mut gguf = tempfile::Builder::new()
            .suffix(".gguf")
            .tempfile()
            .expect("temp gguf");
        gguf.write_all(&[0u8; 4096]).expect("write weights");
        gguf.flush().expect("flush");
        daemon.catalog.register(
            fake_model(id),
            ModelStatus {
                availability: Availability::NotDownloaded,
                verified: None,
            },
        );
        assert!(
            daemon
                .catalog
                .attach_local_artifact(id, gguf.path().to_path_buf(), None),
            "ready model lands on disk"
        );
        assert!(
            daemon.live_candidates().iter().any(|f| f.model_id == id),
            "a Ready model is a candidate before unload"
        );

        // serving/unload: pin it OFF → excluded from candidates → lane frees.
        daemon.intent.set_suppressed(id, true, true);
        assert!(
            !daemon.live_candidates().iter().any(|f| f.model_id == id),
            "a suppressed model is excluded → planner drops it → VRAM frees"
        );

        // serving/load: permit it again → returns as a candidate (planner decides).
        daemon.intent.set_suppressed(id, false, true);
        assert!(
            daemon.live_candidates().iter().any(|f| f.model_id == id),
            "an un-suppressed model returns as a candidate"
        );
    }

    // what this catches: #142 — the autonomic planner conscripting a benchmark
    // OPPONENT as the citizens' model. A Ready GGUF whose catalog row opts out
    // (`persona_serving_eligible: false`) must be invisible to the autonomic
    // plan (the tick picked Hermes-4.3 twice, 2026-07-12, purely because it was
    // the largest Ready artifact), while an explicit `serving/pin` — operator
    // consent — still serves it (how the benchmark matrix brings one up).
    #[tokio::test]
    async fn ineligible_rows_never_join_the_autonomic_plan_but_pin_bypasses() {
        use crate::model_registry::live::ModelStatus;
        use std::io::Write;

        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves, true)));
        let id = "opponent-probe";

        let mut opponent = fake_model(id);
        opponent.persona_serving_eligible = false;
        let mut gguf = tempfile::Builder::new()
            .suffix(".gguf")
            .tempfile()
            .expect("temp gguf");
        gguf.write_all(&[0u8; 4096]).expect("write weights");
        gguf.flush().expect("flush");
        daemon.catalog.register(
            opponent,
            ModelStatus {
                availability: Availability::NotDownloaded,
                verified: None,
            },
        );
        assert!(
            daemon
                .catalog
                .attach_local_artifact(id, gguf.path().to_path_buf(), None),
            "opponent GGUF lands on disk and flips Ready"
        );

        // Ready on disk, but ineligible → the autonomic path must never see it.
        assert!(
            !daemon.live_candidates().iter().any(|f| f.model_id == id),
            "an ineligible Ready model is excluded from the autonomic plan"
        );

        // Explicit pin = operator consent → eligibility is bypassed.
        daemon.intent.set_pin(Some(id.to_string()), true);
        assert!(
            daemon.live_candidates().iter().any(|f| f.model_id == id),
            "a pinned ineligible model serves — pin is consent"
        );

        // Unpin → back off the autonomic plan.
        daemon.intent.set_pin(None, true);
        assert!(
            !daemon.live_candidates().iter().any(|f| f.model_id == id),
            "unpin returns the opponent to benchmark-only invisibility"
        );
    }

    // what this catches: the EnsureOutcome → ServingSnapshot mapping. A live or
    // spawned model is ready with the served base url AND the real per-slot window
    // it carries through to personas; a degraded reconcile — OR a ready server
    // whose served window was unreadable (0) — publishes "nothing live", never a
    // half-true ready-with-no-model or ready-with-zero-window (the drift fix:
    // budgeting against a zero/guessed window is exactly what we refuse).
    #[test]
    fn snapshot_mapping_is_honest() {
        let genes = vec!["/genes/a.gguf".to_string()];
        let up = snapshot_from_outcome(
            &EnsureOutcome::Spawned { model: "m".into() },
            "coder-14b",
            &genes,
            11008,
            4,
            8_704,
            Some(crate::inference::vision_sidecar::SidecarLane {
                base_url: "http://127.0.0.1:58091/v1".to_string(),
                model_id: "vl-7b".to_string(),
            }),
        );
        assert_eq!(up.active_model.as_deref(), Some("coder-14b"));
        assert!(up.ready);
        assert!(up.base_url.ends_with("/v1"));
        assert_eq!(
            up.adapters, genes,
            "live snapshot carries the loaded genome set"
        );
        assert_eq!(
            up.served_context_window, 11008,
            "ready snapshot carries the real per-slot window personas budget to"
        );
        assert_eq!(
            up.lanes, 4,
            "ready snapshot carries the --parallel lane count for total-KV accounting"
        );
        assert_eq!(
            up.host_prompt_cache_mib, 8_704,
            "ready snapshot carries the grant the engine was launched with"
        );
        assert!(
            up.vision_ready,
            "the reconcile's verified multimodal verdict must survive into the snapshot \
             — the observe path gates on THIS field (#106)"
        );
        assert_eq!(
            up.vision_base_url.as_deref(),
            Some("http://127.0.0.1:58091/v1"),
            "the ONE vision value projects the routing address with the flag — an \
             address can never publish without verified readiness"
        );
        assert_eq!(up.vision_model.as_deref(), Some("vl-7b"));

        let already = snapshot_from_outcome(
            &EnsureOutcome::AlreadyServing,
            "coder-14b",
            &genes,
            11008,
            4,
            0,
            None,
        );
        assert_eq!(already.active_model.as_deref(), Some("coder-14b"));
        assert!(already.ready);
        assert_eq!(already.served_context_window, 11008);
        assert_eq!(already.lanes, 4);
        assert!(
            !already.vision_ready,
            "a text lane (no verified endpoint) must never read as sighted"
        );
        assert!(
            already.vision_base_url.is_none() && already.vision_model.is_none(),
            "no verified endpoint → no address, no model (None-iff-not-ready)"
        );

        // Ready outcome but the served window was unreadable (0) → publish the gap,
        // NOT a ready snapshot with a zero window a persona would budget against.
        let windowless = snapshot_from_outcome(
            &EnsureOutcome::Spawned { model: "m".into() },
            "coder-14b",
            &genes,
            0,
            4,
            8_704,
            None,
        );
        assert_eq!(
            windowless.active_model, None,
            "ready-but-no-window → nothing live"
        );
        assert!(!windowless.ready);
        assert_eq!(windowless.served_context_window, 0);
        assert_eq!(windowless.lanes, 0, "empty snapshot carries no lanes");
        assert_eq!(windowless.host_prompt_cache_mib, 0, "nothing live → no grant to charge");
        let degraded = snapshot_from_outcome(
            &EnsureOutcome::Degraded { reason: "x".into() },
            "coder-14b",
            &genes,
            11008,
            4,
            8_704,
            None,
        );
        assert_eq!(degraded.active_model, None, "degraded → nothing live");
        assert!(!degraded.ready);
        assert!(degraded.adapters.is_empty(), "degraded → no genome claimed");
        // regression for the 2026-07-24 Windows repro: the spawn-failure reason
        // must SURVIVE into the published snapshot — serving/status saying only
        // null/false while spawn fails every tick is the silent-failure lie.
        assert_eq!(
            degraded.degraded_reason.as_deref(),
            Some("x"),
            "degraded reason must reach the snapshot"
        );
        assert_eq!(
            windowless.degraded_reason, None,
            "a windowless-ready snapshot is not degraded — no reason claimed"
        );
    }

    // what this catches: the snapshot describes the ENGINE, not the plan. Regression for
    // the M5 at 2026-09-20 07:52Z — the plan fell to 1 lane, the 2-slot engine was adopted
    // (slots ≥ target never relaunch), and the snapshot published the target's count while
    // the footprint measurement, the ambient budget and the roster bound all read it as the
    // engine's. A 2-slot engine under a 1-lane target publishes 2; an engine that names no
    // slot count falls back to the target's; and the grant rides the snapshot from the
    // launch, so a target re-derived under a running engine cannot move the number the
    // measurement subtracts.
    #[test]
    fn the_snapshot_reports_the_engines_lanes_and_grant_not_the_plans() {
        assert_eq!(published_lanes(Some(2), 1), 2, "the engine's slots, not the plan's");
        assert_eq!(published_lanes(Some(0), 1), 1, "no slot count named → the target's");
        assert_eq!(published_lanes(None, 3), 3, "unreadable → the target's");
        let snap = snapshot_from_outcome(
            &EnsureOutcome::AlreadyServing,
            "qwen-27b",
            &[],
            67_072,
            published_lanes(Some(2), 1),
            14_396,
            None,
        );
        assert_eq!(snap.lanes, 2);
        assert_eq!(snap.host_prompt_cache_mib, 14_396);
        assert_eq!(
            attribute_serving(20 * GB, snap.lanes, snap.host_prompt_cache_mib),
            20 * GB + 14_396 * 1024 * 1024,
            "the board credits the grant the engine holds"
        );
    }

    // what this catches (#175 sticky window): the LoRA-load relaunch cascade must not
    // ratchet the per-slot window down and strand earlier-pinned personas. A same-lane
    // genome relaunch KEEPS the larger incumbent window; a lane-count change lets the
    // plan window through (resizing KV is legitimate; keeping the old window across a
    // lane increase would OOM); a larger plan window is never held down (the
    // starved-grow check owns growing); nothing-serving-yet → the plan window stands.
    #[test]
    fn sticky_window_holds_the_incumbent_only_on_a_same_lane_relaunch() {
        let live = |ready: bool, window: u32, lanes: u32| ServingSnapshot {
            ready,
            served_context_window: window,
            lanes,
            ..ServingSnapshot::empty()
        };
        assert_eq!(
            sticky_served_window(31_744, 2, &live(true, 49_664, 2)),
            49_664
        );
        assert_eq!(
            sticky_served_window(31_744, 2, &live(true, 49_664, 1)),
            31_744
        );
        assert_eq!(
            sticky_served_window(60_000, 2, &live(true, 49_664, 2)),
            60_000
        );
        assert_eq!(sticky_served_window(31_744, 2, &live(false, 0, 0)), 31_744);
    }

    // what this catches (#363 wedge-heal floor): a wedge declaration EMPTIES the live
    // snapshot, which disarms the sticky floor above — so the very relaunch the wedge
    // detector triggers used to spawn at a teardown-transient plan (measured live: plan
    // dipped to 9,984 while the dying 34,048 predecessor still held its memory; every
    // subsequent 12k prompt was rejected). The successor must inherit the last HEALTHY
    // window when lanes are unchanged; a live-ready lane defers to sticky; a lane-count
    // change resizes legitimately; and no-history (cold first boot) passes the plan.
    #[test]
    fn wedge_heal_floor_holds_the_last_healthy_window_across_the_empty_snapshot() {
        // The measured blackout shape: not ready (wedge emptied it), same lanes,
        // transient plan far below the last healthy window → floored.
        assert_eq!(wedge_heal_floor(9_984, 2, false, 34_048, 2), 34_048);
        // Live lane still ready → sticky owns the decision; floor stands down.
        assert_eq!(wedge_heal_floor(9_984, 2, true, 34_048, 2), 9_984);
        // Lane-count change legitimately resizes KV → plan through.
        assert_eq!(wedge_heal_floor(9_984, 4, false, 34_048, 2), 9_984);
        // Plan already >= last healthy → never held down.
        assert_eq!(wedge_heal_floor(41_216, 2, false, 34_048, 2), 41_216);
        // No healthy lane ever observed (cold boot) → plan through.
        assert_eq!(wedge_heal_floor(9_984, 2, false, 0, 0), 9_984);
    }

    /// The #438 boot geometry, in the shape taken from the live ledger (2026-08-19): a 27B whose
    /// weights are ~19 GB is still resident from the previous generation, so the successor's
    /// first sample of free VRAM reads ~6 GB. Only something small fits that; the 27B does not.
    fn boot_squeeze() -> (HostBudget, Vec<ModelFootprint>) {
        (
            HostBudget {
                usable_bytes: 6 * GB,
                perf_cores: 6,
            },
            vec![
                footprint_from_parts("qwen3-27b", 19 * GB, 8192, true, None).unwrap(),
                footprint_from_parts("coder-4b", 3 * GB, 8192, true, None).unwrap(),
            ],
        )
    }

    fn inherited_27b() -> LaneRecord {
        LaneRecord {
            pid: 11341,
            port: 58057,
            role: crate::inference::lane_registry::LaneRole::Live,
            model: "qwen3-27b".into(),
            context_window: 25_075,
            lanes: 4,
            page_dir: None,
            engine_bin: None,
            started_s: 0,
        }
    }

    // what this catches: THE #438 boot downshift, measured live 2026-08-19. A successor core
    // boots while its predecessor's 27B still holds ~45 GB, so the first plan honestly samples
    // ~6 GB free and — with no incumbent — plainly selects the biggest thing that fits: a 14B at
    // the floor. The predecessor was then reclaimed 1.2s later, freeing memory the plan had
    // already spent. Counting the inherited lane as the incumbent re-arms the at-rest credit
    // that exists for exactly this ("a model's OWN residency can never flap it out"), so the
    // 27B holds. Regression here = every crash-and-restart silently demotes the served model.
    #[tokio::test]
    async fn a_lane_inherited_from_a_previous_generation_is_the_incumbent_the_first_plan_defends() {
        let mut daemon = daemon_with(Arc::new(FakeServer::healthy(
            Arc::new(AtomicUsize::new(0)),
            true,
        )));
        daemon.set_inherited_lane(Arc::new(|| Some(inherited_27b())));

        let (budget, candidates) = boot_squeeze();
        daemon.publish_plan(budget, &candidates, &candidates);

        let plan = daemon.plan_tx.borrow().plan.clone().expect("a plan");
        assert_eq!(
            plan.base_model.model_id, "qwen3-27b",
            "the inherited 27B is ours and its bytes are ours to reclaim — the successor must \
             not flee its own predecessor onto a 14B"
        );
    }

    // what this catches: the NEGATIVE control for the test above — proof the fixture really is
    // a squeeze and the assertion is not vacuous. Same budget, same candidates, no inherited
    // lane: the planner correctly downshifts, because on a genuinely 6 GB machine a 27B does
    // not fit. If this ever also returns the 27B, the test above proves nothing.
    #[tokio::test]
    async fn without_a_past_form_of_ourself_a_six_gigabyte_box_really_does_downshift() {
        // A genuinely 8 GB card: 6 GB live is not a squeeze, it is the box. The host
        // floor (physical × 0.80 = 6.4 GB) is honestly the 4B, so the downshift stands.
        let daemon = daemon_on_card(
            Arc::new(FakeServer::healthy(Arc::new(AtomicUsize::new(0)), true)),
            8 * GB,
        );
        let (budget, candidates) = boot_squeeze();
        daemon.publish_plan(budget, &candidates, &candidates);
        let plan = daemon.plan_tx.borrow().plan.clone().expect("a plan");
        assert_eq!(
            plan.base_model.model_id, "coder-4b",
            "with no incumbent to credit, 6 GB genuinely cannot hold a 19 GB model"
        );
    }

    // what this catches (2026-09-16): the same 6 GB reading on a 53 GB card is a
    // TRANSIENT, and with no incumbent to defend (a fresh boot, an unload that
    // suppressed the 27B) every other guard is disarmed. The host floor is not: the
    // card's physical capacity serves the 27B, so a plan naming the 4B is refused and
    // NOTHING is published — the host waits for its budget rather than sinking. This is
    // the crash-restart demotion of #438 and the unload demotion of #4145, closed at
    // the layer both share.
    #[tokio::test]
    async fn a_transient_six_gigabytes_on_a_big_card_refuses_to_sink_below_the_floor() {
        let daemon = daemon_with(Arc::new(FakeServer::healthy(
            Arc::new(AtomicUsize::new(0)),
            true,
        )));
        let (budget, candidates) = boot_squeeze();
        daemon.publish_plan(budget, &candidates, &candidates);
        assert!(
            daemon.plan_tx.borrow().plan.is_none(),
            "a 53 GB card whose floor is the 27B must not publish a 4B plan on a 6 GB reading"
        );
        // And when the budget comes back, the 27B is adopted at once.
        let recovered = HostBudget {
            usable_bytes: 40 * GB,
            perf_cores: budget.perf_cores,
        };
        daemon.publish_plan(recovered, &candidates, &candidates);
        let plan = daemon
            .plan_tx
            .borrow()
            .plan
            .clone()
            .expect("a plan once the budget returns");
        assert_eq!(plan.base_model.model_id, "qwen3-27b");
    }

    // what this catches: precedence. Once THIS core has published a decision, its own plan is
    // the incumbent and a stale registry record must never override it — otherwise a leftover
    // .lane file would pin the model forever against every later replan.
    #[test]
    fn a_published_plan_outranks_an_inherited_lane() {
        assert_eq!(
            incumbent_for_plan(Some("coder-14b".into()), Some(&inherited_27b())).as_deref(),
            Some("coder-14b"),
        );
        // …and with nothing published, the inherited lane speaks.
        assert_eq!(
            incumbent_for_plan(None, Some(&inherited_27b())).as_deref(),
            Some("qwen3-27b"),
        );
        // …and a true cold boot still has no incumbent at all.
        assert_eq!(incumbent_for_plan(None, None), None);
    }

    // what this catches: Kimi's attempt 2 (5090, 2026-09-28 12:29Z). The plan wanted a different
    // model while a training run was bound to the live engine, the reconcile replaced the
    // engine, and the run died with it. With the run's binding recorded against THIS engine
    // incarnation (here, this test process: same pid, same start time), the same plan that
    // would otherwise serve launches nothing; once the binding is released it proceeds.
    #[tokio::test]
    async fn an_engine_bound_to_live_work_is_never_replaced_under_it() {
        use crate::inference::engine_residency::{record, release, store_path, EngineIncarnation, ResidentWork};
        let serves = Arc::new(AtomicUsize::new(0));
        let mut daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));
        let pid = std::process::id();
        let engine = EngineIncarnation::of(pid, 61347).expect("test: this process has a start time");
        let lane = LaneRecord { pid, port: 61347, started_s: engine.started_s, ..inherited_27b() };
        daemon.set_inherited_lane(Arc::new(move || Some(lane.clone())));
        let budget = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
        daemon.publish_plan(budget, &candidates, &candidates);

        let store = store_path(&crate::paths::continuum_home().expect("test: a test home"));
        let job = uuid::Uuid::from_u128(0xef9d_f13b);
        let bound = ResidentWork {
            job,
            out: "run.gguf".into(),
            engine,
            base_model: "qwen3-27b".into(),
            created_ms: 1,
            consumer: "genome-train:test".into(),
            reserved_bytes: 1 << 30,
            interrupted: None,
            job_spec: None,
        };
        record(&store, bound.clone()).expect("test: record");
        assert!(daemon.reconcile_to_plan().is_none(), "an engine bound to live work is not reconciled");
        assert_eq!(serves.load(Ordering::SeqCst), 0, "nothing was launched over the run");

        release(&store, &bound).expect("test: release");
        let handle = daemon.reconcile_to_plan().expect("released: the plan proceeds");
        handle.await.unwrap();
        assert_eq!(serves.load(Ordering::SeqCst), 1);
    }

    // what this catches: a published plan drives a reconcile that brings the
    // server up and publishes a ready ServingSnapshot for that model — the
    // plan→reality wiring. Regression here = the daemon decides but never acts.
    #[tokio::test]
    async fn reconcile_brings_planned_model_up() {
        let serves = Arc::new(AtomicUsize::new(0));
        let mut daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));

        // Publish a plan (most-capable fitting model = coder-14b).
        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
        daemon.publish_plan(budget, &candidates, &candidates);

        // What this catches: an intent change during target preparation must not
        // unprotect the incumbent or attribute an unlaunched resident on refusal.
        let paths = tempfile::tempdir().expect("isolated artifact paths");
        let incumbent = paths.path().join("incumbent.gguf");
        let candidate = paths.path().join("candidate.gguf");
        daemon.set_active_artifact(Some(incumbent.clone()));
        *daemon.served_resident.lock().unwrap() = Some(incumbent.clone());
        let resolver = daemon.model_resolver.clone();
        let preparing = resolver.clone();
        let intent = daemon.intent.clone();
        daemon.set_model_resolver(Arc::new(move |id| {
            intent.set_pin(Some(id.to_string()), true);
            preparing(id).map(|mut model| {
                model.gguf_local_path = Some(candidate.clone());
                model
            })
        }));
        assert!(daemon.reconcile_to_plan().is_none());
        assert_eq!(
            daemon.active_artifact.lock().unwrap().as_ref(),
            Some(&incumbent)
        );
        assert_eq!(
            daemon.served_resident.lock().unwrap().as_ref(),
            Some(&incumbent)
        );
        assert_eq!(serves.load(Ordering::SeqCst), 0);
        daemon.set_active_artifact(None);
        daemon.set_model_resolver(resolver);
        daemon.publish_plan(budget, &candidates, &candidates);

        let handle = daemon
            .reconcile_to_plan()
            .expect("a reconcile should be spawned");
        handle.await.unwrap();

        let snap = daemon.subscribe_serving().borrow().clone();
        assert_eq!(snap.active_model.as_deref(), Some("coder-14b"));
        assert!(snap.ready);
        assert_eq!(serves.load(Ordering::SeqCst), 1, "served exactly once");
        assert!(
            daemon.verified_serving_target().is_none(),
            "an unowned fixture cannot invent launch provenance"
        );

        // A no-op probe never crosses lifecycle admission. New intent during
        // that probe must prevent downstream publication/sidecar actions too.
        let intent = ServingIntent::new(None);
        let changed = intent.clone();
        let mut server = FakeServer::healthy(serves.clone(), true);
        server.active = Some("coder-14b".into());
        server.probe_effect = Some(Arc::new(move || {
            changed.set_pin(Some("coder-14b".into()), true);
        }));
        let mut stale = daemon_with(Arc::new(server));
        stale.intent = intent;
        stale.publish_plan(budget, &candidates, &candidates);
        stale.plan_tx.send_modify(|snapshot| {
            let plan = snapshot.plan.as_mut().unwrap();
            plan.served_context_window = 11008;
            plan.lanes = 1;
        });
        let queued = stale.reconcile_to_plan().expect("queued serve");
        queued.abort();
        assert!(queued.await.unwrap_err().is_cancelled());
        assert!(!stale.reconciling.load(Ordering::Acquire));
        stale
            .reconcile_to_plan()
            .expect("probe reconcile")
            .await
            .unwrap();
        assert!(
            stale.serving_tx.borrow().active_model.is_none(),
            "superseded no-op cannot publish"
        );
        assert_eq!(
            serves.load(Ordering::SeqCst),
            1,
            "no new lifecycle admission"
        );
        assert!(stale.active_artifact.lock().unwrap().is_none());

        // Stop inside the real operation future after admission. The old ready
        // incumbent must already be unavailable, and cancellation releases ownership.
        let entered = Arc::new(tokio::sync::Notify::new());
        let mut server = FakeServer::healthy(serves.clone(), true);
        server.serve_started = Some(entered.clone());
        let cancelled = daemon_with(Arc::new(server));
        cancelled.publish_plan(budget, &candidates, &candidates);
        let mut incumbent = ready_snapshot();
        incumbent.active_model = Some("previous-model".into());
        let _ = cancelled.serving_tx.send_replace(incumbent);
        *cancelled.pending_model_change.lock().unwrap() = Some("coder-14b".into());
        cancelled
            .model_change_streak
            .store(REHOME_SUSTAINED_TICKS, Ordering::Relaxed);
        cancelled.force_relaunch.store(true, Ordering::Release);
        let admitted = cancelled.reconcile_to_plan().expect("admitted serve");
        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("operation reaches controlled await");
        assert!(!cancelled.serving_tx.borrow().ready);
        assert_eq!(
            cancelled.serving_tx.borrow().active_model.as_deref(),
            Some("previous-model")
        );
        assert_eq!(
            cancelled.serving_tx.borrow().loading_model.as_deref(),
            Some("coder-14b")
        );
        admitted.abort();
        assert!(admitted.await.unwrap_err().is_cancelled());
        assert!(!cancelled.reconciling.load(Ordering::Acquire));
        assert!(
            cancelled.force_relaunch.load(Ordering::Acquire),
            "cancelled forced probe is still owed"
        );
        let retry = cancelled
            .reconcile_to_plan()
            .expect("cancellation cannot strand recovery");
        retry.abort();
        assert!(retry.await.unwrap_err().is_cancelled());

        // The same forced-probe claim survives cancellation during preflight,
        // before any lifecycle admission or readiness mutation is permitted.
        let entered = Arc::new(tokio::sync::Notify::new());
        let mut server = FakeServer::healthy(serves, true);
        server.probe_started = Some(entered.clone());
        let probing = daemon_with(Arc::new(server));
        probing.publish_plan(budget, &candidates, &candidates);
        probing.force_relaunch.store(true, Ordering::Release);
        let task = probing.reconcile_to_plan().expect("preflight probe");
        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("operation reaches controlled await");
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(probing.force_relaunch.load(Ordering::Acquire));
        assert!(!probing.reconciling.load(Ordering::Acquire));
        assert!(
            probing.serving_tx.borrow().loading_model.is_none(),
            "no admission before probe completes"
        );
    }

    // what this catches: once the desired model is ready, a subsequent reconcile
    // is a NO-OP (no relaunch) — otherwise every tick would thrash the
    // GPU-warm server.
    #[tokio::test]
    async fn reconcile_is_noop_when_already_serving() {
        let serves = Arc::new(AtomicUsize::new(0));
        let server = FakeServer::healthy(serves.clone(), true);
        let verified_reads = server.verified_reads.clone();
        let daemon = daemon_with(Arc::new(server));

        // Pretend coder-14b is already up and ready.
        let _ = daemon.serving_tx.send_replace(ServingSnapshot {
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 4,
            host_prompt_cache_mib: 0,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
            loading_model: None,
        });
        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
        daemon.publish_plan(budget, &candidates, &candidates);

        assert!(
            daemon.reconcile_to_plan().is_none(),
            "already serving → no reconcile"
        );
        assert_eq!(serves.load(Ordering::SeqCst), 0, "no relaunch");
        let reads = verified_reads.load(Ordering::SeqCst);
        assert!(
            reads > 0,
            "same-model retention considers original provenance"
        );
        daemon.intent.set_pin(Some("other-model".into()), true);
        let revision = daemon.intent.snapshot().revision;
        daemon.plan_tx.send_modify(|snapshot| {
            snapshot.intent_revision = revision;
            snapshot.plan.as_mut().unwrap().base_model.model_id = "other-model".into();
        });
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "first model change is deferred"
        );
        assert_eq!(
            verified_reads.load(Ordering::SeqCst),
            reads,
            "a deferred different model cannot restamp the incumbent capture"
        );
        assert!(daemon.verified_serving_target().is_none());
    }

    // what this catches: paging quarantine must bypass the unchanged healthy
    // snapshot shortcut and reach the existing reconcile/serve owner.
    #[tokio::test]
    async fn paging_quarantine_reconciles_an_unchanged_healthy_plan() {
        let serves = Arc::new(AtomicUsize::new(0));
        let root = format!("test://reconcile-paging-{}", uuid::Uuid::new_v4());
        let endpoint = crate::inference::slots::directory().endpoint(&root);
        let mut server = FakeServer::healthy(serves.clone(), false);
        assert!(
            server.smoke_ok.load(Ordering::Relaxed),
            "control-plane health cannot heal paging uncertainty"
        );
        server.paging_endpoint = Some(endpoint.clone());
        let daemon = daemon_with(Arc::new(server));
        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
        daemon.publish_plan(budget, &candidates, &candidates);
        let plan = daemon
            .plan_tx
            .borrow()
            .plan
            .clone()
            .expect("published plan");
        let mut live = ready_snapshot();
        live.active_model = Some("coder-14b".into());
        live.served_context_window = plan.served_context_window;
        live.lanes = plan.lanes;
        live.base_url = format!("{root}/v1");
        let _ = daemon.serving_tx.send_replace(live);
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "healthy unchanged plan stays resident"
        );
        let admission = endpoint.admit().await.expect("initial endpoint admission");
        admission.quarantine_paging();
        drop(admission);
        let replacement = daemon
            .reconcile_to_plan()
            .expect("quarantine enters normal replacement");
        assert!(
            !daemon.serving_tx.borrow().ready,
            "publish refusal before replacement awaits"
        );
        replacement.await.expect("normal reconcile task");
        assert_eq!(serves.load(Ordering::SeqCst), 1);
        assert!(
            !daemon.serving_tx.borrow().ready,
            "failed replacement cannot publish ready"
        );
        assert!(
            endpoint.paging_recovery_required(),
            "only verified generation replacement clears quarantine"
        );
    }

    /// A lane whose live per-slot window sits at `live_pct` of what the plan
    /// actually affords, ready to have `reconcile_to_plan` driven at it repeatedly.
    /// Returns the daemon and the REAL planned window.
    ///
    /// Two things this fixture exists to get right, both of which a hand-written
    /// pair of numbers gets wrong:
    ///
    /// 1. A plan is capped by MEASURED demand ([`ServingDemand`]). With nothing
    ///    recorded it falls back to [`BOOTSTRAP_WORKING_SET`] — 16,384, which is
    ///    exactly the number the live lane was stranded at, so a fixture that skips
    ///    this reproduces a plan == live and no gain to sustain at all.
    /// 2. The planner is free to serve less than the model's max context (host fit,
    ///    lane split). Asserting against a window we merely ASKED for tests the
    ///    fixture; deriving the live window from the plan we GOT tests the guard.
    fn lane_under_plan(
        serves: Arc<AtomicUsize>,
        plan_model_ctx: u32,
        live_pct: u32,
    ) -> (ServingDaemonModule, u32) {
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves, true)));
        // A measured peak is what lets a plan exceed the cold-start prior.
        daemon
            .working_set()
            .record_in_memory(uuid::Uuid::new_v4(), plan_model_ctx, 0);
        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates =
            vec![footprint_from_parts("coder-14b", 9 * GB, plan_model_ctx, true, None).unwrap()];
        daemon.publish_plan(budget, &candidates, &candidates);
        let (plan_window, plan_lanes) = {
            let plan = daemon.plan_tx.borrow();
            let plan = plan.plan.as_ref().expect("plan published");
            (plan.served_context_window, plan.lanes)
        };
        let _ = daemon.serving_tx.send_replace(ServingSnapshot {
            ready_verified_at_ms: None,
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: plan_window * live_pct / 100,
            // Live lanes match the plan's so these fixtures isolate the WINDOW
            // dimension of re-home evidence; the lanes dimension has its own test
            // (rehome_evidence_counts_lanes_not_just_per_slot_window).
            lanes: plan_lanes,
            host_prompt_cache_mib: 0,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
            loading_model: None,
        });
        (daemon, plan_window)
    }

    // what this catches: the failure the OLD single-sample `live * 2 <= plan` rule
    // could not see. Measured live 2026-08-06: plan 26,323 vs lane 16,384 — a 62%
    // lane, comfortably above half — held indefinitely while every persona's binding
    // read the stale window and the demand loop's output was silently discarded.
    // A real, PERSISTENT gain must eventually re-home no matter where it sits in the
    // 50-99% band. (BigMama's call; she owns the guard the ratio came from.)
    #[tokio::test]
    async fn a_sustained_gain_above_the_old_half_bar_eventually_re_homes() {
        let serves = Arc::new(AtomicUsize::new(0));
        // 62% of plan — the live incident's exact ratio, comfortably above the old
        // half bar and therefore invisible to the rule this replaces.
        let (daemon, plan_window) = lane_under_plan(serves.clone(), 65_536, 62);
        // Guard the FIXTURE, not just the guard: if a measured demand had not raised
        // the plan above the cold-start prior, plan == live, there would be no gain to
        // sustain, and every assertion below would pass for the wrong reason. That is
        // exactly how this test failed its first time out.
        assert!(
            plan_window > daemon.serving_tx.borrow().served_context_window,
            "fixture is vacuous: the plan must actually exceed the lane"
        );

        // Below the streak it must hold — a relaunch kills in-flight turns, so an
        // instant re-home on first sight is exactly the thrash we are avoiding.
        for tick in 1..REHOME_SUSTAINED_TICKS {
            assert!(
                daemon.reconcile_to_plan().is_none(),
                "tick {tick}: must not re-home before the gain has proven itself sustained"
            );
        }
        assert_eq!(
            serves.load(Ordering::SeqCst),
            0,
            "no relaunch during the streak"
        );

        assert!(
            daemon.reconcile_to_plan().is_some(),
            "a gain that persisted for a full lane-readiness window MUST re-home — \
             this is the 62%-forever case the 2x ratio never caught"
        );
    }

    // what this catches: the property the 2x ratio was RIGHT about, which the
    // replacement must not lose. A plan that jitters above the bar and falls back
    // must never accumulate its way to a relaunch — the outage this guard exists for
    // was a lane relaunching against a plan wandering 3.6k<->22k.
    #[tokio::test]
    async fn a_jittering_gain_never_accumulates_into_a_re_home() {
        let serves = Arc::new(AtomicUsize::new(0));
        let (daemon, plan_window) = lane_under_plan(serves.clone(), 65_536, 62);
        let live_window = daemon.serving_tx.borrow().served_context_window;
        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };

        // Alternate: plenty of headroom, then none. Far MORE ticks than the streak
        // needs in total — but never consecutively.
        for _ in 0..(REHOME_SUSTAINED_TICKS * 3) {
            assert!(
                daemon.reconcile_to_plan().is_none(),
                "wanting: still below streak"
            );
            // A dip: the plan momentarily affords no more than the lane already has.
            let small = vec![footprint_from_parts("coder-14b", 9 * GB, live_window, true, None).unwrap()];
            daemon.publish_plan(budget, &small, &small);
            assert!(daemon.reconcile_to_plan().is_none(), "dip: nothing to gain");
            // …and back up.
            let big = vec![footprint_from_parts("coder-14b", 9 * GB, plan_window, true, None).unwrap()];
            daemon.publish_plan(budget, &big, &big);
        }
        assert_eq!(
            serves.load(Ordering::SeqCst),
            0,
            "jitter must NEVER re-home, however long it goes on — one dip resets the streak"
        );
    }

    // what this catches (2026-10-07, the 5090): a lane serving 2,304 under a plan that
    // alternated 33k <-> 70k never re-homed for an hour. Every rise reset the streak,
    // although BOTH plan values wanted a far bigger lane, so Kimi, needing 52k, could
    // not fit while the lane read ready. A plan moving below a high it already reached
    // is settled enough: the streak counts and the lane re-homes within a few ticks.
    #[tokio::test]
    async fn a_lane_far_below_an_alternating_plan_re_homes() {
        let serves = Arc::new(AtomicUsize::new(0));
        let (daemon, full) = lane_under_plan(serves.clone(), 65_536, 4);
        let live_window = daemon.serving_tx.borrow().served_context_window;
        assert!(live_window * 4 < full / 2, "fixture: the lane must sit far below both plan values");
        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let half = vec![footprint_from_parts("coder-14b", 9 * GB, full / 2, true, None).unwrap()];
        let whole = vec![footprint_from_parts("coder-14b", 9 * GB, full, true, None).unwrap()];
        let mut fired_at = None;
        for tick in 0..(REHOME_SUSTAINED_TICKS * 4) {
            let plan = if tick % 2 == 0 { &half } else { &whole };
            daemon.publish_plan(budget, plan, plan);
            if daemon.reconcile_to_plan().is_some() {
                fired_at = Some(tick);
                break;
            }
        }
        let fired_at = fired_at.expect("a lane far below both values of an alternating plan must re-home");
        assert!(
            fired_at <= REHOME_SUSTAINED_TICKS + 3,
            "re-homed only at tick {fired_at}: the alternation still resets the streak"
        );
        // and it lands at the HIGH value (the relaunch takes the fire tick's plan): a re-home
        // on a low tick would bring her back under her need and owe a second relaunch
        assert_eq!(fired_at % 2, 1, "fired on a low tick (half the plan), not the high one");
    }

    // what this catches: the boot staircase (2026-09-02) the streak's plan-high rule
    // exists for. Personas register serially at boot, so the plan climbs in stairs;
    // a re-home per stair was sixteen model loads in one lane's log. While every tick
    // is a new high, nothing fires; once the climb stops, exactly one re-home at the top.
    #[tokio::test]
    async fn a_climbing_plan_coalesces_into_one_re_home_at_its_top() {
        let serves = Arc::new(AtomicUsize::new(0));
        let (daemon, top) = lane_under_plan(serves.clone(), 65_536, 4);
        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let at = |w: u32| vec![footprint_from_parts("coder-14b", 9 * GB, w, true, None).unwrap()];
        for (stair, w) in [top / 4, top * 3 / 8, top / 2, top * 5 / 8, top * 3 / 4, top * 7 / 8, top]
            .into_iter()
            .enumerate()
        {
            let plan = at(w);
            daemon.publish_plan(budget, &plan, &plan);
            assert!(daemon.reconcile_to_plan().is_none(), "stair {stair}: a climbing plan must not re-home");
        }
        let plan = at(top);
        for tick in 1..REHOME_SUSTAINED_TICKS {
            daemon.publish_plan(budget, &plan, &plan);
            assert!(daemon.reconcile_to_plan().is_none(), "settled tick {tick}: still proving the plan stopped");
        }
        daemon.publish_plan(budget, &plan, &plan);
        assert!(
            daemon.reconcile_to_plan().is_some(),
            "the climb stopped: one re-home, at its top (no stair above re-homed)"
        );
    }

    // what this catches: the cooldown collapsing into the sustained-delta test. The
    // two answer different questions — "is the gain real?" vs "may we pay for it
    // yet?" — and a relaunch kills every in-flight turn on the lane, so the rate
    // limit has to hold even when the evidence is perfect. If the cooldown were
    // merely "reset the streak", 3 more qualifying ticks would re-fire in 15s
    // instead of 90. (BigMama's requirement 1 of 2, 2026-08-06.)
    #[tokio::test]
    async fn a_second_re_home_waits_out_the_cooldown_however_good_the_evidence_is() {
        let serves = Arc::new(AtomicUsize::new(0));
        let (daemon, _plan) = lane_under_plan(serves.clone(), 65_536, 62);
        let starved = daemon.serving_tx.borrow().clone();

        let mut decision = None;
        for _ in 0..REHOME_SUSTAINED_TICKS {
            decision = daemon.reconcile_to_plan();
        }
        decision
            .expect("evidence qualifies by the third tick")
            .await
            .unwrap();
        assert_eq!(
            serves.load(Ordering::SeqCst),
            1,
            "the first re-home fires on evidence"
        );

        // Re-pin the lane to its starved window: the relaunch is faked, so nothing
        // actually resized. That makes the gain as compelling on every following tick
        // as it was on the first — only the cooldown stands between us and a storm.
        let _ = daemon.serving_tx.send_replace(starved);
        for tick in 0..REHOME_COOLDOWN_TICKS {
            assert!(
                daemon.reconcile_to_plan().is_none(),
                "cooldown tick {tick}: perfect evidence must still not buy a second relaunch"
            );
        }
        assert_eq!(
            serves.load(Ordering::SeqCst),
            1,
            "exactly one relaunch per cooldown window"
        );

        // Cooldown spent — the gain must now re-prove itself from zero before firing.
        for tick in 0..(REHOME_SUSTAINED_TICKS - 1) {
            assert!(
                daemon.reconcile_to_plan().is_none(),
                "post-cooldown tick {tick}: the streak was lost while cooling, re-prove it"
            );
        }
        assert!(
            daemon.reconcile_to_plan().is_some(),
            "once cooled AND re-proven, the still-stranded lane re-homes again"
        );
    }

    // what this catches: a margin expressed as an absolute token count. The same
    // 2,048-token shortfall is a third of a small lane and rounding error on a large
    // one — an absolute bar means something different at every window size, so the
    // relative margin is the property, not an implementation detail of it.
    #[tokio::test]
    async fn the_margin_is_relative_so_the_same_shortfall_decides_differently_by_lane_size() {
        // A lane at 93% of plan: the shortfall is ~7.5% of what it serves — real
        // tokens, but under the bar however long it persists.
        let quiet = Arc::new(AtomicUsize::new(0));
        let (near_plan, _) = lane_under_plan(quiet.clone(), 65_536, 93);
        for _ in 0..(REHOME_SUSTAINED_TICKS * 3) {
            near_plan.reconcile_to_plan();
        }
        assert_eq!(
            quiet.load(Ordering::SeqCst),
            0,
            "a 7.5% shortfall is not worth killing in-flight turns for, however long it holds"
        );

        // A lane at 67% of the SAME plan: the shortfall is ~49% of what it serves —
        // half a mind again, a step change.
        let loud = Arc::new(AtomicUsize::new(0));
        let (far_below, _) = lane_under_plan(loud.clone(), 65_536, 67);
        let mut decision = None;
        for _ in 0..REHOME_SUSTAINED_TICKS {
            decision = far_below.reconcile_to_plan();
        }
        decision.expect("a 49% shortfall qualifies").await.unwrap();
        assert_eq!(
            loud.load(Ordering::SeqCst),
            1,
            "half the lane's own window again IS a step change — re-home"
        );
    }

    /// Believe-ready snapshot fixture for the liveness-heartbeat tests.
    fn ready_snapshot() -> ServingSnapshot {
        ServingSnapshot {
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 4,
            host_prompt_cache_mib: 0,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
            loading_model: None,
        }
    }

    // what this catches: THE WIRING. The 2026-08-05 outage was not a missing detector — the
    // reap actuator already existed and was tested. What was missing was anything that CALLED
    // it, and a detector that reports into a void is indistinguishable from no detector at
    // all. This drives the whole chain the live system uses: the stderr watcher raises the
    // flag → the daemon tick takes it → the lane is published NOT-ready.
    //
    // The not-ready assertion is the load-bearing half. Arming `force_relaunch` alone is
    // silently useless: `reconcile_to_plan` returns early on `ready && same model && same
    // adapters` BEFORE it ever reads the flag, so a wedged-but-"ready" lane would sit there
    // exactly as it did for four hours. regression for the 172 GB log outage
    #[tokio::test]
    async fn a_reported_wedge_flips_the_lane_not_ready() {
        let fake = Arc::new(FakeServer::healthy(Arc::new(AtomicUsize::new(0)), true));
        let daemon = daemon_with(fake.clone());
        let _ = daemon.serving_tx.send_replace(ready_snapshot());

        // Nothing reported yet: a healthy lane is left alone.
        daemon.take_reported_wedge();
        assert!(
            daemon.serving_tx.borrow().ready,
            "an unreported lane must never be reaped"
        );

        // The stderr watcher sees `progress = 1.10` four times and raises.
        fake.wedge.raise();
        daemon.take_reported_wedge();

        assert!(
            !daemon.serving_tx.borrow().ready,
            "a reported wedge must publish NOT-ready, or the reconcile's no-op guard \
             returns before it ever consumes the force-relaunch flag"
        );
        assert!(
            daemon.force_relaunch.load(Ordering::Acquire),
            "the reconcile must be told to re-prove decode on a child we own"
        );

        // The report is consumed exactly once — a second tick must not re-escalate a lane
        // that is already being relaunched.
        let _ = daemon.serving_tx.send_replace(ready_snapshot());
        daemon.take_reported_wedge();
        assert!(
            daemon.serving_tx.borrow().ready,
            "take clears the flag; one report is one escalation"
        );
    }

    // what this catches: #175 self-heal (detection + recovery). A lane the daemon believes
    // is `ready` but whose COMPUTE path is wedged — the decode heartbeat fails while the
    // control plane still 200s, the exact Metal-OOM-poison shape — is flipped NOT-ready
    // after HEALTH_FAILS_TO_RELAUNCH consecutive heartbeats (which the reconcile then turns
    // into a kill+respawn). Before this the owned lane was trusted forever and the persona
    // substrate stayed bricked until a human reboot. regression for #175
    #[tokio::test]
    async fn health_heartbeat_flips_ready_after_sustained_decode_failure() {
        let serves = Arc::new(AtomicUsize::new(0));
        let smoke = Arc::new(AtomicBool::new(false)); // wedged compute path
        let daemon = daemon_with(Arc::new(FakeServer {
            smoke_ok: smoke.clone(),
            ..FakeServer::healthy(serves, true)
        }));
        let _ = daemon.serving_tx.send_replace(ready_snapshot());

        // First heartbeat (tick 0) fails once — hysteresis holds, still ready.
        if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
            h.await.unwrap();
        }
        assert!(
            daemon.serving_tx.borrow().ready,
            "one failed probe is not enough — a merely-busy lane must not be reaped"
        );

        // Force the NEXT call to be a heartbeat tick; the second failure reaches threshold.
        daemon.health_ticks.store(0, Ordering::Relaxed);
        if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
            h.await.unwrap();
        }
        let s = daemon.serving_tx.borrow();
        assert!(
            !s.ready,
            "sustained decode failure flips the lane not-ready so the reconcile respawns it (#175)"
        );
        assert!(
            s.active_model.is_none(),
            "not-ready is published as the empty gap the reconcile relaunches from"
        );
    }

    // what this catches: hysteresis RESET. A lane that fails a heartbeat once then RECOVERS
    // (probe passes) must have its failure streak cleared — so an isolated failure minutes
    // later never accumulates with a stale one into a spurious reap.
    #[tokio::test]
    async fn health_heartbeat_streak_resets_on_a_passing_probe() {
        let serves = Arc::new(AtomicUsize::new(0));
        let smoke = Arc::new(AtomicBool::new(false));
        let daemon = daemon_with(Arc::new(FakeServer {
            smoke_ok: smoke.clone(),
            ..FakeServer::healthy(serves, true)
        }));
        let _ = daemon.serving_tx.send_replace(ready_snapshot());

        // Fail once.
        if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
            h.await.unwrap();
        }
        assert!(daemon.serving_tx.borrow().ready);

        // Lane recovers; next heartbeat passes → streak resets.
        smoke.store(true, Ordering::Relaxed);
        daemon.health_ticks.store(0, Ordering::Relaxed);
        if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
            h.await.unwrap();
        }
        assert!(daemon.serving_tx.borrow().ready);

        // Fail once more — because the streak reset, this is the FIRST failure again, so
        // it must NOT reap.
        smoke.store(false, Ordering::Relaxed);
        daemon.health_ticks.store(0, Ordering::Relaxed);
        if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
            h.await.unwrap();
        }
        assert!(
            daemon.serving_tx.borrow().ready,
            "a passing probe reset the streak — one later failure is not 'sustained'"
        );
    }

    // what this catches: the SLOW cadence. The heartbeat must NOT probe every tick (that
    // would burn one GPU decode per 5s tick, the load-per-tick the trust short-circuit
    // exists to avoid); it fires only every HEALTH_PROBE_EVERY_TICKS.
    #[tokio::test]
    async fn health_heartbeat_only_fires_on_the_slow_cadence() {
        let serves = Arc::new(AtomicUsize::new(0));
        let smoke = Arc::new(AtomicBool::new(false));
        let daemon = daemon_with(Arc::new(FakeServer {
            smoke_ok: smoke.clone(),
            ..FakeServer::healthy(serves, true)
        }));
        let _ = daemon.serving_tx.send_replace(ready_snapshot());

        // Tick 0 is due.
        let first = daemon.spawn_health_heartbeat_if_due();
        assert!(first.is_some(), "tick 0 probes");
        if let Some(h) = first {
            h.await.unwrap();
        }
        // The next HEALTH_PROBE_EVERY_TICKS-1 ticks must NOT probe, so one failure can't be
        // compounded to the reap threshold faster than the cadence.
        for _ in 1..HEALTH_PROBE_EVERY_TICKS {
            assert!(
                daemon.spawn_health_heartbeat_if_due().is_none(),
                "non-heartbeat ticks skip the probe"
            );
        }
        assert!(
            daemon.serving_tx.borrow().ready,
            "only one probe ran across the cadence window → hysteresis holds"
        );
    }

    // what this catches: DELIVERY BEATS PROBING — the trust short-circuit that was
    // untestable until the decode-age evidence became a per-daemon seam. A lane that
    // delivered real tokens inside the probe window is PROVEN alive by work that already
    // happened, so the heartbeat must not spend a slot re-proving it; a lane whose last
    // decode is older than the window has no live evidence and must probe. Regresses the
    // measured SWE-bench kill (v13): a busy lane failed two synthetic probes purely by
    // losing the slot race, the relaunch left every downstream generate refusing with
    // `serving: <none>`, and the run died — the recovery being scored as a capability zero
    // ([[a-benchmark-zero-is-a-claim-about-the-harness-until-proven-otherwise]]).
    #[tokio::test]
    async fn a_lane_proven_alive_by_real_decode_is_not_probed() {
        let window_ms = TICK.as_millis() as u64 * HEALTH_PROBE_EVERY_TICKS;
        let serves = Arc::new(AtomicUsize::new(0));

        // Fresh decode INSIDE the window → trusted, no probe.
        let slots_reads = Arc::new(AtomicUsize::new(0));
        let mut busy = daemon_with(Arc::new(FakeServer {
            smoke_ok: Arc::new(AtomicBool::new(false)),
            slots_reads: slots_reads.clone(),
            ..FakeServer::healthy(serves.clone(), true)
        }));
        busy.set_decode_age_source(Arc::new(move || Some(window_ms / 2)));
        let _ = busy.serving_tx.send_replace(ready_snapshot());
        assert!(
            busy.spawn_health_heartbeat_if_due().is_none(),
            "tokens came out half a window ago — the compute path is already proven, so the \
             heartbeat must not contend for a slot with the work proving it"
        );
        // And the trust RESETS the streak: evidence of life is evidence, not a skipped verdict.
        assert_eq!(busy.health_fails.load(Ordering::Relaxed), 0);
        // The busy lane is exactly the one the prefill knee must read (card e370a673): its
        // `/slots` read runs on its own task whether or not the smoke probe is skipped. Read
        // inside the probe, it never saw a busy lane and never published on the M5.
        tokio::time::timeout(Duration::from_secs(5), async {
            while slots_reads.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("a busy lane's /slots is read for the prefill knee even when the smoke probe is skipped");

        // Stale decode OUTSIDE the window → no live evidence, probe as usual.
        let mut quiet = daemon_with(Arc::new(FakeServer {
            smoke_ok: Arc::new(AtomicBool::new(true)),
            ..FakeServer::healthy(serves, true)
        }));
        quiet.set_decode_age_source(Arc::new(move || Some(window_ms + 1)));
        let _ = quiet.serving_tx.send_replace(ready_snapshot());
        let probe = quiet.spawn_health_heartbeat_if_due();
        assert!(
            probe.is_some(),
            "the last token predates the probe window — nothing proves this lane alive, so it \
             must be probed rather than trusted"
        );
        if let Some(h) = probe {
            h.await.unwrap();
        }
    }

    // what this catches: the #363 blackout class — a lane that REJECTS the fleet's
    // real prompts (undersized slot / mid-stream death) while both trust paths stay
    // green: recent-decode trust says alive, the 1-token smoke probe would pass.
    // Sustained REAL-turn failure must outrank both and declare the wedge, flipping
    // the snapshot not-ready + arming force_relaunch — without ever spending a probe.
    // Regression for the 2026-08-07 25-minute room blackout (serving/status
    // ready:true throughout, 4 citizens' turns all dying mid-stream).
    #[tokio::test]
    async fn sustained_real_turn_failures_outrank_a_passing_probe() {
        let serves = Arc::new(AtomicUsize::new(0));
        let mut d = daemon_with(Arc::new(FakeServer {
            // The smoke probe WOULD pass — that is the point: it must not get the
            // chance to vouch for a lane the real workload proves broken.
            smoke_ok: Arc::new(AtomicBool::new(true)),
            ..FakeServer::healthy(serves, true)
        }));
        // Fresh decode trust too (a partial stream can stamp it) — must ALSO be outranked.
        let window_ms = TICK.as_millis() as u64 * HEALTH_PROBE_EVERY_TICKS;
        d.set_decode_age_source(Arc::new(move || Some(window_ms / 2)));
        d.set_real_fails_source(Arc::new(|| HEALTH_FAILS_TO_RELAUNCH as u64));
        let _ = d.serving_tx.send_replace(ready_snapshot());

        assert!(
            d.spawn_health_heartbeat_if_due().is_none(),
            "the wedge verdict is immediate — no probe task is spawned"
        );
        assert!(
            !d.serving_tx.borrow().ready,
            "sustained real failures must flip the snapshot not-ready even though the \
             smoke probe would pass and decode trust is fresh"
        );
        assert!(
            d.force_relaunch.load(Ordering::Acquire),
            "the reconcile must be forced to re-prove the lane, not re-adopt it"
        );
    }

    // what this catches (L11 — the 2026-08-16 23-minute boot kill-loop): after a
    // reboot, the adopted lane ground through the dead core's GHOST turns — real
    // work no adapter stream observes, so decode-age trust (L9) was blind, two
    // smoke misses counted as wedge evidence, and a healthy lane was killed three
    // times. The heartbeat must consult the server's OWN /slots between misses:
    // an ADVANCING fingerprint exonerates the miss (streak resets, lane stays
    // ready); a FROZEN fingerprint keeps counting and wedges at the same
    // threshold/latency as before.
    #[tokio::test]
    async fn a_smoke_miss_on_a_lane_with_advancing_slots_is_not_wedge_evidence() {
        let serves = Arc::new(AtomicUsize::new(0));
        let slots_fp = Arc::new(AtomicU64::new(1));
        let mut d = daemon_with(Arc::new(FakeServer {
            // Every smoke probe MISSES — the ghost work holds the slots.
            smoke_ok: Arc::new(AtomicBool::new(false)),
            slots_fp: slots_fp.clone(),
            ..FakeServer::healthy(serves, true)
        }));
        // Pin both process-global evidence sources to inert test values — the
        // globals are stamped by unrelated tests under full-suite parallelism
        // (the #7 isolation class), which would silently skip the probe path.
        d.set_decode_age_source(Arc::new(|| None));
        d.set_real_fails_source(Arc::new(|| 0));
        let _ = d.serving_tx.send_replace(ready_snapshot());

        // Drive a heartbeat probe to completion (the cadence gate only fires
        // every Nth call; bounded by that cadence).
        let probe_once = |d: &ServingDaemonModule| {
            let mut h = None;
            for _ in 0..(HEALTH_PROBE_EVERY_TICKS as usize + 1) {
                if let Some(handle) = d.spawn_health_heartbeat_if_due() {
                    h = Some(handle);
                    break;
                }
            }
            h.expect("a heartbeat probe must fire within one cadence window")
        };

        // Miss #1: first miss has nothing to compare — it counts (streak = 1).
        probe_once(&d).await.unwrap();
        assert_eq!(d.health_fails.load(Ordering::Relaxed), 1);
        // The serve loop ADVANCES between misses (ghost work / other clients).
        slots_fp.store(2, Ordering::Relaxed);
        // Miss #2: fingerprint moved → exonerated, streak resets, still ready.
        probe_once(&d).await.unwrap();
        assert_eq!(
            d.health_fails.load(Ordering::Relaxed),
            0,
            "slot progress between misses proves the serve loop alive — the miss \
             must reset the streak, not feed the relaunch threshold"
        );
        assert!(d.serving_tx.borrow().ready, "lane must stay ready");
        assert!(!d.force_relaunch.load(Ordering::Acquire));

        // Now FREEZE the fingerprint (the 2026-08-05 wedge signature): two more
        // misses with no slot movement must wedge exactly as before L11.
        probe_once(&d).await.unwrap();
        assert_eq!(d.health_fails.load(Ordering::Relaxed), 1);
        probe_once(&d).await.unwrap();
        assert!(
            !d.serving_tx.borrow().ready,
            "a frozen /slots across the miss window is the real wedge — detection \
             latency must be unchanged"
        );
        assert!(d.force_relaunch.load(Ordering::Acquire));
    }

    // what this catches: a lane whose per-slot window froze at ≤ HALF the
    // currently-planned window gets RE-HOMED (relaunched) even though model +
    // genome match. A server spawned under transient memory pressure (a scratch
    // benchmark server) otherwise keeps its starved window forever — 2026-07-10:
    // the living Devstral froze at 3.8k/slot on a 131k model while 14,700 plan
    // recomputes wandered above it, and the room degenerated into a greeting
    // loop. Within 2× of the plan → hysteresis holds, no relaunch churn.
    #[tokio::test]
    async fn reconcile_re_homes_a_starved_window_but_holds_within_hysteresis() {
        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));

        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
        daemon.publish_plan(budget, &candidates, &candidates);
        let (plan_window, plan_lanes) = {
            let plan = daemon.plan_tx.borrow();
            let plan = plan.plan.as_ref().expect("plan published");
            (plan.served_context_window, plan.lanes)
        };

        // Same model + genome, but the live slot froze far below the plan.
        let _ = daemon.serving_tx.send_replace(ServingSnapshot {
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: plan_window / 4,
            lanes: plan_lanes,
            host_prompt_cache_mib: 0,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
            loading_model: None,
        });
        // A quarter of the plan is a 300% shortfall — far past the margin — but the
        // gain must still PERSIST before it buys a relaunch (BigMama's sustained-delta
        // replacement for the original 2x ratio, 2026-08-06).
        for tick in 1..REHOME_SUSTAINED_TICKS {
            assert!(
                daemon.reconcile_to_plan().is_none(),
                "tick {tick}: even a starved lane must prove the gain is sustained"
            );
        }
        daemon
            .reconcile_to_plan()
            .expect("starved window must trigger a re-home")
            .await
            .unwrap();
        assert_eq!(serves.load(Ordering::SeqCst), 1, "one relaunch");

        // Within hysteresis (> half the plan) → hold, no churn.
        let _ = daemon.serving_tx.send_replace(ServingSnapshot {
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: plan_window / 2 + 256,
            lanes: plan_lanes,
            host_prompt_cache_mib: 0,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
            loading_model: None,
        });
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "within 2\u{d7} of the plan the lane holds — no relaunch churn"
        );
        assert_eq!(serves.load(Ordering::SeqCst), 1, "still one relaunch");
    }

    // what this catches: no servable plan (empty registry) publishes the empty
    // snapshot so readers (and a grid allocator) see "nothing live here" and can
    // route the lease elsewhere — the Intel-Mac/weak-node path.
    #[tokio::test]
    async fn no_plan_publishes_empty_snapshot() {
        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));

        // Seed a live snapshot, then publish an empty plan → must clear to empty.
        let _ = daemon.serving_tx.send_replace(ServingSnapshot {
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("stale".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 4,
            host_prompt_cache_mib: 0,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
            loading_model: None,
        });
        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        daemon.publish_plan(budget, &[], &[]); // no candidates → plan None

        daemon
            .reconcile_to_plan()
            .expect("empty plan uses the operation owner")
            .await
            .unwrap();
        let snap = daemon.subscribe_serving().borrow().clone();
        assert_eq!(snap.active_model, None, "stale snapshot cleared to empty");
        assert!(!snap.ready);
        assert_eq!(serves.load(Ordering::SeqCst), 0);
    }

    // what this catches: a degraded reconcile (serve fails) publishes "nothing
    // live" and clears the in-flight gate, so the NEXT reconcile can retry
    // rather than being permanently locked out.
    #[tokio::test]
    async fn degraded_reconcile_clears_gate_and_publishes_empty() {
        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), false)));

        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
        daemon.publish_plan(budget, &candidates, &candidates);

        daemon.reconcile_to_plan().expect("spawned").await.unwrap();
        let snap = daemon.subscribe_serving().borrow().clone();
        assert!(!snap.ready, "degraded → not ready");
        assert_eq!(snap.active_model, None);
        assert!(
            !daemon.reconciling.load(Ordering::SeqCst),
            "gate cleared for retry"
        );

        // Gate cleared → a retry actually spawns again.
        daemon
            .reconcile_to_plan()
            .expect("retry spawned")
            .await
            .unwrap();
        assert_eq!(serves.load(Ordering::SeqCst), 2, "retried after degrade");
    }

    // what this catches: a reconcile emits the live snapshot on the BUS (topic
    // serving.snapshot), not just the in-process watch — the cbar fan-out that
    // lets any subscriber (and a remote grid allocator) get serving state pushed
    // without point-to-point plumbing. Regression = silent watch-only updates
    // that no subscriber ever sees.
    #[tokio::test]
    async fn reconcile_emits_snapshot_on_the_bus() {
        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));
        let bus = Arc::new(MessageBus::new());
        let _ = daemon.bus.set(bus.clone());

        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
        daemon.publish_plan(budget, &candidates, &candidates);
        daemon.reconcile_to_plan().expect("spawned").await.unwrap();

        let event = bus
            .find_recent_event(SERVING_SNAPSHOT_EVENT)
            .expect("serving.snapshot must be emitted on the bus");
        let snap: ServingSnapshot = serde_json::from_value((*event.payload).clone()).unwrap(); // test-only decode of the shared bus payload
        assert_eq!(snap.active_model.as_deref(), Some("coder-14b"));
        assert!(snap.ready, "emitted snapshot reflects the live model");
        // find_recent_event consumes newest first. A fast completion must not
        // coalesce with admission, or remote consumers remain falsely unavailable.
        let admission = bus
            .find_recent_event(SERVING_SNAPSHOT_EVENT)
            .expect("admission must reach bus consumers before completion");
        let admission: ServingSnapshot =
            serde_json::from_value((*admission.payload).clone()).unwrap();
        assert!(!admission.ready);
        assert_eq!(admission.loading_model.as_deref(), Some("coder-14b"));
        assert!(
            bus.find_recent_event(SERVING_SNAPSHOT_EVENT).is_none(),
            "one admission and one completion"
        );
    }

    // what this catches (the M5, 2026-09-20 02:4xZ): the board attributing 22.8 GB to a
    // lane whose process held 27.5 GB — the 8.7 GB `--cache-ram` the daemon itself granted
    // read as another holder's bytes, and the plan lost the second lane to it. The
    // attribution is the plan's peak plus the granted cache; nothing resident, or no
    // lanes, charges no cache.
    #[test]
    fn the_serving_attribution_includes_the_prompt_cache_it_granted() {
        let mib = 1024 * 1024;
        assert_eq!(attribute_serving(22_802_000_000, 1, 8_343), 22_802_000_000 + 8_343 * mib);
        assert_eq!(attribute_serving(22_802_000_000, 2, 0), 22_802_000_000, "no grant, no charge");
        assert_eq!(attribute_serving(0, 2, 8_343), 0, "nothing resident: the grant is not yet held");
        assert_eq!(attribute_serving(19_000_000_000, 0, 8_343), 19_000_000_000, "no lanes: weights only");
    }

    // what this catches: the footprint resolver serving hands the authority reads
    // the SAME live catalog + estimator the serving plan uses (one footprint
    // authority) — a Ready model resolves to its real on-disk weights, and an id
    // the catalog doesn't know resolves to 0 (nothing resident to attribute, never
    // a phantom the governor would over-account). If this drifted from the plan's
    // footprint the board would attribute a different number than the planner sized
    // against.
    #[test]
    fn serving_footprint_fn_resolves_live_catalog_weights() {
        use crate::model_registry::live::ModelStatus;
        use std::io::Write;

        let catalog = test_catalog();
        let id = "footprint-probe";
        catalog.register(
            fake_model(id),
            ModelStatus {
                availability: Availability::NotDownloaded,
                verified: None,
            },
        );
        let resolve = serving_footprint_fn(catalog.clone());

        // NotDownloaded (no on-disk weights) → nothing resident yet. Window/lanes
        // are the live serving shape; with no weights they resolve to 0 anyway.
        assert_eq!(
            resolve(id, 8192, 2, 0),
            0,
            "no weights on disk → nothing to attribute"
        );
        // An id the catalog has never heard of → 0, never a phantom.
        assert_eq!(resolve("never-registered", 8192, 2, 0), 0);

        // Land the artifact: real bytes on disk, flips Ready.
        let mut gguf = tempfile::Builder::new()
            .suffix(".gguf")
            .tempfile()
            .expect("temp gguf");
        gguf.write_all(&[0u8; 4096]).expect("write weights");
        gguf.flush().expect("flush");
        assert!(catalog.attach_local_artifact(id, gguf.path().to_path_buf(), None));

        // Same resolver, same live catalog → now reports the real PEAK resident bytes:
        // weights (4096) + the KV of every lane at the served window + the concurrent-
        // prefill compute reserve of every lane (#56/G5). kv_per_token floors at 20_000,
        // so 2 lanes × 20_000 × 8192 tokens of KV on top of 4096 weights, PLUS the
        // window-scaled compute reserve. Charging the KV stops it masquerading as
        // external (#79); charging the compute reserve stops the board over-reporting
        // free by the prefill buffer (G5). The compute-reserve term comes from the
        // footprint's own method so this expectation can't drift from the plan's sizing.
        // Build the expectation through the SAME quant transform the resolver applies
        // (`apply_kv_quantization` rides the live resolved KV cache decision):
        // with f16/unset this is byte-identical to raw parts; with q8_0 configured the
        // rate halves on BOTH sides. Before this the expectation assumed f16 and the
        // test failed on any box actually serving quantized KV while CI stayed green.
        let fp = apply_kv_quantization(
            footprint_from_parts(id, 4096, 8192, false, None).expect("footprint"),
        );
        let kv_per_token = fp.kv_per_token; // 20_000 raw, ÷ live quant divisor
        let expect = 4096 + 2 * kv_per_token * 8192 + fp.prefill_compute_reserve(8192, 2);
        assert_eq!(
            resolve(id, 8192, 2, 0),
            expect,
            "resolves real weights + per-lane KV + prefill compute reserve (peak), no reboot"
        );
        assert_eq!(
            resolve(id, 8192, 2, 0),
            fp.peak_resident_bytes(8192, 2),
            "resolver reports peak_resident_bytes — the plan's chosen_cost, not resident-only"
        );
        // A no-window snapshot (nothing served yet) still charges weights + every lane's
        // compute-buffer FLOOR (the reserve exists even before a window is chosen).
        assert_eq!(
            resolve(id, 0, 2, 0),
            4096 + fp.prefill_compute_reserve(0, 2),
            "no served window → weights + per-lane compute floor",
        );
        // The grant the ENGINE was launched with (off the snapshot, beside window and
        // lanes) is credited on top of the peak — the 2026-09-20 attribution — and only
        // while lanes are resident (no lanes → nothing to charge a cache to).
        assert_eq!(
            resolve(id, 8192, 2, 1_024),
            fp.peak_resident_bytes(8192, 2) + 1_024 * 1024 * 1024,
            "the running engine's --cache-ram grant is serving's, not external"
        );
        assert_eq!(
            resolve(id, 8192, 0, 1_024),
            fp.peak_resident_bytes(8192, 0),
            "no lanes → no cache charged"
        );
    }

    // what this catches: THE slice-1 production-wiring gap — ServingConsumer was
    // defined + unit-tested but never CONSTRUCTED in the boot path, so the
    // authority polled nothing and stayed blind to serving's multi-GB residency.
    // register_as_consumer (called from initialize) must register a consumer under
    // SERVING_CONSUMER_ID with the one per-machine authority. Regresses deleting
    // that registration — which would silently reopen the granted:0-while-full hole.
    #[tokio::test]
    async fn register_as_consumer_wires_serving_into_the_authority() {
        let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
        let system = Arc::new(SystemResourceMonitor::new());
        let daemon = ServingDaemonModule::new(
            gpu,
            system,
            test_resource_daemon(),
            test_catalog(),
            test_pin_store(),
        );

        assert!(
            !daemon
                .resource_daemon
                .consumer_ids()
                .contains(&SERVING_CONSUMER_ID.to_string()),
            "not registered until the daemon wires itself in"
        );
        daemon.register_as_consumer();
        assert!(
            daemon
                .resource_daemon
                .consumer_ids()
                .contains(&SERVING_CONSUMER_ID.to_string()),
            "serving must register itself as a measured consumer with the authority"
        );
    }

    // what this catches (2026-09-16, the 5090): a host sinking BELOW what its physical
    // capacity serves. `serving/unload` suppressed the 27B, so it left `candidates`;
    // the debounce read that as "incumbent gone from disk" (the case just below) and
    // adopted the 0.5B on a 32 GB card at a transient 24 GB — 35 silent relaunches
    // (#4145). The floor is the plan's OWN selection law at the PHYSICAL budget over
    // everything on disk, suppression ignored: on this card that is the 27B, so a plan
    // naming anything less is refused. On a card that cannot hold the 27B at all the
    // floor is honestly the smaller model and the same plan passes.
    #[test]
    fn a_host_never_sinks_below_what_its_physical_capacity_serves() {
        let footprint = |id: &str, weights_gb: u64, rank: u8| ModelFootprint {
            model_id: id.into(),
            weights_bytes: weights_gb * GB,
            kv_per_token: 100_000,
            context_window: 32768,
            capability_rank: rank,
            fixed_per_lane_bytes: 0,
        };
        let big = footprint("qwen3.8-27b", 17, 9);
        let tiny = footprint("qwen-0.5b", 1, 1);
        let on_disk = vec![big.clone(), tiny.clone()];
        let demand = ServingDemand::new(1, Some(8192));
        let plan_for = |m: &ModelFootprint| ServingPlan {
            base_model: m.clone(),
            served_context_window: 2048,
            lanes: 1,
            grid_overflow_lanes: 0,
            resident_models: 1,
            fits_on_gpu: true,
            rationale: String::new(),
            lanes_bound: crate::cognition::serving_plan::LanesBound::Demand,
        };
        // The 5090: 32 GB × 0.80 physical. The floor is the 27B — even though the
        // plan that asked was computed over a candidate list the unload had emptied
        // of it (the floor looks at what is ON DISK, not what is currently permitted).
        let card_5090 = HostBudget {
            usable_bytes: (32.0 * GB as f64 * 0.80) as u64,
            perf_cores: 8,
        };
        let floor = host_floor_of(card_5090, &on_disk, &demand).expect("something on disk");
        assert_eq!(floor.model_id, "qwen3.8-27b");
        assert_eq!(
            floor_gate(&plan_for(&tiny), Some(&floor)),
            FloorVerdict::Below {
                floor: "qwen3.8-27b".into(),
                wanted: "qwen-0.5b".into()
            },
            "the 0.5B on a 5090 is a demotion below physical capacity, whatever the live budget said"
        );
        assert_eq!(floor_gate(&plan_for(&big), Some(&floor)), FloorVerdict::AtOrAbove);
        // A card the 27B cannot fit at all (16 GB × 0.80 < 17 GB weights): the floor is
        // honestly the small model, and serving it is not a demotion.
        let small_card = HostBudget {
            usable_bytes: (16.0 * GB as f64 * 0.80) as u64,
            perf_cores: 4,
        };
        let floor = host_floor_of(small_card, &on_disk, &demand).expect("something on disk");
        assert_eq!(floor.model_id, "qwen-0.5b");
        assert_eq!(floor_gate(&plan_for(&tiny), Some(&floor)), FloorVerdict::AtOrAbove);
        // THE PIN IS THE FLOOR'S CEILING (Fable, #4146 review): the M5 pins a lower-ranked
        // model with a higher-ranked one on disk. The floor is judged over the PINNED
        // universe, so the pinned model IS the floor and serving it is never a demotion.
        let ornith = footprint("ornith-35b-a3b", 20, 5);
        let pinned_universe = vec![ornith.clone()]; // what servable_candidates yields under the pin
        let floor =
            host_floor_of(card_5090, &pinned_universe, &demand).expect("pinned model on disk");
        assert_eq!(floor.model_id, "ornith-35b-a3b");
        assert_eq!(
            floor_gate(&plan_for(&ornith), Some(&floor)),
            FloorVerdict::AtOrAbove
        );
        // No floor (nothing on disk / no GPU) never refuses.
        assert_eq!(floor_gate(&plan_for(&tiny), None), FloorVerdict::AtOrAbove);
    }

    // what this catches (#368, 2nd occurrence): a ONE-tick budget collapse must
    // classify as a Downshift (so publish_plan HOLDS the incumbent plan until
    // the squeeze proves sustained) — while same-model, upshift, and
    // incumbent-gone cases adopt immediately. The live failure: usable_gb hit 0
    // for ~6 seconds at solve launch, the fresh plan named the 0.5B, and the
    // reconcile re-brained every citizen onto it before the dip had even passed.
    #[test]
    fn downshift_gate_classifies_the_transient_lobotomy() {
        let footprint = |id: &str, weights_gb: u64, rank: u8| ModelFootprint {
            model_id: id.into(),
            weights_bytes: weights_gb * GB,
            kv_per_token: 100_000,
            context_window: 32768,
            capability_rank: rank,
            fixed_per_lane_bytes: 0,
        };
        let devstral = footprint("devstral-24b", 14, 8);
        let tiny = footprint("qwen-0.5b", 1, 1);
        let plan_for = |id: &str| ServingPlan {
            base_model: footprint(id, 1, 1),
            served_context_window: 2048,
            lanes: 1,
            grid_overflow_lanes: 0,
            resident_models: 1,
            fits_on_gpu: true,
            rationale: String::new(),
            lanes_bound: crate::cognition::serving_plan::LanesBound::Demand,
        };
        let both = vec![devstral.clone(), tiny.clone()];

        // The incident shape: incumbent Devstral, fresh wants the 0.5B → HOLD.
        assert_eq!(
            downshift_gate(&plan_for("qwen-0.5b"), Some("devstral-24b"), &both),
            DownshiftVerdict::Downshift,
        );
        // Same model → nothing to debounce.
        assert_eq!(
            downshift_gate(&plan_for("devstral-24b"), Some("devstral-24b"), &both),
            DownshiftVerdict::NotADownshift,
        );
        // UPSHIFT (0.5B incumbent, fresh wants Devstral) must adopt immediately —
        // holding it would have kept the citizens lobotomized on purpose.
        assert_eq!(
            downshift_gate(&plan_for("devstral-24b"), Some("qwen-0.5b"), &both),
            DownshiftVerdict::NotADownshift,
        );
        // Incumbent vanished from disk → holding a plan naming a ghost is worse
        // than any downshift; adopt what is actually present.
        assert_eq!(
            downshift_gate(&plan_for("qwen-0.5b"), Some("devstral-24b"), &[tiny]),
            DownshiftVerdict::NotADownshift,
        );
        // No incumbent (boot) → plain adoption.
        assert_eq!(
            downshift_gate(&plan_for("qwen-0.5b"), None, &both),
            DownshiftVerdict::NotADownshift,
        );
    }

    // what this catches (card 7c72c0f0, Intel Mac 2026-09-17): the plan's budget is the
    // row the lane's PLACEMENT puts its bytes in. A CPU-placed lane budgets against the
    // Ram row — the 30 GB the KV actually fills — not the 4 GB Vram row of a card that
    // holds nothing of it (nine citizens sized against 5.7 GB on a 34.4 GB box: one warm
    // slot, eight exiled to a remote lane). The GPU arm is byte-identical to before.
    #[tokio::test]
    async fn a_cpu_placed_lane_budgets_against_the_ram_row_not_the_card() {
        use crate::inference::llama_server::LanePlacement;
        use crate::resources::{DaemonConfig, GovernorConfig, MockCapacitySource, ResourceDaemon};
        let vram = Arc::new(MockCapacitySource::new(crate::resources::ResourceKind::Vram, 4_000));
        let ram = Arc::new(MockCapacitySource::new(crate::resources::ResourceKind::Ram, 30_000));
        let daemon = ResourceDaemon::start(
            vec![vram, ram],
            vec![],
            DaemonConfig {
                tick_interval: std::time::Duration::from_millis(20),
                min_reclaim_budget: std::time::Duration::from_millis(100),
                governor: GovernorConfig {
                    min_dwell_ms: 0,
                    graceful_grace_ms: 50,
                },
            },
        );
        assert_eq!(pool_kind_for(LanePlacement::Cpu), crate::resources::ResourceKind::Ram);
        assert_eq!(pool_kind_for(LanePlacement::Gpu), crate::resources::ResourceKind::Vram);
        assert_eq!(
            governed_pool_ceiling(&daemon, pool_kind_for(LanePlacement::Cpu)),
            Some(30_000),
            "a CPU-placed lane plans against the RAM its KV lives in"
        );
        assert_eq!(
            governed_pool_ceiling(&daemon, pool_kind_for(LanePlacement::Gpu)),
            Some(4_000),
            "a GPU-placed lane plans against the card"
        );
    }

    // what this catches (#225, Joel 2026-08-08 "the budgeter just has all its
    // parts figure it out"): serving's plan budget (`governed_vram_ceiling`)
    // reads serving's OWN view of the board — global available minus every
    // OTHER consumer's unmet reservation floor — so the plan can no longer size
    // the window into the embed lane's slice and starve a faculty cognition
    // needs every turn. Regression: reading the reservation-blind global
    // `available_bytes` again would make this budget ignore the floor.
    #[tokio::test]
    async fn serving_budget_plans_around_other_consumers_floors() {
        use crate::resources::{DaemonConfig, GovernorConfig, MockCapacitySource, ResourceDaemon};
        let src = Arc::new(MockCapacitySource::new(
            crate::resources::ResourceKind::Vram,
            10_000,
        ));
        let daemon = ResourceDaemon::start(
            vec![src],
            vec![],
            DaemonConfig {
                tick_interval: std::time::Duration::from_millis(20),
                min_reclaim_budget: std::time::Duration::from_millis(100),
                governor: GovernorConfig {
                    min_dwell_ms: 0,
                    graceful_grace_ms: 50,
                },
            },
        );
        // No floors → serving sees the whole board.
        assert_eq!(governed_vram_ceiling(&daemon), Some(10_000));
        // The embed lane claims its standing floor → serving's plannable view
        // shrinks by exactly that slice; the board's Vram row still exists so
        // the None ("governor hasn't reported") semantics are untouched.
        daemon.reserve("embed", crate::resources::ResourceKind::Vram, 1_800);
        assert_eq!(governed_vram_ceiling(&daemon), Some(8_200));
        // Serving's own hypothetical floor would NOT count against itself.
        daemon.reserve(
            SERVING_CONSUMER_ID,
            crate::resources::ResourceKind::Vram,
            3_000,
        );
        assert_eq!(governed_vram_ceiling(&daemon), Some(8_200));
    }

    // what this catches: a node saying "no servable model on disk" while its weights sit
    // there (the 5090, 2026-09-08: zero citizens for an hour, candidates: 0, and the plan
    // named neither the models it refused nor why). Every refusal has a reason and the
    // reason reaches the reader.
    #[test]
    fn every_refused_candidate_is_named_with_its_reason() {
        use std::collections::HashSet;
        let snap = test_catalog().snapshot();
        // ELIGIBLE too: the three opt-out rows (an embedding model, the benchmark
        // opponent, Devstral) are refused for a different reason and would make the last
        // assertion here read as a bug in the filter rather than as the catalog's intent.
        let any = snap
            .models
            .values()
            .find(|l| {
                l.status.availability == Availability::Ready
                    && can_serve_minds(&l.model)
                    && l.model.persona_serving_eligible
                    && footprint_for(&l.model).is_some()
            })
            .map(|l| l.model.id.clone());
        let Some(model_id) = any else {
            return; // a catalog with nothing ready has nothing to refuse
        };
        // Suppressed by an unload.
        let mut sup = HashSet::new();
        sup.insert(model_id.clone());
        let (kept, refused) = servable_candidates_with_refusals(&snap, &sup, &None);
        assert!(!kept.iter().any(|c| c.model_id == model_id));
        assert!(
            refused.iter().any(|r| r.model_id == model_id && r.reason == REFUSED_SUPPRESSED),
            "a suppressed model names the unload: {refused:?}"
        );
        // Another model pinned.
        let (kept, refused) =
            servable_candidates_with_refusals(&snap, &HashSet::new(), &Some("not-a-real-model".to_string()));
        assert!(kept.is_empty(), "nothing matches a pin for a model that is not here");
        assert!(
            refused.iter().all(|r| r.reason == REFUSED_NOT_PINNED) && !refused.is_empty(),
            "every refusal names the pin: {refused:?}"
        );
        // Nothing suppressed, nothing pinned: eligible rows survive.
        let (kept, _) = servable_candidates_with_refusals(&snap, &HashSet::new(), &None);
        assert!(kept.iter().any(|c| c.model_id == model_id));
    }


    /// This file's PRODUCTION half — everything before `#[cfg(test)]`.
    ///
    /// A source assertion must never be able to satisfy itself. `include_str!` pulls in
    /// the whole file, tests included, so `src.contains("install_lane_admission(")`
    /// matched the assertion's OWN string literal and stayed green with the production
    /// call deleted — a check that cannot fail, which is the exact class this repo keeps
    /// getting bitten by ([[success-signal-that-cannot-report-failure]]). Caught here by
    /// a sibling assertion tripping over the same self-reference.
    /// The daemon's production source: this file up to its test mod, plus every file of
    /// the package. A source assertion must see a seam wherever it was moved to.
    fn production_source() -> &'static str {
        static SRC: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            let src = include_str!("serving_daemon.rs");
            let cut = src
                .find("
#[cfg(test)]")
                .expect("this file has a test mod, so the marker must exist");
            [
                &src[..cut],
                include_str!("serving_daemon/academy_batch.rs"),
                include_str!("serving_daemon/lane_demand.rs"),
                include_str!("serving_daemon/gates.rs"),
                include_str!("serving_daemon/host_budget.rs"),
                include_str!("serving_daemon/vram.rs"),
                include_str!("serving_daemon/prompt_cache.rs"),
                include_str!("serving_daemon/candidates.rs"),
                include_str!("serving_daemon/snapshot.rs"),
                include_str!("serving_daemon/wiring.rs"),
                include_str!("serving_daemon/intent.rs"),
                include_str!("serving_daemon/plan.rs"),
                include_str!("serving_daemon/measure.rs"),
                include_str!("serving_daemon/reconcile.rs"),
                include_str!("serving_daemon/health.rs"),
                include_str!("serving_daemon/publish.rs"),
            ]
            .concat()
        });
        &SRC
    }

    /// A stored pin must never be able to take the node dark. Nested per the one-mod rule.
    mod a_bad_pin_cannot_brick_the_node {
        use super::*;

        // what this catches: the #3955 outage, twice on this host. A stored pin naming
        // `AtomicChat/Qwen3.8-Flash-Next-GGUF` (`availability: not_downloaded`) was
        // honoured absolutely at boot: every downloaded candidate refused as "another
        // model is pinned", the pinned one unloadable, so zero lanes and zero citizens —
        // with NOTHING red. `serving/pin` fit-gates at SET time, so this shape can only
        // arrive via RESTORE, which had no check at all.
        //
        // The daemon's constructor needs a GPU, a monitor and a resource daemon, so the
        // decision is asserted where it is made: honour a restored pin only when the
        // catalog says the model is actually servable.
        #[test]
        fn a_restored_pin_is_honoured_only_when_the_model_is_servable() {
            let src = production_source();
            assert!(
                src.contains("serving.pin.unservable_ignored"),
                "an unservable restored pin must be IGNORED with a loud probe — honouring                  it refuses every real candidate and takes the node to zero lanes"
            );
            assert!(
                src.contains("entry.status.availability == Availability::Ready"),
                "and the test for servable must be the catalog's own availability, not a                  guess about the file"
            );
            // Intent is durable: the file is kept so the pin is honoured again once the
            // weights land. A fix that silently discarded the operator's choice would be
            // its own bug.
            assert!(
                !src.contains("pin_store.clear()"),
                "the pin FILE must survive being ignored — pull the weights and the next                  boot honours it; only serving/unpin drops the intent"
            );
        }

        // what this catches: the refusal that named every loser and never the winner.
        // Three refusals a second, none identifying the pin doing the refusing, so
        // diagnosing it needed a grep of a different log for `serving.pin.restored`.
        #[test]
        fn a_refusal_names_the_pin_that_caused_it() {
            let src = production_source();
            let probe = src
                .split("class = \"serving.plan.candidate_refused\"")
                .nth(1)
                .expect("the refusal probe exists");
            let body = &probe[..probe.find(");").expect("probe ends")];
            assert!(
                body.contains("pinned ="),
                "the refusal must carry the pin that blocked the candidate — a reason that                  states its category but not its cause cannot be acted on: {body}"
            );
        }
    }

    /// The gate that replaced #56's unbuilt `ResourceGovernor`. Nested per the one-mod rule.
    mod engine_admission {
        use super::*;
        use crate::inference::llama_server::LaneAdmission;

        fn refusal(a: LaneAdmission) -> String {
            match a {
                LaneAdmission::Refuse(r) => r,
                LaneAdmission::Admit => panic!("expected a refusal"),
            }
        }
        fn is_admit(a: &LaneAdmission) -> bool {
            matches!(a, LaneAdmission::Admit)
        }

        // what this catches: THE bug. Three vision sidecars co-existed on one GPU because
        // the only question asked before spawning was `first_free_port` — is a port free.
        // A second engine that does not fit beside the live lane must be refused. If this
        // ever admits, ports are deciding device capacity again and the box goes back to
        // 3.1-8.6 tok/s per engine.
        #[test]
        fn a_second_engine_that_does_not_fit_is_refused() {
            // 24 GB budget, a 19 GB live lane already up, another 19 GB engine asked for.
            let d = lane_admission_decision(24_000_000_000, 19_000_000_000, 19_000_000_000, 0, 0);
            let why = refusal(d);
            for term in ["19.0 GB", "24.0 GB", "5.0 GB"] {
                assert!(why.contains(term), "the refusal must state its terms: {why}");
            }
            assert!(
                why.contains("time-slice"),
                "and must say WHY co-residence is not extra capacity: {why}"
            );
        }

        // what this catches: the sign of the incumbent. A pin SWAPS, so `pin_fit_decision`
        // credits the incumbent back into the budget; an ephemeral lane JOINS, so it must
        // be spent. Flip this and a full device reads as empty — which is precisely the
        // second-engine admission this gate exists to stop.
        #[test]
        fn the_live_lane_is_spent_not_credited() {
            let budget = 24_000_000_000;
            let want = 19_000_000_000;
            // Nothing resident: the same engine fits.
            assert!(
                is_admit(&lane_admission_decision(budget, want, 0, 0, 0)),
                "with an empty device this engine fits"
            );
            // Same engine, same budget, live lane up: it must NOT fit. If the incumbent
            // were credited back (the pin arithmetic) this would admit.
            assert!(
                !is_admit(&lane_admission_decision(budget, want, 19_000_000_000, 0, 0)),
                "co-residence spends the incumbent — crediting it back is the pin's rule, \
                 not this one"
            );
        }

        // what this catches: siblings must weigh. The measured failure was the THIRD
        // sidecar — each one individually small enough to look affordable, the set of
        // them not. An admission that ignores existing ephemeral lanes admits N engines
        // one at a time.
        #[test]
        fn ephemeral_siblings_weigh_on_the_decision() {
            let budget = 24_000_000_000;
            let want = 6_000_000_000;
            assert!(
                is_admit(&lane_admission_decision(budget, want, 6_000_000_000, 0, 0)),
                "one sibling's worth of room is there"
            );
            // Two 6 GB siblings already up beside a 6 GB live lane: 18 committed, 6 free —
            // still exactly affordable.
            assert!(
                is_admit(&lane_admission_decision(budget, want, 6_000_000_000, 12_000_000_000, 2)),
                "exactly-affordable must admit — this gate refuses overflow, not company"
            );
            // A third sibling tips it over.
            let why = refusal(lane_admission_decision(
                budget,
                want,
                6_000_000_000,
                18_000_000_000,
                3,
            ));
            assert!(
                why.contains("3 ephemeral sibling(s)"),
                "the refusal names how many were already there: {why}"
            );
        }

        // what this catches (2026-09-15 00:17Z): the vision sidecar was admitted on a live
        // free-RAM read taken inside the main lane's relaunch window (weights + KV
        // momentarily unwired) and the box swapped 26 GB. The PLANNED headroom after the
        // main lane's working set — full weights, KV at the served window × lanes, the
        // compute reserve, the prompt cache, the OS floor — must say NO on a 64 GB box
        // serving a 21 GB model over 6 lanes at 59k, whatever the live read says.
        #[test]
        fn the_sidecar_headroom_is_planned_not_a_relaunch_window_free_read() {
            let mut model = fake_model("ornith-ai/Ornith-1.5-35B-A3B-GGUF");
            model.weights_bytes = Some(21 * 1024 * 1024 * 1024);
            model.context_window = 262_144;
            let target = ServingTarget {
                host_prompt_cache_mib: 2505,
                model,
                context_window: 59_392,
                lanes: 6,
                adapters: Vec::new(),
                placement: crate::inference::llama_server::main_lane_placement(),
                expert_placement: None,
                resident_override: None,
                vision_sidecar: false,
            };
            let physical = 64 * 1024 * 1024 * 1024;
            let headroom = sidecar_planned_headroom_bytes(&target, 59_392, physical);
            let need = 22_131_161_622; // the 27B sidecar's measured need on 2026-09-15
            assert!(headroom < need, "planned headroom {headroom} must refuse a {need}-byte sidecar beside a 21 GB main lane on 64 GB");
            // Not a blanket refusal: the same lane on a 256 GB box leaves the sidecar room.
            let big = sidecar_planned_headroom_bytes(&target, 59_392, 256 * 1024 * 1024 * 1024);
            assert!(big >= need, "on 256 GB the planned headroom {big} admits the sidecar");
            // A row without a footprint (no stamped weights, no resolvable GGUF) yields ZERO
            // headroom — the live read alone never admits.
            let mut blind = target.model.clone();
            blind.weights_bytes = None;
            let blind_target = ServingTarget { model: blind, ..target.clone() };
            assert_eq!(sidecar_planned_headroom_bytes(&blind_target, 59_392, physical), 0);
        }


        // what this catches: the authority going UNWIRED — which is not hypothetical, it
        // is the whole reason this gate had to be written. `EphemeralServingLane::spawn`
        // carried a doc comment for months saying budget-gating "is the ResourceGovernor's
        // concern (#56)", and `ResourceGovernor` was never built: all eight of its mentions
        // in the tree are doc comments promising a future owner. So the decision was
        // delegated to nobody and made by `first_free_port`.
        //
        // A source assertion because installation happens inside the daemon's async
        // `initialize`, and the seam it writes to is a process-wide `OnceLock` a unit test
        // cannot un-set between runs. Same shape as the `source_hygiene` ratchets: cheap,
        // and it fails the moment someone deletes the call.
        #[test]
        fn the_serving_daemon_installs_the_admission_authority() {
            let src = production_source();
            assert!(
                src.contains("install_lane_admission("),
                "serving_daemon must INSTALL the device-budget authority — without it                  EphemeralServingLane::spawn admits every engine on a free-port check,                  which is how three sidecars once shared one GPU"
            );
            assert!(
                src.contains("fn lane_admission_authority("),
                "and must build it from the same live budget the planner uses, so the gate                  and the planner cannot disagree about what fits"
            );
        }

        // what this catches: an over-committed device must not wrap. `saturating_sub` on
        // headroom is what keeps a device already past its budget from reporting a huge
        // free number and admitting everything — the arithmetic failure mode that turns a
        // guard into an accelerator.
        #[test]
        fn an_already_overcommitted_device_refuses_rather_than_wrapping() {
            let why = refusal(lane_admission_decision(
                8_000_000_000,
                1_000_000_000,
                20_000_000_000,
                0,
                0,
            ));
            assert!(
                why.contains("headroom 0.0 GB"),
                "past the budget, headroom floors at zero: {why}"
            );
        }
    }

    // what this catches (2026-09-18 21:56:03Z, the M5, card below): the live free read
    // compared RAW admitted a 24.1 GB sidecar into 26.7 GB "free" beside the 27B, and
    // the box paged (swap 6.7 GB, compressor 20 GB). With the same OS floor the planned
    // term keeps, every measured tick of that boot refuses it, the tighter term still
    // wins, and a small VL sidecar is still admitted — sight is not refused, a mind-sized
    // sidecar is. Numbers are the measured ones; the floor is the rule's own.
    #[test]
    fn the_live_free_read_keeps_the_same_os_floor_the_planned_term_keeps() {
        const GB: u64 = 1024 * 1024 * 1024;
        let physical = 64 * GB;
        let floor = host_os_floor_bytes(physical);
        let need = 24_078_233_892u64;
        // 21:55:27Z: live 14.2 GB, planned 16.6 GB → refused either way.
        assert!(sidecar_admissible_bytes(14_215_921_664, 16_647_885_296, physical) < need);
        // 21:56:03Z: live 26.7 GB, planned 31.8 GB → admitted before; REFUSED now.
        let admitted_before = 26_689_634_304u64.min(31_805_592_074);
        assert!(admitted_before >= need, "the raw comparison said yes — that is the defect");
        let now = sidecar_admissible_bytes(26_689_634_304, 31_805_592_074, physical);
        assert!(now < need, "with the floor ({} GB) the same tick refuses: {} < {}", floor / GB, now, need);
        assert_eq!(now, 26_689_634_304 - floor, "the live term after the floor is the tighter one here");
        // The planned term still wins when it is the tighter one.
        assert_eq!(sidecar_admissible_bytes(40 * GB, 10_374_868_340, physical), 10_374_868_340);
        // A small VL sidecar (4 GB) is still admitted on the same tick.
        assert!(now >= 4 * GB, "sight is not refused, a mind-sized sidecar is");
        // An over-full box yields zero, never a wrap.
        assert_eq!(sidecar_admissible_bytes(floor / 2, 31 * GB, physical), 0);
    }

    // what this catches (Cormac's three arms on #4207 — the 5090's 2 × 17k for 30k coders on
    // a fresh boot): (1) a live median wins and is the one remembered; (2) no live median +
    // a record for the SAME model → the plan floors from it; (3) no live median + a record
    // for a DIFFERENT model, or a 0 on disk → nothing: a seat is never pinned to a stale or
    // foreign prompt size. Pure over the store's record, so no daemon and no disk.
    #[test]
    fn the_typical_prompt_is_live_when_measured_remembered_for_the_same_model_and_never_foreign() {
        use crate::modules::served_window_store::StoredServedWindow;
        let rec = |model: &str, typical: u32| StoredServedWindow {
            model_id: model.into(), per_slot_window: 49_243, lanes: 1, typical_prompt_tokens: typical, set_at_ms: 1,
        };
        let same = rec("qwen-27b", 30_000);
        assert_eq!(typical_prompt_for_plan(Some(31_000), Some("qwen-27b"), Some(&same)), TypicalPrompt::Live(31_000), "arm 1: live wins over the record");
        assert_eq!(typical_prompt_for_plan(None, Some("qwen-27b"), Some(&same)), TypicalPrompt::Remembered(30_000), "arm 2: unmeasured → the same model's remembered prompt");
        assert_eq!(typical_prompt_for_plan(None, Some("qwen-27b"), Some(&rec("other-4b", 33_000))), TypicalPrompt::None, "arm 3a: a different model's record floors nothing");
        assert_eq!(typical_prompt_for_plan(None, Some("qwen-27b"), Some(&rec("qwen-27b", 0))), TypicalPrompt::None, "arm 3b: a 0 on disk is not a floor");
        assert_eq!(typical_prompt_for_plan(None, None, Some(&same)), TypicalPrompt::None, "no seat model (a cold boot with no record model) floors nothing");
        assert_eq!(typical_prompt_for_plan(Some(0), Some("qwen-27b"), Some(&same)), TypicalPrompt::Remembered(30_000), "a live 0 is unmeasured, not a measurement");
    }
}
