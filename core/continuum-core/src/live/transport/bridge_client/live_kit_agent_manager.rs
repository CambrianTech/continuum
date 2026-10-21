use super::*;

impl LiveKitAgentManager {
    pub fn new() -> Self {
        #[cfg(unix)]
        let socket_dir = std::env::var("CONTINUUM_SOCKET_DIR").unwrap_or_else(|_| {
            dirs::home_dir()
                .map(|h| h.join(".continuum/sockets").to_string_lossy().to_string())
                .unwrap_or_else(|| "/tmp".to_string())
        });
        #[cfg(unix)]
        let bridge_socket_path = format!("{}/livekit-bridge.sock", socket_dir);
        #[cfg(windows)]
        let bridge_socket_path = continuum_bridge_protocol::WINDOWS_BRIDGE_ADDRESS.to_string();
        let livekit_url =
            std::env::var("LIVEKIT_URL").unwrap_or_else(|_| "ws://localhost:7880".to_string());

        Self {
            writer: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            media_tx: Mutex::new(None),
            out_channels: Mutex::new(std::collections::HashMap::new()),
            next_out_channel: AtomicU64::new(1),
            media_dropped: Arc::new(AtomicU64::new(0)),
            enqueue_tally: Mutex::new(super::super::pump_tally::PumpTally::new(std::time::Duration::from_secs(1))),
            drop_tally: Mutex::new(super::super::pump_tally::PumpTally::new(std::time::Duration::from_secs(1))),
            bridge_socket_path,
            livekit_url,
            next_request_id: AtomicU64::new(1),
            transcription_buffer: Arc::new(tokio::sync::Mutex::new(VecDeque::new())),
            reader_started: Mutex::new(false),
        }
    }

    pub fn url(&self) -> &str {
        &self.livekit_url
    }

    /// Ensure connected to bridge. Spawns reader thread on first connection.
    pub(super) fn ensure_connected(&self) -> Result<(), String> {
        let mut writer = self.writer.lock().unwrap();
        if writer.is_some() {
            return Ok(());
        }

        let stream = UnixStream::connect(&self.bridge_socket_path)
            .map_err(|e| format!("Bridge not available at {}: {}", self.bridge_socket_path, e))?;

        clog_info!(
            "🌉 Connected to livekit-bridge at {}",
            self.bridge_socket_path
        );

        // Clone for reader thread
        let reader_stream = stream
            .try_clone()
            .map_err(|e| format!("Failed to clone socket: {}", e))?;

        *writer = Some(stream);

        // Start reader thread (once)
        let mut started = self.reader_started.lock().unwrap();
        if !*started {
            *started = true;
            let pending = self.pending.clone();
            std::thread::spawn(move || {
                reader_loop(reader_stream, pending);
            });
        }
        drop(started);

        // (Re)start the media pump for this connection. Replacing the sender
        // disconnects the previous pump's receiver, so the old thread exits on
        // its own — one live pump per connection, no generation bookkeeping.
        let (tx, rx) = std::sync::mpsc::sync_channel::<MediaFrame>(MEDIA_QUEUE_DEPTH);
        *self.media_tx.lock().unwrap() = Some(tx);
        let pump_writer = Arc::clone(&self.writer);
        std::thread::spawn(move || {
            media_pump_loop(rx, pump_writer);
        });

        Ok(())
    }

