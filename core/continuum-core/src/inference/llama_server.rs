//! llama_server.rs — Rust owns the llama-server process.
//!
//! This is the unsloth-Studio replacement for LOCAL serving control. Where
//! `unsloth_control` drove unsloth-Studio's management API (`/api/inference/load`
//! etc.) over HTTP, llama.cpp's `llama-server` has NO such management surface — it
//! serves exactly ONE model passed at launch and exposes only `/v1/*`, `/props`,
//! and `/health`. So "switch model" is not an API call; it is a process relaunch.
//! The clean consequence: the core OWNS the C++ launch. No second Python Studio
//! racing us for the GPU, no dual-server confusion — one supervised child, one
//! source of truth for "what is being served right now."
//!
//! Shape: this module is the *leaf* of the serving stack. The
//! [`crate::modules::serving_daemon::ServingDaemonModule`] is the canonical
//! ServiceModule (own tick + `watch` publish, per CONCURRENCY-STYLE-GUIDE.md); it
//! DECIDES which model to serve and how many lanes, then asks this process-owner
//! to reconcile the running `llama-server` to that decision. Degrade-never-panic:
//! a spawn or readiness failure surfaces as [`EnsureOutcome::Degraded`] with a
//! named reason, never an `unwrap`/panic on the daemon's tick.
//!
//! The published [`ServingSnapshot`] (which model is live, is it ready, on what
//! base url) is also the grid seam: a node going down makes its snapshot go
//! stale/empty; a node coming online publishes a fresh one. A grid allocator
//! contracts inference leases against these snapshots. That layer is deferred
//! (single-machine first), but the snapshot is the rail it slots onto.

use crate::model_registry::Model;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;

/// Default root the core serves from when nothing overrides it. llama-server's
/// OpenAI surface lives under `/v1`; `/health` and `/props` are at the root.
const DEFAULT_HOST: &str = "127.0.0.1";
/// The CANONICAL local serving port. The live persona lane PINS this — it never
/// scans away from it ([`chosen_port`]). Pinning is what makes a crashed core's
/// orphan reclaimable: the fresh core looks for its predecessor exactly here,
/// adopts it if healthy or reaps it via [`crate::inference::lane_pidfile`] if not,
/// and binds the same port. The old "scan up to the first free port" behavior was
/// the bug, not the resilience: scanning PAST a held canonical port both spawned a
/// SECOND model on the one GPU (Metal decode-time OOM → silent persona abstain)
/// AND pointed the daemon's own `/v1` probe at the empty scanned port, blinding it
/// to the perfectly-good server it could have adopted. A foreign squatter on this
/// port now fails the bind LOUD ([[fallbacks-are-illegal-fail-loud]]) instead of
/// fleeing to a competitor — the operator relocates it or sets
/// `LLAMA_SERVER_BASE_URL`. (Throwaway measurement lanes are different: they WANT
/// a distinct port and keep scanning — see [`EphemeralServingLane`].)
const DEFAULT_PORT: u16 = 58057;

/// Compile-time default OpenAI `/v1` base for the local llama-server provider spec
/// ([`crate::model_registry::catalog`]). A placeholder only: the adapter is
/// repointed at the serving daemon's live snapshot `base_url` (== [`serving_v1_url`])
/// at registration. Mirrors [`DEFAULT_PORT`]; operator override is
/// `LLAMA_SERVER_BASE_URL`. Owned HERE — never the dead Unsloth `:8888` gateway.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:58057/v1";

/// How many ports above a base an EPHEMERAL measurement lane scans for a free one
/// ([`first_free_port`], used only by [`EphemeralServingLane`]) before giving up
/// and binding the base (letting the spawn fail loud rather than serving somewhere
/// unexpected). A small window: if 64 consecutive ports are taken the machine has
/// a real problem worth surfacing. The live lane does NOT scan — it pins
/// [`DEFAULT_PORT`]. The idealized single registry of every port in the system
/// (Joel: "a singular place that keeps track of ports") is deferred as
/// over-engineering for now.
const PORT_SCAN_WINDOW: u16 = 64;

/// How long we wait for a freshly-spawned server to answer `/health` with 200
/// before declaring the launch degraded. Model load (mmap + Metal warm) can take
/// many seconds for a large GGUF; this is generous but bounded so a wedged
/// launch can't hang the daemon's reconcile forever.
const READY_TIMEOUT: Duration = Duration::from_secs(90);

/// Poll cadence while waiting for `/health`. 503 → still loading, keep waiting.
const READY_POLL: Duration = Duration::from_millis(500);

/// Bounded per-request timeout for control-plane probes (`/v1/models` in
/// [`active_model`], `/health` in [`wait_ready`]). A healthy llama-server answers
/// these in well under a second even mid-generation — they are served off the HTTP
/// layer, not queued behind the inference slots. A WEDGED server, though — a GPU
/// hang, OOM-thrash, deadlock, or a frozen orphan inherited from a crashed
/// predecessor — holds the listening socket open so the TCP connect SUCCEEDS, then
/// never answers the read. Without a bound the probe blocks forever: in
/// [`active_model`] that means `ensure_model_serving` never returns, so the
/// reconcile that would reclaim the sick lane never runs (the lane stays wedged and
/// the gateway never registers); in [`wait_ready`] an unbounded `send` silently
/// defeats the [`READY_TIMEOUT`] deadline, which is only checked AFTER `send`
/// returns. Bounding each probe converts "wedged" into a fast `Unreachable` — the
/// exact signal `ensure_model_serving` turns into serve()+reclaim. Kept under the
/// serving daemon's reconcile cadence so a probe never overruns its own tick.
const PROBE_TIMEOUT: Duration = Duration::from_secs(4);

/// Bound for the decode SMOKE-PROBE (one-token generation in [`decode_smoke_ok`]).
/// Unlike a control-plane read, this queues behind the inference slots and runs an
/// actual decode, so it is given more room than [`PROBE_TIMEOUT`] — but still
/// bounded well under the daemon's own budget so a wedged compute path (the very
/// thing we are probing for) resolves to "cannot decode" fast instead of hanging
/// the reconcile. A healthy 14B answers a 1-token request in well under a second.
const DECODE_SMOKE_TIMEOUT: Duration = Duration::from_secs(10);

/// A same-model/same-genome relaunch is required when the target per-slot window
/// exceeds the running server's served window by MORE than this. llama.cpp has no
/// hot-resize API, so a genuine window GROW can only be honored by a relaunch —
/// exactly like an adapter-set change. The margin absorbs llama.cpp's internal
/// 256-multiple padding of the launch `-c/--parallel` window (served ≈ round-up-256
/// of the launched per-slot value), so a padded steady-state window never reads as
/// a spurious grow and re-triggers a relaunch every tick. Comfortably above one
/// 256-pad; the daemon only sends a grow target when it is ≥ 2× the served window
/// (its `starved` gate), so this margin never masks a real grow.
const WINDOW_RELAUNCH_TOLERANCE: u32 = 512;

/// Minimum completion tokens a healthy lane must produce on the decode smoke-probe.
/// The failure mode this guards is the intermittently-wedged fresh lane that answers
/// EVERY request with ~2 tokens then stops (observed on ephemeral eval lanes: same
/// base+gene generated 82 tok/task on one spawn, 2 tok/task on the next, silently
/// scoring the whole benchmark 0). An HTTP-200-only probe with `max_tokens: 1` cannot
/// tell that lane from a healthy one — so the probe now forces a prompt a healthy model
/// MUST answer with many tokens and asserts it did. `5` sits comfortably above the
/// ~2-token wedge and far below the ~20+ a healthy "count to 20" yields.
const MIN_SMOKE_DECODE_TOKENS: u64 = 5;

/// Everything the launcher needs to bring a model up, GROUPED so adding a new
/// serving knob is a field here — never a new param threaded down the call chain
/// ([[pass-the-model-struct-no-param-hell]]). The [`Model`] carries its own id,
/// gguf path, chat template, and trained `context_window` (resolved ONCE at
/// registry load — never re-fetched downstream). `context_window` here is the
/// HOST-FIT served window the serving daemon derived for THIS host from
/// `(model.context_window ∩ budget/lanes/kv)` — a computed property of the
/// serving plan, NOT a hardcoded constant cap. It is the single source of truth
/// for the effective serving window (task #50): it becomes llama-server's `-c`,
/// the persona's `context_length`, and the deliberation prompt budget.
#[derive(Debug, Clone)]
pub struct ServingTarget {
    /// The chosen base model — the grouped model info, resolved once.
    pub model: Model,
    /// The host-fit served window the planner computed for this host — the
    /// PER-LANE window. Sized to fit the working set (tool schemas + framing +
    /// room burst + recalled memory + completion reserve) within the budget,
    /// capped only by the model's own trained ceiling. The deliberation faculty
    /// keeps its prompt inside this so llama-server never 500s ("Context size
    /// has been exceeded").
    ///
    /// CRUCIAL: this is per-lane, NOT the `-c` total. llama-server's `-c` is the
    /// WHOLE KV cache, split evenly across `--parallel` slots — each request
    /// only gets `-c / n_parallel` tokens. So to hand each of `lanes` slots a
    /// full `context_window`, the spawn launches `-c (context_window * lanes)
    /// --parallel lanes`. Launching `-c context_window` without `--parallel`
    /// (the prior bug) let llama.cpp default to 4 slots and silently quartered
    /// the per-request window, 500-ing every deliberation whose prompt fit the
    /// planned window but overflowed the unplanned per-slot share.
    pub context_window: u32,
    /// Continuous-batching lanes (`n_seq_max`) from the plan (≥ 1). Drives
    /// llama-server's `--parallel`. The plan budgeted `kv_at(context_window) *
    /// lanes` of KV against the host budget, so `-c (context_window * lanes)`
    /// is memory-safe by the planner's own arithmetic.
    pub lanes: u32,
    /// The trained LoRA genome layers to load into the serving catalog at spawn
    /// (`llama-server --lora <path>` per entry). This is the SET — which genes
    /// are *loadable*; the per-request `"lora":[{id,scale}]` body field decides
    /// which page IN for a given turn. llama.cpp has no hot-load API, so a change
    /// to this set is a relaunch (rare — genes are produced post-training);
    /// page-in/out within a loaded set never relaunches. Empty = base model only
    /// (the legitimate no-genes-trained state, NOT a fallback).
    pub adapters: Vec<AdapterEntry>,
    /// Where this lane's weights run. The live persona lane runs on the GPU
    /// (`Gpu` — every offloadable layer) for throughput; a throwaway measurement
    /// lane that must COEXIST with the living GPU lane runs on the CPU (`Cpu` —
    /// `--n-gpu-layers 0`) so it never contends for the VRAM the living persona
    /// already holds. On a single GPU two resident models OOM the Metal command
    /// buffer at decode time (`kIOGPUCommandBufferCallbackErrorOutOfMemory`),
    /// which surfaced as a silent all-empty eval; pinning the eval lane to CPU
    /// honors humane-eval (#59 — never degrade the living lane) AND the
    /// single-GPU budget (#56) by using the free CPU RAM (the misfit-toy
    /// resource). This is an explicit placement the CALLER chooses, NOT a silent
    /// fallback — when the `ResourceGovernor` (#56) lands it owns this decision
    /// (tier the live lane down, route to a grid peer with free GPU, or pin CPU).
    pub placement: LanePlacement,
}

