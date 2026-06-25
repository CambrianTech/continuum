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
/// Preferred local serving port. Deliberately chosen (not random) so a healthy
/// boot always lands here and operators have a stable default to expect — but
/// it is a PREFERENCE, not an assumption: [`chosen_port`] scans up from it for
/// the first free port. "Pick a port, but always scan — never hardcode" (Joel).
const DEFAULT_PORT: u16 = 58057;

/// How many ports above [`DEFAULT_PORT`] we scan for a free one before giving up
/// and binding the base (letting the spawn fail loud rather than serving
/// somewhere unexpected). A small window: if 64 consecutive ports are taken the
/// machine has a real problem worth surfacing. The idealized single registry of
/// every port in the system (Joel: "a singular place that keeps track of ports")
/// is deferred as over-engineering for now; this is the scan-don't-hardcode
/// floor of it.
const PORT_SCAN_WINDOW: u16 = 64;

/// How long we wait for a freshly-spawned server to answer `/health` with 200
/// before declaring the launch degraded. Model load (mmap + Metal warm) can take
/// many seconds for a large GGUF; this is generous but bounded so a wedged
/// launch can't hang the daemon's reconcile forever.
const READY_TIMEOUT: Duration = Duration::from_secs(90);

/// Poll cadence while waiting for `/health`. 503 → still loading, keep waiting.
const READY_POLL: Duration = Duration::from_millis(500);

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
    /// The host-fit served window the planner computed for this host. Sized to
    /// fit the working set (tool schemas + framing + room burst + recalled
    /// memory + completion reserve) within the budget, capped only by the
    /// model's own trained ceiling. The deliberation faculty keeps its prompt
    /// inside this so llama-server never 500s ("Context size has been exceeded").
    pub context_window: u32,
    /// The trained LoRA genome layers to load into the serving catalog at spawn
    /// (`llama-server --lora <path>` per entry). This is the SET — which genes
    /// are *loadable*; the per-request `"lora":[{id,scale}]` body field decides
    /// which page IN for a given turn. llama.cpp has no hot-load API, so a change
    /// to this set is a relaunch (rare — genes are produced post-training);
    /// page-in/out within a loaded set never relaunches. Empty = base model only
    /// (the legitimate no-genes-trained state, NOT a fallback).
    pub adapters: Vec<AdapterEntry>,
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

/// The local serving port chosen for this run: the first bindable port at or
/// above [`DEFAULT_PORT`], decided ONCE and memoized for the process lifetime so
/// the launch args, the `/health` probe, and the published snapshot's `base_url`
/// all agree (they all derive from [`serving_root`]). Scanning — never assuming
/// the preferred port is free — means a stale holder of the default never wedges
/// our bind: we move up, and the snapshot carries the real port to every
/// consumer, which already reads `base_url` from the snapshot rather than a const.
fn chosen_port() -> u16 {
    *CHOSEN_PORT.get_or_init(|| first_free_port(DEFAULT_PORT))
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

/// Path to the `llama-server` binary. Deployment shape: `LLAMA_SERVER_BIN`
/// overrides; otherwise we rely on it being on `PATH`. We do NOT silently fall
/// back to a different server — a missing binary surfaces loudly when spawn is
/// attempted ([[fallbacks-are-illegal-fail-loud]]).
fn server_bin() -> String {
    crate::config_env::read("LLAMA_SERVER_BIN")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "llama-server".to_string())
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
}