    /// Fire-and-forget media-plane send: enqueue a wire-encoded frame for the
    /// pump thread and return immediately. Never blocks the caller on the
    /// socket, never waits for an ack (the bridge's response, if any, falls
    /// through `reader_loop`'s unclaimed-id path). A full queue DROPS the frame
    /// and counts it — the doctrine: media never starves the producer, and a
    /// drop is loud, never silent.
    ///
    /// Grid contract: this is the LOCAL media plane. A remote node never speaks
    /// this socket — it acquires a media handle via a command (grid calls are
    /// commands) and its bytes arrive through that handle's plane.
    /// Bind (once) the outbound binary channel for `(call, user, kind)`. The
    /// control round-trip happens on the FIRST frame of a stream; every frame
    /// after rides the binary plane with no identity and no JSON.
    pub(super) fn ensure_out_channel(
        &self,
        call_id: &str,
        user_id: &str,
        kind: continuum_bridge_protocol::MediaKind,
    ) -> Result<u16, String> {
        let key = (call_id.to_string(), user_id.to_string(), kind as u8);
        if let Some(ch) = self.out_channels.lock().unwrap().get(&key) {
            return Ok(*ch);
        }
        let channel = self.next_out_channel.fetch_add(1, Ordering::Relaxed) as u16;
        self.send_command(
            BridgeCommand::OpenMediaOut {
                channel,
                kind,
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
            },
            None,
        )?;
        self.out_channels.lock().unwrap().insert(key, channel);
        crate::probe!(
            class = "media.pump.channel_open",
            module = "livekit-bridge",
            channel = channel as u64,
            "outbound media channel bound — identity once, binary frames from here on"
        );
        Ok(channel)
    }