/// Where a serving lane's model weights are resident — see [`ServingTarget::placement`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LanePlacement {
    /// Offload every layer llama-server can to the GPU — its default, so we omit
    /// the flag. The throughput placement for the living persona lane.
    #[default]
    Gpu,
    /// Pin all layers to CPU RAM (`--n-gpu-layers 0`). Zero VRAM contention — for
    /// a measurement lane coexisting with a GPU-resident living lane.
    Cpu,
}

/// One LoRA genome layer to load into the serving catalog at spawn. The `path`
/// is llama.cpp's load identity (what `--lora` takes and what the per-request
/// resolver matches against); the `alias` is ours, for logs and the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterEntry {
    /// Human label for logs/observability (the gene's name).
    pub alias: String,
    /// Absolute path to the GGUF-lora the server loads via `--lora`.
    pub path: PathBuf,
}

impl ServingTarget {
    /// The model id the running server should report — used to decide whether a
    /// relaunch is needed.
    pub fn model_id(&self) -> &str {
        &self.model.id
    }

    /// Lane count to hand llama-server's `--parallel`, floored at 1. The plan
    /// guarantees ≥ 1; the floor is a defensive guard so a zero can never
    /// collapse the KV cache to nothing.
    pub fn parallel_lanes(&self) -> u32 {
        self.lanes.max(1)
    }

    /// The `-c` value: the TOTAL KV cache to request = per-lane `context_window`
    /// × `lanes`. llama-server splits this evenly across the `--parallel` slots,
    /// so each slot ends up with one full `context_window`. Launching `-c
    /// context_window` alone (no `--parallel`) was the prior bug: llama.cpp
    /// defaulted to 4 slots and quartered the per-request window, 500-ing
    /// deliberations whose prompt fit the planned window. `saturating_mul`
    /// guards the (unreachable on real plans) overflow.
    pub fn served_total_ctx(&self) -> u32 {
        self.context_window.saturating_mul(self.parallel_lanes())
    }

    /// The adapter SET as a sorted list of path strings — the identity llama.cpp
    /// loads by and the key [`ensure_model_serving`] compares to decide whether a
    /// genome change requires a relaunch. Sorted so order can't spuriously trip
    /// the diff.
    pub fn adapter_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .adapters
            .iter()
            .map(|a| a.path.to_string_lossy().into_owned())
            .collect();
        paths.sort();
        paths
    }
}

/// Resolve the serving root (`http://host:port`). Deployment shape (which host /
/// port llama-server listens on) is legitimately env-configurable per the
/// concurrency guide's "socket paths / deployment shape" carve-out — unlike
/// substrate *thresholds*, which stay `const`. The value carries no `/v1`; we
/// append `/v1` and `/health` as needed so there is one source of truth.
pub fn serving_root() -> String {
    if let Some(raw) = crate::config_env::read("LLAMA_SERVER_BASE_URL") {
        let trimmed = raw.trim().trim_end_matches('/');
        // Accept a configured `.../v1` and normalize back to the root so callers
        // get a single canonical form regardless of how the operator wrote it.
        // An operator-pinned endpoint is honored VERBATIM — no scan: they chose
        // a fixed address deliberately (possibly a remote server).
        let root = trimmed.strip_suffix("/v1").unwrap_or(trimmed);
        if !root.is_empty() {
            return root.to_string();
        }
    }
    format!("http://{DEFAULT_HOST}:{}", chosen_port())
}

/// The live lane's serving port: the PINNED canonical [`DEFAULT_PORT`]. We do NOT
/// scan for the live lane — pinning is what makes a crashed predecessor's orphan
/// reclaimable (the fresh core looks here, adopts-or-reaps, binds the same port)
/// and keeps the launch args, the `/health` probe, and the published snapshot's
/// `base_url` trivially in agreement. The orphan-reap that frees this port from a
/// SIGKILLed core runs lazily on the live lane's first fresh claim, inside
/// [`LlamaServerControl::serve`] (gated on not already owning a child →
/// [`crate::inference::lane_pidfile::reclaim`]), so a healthy orphan serving the
/// right model is adopted for free upstream before any reap is considered; a
/// foreign squatter we can't reclaim fails the bind loud rather than fleeing to a
/// GPU-competing port. (Ephemeral measurement lanes still scan via
/// [`first_free_port`].)
fn chosen_port() -> u16 {
    DEFAULT_PORT
}

/// Scan `[base, base + PORT_SCAN_WINDOW)` for a port we can bind right now. A
/// successful bind-then-drop proves the port is free; the brief TOCTOU gap until
/// llama-server claims it is absorbed by the readiness poll (a lost race surfaces
/// loudly as `NotReady`, never a silent wrong-port serve). If nothing in the
/// window is free we return `base` and let the spawn fail loud
/// ([[fallbacks-are-illegal-fail-loud]]).
fn first_free_port(base: u16) -> u16 {
    for port in base..base.saturating_add(PORT_SCAN_WINDOW) {
        if std::net::TcpListener::bind((DEFAULT_HOST, port)).is_ok() {
            return port;
        }
    }
    base
}

/// The OpenAI-compatible base url personas' inference adapters point at.
pub fn serving_v1_url() -> String {
    format!("{}/v1", serving_root())
}

/// Path to the `llama-server` binary — the inference engine WE OWN, built from
/// our vendored llama.cpp submodule by `tools/scripts/install-llama-server.sh`
/// into `~/.continuum/bin`. Resolution order:
///   1. `LLAMA_SERVER_BIN` config override (deployment escape hatch),
///   2. our owned install at `~/.continuum/bin/llama-server` (the normal case —
///      the core knows where its own engine lives; no reliance on a launcher
///      munging `PATH`, no borrowing `~/.unsloth`'s build),
///   3. bare `"llama-server"` (let the OS resolve it on `PATH`).
/// We do NOT silently fall back to a different engine — a missing binary
/// surfaces loudly when spawn is attempted ([[fallbacks-are-illegal-fail-loud]]).
fn server_bin() -> String {
    if let Some(over) = crate::config_env::read("LLAMA_SERVER_BIN")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        return over;
    }
    if let Some(home) = std::env::var_os("HOME") {
        let owned = std::path::Path::new(&home)
            .join(".continuum")
            .join("bin")
            .join("llama-server");
        if owned.is_file() {
            return owned.to_string_lossy().into_owned();
        }
    }
    "llama-server".to_string()
}

/// Published serving state. One model, is it ready, on what `/v1` url. The
/// daemon owns the `watch::Sender<ServingSnapshot>`; everything downstream
/// (adapters, operators, a future grid allocator) reads it instead of probing
/// the process directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/ServingSnapshot.ts")]
pub struct ServingSnapshot {
    /// The model id currently being served, if any. `None` = nothing live yet.
    #[ts(optional)]
    pub active_model: Option<String>,
    /// True once `/health` has answered 200 for the active model.
    pub ready: bool,
    /// The `/v1` base url personas point their inference adapter at.
    pub base_url: String,
    /// The LoRA genome layers loaded into the serving catalog (sorted paths).
    /// Empty = base model only. Lets a reader (and the reconcile guard) see WHICH
    /// genome is live without probing the process. `serde(default)` keeps older
    /// persisted snapshots readable.
    #[serde(default)]
    pub adapters: Vec<String>,
    /// The REAL per-slot context window the running server serves, read from its
    /// own `/props` (`default_generation_settings.n_ctx`). This is the
    /// AUTHORITATIVE model metadata personas budget their prompts to. llama.cpp
    /// pads the launch per-slot window (`-c / --parallel`) UP to a 256-multiple
    /// internally, so the planner's window and the served window differ — and the
    /// planner RE-computes its window every tick against live memory, drifting
    /// ABOVE the running server's frozen slot. Budgeting to that drifted value
    /// overflows the slot → llama-server 500 "Compute error". So a persona reads
    /// THIS (the process's own truth), never a recomputed plan value. `0` only on
    /// the empty/not-yet-served snapshot — a `ready` snapshot always carries the
    /// real window (the daemon refuses to publish `ready` without it).
    /// `serde(default)` keeps older persisted snapshots (window-less) readable.
    #[serde(default)]
    pub served_context_window: u32,
    /// The `--parallel` slot count the running server serves — how many personas
    /// can occupy a lane concurrently. llama.cpp allocates one full
    /// `served_context_window` KV window PER slot, so total resident KV scales
    /// with this: `lanes × kv_per_token × served_context_window`. Carried on the
    /// snapshot so a reader (the resource authority's `footprint()`, a grid
    /// allocator sizing concurrency) sees the true residency without probing the
    /// process. `0` only on the empty/not-yet-served snapshot — a `ready`
    /// snapshot always carries the real lane count. `serde(default)` keeps older
    /// persisted snapshots (lane-less) readable.
    #[serde(default)]
    pub lanes: u32,
}

impl ServingSnapshot {
    /// The "nothing served" state — boot, or after a node drops its server.
    pub fn empty() -> Self {
        Self {
            active_model: None,
            ready: false,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            // Nothing served → no real window yet. A `ready` snapshot never
            // carries 0 (the daemon stamps the live `/props` window before
            // publishing ready); 0 is the unambiguous "no window known" sentinel.
            served_context_window: 0,
            // Nothing served → no lanes. A `ready` snapshot always carries the
            // real `--parallel` count; 0 is the "no lanes" sentinel.
            lanes: 0,
        }
    }
}

/// Process-wide readable handle to the daemon's serving state. Set ONCE by
/// [`crate::modules::serving_daemon::ServingDaemonModule`] at init via
/// [`install_serving_state`]. This is the seam the bus reminder points at:
/// consumers READ the latest snapshot as a cheap pointer (a `watch` borrow)
/// instead of each issuing its own HTTP `/v1/models` probe. One source of
/// truth — the daemon's own reconcile — fanned out to every reader, modules
/// and free functions alike.
static SERVING_STATE: OnceLock<watch::Receiver<ServingSnapshot>> = OnceLock::new();

/// Install the daemon's serving-state receiver as the process-wide readable
/// seam. The daemon is a singleton so this is set-once; a second call (e.g. a
/// re-init under test) is ignored. Returns `true` iff this call installed it.
pub fn install_serving_state(rx: watch::Receiver<ServingSnapshot>) -> bool {
    SERVING_STATE.set(rx).is_ok()
}

/// The model currently served on this node, per the daemon's last reconcile.
/// Returns [`ServingSnapshot::empty`] before the daemon installs its state
/// (boot) or when nothing is live. No HTTP, no probe — a `watch` borrow.
/// Callers MUST treat `active_model == None` as "nothing served yet" and fail
/// loud, never substitute a stand-in ([[fallbacks-are-illegal-fail-loud]]).
pub fn current_serving() -> ServingSnapshot {
    SERVING_STATE
        .get()
        .map(|rx| rx.borrow().clone())
        .unwrap_or_else(ServingSnapshot::empty)
}

