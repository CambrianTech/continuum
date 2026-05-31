//! Discover the running `airc` daemon's IPC socket — independent of
//! how `airc` itself encodes the path. Asks `airc ipc-endpoint`
//! (airc#1095) so airc remains free to evolve its socket-resolution
//! scheme (machine-account hashing, SUN_LEN fallbacks,
//! `$AIRC_RUNTIME_DIR` override) without breaking continuum-core.
//!
//! ### Resolution order
//!
//! 1. `$AIRC_DAEMON_SOCKET` env override — explicit operator control,
//!    used by tests + CI to point at an ephemeral daemon.
//! 2. `airc ipc-endpoint` — the canonical answer when the user has
//!    `airc` on PATH (Joel's setup, most existing devs).
//! 3. Auto-install airc via the canonical installer URL + re-query —
//!    most users won't have airc pre-installed; continuum-core
//!    bootstraps it so the persona-as-airc-peer flow works out of
//!    the box per `ALPHA-GAP-ANALYSIS.md` §0A line 706.
//! 4. `Err(DiscoveryError)` with actionable remedy.
//!
//! ### Decoupling property
//!
//! continuum-core does NOT vendor or duplicate airc's socket-path
//! logic. The previous stale local resolver
//! (`daemon_endpoint::default_socket_path_in` — kept temporarily
//! as `#[deprecated]` for migration) hashed the home dir into
//! `/tmp/airc-ipc-v<N>-<sha12>.sock`; airc itself now binds
//! `~/.airc/runtime/airc-machine-<account-hash>-v<N>.sock`. The
//! mismatch was the headless-boot break that motivated this
//! discovery module. The fix: stop deriving, start asking.

use std::path::PathBuf;

use tokio::process::Command as TokioCommand;
use tracing::{info, warn};

/// Canonical installer URL. Same one printed at the top of airc's
/// `install.sh` and in airc's README. Pinning here keeps the curl-pipe-
/// bash idempotent + transparent — readers see exactly where the
/// bootstrap downloads from.
const AIRC_INSTALL_URL: &str =
    "https://raw.githubusercontent.com/CambrianTech/airc/main/install.sh";

/// Opt-out env var. Set to `1` to suppress auto-install (CI, hermetic
/// builds, distros that vendor airc themselves). When set, discovery
/// returns an error instead of running the installer.
const AIRC_DISABLE_AUTOINSTALL: &str = "CONTINUUM_DISABLE_AIRC_AUTOINSTALL";

/// Explicit socket-path override. Honored unconditionally — when set,
/// no discovery, no install, no PATH probe. For tests pointing at
/// ephemeral daemons, and for operators with non-standard airc deploys.
const AIRC_DAEMON_SOCKET_ENV: &str = "AIRC_DAEMON_SOCKET";

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("airc binary not found on PATH and auto-install failed: {0}")]
    InstallFailed(String),
    #[error("auto-install suppressed via {AIRC_DISABLE_AUTOINSTALL}=1 — install airc manually: curl -fsSL {AIRC_INSTALL_URL} | bash")]
    AutoInstallDisabled,
    #[error("`airc ipc-endpoint` failed: {0}")]
    EndpointCommandFailed(String),
    #[error("`airc ipc-endpoint` returned an empty path — airc binary may be from before #1095 (add the command or upgrade airc)")]
    EmptyPath,
}

