//! VoiceModule — wraps voice synthesis, transcription, and call management.
//!
//! Handles: voice/register-session, voice/on-utterance, voice/should-route-tts,
//!          voice/synthesize, voice/speak-in-call, voice/synthesize-handle,
//!          voice/play-handle, voice/discard-handle, voice/transcribe,
//!          voice/transcribe-with-adapter, voice/stt-list,
//!          voice/test-audio-generate,
//!          voice/inject-audio, voice/ambient-add, voice/ambient-inject,
//!          voice/ambient-remove, voice/poll-transcriptions,
//!          voice/set-cognitive-state,
//!          voice/snapshot-room, voice/snapshot-participant
//!
//! Priority: Realtime — voice operations are time-critical.

use crate::live::audio::buffer::AudioBufferPool;
use crate::live::audio::resource_lifecycle::AudioResourceLifecycle;
use crate::live::session::voice_service::VoiceService;
use crate::live::transport::bridge_client::LiveKitAgentManager;
use crate::live::{UtteranceEvent, VoiceParticipant};
use crate::logging::TimingGuard;
use crate::runtime::{
    CommandExecutor, CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority,
    ServiceModule,
};
use crate::utils::params::Params;
use crate::{log_error, log_info};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Response field name for voice responder IDs
const VOICE_RESPONSE_FIELD_RESPONDER_IDS: &str = "responder_ids";

/// Shared state for voice module.
pub struct VoiceState {
    pub voice_service: Arc<VoiceService>,
    pub livekit_manager: Arc<LiveKitAgentManager>,
    /// Native call plane (call_server WS 8790). The avatar pump tees each Bevy
    /// frame here so native clients see the real face (#193/#172) — not just
    /// LiveKit subscribers.
    pub call_manager: Arc<crate::live::transport::call_server::CallManager>,
    pub audio_pool: Arc<AudioBufferPool>,
    pub resource_lifecycle: Arc<AudioResourceLifecycle>,
    /// Track active CANONICAL call ids (= airc room ids, #193 slice A) to make
    /// register-session idempotent. Prevents duplicate agent spawns on browser
    /// refresh / re-navigate.
    active_sessions: std::sync::Mutex<HashSet<String>>,
    /// #193 slice A legacy aliases: client-minted `session_id` → canonical airc
    /// `room_id`. The server is now AUTHORITATIVE that a call IS its airc room —
    /// every call is keyed by room_id. Until the client cutover (slice B) lands,
    /// clients may still address verbs by their minted session_id; this map lets
    /// [`VoiceState::canonical_call_id`] resolve those to the room. Entries are
    /// dropped when their call ends. Empty once slice B ships — that's the
    /// done-signal.
    legacy_call_aliases: std::sync::Mutex<HashMap<String, String>>,
    /// Late-bound command executor for the vision describer the perception-ingest
    /// drain builds. Installed post-boot by the runtime (the #224 late-bind slot),
    /// read lazily on the first live-call frame — never present at `initialize`.
    executor: LateBound<CommandExecutor>,
}

impl VoiceState {
    pub fn new(
        voice_service: Arc<VoiceService>,
        livekit_manager: Arc<LiveKitAgentManager>,
        call_manager: Arc<crate::live::transport::call_server::CallManager>,
        audio_pool: Arc<AudioBufferPool>,
    ) -> Self {
        let resource_lifecycle = Arc::new(AudioResourceLifecycle::new());
        Self {
            voice_service,
            livekit_manager,
            call_manager,
            audio_pool,
            resource_lifecycle,
            active_sessions: std::sync::Mutex::new(HashSet::new()),
            legacy_call_aliases: std::sync::Mutex::new(HashMap::new()),
            executor: LateBound::new("live::perception_ingest::executor"),
        }
    }

    /// THE one resolver for call/session ids (#193 slice A — compression: one
    /// resolver, called at each verb's entry, never per-site logic). A legacy
    /// client-minted `session_id` resolves to the canonical airc `room_id` it was
    /// aliased to at register time; any other id (already canonical, or unknown)
    /// passes through unchanged.
    /// Is a voice session live for this canonical call/room id? The reply
    /// speaker asks this before turning a room line into speech.
    pub fn has_active_session(&self, call_id: &str) -> bool {
        self.active_sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(call_id)
    }

    /// Synthesize `text` in `user_id`'s voice into the call (LiveKit track) and
    /// tee the SAME samples into the native call plane so native clients hear
    /// her too. ONE speak path — the `voice/speak-in-call` verb and the reply
    /// speaker both come through here. Returns (samples, duration_ms, rate).
    pub async fn speak_in_room(
        &self,
        call_id: &str,
        user_id: &str,
        text: &str,
        voice: Option<&str>,
        adapter: Option<&str>,
        display_name: Option<&str>,
    ) -> Result<(usize, u64, u32), String> {
        let (samples, duration_ms, sample_rate) = self
            .livekit_manager
            .speak_in_call(call_id, user_id, text, voice, adapter, display_name)
            .await?;
        let num_samples = samples.len();
        // #193 audio convergence: tee the SAME synthesized voice into the
        // native call plane so native clients (positron web, glass-box harness)
        // HEAR her instead of the hold-music the lonely-listener mixer plays.
        // Sibling of the avatar-video tee. No-op if no native client is on the call.
        self.call_manager
            .push_persona_audio(call_id, user_id, display_name.unwrap_or(user_id), samples)
            .await;
        Ok((num_samples, duration_ms, sample_rate))
    }

