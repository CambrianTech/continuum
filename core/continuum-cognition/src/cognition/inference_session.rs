//! Inference sessions — the per-persona lease a turn binds its model through.
//!
//! Per Joel ("always use commands"): inference is held through a HANDLE that keeps
//! the session alive (reused across turns, recoverable), never a one-shot HTTP call
//! ([[long-running-commands-are-handle-based]], [[compute-lease-boundary]]). The
//! command surface for handles is `ai/inference/{open,generate,close,inspect}` in
//! `continuum-engine`'s `inference/handle_module.rs`; this module is the per-persona
//! lease registry `generate_response` uses, plus [`resolve_model`].

use continuum_base::time::EpochMs;
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::sdk_codegen::CommandError;

/// A live inference lease: a handle bound to a concrete model. Held in the global
/// registry so it survives across turns (keep-alive) and can be re-found if a caller
/// loses its reference. Token generation delegates to the resolved provider adapter
/// (next slice); the session itself is the lease record + identity.
#[derive(Debug, Clone, Serialize, TS)]
pub struct InferenceSession {
    #[ts(type = "string")]
    pub id: Uuid,
    /// The concrete model this lease is bound to (what the engine serves).
    pub model: String,
    #[ts(type = "number")]
    pub opened_at_ms: u64,
}

/// Process-global registry of live inference sessions — the ONE place a handle's
/// liveness is tracked.
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
    fn open(&self, model: String) -> Arc<InferenceSession> {
        let session = Arc::new(InferenceSession {
            id: Uuid::new_v4(),
            model,
            opened_at_ms: EpochMs::now().millis(),
        });
        self.sessions.insert(session.id, session.clone());
        session
    }

    /// Recover a handle by id — `Some` if the lease is still live, `None` if it was
    /// lost (the caller then re-opens / re-resolves; never crashes).
    fn find(&self, id: Uuid) -> Option<Arc<InferenceSession>> {
        self.sessions.get(&id).map(|e| e.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: THE PER-PERSONA LEASE — a persona keeps ONE established
    // lease across turns (the second ensure reuses it), and two personas never
    // share a handle.
    #[test]
    fn ensure_for_persona_reuses_one_lease_per_persona() {
        let reg = InferenceSessionRegistry::new();
        let (aiko, aris) = (Uuid::new_v4(), Uuid::new_v4());
        let first = reg.ensure_for_persona(aiko, "m".into());
        let again = reg.ensure_for_persona(aiko, "other".into());
        assert_eq!(first.id, again.id, "a live lease is reused across turns");
        assert_eq!(again.model, "m", "reuse keeps the bound model");
        assert_eq!(reg.persona_session(aiko).map(|s| s.id), Some(first.id));
        assert!(reg.persona_session(aris).is_none(), "no lease until ensured");
        assert_ne!(reg.ensure_for_persona(aris, "m".into()).id, first.id);
    }
}
