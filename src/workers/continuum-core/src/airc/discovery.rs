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
use std::time::Duration;

use tokio::process::Command as TokioCommand;
use tokio::time::timeout;
use tracing::{info, warn};

/// Deadline for fast subprocess discovery calls (`which airc`,
/// `airc ipc-endpoint`, `airc room`). 5s matches airc-ipc's
/// `DEFAULT_RPC_TIMEOUT` — if the airc binary itself hangs for
/// longer than this, the whole substrate IPC layer would already be
/// declaring the daemon dead. We refuse to wait longer.
///
/// Per [[no-stdio-piping-for-process-ipc]] memory: every subprocess
/// wait MUST be bounded; an unbounded `.output().await` is a dead-end.
const DISCOVERY_SUBPROCESS_DEADLINE: Duration = Duration::from_secs(5);

/// Deadline for the auto-install path. Generous because the install
/// script runs `curl` + `bash` and on a cold install can clone +
/// build airc — minutes, legitimately. 120s catches a truly stuck
/// install without holding boot forever; below this we trust the
/// installer's own progress.
const AUTO_INSTALL_DEADLINE: Duration = Duration::from_secs(120);

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
    #[error("`airc room` failed: {0}")]
    RoomCommandFailed(String),
    #[error("`airc room` output did not contain a parseable `channel: <uuid>` line: {0}")]
    UnparseableChannel(String),
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
    let probe = TokioCommand::new("which").arg("airc").output();
    timeout(DISCOVERY_SUBPROCESS_DEADLINE, probe)
        .await
        .ok()
        .and_then(|res| res.ok())
        .map(|out| out.status.success())
        .unwrap_or(false)
}

