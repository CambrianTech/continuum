#![allow(unused_variables)]
use uuid::Uuid;

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use uuid::Uuid;

/// Timeout for discovery subprocesses.
const DISCOVERY_SUBPROCESS_DEADLINE: Duration = Duration::from_secs(5);

/// Deadline for the auto-install path. Generous because the install
/// script runs `curl` + `bash` and on a cold install can clone +
/// build airc — minutes, legitimately. 120s catches a truly stuck
/// install; it now bounds the **detached background task**
/// (`discover_airc_socket` spawns the install and fails fast), so boot
/// NEVER waits on it — below this we trust the installer's own progress.
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
pub(crate) const AIRC_DAEMON_SOCKET_ENV: &str = "AIRC_DAEMON_SOCKET";

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("airc binary not found on PATH and auto-install failed: {0}")]
    InstallFailed(String),
    #[error("auto-install suppressed via {AIRC_DISABLE_AUTOINSTALL}=1 — install airc manually: curl -fsSL {AIRC_INSTALL_URL} | bash")]
    AutoInstallDisabled,
    #[error("airc not on PATH — bootstrapping it in the background; the node is UP (local commands work) but not yet a grid peer. Restart the core once the install completes to join airc (self-healing re-attach without restart is a follow-up).")]
    AutoInstallInProgress,
    #[error("`airc ipc-endpoint` failed: {0}")]
    EndpointCommandFailed(String),
    #[error("`airc ipc-endpoint` returned an empty path — airc binary may be from before #1095 (add the command or upgrade airc)")]
    EmptyPath,
    #[error("`airc room` failed: {0}")]
    RoomCommandFailed(String),
    #[error("`airc room` output did not contain a parseable `channel: <uuid>` line: {0}")]
    UnparseableChannel(String),
    #[error("daemon Status RPC failed: {0}")]
    PeerStatusFailed(String),
    #[error("daemon Status returned an unparseable peer_id ({0:?}): {1}")]
    UnparseablePeerId(String, uuid::Error),
}

struct TokioCommand {
    command: String,
    args: Vec<String>,
}

impl TokioCommand {
    fn new(command: &str) -> Self {
        Self {
            command: command.to_string(),
            args: vec![],
        }
    }

    fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_string());
        self
    }
}

impl TokioCommand {
    async fn output(&self) -> Result<String> {
        let output = tokio::process::Command::new(&self.command)
            .args(&self.args)
            .output()
            .await?;
        if !output.status.success() {
            anyhow::bail!("Command failed: {}", String::from_utf8_lossy(&output.stderr));
        }
        Ok(String::from_utf8(output.stdout)?)
    }
}

/// Extract the `channel: <uuid>` line from `airc room` stdout.
///
/// Output today (from airc rust-rewrite branch, as of this PR):
/// ```text
/// room:    continuum
/// wire:    ~/.airc/wires/<room>
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
    } else {
        println!("Channel is empty.");
    }

    // Fall back to the original behavior if the environment variable is not set.
        if let Some(room_name_raw) = std::env::var_os("AIRC_DEFAULT_ROOM_NAME_ENV") {
            let room_name = room_name_raw.to_string_lossy().trim().to_string();
            if !room_name.is_empty() {
                // Resolve the room name to a channel UUID
                let call = TokioCommand::new("airc").arg("room").output();
                let out = timeout(DISCOVERY_SUBPROCESS_DEADLINE, call)
                    .await
                    .map_err(|_| DiscoveryError::RoomDiscoveryFailed("Timed out while discovering room".into()))?;
                
                let trimmed_channel = out.trim();
                if trimmed_channel.is_empty() {
                    return Err(DiscoveryError::NoChannelFound.into());
                }

                // Attempt to parse the channel as a UUID
                let parts: Vec<&str> = trimmed_channel.split(':').collect();
                if parts.len() == 2 {
                    let uuid_str = parts[1].trim();
                    return uuid_str.parse::<Uuid>().map_err(|e| {
                        DiscoveryError::UnparseableChannel(format!("channel: {} is not a valid UUID: {}", uuid_str, e))
                    });
                }
            }
        }
        // Fall back to the original behavior if the environment variable is not set.
        Err(DiscoveryError::NoChannelFound.into())
    }

    #[test]
    fn parses_channel_from_typical_airc_room_output() {
        let stdout = "\
room:    continuum
wire:    ~/.airc/wires/<room>
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