/// Default bound for an upstart path that needs the served model and can wait
/// for the daemon's first reconcile (persona upstart, embedder build, lease
/// resolve). One source of truth so every consumer waits the same; generous
/// enough to cover a cold relaunch of a large GGUF, bounded so an empty/wedged
/// serving plan fails loud instead of hanging. Hot-path readers that must not
/// block use [`current_serving`] (no wait) instead.
///
/// DERIVED from [`READY_TIMEOUT`] (the spawner's own load budget) + a margin for
/// the reconcile to start (tick cadence) and HTTP slack. The two constants
/// describe the SAME physical event — a cold model becoming ready — so the boot
/// gate must never fail before the spawner has exhausted its legitimate budget.
/// A flat 30s here was the bug: a genuine 31–90s cold load is still within the
/// spawner's window, yet the boot-time inference gate declared failure and
/// hard-killed the core. Fail LOUD, not fail FAST — a premature false failure is
/// worse than a slower true one. Bump READY_TIMEOUT and this follows.
pub const DEFAULT_SERVING_WAIT: Duration = Duration::from_secs(READY_TIMEOUT.as_secs() + 30);

/// The registry id of the local OpenAI-compatible serving gateway this seam
/// fronts — `llama-server` (llama.cpp's `/v1` server, proven live on :58057).
/// ONE source of truth for the id string: the catalog entry, the registration
/// path, the default generate route, and every `from_registry(...)` caller all
/// reference this const so the id is named in exactly one place. (Formerly the
/// scattered `"unsloth"` string; freed of behavior by #55, renamed here.)
pub const PROVIDER_ID: &str = "llama-server";

/// Await a READY served model, bounded by `timeout`. Resolves the instant the
/// daemon's snapshot reports `ready` with an `active_model`. Returns the live
/// snapshot, or `None` on timeout / before the daemon has installed its state.
///
/// This REPLACES the old "HTTP-probe-with-timeout" boot dance: an upstart path
/// waits on the daemon's own readiness signal (the same `watch` it publishes),
/// not a raw port probe that races the reconcile. The sender lives for the
/// process lifetime (the daemon owns it), so the only non-ready resolution is
/// the timeout.
pub async fn await_ready_serving(timeout: Duration) -> Option<ServingSnapshot> {
    let mut rx = SERVING_STATE.get()?.clone();
    {
        // Fast path: already ready, no await.
        let cur = rx.borrow_and_update();
        if cur.ready && cur.active_model.is_some() {
            return Some(cur.clone());
        }
    }
    // Bind the timeout result before matching so its `watch::Ref` temporary
    // drops before `rx` does (else the borrow outlives `rx` — E0597).
    let waited =
        tokio::time::timeout(timeout, rx.wait_for(|s| s.ready && s.active_model.is_some())).await;
    match waited {
        Ok(Ok(guard)) => Some(guard.clone()),
        _ => None,
    }
}

/// Wait until the served window has SETTLED after a serving-mode change that may have triggered
/// a grow-relaunch (e.g. an exam declaring Ludicrous/Performance). The caller must be able to
/// pin the lane WITHOUT bouncing in-flight work, so it must know the relaunch is DONE — not
/// merely that the lane was ready a moment ago (it may be about to relaunch on the next 5s plan
/// tick). Returns the settled per-slot window, or `None` on timeout / no serving state.
///
/// Two phases, watch-driven (no polling, no time-based stability guessing):
///  1. Within `settle_grace` (must exceed the daemon's `TICK`, so a re-plan can fire), watch for
///     the lane to go NOT-ready — the signature of a relaunch. If it never does, no relaunch was
///     needed (the window was already at target) → return the current window immediately.
///  2. A relaunch happened → wait (bounded by `timeout`) for the lane to come back decode-ready
///     at its new window.
pub async fn wait_for_serving_window_settle(
    settle_grace: Duration,
    timeout: Duration,
) -> Option<u32> {
    let mut rx = SERVING_STATE.get()?.clone();
    let start = std::time::Instant::now();
    // Phase 1: did a relaunch start (lane went not-ready) within the grace window?
    let relaunched = tokio::time::timeout(settle_grace, rx.wait_for(|s| !s.ready))
        .await
        .is_ok();
    if !relaunched {
        // No relaunch — the window was already at target. Return it if ready.
        let s = rx.borrow_and_update();
        return s.ready.then_some(s.served_context_window);
    }
    // Phase 2: wait for the relaunched lane to come back decode-ready. Bind the timeout result
    // before matching so its `watch::Ref` temporary drops before `rx` does (else E0597).
    let remaining = timeout.saturating_sub(start.elapsed());
    let waited =
        tokio::time::timeout(remaining, rx.wait_for(|s| s.ready && s.active_model.is_some())).await;
    match waited {
        Ok(Ok(guard)) => Some(guard.served_context_window),
        _ => None,
    }
}

/// Typed serving-control failures. The only string is a display-only leaf
/// detail — never parsed back into control flow ([[protocols-prevent-pain]]).
#[derive(Debug, thiserror::Error)]
pub enum LlamaServerError {
    /// The server isn't answering — not yet spawned, or down. The reconcile
    /// loop treats this as "no active model" and (re)spawns, rather than failing.
    #[error("llama-server unreachable: {0}")]
    Unreachable(String),
    /// Spawn itself failed (binary missing, bad args, OS refused). Loud.
    #[error("llama-server spawn failed: {0}")]
    Spawn(String),
    /// The model GGUF could not be resolved on disk for the requested id.
    #[error("no local GGUF for model '{0}'")]
    ModelNotFound(String),
    /// The server spawned but never became ready within the timeout.
    #[error("llama-server not ready after {0:?}: {1}")]
    NotReady(Duration, String),
    /// A target named a LoRA genome whose GGUF is not on disk. Loud — we never
    /// silently skip a missing `--lora` (that would serve a different genome than
    /// the plan asked for, the wrong kind of degrade)
    /// ([[fallbacks-are-illegal-fail-loud]]).
    #[error("LoRA adapter file not found: {0}")]
    AdapterNotFound(String),
}

/// Outcome of one reconcile pass. The daemon logs/publishes against this; it is
/// exhaustive so a new state can't be silently dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnsureOutcome {
    /// The desired model was already being served — no relaunch.
    AlreadyServing,
    /// We (re)spawned and the server became ready serving `model`.
    Spawned { model: String },
    /// We could not bring the desired model up; `reason` says why (display-only).
    Degraded { reason: String },
}

/// The serving-control seam. `LlamaServerProcess` is the real impl; tests use a
/// fake to exercise the pure reconcile decision without a live process.
#[async_trait]
pub trait LlamaServerControl: Send + Sync {
    /// The model id the running server reports serving, or `None` if nothing is
    /// up. `Unreachable` = no server answering (a normal pre-spawn state).
    async fn active_model(&self) -> Result<Option<String>, LlamaServerError>;

    /// The LoRA genome layers the running server has loaded (sorted paths — the
    /// load identity, matching [`ServingTarget::adapter_paths`]). Empty when
    /// nothing is up or no genes are loaded. [`ensure_model_serving`] compares
    /// this against the desired set to detect a genome change (a new gene
    /// trained, or one retired) that requires a relaunch — page-in/out *within* a
    /// loaded set is per-request scale and never reaches here.
    async fn active_adapters(&self) -> Result<Vec<String>, LlamaServerError>;

    /// (Re)spawn llama-server to serve `target` and block until it is ready.
    /// Switching models is a relaunch — there is no load-by-name API. The
    /// target carries the resolved model AND the host-fit served window, so the
    /// launcher never re-resolves or re-clamps anything.
    async fn serve(&self, target: &ServingTarget) -> Result<(), LlamaServerError>;

    /// The REAL per-slot context window the running server serves, read from
    /// `/props` (`default_generation_settings.n_ctx`). This is the AUTHORITATIVE
    /// model metadata: llama.cpp pads the launch per-slot window (`-c / --parallel`)
    /// UP to a 256-multiple internally, so only the process can report what each
    /// slot will actually accept. The daemon reads this after a ready reconcile and
    /// stamps it onto [`ServingSnapshot::served_context_window`], so every persona
    /// budgets its prompt to the truth instead of the planner's drifting window
    /// (which overflows the slot → 500 "Compute error"). `Unreachable` = no server
    /// answering (pre-spawn), or a ready server whose `/props` shape is missing
    /// `n_ctx` (a loud invariant violation — never a guessed window).
    async fn served_context_window(&self) -> Result<u32, LlamaServerError>;

    /// Prove the GPU DECODE path works, not just that the HTTP server is up. A
    /// llama-server can answer `/health`, `/v1/models` and `/props` with 200
    /// while EVERY `llama_decode` returns 500 "Compute error" — observed live in
    /// a 47-min-old orphan reclaimed from a SIGKILLed core, whose Metal compute
    /// context had gone bad. `/models` proves the listener thread is alive; only
    /// an actual generation proves the model can think. This runs a trivial
    /// 1-token completion and returns `true` iff it succeeds — the readiness
    /// signal `/health` alone cannot give. Any error (unreachable, non-200,
    /// compute error) → `false`: "this lane cannot decode", which the caller
    /// turns into reap + respawn rather than adopting a wedged server.
    async fn decode_smoke_ok(&self) -> bool;

    /// Does THIS control own the running child (we spawned it), versus pointing
    /// at a server some other process launched (an adopted orphan)? The adopt
    /// decision trusts a child we spawned and verified at [`wait_ready`], but
    /// must decode-smoke-probe an orphan we did not — that orphan is exactly the
    /// one that can be compute-wedged. Default `false` (a fake/remote control
    /// owns no local child) so the conservative path (probe before adopt) is the
    /// default.
    fn owns_child(&self) -> bool {
        false
    }
}