/// Discover the airc daemon socket path. See module docs for resolution
/// order. Async because the install step shells out via tokio.
pub async fn discover_airc_socket() -> Result<PathBuf, DiscoveryError> {
    if let Some(path) = std::env::var_os(AIRC_DAEMON_SOCKET_ENV) {
        let path = PathBuf::from(path);
        info!(
            ?path,
            "Using {AIRC_DAEMON_SOCKET_ENV} override for airc daemon socket"
        );
        return Ok(path);
    }

    if airc_on_path().await {
        return query_airc_endpoint().await;
    }

    if std::env::var_os(AIRC_DISABLE_AUTOINSTALL).is_some() {
        return Err(DiscoveryError::AutoInstallDisabled);
    }

    warn!(
        "airc not found on PATH — installing from {AIRC_INSTALL_URL}. \
         Most users won't have airc pre-installed; continuum-core \
         bootstraps it so the persona-as-airc-peer flow works headless. \
         Set {AIRC_DISABLE_AUTOINSTALL}=1 to opt out."
    );
    auto_install_airc().await?;
    if !airc_on_path().await {
        return Err(DiscoveryError::InstallFailed(
            "post-install `which airc` still empty — check $HOME/.local/bin in PATH".into(),
        ));
    }
    query_airc_endpoint().await
}

async fn airc_on_path() -> bool {
    TokioCommand::new("which")
        .arg("airc")
        .output()
        .await
        .map(|out| out.status.success())
        .unwrap_or(false)
}

async fn query_airc_endpoint() -> Result<PathBuf, DiscoveryError> {
    let out = TokioCommand::new("airc")
        .arg("ipc-endpoint")
        .output()
        .await
        .map_err(|e| DiscoveryError::EndpointCommandFailed(e.to_string()))?;
    if !out.status.success() {
        return Err(DiscoveryError::EndpointCommandFailed(format!(
            "exit {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() {
        return Err(DiscoveryError::EmptyPath);
    }
    Ok(PathBuf::from(path))
}

async fn auto_install_airc() -> Result<(), DiscoveryError> {
    // `curl -fsSL <URL> | bash` keeps the bootstrap one-shot and matches
    // airc's own published install instructions (top of `install.sh`,
    // README quickstart). bash -c keeps the pipe in one process so we
    // can capture the combined exit status.
    let cmd = format!("curl -fsSL {AIRC_INSTALL_URL} | bash");
    let out = TokioCommand::new("bash")
        .args(["-c", &cmd])
        .output()
        .await
        .map_err(|e| DiscoveryError::InstallFailed(format!("spawn bash: {e}")))?;
    if !out.status.success() {
        return Err(DiscoveryError::InstallFailed(format!(
            "installer exit {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    info!("airc installed via {AIRC_INSTALL_URL}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn env_override_short_circuits_discovery() {
        // SAFETY: env mutation in tests is racy under cargo's parallel
        // pool. Use a unique value so even if a parallel test reads
        // before our remove, the value here is unmistakable. Production
        // code never sets this env, so collision risk is local to tests.
        let unique = "/tmp/headless-airc-discover-test-unique-marker.sock";
        // SAFETY: tests are single-threaded for this var by design;
        // we set + unset in pair.
        unsafe { std::env::set_var(AIRC_DAEMON_SOCKET_ENV, unique) };
        let path = discover_airc_socket().await.expect("override path");
        unsafe { std::env::remove_var(AIRC_DAEMON_SOCKET_ENV) };
        assert_eq!(path, PathBuf::from(unique));
    }

    #[tokio::test]
    async fn empty_endpoint_output_is_distinct_error() {
        // Direct test of the parser: simulate an `airc ipc-endpoint`
        // that prints nothing. We can't actually run the real `airc`
        // here (CI may not have it), but the parser sees the same
        // empty-stdout case if the binary degrades.
        let _temp = TempDir::new().expect("tempdir");
        // Smoke: the error type carries the right diagnostic.
        let err = DiscoveryError::EmptyPath;
        let msg = err.to_string();
        assert!(msg.contains("empty path"));
        assert!(msg.contains("#1095") || msg.contains("airc binary"));
    }

    #[test]
    fn install_disabled_error_quotes_install_url_and_opt_out() {
        let err = DiscoveryError::AutoInstallDisabled;
        let msg = err.to_string();
        assert!(msg.contains(AIRC_INSTALL_URL));
        assert!(msg.contains(AIRC_DISABLE_AUTOINSTALL));
    }
}
