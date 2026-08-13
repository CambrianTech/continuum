//! Inference sessions — the command→handle lifecycle for inference.
//!
//! Per Joel ("always use commands"): inference is acquired by a COMMAND that mints
//! a HANDLE, the handle keeps the session alive (reused across turns, recoverable),
//! and it is closed when done — never a one-shot HTTP call. This is the structural
//! foundation that makes inference grid-routable (the open-command routes to any
//! node) and self-healing (a lost handle → re-resolve, don't crash)
//! ([[long-running-commands-are-handle-based]], [[compute-lease-boundary]]).
//!
//! This module is the lifecycle skeleton: `ai/inference/open` mints a session bound
//! to a discovered model, `ai/inference/find` recovers a handle (so a caller can
//! tell a live lease from a lost one), `ai/inference/close` releases it. Token
//! generation OVER a handle + grid routing of `open` are the next slices; the
//! handle is the seam they hang on.

use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use dashmap::DashMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::ai::types::ChatMessage;
use crate::modules::ai_provider::{generate_text, global_registry};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx, DynCommand};

/// A live inference lease: a handle bound to a concrete model. Held in the global
/// registry so it survives across turns (keep-alive) and can be re-found if a caller
/// loses its reference. Token generation delegates to the resolved provider adapter
/// (next slice); the session itself is the lease record + identity.
#[derive(Debug, Clone, Serialize, TS)]
pub struct InferenceSession {
    #[ts(type = "string")]
    pub id: Uuid,
    /// The concrete model this lease is bound to (what unsloth serves).
    pub model: String,
    #[ts(type = "number")]
    pub opened_at_ms: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Process-global registry of live inference sessions — the ONE place a handle's
/// liveness is tracked (so `find` is authoritative and `close` is a real release).
pub struct InferenceSessionRegistry {
    sessions: DashMap<Uuid, Arc<InferenceSession>>,
    /// persona → its current handle id, so a persona keeps ONE established lease
    /// across turns (reused, or re-homed when lost). The self-heal index.
    by_persona: DashMap<Uuid, Uuid>,
}

impl InferenceSessionRegistry {
    fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            by_persona: DashMap::new(),
        }
    }

    /// Open a lease bound to `model`, returning the live session (handle).
    pub fn open(&self, model: String) -> Arc<InferenceSession> {
        let session = Arc::new(InferenceSession {
            id: Uuid::new_v4(),
            model,
            opened_at_ms: now_ms(),
        });
        self.sessions.insert(session.id, session.clone());
        session
    }

    /// Recover a handle by id — `Some` if the lease is still live, `None` if it was
    /// closed / lost (the caller then re-opens / re-resolves; never crashes).
    pub fn find(&self, id: Uuid) -> Option<Arc<InferenceSession>> {
        self.sessions.get(&id).map(|e| e.clone())
    }

    /// Release a lease. Returns whether a live session was actually closed.
    pub fn close(&self, id: Uuid) -> bool {
        self.sessions.remove(&id).is_some()
    }

    /// Keep an inference relationship ESTABLISHED: if `prior` is still live, reuse
    /// it; otherwise open a fresh lease on `model`. Returns `(session, reused)`.
    /// This is the self-healing seam — a caller passes its last handle every turn
    /// and transparently re-homes onto a new lease when the old node/handle is gone,
    /// never crashing on a lost handle ([[long-running-commands-are-handle-based]]).
    pub fn ensure(&self, prior: Option<Uuid>, model: String) -> (Arc<InferenceSession>, bool) {
        if let Some(id) = prior {
            if let Some(s) = self.find(id) {
                return (s, true);
            }
        }
        (self.open(model), false)
    }

    /// The persona's current live lease, if any (no probe, no open) — the cheap
    /// happy-path check a per-turn caller uses before resolving a model.
    pub fn persona_session(&self, persona: Uuid) -> Option<Arc<InferenceSession>> {
        let hid = *self.by_persona.get(&persona)?;
        self.find(hid)
    }

    /// Keep ONE established lease per persona across turns: reuse the persona's live
    /// handle, or open a fresh one on `model` and index it (re-homing if the prior
    /// died). This is what makes a persona's inference relationship survive node /
    /// handle loss without crashing the turn.
    pub fn ensure_for_persona(&self, persona: Uuid, model: String) -> Arc<InferenceSession> {
        if let Some(s) = self.persona_session(persona) {
            return s;
        }
        let s = self.open(model);
        self.by_persona.insert(persona, s.id);
        s
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

/// The one process-global session registry.
pub fn global_inference_sessions() -> &'static InferenceSessionRegistry {
    static G: OnceLock<InferenceSessionRegistry> = OnceLock::new();
    G.get_or_init(InferenceSessionRegistry::new)
}

