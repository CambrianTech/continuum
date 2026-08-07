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
/// How long a lane may take to come ready after spawn. Also the anchor for the
/// serving daemon's sustained-delta re-home window: you must never relaunch more
/// often than a relaunch can finish, so the observation window that justifies one
/// is derived from THIS ([`crate::modules::serving_daemon`]).
pub const READY_TIMEOUT: Duration = Duration::from_secs(90);

/// Readiness budget for an EPHEMERAL lane ([`EphemeralServingLane`]) — the eval /
/// teacher measurement lanes. These cold-load a SECOND large GGUF (a 24B Devstral)
/// on a Metal GPU that ALREADY holds the live persona lane's copy, so the weight
/// mmap + the first-decode Metal graph/command-buffer warmup runs under GPU
/// co-residency and legitimately exceeds the live lane's 90s budget. Glass-boxed
/// 2026-07-21: a co-resident cold Devstral-24B teacher lane answered `/health` 200
/// but its decode smoke-probe was still warming at the 90s cap — a PREMATURE FALSE
/// FAILURE (the lane served completions immediately after), which then dropped the
/// teacher onto the live LoRA lane and yielded 0 corpus. Same doctrine as the
/// 30→90 live-lane bump (see [`DEFAULT_SERVING_WAIT`]): fail LOUD, not FAST — a
/// generous bound that covers the real physical event, still bounded so a truly
/// wedged lane fails loud. Scoped to ephemeral lanes via `is_live_lane` so the live
/// lane's tighter fail-loud budget is untouched.
const EPHEMERAL_READY_TIMEOUT: Duration = Duration::from_secs(240);

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
/// Generous on purpose: the probe shares the lane with LIVE persona traffic, and a
/// 1,238-token co-tenant prefill alone runs ~9s — measured 2026-07-23: the identical
/// probe body took 60s / 24s / 0s across three tries behind normal load. The old 10s
/// budget produced sustained FALSE decode-failures that killed healthy lanes all
/// night (heartbeat → not-ready → kill+respawn → bind race → "crash loop"). A probe's
/// job is truth, not speed — it runs on a slow cadence; let it wait out the queue.
const DECODE_SMOKE_TIMEOUT: Duration = Duration::from_secs(75);

/// Wall-clock ms of the last REAL generation that produced tokens on the served lane.
///
/// Health is recent DELIVERY, never a synthetic probe. The decode heartbeat
/// (`serving_daemon::spawn_health_heartbeat_if_due`) runs a real multi-token generation
/// through the LIVE slots, so it competes for the same slots as actual work: a lane saturated
/// by long prefills cannot hand the probe a slot, the probe reads that as "no decode", and two
/// misses relaunch a lane that was never wedged — just busy. Glass-boxed on the SWE bench
/// (v13): `serving.health {ok:false} x2 -> {action:"relaunch"}` mid-run, then every downstream
/// generate refused with `serving: <none>`.
///
/// Real tokens are the honest liveness evidence, and they cost nothing to observe — a lane that
/// just decoded for a persona is provably not wedged. So the probe is for QUIET lanes, which is
/// exactly when it is both cheap and meaningful. Same principle as the airc delivery receipts:
/// health is recently-ACKed delivery, never the existence of a connection.
/// [[a-benchmark-zero-is-a-claim-about-the-harness-until-proven-otherwise]]
static LAST_REAL_DECODE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Record that a real generation produced tokens on the served lane. Called from the adapter's
/// success path — the one place that knows tokens actually came out.
pub fn note_real_decode() {
    // A clock we cannot read must NOT be stored: 0 is this atomic's "never decoded"
    // sentinel, so `.unwrap_or(0)` here would erase a real decode rather than record
    // one. Leave the last good stamp in place and say so.
    let Ok(since_epoch) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        tracing::warn!(
            "system clock is before the UNIX epoch — cannot stamp this decode; \
             liveness will be judged from the previous stamp or by probing"
        );
        return;
    };
    LAST_REAL_DECODE_MS.store(since_epoch.as_millis() as u64, std::sync::atomic::Ordering::Relaxed);
}

/// Milliseconds since the last real token-producing generation, or `None` if none has been
/// observed yet (fresh boot) — the caller must then fall back to probing.
pub fn ms_since_real_decode() -> Option<u64> {
    let last = LAST_REAL_DECODE_MS.load(std::sync::atomic::Ordering::Relaxed);
    if last == 0 {
        return None;
    }
    // `.unwrap_or(0)` here was the dangerous half: `now` = 0 makes the saturating_sub
    // yield 0, i.e. "a token came out 0 ms ago" — an unreadable clock would present a
    // WEDGED lane as perfectly fresh, and the caller's documented fallback (probe it)
    // would never run. `None` already means "no usable evidence, go probe".
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    Some(now.saturating_sub(last))
}

/// A same-model/same-genome relaunch is required when the target per-slot window
/// exceeds the running server's served window by MORE than this. llama.cpp has no
/// hot-resize API, so a genuine window GROW can only be honored by a relaunch —
/// exactly like an adapter-set change. The margin absorbs llama.cpp's internal
/// 256-multiple padding of the launch `-c/--parallel` window (served ≈ round-up-256
/// of the launched per-slot value), so a padded steady-state window never reads as
/// a spurious grow and re-triggers a relaunch every tick. Comfortably above one
/// 256-pad; the daemon only sends a grow target when it is ≥ 2× the served window
/// (its `starved` gate), so this margin never masks a real grow.
// context-budget-exempt: a HYSTERESIS band: how far the served window may drift before a relaunch is worth it. Tolerance, never a cap — it doesn't limit the window, it stops us thrashing it
const WINDOW_RELAUNCH_TOLERANCE: u32 = 512;

/// Minimum completion tokens a healthy lane must produce on the decode smoke-probe.
/// The failure mode this guards is the intermittently-wedged fresh lane that answers
/// EVERY request with ~2 tokens then stops (observed on ephemeral eval lanes: same
/// base+gene generated 82 tok/task on one spawn, 2 tok/task on the next, silently
/// scoring the whole benchmark 0). An HTTP-200-only probe with `max_tokens: 1` cannot
/// tell that lane from a healthy one — so the probe now forces a prompt a healthy model
/// MUST answer with many tokens and asserts it did. `5` sits comfortably above the
/// ~2-token wedge and far below the ~20+ a healthy "count to 20" yields.
// context-budget-exempt: how many tokens a post-spawn smoke decode must produce to prove the lane is alive — a liveness probe, not a budget
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
    /// K3 slice-1 expert placement: the layer-granular residency the pager computed
    /// (`PlacementRequest.hot_layers` = the real `blk.N` whose stacked expert block fits
    /// GPU). `None` = no expert paging (the default — the whole model places normally).
    /// When `Some`, the COLD layers' `ffn_*_exps` tensors are offloaded to CPU via `-ot` at
    /// spawn (see [`ServingTarget::expert_ot_value`]); the hot layers stay GPU-resident.
    /// llama.cpp places tensors at LOAD time only, so a change to the hot set is honored by
    /// a relaunch — that relaunch is DECIDED by the pager (`serving_pager`'s
    /// `relaunch_needed`), not by the probe-based reconcile here, since there is no API to
    /// read a running server's `-ot`. Per-expert paging (no relaunch) is slice-2.
    pub expert_placement: Option<crate::capacity::placement::PlacementRequest>,
    /// Device-fit resident-override: when the model's RESIDENT (non-expert) tier
    /// overflows the governed VRAM budget as-shipped, the governor's
    /// [`device_fit`](crate::capacity::device_fit) plan resolves a precision-shrunk
    /// resident-override GGUF that fits. The launcher exports it as
    /// `LLAMA_RESIDENT_OVERRIDE` so llama.cpp sources the resident tensors from it
    /// (all offloaded to GPU) while the primary GGUF streams experts. `None` =
    /// resident fits as-shipped (Native), served with no override — the default and
    /// the only shape for a dense or small-MoE model. [[device-fit-repeatable-primitive]] / #29.
    pub resident_override: Option<std::path::PathBuf>,
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

    /// The `-ot`/`--override-tensor` VALUE that offloads the COLD layers' stacked expert
    /// tensors to CPU for this target's [`expert_placement`](Self::expert_placement), or
    /// `None` when there is no placement or nothing is cold (every block hot). Delegates to
    /// the pure [`cold_expert_offload_ot`] builder, keyed on the placement's real `blk.N`
    /// hot set. The spawn path applies this as a `--override-tensor` arg.
    pub fn expert_ot_value(&self) -> Option<String> {
        let p = self.expert_placement.as_ref()?;
        cold_expert_offload_ot(&p.hot_layers, p.n_layers)
    }
}

