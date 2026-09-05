    if let Some(room_name_raw) = std::env::var_os(AIRC_DEFAULT_ROOM_NAME_ENV) {
        let room_name = room_name_raw.to_string_lossy().trim().to_string();
        if !room_name.is_empty() {
            // Resolve the room name to a channel UUID
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

            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("room:") || trimmed.starts_with("Room:") || trimmed.starts_with("ROOM:") || trimmed.starts_with("current:") {
                    let parts: Vec<&str> = trimmed.split(':').collect();
                    if parts.len() == 2 && parts[1].trim().eq_ignore_ascii_case(&room_name) {
                        // Found the room name; now find the channel UUID
                        for channel_line in stdout.lines() {
                            let trimmed_channel = channel_line.trim();
                            if trimmed_channel.starts_with("channel:") || trimmed_channel.startswith("Channel:") || trimmed_channel.startswith("CHANNEL:") {
                                let parts: Vec<&str> = trimmed_channel.split(':').collect();
                                if parts.len() == 2 {
                                    let uuid_str = parts[1].trim();
                                    return uuid_str.parse::<uuid::Uuid>().map_err(|e| {
                                        DiscoveryError::UnparseableChannel(format!(
                                            "channel: {} is not a valid UUID: {}",
                                            uuid_str, e
                                        ))
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }