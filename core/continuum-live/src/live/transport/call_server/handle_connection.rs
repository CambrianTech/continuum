use super::*;

/// Handle a single WebSocket connection
pub(super) async fn handle_connection(stream: TcpStream, addr: SocketAddr, manager: Arc<CallManager>) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            clog_error!("WebSocket handshake failed for {}: {}", addr, e);
            return;
        }
    };

    clog_info!("New WebSocket connection from {}", addr);

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let mut participant_handle: Option<Handle> = None;
    let mut is_muted = false; // Track mute state at connection level

    // Channel for sending messages from audio receiver task
    let (msg_tx, mut msg_rx) = mpsc::channel::<Message>(64);

    // Spawn task to forward messages to WebSocket
    let sender_task = tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Main message loop
    loop {
        tokio::select! {
            // Receive message from WebSocket
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<CallMessage>(&text) {
                            Ok(CallMessage::Join { call_id, user_id, display_name, is_ai }) => {
                                // The call room IS the airc room: a call_id that isn't an airc
                                // RoomId is refused, not given a rogue parallel identity (#193).
                                let join = match manager.join_call(&call_id, &user_id, &display_name, is_ai).await {
                                    Ok(j) => j,
                                    Err(e) => {
                                        clog_error!("Join refused for call_id '{}': {}", call_id, e);
                                        let _ = msg_tx.send(Message::Text(
                                            serde_json::json!({ "type": "error", "error": e }).to_string().into()
                                        )).await;
                                        continue;
                                    }
                                };
                                let handle = join.handle;
                                let mut audio_rx = join.audio_rx;
                                let mut transcription_rx = join.transcription_rx;
                                let mut video_rx = join.video_rx;
                                let mut message_rx = join.message_rx;
                                participant_handle = Some(handle);

                                // Audio forwarding: SFU per-sender with sender_id in wire format
                                // Wire: [0x01 FrameKind::Audio][sender_id_len: u8][sender_id: UTF-8][PCM16 i16 LE]
                                // Same pattern as video — browser routes by senderId for A/V sync
                                let msg_tx_audio = msg_tx.clone();
                                tokio::spawn(async move {
                                    while let Ok((sender_handle, sender_user_id, audio)) = audio_rx.recv().await {
                                        // Mix-minus: skip our own audio frames
                                        if sender_handle != handle {
                                            let id_bytes = sender_user_id.as_bytes();
                                            let id_len = id_bytes.len().min(255) as u8;
                                            let mut bytes = Vec::with_capacity(1 + 1 + id_len as usize + audio.len() * 2);
                                            bytes.push(FrameKind::Audio as u8);
                                            bytes.push(id_len);
                                            bytes.extend_from_slice(&id_bytes[..id_len as usize]);
                                            bytes.extend(audio.iter().flat_map(|&s| s.to_le_bytes()));
                                            if msg_tx_audio.send(Message::Binary(bytes.into())).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                });

                                // Transcription forwarding (JSON text frames)
                                let msg_tx_transcription = msg_tx.clone();
                                let ws_display_name = display_name.clone();
                                tokio::spawn(async move {
                                    while let Ok(event) = transcription_rx.recv().await {
                                        clog_info!("[STEP 7] 🌐 WebSocket sending transcription to {}: \"{}\"",
                                            ws_display_name, event.text.chars().take(TEXT_PREVIEW_LENGTH).collect::<String>());
                                        let msg = CallMessage::Transcription {
                                            user_id: event.user_id,
                                            display_name: event.display_name,
                                            text: event.text,
                                            confidence: event.confidence,
                                            language: event.language,
                                        };
                                        if let Ok(json) = serde_json::to_string(&msg) {
                                            if msg_tx_transcription.send(Message::Text(json.into())).await.is_err() {
                                                clog_warn!("[STEP 7] ❌ WebSocket send FAILED for {}", ws_display_name);
                                                break;
                                            }
                                        }
                                    }
                                });

                                // Video forwarding: mix-minus (see everyone but yourself)
                                // Wire format: [0x02 FrameKind::Video][sender_id_len: u8][sender_id: UTF-8][VideoFrameHeader 16b][pixels]
                                let msg_tx_video = msg_tx.clone();
                                tokio::spawn(async move {
                                    while let Ok((sender_handle, sender_user_id, video_data)) = video_rx.recv().await {
                                        // Mix-minus: skip our own video frames
                                        if sender_handle != handle {
                                            let id_bytes = sender_user_id.as_bytes();
                                            let id_len = id_bytes.len().min(255) as u8;
                                            let mut frame = Vec::with_capacity(1 + 1 + id_len as usize + video_data.len());
                                            frame.push(FrameKind::Video as u8);
                                            frame.push(id_len);
                                            frame.extend_from_slice(&id_bytes[..id_len as usize]);
                                            frame.extend_from_slice(&video_data);
                                            if msg_tx_video.send(Message::Binary(frame.into())).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                });

                                // General message forwarding (avatar updates, video config, etc.)
                                let msg_tx_messages = msg_tx.clone();
                                tokio::spawn(async move {
                                    while let Ok(call_msg) = message_rx.recv().await {
                                        if let Ok(json) = serde_json::to_string(&call_msg) {
                                            if msg_tx_messages.send(Message::Text(json.into())).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                });
                            }
                            Ok(CallMessage::Leave) => {
                                if let Some(handle) = participant_handle.take() {
                                    manager.leave_call(&handle).await;
                                }
                                break;
                            }
                            Ok(CallMessage::Audio { data }) => {
                                // Skip processing if muted at connection level
                                if is_muted {
                                    continue;
                                }
                                if let Some(handle) = &participant_handle {
                                    if let Some(samples) = base64_decode_i16(&data) {
                                        CallManager::note_human_audio_level("ws_json", &handle.short(), &samples);
                                        manager.push_audio(handle, samples).await;
                                    }
                                }
                            }
                            Ok(CallMessage::Mute { muted }) => {
                                is_muted = muted; // Track locally for this connection
                                if let Some(handle) = &participant_handle {
                                    manager.set_mute(handle, muted).await;
                                }
                                clog_info!("Connection mute state set: {}", muted);
                            }
                            Ok(CallMessage::VideoConfig { width, height, fps, format }) => {
                                clog_info!(
                                    "📹 Video config from {}: {}x{} @{}fps format={}",
                                    addr, width, height, fps, format
                                );
                                // Mark this call as having video enabled
                                if let Some(handle) = &participant_handle {
                                    let call_id = {
                                        let pc = manager.participant_calls.read().await;
                                        pc.get(handle).cloned()
                                    };
                                    if let Some(call_id) = call_id {
                                        let calls = manager.calls.read().await;
                                        if let Some(call) = calls.get(&call_id) {
                                            let mut call = call.write().await;
                                            call.has_video = true;
                                        }
                                    }
                                }
                            }
                            Ok(_) => {
                                // Ignore other message types from client
                            }
                            Err(e) => {
                                clog_warn!("Failed to parse message: {}", e);
                            }
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        // Binary frame protocol: first byte is FrameKind discriminator
                        if data.is_empty() || is_muted { continue; }
                        if let Some(handle) = &participant_handle {
                            match FrameKind::from_byte(data[0]) {
                                Some(FrameKind::Audio) => {
                                    // [0x01][PCM16 i16 LE bytes]
                                    let samples = bytes_to_i16(&data[1..]);
                                    manager.push_audio(handle, samples).await;
                                }
                                Some(FrameKind::Video) => {
                                    // [0x02][VideoFrameHeader 16 bytes][pixel data]
                                    manager.push_video(handle, data[1..].to_vec()).await;
                                }
                                Some(FrameKind::AvatarState) => {
                                    // Client should not send avatar state (server→client only)
                                    clog_warn!("Received AvatarState from client — ignored");
                                }
                                None => {
                                    // Legacy: no FrameKind prefix, treat entire payload as raw audio
                                    let samples = bytes_to_i16(&data);
                                    manager.push_audio(handle, samples).await;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Ok(_)) => {
                        // Ignore ping/pong
                    }
                    Some(Err(e)) => {
                        clog_error!("WebSocket error: {}", e);
                        break;
                    }
                }
            }
        }
    }

    // Cleanup
    if let Some(handle) = participant_handle {
        manager.leave_call(&handle).await;
    }

    clog_info!("WebSocket connection closed for {}", addr);
    sender_task.abort();
}

/// Start the WebSocket call server with an externally-created CallManager.
/// This allows the IPC server to share the same CallManager for direct audio injection.
pub async fn start_call_server(
    addr: &str,
    manager: Arc<CallManager>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(addr).await?;

    clog_info!("Call server listening on {}", addr);

    loop {
        let (stream, addr) = listener.accept().await?;
        let manager = manager.clone();
        tokio::spawn(handle_connection(stream, addr, manager));
    }
}
