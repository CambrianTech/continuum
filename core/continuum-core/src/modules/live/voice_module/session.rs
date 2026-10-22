//! Call lifecycle: register, end, utterance routing.

use super::super::*;

impl VoiceModule {
    pub(super) async fn cmd_register_session(&self, p: &Params<'_>) -> Result<CommandResult, String> {
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

    pub(super) async fn cmd_end_session(&self, p: &Params<'_>) -> Result<CommandResult, String> {
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

    pub(super) async fn cmd_on_utterance(&self, p: &Params<'_>) -> Result<CommandResult, String> {
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
}
