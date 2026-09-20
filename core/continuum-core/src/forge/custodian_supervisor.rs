//! `forge::custodian_supervisor` — lazy self-provisioning of the LOCAL forge
//! custodian sidecar.
//!
//! ## Why this exists (the autonomic gap it closes)
//! The L3 completion sentinel already fires on any completed training job and
//! dispatches `forge/export` to convert the MLX checkpoint into a pageable
//! gguf-lora gene. But `forge/export` speaks to a SEPARATE process — the forge
//! custodian binary (`forge::custodian_client`) — and until this module existed
//! that process had to be started by an OPERATOR. Glass-boxed 2026-07-20: an
//! organism-initiated `genome/job-create` trained cleanly, the sentinel fired on
//! completion, and the whole loop died at
//! `forge/export … custodian unreachable … NOT adopted` because nobody had
//! hand-started the sidecar. A self-improvement loop that needs a human to start
//! a converter is not autonomic ([[managed-product-everything-self-provisions-no-operator-steps]]).
//!
//! ## The fix: bring your own converter, on demand
//! Before dispatching an export, [`ensure_local_custodian`] health-checks the
//! sidecar; if it is UNREACHABLE it spawns the binary and blocks until `/health`
//! is ready. This mirrors the llama-server auto-spawn (#58): the substrate owns
//! its sidecars' lifecycle. Lazy, not eager — a MacBook-first substrate must not
//! hold a 200MB+ converter resident while idle; the first export pays a cold
//! start, every export after reuses the live sidecar (and it survives a core
//! reboot: the next core re-healths the same process and reuses it).
//!
//! ## Fail loud, heal only the healable
//! Only [`ForgeCustodianError::Unreachable`] triggers a spawn — a reachable but
//! unhealthy custodian ([`ForgeCustodianError::Api`]) is NOT a spawn problem and
//! surfaces loud ([[fallbacks-are-illegal-fail-loud]]). A spawn that never comes
//! up within [`READY_TIMEOUT`] fails loud rather than dispatching to a half-up
//! sidecar. This is the LOCAL custodian only; a grid custodian is never spawned
//! here — the local-http path is the sole caller.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::forge::custodian_client::{ForgeCustodian, ForgeCustodianError};

/// Serialize ensure-calls: two concurrent exports must not both try to spawn a
/// custodian (the second would lose the port-bind and exit, and we'd log a spurious
/// failure). The lock is held only for the spawn + wait-ready window.
static ENSURE_LOCK: Mutex<()> = Mutex::const_new(());

/// How long to wait for a freshly-spawned custodian to answer `/health` before
/// giving up loud. Cold start = load the axum server + resolve the converter
/// tooling; seconds, not minutes. Deployment-tunable would be over-engineering —
/// this is a fixed local-process bring-up, not a substrate cognition threshold.
const READY_TIMEOUT: Duration = Duration::from_secs(60);

/// Poll cadence while waiting for the spawned sidecar to become ready.
const READY_POLL: Duration = Duration::from_millis(500);

/// Resolve the custodian binary path, most-specific first:
/// 1. `FORGE_CUSTODIAN_BIN` config override (an explicit deployment choice).
/// 2. A `forge-custodian` sibling of the running core executable — the normal
///    case: core + sidecar are built into the same target dir / installed together.
/// 3. Bare `forge-custodian` on `PATH` (let the OS resolve at spawn).
///
/// Returns the first candidate that resolves; the PATH fallback is always a
/// candidate so a `$PATH`-installed binary works even off-tree.
fn resolve_custodian_bin() -> PathBuf {
    if let Some(p) = crate::config_env::read("FORGE_CUSTODIAN_BIN") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return pb;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(sib) = exe.parent().map(|d| d.join("forge-custodian")) {
            if sib.exists() {
                return sib;
            }
        }
    }
    PathBuf::from("forge-custodian")
}