/// Resolve the model a lease should bind to: the explicit `model` if given, else
/// the model the serving daemon ACTUALLY has live. Reads the daemon's published
/// ServingSnapshot (the bus seam) — no HTTP probe — and waits for readiness so an
/// upstart that races the boot reconcile binds correctly. Fail loud if neither —
/// never a stand-in ([[fallbacks-are-illegal-fail-loud]]). The readiness-wait is
/// the one shared `llama_server::DEFAULT_SERVING_WAIT` so every upstart path waits
/// alike.
pub async fn resolve_model(explicit: Option<String>) -> Result<String, CommandError> {
    if let Some(m) = explicit.filter(|s| !s.trim().is_empty()) {
        return Ok(m);
    }
    crate::inference::llama_server::await_ready_serving(
        crate::inference::llama_server::DEFAULT_SERVING_WAIT,
    )
    .await
    .and_then(|s| s.active_model)
    .ok_or_else(|| {
        CommandError::Invalid(
            "no model to bind: pass `model`, or let the serving daemon bring one up \
             (no servable GGUF on disk, or it failed to become ready). No fallback."
                .to_string(),
        )
    })
}

// ───────────────────────────── ai/inference/open ─────────────────────────────

/// Open an inference lease → returns a handle bound to a (discovered or explicit)
/// model. The command→handle entry point: route this command and the lease lands on
/// whatever node serves the model (grid routing = a later slice on the same seam).
pub struct InferenceOpenCommand;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
pub struct InferenceOpenParams {
    /// Bind to this model id; omit to bind to whatever unsloth currently serves.
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct InferenceHandleOutput {
    #[ts(type = "string")]
    pub handle: Uuid,
    pub model: String,
    #[ts(type = "number")]
    pub opened_at_ms: u64,
}

#[async_trait]
impl ActionCommand for InferenceOpenCommand {
    const NAME: &'static str = "ai/inference/open";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Open an inference lease: mint a handle bound to a model (the one unsloth serves, \
         or an explicit `model`). Use the handle to generate; close it when done. Fails \
         loud if no model is available — no local fallback.";
    type Params = InferenceOpenParams;
    type Output = InferenceHandleOutput;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: InferenceOpenParams,
    ) -> Result<InferenceHandleOutput, CommandError> {
        let model = resolve_model(p.model).await?;
        let session = global_inference_sessions().open(model);
        Ok(InferenceHandleOutput {
            handle: session.id,
            model: session.model.clone(),
            opened_at_ms: session.opened_at_ms,
        })
    }
}

// ───────────────────────────── ai/inference/find ─────────────────────────────

/// Recover a handle: is this lease still live? The caller distinguishes a live lease
/// from a lost one (closed, or the node went down) and re-opens if gone — never
/// crashes on a lost handle.
pub struct InferenceFindCommand;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
pub struct InferenceHandleParams {
    #[ts(type = "string")]
    pub handle: Uuid,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct InferenceFindOutput {
    pub found: bool,
    #[ts(optional)]
    pub model: Option<String>,
}

#[async_trait]
impl ActionCommand for InferenceFindCommand {
    const NAME: &'static str = "ai/inference/find";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Recover an inference handle: report whether the lease is still live (and its \
         model). A lost handle is not an error — the caller re-opens / re-resolves.";
    type Params = InferenceHandleParams;
    type Output = InferenceFindOutput;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: InferenceHandleParams,
    ) -> Result<InferenceFindOutput, CommandError> {
        Ok(match global_inference_sessions().find(p.handle) {
            Some(s) => InferenceFindOutput {
                found: true,
                model: Some(s.model.clone()),
            },
            None => InferenceFindOutput {
                found: false,
                model: None,
            },
        })
    }
}

// ───────────────────────────── ai/inference/close ────────────────────────────

/// Release an inference lease (close the handle). Idempotent: closing an
/// already-gone handle reports `closed: false`, not an error.
pub struct InferenceCloseCommand;

#[derive(Debug, Clone, Serialize, TS)]
pub struct InferenceCloseOutput {
    pub closed: bool,
}

#[async_trait]
impl ActionCommand for InferenceCloseCommand {
    const NAME: &'static str = "ai/inference/close";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Release an inference lease by handle. Idempotent — closing a lost/closed handle \
         reports closed=false, not an error.";
    type Params = InferenceHandleParams;
    type Output = InferenceCloseOutput;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: InferenceHandleParams,
    ) -> Result<InferenceCloseOutput, CommandError> {
        Ok(InferenceCloseOutput {
            closed: global_inference_sessions().close(p.handle),
        })
    }
}

// ─────────────────────────── ai/inference/generate ───────────────────────────

/// Generate OVER a live handle: the "use the handle to infer" step. Looks up the
/// lease (NotFound if lost — the caller re-opens), binds the request to the
/// session's model, and delegates to the registered provider (unsloth) via the one
/// `generate_text` path — no parallel inference path, no one-shot bypass.
pub struct InferenceGenerateCommand;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
pub struct InferenceGenerateParams {
    #[ts(type = "string")]
    pub handle: Uuid,
    /// The user prompt to generate from.
    pub prompt: String,
    /// Optional system prompt.
    #[serde(default)]
    pub system: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct InferenceGenerateOutput {
    pub text: String,
    pub model: String,
}

#[async_trait]
impl ActionCommand for InferenceGenerateCommand {
    const NAME: &'static str = "ai/inference/generate";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Generate over an open inference handle. The lease's model is used; delegates \
         to the registered gateway via the one generate path. NotFound if the handle \
         is lost (re-open) — no fallback.";
    type Params = InferenceGenerateParams;
    type Output = InferenceGenerateOutput;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: InferenceGenerateParams,
    ) -> Result<InferenceGenerateOutput, CommandError> {
        let session = global_inference_sessions().find(p.handle).ok_or_else(|| {
            CommandError::NotFound(format!(
                "inference handle {} is not live — re-open",
                p.handle
            ))
        })?;

        // Round-trip messages through ChatMessage's own serde so we don't hand-encode
        // the MessageContent shape; bind to the session's model; route via the ONE
        // generate path (no parallel inference path).
        let user_msg = serde_json::to_value(ChatMessage::text("user", &p.prompt))
            .map_err(|e| CommandError::Internal(format!("encode message: {e}")))?;
        let mut request = serde_json::json!({
            "messages": [user_msg],
            "model": session.model,
            "provider": crate::inference::llama_server::PROVIDER_ID,
        });
        if let Some(sys) = p.system.filter(|s| !s.trim().is_empty()) {
            request["systemPrompt"] = serde_json::Value::String(sys);
        }
        let request = serde_json::from_value(request)
            .map_err(|e| CommandError::Internal(format!("build request: {e}")))?;

        let registry = global_registry();
        let guard = registry.read().await;
        let resp = generate_text(&guard, request)
            .await
            .map_err(CommandError::Internal)?;
        Ok(InferenceGenerateOutput {
            text: resp.text,
            model: session.model.clone(),
        })
    }
}

// ─────────────────────────── ai/inference/ensure ─────────────────────────────

