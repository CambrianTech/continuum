#![allow(unused_variables)]
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{timeout, Duration};
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

struct StubAircCitizen;

#[async_trait]
impl StubAircCitizen {
    async fn subscribe_all_rooms(&self) -> Result<()> {
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
                    return uuid_str.parse::<uuid::Uuid>().map_err(|e| {
                        DiscoveryError::UnparseableChannel(format!("channel: {} is not a valid UUID: {}", uuid_str, e))
                    });
                }
            }
        }
        // Fall back to the original behavior if the environment variable is not set.
        Err(DiscoveryError::NoChannelFound.into())
    }
}