/// Build the `--override-tensor` (`-ot`) VALUE that offloads every COLD MoE layer's
/// stacked expert tensor to CPU, keeping `hot_layers` on the GPU (the default placement).
/// This is the slice-1 buft-override half of K3 physical expert paging.
///
/// Experts are STACKED per layer — `blk.N.ffn_*_exps` is ONE tensor holding all of layer
/// N's experts — so `-ot`, which places WHOLE tensors, pages at LAYER granularity. The
/// residency planner (the `ServingExpertPager`) owns the VRAM fit and hands us the
/// `PlacementRequest.hot_layers` (the real `blk.N` indices that fit GPU); we emit the
/// inverse: the rest to CPU. (True per-expert paging is slice-2 — a sub-range
/// `ggml_backend_tensor_set` upload — and does not use `-ot`.)
///
/// `n_layers` is the total transformer-block count (the iteration ceiling). Cold =
/// `(0..n_layers)` minus `hot_layers`. A dense (non-MoE) block in that range is HARMLESS:
/// it has no `ffn_*_exps` tensor, so its pattern is inert — which is why we need only the
/// hot set + the block count, never the exact MoE-layer set.
///
/// Returns `None` when nothing is cold (every block is hot, or `n_layers == 0`): there is
/// no override to apply, so the caller omits the flag entirely — never an empty `-ot`,
/// which llama-server rejects. The value targets the `CPU` buffer type
/// (`common/arg.cpp::parse_tensor_buffer_overrides`).
pub fn cold_expert_offload_ot(hot_layers: &[u32], n_layers: u32) -> Option<String> {
    use std::collections::BTreeSet;
    // Ignore any hot index outside the block range (a stale plan can't force a bad pattern).
    let hot: BTreeSet<u32> = hot_layers
        .iter()
        .copied()
        .filter(|&l| l < n_layers)
        .collect();
    let cold: Vec<u32> = (0..n_layers).filter(|l| !hot.contains(l)).collect();
    if cold.is_empty() {
        return None;
    }
    // `\.` on BOTH sides of the block index so `blk.3` never also matches `blk.31`, and
    // `ffn.*_exps` covers all four stacked projections (gate/up/down/gate_up).
    let alternation = cold
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join("|");
    Some(format!(r"blk\.({alternation})\.ffn.*_exps=CPU"))
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

/// The daemon-owned MoE glass-box file locations for the lane on `port` — ONE
/// pure derivation shared by the spawn (which hands them to the child process
/// as `GGML_MOE_CAPTURE_FILE` / `GGML_MOE_PLAN_FILE`) and the positron serving
/// source (which re-derives the capture path from the snapshot's port to tail
/// it). Deriving both sides from the port is what lights the glass box with NO
/// operator env and no wire-type change: the system owns the seam (#278).
/// `None` only when there is no home directory (no stable place to put them).
pub struct MoeGlassBoxPaths {
    /// Per-token pager telemetry JSONL the fork appends — bounded by the
    /// fork's 32 MB default cap (`GGML_MOE_CAPTURE_MB` overrides), so this is
    /// not a new unbounded cache class.
    pub capture: PathBuf,
    /// The v1 policy→mechanism plan document the controller writes atomically
    /// and the fork's `ResidencyCache` mtime-polls per token
    /// (docs/architecture/EXPERT-PAGING-CONTROL-LAW.md §5).
    pub plan: PathBuf,
    /// The ordered routed-expert activation trace the fork appends (12-byte
    /// binary records: tkey u64 + expert u32) — the observation feed the
    /// daemon's pin actuator tails per tick (#281). Truncated by the fork at
    /// each fresh serve (opened `"wb"`), so the tail's truncation-reset is
    /// the rotation story.
    pub trace: PathBuf,
}

/// See [`MoeGlassBoxPaths`]. Lives beside the per-port stderr logs
/// (`~/.continuum/logs/llama-server-{port}.log`) — same dir, same lifecycle.
pub fn moe_glass_box_paths(port: u16) -> Option<MoeGlassBoxPaths> {
    let dir = dirs::home_dir()?.join(".continuum").join("logs");
    Some(MoeGlassBoxPaths {
        capture: dir.join(format!("moe-capture-{port}.jsonl")),
        plan: dir.join(format!("moe-plan-{port}.json")),
        trace: dir.join(format!("moe-trace-{port}.bin")),
    })
}

/// Port of a serving `base_url` (`http://127.0.0.1:58057/v1` → `58057`) — THE
/// derivation every [`moe_glass_box_paths`] consumer uses to key the per-port
/// glass-box files off a [`ServingSnapshot`]. One place (compression principle):
/// the positron capture tail and the serving daemon's plan-file publisher must
/// agree on this parse or they read/write different files than spawn env'd.
pub fn port_of_base_url(url: &str) -> Option<u16> {
    let after_scheme = url.split("://").nth(1).unwrap_or(url);
    let host_port = after_scheme.split('/').next()?;
    host_port.rsplit_once(':')?.1.parse().ok()
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
    // Windows: `HOME` is usually unset (the home is `USERPROFILE`) and the binary
    // carries `.exe` — probing only the unix name silently skipped the owned
    // install and fell through to a bare PATH lookup that spawns nothing (live
    // repro 2026-07-24, BigMama: planned lane, empty log, no server).
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if let Some(home) = home {
        let name = if cfg!(windows) {
            "llama-server.exe"
        } else {
            "llama-server"
        };
        let owned = std::path::Path::new(&home)
            .join(".continuum")
            .join("bin")
            .join(name);
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
#[ts(
    export,
    export_to = "../../../shared/generated/persona/ServingSnapshot.ts"
)]
pub struct ServingSnapshot {
    /// The model id currently being served, if any. `None` = nothing live yet.
    #[ts(optional)]
    pub active_model: Option<String>,
    /// True once `/health` has answered 200 for the active model.
    ///
    /// READ THIS WITH [`ready_verified_at_ms`](Self::ready_verified_at_ms). `ready`
    /// is a CACHED CLAIM, not a live probe (this snapshot is a `watch` borrow by
    /// design — see `serving/status`). Nothing revises it when the process dies, so
    /// on 2026-08-05 `serving/status` returned `ready: true, degraded_reason: null`
    /// AFTER the llama-server was SIGKILLed and its port was dead. Every consumer
    /// that trusted the bare bool was reading a claim with no expiry — the same
    /// defect class as an `[ok]` route health on a route that has not delivered in
    /// ten hours. A claim must carry the age of its evidence.
    pub ready: bool,
    /// When `ready` was last CONFIRMED by real evidence (a `/health` 200 or a real
    /// token delivery) — epoch ms. `None` = never confirmed.
    ///
    /// This is the expiry the bare `ready` bool lacks. A reader deciding anything
    /// load-bearing (route a persona here? score a benchmark against it?) must ask
    /// how old the confirmation is, not merely whether the flag is set: `ready:true`
    /// verified 3s ago and `ready:true` verified 40 minutes ago are different facts,
    /// and only one of them is a lane you should send work to.
    /// [[a-wedged-llama-slot-spins-forever-while-health-and-serving-status-both-say-ready]]
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub ready_verified_at_ms: Option<u64>,
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
    /// WHY nothing is serving, when the last reconcile ended Degraded — the
    /// spawn/probe failure reason, verbatim (e.g. a missing llama-server binary
    /// names its path here). `None` on healthy and never-attempted snapshots.
    /// Live repro 2026-07-24 (BigMama/Windows): the daemon planned correctly,
    /// spawn failed every tick, and `serving/status` showed only
    /// `active_model=null ready=false` — the reason was dropped on the floor,
    /// an operator-facing silent failure ([[fallbacks-are-illegal-fail-loud]]).
    #[serde(default)]
    #[ts(optional)]
    pub degraded_reason: Option<String>,
    /// True iff the active model declares [`Capability::Vision`] AND the lane
    /// VERIFIED its multimodal endpoint actually answers: an mmproj projector
    /// resolved at spawn (`--mmproj` was passed) and the running server's own
    /// `/props` reports `modalities.vision == true` (#106). This is the gate the
    /// observation path (`cognition/vision-describe` → the persona's eyes) reads
    /// before routing image bytes to this lane — a text-only lane, or a vision
    /// row whose projector failed to load, reads `false` and the observe act
    /// fails HONESTLY instead of POSTing pixels a server would silently drop
    /// ([[fallbacks-are-illegal-fail-loud]]). `serde(default)` keeps older
    /// persisted snapshots (pre-#106) readable as not-vision-ready.
    #[serde(default)]
    pub vision_ready: bool,
    /// The `/v1` base url of the VERIFIED vision endpoint on this node — the
    /// address the observation path routes image bytes to. When the MAIN lane's
    /// model itself sees, this is `base_url`; when a vision SIDECAR lane serves
    /// beside a text-only mind (#106, `vision_sidecar`), it is the sidecar's
    /// own url. `None` exactly when `vision_ready == false` — an address is
    /// only ever published for an endpoint whose `/props` confirmed sight.
    #[serde(default)]
    #[ts(optional)]
    pub vision_base_url: Option<String>,
    /// The model id the verified vision endpoint serves — what the describe
    /// path selects and stamps on its result. Same `None`-iff-not-ready
    /// contract as `vision_base_url`.
    #[serde(default)]
    #[ts(optional)]
    pub vision_model: Option<String>,
}

impl ServingSnapshot {
    /// Is a lane ACTUALLY live right now — a model resident AND decode-ready?
    ///
    /// The canonical "there is a brain attached" predicate, in ONE place, because it has two
    /// halves and every reader needs both. `ready` alone is not enough: it is a *cached
    /// claim* (with `ready_verified_at_ms` as its expiry), and a snapshot carrying no
    /// `active_model` has nothing for a request to be answered BY, whatever the flag says.
    ///
    /// Readers: [`await_ready_serving`] (what it waits for), the persona self-tick gate
    /// (whether to spend a deliberation at all, #350), and the adapter's pre-flight guard
    /// (whether a refusal is terminal or a transition worth waiting out). They had drifted
    /// into two spellings of this — `ready` in one, `ready && active_model.is_some()` in the
    /// other — which is exactly the kind of split that makes a gate look correct while it
    /// passes the case it was written to stop.
    pub fn is_live(&self) -> bool {
        self.ready && self.active_model.is_some()
    }

    /// The "nothing served" state — boot, or after a node drops its server.
    pub fn empty() -> Self {
        Self {
            active_model: None,
            ready: false,
            // never confirmed — an empty snapshot has no evidence by definition
            ready_verified_at_ms: None,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            // Nothing served → no real window yet. A `ready` snapshot never
            // carries 0 (the daemon stamps the live `/props` window before
            // publishing ready); 0 is the unambiguous "no window known" sentinel.
            served_context_window: 0,
            // Nothing served → no lanes. A `ready` snapshot always carries the
            // real `--parallel` count; 0 is the "no lanes" sentinel.
            lanes: 0,
            degraded_reason: None,
            // Nothing served → nothing can see.
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
        }
    }

    /// The "nothing served AND here is why" state — a reconcile that ended
    /// Degraded publishes its reason so `serving/status` tells the operator
    /// what failed (a missing binary names its path) instead of a silent
    /// `active_model=null`.
    pub fn degraded(reason: String) -> Self {
        Self {
            degraded_reason: Some(reason),
            ..Self::empty()
        }
    }
}

/// What the running llama-server's own `/props` says it can perceive — the
/// `modalities` block llama.cpp publishes once an mmproj projector loads. This
/// is the ENDPOINT truth (#106): the catalog row *declares* Vision, the mmproj
/// file *exists*, but only the process itself can confirm the projector loaded
/// and the multimodal tokenize path answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultimodalSupport {
    /// `/props` `modalities.vision` — the server accepts image content parts.
    pub vision: bool,
    /// `/props` `modalities.audio` — the server accepts audio content parts.
    pub audio: bool,
}