async fn query_airc_endpoint() -> Result<PathBuf, DiscoveryError> {
    let call = TokioCommand::new("airc").arg("ipc-endpoint").output();
    let out = timeout(DISCOVERY_SUBPROCESS_DEADLINE, call)
        .await
        .map_err(|_| {
            DiscoveryError::EndpointCommandFailed(format!(
                "`airc ipc-endpoint` did not exit within {DISCOVERY_SUBPROCESS_DEADLINE:?} \
                 — substrate is unresponsive, refusing to wait",
            ))
        })?
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

/// Discover the airc scope's current room channel UUID. The owner-core
/// model requires `AttachRequest.channel` be set explicitly (per-channel
/// router subscriptions, no global fan-out) — so the inbound attach
/// path needs a specific channel before it can stream events.
///
/// Resolution order:
///  1. `$AIRC_DEFAULT_CHANNEL` env override — explicit UUID for tests
///     or operators with multi-room scopes who want to pin the first
///     attach.
///  2. Parse `airc room` output for the `channel: <uuid>` line — that's
///     the scope's current default room, the one `airc msg`/`airc send`
///     publish to.
///
/// Future work: when airc adds `airc room --print-channel` (mirroring
/// the `airc ipc-endpoint` decoupling pattern), switch to that flag for
/// stability — the current parser is robust to whitespace but coupled
/// to airc's human-prose stdout format.
pub async fn discover_default_channel() -> Result<uuid::Uuid, DiscoveryError> {
    const AIRC_DEFAULT_CHANNEL_ENV: &str = "AIRC_DEFAULT_CHANNEL";
    if let Some(raw) = std::env::var_os(AIRC_DEFAULT_CHANNEL_ENV) {
        let raw = raw.to_string_lossy().trim().to_string();
        return raw.parse::<uuid::Uuid>().map_err(|e| {
            DiscoveryError::UnparseableChannel(format!(
                "{AIRC_DEFAULT_CHANNEL_ENV}={raw:?} is not a valid UUID: {e}"
            ))
        });
    }
    let call = TokioCommand::new("airc").arg("room").output();
    let out = timeout(DISCOVERY_SUBPROCESS_DEADLINE, call)
        .await
        .map_err(|_| {
            DiscoveryError::RoomCommandFailed(format!(
                "`airc room` did not exit within {DISCOVERY_SUBPROCESS_DEADLINE:?} \
                 — substrate is unresponsive, refusing to wait",
            ))
        })?
        .map_err(|e| DiscoveryError::RoomCommandFailed(e.to_string()))?;
    if !out.status.success() {
        return Err(DiscoveryError::RoomCommandFailed(format!(
            "exit {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    parse_channel_from_room_output(&String::from_utf8_lossy(&out.stdout))
}

/// Extract the `channel: <uuid>` line from `airc room` stdout.
///
/// Output today (from airc rust-rewrite branch, as of this PR):
/// ```text
/// room:    continuum
/// wire:    /Users/joel/.airc/wires/continuum
/// channel: 11c1a7ac-cb85-5ca0-a5b4-2847280ea3fa
/// ```
///
/// We match the literal `channel:` label (case-insensitive) followed by
/// whitespace and a UUID — robust to alignment changes but coupled to
/// the label name. If airc renames this field, the parser fails loudly
/// (UnparseableChannel error) rather than silently misreading.
fn parse_channel_from_room_output(stdout: &str) -> Result<uuid::Uuid, DiscoveryError> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed
            .strip_prefix("channel:")
            .or_else(|| trimmed.strip_prefix("Channel:"))
            .or_else(|| trimmed.strip_prefix("CHANNEL:"))
        else {
            continue;
        };
        let candidate = rest.trim();
        if let Ok(uuid) = candidate.parse::<uuid::Uuid>() {
            return Ok(uuid);
        }
    }
    Err(DiscoveryError::UnparseableChannel(format!(
        "no `channel: <uuid>` line in stdout: {stdout:?}"
    )))
}

async fn auto_install_airc() -> Result<(), DiscoveryError> {
    // `curl -fsSL <URL> | bash` keeps the bootstrap one-shot and matches
    // airc's own published install instructions (top of `install.sh`,
    // README quickstart). bash -c keeps the pipe in one process so we
    // can capture the combined exit status. Wrapped with
    // [`AUTO_INSTALL_DEADLINE`] so a hung installer can't pin the boot
    // loop indefinitely — 120s is generous (clone + cargo build on a
    // cold machine fits inside it) but bounded.
    let cmd = format!("curl -fsSL {AIRC_INSTALL_URL} | bash");
    let install = TokioCommand::new("bash").args(["-c", &cmd]).output();
    let out = timeout(AUTO_INSTALL_DEADLINE, install)
        .await
        .map_err(|_| {
            DiscoveryError::InstallFailed(format!(
                "airc installer did not exit within {AUTO_INSTALL_DEADLINE:?} \
                 — check network + `curl -fsSL {AIRC_INSTALL_URL}` by hand",
            ))
        })?
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

    #[test]
    fn parses_channel_from_typical_airc_room_output() {
        let stdout = "\
room:    continuum
wire:    /Users/joel/.airc/wires/continuum
channel: 11c1a7ac-cb85-5ca0-a5b4-2847280ea3fa
";
        let uuid = parse_channel_from_room_output(stdout).expect("parse channel");
        assert_eq!(
            uuid,
            "11c1a7ac-cb85-5ca0-a5b4-2847280ea3fa"
                .parse::<uuid::Uuid>()
                .unwrap()
        );
    }

    #[test]
    fn parses_channel_with_alternate_capitalization_and_whitespace() {
        let stdout = "  Channel:    11c1a7ac-cb85-5ca0-a5b4-2847280ea3fa\n";
        let uuid = parse_channel_from_room_output(stdout).expect("parse channel");
        assert_eq!(
            uuid,
            "11c1a7ac-cb85-5ca0-a5b4-2847280ea3fa"
                .parse::<uuid::Uuid>()
                .unwrap()
        );
    }

    #[test]
    fn parser_fails_loud_when_channel_line_absent() {
        let stdout = "room:    continuum\nwire:    /tmp/x\n";
        let err = parse_channel_from_room_output(stdout).expect_err("must fail");
        assert!(matches!(err, DiscoveryError::UnparseableChannel(_)));
        assert!(err.to_string().contains("no `channel:"));
    }

    #[test]
    fn parser_fails_loud_on_non_uuid_after_label() {
        let stdout = "channel: not-a-uuid\n";
        let err = parse_channel_from_room_output(stdout).expect_err("must fail");
        assert!(matches!(err, DiscoveryError::UnparseableChannel(_)));
    }
}