/// Pure reconcile decision: bring the running server in line with `desired`.
/// Split from the process impl so the branch logic is unit-tested against a
/// fake. `Unreachable` from the probe means "nothing up" → serve; any other
/// probe error degrades rather than blindly relaunching over a sick server.
pub async fn ensure_model_serving<C: LlamaServerControl + ?Sized>(
    ctrl: &C,
    target: &ServingTarget,
    // Force a decode SMOKE-PROBE even on a child we own, bypassing the "trusted
    // thereafter" short-circuit. The serving daemon's liveness heartbeat sets this after
    // it has already seen the live lane fail the decode probe on a slow cadence
    // (#175): a Metal-OOM-poisoned backend answers `/v1/models` 200 while every decode
    // 500s, so without re-proving decode the owned-child trust would re-adopt the wedged
    // lane forever. `false` is the steady state (no per-tick decode load).
    force_probe: bool,
) -> EnsureOutcome {
    let active = match ctrl.active_model().await {
        Ok(active) => active,
        Err(LlamaServerError::Unreachable(_)) => None,
        Err(other) => {
            return EnsureOutcome::Degraded {
                reason: format!("probe failed: {other}"),
            }
        }
    };

    if active.as_deref() == Some(target.model_id()) {
        // Same base model live. Now check the loaded genome SET: a new gene (or a
        // retired one) is a set change that llama.cpp can only honor by relaunch
        // (no hot-load API). An unreadable adapter probe is treated as "unknown
        // set" → relaunch to be safe, never a silent serve of a stale genome.
        let desired = target.adapter_paths();
        let active_adapters = ctrl.active_adapters().await.unwrap_or_default();
        if active_adapters == desired {
            // Right model, right genome — but does the running server's per-slot
            // WINDOW match the target? llama.cpp cannot hot-resize, so a target
            // window that meaningfully EXCEEDS the served window is a real mismatch
            // only a relaunch can honor — exactly like the adapter-set change below.
            // Without this, a daemon-decided starved grow-back (2048→31k, same model
            // + same genome) short-circuits to AlreadyServing and the lane stays
            // frozen at the boot floor FOREVER, while the daemon re-decides "starved"
            // and logs "re-homing" every tick — a relaunch that never happens
            // (glass-boxed 2026-07-20: window pinned 2048 vs a 31k plan, benchmarks
            // blocked). Grow-only: a down-plan is kept by the daemon's sticky window
            // and never reaches here; a `/props` read failure (served 0/Err) is
            // treated as "window OK" so a probe error never spuriously relaunches.
            let window_ok = match ctrl.served_context_window().await {
                Ok(served) => {
                    served == 0
                        || target.context_window
                            <= served.saturating_add(WINDOW_RELAUNCH_TOLERANCE)
                }
                Err(_) => true,
            };
            if !window_ok {
                crate::probe!(
                    class = "serving.window_grow",
                    model = target.model_id(),
                    target_window = target.context_window,
                    "served window is below the target beyond padding tolerance — \
                     relaunching to grow (llama.cpp has no hot-resize; a genome-set \
                     match alone must not strand a starved lane at the boot floor)",
                );
                // fall through to relaunch at the larger window.
            } else {
                // Window matches. Is the COMPUTE path alive? A child we spawned
                // ourselves was decode-verified at `wait_ready` and is trusted
                // thereafter (no per-tick decode load). A server we did NOT spawn (an
                // orphan reclaimed from a dead core) can answer `/v1/models` while
                // every `llama_decode` 500s — so prove decode before adopting it. A
                // wedged orphan fails the probe and falls through to `serve()`, which
                // reaps it and spawns fresh. Trust an owned child without a per-tick
                // decode probe — UNLESS `force_probe` is set, in which case the
                // liveness heartbeat flagged it wedged and we must re-prove decode
                // before re-adopting (#175). A non-owned orphan is always probed.
                if (ctrl.owns_child() && !force_probe) || ctrl.decode_smoke_ok().await {
                    return EnsureOutcome::AlreadyServing;
                }
                crate::probe!(
                    class = "serving.adopt_rejected",
                    model = target.model_id(),
                    owned = ctrl.owns_child(),
                    force_probe,
                    "lane answers /v1/models but fails the decode smoke-probe (compute-wedged \
                     — a poisoned Metal backend / OOM, #175); reaping + respawning a fresh lane",
                );
                // fall through to relaunch.
            }
        }
        // else: genome set differs → fall through to relaunch.
    }

    match ctrl.serve(target).await {
        Ok(()) => EnsureOutcome::Spawned {
            model: target.model_id().to_string(),
        },
        Err(reason) => EnsureOutcome::Degraded {
            reason: reason.to_string(),
        },
    }
}

/// Owns the supervised `llama-server` child. One per host. `serve` kills any
/// prior child before launching the new model, so there is never more than one
/// llama-server competing for the GPU. `Drop` kills the child — the core owning
/// the launch means the core owns the teardown.
pub struct LlamaServerProcess {
    root: String,
    v1_url: String,
    bin: String,
    client: reqwest::Client,
    /// The live child, if one is running. `std::sync::Mutex` (not tokio) because
    /// it is held only for the brief swap/kill, never across an await.
    child: Arc<StdMutex<Option<tokio::process::Child>>>,
    /// The LoRA genome set (sorted paths) the CURRENT child was launched with —
    /// the truthful record of what `/lora-adapters` holds, since llama.cpp has no
    /// API to query it. `ensure_model_serving` compares this against the desired
    /// set to decide a relaunch. Reset on each `serve()`. After a core restart
    /// with a persisting server this reads empty → one safe extra relaunch
    /// re-establishes the genome (core owns the lifecycle, so this is truthful in
    /// the steady state).
    served_adapters: Arc<StdMutex<Vec<String>>>,
    /// True for THE host's live persona lane (pins the canonical port, writes the
    /// reclaim pidfile, reaps a crashed predecessor's orphan). False for an
    /// [`EphemeralServingLane`]'s `with_root` process, which owns its own scanned
    /// port and must NEVER write the canonical pidfile or reclaim the live port —
    /// it would reap the living persona's own server. This boolean is the seam
    /// that keeps the reclaim machinery bound to exactly one process.
    is_live_lane: bool,
}

impl LlamaServerProcess {
    pub fn new() -> Self {
        Self::with_client(reqwest::Client::new())
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self {
            root: serving_root(),
            v1_url: serving_v1_url(),
            bin: server_bin(),
            client,
            child: Arc::new(StdMutex::new(None)),
            served_adapters: Arc::new(StdMutex::new(Vec::new())),
            // THE host's live lane — pins the canonical port, owns the reclaim
            // pidfile. `new()`/`with_client()` are the live constructors.
            is_live_lane: true,
        }
    }

    /// Build a process bound to an EXPLICIT `root` (`http://host:port`) instead of
    /// the global [`serving_root`]. This is the seam that lets a SECOND
    /// llama-server run on its own port without colliding with the live lane or
    /// publishing to the global serving snapshot (`serve()` is already
    /// snapshot-free — only [`crate::modules::serving_daemon`] publishes the
    /// global state). [`EphemeralServingLane`] is the only caller; the `v1_url` is
    /// derived from `root` so a persona's inference adapter can point straight at
    /// this lane.
    pub fn with_root(root: String) -> Self {
        let v1_url = format!("{}/v1", root.trim_end_matches('/'));
        Self {
            root,
            v1_url,
            bin: server_bin(),
            client: reqwest::Client::new(),
            child: Arc::new(StdMutex::new(None)),
            served_adapters: Arc::new(StdMutex::new(Vec::new())),
            // Ephemeral lane on its OWN scanned port: NOT the canonical live lane.
            // It must never write the canonical pidfile or reclaim the live port.
            is_live_lane: false,
        }
    }

    /// Kill the currently-running child, if any. Idempotent.
    fn kill_child(&self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            // start_kill is non-blocking; the OS reaps. We're replacing it, so
            // we don't await the exit — the new spawn binds the same port once
            // the old one releases it (readiness poll absorbs the gap).
            let _ = child.start_kill();
        }
    }

    /// Poll `/health` until the server answers 200, THEN prove the compute path
    /// with a one-token decode — only then is the lane truly ready. 503 means
    /// "still loading the model" — keep waiting. A connection error means "not up
    /// yet" early in the launch — also keep waiting, until the deadline. The
    /// decode check makes "ready" mean "can think", not merely "HTTP listener is
    /// up": a freshly-spawned server that answers `/health` 200 but cannot decode
    /// is a loud, novel failure surfaced as `NotReady` (→ Degraded) rather than a
    /// silently-adopted wedged lane. (This is the spawn-side guarantee that lets
    /// the adopt path in `ensure_model_serving` TRUST a child we own without
    /// re-probing every reconcile tick.)
    async fn wait_ready(&self) -> Result<(), LlamaServerError> {
        let health = format!("{}/health", self.root);
        let deadline = Instant::now() + READY_TIMEOUT;
        let mut last = String::from("no response");
        loop {
            match self.client.get(&health).timeout(PROBE_TIMEOUT).send().await {
                Ok(resp) if resp.status().is_success() => {
                    // HTTP listener is up and the model is loaded. Confirm the GPU
                    // decode path before declaring ready — a 200 /health does not
                    // prove `llama_decode` works (the wedged-orphan failure mode).
                    if self.decode_smoke_ok().await {
                        return Ok(());
                    }
                    last = String::from("/health 200 but decode smoke-probe failed");
                }
                Ok(resp) => last = format!("status {}", resp.status()),
                Err(e) => last = e.to_string(),
            }
            if Instant::now() >= deadline {
                return Err(LlamaServerError::NotReady(READY_TIMEOUT, last));
            }
            tokio::time::sleep(READY_POLL).await;
        }
    }
}

impl Default for LlamaServerProcess {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LlamaServerProcess {
    fn drop(&mut self) {
        // Did we OWN the child we're about to kill? Capture the pid + ownership
        // before `kill_child` takes it. Ownership is the difference between a core
        // that SPAWNED its server and one that ADOPTED a persisting one
        // (`AlreadyServing` → never spawned → no child). Only the spawner kills the
        // server on the way out.
        let owned_pid = self.child.lock().unwrap().as_ref().and_then(|c| c.id());
        let owned_child = owned_pid.is_some();
        self.kill_child();
        // Remove this lane's registry record — for BOTH live and ephemeral — since
        // we just killed the process it named. A crash skips Drop, which is exactly
        // the case the record survives for (the next boot's `sweep_orphans` reaps
        // it). Only remove what we OWNED; an adopted persisting server keeps its
        // record so the successor can reclaim it.
        if let Some(pid) = owned_pid {
            crate::inference::lane_registry::remove(pid);
        }
        // Clear the reclaim pidfile ONLY if we owned the child we just killed — the
        // server is now dead, so there is nothing for the next boot to reclaim. If
        // we ADOPTED a persisting server (no child of our own), it keeps running
        // after we exit, so we LEAVE the pidfile naming it so the successor can
        // reclaim that still-live server if it later goes sick. (A SIGKILL/crash
        // skips Drop entirely — that is exactly the case the pidfile survives for,
        // and the next boot's identity-verified reclaim handles it.)
        if self.is_live_lane && owned_child {
            crate::inference::lane_pidfile::clear();
        }
    }
}

/// An ephemeral, self-contained llama-server lane on its OWN port — the
/// serving-LEASE primitive in embryo: stand up capacity for a
/// `(base model, adapters, context window)` on demand, use it, release it.
///
/// The live persona lane (the global [`serving_root`] snapshot) is never
/// touched: this spawns a SECOND server on a free port scanned up from a
/// caller-chosen base, and the child dies when the lane is dropped (inherited
/// from [`LlamaServerProcess`]'s `Drop`). That isolation is exactly what
/// [`crate::cognition::eval`]'s genome A/B needs — the humane-eval invariant
/// (#59) is "measure a copy, never degrade the living persona", and you cannot
/// measure a gene against its forged base if doing so re-homes the model the
/// living persona is currently thinking with.
///
/// Generalized, this is the same lease the grid uses to place demand-driven
/// capacity across heterogeneous nodes: today the host is `localhost` and the
/// budget check is one machine's free VRAM (the `ResourceGovernor`'s job, #56);
/// tomorrow the host is a peer and the budget spans the interlinked grid. One
/// abstraction, misfit toys.
pub struct EphemeralServingLane {
    proc: LlamaServerProcess,
    port: u16,
}

impl EphemeralServingLane {
    /// Stand up a fresh llama-server for `target` on a free port scanned up from
    /// `base_port` (distinct from the live lane). Fails loud
    /// ([[fallbacks-are-illegal-fail-loud]]) if no port binds, the model GGUF is
    /// missing, or any `--lora` gene file is absent — never serves a substitute.
    /// Budget-gating ("does this fit alongside the live lane?") is the
    /// `ResourceGovernor`'s concern (#56); this primitive only stands the lane up.
    pub async fn spawn(target: &ServingTarget, base_port: u16) -> Result<Self, LlamaServerError> {
        let port = first_free_port(base_port);
        let root = format!("http://{}:{}", DEFAULT_HOST, port);
        let proc = LlamaServerProcess::with_root(root);
        // HARD wall-clock cap on the WHOLE bring-up. `wait_ready`'s READY_TIMEOUT only
        // bounds the /health poll, and its deadline is checked BETWEEN attempts — so a
        // hang INSIDE an attempt (a stalled `decode_smoke_ok`, a wedged model-mmap, a
        // process-launch that never returns) can wait past it indefinitely. Glass-boxed
        // 2026-07-19: an ephemeral eval lane hung 11 min with no process and free VRAM,
        // no timeout ever firing — a silent glacial wedge that violated fail-loud. This
        // net guarantees the eval fails LOUD after the daemon's own load budget instead.
        // On timeout `proc` drops → its `Drop` kills any child it launched. Eval lanes
        // only; the live lane keeps its own (fail-loud-not-fast) bring-up policy.
        match tokio::time::timeout(DEFAULT_SERVING_WAIT, proc.serve(target)).await {
            Ok(res) => res?,
            Err(_) => {
                return Err(LlamaServerError::NotReady(
                    DEFAULT_SERVING_WAIT,
                    "ephemeral eval lane bring-up exceeded the wall-clock budget \
                     (launch / port-bind / model-mmap / health-probe hang)"
                        .to_string(),
                ));
            }
        }
        Ok(Self { proc, port })
    }