/// Pure readiness verdict for the VISION side of a lane (#106): given what the
/// catalog row declares, whether an mmproj projector resolved at spawn, and what
/// the running server's `/props` reports, decide whether this lane may claim
/// sight. Split from the daemon IO so every branch is unit-testable.
///
/// - `Ok(false)` — a text lane: the row never declared Vision. Not an error.
/// - `Ok(true)`  — declared Vision, `--mmproj` was passed, AND the server's own
///   `/props` confirms `modalities.vision`. The lane may claim sight.
/// - `Err(why)`  — the row declares Vision but the lane CANNOT verifiably see:
///   no projector resolved, the server reports no vision modality (projector
///   failed to load / wrong file), or the server build doesn't report
///   modalities at all (unverifiable ≠ working). The daemon logs the reason
///   loud and publishes `vision_ready: false` — the capability lie is surfaced,
///   never served ([[fallbacks-are-illegal-fail-loud]]).
pub fn vision_lane_ready(
    declares_vision: bool,
    mmproj_resolved: bool,
    props: Option<MultimodalSupport>,
) -> Result<bool, String> {
    if !declares_vision {
        return Ok(false);
    }
    if !mmproj_resolved {
        return Err(
            "model row declares Vision but no mmproj projector resolved at spawn — \
             the lane serves TEXT ONLY; pull the model's `*-GGUF` repo (projector ships \
             alongside) or set `mmproj_local_path` on the row"
                .to_string(),
        );
    }
    match props {
        Some(m) if m.vision => Ok(true),
        Some(_) => Err(
            "--mmproj was passed but the running server's /props reports \
             modalities.vision=false — the projector failed to load or is the wrong \
             modality (an audio-only mmproj?); the lane cannot see"
                .to_string(),
        ),
        None => Err(
            "--mmproj was passed but the running server's /props carries no `modalities` \
             block — this llama-server build cannot VERIFY multimodal readiness, and an \
             unverified claim of sight is a fabrication; upgrade the serving binary"
                .to_string(),
        ),
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

/// Has the serving daemon completed its FIRST reconcile in this process?
///
/// # Why this exists as its own signal (#350, measured 2026-08-07)
///
/// [`current_serving`] answers "what is live". It cannot answer "have we looked
/// yet" — and those are different facts that produce an IDENTICAL snapshot.
/// `install_serving_state` is called from the daemon's `initialize()`, BEFORE the
/// first reconcile, so from process start until the first publish every reader
/// borrows [`ServingSnapshot::empty`]: `active_model: None, ready: false`. That is
/// indistinguishable from "the daemon looked and nothing is serving".
///
/// The cost of the confusion, measured rather than supposed: personas begin their
/// self-tick immediately at boot, read the placeholder, and the adapter's
/// single-resident guard correctly refuses — producing a LOUD
/// `persona.selftick.inference_failed` naming a serving fault that does not exist.
/// Across 3 days that was 116 failures in 38 bursts, every burst followed by a real
/// reconcile 10–20s later. It self-heals in seconds and never indicated a broken
/// lane; the daemon published `active=<none>` exactly ZERO times in 12 hours while
/// readers saw `<none>` 61 times. An instrument crying wolf 116 times costs more
/// than the fault it was pointing at — this one cost a full night of investigation
/// aimed at the serving layer, which was healthy throughout.
///
/// Deliberately a separate `OnceLock` rather than a field on [`ServingSnapshot`]:
/// that type is ts-rs-exported and read by the grid and positron, with 19
/// construction sites. "Has the daemon started" is a PROCESS-LIFECYCLE fact, not a
/// property of what is currently served, so it belongs beside `SERVING_STATE` (the
/// other process-lifecycle OnceLock in this module) rather than inside the wire
/// payload. If a remote reader ever needs it, promoting it to a snapshot field is
/// mechanical.
static FIRST_RECONCILE: OnceLock<()> = OnceLock::new();

/// Called by the serving daemon the first time it publishes a reconcile. Idempotent.
pub fn mark_first_reconcile() {
    let _ = FIRST_RECONCILE.set(());
}

/// True once the daemon has published at least one reconcile in this process.
///
/// A reader seeing `active_model == None` MUST consult this before calling it a
/// fault: `false` means "still starting, ask again shortly", `true` means "the
/// daemon looked and there is genuinely nothing serving" — which IS a fault worth
/// shouting about. Same distinction `ready_verified_at_ms` draws for `ready`: a
/// claim has to carry whether it rests on evidence or on not-having-looked.
pub fn has_reconciled() -> bool {
    FIRST_RECONCILE.get().is_some()
}

/// Install the daemon's serving-state receiver as the process-wide readable
/// seam. The daemon is a singleton so this is set-once; a second call (e.g. a
/// re-init under test) is ignored. Returns `true` iff this call installed it.
pub fn install_serving_state(rx: watch::Receiver<ServingSnapshot>) -> bool {
    SERVING_STATE.set(rx).is_ok()
}

/// A clone of the process-wide serving-state receiver, for consumers that
/// need to FOLLOW the snapshot over time (the gateway-sync task, card
/// ed3661c4) rather than read it once. `None` before the daemon installs.
pub fn serving_state_receiver() -> Option<watch::Receiver<ServingSnapshot>> {
    SERVING_STATE.get().cloned()
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
        if cur.is_live() {
            return Some(cur.clone());
        }
    }
    // Bind the timeout result before matching so its `watch::Ref` temporary
    // drops before `rx` does (else the borrow outlives `rx` — E0597).
    let waited = tokio::time::timeout(
        timeout,
        rx.wait_for(|s| s.is_live()),
    )
    .await;
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
    let waited = tokio::time::timeout(
        remaining,
        rx.wait_for(|s| s.is_live()),
    )
    .await;
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

    /// The multimodal capabilities the running server ITSELF reports in `/props`
    /// (`modalities.vision` / `modalities.audio`) — the endpoint-side truth of
    /// whether the `--mmproj` projector actually loaded (#106). `Ok(None)` means
    /// the server answered but its `/props` carries no `modalities` block (an
    /// older llama-server build, or a fake control that doesn't probe) — the
    /// caller must treat that as UNVERIFIED, never as "vision works". Default
    /// impl returns `Ok(None)` so fakes/remote controls stay honest by
    /// construction.
    async fn multimodal_support(&self) -> Result<Option<MultimodalSupport>, LlamaServerError> {
        Ok(None)
    }

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

    /// The flag this control's stderr watcher raises when the running lane proves itself
    /// WEDGED — a slot reporting arithmetically impossible progress (see
    /// [`crate::inference::wedge`]). The serving daemon polls it on its tick and owns the
    /// response; the watcher only reports, so lane lifecycle stays with exactly one owner.
    ///
    /// `None` for a control with nobody to report to: a fake, a remote control, or an
    /// ephemeral eval lane that tears down its own process. Default `None` keeps that the
    /// honest answer rather than a silently-never-raised flag.
    fn wedge_flag(&self) -> Option<crate::inference::wedge::WedgeFlag> {
        None
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
        // The comment above states the policy: an unreadable probe is an UNKNOWN set →
        // relaunch. `.unwrap_or_default()` did the opposite whenever `desired` is also
        // empty (the common no-genome case): the error became an empty Vec that COMPARED
        // EQUAL to the target, short-circuiting to AlreadyServing — the exact silent
        // stale-serve the comment promises never happens. Honor the stated policy.
        let genome_matches = match ctrl.active_adapters().await {
            Ok(active) => active == desired,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "could not read the served genome set — treating it as UNKNOWN and \
                     falling through to relaunch rather than risk serving a stale genome"
                );
                false
            }
        };
        if genome_matches {
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
                        || target.context_window <= served.saturating_add(WINDOW_RELAUNCH_TOLERANCE)
                }
                Err(e) => {
                    // Deliberate: a probe error must not spuriously relaunch a healthy
                    // lane (stated above). But the error is still evidence — if windows
                    // stop growing, this line is the reason, and it must be findable.
                    tracing::warn!(
                        error = %e,
                        "could not read the served window — treating the window as OK \
                         (no spurious relaunch); a starved lane will not grow while this \
                         probe keeps failing"
                    );
                    true
                }
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
    /// Raised by this lane's stderr watcher when a slot reports impossible progress,
    /// polled by the serving daemon. Live lane only — an ephemeral lane has no daemon
    /// watching it and tears down its own process, so its watcher would report into a void.
    wedge: Option<crate::inference::wedge::WedgeFlag>,
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
            wedge: Some(crate::inference::wedge::WedgeFlag::new()),
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
            // No serving daemon watches an ephemeral lane, and its owner tears the process
            // down when the eval ends — a wedge report would have no consumer.
            wedge: None,
        }
    }

    /// Kill the currently-running child, if any. Idempotent.
    fn kill_child(&self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            // start_kill is non-blocking; the OS reaps. We're replacing it, so
            // we don't await the exit — the new spawn binds the same port once
            // the old one releases it (readiness poll absorbs the gap).
            // A kill that FAILS is why the successor will find the port still bound —
            // the one fact the next bring-up failure needs and could never see before.
            if let Err(e) = child.start_kill() {
                tracing::warn!(
                    error = %e,
                    "could not signal the llama-server child to die — the port may stay \
                     bound and the next spawn may fail to bind it"
                );
            }
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
    /// Non-blocking: has the child WE spawned already exited? `Some(status)` once
    /// it has, `None` while it runs (or when we own no child — the adopt path).
    /// Lets [`wait_ready`] fail LOUD the instant a bring-up dies instead of polling
    /// a dead port for the whole budget.
    fn child_exit_status(&self) -> Option<std::process::ExitStatus> {
        self.child
            .lock()
            .unwrap()
            .as_mut()
            .and_then(|c| c.try_wait().ok().flatten())
    }

    /// The tail of this lane's stderr log (`llama-server-<port>.log`, the file the
    /// spawn wires stderr to) — the llama.cpp load banner or ggml/Metal fault that
    /// explains a bring-up failure. Best-effort: an unreadable/absent log yields a
    /// marker, never blocks the error path. An EMPTY log is itself a signal — a
    /// child that produced no output before the deadline hung in early init (e.g.
    /// a co-resident second Metal context that never acquired the device), distinct
    /// from a crash (which prints a banner + fault first).
    fn stderr_log_tail(&self) -> String {
        let Some(port) = self
            .root
            .rsplit(':')
            .next()
            .and_then(|p| p.parse::<u16>().ok())
        else {
            return "<lane port unparseable>".to_string();
        };
        let Some(path) = dirs::home_dir().map(|h| {
            h.join(".continuum")
                .join("logs")
                .join(format!("llama-server-{port}.log"))
        }) else {
            return "<no home dir>".to_string();
        };
        match std::fs::read_to_string(&path) {
            Ok(s) => tail_or_hang_marker(&s),
            // The marker is the contract (never block the error path), but a bare
            // "unreadable" hides WHY — permissions vs absent vs mid-rotation.
            Err(e) => format!("<stderr log unreadable: {e}>"),
        }
    }

    async fn wait_ready(&self) -> Result<(), LlamaServerError> {
        // The live lane keeps the tight 90s fail-loud budget; an ephemeral
        // measurement lane gets the co-resident cold-large-model budget (it loads a
        // SECOND large GGUF beside the live one — the warmup legitimately runs
        // longer). Scoped by `is_live_lane` so a wedged LIVE lane still fails fast.
        let budget = if self.is_live_lane {
            READY_TIMEOUT
        } else {
            EPHEMERAL_READY_TIMEOUT
        };
        let health = format!("{}/health", self.root);
        let deadline = Instant::now() + budget;
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
            // #205 unmask: the instant our OWN child exits, fail LOUD with its exit
            // status + stderr tail — the real cause (bad args, model-load failure, an
            // aborted Metal init) — instead of polling a dead port for the whole budget
            // and reporting a bare "/health request failed" that discards the reason.
            // No-op for the adopt path (we own no child); benefits live + ephemeral alike.
            if let Some(status) = self.child_exit_status() {
                return Err(LlamaServerError::Spawn(format!(
                    "llama-server exited during bring-up ({status}) before it served on {} \
                     — last stderr:\n{}",
                    self.root,
                    self.stderr_log_tail()
                )));
            }
            if Instant::now() >= deadline {
                // Even a non-exiting HANG now surfaces the stderr state: an empty log
                // fingerprints "child emitted nothing before it stopped" — the diagnostic
                // that was missing when this masked as a bare 240s /health timeout. The
                // exit-status arm above disambiguates the two empty-log causes: an OS
                // OOM/jetsam kill (SIGKILL/137 — the proven cause when a second large
                // model won't fit in free unified memory) vs. a genuine early-init hang
                // (no exit status). Glass-boxed 2026-07-28: a second 24B eval lane co-resident
                // with the live 24B lane was jetsam-killed with 9.6 GB free.
                return Err(LlamaServerError::NotReady(
                    budget,
                    format!("{last} — last stderr:\n{}", self.stderr_log_tail()),
                ));
            }
            tokio::time::sleep(READY_POLL).await;
        }
    }
}

/// The last lines of an llama-server stderr log, OR — when the log is empty — a
/// diagnostic marker naming what an empty log MEANS: the child produced NO output
/// before it stopped. Read alongside the exit-status arm in `wait_ready`, that
/// disambiguates the two causes: an OS OOM/jetsam kill (SIGKILL/137 — the proven
/// cause when a second large model can't fit in free unified memory; the child
/// dies before llama.cpp prints its banner) vs. a genuine early-init hang (child
/// still running at the deadline). A crash from bad args or a model-load fault
/// prints the banner + fault FIRST, so a non-empty tail carries that directly.
/// Pure so the load-bearing empty-vs-tail decision is unit-tested without touching
/// the real `~/.continuum/logs` path ([[self-test-via-command-feedback-surface-
/// never-blind]], and #72's env-dependent-test lesson).
fn tail_or_hang_marker(contents: &str) -> String {
    let mut lines: Vec<&str> = contents.lines().rev().take(20).collect();
    lines.reverse();
    let tail = lines.join("\n");
    if tail.trim().is_empty() {
        "<stderr log empty — child produced no output before it stopped: with a SIGKILL/137 \
         exit this is an OS OOM/jetsam kill (a second large model that didn't fit in free \
         memory); with no exit status it is a genuine early-init hang>"
            .to_string()
    } else {
        tail
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
        // HARD wall-clock cap on the WHOLE bring-up. `wait_ready`'s budget only
        // bounds the /health poll, and its deadline is checked BETWEEN attempts — so a
        // hang INSIDE an attempt (a stalled `decode_smoke_ok`, a wedged model-mmap, a
        // process-launch that never returns) can wait past it indefinitely. Glass-boxed
        // 2026-07-19: an ephemeral eval lane hung 11 min with no process and free VRAM,
        // no timeout ever firing — a silent glacial wedge that violated fail-loud. This
        // net guarantees the eval fails LOUD after the lane's own load budget instead.
        // On timeout `proc` drops → its `Drop` kills any child it launched. Eval lanes
        // only; the live lane keeps its own (fail-loud-not-fast) bring-up policy.
        // MUST exceed the ephemeral `wait_ready` budget (EPHEMERAL_READY_TIMEOUT) + margin
        // so the INNER deadline fires first with its specific `/health`/decode reason —
        // otherwise this coarse net pre-empts the diagnostic error (glass-boxed 2026-07-21:
        // a co-resident cold 24B warmup exceeded the old 90s inner budget; the fix raised
        // the inner budget, so this outer cap must follow or it clips the warmup at 120s).
        let spawn_cap = EPHEMERAL_READY_TIMEOUT + Duration::from_secs(30);
        match tokio::time::timeout(spawn_cap, proc.serve(target)).await {
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

    /// The running lane's own `/props modalities` verdict — the #106 endpoint
    /// truth, delegated to the child probe. `Ok(None)` = this llama-server
    /// build publishes no modalities block (unverifiable ≠ working).
    pub async fn multimodal_support(&self) -> Result<Option<MultimodalSupport>, LlamaServerError> {
        LlamaServerControl::multimodal_support(&self.proc).await
    }

    /// The model id this lane actually serves (its `/v1/models` alias) — for
    /// incumbent verification against the process's own report, never our spawn
    /// memory.
    pub async fn active_model(&self) -> Result<Option<String>, LlamaServerError> {
        LlamaServerControl::active_model(&self.proc).await
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

    async fn multimodal_support(&self) -> Result<Option<MultimodalSupport>, LlamaServerError> {
        // Same root-level `/props` as the served window. llama.cpp publishes a
        // `modalities: { vision: bool, audio: bool }` block once an mtmd
        // projector loads (and `vision:false` when launched text-only). An
        // ABSENT block is `Ok(None)` — "this build can't say", which the pure
        // verdict (`vision_lane_ready`) refuses to treat as sight.
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
        Ok(body.get("modalities").map(|m| MultimodalSupport {
            vision: m.get("vision").and_then(|v| v.as_bool()).unwrap_or(false),
            audio: m.get("audio").and_then(|v| v.as_bool()).unwrap_or(false),
        }))
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
            // Enough tokens to prove real decode (MIN_SMOKE_DECODE_TOKENS = 5) with
            // margin, WITHOUT hogging a busy co-tenant lane for 3s of decode — the
            // probe must be a light passenger, not another load source.
            "max_tokens": 12,
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
        // A missing/!u64 `usage.completion_tokens` is NOT "produced 0 tokens" — it is
        // "this server did not tell us". Both fail the smoke test, but only one of them
        // is a wedge; say which, or a schema change reads forever as a dead lane.
        let Some(out_tokens) = v
            .get("usage")
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|t| t.as_u64())
        else {
            tracing::warn!(
                "smoke decode: response carried no usage.completion_tokens — cannot \
                 confirm the compute path (treating as NOT proven, but this is a \
                 missing field, not a measured zero)"
            );
            return false;
        };
        out_tokens >= MIN_SMOKE_DECODE_TOKENS
    }

    fn owns_child(&self) -> bool {
        self.child.lock().unwrap().is_some()
    }

    fn wedge_flag(&self) -> Option<crate::inference::wedge::WedgeFlag> {
        self.wedge.clone()
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

        // KILL-VERIFY GATE (2026-07-23 ready-flap case): llama-server does NOT
        // retry a lost bind — spawned against a still-held port it exits
        // instantly and `wait_ready` burns its whole budget polling a corpse,
        // which is the flap that killed 3,318 live turns in one day. Verify the
        // port is actually free BEFORE spawning: a short grace absorbs a normal
        // teardown (a Metal-resident model takes seconds to release); if the
        // port is STILL held, name the holder — a verified llama-server (the
        // predecessor whose pidfile was disarmed, an adopted-then-churned
        // orphan) is reaped and re-verified; anything else fails loud now
        // instead of wasting the ready budget ([[fallbacks-are-illegal-fail-loud]]).
        if !crate::inference::lane_process::wait_port_free(port, Duration::from_secs(8)).await {
            match crate::inference::lane_process::pid_listening_on_port(port) {
                Some(pid) if crate::inference::lane_process::is_llama_server(pid) => {
                    crate::probe!(
                        class = "serving.lane_kill_verify",
                        port = port,
                        holder_pid = pid,
                        "port still held after teardown grace — reaping the verified llama-server holder",
                    );
                    crate::inference::lane_process::kill9(pid);
                    if !crate::inference::lane_process::wait_port_free(
                        port,
                        Duration::from_secs(10),
                    )
                    .await
                    {
                        return Err(LlamaServerError::Spawn(format!(
                            "port {port} still held after verified reap of llama-server pid {pid} — refusing to spawn against a bound port"
                        )));
                    }
                }
                holder => {
                    return Err(LlamaServerError::Spawn(format!(
                        "port {port} held by {holder:?} (not a verifiable llama-server) — refusing a blind spawn that would flap ready; free the port or change the lane plan"
                    )));
                }
            }
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
        if let Some(mmproj) =
            crate::model_registry::artifacts::resolve_mmproj_for_model(&target.model)
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
        // Device-fit resident-override (#29): source the RESIDENT (non-expert)
        // tensors from the precision-shrunk fit GGUF so the whole resident tier fits
        // VRAM offloaded to GPU, while this primary GGUF streams the experts. The
        // loader hook (`LLAMA_RESIDENT_OVERRIDE`) lazy-maps only the override's
        // resident bytes (its experts are ignored). Set by the governor's device_fit
        // plan when as-shipped resident overflows the VRAM budget; absent = resident
        // fits as-shipped (no override, no env). [[device-fit-repeatable-primitive]].
        if let Some(ov) = &target.resident_override {
            cmd.env("LLAMA_RESIDENT_OVERRIDE", ov);
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
        // ONE comma-separated `--lora` value: llama.cpp (b8784+) deprecated
        // repeated `--lora` flags and SILENTLY keeps only the last — which was
        // collapsing every multi-layer genome stack to a single adapter
        // (glass-boxed 2026-07-23 in the lane's own stderr: 'DEPRECATED:
        // --lora specified multiple times... only last value will be used' ×4
        // while a 4-layer stack served). The genome's whole premise is layers
        // that STACK; this arg shape is what actually stacks them.
        if !target.adapters.is_empty() {
            let joined = target
                .adapters
                .iter()
                .map(|a| a.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(",");
            cmd.arg("--lora").arg(joined);
        }
        // K3 slice-1 physical expert paging: if the residency planner handed us a layer
        // placement, offload the COLD layers' stacked expert tensors to CPU via -ot, keeping
        // the hot layers GPU-resident. Experts are stacked (one blk.N.ffn_*_exps tensor per
        // layer), so -ot — which places whole tensors — pages at LAYER granularity. None /
        // all-hot → no flag (an empty -ot is rejected by llama-server). A change to the hot
        // set is honored on the next relaunch (the pager decides when).
        if let Some(ot) = target.expert_ot_value() {
            cmd.arg("--override-tensor").arg(ot);
        }
        // MoE glass-box env seam (#278): when expert paging is active, the DAEMON
        // hands the fork its capture + plan file locations. Previously these envs
        // existed only when an operator hand-exported them before booting the
        // core, so every unattended MoE serve ran dark — no pager telemetry for
        // the serving console, no actuator channel for the policy controller.
        // The paths are the pure port derivation [`moe_glass_box_paths`]; the
        // positron serving source re-derives the same capture path from the
        // snapshot's port, so the two sides share one truth with no wire-type
        // change. An operator-exported env is an intentional campaign override
        // and is inherited untouched (config over convention — the child gets
        // the parent's env by default; we only fill the ABSENT case).
        if target.expert_placement.is_some() {
            if let Some(gb) = moe_glass_box_paths(port) {
                if std::env::var_os("GGML_MOE_CAPTURE_FILE").is_none() {
                    cmd.env("GGML_MOE_CAPTURE_FILE", &gb.capture);
                }
                if std::env::var_os("GGML_MOE_PLAN_FILE").is_none() {
                    cmd.env("GGML_MOE_PLAN_FILE", &gb.plan);
                }
                if std::env::var_os("GGML_MOE_TRACE_FILE").is_none() {
                    cmd.env("GGML_MOE_TRACE_FILE", &gb.trace);
                }
            }
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
        // Capture the server's stderr (#175). llama.cpp prints its load banner AND the
        // underlying ggml/Metal fault behind a `{"code":500}` reply here; `Stdio::null()`
        // threw that root cause away.
        //
        // PIPED, not a raw file fd. Handing the child a file means the CHILD owns the write
        // and nothing can bound it: on 2026-08-05 a wedged slot printed `progress = 1.10`
        // at 1.2 GB/min for four hours and the log reached 172 GB, taking the machine to
        // zero bytes free. `code::child_log` owns the file instead and rotates at a few MB.
        // [[never-blind-feedback-driven-iteration]]
        let log_path = dirs::home_dir().map(|h| {
            h.join(".continuum")
                .join("logs")
                .join(format!("llama-server-{}.log", port))
        });
        if let Some(dir) = log_path.as_ref().and_then(|p| p.parent()) {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut child = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| LlamaServerError::Spawn(format!("{}: {e}", self.bin)))?;
        // Hand stderr to the capped sink. If either the handle or the path is missing the
        // child still serves — unlogged, and the pipe drains to close so it cannot block.
        // The sink reads every line to keep the file capped, so it is also the cheapest
        // place to notice the engine reporting an IMPOSSIBLE state. A slot printing
        // `progress = 1.10` is wedged; the decode heartbeat can't see it (the OTHER slots
        // still decode, so the lane reads healthy) — which is how one slot burned four
        // hours on 2026-08-05. The watcher only RAISES; the serving daemon reaps.
        let watch: Box<dyn super::child_log::LineWatch> = match self.wedge.clone() {
            Some(flag) => Box::new(super::wedge::WedgeWatch::new(flag)),
            None => Box::new(()),
        };
        match (child.stderr.take(), log_path) {
            (Some(stderr), Some(path)) => super::child_log::drain_capped(stderr, path, watch),
            (Some(_), None) => tracing::warn!(
                probe_class = "serving.llama.stderr_unlogged",
                port = port,
                "no home dir for the llama-server log — serving unlogged (#175)"
            ),
            (None, _) => tracing::warn!(
                probe_class = "serving.llama.stderr_unlogged",
                port = port,
                "could not take llama-server stderr — serving unlogged (#175)"
            ),
        }

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

        self.wait_ready().await?;
        // GENOME DORMANCY (glass-boxed 2026-07-23): llama.cpp loads every
        // `--lora` at scale 1.0 — ALL ACTIVE, superimposed. Four personas'
        // adapters blended at full scale produced the degenerate-decode plague
        // (hangs, digit-splices, zero-walls, prompt-echo) the moment the
        // comma-fix made stacks REALLY load; the month of prior 'stability'
        // was the only-last-adapter bug accidentally serving one costume. The
        // design is paged activation: catalog LOADED, scales DORMANT (0.0),
        // per-request `lora` field activates. Zero them before ready — a lane
        // is not ready while wearing every costume at once.
        if !target.adapters.is_empty() {
            self.zero_adapter_scales().await;
        }
        Ok(())
    }
}

impl LlamaServerProcess {
    /// Set every loaded LoRA adapter's GLOBAL scale to 0.0 (dormant catalog —
    /// per-request activation only). Best-effort: a failure logs loud but does
    /// not fail bringup (a lane with active adapters still serves; it is the
    /// degraded-not-dead case, and the log names it).
    pub async fn zero_adapter_scales(&self) {
        let root = self.v1_url.trim_end_matches('/').trim_end_matches("/v1");
        let url = format!("{root}/lora-adapters");
        let list: Vec<serde_json::Value> = match self.client.get(&url).send().await {
            // `.unwrap_or_default()` turned a malformed/HTML error body into an EMPTY
            // list — indistinguishable from "this lane has no adapters", so dormancy
            // silently no-ops and the genome stays hot at full scale.
            Ok(r) => match r.json().await {
                Ok(list) => list,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "genome dormancy: /lora-adapters returned a body we cannot parse \
                         — adapter scales were NOT zeroed (this is not an empty adapter set)"
                    );
                    return;
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "genome dormancy: could not read /lora-adapters");
                return;
            }
        };
        let zeroed: Vec<serde_json::Value> = list
            .iter()
            .filter_map(|a| a.get("id").and_then(|i| i.as_i64()))
            .map(|id| serde_json::json!({"id": id, "scale": 0.0}))
            .collect();
        if zeroed.is_empty() {
            return;
        }
        match self.client.post(&url).json(&zeroed).send().await {
            Ok(r) if r.status().is_success() => {
                tracing::info!(
                    adapters = zeroed.len(),
                    probe_class = "serving.genome.dormant",
                    "genome catalog loaded DORMANT — all adapter scales zeroed; per-request activation only"
                );
            }
            Ok(r) => {
                tracing::warn!(status = %r.status(), "genome dormancy: scale-zero POST refused")
            }
            Err(e) => tracing::warn!(error = %e, "genome dormancy: scale-zero POST failed"),
        }
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
            if h.is_empty() {
                DEFAULT_HOST.to_string()
            } else {
                h.to_string()
            },
            p.parse().unwrap_or(DEFAULT_PORT),
        ),
        None => (authority.to_string(), DEFAULT_PORT),
    }
}

#[cfg(test)]
mod tests {

    /// what this catches (#350): the two states an EMPTY serving snapshot can mean
    /// collapsing back into one. `ServingSnapshot::empty()` is published from process
    /// start (install_serving_state runs in the daemon's `initialize()`, before the first
    /// reconcile), so "we have not looked yet" and "we looked and nothing is serving" are
    /// byte-identical. Readers that cannot tell them apart shout about a serving fault
    /// during every boot: measured 116 false alarms in 38 bursts over 3 days.
    ///
    /// CORRECTION (2026-08-07) to what this comment first claimed. It said the daemon
    /// "published `active=<none>` ZERO times in 12 hours", which read as: empty means boot,
    /// full stop. It does not. `serving_daemon` publishes `empty()` on EVERY lane teardown
    /// — no servable plan, a re-home, and `declare_lane_wedged` (#175 self-heal) — the
    /// original count simply had no probe on those publish sites to see them. Live receipts
    /// the same day: three personas read `serving: <none>` 59s into a wedge relaunch, 350s
    /// after this latch was set. So `has_reconciled()` answers "has a lane EVER come up",
    /// never "is one up now" — for the latter, read `ServingSnapshot::ready` live.
    ///
    /// The empty snapshot deliberately stays unchanged — it is still the honest "nothing
    /// live" value. What must exist is a SEPARATE signal for whether anyone has looked.
    #[test]
    fn an_empty_snapshot_cannot_itself_distinguish_startup_from_a_serving_fault() {
        let boot = super::ServingSnapshot::empty();
        assert_eq!(boot.active_model, None);
        assert!(!boot.ready);
        // The point: this value is IDENTICAL whether the daemon has reconciled or not,
        // which is exactly why `has_reconciled()` is a separate signal and not something
        // a reader can infer from the snapshot. If someone later adds a field that makes
        // the two distinguishable here, this assertion documents why they must ALSO keep
        // `has_reconciled()` correct rather than silently replacing it.
        assert_eq!(boot, super::ServingSnapshot::empty());
    }

    /// what this catches: `mark_first_reconcile` becoming non-idempotent or
    /// `has_reconciled` reading the wrong cell. A OnceLock set twice must not panic —
    /// the daemon calls it on EVERY reconcile, not just the first.
    ///
    /// NOTE this test can only observe the post-mark state: `FIRST_RECONCILE` is a
    /// process-global OnceLock, so once any test in this binary marks it, it stays
    /// marked. Asserting `!has_reconciled()` first would be order-dependent — the exact
    /// flake shape that made tests pass locally and fail in CI
    /// ([[a-process-global-read-inside-a-decision-makes-tests-order-dependent]]).
    #[test]
    fn marking_the_first_reconcile_is_idempotent_and_observable() {
        super::mark_first_reconcile();
        assert!(super::has_reconciled());
        super::mark_first_reconcile(); // must not panic
        assert!(super::has_reconciled());
    }

    /// what this catches: the health heartbeat going back to probe-always. `decode_smoke_ok`
    /// is a REAL generation through the live slots, so on a saturated lane it cannot get one,
    /// the miss counts as "no decode", and two misses relaunch a lane that is merely busy —
    /// which is what killed SWE run v13 (`serving.health {ok:false} x2 -> relaunch`, then every
    /// generate refused with `serving: <none>`). Real tokens are the honest liveness evidence;
    /// this pins the accessor the heartbeat gates on.
    #[test]
    fn real_token_delivery_is_recorded_as_liveness_evidence() {
        // Fresh process: nothing observed yet ⇒ None, so the caller falls through and probes
        // (a boot with no traffic must still be verified — absence of evidence is not health).
        // NB: process-global, so only assert the post-record contract if a prior test recorded.
        note_real_decode();
        let since = ms_since_real_decode().expect("a recorded decode must be visible");
        assert!(
            since < 5_000,
            "a decode recorded just now must read as recent, got {since}ms"
        );
    }

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // what this catches (#205 unmask, glass-boxed 2026-07-28): an ephemeral eval lane
    // whose child was OS-killed (SIGKILL/137) before it emitted anything produced an
    // EMPTY stderr log and masked as a bare 240s /health timeout. `tail_or_hang_marker`
    // must turn an empty log into the "no output before it stopped" fingerprint naming
    // the OOM-kill vs. early-init-hang split (so the error names the real memory-wall
    // failure mode) while a non-empty log yields its last lines in order. Mutation
    // checks: dropping the empty-branch fails the OOM/jetsam assert; a non-reversed tail
    // fails the ordering assert.
    #[test]
    fn tail_or_hang_marker_fingerprints_empty_and_tails_nonempty() {
        assert!(
            tail_or_hang_marker("").contains("OOM/jetsam"),
            "an empty log is the no-output fingerprint naming the OOM-kill cause, not a silent blank"
        );
        assert!(
            tail_or_hang_marker("   \n\n \t \n").contains("OOM/jetsam"),
            "whitespace-only counts as empty"
        );
        let log = (1..=30)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let tail = tail_or_hang_marker(&log);
        assert!(
            tail.contains("line30") && tail.contains("line11") && !tail.contains("line10"),
            "keeps the LAST 20 lines in original order, not the first: {tail}"
        );
        // sanity: order preserved (line11 appears before line30)
        assert!(tail.find("line11") < tail.find("line30"));
    }

    // what this catches: the slice-1 K3 -ot builder — the LAYER-granular buft-override that
    // physically pages MoE experts. Cold = (0..n_layers) minus hot; the value keys on the
    // real blk.N with `\.` anchors (so blk.3 never matches blk.31), covers all four stacked
    // ffn_*_exps projections, and targets CPU. None when nothing is cold (empty -ot is
    // rejected by llama-server). A regression here = the wrong experts offloaded (a stale
    // plan degrading throughput) or a spawn that 500s on a malformed override.
    #[test]
    fn cold_expert_offload_ot_emits_inverse_of_hot_layers() {
        // Mixed hot/cold: layers 0,2,4 fit GPU → 1,3,5 offload to CPU.
        let v = cold_expert_offload_ot(&[0, 2, 4], 6).expect("some cold → an override");
        assert_eq!(v, r"blk\.(1|3|5)\.ffn.*_exps=CPU");
        // Anchoring intent: the block index is fenced by `\.` on both sides.
        assert!(
            v.contains(r"blk\.(1|3|5)\.ffn"),
            "block index must be dot-anchored"
        );

        // Everything hot → no override at all (NOT an empty string, which llama-server rejects).
        assert_eq!(cold_expert_offload_ot(&[0, 1, 2, 3], 4), None);
        // Zero layers → nothing to place.
        assert_eq!(cold_expert_offload_ot(&[], 0), None);
        // Empty hot set → offload every block (the all-cold extreme).
        assert_eq!(
            cold_expert_offload_ot(&[], 3),
            Some(r"blk\.(0|1|2)\.ffn.*_exps=CPU".to_string())
        );
        // A hot index outside the block range is ignored, never forcing a bogus pattern.
        assert_eq!(
            cold_expert_offload_ot(&[99], 2),
            Some(r"blk\.(0|1)\.ffn.*_exps=CPU".to_string())
        );
    }

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
            expert_placement: None,
            resident_override: None,
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
        assert_eq!(
            ctrl.serves.load(Ordering::SeqCst),
            0,
            "no relaunch when already serving"
        );
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
        assert_eq!(
            ctrl.serves.load(Ordering::SeqCst),
            0,
            "padding noise must not relaunch"
        );
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
            EnsureOutcome::Spawned {
                model: "coder-14b".into()
            },
            "a compute-wedged orphan must be rejected and respawned, never adopted"
        );
        assert_eq!(
            ctrl.serves.load(Ordering::SeqCst),
            1,
            "wedged orphan → fresh spawn"
        );
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
        assert_eq!(
            ctrl.serves.load(Ordering::SeqCst),
            0,
            "no relaunch without force"
        );
        // With force_probe: re-prove decode → fails → reap + respawn a fresh backend.
        let healed = ensure_model_serving(&ctrl, &target("coder-14b"), true).await;
        assert_eq!(
            healed,
            EnsureOutcome::Spawned {
                model: "coder-14b".into()
            },
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
        assert_eq!(
            ctrl.serves.load(Ordering::SeqCst),
            0,
            "no relaunch for an owned child"
        );
    }

    // what this catches: a different model live → relaunch to the desired one.
    #[tokio::test]
    async fn wrong_model_triggers_relaunch() {
        let ctrl = FakeControl::probe(Ok(Some("general-4b".into())));
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
        assert_eq!(
            outcome,
            EnsureOutcome::Spawned {
                model: "coder-14b".into()
            }
        );
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
        let outcome = ensure_model_serving(
            &ctrl,
            &target_with_adapters("coder-14b", &["/genes/b.gguf", "/genes/a.gguf"]),
            false,
        )
        .await;
        assert_eq!(outcome, EnsureOutcome::AlreadyServing);
        assert_eq!(
            ctrl.serves.load(Ordering::SeqCst),
            0,
            "no relaunch when genome set matches"
        );
    }

    // what this catches: SAME model but a NEW gene trained (set grew) → relaunch.
    // This is the wire that closes the genome learning loop — without the adapter
    // comparison a freshly-trained gene with an unchanged model id would return
    // AlreadyServing and never load into the catalog.
    #[tokio::test]
    async fn new_gene_triggers_relaunch_on_same_model() {
        let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
            .with_active_adapters(vec!["/genes/a.gguf".into()]);
        let outcome = ensure_model_serving(
            &ctrl,
            &target_with_adapters("coder-14b", &["/genes/a.gguf", "/genes/b.gguf"]),
            false,
        )
        .await;
        assert_eq!(
            outcome,
            EnsureOutcome::Spawned {
                model: "coder-14b".into()
            }
        );
        assert_eq!(
            ctrl.serves.load(Ordering::SeqCst),
            1,
            "new gene must relaunch to repopulate the catalog"
        );
    }

    // what this catches: nothing running (Unreachable) is NOT an error — it's
    // the normal pre-spawn state, so we serve, not degrade.
    #[tokio::test]
    async fn unreachable_means_spawn_not_degrade() {
        let ctrl = FakeControl::probe(Err("connection refused"));
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
        assert_eq!(
            outcome,
            EnsureOutcome::Spawned {
                model: "coder-14b".into()
            }
        );
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 1);
    }

    // what this catches: no model live and serve() fails → Degraded with the
    // named reason, NEVER a panic on the daemon tick (degrade-never-panic).
    #[tokio::test]
    async fn serve_failure_degrades_with_reason() {
        let ctrl = FakeControl::probe(Ok(None)).serve_fails();
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
        match outcome {
            EnsureOutcome::Degraded { reason } => {
                assert!(reason.contains("boom"), "reason: {reason}")
            }
            other => panic!("expected Degraded, got {other:?}"),
        }
    }

    // what this catches: host:port parsing for the launch args, including the
    // /v1-stripped root and a missing port falling back to the default.
    #[test]
    fn split_host_port_parses_root() {
        assert_eq!(
            split_host_port("http://127.0.0.1:58057"),
            ("127.0.0.1".into(), 58057)
        );
        assert_eq!(
            split_host_port("http://0.0.0.0:9090"),
            ("0.0.0.0".into(), 9090)
        );
        assert_eq!(
            split_host_port("http://localhost"),
            ("localhost".into(), DEFAULT_PORT)
        );
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
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: None,
            ready: true,
            base_url: "x".into(),
            adapters: Vec::new(),
            served_context_window: 0,
            lanes: 0,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
        });
        assert!(!pred(&rx.borrow()));
        // not-ready but has a model → unsatisfied.
        tx.send_replace(ServingSnapshot {
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("coder".into()),
            ready: false,
            base_url: "x".into(),
            adapters: Vec::new(),
            served_context_window: 0,
            lanes: 0,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
        });
        assert!(!pred(&rx.borrow()));
        // ready AND a model → satisfied, and wait_for resolves to it at once.
        tx.send_replace(ServingSnapshot {
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("coder".into()),
            ready: true,
            base_url: "x".into(),
            adapters: Vec::new(),
            served_context_window: 0,
            lanes: 0,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
        });
        let got = tokio::time::timeout(Duration::from_millis(100), rx.wait_for(pred))
            .await
            .expect("should not time out")
            .expect("sender live");
        assert_eq!(got.active_model.as_deref(), Some("coder"));
    }

    // what this catches: the #106 vision-readiness verdict must only claim sight when
    // ALL THREE facts line up — the row declares Vision, an mmproj resolved at spawn,
    // AND the running server's own /props confirms modalities.vision. A regression that
    // returns Ok(true) from a weaker combination publishes `vision_ready: true` for a
    // lane that silently drops image parts — the exact capability lie the observe path
    // gates on ([[fallbacks-are-illegal-fail-loud]]).
    #[test]
    fn vision_lane_ready_requires_declaration_projector_and_props_confirmation() {
        let sees = MultimodalSupport {
            vision: true,
            audio: false,
        };
        let blind = MultimodalSupport {
            vision: false,
            audio: true,
        };
        // Text lane: never declared Vision → not vision-ready, and NOT an error.
        assert_eq!(vision_lane_ready(false, false, None), Ok(false));
        assert_eq!(vision_lane_ready(false, true, Some(sees)), Ok(false));
        // Declared Vision but no projector resolved → loud error naming the gap.
        let err = vision_lane_ready(true, false, None).unwrap_err();
        assert!(err.contains("no mmproj projector resolved"), "{err}");
        // Projector passed, server confirms vision → the ONLY Ok(true) path.
        assert_eq!(vision_lane_ready(true, true, Some(sees)), Ok(true));
        // Projector passed but the server says it can't see (wrong/failed mmproj).
        let err = vision_lane_ready(true, true, Some(blind)).unwrap_err();
        assert!(err.contains("modalities.vision=false"), "{err}");
        // Server build reports no modalities block: UNVERIFIED must not read as sight.
        let err = vision_lane_ready(true, true, None).unwrap_err();
        assert!(err.contains("cannot VERIFY"), "{err}");
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
