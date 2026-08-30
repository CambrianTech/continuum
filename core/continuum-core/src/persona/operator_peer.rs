//! The operator's self-peer (#27) — the human's in-core airc presence.
//!
//! Until this existed, every room-scoped verb invoked without a persona
//! caller was DENIED ("the substrate-local operator has none in-core") — the
//! operator could dispatch work but could not read the rooms it ran in.
//! Diagnosing a comatose citizen took an hour of log archaeology because the
//! transcript she should have been reachable through was unreadable from the
//! operator seat (glass-boxed 2026-08-30; Joel: "Operator room issue is a
//! major bug").
//!
//! Shape: ONE durable identity per machine, kind [`IdentityKind::Human`],
//! labeled by the OS user, homed at `citizens/humans/<label>/airc/` — the
//! same runtime a persona boots (keypair, daemon attach, transcript/roster
//! readers) with NO service loop and NO registry row, so it can never be
//! picked by `any_live_citizen`, the reviewer resolver, the resumer, or any
//! other citizens-only path. Identity durability comes from the keypair on
//! disk: re-bootstrapping the same home resumes the same peer.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::persona::airc_runtime::PersonaAircRuntime;

static OPERATOR: OnceLock<Arc<PersonaAircRuntime>> = OnceLock::new();

/// The operator's label — the OS user, falling back to "operator" only when
/// the environment carries no user at all (containers).
fn operator_label() -> String {
    std::env::var("USER")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| "operator".to_string()) // unwrap_or: no $USER (container) = the neutral label, still durable per home dir
}

/// Boot (or resume) the operator self-peer. Idempotent; first success wins.
/// Called from the persona instance manager's start path — after the daemon
/// socket exists and the executor is installed, alongside citizen births.
pub async fn ensure_operator_peer(
    continuum_root: &Path,
    daemon_socket: PathBuf,
    executor: Arc<crate::runtime::command_executor::CommandExecutor>,
) {
    if OPERATOR.get().is_some() {
        return;
    }
    let label = operator_label();
    match PersonaAircRuntime::bootstrap_as(
        crate::identity::IdentityKind::Human,
        uuid::Uuid::new_v4(), // pre-mint id; the durable identity is the home keypair (post-collapse peer id wins)
        &label,
        continuum_root,
        daemon_socket,
        crate::persona::identity_provider::PersonaIdentitySource::ResumedFromDisk,
        executor,
    )
    .await
    {
        Ok(rt) => {
            let rt = Arc::new(rt);
            crate::probe!(
                class = "operator.peer.online",
                label = %label,
                peer_id = %rt.airc().peer_id(),
                "operator self-peer online — room-scoped verbs now act as the human, not a denial (#27)"
            );
            let _ = OPERATOR.set(rt);
        }
        Err(e) => {
            // Loud, not fatal: the substrate runs without an operator peer the
            // way it always has — verbs deny with the #27 message — but the
            // failure is a named probe, never silence.
            crate::probe!(
                class = "operator.peer.boot_failed",
                label = %label,
                error = %e.to_string(),
                "operator self-peer failed to boot — room verbs stay denied (#27 still open on this boot)"
            );
        }
    }
}

/// The operator's airc handle, when the self-peer is online.
pub fn operator_airc() -> Option<Arc<airc_lib::Airc>> {
    OPERATOR.get().map(|rt| rt.airc().clone())
}

/// The operator's runtime (transcript/roster readers), when online.
pub fn operator_runtime() -> Option<Arc<PersonaAircRuntime>> {
    OPERATOR.get().cloned()
}