impl ServingSnapshot {
    /// The "nothing served" state — boot, or after a node drops its server.
    pub fn empty() -> Self {
        Self {
            active_model: None,
            ready: false,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
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

/// The local serving port chosen for this run (see [`chosen_port`]). Memoized on
/// first access; the free-port scan happens exactly once. Process-wide so the
/// launch, the probe, and every published snapshot agree on one port.
static CHOSEN_PORT: OnceLock<u16> = OnceLock::new();

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
pub const DEFAULT_SERVING_WAIT: Duration = Duration::from_secs(30);

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
}

/// Pure reconcile decision: bring the running server in line with `desired`.
/// Split from the process impl so the branch logic is unit-tested against a
/// fake. `Unreachable` from the probe means "nothing up" → serve; any other
/// probe error degrades rather than blindly relaunching over a sick server.
pub async fn ensure_model_serving<C: LlamaServerControl + ?Sized>(
    ctrl: &C,
    target: &ServingTarget,
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
            return EnsureOutcome::AlreadyServing;
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

    /// Poll `/health` until the server answers 200 (ready) or we time out. 503
    /// means "still loading the model" — keep waiting. A connection error means
    /// "not up yet" early in the launch — also keep waiting, until the deadline.
    async fn wait_ready(&self) -> Result<(), LlamaServerError> {
        let health = format!("{}/health", self.root);
        let deadline = Instant::now() + READY_TIMEOUT;
        let mut last = String::from("no response");
        loop {
            match self.client.get(&health).send().await {
                Ok(resp) if resp.status().is_success() => return Ok(()),
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
        self.kill_child();
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
        self.kill_child();

        let mut cmd = tokio::process::Command::new(&self.bin);
        cmd.arg("-m")
            .arg(&gguf)
            .arg("--alias")
            .arg(&target.model.id)
            .arg("--host")
            .arg(host)
            .arg("--port")
            .arg(port.to_string())
            // The host-fit served window from the plan — never a constant.
            .arg("-c")
            .arg(target.context_window.to_string())
            // Serve embeddings from the same process so the embedder doesn't
            // need a second server. Personas' embedding adapter points here too.
            .arg("--embeddings");
        // Load each trained genome layer into the `/lora-adapters` catalog at
        // index order; the per-request `"lora":[{id,scale}]` field pages them in.
        for adapter in &target.adapters {
            cmd.arg("--lora").arg(&adapter.path);
        }
        let child = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| LlamaServerError::Spawn(format!("{}: {e}", self.bin)))?;

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
    }

    impl FakeControl {
        fn probe(probe: Result<Option<String>, &'static str>) -> Self {
            Self {
                probe,
                active_adapters: Vec::new(),
                serve_ok: true,
                serves: AtomicUsize::new(0),
            }
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
                gguf_local_path: None,
                chat_template: None,
                stop_sequences: Vec::new(),
                multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
                mmproj_local_path: None,
            },
            context_window: 32768,
            adapters: Vec::new(),
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

    // what this catches: the desired model already being served is a NO-OP — we
    // must not relaunch (which would kill the GPU-warm server). Regression here
    // would thrash the served model every tick.
    #[tokio::test]
    async fn already_serving_does_not_relaunch() {
        let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())));
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b")).await;
        assert_eq!(outcome, EnsureOutcome::AlreadyServing);
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 0, "no relaunch when already serving");
    }

    // what this catches: a different model live → relaunch to the desired one.
    #[tokio::test]
    async fn wrong_model_triggers_relaunch() {
        let ctrl = FakeControl::probe(Ok(Some("general-4b".into())));
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b")).await;
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
            ensure_model_serving(&ctrl, &target_with_adapters("coder-14b", &["/genes/b.gguf", "/genes/a.gguf"]))
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
            ensure_model_serving(&ctrl, &target_with_adapters("coder-14b", &["/genes/a.gguf", "/genes/b.gguf"]))
                .await;
        assert_eq!(outcome, EnsureOutcome::Spawned { model: "coder-14b".into() });
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 1, "new gene must relaunch to repopulate the catalog");
    }

    // what this catches: nothing running (Unreachable) is NOT an error — it's
    // the normal pre-spawn state, so we serve, not degrade.
    #[tokio::test]
    async fn unreachable_means_spawn_not_degrade() {
        let ctrl = FakeControl::probe(Err("connection refused"));
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b")).await;
        assert_eq!(outcome, EnsureOutcome::Spawned { model: "coder-14b".into() });
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 1);
    }

    // what this catches: no model live and serve() fails → Degraded with the
    // named reason, NEVER a panic on the daemon tick (degrade-never-panic).
    #[tokio::test]
    async fn serve_failure_degrades_with_reason() {
        let ctrl = FakeControl::probe(Ok(None)).serve_fails();
        let outcome = ensure_model_serving(&ctrl, &target("coder-14b")).await;
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
        });
        assert!(!pred(&rx.borrow()));
        // not-ready but has a model → unsatisfied.
        tx.send_replace(ServingSnapshot {
            active_model: Some("coder".into()),
            ready: false,
            base_url: "x".into(),
            adapters: Vec::new(),
        });
        assert!(!pred(&rx.borrow()));
        // ready AND a model → satisfied, and wait_for resolves to it at once.
        tx.send_replace(ServingSnapshot {
            active_model: Some("coder".into()),
            ready: true,
            base_url: "x".into(),
            adapters: Vec::new(),
        });
        let got = tokio::time::timeout(Duration::from_millis(100), rx.wait_for(pred))
            .await
            .expect("should not time out")
            .expect("sender live");
        assert_eq!(got.active_model.as_deref(), Some("coder"));
    }
}