    /// The OpenAI-compatible `/v1` base url a persona's inference adapter points
    /// at to route deliberation through THIS lane instead of the live one.
    pub fn v1_url(&self) -> String {
        self.proc.v1_url.clone()
    }

    /// `http://host:port` of this lane (no `/v1`).
    pub fn root(&self) -> &str {
        &self.proc.root
    }

    /// The bound port — for probes/telemetry that want to name the lane.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The REAL per-slot window this lane serves, read from its own `/props`
    /// (`default_generation_settings.n_ctx`) — the SAME served-truth source the
    /// live daemon pins the resident persona's window to
    /// (`supervisor.rs`: `profile.context_length = snap.served_context_window`).
    /// An eval fork's cognition is sized to THIS, never the planned launch `-c`,
    /// so a measurement copy plans against exactly the window it is served — no
    /// planned-vs-served drift between training and the lane it runs on
    /// ([[dreaming-mind-eval-must-match-live-cognition]], task #50). Fails loud if
    /// the server is up but `/props` is unreadable — never a guessed window.
    pub async fn served_context_window(&self) -> Result<u32, LlamaServerError> {
        self.proc.served_context_window().await
    }
}

#[async_trait]
impl LlamaServerControl for LlamaServerProcess {
    async fn active_model(&self) -> Result<Option<String>, LlamaServerError> {
        // `/v1/models` reports the id we launched with via `--alias`, so the
        // comparison in `ensure_model_serving` is exact. A connection error
        // means nothing is up (the normal pre-spawn state) → Unreachable.
        let url = format!("{}/models", self.v1_url);
        let resp = self
            .client
            .get(&url)
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(LlamaServerError::Unreachable(format!(
                "status {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        let id = body
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|arr| arr.first())
            .and_then(|m| m.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Ok(id)
    }

    async fn active_adapters(&self) -> Result<Vec<String>, LlamaServerError> {
        // The set we launched the current child with. llama.cpp exposes no query
        // for `/lora-adapters` contents, so the truthful answer is what WE loaded.
        Ok(self.served_adapters.lock().unwrap().clone())
    }

    async fn served_context_window(&self) -> Result<u32, LlamaServerError> {
        // `/props` lives at the root (not under `/v1`), alongside `/health`. The
        // per-slot window the server actually serves is
        // `default_generation_settings.n_ctx` — the launch `-c / --parallel`
        // per-slot value AFTER llama.cpp's internal 256-multiple padding. A
        // connection error means nothing is up (normal pre-spawn) → Unreachable.
        let url = format!("{}/props", self.root);
        let resp = self
            .client
            .get(&url)
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(LlamaServerError::Unreachable(format!(
                "status {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        // Fail loud if the shape is missing the field — never guess a window. The
        // daemon turns this into "publish the gap" (no ready snapshot), so a
        // malformed /props degrades to not-ready and self-heals next tick rather
        // than poisoning every persona's prompt budget.
        body.get("default_generation_settings")
            .and_then(|s| s.get("n_ctx"))
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .ok_or_else(|| {
                LlamaServerError::Unreachable(
                    "/props missing default_generation_settings.n_ctx (server up but shape \
                     unexpected — refusing to guess the served window)"
                        .to_string(),
                )
            })
    }

    async fn decode_smoke_ok(&self) -> bool {
        // A REAL multi-token generation, not just HTTP 200. A wedged lane (the
        // "~2 tokens then stop for EVERY request" failure mode, observed intermittently
        // on fresh ephemeral eval lanes) still answers a `max_tokens: 1` request with
        // 200 — so a status-only probe passes it, and every downstream task then decodes
        // ~2 tokens and silently scores 0. The probe now forces a prompt a healthy model
        // MUST answer with many tokens and asserts the completion actually produced
        // several (`usage.completion_tokens >= MIN_SMOKE_DECODE_TOKENS`). That is the
        // difference between "the server binds" and "the decode path truly generates".
        // A 500 "Compute error" (the wedged-orphan signature) still fails fast on status.
        // The `--alias` id is what the server answers to; reuse the live v1 url.
        let url = format!("{}/chat/completions", self.v1_url);
        let body = serde_json::json!({
            "messages": [{ "role": "user", "content": "Count from 1 to 20, separated by spaces." }],
            "max_tokens": 48,
            "stream": false,
            "temperature": 0.0,
        });
        let resp = match self
            .client
            .post(&url)
            .timeout(DECODE_SMOKE_TIMEOUT)
            .json(&body)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => resp,
            _ => return false,
        };
        // Read the completion-token count; a wedged lane yields ~2, a healthy one 20+.
        let Ok(v) = resp.json::<serde_json::Value>().await else {
            return false;
        };
        let out_tokens = v
            .get("usage")
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        out_tokens >= MIN_SMOKE_DECODE_TOKENS
    }

    fn owns_child(&self) -> bool {
        self.child.lock().unwrap().is_some()
    }

    async fn serve(&self, target: &ServingTarget) -> Result<(), LlamaServerError> {
        // Resolve the GGUF from the model struct already in hand — no re-fetch by
        // id. No file → fail loud; we never serve a substitute model
        // ([[fallbacks-are-illegal-fail-loud]]).
        let gguf: PathBuf = crate::model_registry::artifacts::resolve_gguf_for_model(&target.model)
            .ok_or_else(|| LlamaServerError::ModelNotFound(target.model.id.clone()))?;

        // Validate every gene file BEFORE killing the live server — a missing
        // `--lora` is a loud failure, and we must not tear down a healthy server
        // to launch one that will reject its own args. Fail loud, named cause.
        for adapter in &target.adapters {
            if !adapter.path.is_file() {
                return Err(LlamaServerError::AdapterNotFound(format!(
                    "{} ({})",
                    adapter.path.display(),
                    adapter.alias
                )));
            }
        }

        let (host, port) = split_host_port(&self.root);

        // One server at a time: kill the old child before binding the port.
        // Whether we already OWN a child decides if a stale-orphan reap is needed:
        // a relaunch (we own one) frees the port via `kill_child` and must NOT pay
        // the reclaim wait; a fresh claim (we own none) is where a crashed
        // predecessor's orphan may still hold the canonical port.
        let had_own_child = self.child.lock().unwrap().is_some();
        self.kill_child();

        // Fresh claim on the live lane: reap a crashed predecessor's orphaned
        // llama-server if its pidfile still names one holding our port, so the
        // bind below succeeds instead of failing loud against it. This is reached
        // ONLY when reconcile decided NOT to adopt (wrong model/genome, or the
        // port is unreachable) — a HEALTHY orphan serving the right model is
        // adopted for free upstream (`ensure_model_serving` → `AlreadyServing`,
        // zero reload) and never reaches `serve`. The reap is identity-verified
        // (never a reused pid) and a no-op when there is nothing to reclaim; if a
        // FOREIGN squatter we can't reclaim holds the port, the spawn's bind fails
        // loud rather than fleeing to a GPU-competing port
        // ([[fallbacks-are-illegal-fail-loud]]).
        if self.is_live_lane && !had_own_child {
            let outcome = crate::inference::lane_pidfile::reclaim(port).await;
            crate::probe!(
                class = "serving.lane_reclaim",
                port = port,
                outcome = format!("{outcome:?}").as_str(),
                "fresh-claim reclaim of canonical serving port before spawn",
            );
        }

        // llama-server's `-c` is the TOTAL KV cache, split evenly across the
        // `--parallel` slots; each request only sees `-c / n_parallel` tokens.
        // The plan's `context_window` is PER-LANE, so the total we must request
        // is `context_window * lanes` — then each of `lanes` slots holds exactly
        // one planned window. We pass `--parallel` EXPLICITLY (never inherit
        // llama.cpp's default, which is 4 and silently quartered the window in
        // the prior bug). See `served_total_ctx` / `parallel_lanes`.
        let lanes = target.parallel_lanes();
        let total_ctx = target.served_total_ctx();
        let mut cmd = tokio::process::Command::new(&self.bin);
        cmd.arg("-m")
            .arg(&gguf)
            .arg("--alias")
            .arg(&target.model.id)
            .arg("--host")
            .arg(host)
            .arg("--port")
            .arg(port.to_string())
            // Total KV = per-lane window × lanes; split back to one full window
            // per slot by `--parallel` below. The plan budgeted this exact total
            // (`kv_at(context_window) * lanes`) against the host, so it fits.
            .arg("-c")
            .arg(total_ctx.to_string())
            .arg("--parallel")
            .arg(lanes.to_string())
            // KV PREFIX REUSE across a persona's turns. `cache_prompt:true` (sent
            // per-request) only reuses a slot's prior content when the *exact*
            // prefix still sits in that slot; with the volatile grounding tail
            // changing every turn and embedding requests sharing these same slots,
            // measured cross-turn reuse was ZERO (`cachedTokens: 0` over every
            // captured live turn, forcing a full re-prefill of the ~720-token
            // static identity/doctrine/tool prefix each turn). `--cache-reuse`
            // lets llama.cpp reuse cached chunks ≥ N tokens via KV shifting even
            // when a later span differs — so the stable prefix is kept, not
            // recomputed. 256 is the llama.cpp-recommended min chunk. This is a
            // pure optimization flag: absent it we just re-prefill (correct, slow);
            // present it we reuse (correct, fast) — no fallback, no behavior change.
            .arg("--cache-reuse")
            .arg("256")
            // PREFILL THROUGHPUT (#139). Live personas are prefill-bound: a real turn
            // re-prefills ~4k tokens of fresh RAG context at ~109 tok/s → 30–110s turns
            // (decode is tiny and fast; the mind is NOT slow, the re-read is). The
            // physical micro-batch (`--ubatch-size`, llama.cpp default 512) is how many
            // prompt tokens Metal processes per compute pass — bigger batch = more
            // parallel prefill = higher tok/s, traded against a larger per-slot compute
            // buffer. 1024 doubles prefill parallelism; the compute-buffer growth is the
            // same axis that OOMs (kIOGPUCommandBufferCallbackErrorOutOfMemory) so it is
            // sized WITH the 2-lane headroom, not blindly. Measured knob: watch prefill
            // tok/s in the captures and back off if the lane 500s "Compute error".
            .arg("--ubatch-size")
            .arg("1024")
            // Overflow must FAIL, never silently amputate. With context shift on
            // (the llama.cpp default), a prompt larger than the slot's window has
            // its MIDDLE evicted and generation proceeds on the mutilated prompt —
            // exam-corrupting amnesia no log line reports (#139: 44k-token prompts
            // observed riding ~13.4k slots with no error anywhere). Disabled, the
            // server 400s ("exceeds context size") and the caller's fail-loud path
            // surfaces the real defect: a RAG budget that overshot the served
            // window ([[fallbacks-are-illegal-fail-loud]]).
            .arg("--no-context-shift");
        // MULTIMODAL PROJECTOR (#106): a vision/audio-capable model needs its mmproj GGUF so
        // llama-server loads the vision (or audio) encoder and can tokenize image/audio content
        // parts. Present → the model actually SEES (the `ContentPart::Image` the persona render
        // seam attaches gets mtmd-encoded). Absent on a Vision-capable row → the server serves
        // TEXT only and silently ignores images, which is a capability LIE — so warn LOUD rather
        // than fabricate sight ([[fallbacks-are-illegal-fail-loud]]). Safe on a generation lane
        // (unlike `--embeddings` below): the projector only adds the encoder, it does not switch
        // the server out of causal-generation mode.
        if let Some(mmproj) = crate::model_registry::artifacts::resolve_mmproj_for_model(&target.model)
        {
            cmd.arg("--mmproj").arg(&mmproj);
        } else if target
            .model
            .capabilities
            .contains(&crate::model_registry::Capability::Vision)
        {
            tracing::warn!(
                probe_class = "serving.vision.no_mmproj",
                model = %target.model.id,
                declared = ?target.model.mmproj_local_path,
                "vision-capable model has no resolvable mmproj projector — serving TEXT-ONLY; \
                 image parts will be silently ignored. Fetch the mmproj GGUF (see the model row's \
                 mmproj_local_path) or drop the Vision capability so the row stops claiming sight."
            );
        }
        // `--embeddings` is deliberately NOT set on this GENERATION lane. On the
        // current llama.cpp build it puts the server in embedding (non-causal)
        // mode, which makes generation fail with `500 "Compute error."` on EVERY
        // request — every persona turn went dark (and OAI /v1/embeddings still
        // 400s with "pooling type 'none'", so it wasn't even serving embeddings
        // correctly). One server cannot serve both causal generation and
        // non-causal embeddings. Verified 2026-07-03: the base GGUF generates
        // cleanly the instant this flag is removed. llama-server-hosted
        // embeddings need their OWN lane (`--embeddings --pooling mean/last` on a
        // separate port) — a follow-up; the live embedding path today is the
        // fastembed/ONNX provider, unaffected by this lane.
        // Placement: CPU lanes pin every layer to RAM so they never contend for
        // the GPU VRAM a living lane already holds (the Metal decode-time OOM that
        // muted the eval). GPU lanes omit the flag — llama-server offloads all it
        // can by default. [[ServingTarget::placement]] / #59 / #56.
        if target.placement == LanePlacement::Cpu {
            cmd.arg("--n-gpu-layers").arg("0");
        }
        // Native tool-calling needs the model's TOOL-CAPABLE chat template. The
        // mlx→gguf conversion can strip the embedded template down to a bare
        // ChatML loop (no `<tools>`/`<tool_call>` rendering) — which silently
        // disables native function-calling, so the gateway ignores the `tools`
        // param and the persona's hands go dead (verified live 2026-06-26: the
        // forged GGUF carried a 208-char template, zero tool support). When the
        // forge writes a `chat_template.jinja` sidecar next to the GGUF, hand it
        // to llama-server with --jinja so it renders tools and does
        // grammar-constrained native tool calls — VALID tool-call JSON guaranteed
        // by the sampler, not hand-escaped by a 4B model into a JSON string (the
        // failure that made multi-line code calls unparseable). This is an
        // explicit override file, not a silent fallback: present → it's the
        // truth the GGUF should have carried; absent → the embedded template
        // stands.
        // --jinja is UNCONDITIONAL: it makes llama-server render the `tools` we send and do
        // grammar-constrained parsing of the model's NATIVE tool-call format, using the
        // model's OWN chat template. That is the tool-trained shape a Qwen/Hermes/etc GGUF
        // expects — infinitely more reliable than us reverse-engineering tool calls out of
        // prose after the fact. A normal pulled GGUF carries a tool-capable embedded template;
        // this switch was previously gated on a forge sidecar existing, so pulled models
        // silently ran with tools DISABLED (the gateway ignored the `tools` param → the
        // persona narrated tool calls instead of emitting them). The sidecar, when the forge
        // wrote one, now OVERRIDES the embedded template (for forged GGUFs that shipped a
        // thin, tool-less 208-char template) — present → override, absent → the embedded
        // tool-capable template stands, tools ON either way.
        cmd.arg("--jinja");
        if let Some(tpl) = gguf
            .parent()
            .map(|d| d.join("chat_template.jinja"))
            .filter(|p| p.is_file())
        {
            cmd.arg("--chat-template-file").arg(tpl);
        }
        // Load each trained genome layer into the `/lora-adapters` catalog at
        // index order; the per-request `"lora":[{id,scale}]` field pages them in.
        for adapter in &target.adapters {
            cmd.arg("--lora").arg(&adapter.path);
        }
        // Capture the server's stderr to a per-port log file (#175). llama.cpp prints
        // its load banner AND — critically — the underlying ggml/Metal fault behind a
        // `{"code":500,"message":"Compute error"}` HTTP reply to stderr. The prior
        // `Stdio::null()` threw that root cause away, so a compute-error storm was
        // undiagnosable from outside the process (glass-boxed 2026-07-15: every request
        // 500'd in ~7ms and the WHY went to /dev/null). Fail-soft: if the log can't be
        // opened, fall back to null and say so — an unreadable log must never block
        // serving (same posture as the pidfile below).
        // [[never-blind-feedback-driven-iteration]] [[self-test-via-command-feedback-surface-never-blind]]
        let stderr_stdio = dirs::home_dir()
            .map(|h| h.join(".continuum").join("logs"))
            .and_then(|dir| {
                std::fs::create_dir_all(&dir).ok()?;
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dir.join(format!("llama-server-{}.log", port)))
                    .ok()
            })
            .map(Stdio::from)
            .unwrap_or_else(|| {
                tracing::warn!(
                    probe_class = "serving.llama.stderr_unlogged",
                    port = port,
                    "could not open llama-server stderr log — falling back to null (#175)"
                );
                Stdio::null()
            });
        let child = cmd
            .stdout(Stdio::null())
            .stderr(stderr_stdio)
            .spawn()
            .map_err(|e| LlamaServerError::Spawn(format!("{}: {e}", self.bin)))?;

        // Record the child pid so a SIGKILLed core's successor can reclaim THIS
        // port instead of fleeing to a GPU-competing one. Live lane only — the
        // ephemeral lane owns its own scanned port and must not touch the canonical
        // pidfile. A write failure DISARMS future reclaim but must NOT fail the
        // serve: the server is up and serving; the pidfile is a recovery aid, not a
        // serving precondition. Surface it loud (probe) without aborting.
        if self.is_live_lane {
            match child.id() {
                Some(pid) => {
                    if let Err(e) = crate::inference::lane_pidfile::write(pid) {
                        crate::probe!(
                            class = "serving.lane_pidfile",
                            port = port,
                            error = e.to_string().as_str(),
                            "lane pidfile write failed — orphan reclaim disarmed for this run",
                        );
                    }
                }
                None => crate::probe!(
                    class = "serving.lane_pidfile",
                    port = port,
                    "spawned child has no pid — orphan reclaim disarmed for this run",
                ),
            }
        }

        // Register EVERY lane we spawn (live AND ephemeral) in the lane registry so
        // a crashed core's successor reaps this exact process instead of leaving a
        // ~6 GB orphan. `lane_pidfile` above only covers the live lane's canonical
        // port; the registry is what closes the ephemeral-lane leak. Same as the
        // pidfile, a write failure only DISARMS future reclaim for this lane — it
        // never fails the serve.
        if let Some(pid) = child.id() {
            let role = if self.is_live_lane {
                crate::inference::lane_registry::LaneRole::Live
            } else {
                crate::inference::lane_registry::LaneRole::Ephemeral
            };
            let rec = crate::inference::lane_registry::LaneRecord {
                pid,
                port,
                role,
                model: target.model_id().to_string(),
            };
            if let Err(e) = crate::inference::lane_registry::record(&rec) {
                crate::probe!(
                    class = "serving.lane_registry",
                    port = port,
                    error = e.to_string().as_str(),
                    "lane registry write failed — orphan reclaim disarmed for this lane",
                );
            }
        }

        *self.child.lock().unwrap() = Some(child);
        // Remember the genome set this child was launched with — the truthful
        // catalog record for the relaunch decision (llama.cpp can't report it).
        *self.served_adapters.lock().unwrap() = target.adapter_paths();

        self.wait_ready().await
    }
}

/// Split `http://host:port` into (`host`, `port`). Falls back to the defaults
/// for any part we can't parse — deployment shape, not a substrate threshold.
fn split_host_port(root: &str) -> (String, u16) {
    let no_scheme = root
        .strip_prefix("http://")
        .or_else(|| root.strip_prefix("https://"))
        .unwrap_or(root);
    let authority = no_scheme.split('/').next().unwrap_or(no_scheme);
    match authority.rsplit_once(':') {
        Some((h, p)) => (
            if h.is_empty() { DEFAULT_HOST.to_string() } else { h.to_string() },
            p.parse().unwrap_or(DEFAULT_PORT),
        ),
        None => (authority.to_string(), DEFAULT_PORT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Fake control: scripted probe result + a counter of serve() calls, so the
    /// pure reconcile decision is tested without a live process.
    struct FakeControl {
        probe: Result<Option<String>, &'static str>,
        /// The genome set the fake reports as currently loaded (sorted paths).
        active_adapters: Vec<String>,
        serve_ok: bool,
        serves: AtomicUsize,
        /// Whether the fake's decode smoke-probe succeeds. `true` = a healthy lane
        /// (adoptable); `false` = a compute-wedged orphan (must be rejected).
        decode_ok: bool,
        /// Whether the fake "owns" the running child (we spawned it). `false` =
        /// an adopted orphan (the conservative default that exercises the
        /// smoke-probe gate).
        owns: bool,
        /// The per-slot window the fake's `/props` reports. Defaults to the tests'
        /// target window so the window-grow relaunch check (which only fires when
        /// target > served + tolerance) is a no-op for the model/adapter/decode
        /// tests; a grow-relaunch test sets it BELOW the target explicitly.
        served_window: u32,
    }

    impl FakeControl {
        fn probe(probe: Result<Option<String>, &'static str>) -> Self {
            Self {
                probe,
                active_adapters: Vec::new(),
                serve_ok: true,
                serves: AtomicUsize::new(0),
                decode_ok: true,
                owns: false,
                served_window: 32768,
            }
        }
        /// Model a lane whose live per-slot window is SMALLER than the plan target —
        /// the starved boot-floor case the window-grow relaunch must catch.
        fn with_served_window(mut self, n: u32) -> Self {
            self.served_window = n;
            self
        }
        fn serve_fails(mut self) -> Self {
            self.serve_ok = false;
            self
        }
        /// Report a genome set as already loaded (the live server's catalog).
        fn with_active_adapters(mut self, adapters: Vec<String>) -> Self {
            self.active_adapters = adapters;
            self
        }
        /// Model the compute-wedged orphan: answers `/v1/models` but every decode
        /// 500s.
        fn decode_wedged(mut self) -> Self {
            self.decode_ok = false;
            self
        }
        /// Model a child we spawned ourselves (trusted without a per-tick probe).
        fn owned(mut self) -> Self {
            self.owns = true;
            self
        }
    }

    #[async_trait]
    impl LlamaServerControl for FakeControl {
        async fn active_model(&self) -> Result<Option<String>, LlamaServerError> {
            match &self.probe {
                Ok(v) => Ok(v.clone()),
                Err(m) => Err(LlamaServerError::Unreachable(m.to_string())),
            }
        }
        async fn active_adapters(&self) -> Result<Vec<String>, LlamaServerError> {
            Ok(self.active_adapters.clone())
        }
        async fn serve(&self, _target: &ServingTarget) -> Result<(), LlamaServerError> {
            self.serves.fetch_add(1, Ordering::SeqCst);
            if self.serve_ok {
                Ok(())
            } else {
                Err(LlamaServerError::Spawn("boom".into()))
            }
        }
        async fn served_context_window(&self) -> Result<u32, LlamaServerError> {
            // The per-slot window the fake's /props reports (configurable via
            // `with_served_window`). Non-zero so a ready outcome publishes a ready
            // snapshot; defaults to the target window so the window-grow relaunch
            // check is a no-op unless a test deliberately floors it.
            Ok(self.served_window)
        }
        async fn decode_smoke_ok(&self) -> bool {
            self.decode_ok
        }
        fn owns_child(&self) -> bool {
            self.owns
        }
    }

    /// Minimal serving target for the reconcile-logic tests: only `model.id` is
    /// load-bearing here (it drives the relaunch decision); the rest is filler so
    /// the grouped `Model` constructs. The real launcher path is covered by the
    /// live serving daemon, not these pure-decision tests.
    fn target(id: &str) -> ServingTarget {
        use crate::model_registry::types::{Arch, MultiPartyChatStrategy};
        ServingTarget {
            model: Model {
                id: id.to_string(),
                name: None,
                provider: "llamacpp-local".to_string(),
                arch: Arch::Qwen2,
                context_window: 32768,
                max_output_tokens: 4096,
                tokens_per_second: 0.0,
                capabilities: std::collections::BTreeSet::new(),
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
            },
            context_window: 32768,
            lanes: 1,
            adapters: Vec::new(),
            placement: LanePlacement::Gpu,
        }
    }

    /// Same as [`target`] but with a genome set loaded — for the relaunch-on-
    /// genome-change tests. Paths are the load identity the reconcile compares.
    fn target_with_adapters(id: &str, paths: &[&str]) -> ServingTarget {
        let mut t = target(id);
        t.adapters = paths
            .iter()
            .map(|p| AdapterEntry {
                alias: format!("gene-{p}"),
                path: PathBuf::from(p),
            })
            .collect();
        t
    }

    // what this catches: the per-lane → total `-c` arithmetic. llama-server's
    // `-c` is the WHOLE KV cache split across `--parallel` slots, so to give each
    // of `lanes` slots a full per-lane `context_window` the spawn must request
    // `context_window * lanes` and pass `--parallel lanes`. Launching `-c
    // context_window` alone let llama.cpp default to 4 slots and quarter the
    // per-request window, 500-ing live deliberations ("Context size exceeded").
    // Regression here silently shrinks every persona's real prompt budget.
    #[test]
    fn served_total_ctx_is_per_lane_window_times_lanes() {
        let mut t = target("coder-4b");
        t.context_window = 16_384;
        t.lanes = 4;
        assert_eq!(t.parallel_lanes(), 4);
        assert_eq!(
            t.served_total_ctx(),
            65_536,
            "-c must be per-lane window × lanes so each slot holds one full window"
        );

        // lanes floored at 1: a single-lane plan serves the full window to one slot.
        t.lanes = 1;
        assert_eq!(t.served_total_ctx(), 16_384);

        // defensive floor: a zero lane count never collapses the cache to nothing.
        t.lanes = 0;
        assert_eq!(t.parallel_lanes(), 1);
        assert_eq!(t.served_total_ctx(), 16_384);
    }

    // what this catches: the desired model already being served is a NO-OP — we
    // must not relaunch (which would kill the GPU-warm server). Regression here
    // would thrash the served model every tick.
    #[tokio::test]
    async fn already_serving_does_not_relaunch() {
        let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())));
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
        assert_eq!(outcome, EnsureOutcome::AlreadyServing);
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 0, "no relaunch when already serving");
    }

    // what this catches: a same-model / same-genome lane whose live per-slot window
    // is STARVED far below the plan target (the 2048 boot-floor vs a 31k plan) must
    // RELAUNCH to grow — llama.cpp has no hot-resize. Regression here (returning
    // AlreadyServing on a genome-set match alone) strands the lane at the boot floor
    // forever while the daemon logs "re-homing starved" every tick and never grows
    // it — glass-boxed 2026-07-20, blocked all benchmark runs (prompts overflowed
    // 2048). The daemon's `starved` gate only sends a target ≥ 2× the served window,
    // so the grow is unambiguous.
    #[tokio::test]
    async fn starved_window_relaunches_to_grow() {
        // Owned + decode-healthy: the ONLY thing wrong is the window (2048 ≪ 32768).
        let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
            .owned()
            .with_served_window(2048);
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
        assert!(
            matches!(outcome, EnsureOutcome::Spawned { .. }),
            "a starved window must relaunch to grow, got {outcome:?}"
        );
        assert_eq!(
            ctrl.serves.load(Ordering::SeqCst),
            1,
            "exactly one relaunch to the larger window"
        );
    }

    // what this catches: the window-grow check must NOT fire on llama.cpp's 256-pad
    // (served slightly BELOW target after internal rounding is normal) — only a real
    // step-change grow relaunches. A served window one pad under target stays
    // AlreadyServing; regression would relaunch every tick on padding noise.
    #[tokio::test]
    async fn padded_window_within_tolerance_does_not_relaunch() {
        let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
            .owned()
            .with_served_window(32768 - 256); // one 256-pad under the 32768 target
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
        assert_eq!(outcome, EnsureOutcome::AlreadyServing);
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 0, "padding noise must not relaunch");
    }

    // what this catches: an orphan that answers /v1/models with the RIGHT model
    // but is COMPUTE-WEDGED (every llama_decode 500s "Compute error" — observed
    // live in a 47-min-old orphan reclaimed from a SIGKILLed core). Trusting
    // /models alone re-adopted the wedged lane every 5s tick and BOTH personas
    // abstained forever. The decode smoke-probe must reject it and fall through
    // to serve() → reap + respawn fresh. Regression here silently re-adopts a
    // brain that cannot think.
    #[tokio::test]
    async fn wedged_orphan_rejected_and_respawned() {
        let ctrl = FakeControl::probe(Ok(Some("coder-14b".into()))).decode_wedged();
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
        assert_eq!(
            outcome,
            EnsureOutcome::Spawned { model: "coder-14b".into() },
            "a compute-wedged orphan must be rejected and respawned, never adopted"
        );
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 1, "wedged orphan → fresh spawn");
    }