    pub(super) fn send_media(
        &self,
        kind: continuum_bridge_protocol::MediaKind,
        channel: u16,
        dims: Option<(u16, u16)>,
        payload: Vec<u8>,
        label: &'static str,
    ) -> Result<(), String> {
        self.ensure_connected()?;
        let tx_guard = self.media_tx.lock().unwrap();
        let Some(tx) = tx_guard.as_ref() else {
            return Err("Not connected".to_string());
        };
        let item = MediaFrame::new(kind, channel, dims, payload, label);
        let frame_bytes = item.wire_len();
        let kind = label;
        match tx.try_send(item) {
            Ok(()) => {
                // One summary row per second, not one per frame (12 pumps ×
                // 30 fps blinded the ledger — see pump_tally.rs).
                let summary = self.enqueue_tally.lock().unwrap().record(frame_bytes as u64, 0); // unwrap: tally mutex holds three counters; a poisoned lock here means a panic mid-record, and dropping the probe row is the honest outcome
                if let Some(s) = summary {
                    crate::probe!(
                        class = "media.pump.enqueue",
                        module = "livekit-bridge",
                        kind = kind,
                        frames = s.frames,
                        bytes = s.bytes,
                        span_ms = s.span_ms
                    );
                }
                Ok(())
            }
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                let n = self.media_dropped.fetch_add(1, Ordering::Relaxed) + 1;
                // One summary row per second, like enqueue/write — a saturated
                // pump dropped 12,327 frames in eight minutes on 2026-09-05 and
                // the per-frame rows rotated the voice receipt out of the ledger.
                if let Some(s) = self.drop_tally.lock().unwrap().record(frame_bytes as u64, 0) { // unwrap: three counters behind the lock; a poisoned tally loses one summary row, never a frame
                    crate::probe!(
                        class = "media.pump.drop",
                        module = "livekit-bridge",
                        kind = kind,
                        dropped = s.frames,
                        dropped_total = n,
                        span_ms = s.span_ms
                    );
                }
                if n == 1 || n % 32 == 0 {
                    clog_warn!(
                        "🌉 media pump full — {} frames dropped so far (latest: {})",
                        n,
                        kind
                    );
                }
                Ok(())
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                Err("Media pump not running".to_string())
            }
        }
    }

    /// Send command and wait for response (up to 30s).
    pub(super) fn send_command(
        &self,
        command: BridgeCommand,
        binary: Option<&[u8]>,
    ) -> Result<BridgeResponse, String> {
        self.ensure_connected()?;

        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);

        // Build envelope
        let mut envelope =
            serde_json::to_value(&command).map_err(|e| format!("Serialize error: {}", e))?;
        envelope
            .as_object_mut()
            .unwrap()
            .insert("request_id".to_string(), request_id.into());
        let json_bytes =
            serde_json::to_vec(&envelope).map_err(|e| format!("Serialize error: {}", e))?;
        let frame = continuum_bridge_protocol::encode_frame(&json_bytes, binary);

        // Register pending request
        let pending_req = Arc::new(PendingRequest {
            response: Mutex::new(None),
            signal: Condvar::new(),
        });
        self.pending
            .lock()
            .unwrap()
            .insert(request_id, pending_req.clone());

        // Write command
        {
            let mut writer = self.writer.lock().unwrap();
            if let Some(ref mut stream) = *writer {
                if let Err(e) = stream.write_all(&frame) {
                    *writer = None;
                    self.pending.lock().unwrap().remove(&request_id);
                    return Err(format!("Bridge write failed: {}", e));
                }
            } else {
                self.pending.lock().unwrap().remove(&request_id);
                return Err("Not connected".to_string());
            }
        }

        // Wait for response (30s timeout)
        let response = pending_req.response.lock().unwrap();
        let timeout = std::time::Duration::from_secs(30);
        let (mut guard, timed_out) = pending_req
            .signal
            .wait_timeout_while(response, timeout, |r| r.is_none())
            .unwrap();

        if timed_out.timed_out() {
            self.pending.lock().unwrap().remove(&request_id);
            return Err("Bridge command timed out after 30s".to_string());
        }

        guard
            .take()
            .ok_or_else(|| "No response received".to_string())
    }

    // =========================================================================
    // Public API — matches original LiveKitAgentManager interface
    // =========================================================================

    pub async fn join_as_listener(&self, call_id: &str) -> Result<(), String> {
        let resp = self.send_command(
            BridgeCommand::StartListener {
                call_id: call_id.to_string(),
            },
            None,
        )?;
        if resp.success {
            clog_info!(
                "🎤 STT listener started via bridge for {}",
                &call_id[..8.min(call_id.len())]
            );
            Ok(())
        } else {
            Err(resp.error.unwrap_or_else(|| "Bridge error".to_string()))
        }
    }

    pub async fn get_or_create_agent(
        &self,
        call_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<AgentHandle, String> {
        let resp = self.send_command(
            BridgeCommand::JoinRoom {
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
                display_name: display_name.unwrap_or(user_id).to_string(),
            },
            None,
        )?;
        if resp.success {
            let sid = resp
                .data
                .and_then(|d| {
                    d.get("audio_track_sid")
                        .and_then(|s| s.as_str().map(|s| s.to_string()))
                })
                .unwrap_or_default();
            Ok(AgentHandle {
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
                audio_track_sid: sid,
            })
        } else {
            Err(resp.error.unwrap_or_else(|| "Bridge error".to_string()))
        }
    }

    pub async fn remove_agent(&self, call_id: &str, user_id: &str) {
        let _ = self.send_command(
            BridgeCommand::LeaveRoom {
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
            },
            None,
        );
    }

    pub async fn remove_agents_for_call(&self, call_id: &str) {
        let _ = self.send_command(
            BridgeCommand::LeaveAllAgents {
                call_id: call_id.to_string(),
            },
            None,
        );
    }

    pub async fn remove_listener(&self, call_id: &str) {
        let _ = self.send_command(
            BridgeCommand::StopListener {
                call_id: call_id.to_string(),
            },
            None,
        );
    }

    pub async fn speak_in_call(
        &self,
        call_id: &str,
        user_id: &str,
        text: &str,
        voice: Option<&str>,
        adapter: Option<&str>,
        display_name: Option<&str>,
    ) -> Result<(Vec<i16>, u64, u32), String> {
        use crate::live::audio::tts_service;
        use crate::live::avatar::gender::gender_from_identity;
        use crate::live::avatar::types::AvatarGender;

        // Ensure agent exists in bridge
        let _ = self
            .get_or_create_agent(call_id, user_id, display_name)
            .await?;

        // TTS runs HERE in core (uses ort — safe, no webrtc in this process)
        // Name-anchored gender first so the voice matches the persona's NAME +
        // avatar ([[procedural-persona-genesis]]); id-hash only as a fallback.
        let gender = crate::live::avatar::selection::registered_gender(user_id)
            .unwrap_or_else(|| gender_from_identity(user_id));
        let gender_str = match gender {
            AvatarGender::Male => "male",
            AvatarGender::Female => "female",
            // Neuter (they/them): pass "neutral" — resolve_voice_gendered picks a
            // neutral-tagged voice if the backend has one, else falls through to an
            // identity-seeded pick from the full pool (any voice is coherent with
            // they/them). Forward-compatible with neuter voices as they're added.
            AvatarGender::Neutral => "neutral",
        };

        // Gap #2 ([[procedural-persona-genesis]]): when the persona has no explicit
        // voice, seed the voice pick from its IDENTITY — NOT the literal "default",
        // which hashes to ONE voice and collapses every same-gender persona onto it.
        // The TTS resolver treats an arbitrary string as a gender-filtered per-voice
        // SEED, so `user_id` is exactly the right seed: a stable, UNIQUE voice per
        // persona, coherent with the gender + avatar (all drawn from the same id).
        let voice_seed = match voice {
            Some(v) if !v.is_empty() && v != "default" => v,
            _ => user_id,
        };
        let synthesis =
            tts_service::synthesize_speech_async(text, Some(voice_seed), adapter, Some(gender_str))
                .await
                .map_err(|e| format!("TTS synthesis failed: {}", e))?;

        let num_samples = synthesis.samples.len();
        let duration_ms = synthesis.duration_ms;
        let sample_rate = synthesis.sample_rate;

        // Publish subtitle BEFORE audio
        let _ = self.send_command(
            BridgeCommand::PublishTranscription {
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
                text: text.to_string(),
                track_sid: String::new(),
                r#final: true,
            },
            None,
        );

        // Bevy animation (stays in core). No renderer without `avatar-3d` — nothing
        // to animate, so the call compiles away with it.
        #[cfg(feature = "avatar-3d")]
        self.trigger_speech_animation(user_id, text, &synthesis.samples, sample_rate, duration_ms);

        // Send PCM audio to bridge for LiveKit publishing — BINARY media
        // plane, fire-and-forget: identity bound once, payload moved (never
        // copied), the speak turn never blocks behind the bridge.
        let _ = num_samples;
        let channel = self.ensure_out_channel(
            call_id,
            user_id,
            continuum_bridge_protocol::MediaKind::AudioPcm,
        )?;
        self.send_media(
            continuum_bridge_protocol::MediaKind::AudioPcm,
            channel,
            None,
            pcm_le_bytes(&synthesis.samples),
            "speak-pcm",
        )?;

        // Return the synthesized PCM so the caller can tee it into the native call
        // plane (#193 audio convergence) — the same samples that just went to LiveKit.
        Ok((synthesis.samples, duration_ms, sample_rate))
    }

    pub async fn inject_audio(
        &self,
        call_id: &str,
        user_id: &str,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        let channel = self.ensure_out_channel(
            call_id,
            user_id,
            continuum_bridge_protocol::MediaKind::AudioPcm,
        )?;
        self.send_media(
            continuum_bridge_protocol::MediaKind::AudioPcm,
            channel,
            None,
            pcm_le_bytes(&samples),
            "inject-audio",
        )
    }

    /// Publish an RGBA avatar frame through a persona's LiveKit video track.
    ///
    /// The bridge lazily creates the `NativeVideoSource` + `LocalVideoTrack` on
    /// the first frame (see `Agent::publish_video_frame`), so no explicit track
    /// setup is needed here — the pump just streams frames as the Bevy renderer
    /// produces them. RGBA→I420 conversion happens bridge-side (the only process
    /// that links webrtc). Binary payload = raw RGBA bytes; the bridge trusts
    /// `width`/`height` to interpret them.
    pub async fn publish_video_frame(
        &self,
        call_id: &str,
        user_id: &str,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        // BINARY plane: [w][h] ride as a 4-byte header part; the ~3.7MB RGBA
        // payload is MOVED end-to-end — pump → queue → kernel, zero copies
        // (30fps × N personas forbids any other answer).
        let channel = self.ensure_out_channel(
            call_id,
            user_id,
            continuum_bridge_protocol::MediaKind::VideoRgba,
        )?;
        self.send_media(
            continuum_bridge_protocol::MediaKind::VideoRgba,
            channel,
            Some((width as u16, height as u16)),
            rgba,
            "video-frame",
        )
    }

    pub async fn add_ambient_source(
        &self,
        call_id: &str,
        source_name: &str,
    ) -> Result<String, String> {
        let resp = self.send_command(
            BridgeCommand::AddAmbient {
                call_id: call_id.to_string(),
                source_name: source_name.to_string(),
            },
            None,
        )?;
        if resp.success {
            Ok(resp
                .data
                .and_then(|d| {
                    d.get("handle")
                        .and_then(|h| h.as_str().map(|s| s.to_string()))
                })
                .unwrap_or_else(|| format!("ambient-{}", call_id)))
        } else {
            Err(resp.error.unwrap_or_default())
        }
    }

    pub async fn inject_ambient(
        &self,
        call_id: &str,
        handle: &str,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        // Ambient stays on the COMMAND plane: the bridge's ambient sink is a
        // no-op ack today (server.rs dispatches InjectAmbient to `{"ack"}`),
        // so this is not a live per-frame path. When the ambient mixer gets
        // built bridge-side it joins the binary plane with its own MediaKind —
        // do NOT stream a real 50fps bed through here.
        let pcm_bytes = pcm_le_bytes(&samples);
        self.send_command(
            BridgeCommand::InjectAmbient {
                call_id: call_id.to_string(),
                handle: handle.to_string(),
                sample_count: samples.len() as u32,
            },
            Some(&pcm_bytes),
        )
        .map(|_| ())
    }

    pub async fn remove_ambient_source(&self, call_id: &str, handle: &str) -> Result<(), String> {
        let resp = self.send_command(
            BridgeCommand::RemoveAmbient {
                call_id: call_id.to_string(),
                handle: handle.to_string(),
            },
            None,
        )?;
        if resp.success {
            Ok(())
        } else {
            Err(resp.error.unwrap_or_default())
        }
    }

    pub async fn start_ambient_audio(&self, call_id: &str) -> Result<(), String> {
        let resp = self.send_command(
            BridgeCommand::AddAmbient {
                call_id: call_id.to_string(),
                source_name: "rain".to_string(),
            },
            None,
        )?;
        if resp.success {
            Ok(())
        } else {
            Err(resp.error.unwrap_or_default())
        }
    }

    pub async fn poll_transcriptions(&self, call_id: Option<&str>) -> Vec<TranscriptionEntry> {
        let mut buf = self.transcription_buffer.lock().await;
        if let Some(cid) = call_id {
            let (matching, remaining): (VecDeque<_>, VecDeque<_>) =
                buf.drain(..).partition(|e| e.call_id == cid);
            *buf = remaining;
            matching.into_iter().collect()
        } else {
            buf.drain(..).collect()
        }
    }

    #[cfg(feature = "avatar-3d")]
    pub(super) fn trigger_speech_animation(
        &self,
        user_id: &str,
        text: &str,
        samples: &[i16],
        sample_rate: u32,
        duration_ms: u64,
    ) {
        if let Some(bevy_system) = crate::live::video::bevy_renderer::try_get() {
            use crate::live::session::sentiment::extract_sentiment;
            use crate::live::video::bevy_renderer::SpeechAnimationClip;

            let sentiment = extract_sentiment(text);
            let lip_sync_window_ms = 66u32;
            let mouth_weights = calculate_rms_weights(samples, sample_rate, lip_sync_window_ms);

            if sentiment.emotion != crate::live::video::avatar_types::Emotion::Neutral {
                bevy_system.set_emotion_by_identity(
                    user_id,
                    sentiment.emotion,
                    sentiment.intensity,
                    300,
                );
            }
            if sentiment.gesture != crate::live::video::avatar_types::Gesture::None {
                bevy_system.set_gesture_by_identity(user_id, sentiment.gesture, 2000);
            }
            bevy_system.play_speech_by_identity(
                user_id,
                SpeechAnimationClip {
                    mouth_weights,
                    interval_ms: lip_sync_window_ms,
                    duration_ms,
                },
            );
        }
    }
}

