//! Configuration responsibility of the llama-server process owner.
use super::*;
/// Default root the core serves from when nothing overrides it. llama-server's
/// OpenAI surface lives under `/v1`; `/health` and `/props` are at the root.
pub(super) const DEFAULT_HOST: &str = "127.0.0.1";

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
pub(super) const DEFAULT_PORT: u16 = 58057;


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
pub(super) const PORT_SCAN_WINDOW: u16 = 64;


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
pub(super) const EPHEMERAL_READY_TIMEOUT: Duration = Duration::from_secs(240);


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
pub(super) const PROBE_TIMEOUT: Duration = Duration::from_secs(4);


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
pub(super) const DECODE_SMOKE_TIMEOUT: Duration = Duration::from_secs(75);


/// Resolve the serving root (`http://host:port`). Deployment shape (which host /
/// port llama-server listens on) is legitimately env-configurable per the
/// concurrency guide's "socket paths / deployment shape" carve-out — unlike
/// substrate *thresholds*, which stay `const`. The value carries no `/v1`; we
/// append `/v1` and `/health` as needed so there is one source of truth.
pub fn serving_root() -> String {
    if let Some(raw) = continuum_base::config_env::read("LLAMA_SERVER_BASE_URL") {
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
pub(super) fn chosen_port() -> u16 {
    DEFAULT_PORT
}


/// Scan `[base, base + PORT_SCAN_WINDOW)` for a port we can bind right now, through the
/// one probe ([`crate::system_resources::ports::first_bindable`]). The brief TOCTOU gap until
/// llama-server claims it is absorbed by the readiness poll (a lost race surfaces
/// loudly as `NotReady`, never a silent wrong-port serve). If nothing in the
/// window is free we return `base` and let the spawn fail loud
/// ([[fallbacks-are-illegal-fail-loud]]).
pub(super) fn first_free_port(base: u16) -> u16 {
    crate::system_resources::ports::first_bindable(DEFAULT_HOST, base..base.saturating_add(PORT_SCAN_WINDOW)).unwrap_or(base) // unwrap_or: nothing free in the window -> base, so the spawn's bind fails LOUD (never a silent wrong port)
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


/// Split `http://host:port` into (`host`, `port`). Falls back to the defaults
/// for any part we can't parse — deployment shape, not a substrate threshold.
pub(super) fn split_host_port(root: &str) -> (String, u16) {
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


/// llama-server announces a debug build on its own output ("warning: DEBUG BUILD
/// (asserts enabled) -- performance numbers from this process are not valid").
pub(super) fn is_debug_build(version_output: &str) -> bool {
    version_output.contains("DEBUG BUILD")
}

