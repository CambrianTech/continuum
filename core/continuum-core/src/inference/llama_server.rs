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

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Default root the core serves from when nothing overrides it. llama-server's
/// OpenAI surface lives under `/v1`; `/health` and `/props` are at the root.
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 58057;

/// How long we wait for a freshly-spawned server to answer `/health` with 200
/// before declaring the launch degraded. Model load (mmap + Metal warm) can take
/// many seconds for a large GGUF; this is generous but bounded so a wedged
/// launch can't hang the daemon's reconcile forever.
const READY_TIMEOUT: Duration = Duration::from_secs(90);

/// Poll cadence while waiting for `/health`. 503 → still loading, keep waiting.
const READY_POLL: Duration = Duration::from_millis(500);

/// Per-server working context. A serving cap, not the model's trained ceiling —
/// the daemon's plan sizes lanes against the budget; this is the `-c` we launch
/// with so KV stays bounded. Matches `serving_daemon::PLANNED_CTX_TOKENS`.
const SERVE_CTX_TOKENS: u32 = 8192;

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
        let root = trimmed.strip_suffix("/v1").unwrap_or(trimmed);
        if !root.is_empty() {
            return root.to_string();
        }
    }
    format!("http://{DEFAULT_HOST}:{DEFAULT_PORT}")
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
}

impl ServingSnapshot {
    /// The "nothing served" state — boot, or after a node drops its server.
    pub fn empty() -> Self {
        Self {
            active_model: None,
            ready: false,
            base_url: serving_v1_url(),
        }
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

    /// (Re)spawn llama-server to serve `model_id` and block until it is ready.
    /// Switching models is a relaunch — there is no load-by-name API.
    async fn serve(&self, model_id: &str) -> Result<(), LlamaServerError>;
}

/// Pure reconcile decision: bring the running server in line with `desired`.
/// Split from the process impl so the branch logic is unit-tested against a
/// fake. `Unreachable` from the probe means "nothing up" → serve; any other
/// probe error degrades rather than blindly relaunching over a sick server.
pub async fn ensure_model_serving<C: LlamaServerControl + ?Sized>(
    ctrl: &C,
    desired: &str,
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

    if active.as_deref() == Some(desired) {
        return EnsureOutcome::AlreadyServing;
    }

    match ctrl.serve(desired).await {
        Ok(()) => EnsureOutcome::Spawned {
            model: desired.to_string(),
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

    async fn serve(&self, model_id: &str) -> Result<(), LlamaServerError> {
        // Resolve the id to an on-disk GGUF. No file → fail loud; we never serve
        // a substitute model ([[fallbacks-are-illegal-fail-loud]]).
        let gguf: PathBuf = crate::model_registry::artifacts::resolve_gguf_for_model_id(model_id)
            .ok_or_else(|| LlamaServerError::ModelNotFound(model_id.to_string()))?;

        let (host, port) = split_host_port(&self.root);

        // One server at a time: kill the old child before binding the port.
        self.kill_child();

        let child = tokio::process::Command::new(&self.bin)
            .arg("-m")
            .arg(&gguf)
            .arg("--alias")
            .arg(model_id)
            .arg("--host")
            .arg(host)
            .arg("--port")
            .arg(port.to_string())
            .arg("-c")
            .arg(SERVE_CTX_TOKENS.to_string())
            // Serve embeddings from the same process so the embedder doesn't
            // need a second server. Personas' embedding adapter points here too.
            .arg("--embeddings")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| LlamaServerError::Spawn(format!("{}: {e}", self.bin)))?;

        *self.child.lock().unwrap() = Some(child);

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
        serve_ok: bool,
        serves: AtomicUsize,
    }

    impl FakeControl {
        fn probe(probe: Result<Option<String>, &'static str>) -> Self {
            Self { probe, serve_ok: true, serves: AtomicUsize::new(0) }
        }
        fn serve_fails(mut self) -> Self {
            self.serve_ok = false;
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
        async fn serve(&self, _model_id: &str) -> Result<(), LlamaServerError> {
            self.serves.fetch_add(1, Ordering::SeqCst);
            if self.serve_ok {
                Ok(())
            } else {
                Err(LlamaServerError::Spawn("boom".into()))
            }
        }
    }

    // what this catches: the desired model already being served is a NO-OP — we
    // must not relaunch (which would kill the GPU-warm server). Regression here
    // would thrash the served model every tick.
    #[tokio::test]
    async fn already_serving_does_not_relaunch() {
        let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())));
        let outcome = ensure_model_serving(&ctrl, "coder-14b").await;
        assert_eq!(outcome, EnsureOutcome::AlreadyServing);
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 0, "no relaunch when already serving");
    }

    // what this catches: a different model live → relaunch to the desired one.
    #[tokio::test]
    async fn wrong_model_triggers_relaunch() {
        let ctrl = FakeControl::probe(Ok(Some("general-4b".into())));
        let outcome = ensure_model_serving(&ctrl, "coder-14b").await;
        assert_eq!(outcome, EnsureOutcome::Spawned { model: "coder-14b".into() });
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 1);
    }

    // what this catches: nothing running (Unreachable) is NOT an error — it's
    // the normal pre-spawn state, so we serve, not degrade.
    #[tokio::test]
    async fn unreachable_means_spawn_not_degrade() {
        let ctrl = FakeControl::probe(Err("connection refused"));
        let outcome = ensure_model_serving(&ctrl, "coder-14b").await;
        assert_eq!(outcome, EnsureOutcome::Spawned { model: "coder-14b".into() });
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 1);
    }

    // what this catches: no model live and serve() fails → Degraded with the
    // named reason, NEVER a panic on the daemon tick (degrade-never-panic).
    #[tokio::test]
    async fn serve_failure_degrades_with_reason() {
        let ctrl = FakeControl::probe(Ok(None)).serve_fails();
        let outcome = ensure_model_serving(&ctrl, "coder-14b").await;
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
}
