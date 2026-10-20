use super::*;

#[async_trait]
impl ServiceModule for VoiceModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "live",
            priority: ModulePriority::Realtime,
            command_prefixes: &["voice/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    /// Receive the wired command executor at boot (the #224 late-bind slot). The
    /// perception-ingest drain reads it lazily to build the vision describer.
    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        self.state.executor.install(executor);
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Spawn idle watcher here (inside tokio runtime), not in VoiceState::new()
        self.state.resource_lifecycle.spawn_idle_watcher();
        // A hosted persona's room line becomes her VOICE while the room has a
        // live call (Joel, 2026-09-05: "they say they're talking … no output").
        super::super::voice_reply_speaker::spawn(ctx.bus.clone(), self.state.clone());
        // Safety net: detect orphaned sessions (browser crash, lost WebSocket, deploy)
        self.state.resource_lifecycle.spawn_orphan_watchdog();

        // #192: spawn the live-call perception-ingest drain. The bridge reader thread
        // posts decoded frames onto a process-global mpsc (perception_ingest); this
        // drain, on the runtime, fans each frame out to every persona-viewer's
        // PerceptionBuffer. Installed once — a second module init would get None.
        if let Some(rx) = crate::media::perception_ingest::install_channel() {
            tokio::spawn(perception_ingest_drain(rx, self.state.clone()));
        }
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);

        match command {
            "voice/register-session" => {
                let _timer = TimingGuard::new("module", "voice_register_session");
                let session_id = p.str("session_id")?;
                let room_id = p.str("room_id")?;
                let participants: Vec<VoiceParticipant> = p.json_or("participants");

                // #193 slice A — the server is AUTHORITATIVE that a call IS its airc room.
                // The airc `room_id` is THE canonical call/session key: the session
                // registers under it, and it becomes the LiveKit room name (listener,
                // ambient, agents below). A divergent client-minted `session_id` is a
                // LEGACY alias, recorded so every other voice/* verb can resolve it via
                // `canonical_call_id` until the client cutover
                // ([[all-rooms-are-airc-rooms-no-mirrors]], [[livekit-media-plane-rides-airc-not-parallel]]).
                if session_id != room_id {
                    // Record BEFORE the idempotency guard: a browser refresh mints a NEW
                    // session_id for the same room — the re-register dedupes on the
                    // canonical id below, but the fresh legacy id must still resolve.
                    self.state
                        .legacy_call_aliases
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(session_id.to_string(), room_id.to_string());
                    // clog_warn! auto-routes by module_path to the module's log file (live.log) —
                    // the SAME sink the orchestrator's register logs land in — so the divergence is
                    // observable next to the registration it describes. (log_warn!/tracing::warn!
                    // routed elsewhere and were invisible; #194 also masked this by shipping stale.)
                    //
                    // MERGE NOTE (canary ← #1957): both sides changed this block and both are
                    // wanted. canary added the alias RECORDING above (new behaviour, #193 slice A);
                    // #1957 changed only where this warn LANDS. Keeping canary's message — it
                    // describes the aliasing that now actually happens — routed through the PR's
                    // sink. Picking either side alone would have dropped real work.
                    crate::clog_warn!(
                        "#193: legacy divergent session_id {} aliased to room_id {} — call keyed by \
                         the airc room; client cutover pending (#193 slice B, this warn going \
                         silent is the done-signal).",
                        session_id,
                        room_id
                    );
                }
                let call_id = room_id;

                // Idempotency: skip if this call (keyed by the CANONICAL id) is already
                // registered. Browser refresh triggers a re-join which calls
                // register-session again — with a fresh client-minted session_id but the
                // same room. Without this guard, we spawn duplicate STT listeners and
                // agent batches, causing the reconnect churn cycle (agents connect → old
                // ones get evicted by LiveKit → new ones reconnect → repeat).
                {
                    let mut sessions = self.state.active_sessions.lock().unwrap();
                    if !sessions.insert(call_id.to_string()) {
                        log_info!(
                            "module",
                            "voice_register_session",
                            "Call {} already active — skipping duplicate registration",
                            &call_id[..8.min(call_id.len())]
                        );
                        return Ok(CommandResult::Json(
                            serde_json::json!({ "registered": true, "already_active": true, "call_id": call_id }),
                        ));
                    }
                }

                // Extract AI participant info BEFORE register_session consumes the vec
                let ai_participants: Vec<(String, String)> = participants
                    .iter()
                    .filter(|p| {
                        matches!(
                            p.participant_type,
                            crate::live::SpeakerType::Persona | crate::live::SpeakerType::Agent
                        )
                    })
                    .map(|p| (p.user_id.to_string(), p.display_name.clone()))
                    .collect();

                // Capture each persona's NAME-anchored gender keyed by its live
                // identity — the one point where identity + display name co-occur.
                // Every later avatar/voice selection resolves gender from this, so the
                // profile snapshot, live video pump, and voice all agree with the
                // visible NAME ([[procedural-persona-genesis]] coherence anchor).
                for (user_id, display_name) in &ai_participants {
                    crate::live::avatar::selection::register_persona_gender(user_id, display_name);
                }

                // Registered UNDER the room id — the session key IS the airc room (#193).
                self.state
                    .voice_service
                    .register_session(call_id, room_id, participants)?;

                // Track session for resource lifecycle (idle timeout unloading)
                self.state.resource_lifecycle.on_session_start();

                // CRITICAL: STT listener MUST connect first, before agents.
                // With 20+ agents all connecting simultaneously, LiveKit gets overwhelmed
                // (DTLS timeouts, pc_state failures). The STT listener is the most important
                // participant — without it, no speech → text → no AI responses.
                let livekit_manager = self.state.livekit_manager.clone();
                let listener_call_id = call_id.to_string();
                let ambient_manager = self.state.livekit_manager.clone();
                let ambient_call_id = call_id.to_string();

                tokio::spawn(async move {
                    // Phase 1: STT listener (highest priority — enables transcription)
                    if let Err(e) = livekit_manager.join_as_listener(&listener_call_id).await {
                        log_error!(
                            "module",
                            "voice_register_session",
                            "Failed to spawn STT listener for call {}: {}",
                            &listener_call_id[..8.min(listener_call_id.len())],
                            e
                        );
                    }
                    // Give STT listener time to establish WebRTC connection
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

                    // Phase 2: Ambient audio
                    if let Err(e) = ambient_manager.start_ambient_audio(&ambient_call_id).await {
                        log_error!(
                            "module",
                            "voice_register_session",
                            "Failed to start ambient audio for call {}: {}",
                            &ambient_call_id[..8.min(ambient_call_id.len())],
                            e
                        );
                    }
                });

                // Phase 3: Pre-allocate avatars + create agents (staggered)
                // Pre-allocate models for all participants at once (batch allocation
                // ensures unique models across the group — no duplicate green dinos).
                if !ai_participants.is_empty() {
                    let batch: Vec<(&str, Option<&str>)> = ai_participants
                        .iter()
                        .map(|(id, _)| (id.as_str(), None))
                        .collect();
                    crate::live::avatar::allocate_dynamic_batch(&batch);

                    let agent_manager = self.state.livekit_manager.clone();
                    let native_call_manager = self.state.call_manager.clone();
                    let agent_call_id = call_id.to_string();
                    tokio::spawn(async move {
                        // Wait for STT listener + ambient to finish connecting
                        tokio::time::sleep(std::time::Duration::from_millis(3000)).await;

                        // Stagger agent creation: 2s between each to avoid
                        // overwhelming LiveKit with concurrent WebRTC connections.
                        // With 20 agents × 2s = 40s to fully populate — acceptable since
                        // avatars appear progressively while STT works immediately.
                        for (user_id, display_name) in &ai_participants {
                            match agent_manager
                                .get_or_create_agent(&agent_call_id, user_id, Some(display_name))
                                .await
                            {
                                Ok(_) => {
                                    tracing::info!(
                                        "🎨 Pre-created agent for '{}' in call {}",
                                        display_name,
                                        &agent_call_id[..8.min(agent_call_id.len())]
                                    );

                                    // Stream this persona's Bevy-rendered avatar into
                                    // its LiveKit video track. Spawned (not awaited) so
                                    // the ~5s slot allocation runs in parallel with the
                                    // next agent's staggered creation. The pump owns its
                                    // slot guard and self-terminates (fail loud → drop
                                    // guard → recycle slot) when the agent is removed at
                                    // session end. See `spawn_avatar_video_pump`.
                                    let pump_manager = agent_manager.clone();
                                    let pump_call_manager = native_call_manager.clone();
                                    let pump_call_id = agent_call_id.clone();
                                    let pump_user_id = user_id.clone();
                                    let pump_display = display_name.clone();
                                    tokio::spawn(async move {
                                        if let Err(e) =
                                            crate::live::avatar::spawn_avatar_video_pump(
                                                pump_manager,
                                                pump_call_manager,
                                                pump_call_id,
                                                pump_user_id,
                                                pump_display.clone(),
                                            )
                                            .await
                                        {
                                            log_error!(
                                                "module",
                                                "voice_register_session",
                                                "Failed to start video pump for '{}': {}",
                                                pump_display,
                                                e
                                            );
                                        }
                                    });
                                }
                                Err(e) => {
                                    log_error!(
                                        "module",
                                        "voice_register_session",
                                        "Failed to pre-create agent for '{}': {}",
                                        display_name,
                                        e
                                    );
                                }
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
                        }
                    });
                }

                Ok(CommandResult::Json(
                    serde_json::json!({ "registered": true, "call_id": call_id }),
                ))
            }

            "voice/end-session" => {
                let _timer = TimingGuard::new("module", "voice_end_session");
                let call_id = self.state.canonical_call_id(p.str("session_id")?);

                // Remove from active sessions so a future register-session can proceed,
                // and drop any legacy aliases pointing at the ended call — a rejoin
                // records fresh ones.
                {
                    let mut sessions = self.state.active_sessions.lock().unwrap();
                    sessions.remove(&call_id);
                }
                {
                    let mut aliases = self
                        .state
                        .legacy_call_aliases
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    aliases.retain(|_, canonical| canonical != &call_id);
                }

                log_info!(
                    "module",
                    "voice_end_session",
                    "Ending call {} — cleaning up agents and listeners",
                    &call_id[..8.min(call_id.len())]
                );

                // Remove all LiveKit agents for this call
                self.state
                    .livekit_manager
                    .remove_agents_for_call(&call_id)
                    .await;

                // Remove the STT listener room
                self.state.livekit_manager.remove_listener(&call_id).await;

                // Track session end for resource lifecycle (triggers idle timeout).
                // Avatar models and audio adapters unload after the idle timeout
                // (default 60s) — NOT immediately, because:
                // 1. User might rejoin quickly (avoids expensive model reload)
                // 2. LiveKit agents need time to tear down cleanly
                self.state.resource_lifecycle.on_session_end();

                Ok(CommandResult::Json(
                    serde_json::json!({ "ended": true, "session_id": call_id }),
                ))
            }

            "voice/on-utterance" => {
                let _timer = TimingGuard::new("module", "voice_on_utterance");
                let mut event: UtteranceEvent = p.json("event")?;

                // Resolve a legacy client-minted session id to the canonical room id
                // (#193 slice A) — the orchestrator keys sessions by the airc room.
                event.session_id = self
                    .state
                    .canonical_call_id(&event.session_id.to_string())
                    .parse()
                    .map_err(|e| format!("canonical call id is not a UUID: {e}"))?;

                let responder_ids = self.state.voice_service.on_utterance(event)?;
                Ok(CommandResult::Json(serde_json::json!({
                    VOICE_RESPONSE_FIELD_RESPONDER_IDS: responder_ids.into_iter().map(|id| id.to_string()).collect::<Vec<String>>()
                })))
            }

            "voice/synthesize" => {
                let _timer = TimingGuard::new("module", "voice_synthesize");
                let text = p.str("text")?;
                let voice = p.str_opt("voice");
                let adapter = p.str_opt("adapter");

                use crate::live::audio::tts_service;
                let synthesis = tts_service::synthesize_speech_async(text, voice, adapter, None)
                    .await
                    .map_err(|e| {
                        log_error!("module", "voice_synthesize", "TTS failed: {}", e);
                        format!("TTS synthesis failed: {}", e)
                    })?;

                let pcm_bytes: Vec<u8> = synthesis
                    .samples
                    .iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect();

                log_info!(
                    "module",
                    "voice_synthesize",
                    "Synthesized {} samples at {}Hz ({:.1}s) → {} bytes raw PCM",
                    synthesis.samples.len(),
                    synthesis.sample_rate,
                    synthesis.duration_ms as f64 / 1000.0,
                    pcm_bytes.len()
                );

                Ok(CommandResult::Binary {
                    metadata: serde_json::json!({
                        "sample_rate": synthesis.sample_rate,
                        "num_samples": synthesis.samples.len(),
                        "duration_ms": synthesis.duration_ms,
                        "format": "pcm_i16_le"
                    }),
                    data: pcm_bytes,
                })
            }

            "voice/speak-in-call" => {
                let _timer = TimingGuard::new("module", "voice_speak_in_call");
                let call_id = self.state.canonical_call_id(p.str("call_id")?);
                let user_id = p.str("user_id")?;
                let text = p.str("text")?;
                let voice = p.str_opt("voice");
                let adapter = p.str_opt("adapter");
                let display_name = p.str_opt("display_name");
                // Timeline sequence number for output ordering.
                // Tells us WHERE in the conversation this response belongs.
                // TODO: Use for Rust-side TTS output scheduling (ordering + stale detection).
                let _timeline_seq = p.u64_opt("timeline_seq");

                let (num_samples, duration_ms, sample_rate) = self
                    .state
                    .speak_in_room(&call_id, user_id, text, voice, adapter, display_name)
                    .await
                    .map_err(|e| {
                        log_error!(
                            "module",
                            "voice_speak_in_call",
                            "Speak-in-call failed: {}",
                            e
                        );
                        format!("Speak-in-call failed: {}", e)
                    })?;

                log_info!(
                    "module",
                    "voice_speak_in_call",
                    "Injected {} samples ({:.1}s) into call {} for user {}",
                    num_samples,
                    duration_ms as f64 / 1000.0,
                    call_id,
                    user_id
                );
                Ok(CommandResult::Json(serde_json::json!({
                    "num_samples": num_samples,
                    "duration_ms": duration_ms,
                    "sample_rate": sample_rate,
                    "injected": true
                })))
            }

            "voice/synthesize-handle" => {
                let _timer = TimingGuard::new("module", "voice_synthesize_handle");
                let text = p.str("text")?;
                let voice = p.str_opt("voice");
                let adapter = p.str_opt("adapter");

                use crate::live::audio::tts_service;
                let synthesis = tts_service::synthesize_speech_async(text, voice, adapter, None)
                    .await
                    .map_err(|e| {
                        log_error!("module", "voice_synthesize_handle", "TTS failed: {}", e);
                        format!("TTS synthesis failed: {}", e)
                    })?;

                let adapter_name = adapter.unwrap_or("default");
                let info = self.state.audio_pool.store(
                    synthesis.samples,
                    synthesis.sample_rate,
                    synthesis.duration_ms,
                    adapter_name,
                );

                log_info!(
                    "module",
                    "voice_synthesize_handle",
                    "Stored handle {} ({} samples, {}ms, {})",
                    &info.handle[..8],
                    info.sample_count,
                    info.duration_ms,
                    info.adapter
                );
                Ok(CommandResult::Json(serde_json::json!({
                    "handle": info.handle,
                    "sample_count": info.sample_count,
                    "sample_rate": info.sample_rate,
                    "duration_ms": info.duration_ms,
                    "adapter": info.adapter,
                })))
            }

            "voice/play-handle" => {
                let _timer = TimingGuard::new("module", "voice_play_handle");
                let handle = p.str("handle")?;
                let call_id = self.state.canonical_call_id(p.str("call_id")?);
                let user_id = p.str("user_id")?;

                use crate::runtime::handle::Handle as VoiceHandle;
                let voice_handle: VoiceHandle = handle
                    .parse()
                    .map_err(|e| format!("Invalid handle UUID: {}", e))?;

                let samples = self.state.audio_pool.get(&voice_handle).ok_or_else(|| {
                    format!(
                        "Audio handle not found or expired: {}",
                        &handle[..8.min(handle.len())]
                    )
                })?;

                let sample_count = samples.len();
                let duration_ms =
                    (sample_count as u64 * 1000) / crate::audio_constants::AUDIO_SAMPLE_RATE as u64;

                self.state
                    .livekit_manager
                    .inject_audio(&call_id, user_id, samples)
                    .await
                    .map_err(|e| {
                        log_error!(
                            "module",
                            "voice_play_handle",
                            "Failed to inject audio: {}",
                            e
                        );
                        format!("Failed to inject audio: {}", e)
                    })?;

                log_info!(
                    "module",
                    "voice_play_handle",
                    "Played handle {} into call {} for user {} ({} samples, {}ms)",
                    &handle[..8],
                    call_id,
                    user_id,
                    sample_count,
                    duration_ms
                );
                Ok(CommandResult::Json(serde_json::json!({
                    "played": true,
                    "sample_count": sample_count,
                    "duration_ms": duration_ms
                })))
            }

            "voice/discard-handle" => {
                let handle = p.str("handle")?;

                use crate::runtime::handle::Handle as VoiceHandle;
                let voice_handle: VoiceHandle = handle
                    .parse()
                    .map_err(|e| format!("Invalid handle UUID: {}", e))?;

                let discarded = self.state.audio_pool.discard(&voice_handle);
                Ok(CommandResult::Json(
                    serde_json::json!({ "discarded": discarded }),
                ))
            }

            "voice/transcribe" => {
                let _timer = TimingGuard::new("module", "voice_transcribe");
                let audio = p.str("audio")?;
                let language = p.str_opt("language");

                use crate::live::audio::stt_service;
                use base64::Engine;

                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(audio)
                    .map_err(|e| {
                        log_error!("module", "voice_transcribe", "Base64 decode failed: {}", e);
                        format!("Base64 decode failed: {}", e)
                    })?;

                if bytes.len() % 2 != 0 {
                    return Err("Audio data must be even length (16-bit samples)".to_string());
                }

                let samples: Vec<i16> = bytes
                    .chunks_exact(2)
                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();

                log_info!(
                    "module",
                    "voice_transcribe",
                    "Transcribing {} samples ({:.1}s)",
                    samples.len(),
                    samples.len() as f64 / crate::audio_constants::AUDIO_SAMPLE_RATE as f64
                );

                let transcript = stt_service::transcribe_speech_async(&samples, language)
                    .await
                    .map_err(|e| {
                        log_error!("module", "voice_transcribe", "STT failed: {}", e);
                        format!("STT failed: {}", e)
                    })?;

                log_info!(
                    "module",
                    "voice_transcribe",
                    "Transcribed: \"{}\" (confidence: {:.2})",
                    transcript.text,
                    transcript.confidence
                );
                Ok(CommandResult::Json(serde_json::json!({
                    "text": transcript.text,
                    "language": transcript.language,
                    "confidence": transcript.confidence,
                    "segments": transcript.segments.iter().map(|s| {
                        serde_json::json!({
                            "text": s.text,
                            "start_ms": s.start_ms,
                            "end_ms": s.end_ms
                        })
                    }).collect::<Vec<_>>()
                })))
            }

            "voice/inject-audio" => {
                let _timer = TimingGuard::new("module", "voice_inject_audio");
                let call_id = self.state.canonical_call_id(p.str("call_id")?);
                let user_id = p.str("user_id")?;
                let samples: Vec<i16> = p.json("samples")?;

                self.state
                    .livekit_manager
                    .inject_audio(&call_id, user_id, samples)
                    .await
                    .map_err(|e| {
                        log_error!("module", "voice_inject_audio", "inject-audio failed: {}", e);
                        format!("inject-audio failed: {}", e)
                    })?;

                log_info!(
                    "module",
                    "voice_inject_audio",
                    "Injected audio into call {} for {}",
                    call_id,
                    user_id
                );
                Ok(CommandResult::Json(serde_json::json!({ "success": true })))
            }

            "voice/ambient-add" => {
                let _timer = TimingGuard::new("module", "voice_ambient_add");
                let call_id = self.state.canonical_call_id(p.str("call_id")?);
                let source_name = p.str("source_name")?;

                let handle = self
                    .state
                    .livekit_manager
                    .add_ambient_source(&call_id, source_name)
                    .await
                    .map_err(|e| {
                        log_error!("module", "voice_ambient_add", "ambient-add failed: {}", e);
                        format!("ambient-add failed: {}", e)
                    })?;

                log_info!(
                    "module",
                    "voice_ambient_add",
                    "Added ambient source '{}' to call {}",
                    source_name,
                    call_id
                );
                Ok(CommandResult::Json(serde_json::json!({
                    "handle": handle,
                    "source_name": source_name,
                })))
            }

            "voice/ambient-inject" => {
                let _timer = TimingGuard::new("module", "voice_ambient_inject");
                let call_id = self.state.canonical_call_id(p.str("call_id")?);
                let handle_str = p.str("handle")?;
                let samples: Vec<i16> = p.json("samples")?;

                self.state
                    .livekit_manager
                    .inject_ambient(&call_id, handle_str, samples)
                    .await
                    .map_err(|e| {
                        log_error!(
                            "module",
                            "voice_ambient_inject",
                            "ambient-inject failed: {}",
                            e
                        );
                        format!("ambient-inject failed: {}", e)
                    })?;

                Ok(CommandResult::Json(serde_json::json!({ "success": true })))
            }

            "voice/stt-list" => {
                let _timer = TimingGuard::new("module", "voice_stt_list");

                use crate::live::audio::stt;
                let registry = stt::get_registry();
                let reg = registry.read();

                let adapters: Vec<serde_json::Value> = reg
                    .list()
                    .iter()
                    .map(|(name, initialized)| {
                        let desc = reg
                            .get(name)
                            .map(|a| a.description().to_string())
                            .unwrap_or_default();
                        serde_json::json!({
                            "name": name,
                            "initialized": initialized,
                            "description": desc,
                        })
                    })
                    .collect();

                let active = reg
                    .get_active()
                    .map(|a| a.name().to_string())
                    .unwrap_or_default();

                Ok(CommandResult::Json(serde_json::json!({
                    "adapters": adapters,
                    "active": active,
                })))
            }

            "voice/transcribe-with-adapter" => {
                let _timer = TimingGuard::new("module", "voice_transcribe_with_adapter");
                let audio = p.str("audio")?;
                let language = p.str_opt("language");
                let adapter_name = p.str("adapter")?;

                use crate::live::audio::stt;
                use crate::live::audio::pcm::i16_to_f32;
                use base64::Engine;

                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(audio)
                    .map_err(|e| {
                        log_error!(
                            "module",
                            "voice_transcribe_with_adapter",
                            "Base64 decode failed: {}",
                            e
                        );
                        format!("Base64 decode failed: {}", e)
                    })?;

                if bytes.len() % 2 != 0 {
                    return Err("Audio data must be even length (16-bit samples)".to_string());
                }

                let samples: Vec<i16> = bytes
                    .chunks_exact(2)
                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();

                log_info!(
                    "module",
                    "voice_transcribe_with_adapter",
                    "Transcribing {} samples ({:.1}s) with adapter '{}'",
                    samples.len(),
                    samples.len() as f64 / crate::audio_constants::AUDIO_SAMPLE_RATE as f64,
                    adapter_name
                );

                // Initialize registry if needed
                if !stt::is_initialized() {
                    stt::init_registry();
                }

                // Get specific adapter by name
                let adapter = {
                    let registry = stt::get_registry();
                    let reg = registry.read();
                    reg.get(adapter_name).ok_or_else(|| {
                        format!(
                            "STT adapter '{}' not found. Available: {:?}",
                            adapter_name,
                            reg.list().iter().map(|(n, _)| *n).collect::<Vec<_>>()
                        )
                    })?
                };

                // Initialize if not yet ready
                if !adapter.is_initialized() {
                    adapter.initialize().await.map_err(|e| {
                        log_error!(
                            "module",
                            "voice_transcribe_with_adapter",
                            "Failed to initialize adapter '{}': {}",
                            adapter_name,
                            e
                        );
                        format!("Failed to initialize adapter '{}': {}", adapter_name, e)
                    })?;
                }

                let f32_samples = i16_to_f32(&samples);
                let transcript = adapter
                    .transcribe(f32_samples, language)
                    .await
                    .map_err(|e| {
                        log_error!(
                            "module",
                            "voice_transcribe_with_adapter",
                            "STT failed with adapter '{}': {}",
                            adapter_name,
                            e
                        );
                        format!("STT failed with adapter '{}': {}", adapter_name, e)
                    })?;

                log_info!(
                    "module",
                    "voice_transcribe_with_adapter",
                    "Transcribed with '{}': \"{}\" (confidence: {:.2})",
                    adapter_name,
                    transcript.text,
                    transcript.confidence
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "text": transcript.text,
                    "language": transcript.language,
                    "confidence": transcript.confidence,
                    "adapter": adapter_name,
                    "segments": transcript.segments.iter().map(|s| {
                        serde_json::json!({
                            "text": s.text,
                            "start_ms": s.start_ms,
                            "end_ms": s.end_ms
                        })
                    }).collect::<Vec<_>>()
                })))
            }

            "voice/test-audio-generate" => {
                let _timer = TimingGuard::new("module", "voice_test_audio_generate");
                let noise_type_str = p.str("noise_type")?;
                let duration_ms: u32 = p.json("duration_ms")?;
                let params = p.value("params");

                use crate::live::audio::vad::{NoiseType, TestAudioGenerator};
                use base64::Engine;

                let noise_type = NoiseType::from_name(noise_type_str, params)
                    .map_err(|e| format!("Invalid noise type: {}", e))?;

                let gen = TestAudioGenerator::default();
                let duration_samples = (crate::audio_constants::AUDIO_SAMPLE_RATE as u64
                    * duration_ms as u64
                    / 1000) as usize;

                let samples = gen.generate_noise(&noise_type, duration_samples);

                // Encode as base64 i16 LE PCM
                let pcm_bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
                let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&pcm_bytes);

                log_info!(
                    "module",
                    "voice_test_audio_generate",
                    "Generated {} noise: {} samples ({}ms)",
                    noise_type.label(),
                    samples.len(),
                    duration_ms
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "audio": audio_b64,
                    "samples": samples.len(),
                    "duration_ms": duration_ms,
                    "noise_type": noise_type.label(),
                    "sample_rate": crate::audio_constants::AUDIO_SAMPLE_RATE,
                })))
            }

            "voice/selftest" => {
                // THE SELF-PROOF VERB (MULTIMODAL-WIRING-AND-SELF-PROOF.md §2,
                // Joel 2026-09-02: "tested reliably, repeatedly … it can't
                // require intervention"). One entirely server-side round trip
                // through the SAME chain a live caller exercises: join a call
                // as a synthetic human, speak a nonce phrase through TTS, feed
                // the PCM through push_audio (VAD → speech-end → STT), and
                // await OUR OWN transcription event. No browser, no bridge, no
                // human. A regression in any link is a red receipt on a
                // schedule instead of a debugging night ("I don't think they
                // were hearing me", 2026-09-01).
                let _timer = TimingGuard::new("module", "voice_selftest");
                // A fixed synthetic call id (any uuid is a valid call key;
                // nothing subscribes to it, so no citizen perceives the test).
                const SELFTEST_CALL: &str = "5e1f7e57-0000-4000-8000-c0117e575e1f";
                // A NATURAL phrase: hex nonces are hostile to speech models
                // (measured 2026-09-02: Orpheus emitted 256ms then stopped on
                // 'continuum self test 075cf5') and to STT. The nonce is a
                // WORD, picked by uuid — unpredictable enough to prove this
                // run's audio, speakable enough to survive both directions.
                const NONCE_WORDS: [&str; 8] = [
                    "harbor", "velvet", "compass", "lantern",
                    "meadow", "ember", "willow", "granite",
                ];
                let word = NONCE_WORDS
                    [uuid::Uuid::new_v4().as_u128() as usize % NONCE_WORDS.len()];
                let phrase =
                    format!("This is the continuum self test. The magic word is {word}.");

                // Per-engine leg (Joel 2026-09-02: "the TDD for this is
                // voice <-> STT"): the same round trip proves ANY adapter —
                // `voice/selftest --adapter orpheus` is Orpheus's acceptance
                // test, and the nightly battery runs one leg per engine.
                let adapter = p.str_opt("adapter");
                // ROUTED (Joel 2026-09-02, the VDD/WM split): the ENGINE test
                // is a DIRECT TTS→STT round trip — deterministic, replayable,
                // no VAD timing, no mixer clock — because "does this engine
                // produce intelligible speech" is a property of the samples,
                // not of the room. `--room` opts into the full call-path leg
                // (join → push_audio → VAD → transcription broadcast) which
                // tests the ROOM plumbing separately. Two legs, one thing each
                // — the decomposition the flaky combined test was hiding.
                let room_leg = p.value("room").and_then(|v| v.as_bool()).unwrap_or(false); // safe: absent flag = engine leg, the default
                let t0 = std::time::Instant::now();
                let synthesis = crate::live::audio::tts_service::synthesize_speech_async(
                    &phrase, None, adapter, None,
                )
                .await
                .map_err(|e| format!("selftest TTS failed: {e}"))?;
                let tts_ms = t0.elapsed().as_millis() as u64;

                let t1 = std::time::Instant::now();
                let transcript = if room_leg {
                    // Full room path: the SAME chain a live caller exercises.
                    let joined = self
                        .state
                        .call_manager
                        .join_call(SELFTEST_CALL, "selftest-human", "Selftest", false)
                        .await
                        .map_err(|e| format!("selftest join failed: {e}"))?;
                    let handle = joined.handle;
                    let mut transcripts = joined.transcription_rx;
                    let mut pcm = synthesis.samples.clone();
                    pcm.extend(std::iter::repeat(0i16).take(
                        (crate::audio_constants::AUDIO_SAMPLE_RATE as usize) * 2,
                    ));
                    for chunk in pcm.chunks(crate::audio_constants::AUDIO_FRAME_SIZE) {
                        let mut frame = chunk.to_vec();
                        frame.resize(crate::audio_constants::AUDIO_FRAME_SIZE, 0);
                        self.state.call_manager.push_audio(&handle, frame).await;
                        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                    }
                    let mut heard = String::new();
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(30), async {
                        loop {
                            match transcripts.recv().await {
                                Ok(ev) if ev.user_id == "selftest-human" => {
                                    if !heard.is_empty() { heard.push(' '); }
                                    heard.push_str(&ev.text);
                                    if heard.to_lowercase().contains(word) { break; }
                                }
                                Ok(_) => continue,
                                Err(_) => break,
                            }
                        }
                    })
                    .await;
                    self.state.call_manager.leave_call(&handle).await;
                    heard // Partial utterances remain diagnostic evidence, never an automatic pass.
                } else {
                    // Engine leg: samples straight to STT. This is the TDD loop.
                    match crate::live::audio::stt_service::transcribe_speech_async(
                        &synthesis.samples,
                        Some("en"),
                    )
                    .await
                    {
                        Ok(r) => r.text,
                        // Surface the STT error into the receipt — a swallowed
                        // error read as an empty transcript and hid the real
                        // failure for a full debug cycle (2026-09-02).
                        Err(e) => return Err(format!("selftest STT failed: {e}")),
                    }
                };
                let stt_ms = t1.elapsed().as_millis() as u64;
                let lower = transcript.to_lowercase();
                let hits = ["continuum", "self", "test", word]
                    .iter()
                    .filter(|w| lower.contains(**w))
                    .count();
                // A generic fragment (or leaked vocabulary IDs) is not proof
                // that this synthesized nonce survived the audio path.
                let matched = hits >= 2 && lower.contains(word) && !transcript.contains('\t');
                let leg = if room_leg { "room" } else { "engine" };
                crate::probe!(
                    class = "live.selftest",
                    adapter = adapter.unwrap_or("active"), // safe: no adapter = the active one, a display label
                    leg = leg,
                    matched = matched,
                    tts_ms = tts_ms,
                    stt_ms = stt_ms,
                    transcript = %transcript,
                    "voice self-proof (engine leg = direct TTS→STT; room leg = full call path)"
                );
                Ok(CommandResult::Json(serde_json::json!({
                    "matched": matched,
                    "leg": leg,
                    "phrase": phrase,
                    "transcript": transcript,
                    "tts_ms": tts_ms,
                    "stt_ms": stt_ms,
                    "synthesized_ms": synthesis.duration_ms,
                })))
            }

            "voice/poll-transcriptions" => {
                let _timer = TimingGuard::new("module", "voice_poll_transcriptions");
                let call_id = p
                    .str_opt("call_id")
                    .map(|c| self.state.canonical_call_id(c));

                let entries = self
                    .state
                    .livekit_manager
                    .poll_transcriptions(call_id.as_deref())
                    .await;

                log_info!(
                    "module",
                    "voice_poll_transcriptions",
                    "Polled {} transcriptions{}",
                    entries.len(),
                    call_id
                        .as_deref()
                        .map(|c| format!(" for call {}", &c[..8.min(c.len())]))
                        .unwrap_or_default()
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "transcriptions": entries,
                    "count": entries.len(),
                })))
            }

            "voice/ambient-remove" => {
                let _timer = TimingGuard::new("module", "voice_ambient_remove");
                let call_id = self.state.canonical_call_id(p.str("call_id")?);
                let handle_str = p.str("handle")?;

                self.state
                    .livekit_manager
                    .remove_ambient_source(&call_id, handle_str)
                    .await
                    .map_err(|e| {
                        log_error!(
                            "module",
                            "voice_ambient_remove",
                            "ambient-remove failed: {}",
                            e
                        );
                        format!("ambient-remove failed: {}", e)
                    })?;

                log_info!(
                    "module",
                    "voice_ambient_remove",
                    "Removed ambient source from call {}",
                    call_id
                );
                Ok(CommandResult::Json(serde_json::json!({ "removed": true })))
            }

            "voice/set-cognitive-state" => {
                let user_id = p.str("user_id")?;
                let state_str = p.str("state")?;

                use crate::live::session::cognitive_animation::CognitiveState;
                let state = match state_str {
                    "evaluating" => CognitiveState::Evaluating,
                    "generating" => CognitiveState::Generating,
                    "idle" => CognitiveState::Idle,
                    _ => return Err(format!("Invalid cognitive state: {state_str} (expected evaluating|generating|idle)")),
                };

                #[cfg(feature = "avatar-3d")]
                let found = if let Some(bevy_system) = crate::live::video::bevy_renderer::try_get()
                {
                    bevy_system.set_cognitive_state_by_identity(user_id, state)
                } else {
                    false
                };
                // Built without `avatar-3d`: no 3D avatar exists to carry the state, so
                // nothing was set — the same answer as a renderer that isn't running.
                #[cfg(not(feature = "avatar-3d"))]
                let found = {
                    let _ = (user_id, state);
                    false
                };

                Ok(CommandResult::Json(serde_json::json!({ "set": found })))
            }

            "voice/snapshot-room" => {
                #[cfg(feature = "livekit-webrtc")]
                {
                    use crate::live::video::capture::VideoFrameCapture;
                    use base64::Engine;

                    let capture = VideoFrameCapture::instance();
                    match capture.snapshot_room().await {
                        Some(snap) => {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&snap.jpeg);
                            Ok(CommandResult::Json(serde_json::json!({
                                "success": true,
                                "base64": b64,
                                "mimeType": "image/jpeg",
                                "width": snap.width,
                                "height": snap.height,
                                "participants": snap.display_name,
                                "hash": snap.hash,
                                "capturedAt": snap.captured_at,
                            })))
                        }
                        None => Ok(CommandResult::Json(serde_json::json!({
                            "success": false,
                            "error": "No video frames captured yet"
                        }))),
                    }
                }
                #[cfg(not(feature = "livekit-webrtc"))]
                Ok(CommandResult::Json(serde_json::json!({
                    "success": false,
                    "error": "Video capture unavailable (compiled without livekit-webrtc)"
                })))
            }

            "voice/snapshot-participant" => {
                #[cfg(feature = "livekit-webrtc")]
                {
                    use crate::live::video::capture::VideoFrameCapture;
                    use base64::Engine;

                    let identity = p.str("identity")?;
                    let capture = VideoFrameCapture::instance();
                    match capture.snapshot_participant(identity).await {
                        Some(snap) => {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&snap.jpeg);
                            Ok(CommandResult::Json(serde_json::json!({
                                "success": true,
                                "base64": b64,
                                "mimeType": "image/jpeg",
                                "width": snap.width,
                                "height": snap.height,
                                "identity": snap.identity,
                                "displayName": snap.display_name,
                                "hash": snap.hash,
                                "capturedAt": snap.captured_at,
                            })))
                        }
                        None => Ok(CommandResult::Json(serde_json::json!({
                            "success": false,
                            "error": format!("No video frame for participant '{}'", identity)
                        }))),
                    }
                }
                #[cfg(not(feature = "livekit-webrtc"))]
                Ok(CommandResult::Json(serde_json::json!({
                    "success": false,
                    "error": "Video capture unavailable (compiled without livekit-webrtc)"
                })))
            }

            "voice/resource-status" => {
                let _timer = TimingGuard::new("module", "voice_resource_status");

                // Collect STT adapter status
                let stt_status: Vec<serde_json::Value> = {
                    let registry = crate::live::audio::stt::get_registry();
                    let reg = registry.read();
                    reg.list()
                        .iter()
                        .map(|(name, initialized)| {
                            serde_json::json!({
                                "name": name,
                                "loaded": initialized,
                            })
                        })
                        .collect()
                };

                // Collect TTS adapter status
                let tts_status: Vec<serde_json::Value> = {
                    let registry = crate::live::audio::tts::get_registry();
                    let reg = registry.read();
                    reg.list()
                        .iter()
                        .map(|(name, initialized)| {
                            serde_json::json!({
                                "name": name,
                                "loaded": initialized,
                            })
                        })
                        .collect()
                };

                let active_sessions = self.state.resource_lifecycle.active_count();
                #[cfg(feature = "avatar-3d")]
                let bevy_running = crate::live::video::bevy_renderer::is_running();
                // No renderer is compiled into this build, so it is not running.
                #[cfg(not(feature = "avatar-3d"))]
                let bevy_running = false;

                Ok(CommandResult::Json(serde_json::json!({
                    "active_sessions": active_sessions,
                    "bevy_renderer": bevy_running,
                    "stt_adapters": stt_status,
                    "tts_adapters": tts_status,
                })))
            }

            "voice/resource-unload" => {
                let _timer = TimingGuard::new("module", "voice_resource_unload");

                log_info!(
                    "module",
                    "voice_resource_unload",
                    "Force-unloading all audio models"
                );

                let mut unloaded = Vec::new();

                // Collect initialized STT adapters (drop lock before await)
                let stt_adapters: Vec<_> = {
                    let registry = crate::live::audio::stt::get_registry();
                    let reg = registry.read();
                    reg.list()
                        .into_iter()
                        .filter(|(_, initialized)| *initialized)
                        .filter_map(|(name, _)| reg.get(name).map(|a| (name, a)))
                        .collect()
                };
                for (name, adapter) in stt_adapters {
                    match adapter.shutdown().await {
                        Ok(()) => unloaded.push(name.to_string()),
                        Err(e) => {
                            log_error!(
                                "module",
                                "voice_resource_unload",
                                "STT '{}' shutdown failed: {}",
                                name,
                                e
                            );
                        }
                    }
                }

                // Collect initialized TTS adapters (drop lock before await)
                let tts_adapters: Vec<_> = {
                    let registry = crate::live::audio::tts::get_registry();
                    let reg = registry.read();
                    reg.list()
                        .into_iter()
                        .filter(|(_, initialized)| *initialized)
                        .filter_map(|(name, _)| reg.get(name).map(|a| (name, a)))
                        .collect()
                };
                for (name, adapter) in tts_adapters {
                    match adapter.shutdown().await {
                        Ok(()) => unloaded.push(name.to_string()),
                        Err(e) => {
                            log_error!(
                                "module",
                                "voice_resource_unload",
                                "TTS '{}' shutdown failed: {}",
                                name,
                                e
                            );
                        }
                    }
                }

                // Reset session counter — force-unload means we don't care about
                // in-flight sessions. Models will reload on next call.
                self.state.resource_lifecycle.reset_sessions();

                // Shut down Bevy renderer to reclaim ~3GB GPU/ECS memory (only exists
                // under `avatar-3d`; without it there is nothing to shut down).
                #[cfg(feature = "avatar-3d")]
                let bevy_was_running = {
                    let running = crate::live::video::bevy_renderer::is_running();
                    if running {
                        crate::live::video::bevy_renderer::shutdown();
                        crate::live::avatar::reset_slot_pool();
                        unloaded.push("bevy-renderer".to_string());
                    }
                    running
                };
                #[cfg(not(feature = "avatar-3d"))]
                let bevy_was_running = false;

                log_info!(
                    "module",
                    "voice_resource_unload",
                    "Unloaded {} adapters: {:?}",
                    unloaded.len(),
                    unloaded
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "unloaded": unloaded,
                    "count": unloaded.len(),
                    "bevy_shutdown": bevy_was_running,
                })))
            }

            _ => Err(format!("Unknown voice command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