impl Default for LiveKitAgentManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Lightweight handle — actual agent lives in bridge process.
pub struct AgentHandle {
    pub call_id: String,
    pub user_id: String,
    pub audio_track_sid: String,
}

// =============================================================================
// Reader thread — receives responses + pushed events from bridge
// =============================================================================
/// Media-plane pump: drain the bounded queue onto the shared socket writer.
/// Runs on its own thread so a slow/stalled bridge back-pressures into DROPPED
/// frames (counted at enqueue) instead of blocking speak turns or the Bevy
/// render loop. Writes serialize through the SAME writer mutex as control
/// commands, so wire framing stays intact; each write is a short memcpy to the
/// kernel, never a wait-for-ack. Exits when the sender is replaced (reconnect)
/// or dropped (shutdown); a write error clears the shared writer so the next
/// caller's `ensure_connected` reconnects and spawns a fresh pump.
pub(super) fn media_pump_loop(
    rx: std::sync::mpsc::Receiver<MediaFrame>,
    writer: Arc<Mutex<Option<UnixStream>>>,
) {
    let mut tally = super::super::pump_tally::PumpTally::new(std::time::Duration::from_secs(1));
    while let Ok(item) = rx.recv() {
        let start = std::time::Instant::now();
        let mut guard = writer.lock().unwrap();
        match guard.as_mut() {
            Some(stream) => {
                // Parts back-to-back — the payload was MOVED here, never
                // copied into a combined frame (30fps HD forbids it).
                let write = stream
                    .write_all(&item.header)
                    .and_then(|_| match &item.dims {
                        Some(d) => stream.write_all(d),
                        None => Ok(()),
                    })
                    .and_then(|_| stream.write_all(&item.payload));
                if let Err(e) = write {
                    clog_warn!("🌉 media pump write failed ({}): {}", item.kind, e);
                    *guard = None;
                    return;
                }
                drop(guard);
                // Timing per write: lock wait + kernel copy together. Steady
                // state is tens of µs; a growing write_us trend means the
                // bridge socket buffer is backing up — the drop counter's
                // leading indicator.
                if let Some(s) = tally.record(item.wire_len() as u64, start.elapsed().as_micros() as u64) {
                    crate::probe!(
                        class = "media.pump.write",
                        module = "livekit-bridge",
                        kind = item.kind,
                        frames = s.frames,
                        bytes = s.bytes,
                        max_write_us = s.max_us,
                        span_ms = s.span_ms
                    );
                }
            }
            None => {
                // Connection died under us — this pump's generation is over.
                return;
            }
        }
    }
}

/// One bound INBOUND media channel: identity strings held once as `Arc<str>`
/// so the per-frame path clones two pointers, never bytes.
pub(super) struct InMediaBinding {
    pub(super) kind: continuum_bridge_protocol::MediaKind,
    pub(super) call_id: std::sync::Arc<str>,
    pub(super) speaker_id: std::sync::Arc<str>,
    pub(super) speaker_name: std::sync::Arc<str>,
    pub(super) frames: u64,
}