/// Ensure the local forge custodian is up and healthy before an export is
/// dispatched. Cheap health check on the happy path; spawn-and-wait on
/// `Unreachable`; loud error on a reachable-but-unhealthy sidecar or a spawn that
/// never comes ready. Idempotent and race-safe (serialized under [`ENSURE_LOCK`]).
pub async fn ensure_local_custodian(client: &dyn ForgeCustodian) -> Result<(), String> {
    // Fast path: already reachable. An Api error means it's up but wrong — a
    // respawn cannot fix that, so surface it rather than spawning over it.
    match client.health().await {
        Ok(_) => return Ok(()),
        Err(ForgeCustodianError::Api(m)) => {
            return Err(format!(
                "forge custodian reachable but unhealthy (not a spawn problem): {m}"
            ));
        }
        Err(ForgeCustodianError::Unreachable(_)) => { /* spawn below */ }
    }

    // Serialize the spawn window. Re-check under the lock: a concurrent caller may
    // have already brought it up while we waited for the lock.
    let _guard = ENSURE_LOCK.lock().await;
    if client.health().await.is_ok() {
        return Ok(());
    }

    let bin = resolve_custodian_bin();
    tracing::info!(
        bin = %bin.display(),
        "forge custodian unreachable — self-provisioning the local sidecar (managed-product: no operator step)"
    );

    // Detach: the sidecar must OUTLIVE this call (and this core's lifetime) so the
    // next export — and the next core after a reboot — reuses it. Dropping the
    // Child handle leaves the process running; the custodian owns its own graceful
    // shutdown (R5). stdout/stderr to null: it logs to its own surface.
    std::process::Command::new(&bin)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| {
            format!(
                "failed to spawn forge custodian ({}): {e} — set FORGE_CUSTODIAN_BIN or build the forge-custodian binary",
                bin.display()
            )
        })?;

    // Wait for /health ready, bounded — never dispatch an export to a half-up
    // sidecar; fail loud on timeout instead.
    let start = Instant::now();
    loop {
        if client.health().await.is_ok() {
            tracing::info!("forge custodian self-provisioned and healthy");
            return Ok(());
        }
        if start.elapsed() >= READY_TIMEOUT {
            return Err(format!(
                "forge custodian spawned ({}) but did not become healthy within {READY_TIMEOUT:?}",
                bin.display()
            ));
        }
        tokio::time::sleep(READY_POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: bin resolution always yields a candidate (never panics /
    // never None) — the PATH fallback is the floor, so a caller off-tree still gets
    // `forge-custodian` to hand the OS. Regression guard for the self-provision path
    // (glass-boxed 2026-07-20: sentinel died at custodian-unreachable).
    #[test]
    fn resolve_custodian_bin_always_returns_a_candidate() {
        let bin = resolve_custodian_bin();
        // Either an existing sibling/override, or the bare PATH name.
        assert!(
            bin.exists() || bin == PathBuf::from("forge-custodian"),
            "expected an existing binary or the bare PATH fallback, got {}",
            bin.display()
        );
    }

    // what this catches: a reachable-but-unhealthy custodian (Api error) is NOT
    // healed by a respawn — ensure surfaces it loud instead of spawning over a
    // process that's up. The heal-only-the-healable contract.
    #[tokio::test]
    async fn api_error_is_not_healed_by_spawn() {
        use crate::forge::custodian_client::ForgeCustodianError;
        use crate::forge::protocol::{ExportResult, GgufLoraRequest, HealthResponse};
        use async_trait::async_trait;

        struct Unhealthy;
        #[async_trait]
        impl ForgeCustodian for Unhealthy {
            async fn health(&self) -> Result<HealthResponse, ForgeCustodianError> {
                Err(ForgeCustodianError::Api("contract mismatch".into()))
            }
            async fn export_gguf_lora(
                &self,
                _req: &GgufLoraRequest,
            ) -> Result<ExportResult, ForgeCustodianError> {
                unreachable!()
            }
        }
        let err = ensure_local_custodian(&Unhealthy)
            .await
            .expect_err("Api error must surface, not spawn");
        assert!(err.contains("not a spawn problem"), "got: {err}");
    }

    // what this catches: an already-healthy custodian is a no-op fast path — ensure
    // never spawns when one is up (idempotent reuse across exports and reboots).
    #[tokio::test]
    async fn healthy_custodian_is_a_noop() {
        use crate::forge::custodian_client::ForgeCustodianError;
        use crate::forge::protocol::{ExportResult, GgufLoraRequest, HealthResponse};
        use async_trait::async_trait;

        struct Healthy;
        #[async_trait]
        impl ForgeCustodian for Healthy {
            async fn health(&self) -> Result<HealthResponse, ForgeCustodianError> {
                Ok(HealthResponse::ok_gguf_lora())
            }
            async fn export_gguf_lora(
                &self,
                _req: &GgufLoraRequest,
            ) -> Result<ExportResult, ForgeCustodianError> {
                unreachable!()
            }
        }
        ensure_local_custodian(&Healthy)
            .await
            .expect("a healthy custodian ensures cleanly with no spawn");
    }
}