    fn canonical_call_id(&self, id: &str) -> String {
        let aliases = self
            .legacy_call_aliases
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        aliases.get(id).cloned().unwrap_or_else(|| id.to_string())
    }
}

pub struct VoiceModule {
    state: Arc<VoiceState>,
}

impl VoiceModule {
    pub fn new(state: Arc<VoiceState>) -> Self {
        Self { state }
    }
}

/// Drains the live-call perception-ingest channel (#192): each decoded frame is fanned
/// out to every persona-viewer of its call. The vision DESCRIBER is built lazily on the
/// first frame after the executor is installed (boot-once) and reused, so N viewers of
/// one frame share ONE describe on the content-addressed cache — the multi-persona
/// vision moat. Non-blocking per frame: `fan_out` coalesces + gates the warm, so the
/// ~1 fps ingest never becomes a describe storm and the human video plane is untouched.
async fn perception_ingest_drain(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<crate::media::perception_ingest::IngestFrame>,
    state: Arc<VoiceState>,
) {
    use crate::media::perception_ingest::FrameIngest;

    let mut ingest: Option<FrameIngest> = None;
    let mut fanned: u64 = 0;

    while let Some(frame) = rx.recv().await {
        // Build the fan-out (with its vision describer) once the executor is installed.
        // Until then — boot only, well before any call joins — drop the frame.
        if ingest.is_none() {
            match state.executor.cloned() {
                Some(executor) => {
                    let describer = Arc::new(
                        crate::cognition::vision_describe::VisionDescribeFramer::new(executor),
                    );
                    ingest = Some(FrameIngest::new(describer));
                }
                None => continue,
            }
        }

        let viewers = state.voice_service.video_viewers(&frame.call_id);
        if viewers.is_empty() {
            continue;
        }

        // Fan out: each viewer's PerceptionBuffer coalesces the frame + fires a gated
        // async warm; compute-once/share-many means one describe across all viewers.
        ingest.as_ref().expect("ingest built above").fan_out(
            &frame.speaker_id,
            &viewers,
            frame.jpeg,
            &frame.mime,
            crate::persona::recall_metadata::now_ms(),
        );

        fanned += 1;
        if fanned == 1 || fanned % 120 == 0 {
            log_info!(
                "module",
                "perception_ingest",
                "fanned {} live-call frames into perception (latest: speaker {} → {} viewers)",
                fanned,
                &frame.speaker_id[..8.min(frame.speaker_id.len())],
                viewers.len()
            );
        }
    }
}

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
        super::voice_reply_speaker::spawn(ctx.bus.clone(), self.state.clone());
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
                    crate::log_warn!(
                        "module",
                        "voice_register_session",
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
                use crate::utils::audio::i16_to_f32;
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
                    for chunk in pcm.chunks(320) {
                        self.state.call_manager.push_audio(&handle, chunk.to_vec()).await;
                        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                    }
                    let wait = tokio::time::timeout(std::time::Duration::from_secs(30), async {
                        loop {
                            match transcripts.recv().await {
                                Ok(ev) if ev.user_id == "selftest-human" => break Some(ev.text),
                                Ok(_) => continue,
                                Err(_) => break None,
                            }
                        }
                    })
                    .await;
                    self.state.call_manager.leave_call(&handle).await;
                    wait.ok().flatten().unwrap_or_default() // safe: no event = empty = red receipt
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
                let matched = hits >= 2;
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

                let found = if let Some(bevy_system) = crate::live::video::bevy_renderer::try_get()
                {
                    bevy_system.set_cognitive_state_by_identity(user_id, state)
                } else {
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
                let bevy_running = crate::live::video::bevy_renderer::is_running();

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

                // Shut down Bevy renderer to reclaim ~3GB GPU/ECS memory
                let bevy_was_running = crate::live::video::bevy_renderer::is_running();
                if bevy_was_running {
                    crate::live::video::bevy_renderer::shutdown();
                    crate::live::avatar::reset_slot_pool();
                    unloaded.push("bevy-renderer".to_string());
                }

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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn module() -> VoiceModule {
        VoiceModule::new(Arc::new(VoiceState::new(
            Arc::new(VoiceService::new()),
            Arc::new(LiveKitAgentManager::new()),
            Arc::new(crate::live::transport::call_server::CallManager::new()),
            Arc::new(AudioBufferPool::new()),
        )))
    }

    /// Register a session with one Persona participant; returns the JSON result.
    /// The bridge socket doesn't exist under test — the spawned listener/agent
    /// tasks fail loud into logs without affecting the registration state under test.
    async fn register(m: &VoiceModule, session_id: &str, room_id: &str, persona: Uuid) -> Value {
        let result = m
            .handle_command(
                "voice/register-session",
                serde_json::json!({
                    "session_id": session_id,
                    "room_id": room_id,
                    "participants": [{
                        "user_id": persona,
                        "display_name": "Test Persona",
                        "participant_type": "persona",
                        "expertise": [],
                    }],
                }),
            )
            .await
            .expect("register-session");
        match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json result, got {other:?}"),
        }
    }

    // what this catches: #193 slice A — with session_id == room_id (the slice-B client
    // contract) the call registers under the room id with NO legacy alias recorded and
    // the resolver is the identity. Regresses if the alias map ever grows on the
    // canonical path.
    #[tokio::test]
    async fn call_registers_under_room_id_with_no_alias_when_ids_agree() {
        let m = module();
        let room = Uuid::new_v4().to_string();
        let v = register(&m, &room, &room, Uuid::new_v4()).await;

        assert_eq!(v["registered"], true);
        assert_eq!(v["call_id"], room);
        assert!(m.state.active_sessions.lock().unwrap().contains(&room));
        assert!(
            m.state
                .legacy_call_aliases
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty(),
            "canonical registration must not record an alias"
        );
        assert_eq!(m.state.canonical_call_id(&room), room);
    }

    // what this catches: #193 slice A — a divergent client-minted session_id registers
    // the call UNDER the airc room id (server authoritative), records a legacy alias,
    // and a subsequent verb addressed by the LEGACY id (on-utterance, then end-session)
    // resolves to the SAME session. Regresses if any verb grows per-site id logic
    // instead of the one `canonical_call_id` resolver.
    #[tokio::test]
    async fn divergent_session_id_is_aliased_and_legacy_verbs_resolve_to_the_room() {
        let m = module();
        let legacy = Uuid::new_v4().to_string();
        let room = Uuid::new_v4().to_string();
        let persona = Uuid::new_v4();
        let v = register(&m, &legacy, &room, persona).await;
        assert_eq!(v["call_id"], room);

        // Registered under the ROOM id, never the legacy id.
        {
            let sessions = m.state.active_sessions.lock().unwrap();
            assert!(sessions.contains(&room));
            assert!(!sessions.contains(&legacy));
        }
        assert_eq!(m.state.canonical_call_id(&legacy), room);

        // on-utterance addressed by the LEGACY id reaches the room-keyed session:
        // the orchestrator returns the registered persona as responder.
        let result = m
            .handle_command(
                "voice/on-utterance",
                serde_json::json!({
                    "event": {
                        "session_id": legacy,
                        "speaker_id": Uuid::new_v4(),
                        "speaker_name": "Operator",
                        "speaker_type": "human",
                        "transcript": "hello there",
                        "confidence": 1.0,
                        "timestamp": 0,
                    }
                }),
            )
            .await
            .expect("on-utterance");
        let CommandResult::Json(v) = result else {
            panic!("expected Json result");
        };
        let responders: Vec<&str> = v["responder_ids"]
            .as_array()
            .expect("responder_ids array")
            .iter()
            .map(|x| x.as_str().expect("uuid string"))
            .collect();
        assert_eq!(responders, vec![persona.to_string().as_str()]);

        // end-session by the LEGACY id tears down the canonical call and drops the alias.
        m.handle_command(
            "voice/end-session",
            serde_json::json!({ "session_id": legacy }),
        )
        .await
        .expect("end-session");
        assert!(!m.state.active_sessions.lock().unwrap().contains(&room));
        assert!(m
            .state
            .legacy_call_aliases
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty());
    }

    // what this catches: #193 slice A — the browser-refresh re-join (fresh legacy
    // session_id, same room) and a canonical re-register both dedupe on the CANONICAL
    // id: one active call, no duplicate listener/agent spawns, and the fresh legacy id
    // still resolves. Regresses if the idempotency guard keys on the raw session_id.
    #[tokio::test]
    async fn re_register_with_either_id_dedupes_on_the_canonical_id() {
        let m = module();
        let room = Uuid::new_v4().to_string();
        let persona = Uuid::new_v4();
        register(&m, &room, &room, persona).await;

        // Refresh path: new client-minted id, same room → deduped AND aliased.
        let legacy = Uuid::new_v4().to_string();
        let v = register(&m, &legacy, &room, persona).await;
        assert_eq!(v["already_active"], true);
        assert_eq!(v["call_id"], room);
        assert_eq!(m.state.canonical_call_id(&legacy), room);

        // Canonical re-register → deduped.
        let v = register(&m, &room, &room, persona).await;
        assert_eq!(v["already_active"], true);
        assert_eq!(m.state.active_sessions.lock().unwrap().len(), 1);
    }
}