/// Keep the inference relationship established: pass your last handle; get it back
/// if still live, or a freshly re-resolved lease if it was lost (node down, closed).
/// The self-healing entry point — a persona calls this each turn and never crashes
/// on a dead handle; it just re-homes onto a new lease.
pub struct InferenceEnsureCommand;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
pub struct InferenceEnsureParams {
    /// The handle from last time, if any. If still live it's reused; else re-resolved.
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub handle: Option<Uuid>,
    /// Model to bind a fresh lease to (omit → discovered served model).
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct InferenceEnsureOutput {
    #[ts(type = "string")]
    pub handle: Uuid,
    pub model: String,
    /// True = the prior handle was still live and reused; false = re-resolved fresh.
    pub reused: bool,
}

#[async_trait]
impl ActionCommand for InferenceEnsureCommand {
    const NAME: &'static str = "ai/inference/ensure";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Keep inference established: pass your last handle; get it back if live, else a \
         freshly re-resolved lease (self-healing — a dead handle re-homes, never crashes).";
    type Params = InferenceEnsureParams;
    type Output = InferenceEnsureOutput;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: InferenceEnsureParams,
    ) -> Result<InferenceEnsureOutput, CommandError> {
        // Reuse-if-live needs no model resolution; only re-resolve a model when we
        // must open fresh (don't probe the gateway on the happy path).
        if let Some(id) = p.handle {
            if let Some(s) = global_inference_sessions().find(id) {
                return Ok(InferenceEnsureOutput {
                    handle: s.id,
                    model: s.model.clone(),
                    reused: true,
                });
            }
        }
        let model = resolve_model(p.model).await?;
        let (s, reused) = global_inference_sessions().ensure(None, model);
        Ok(InferenceEnsureOutput {
            handle: s.id,
            model: s.model.clone(),
            reused,
        })
    }
}

// NOTE: these ai/inference/* commands DUPLICATE the pre-existing handle system in
// `inference/handle_module.rs` (ai/inference/open/generate/close) — registering them
// panics the registry on duplicate names. They are NOT registered. Reconciling this
// module's per-persona session helper with the canonical handle_module is task #17
// ("Reconcile the two handle models"). evaluate_response uses the registry helper
// (InferenceSessionRegistry) directly, not these commands.
//   crate::register_command!(InferenceOpenCommand);  — DUP of handle_module
//   crate::register_command!(InferenceFindCommand);
//   crate::register_command!(InferenceCloseCommand); — DUP of handle_module
//   crate::register_command!(InferenceGenerateCommand); — DUP of handle_module
//   crate::register_command!(InferenceEnsureCommand);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: THE LIFECYCLE — open mints a live handle, find recovers it
    // while live, close releases it, and find-after-close reports lost (not an error).
    #[test]
    fn open_find_close_lifecycle() {
        let reg = InferenceSessionRegistry::new();
        let s = reg.open("qwen3.5-4b".to_string());
        assert!(reg.find(s.id).is_some(), "live handle is findable");
        assert_eq!(reg.find(s.id).unwrap().model, "qwen3.5-4b");
        assert!(reg.close(s.id), "close releases the live lease");
        assert!(
            reg.find(s.id).is_none(),
            "lost handle → None, the recover signal"
        );
        assert!(!reg.close(s.id), "double-close is idempotent, not an error");
    }

    // what this catches: THE SELF-HEALING SEAM — ensure reuses a live handle, but
    // when the prior handle is gone (node down / closed) it re-homes onto a fresh
    // lease instead of failing. A lost handle never crashes the caller.
    #[test]
    fn ensure_reuses_live_else_rehomes() {
        let reg = InferenceSessionRegistry::new();
        let s = reg.open("m".into());
        let (same, reused) = reg.ensure(Some(s.id), "m".into());
        assert!(reused && same.id == s.id, "live handle is reused");

        reg.close(s.id); // node went down / lease lost
        let (fresh, reused2) = reg.ensure(Some(s.id), "m".into());
        assert!(
            !reused2 && fresh.id != s.id,
            "lost handle re-homes onto a fresh lease"
        );

        let (cold, reused3) = reg.ensure(None, "m".into());
        assert!(
            !reused3 && cold.id != fresh.id,
            "no prior handle → fresh lease"
        );
    }

    // what this catches: each open is a distinct lease (distinct handles), and the
    // registry tracks liveness count honestly.
    #[test]
    fn distinct_leases_tracked() {
        let reg = InferenceSessionRegistry::new();
        let a = reg.open("m".into());
        let b = reg.open("m".into());
        assert_ne!(a.id, b.id);
        assert_eq!(reg.len(), 2);
        reg.close(a.id);
        assert_eq!(reg.len(), 1);
    }
}
