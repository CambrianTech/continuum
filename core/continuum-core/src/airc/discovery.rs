#![allow(unused_variables)]
use uuid::Uuid;

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use uuid::Uuid;

/// Timeout for discovery subprocesses.
const DISCOVERY_SUBPROCESS_DEADLINE: Duration = Duration::from_secs(5);

#[derive(thiserror::Error, Debug)]
enum DiscoveryError {
    #[error("Failed to discover room: {0}")]
    RoomDiscoveryFailed(String),
    #[error("Channel {0} is not a valid UUID: {1}")]
    UnparseableChannel(String, uuid::UuidError),
    #[error("No channel found for room name")]
    NoChannelFound,
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