    // what this catches: #175 self-heal. A child WE OWN that is compute-wedged (decode
    // probe fails while /v1/models still 200s — the Metal-GPU-OOM poison) is normally
    // TRUSTED forever ("decode-verified at wait_ready, trusted thereafter, no per-tick
    // decode load"). Once the serving daemon's liveness heartbeat has flagged it,
    // `force_probe=true` makes `ensure_model_serving` re-prove decode EVEN on an owned
    // child, so the wedged lane is reaped + respawned instead of re-adopted. Regression
    // here re-bricks the persona substrate on any transient GPU OOM. regression for #175
    #[tokio::test]
    async fn force_probe_relaunches_an_owned_but_wedged_lane() {
        let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
            .owned()
            .decode_wedged();
        // Baseline (the blindness we fix): an owned child is trusted despite the wedge.
        let trusted = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
        assert_eq!(
            trusted,
            EnsureOutcome::AlreadyServing,
            "an owned child is trusted without force_probe (the pre-#175 blindness)"
        );
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 0, "no relaunch without force");
        // With force_probe: re-prove decode → fails → reap + respawn a fresh backend.
        let healed = ensure_model_serving(&ctrl, &target("coder-14b"), true).await;
        assert_eq!(
            healed,
            EnsureOutcome::Spawned { model: "coder-14b".into() },
            "force_probe relaunches an owned wedged lane — the #175 self-heal"
        );
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 1, "exactly one respawn");
    }

    // what this catches: a child WE spawned (owns_child) is decode-verified once
    // at wait_ready and trusted thereafter — the adopt path must NOT run a decode
    // smoke-probe against it every 5s tick (that would burn a GPU decode per tick
    // and contend with the personas' real inference slots). owns_child
    // short-circuits the probe: even a `decode_wedged` fake adopts, proving the
    // probe was skipped for an owned child.
    #[tokio::test]
    async fn owned_child_adopted_without_smoke_probe() {
        let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
            .owned()
            .decode_wedged();
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
        assert_eq!(
            outcome,
            EnsureOutcome::AlreadyServing,
            "our own decode-verified child is trusted without a per-tick re-probe"
        );
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 0, "no relaunch for an owned child");
    }

    // what this catches: a different model live → relaunch to the desired one.
    #[tokio::test]
    async fn wrong_model_triggers_relaunch() {
        let ctrl = FakeControl::probe(Ok(Some("general-4b".into())));
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
        assert_eq!(outcome, EnsureOutcome::Spawned { model: "coder-14b".into() });
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 1);
    }

    // what this catches: SAME model + SAME genome set live → no relaunch. The
    // adapter-set comparison must not spuriously relaunch a correctly-genomed
    // server (which would kill the GPU-warm catalog every tick).
    #[tokio::test]
    async fn same_model_same_genome_does_not_relaunch() {
        let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
            .with_active_adapters(vec!["/genes/a.gguf".into(), "/genes/b.gguf".into()]);
        // Desired set equal but given out of order — adapter_paths() sorts, so it matches.
        let outcome =
            ensure_model_serving(&ctrl, &target_with_adapters("coder-14b", &["/genes/b.gguf", "/genes/a.gguf"]), false)
                .await;
        assert_eq!(outcome, EnsureOutcome::AlreadyServing);
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 0, "no relaunch when genome set matches");
    }

    // what this catches: SAME model but a NEW gene trained (set grew) → relaunch.
    // This is the wire that closes the genome learning loop — without the adapter
    // comparison a freshly-trained gene with an unchanged model id would return
    // AlreadyServing and never load into the catalog.
    #[tokio::test]
    async fn new_gene_triggers_relaunch_on_same_model() {
        let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
            .with_active_adapters(vec!["/genes/a.gguf".into()]);
        let outcome =
            ensure_model_serving(&ctrl, &target_with_adapters("coder-14b", &["/genes/a.gguf", "/genes/b.gguf"]), false)
                .await;
        assert_eq!(outcome, EnsureOutcome::Spawned { model: "coder-14b".into() });
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 1, "new gene must relaunch to repopulate the catalog");
    }

    // what this catches: nothing running (Unreachable) is NOT an error — it's
    // the normal pre-spawn state, so we serve, not degrade.
    #[tokio::test]
    async fn unreachable_means_spawn_not_degrade() {
        let ctrl = FakeControl::probe(Err("connection refused"));
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
        assert_eq!(outcome, EnsureOutcome::Spawned { model: "coder-14b".into() });
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 1);
    }

    // what this catches: no model live and serve() fails → Degraded with the
    // named reason, NEVER a panic on the daemon tick (degrade-never-panic).
    #[tokio::test]
    async fn serve_failure_degrades_with_reason() {
        let ctrl = FakeControl::probe(Ok(None)).serve_fails();
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
        match outcome {
            EnsureOutcome::Degraded { reason } => assert!(reason.contains("boom"), "reason: {reason}"),
            other => panic!("expected Degraded, got {other:?}"),
        }
    }

    // what this catches: host:port parsing for the launch args, including the
    // /v1-stripped root and a missing port falling back to the default.
    #[test]
    fn split_host_port_parses_root() {
        assert_eq!(split_host_port("http://127.0.0.1:58057"), ("127.0.0.1".into(), 58057));
        assert_eq!(split_host_port("http://0.0.0.0:9090"), ("0.0.0.0".into(), 9090));
        assert_eq!(split_host_port("http://localhost"), ("localhost".into(), DEFAULT_PORT));
    }

    // what this catches: serving_root normalizes a configured `.../v1` back to
    // the bare root so /health and /v1 are derived from one source of truth.
    // (No env set in test → default; the strip logic is exercised by the unit
    // below feeding the same normalization the reader uses.)
    #[test]
    fn root_normalization_strips_v1() {
        // Mirror the normalization serving_root applies to a configured value.
        let normalize = |raw: &str| {
            let trimmed = raw.trim().trim_end_matches('/');
            trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
        };
        assert_eq!(normalize("http://h:1/v1"), "http://h:1");
        assert_eq!(normalize("http://h:1/v1/"), "http://h:1");
        assert_eq!(normalize("http://h:1"), "http://h:1");
    }

    // what this catches: first_free_port must SKIP a port already bound and
    // return a different, bindable one — the whole point of "scan, never assume
    // free". A regression to "return base unconditionally" would wedge the bind
    // whenever the preferred port is taken (the Studio-squats-58057 case).
    #[test]
    fn first_free_port_skips_a_taken_port() {
        // Bind an ephemeral port so we KNOW one is taken, then scan from it.
        let taken = std::net::TcpListener::bind((DEFAULT_HOST, 0)).expect("bind ephemeral");
        let taken_port = taken.local_addr().unwrap().port();
        let chosen = first_free_port(taken_port);
        assert_ne!(chosen, taken_port, "must not return the held port");
        // And the returned port must itself be bindable right now.
        assert!(
            std::net::TcpListener::bind((DEFAULT_HOST, chosen)).is_ok(),
            "scanned port {chosen} should be free"
        );
    }

    // what this catches: an ephemeral lane must derive its `/v1` url from the
    // EXPLICIT root it was handed (not the global serving_root), and must not
    // double up the scheme or the `/v1` segment — the same `/v1/v1` class of bug
    // that muted Asha. `with_root` is the seam EphemeralServingLane builds on.
    #[test]
    fn with_root_derives_v1_url_from_explicit_root() {
        let proc = LlamaServerProcess::with_root("http://127.0.0.1:59123".to_string());
        assert_eq!(proc.root, "http://127.0.0.1:59123");
        assert_eq!(proc.v1_url, "http://127.0.0.1:59123/v1");
        // a trailing slash on the root must not yield `//v1`.
        let proc = LlamaServerProcess::with_root("http://127.0.0.1:59123/".to_string());
        assert_eq!(proc.v1_url, "http://127.0.0.1:59123/v1");
    }

    // what this catches: THE seam that binds the orphan-reclaim machinery to
    // exactly one process. The live constructors (`new`/`with_client`) must mark
    // `is_live_lane` so they pin the canonical port + own the reclaim pidfile;
    // `with_root` (the EphemeralServingLane seam) must NOT — an ephemeral lane that
    // wrote the canonical pidfile or reaped the live port would kill the living
    // persona's own server. A regression flipping either default reintroduces the
    // two-servers-on-one-GPU OOM (live not pinning) or self-reaping (ephemeral
    // claiming the live role).
    #[test]
    fn live_constructors_are_live_lane_ephemeral_is_not() {
        assert!(
            LlamaServerProcess::new().is_live_lane,
            "new() is THE live lane — must pin the canonical port + own the pidfile"
        );
        assert!(
            LlamaServerProcess::with_client(reqwest::Client::new()).is_live_lane,
            "with_client() is a live constructor"
        );
        assert!(
            !LlamaServerProcess::with_root("http://127.0.0.1:59123".to_string()).is_live_lane,
            "with_root() is ephemeral — must never write the canonical pidfile or reclaim the live port"
        );
    }

    // what this catches: the live lane PINS the canonical port — it must never
    // scan away from DEFAULT_PORT. Scanning past a held canonical port was the bug
    // (spawned a GPU competitor AND blinded the daemon's own probe). The pin is
    // what makes a crashed predecessor's orphan reclaimable on a known port. The
    // ephemeral scan (first_free_port) is tested separately above and stays.
    #[test]
    fn live_lane_pins_the_canonical_port() {
        assert_eq!(
            chosen_port(),
            DEFAULT_PORT,
            "the live lane must bind the canonical port, never a scanned one"
        );
    }

    // what this catches: the readiness gate `await_ready_serving` waits on must
    // require BOTH `ready` AND an `active_model` — a regression to `ready` alone
    // (or `ready || ...`) would resolve a lease against a `None` model and bind
    // a persona to nothing. Exercised on a LOCAL watch so it doesn't mutate the
    // process-global SERVING_STATE (set-once → would race other tests).
    #[tokio::test]
    async fn ready_predicate_requires_ready_and_a_model() {
        let pred = |s: &ServingSnapshot| s.ready && s.active_model.is_some();
        let (tx, mut rx) = watch::channel(ServingSnapshot::empty());

        // empty: not ready, no model → unsatisfied.
        assert!(!pred(&rx.borrow()));
        // ready but no model → STILL unsatisfied (the bug we guard against).
        tx.send_replace(ServingSnapshot {
            active_model: None,
            ready: true,
            base_url: "x".into(),
            adapters: Vec::new(),
            served_context_window: 0,
            lanes: 0,
        });
        assert!(!pred(&rx.borrow()));
        // not-ready but has a model → unsatisfied.
        tx.send_replace(ServingSnapshot {
            active_model: Some("coder".into()),
            ready: false,
            base_url: "x".into(),
            adapters: Vec::new(),
            served_context_window: 0,
            lanes: 0,
        });
        assert!(!pred(&rx.borrow()));
        // ready AND a model → satisfied, and wait_for resolves to it at once.
        tx.send_replace(ServingSnapshot {
            active_model: Some("coder".into()),
            ready: true,
            base_url: "x".into(),
            adapters: Vec::new(),
            served_context_window: 0,
            lanes: 0,
        });
        let got = tokio::time::timeout(Duration::from_millis(100), rx.wait_for(pred))
            .await
            .expect("should not time out")
            .expect("sender live");
        assert_eq!(got.active_model.as_deref(), Some("coder"));
    }

    // what this catches: a WEDGED server (port held open, never answers HTTP — a
    // GPU hang, OOM-thrash, deadlock, or a SIGSTOP'd orphan from a crashed
    // predecessor) must not hang `active_model` forever. Without the per-request
    // PROBE_TIMEOUT the probe blocks indefinitely: the TCP connect succeeds against
    // the held socket, then the read never returns, so `ensure_model_serving` never
    // returns and the reconcile that would reclaim the sick lane never runs — the
    // exact silent-hang that wedged a successor core. The bound converts "wedged"
    // into a fast `Unreachable`, which serve()+reclaim acts on. We assert it
    // returns (Err) well under an unbounded "forever" — if the timeout regressed to
    // unbounded this test would hang and the runner would kill it. Regression for
    // the unbounded control-plane probe (#90 reliability follow-up).
    #[tokio::test]
    async fn wedged_server_probe_times_out_instead_of_hanging() {
        use tokio::io::AsyncReadExt;
        use tokio::net::TcpListener;

        // A listener that ACCEPTS then holds the socket silent — the precise wedge a
        // frozen llama-server produces (connect ok, read never answered).
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let _holder = tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                // Drain quietly and never reply; hold the conn open past the probe.
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });

        let proc = LlamaServerProcess::with_root(format!("http://127.0.0.1:{port}"));
        let started = Instant::now();
        let result = proc.active_model().await;
        let elapsed = started.elapsed();

        assert!(
            matches!(result, Err(LlamaServerError::Unreachable(_))),
            "wedged server must surface as Unreachable, got {result:?}"
        );
        assert!(
            elapsed < PROBE_TIMEOUT + Duration::from_secs(4),
            "probe must be bounded by PROBE_TIMEOUT, took {elapsed:?}"
        );
    }
}
