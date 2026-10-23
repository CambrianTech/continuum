//! Boot phase 8: the LIVE CALL media plane (call manager, STT forwarder, call
//! server) and the VoiceModule that fronts it.

use super::*;

/// What the later phases (server state, WS live-call emitter) keep from the call plane.
pub(super) struct LiveCall {
    pub(super) voice_service: Arc<crate::live::session::voice_service::VoiceService>,
    pub(super) voice_service_for_view: Arc<crate::live::session::voice_service::VoiceService>,
    pub(super) live_registrar_executor:
        Arc<crate::runtime::LateBound<crate::runtime::CommandExecutor>>,
    pub(super) call_manager: Arc<crate::live::transport::call_server::CallManager>,
}

pub(super) fn start_live_call_plane(rt_handle: &tokio::runtime::Handle) -> LiveCall {
    // The LIVE CALL media plane (finish-live-video, 2026-07-25): the WebSocket
    // call server (mix-minus audio, STT, avatar-state fanout, video frames)
    // was fully built and NEVER STARTED — the armed-but-idle pattern. Start it
    // at boot on its own task; the browser's live face dials it on Go-live.
    // Port from config.env CONTINUUM_CALL_WS (one owner, [[config-env-single-owner]]),
    // default 8790. Bind failure is LOUD (a squatted port must surface), but
    // non-fatal: the core serves without a media plane rather than dying —
    // the live face shows its honest avatar-presence chip in that state.
    let voice_service = Arc::new(crate::live::session::voice_service::VoiceService::new());
    // Handle for the live-call projection (#58). Taken HERE because the original is
    // moved into the module registry below, and the emitter is a READER — it must not
    // change who owns the service.
    let voice_service_for_view = voice_service.clone();

    // CORE-DRIVEN SESSION REGISTRATION (#58). Built mutable so the registrar can be
    // installed BEFORE the Arc freezes it: joining a call now registers the session in
    // the core itself, instead of waiting for whichever client happened to implement it.
    // Exactly one ever did (Node, in legacy/, deleted: tag legacy-archive-2026-10-23), which is why iOS/Android/TUI
    // citizens were structurally voiceless rather than merely buggy.
    // The registrar's late-bound executor: filled beside `install_executor_on_all`
    // so a human's call join can invoke `voice/register-session` — the verb that
    // populates a call's CITIZENS (agents + Bevy video pumps). That verb was only
    // ever called by the retired Node shell, so a web Go-Live produced a call
    // with a human, hold music, and zero citizens (live 2026-08-31: four static
    // portraits, nothing animating — the pumps' spawn block simply never ran).
    let live_registrar_executor: std::sync::Arc<
        crate::runtime::LateBound<crate::runtime::CommandExecutor>,
    > = std::sync::Arc::new(crate::runtime::LateBound::new(
        "live::session-registrar executor",
    ));
    let call_manager = build_call_manager(&voice_service, &live_registrar_executor);
    spawn_human_audio_forwarder(rt_handle, &call_manager);
    spawn_call_server(rt_handle, &call_manager);
    LiveCall {
        voice_service,
        voice_service_for_view,
        live_registrar_executor,
        call_manager,
    }
}

fn build_call_manager(
    voice_service: &Arc<crate::live::session::voice_service::VoiceService>,
    live_registrar_executor: &Arc<crate::runtime::LateBound<crate::runtime::CommandExecutor>>,
) -> Arc<crate::live::transport::call_server::CallManager> {
    let mut mgr = crate::live::transport::call_server::CallManager::new();
    let voice_for_join = voice_service.clone();
    let citizens_exec = live_registrar_executor.clone();
    mgr.set_session_registrar(std::sync::Arc::new(
            move |call_id: &str, user_id: &str, display_name: &str, is_ai: bool| {
                // CORE-DRIVEN CITIZEN POPULATION: the first HUMAN joining a call
                // pulls the node's resident citizens in (idempotent — the voice
                // module dedupes re-registration on the canonical id).
                if !is_ai {
                    if let Some(exec) = citizens_exec.cloned() {
                        let call = call_id.to_string();
                        let human_id = user_id.to_string();
                        let human_name = display_name.to_string();
                        tokio::spawn(async move {
                            let Some(reg) =
                                crate::persona::PersonaAircRuntimeRegistry::try_global()
                            else {
                                return;
                            };
                            let mut participants = vec![serde_json::json!({
                                "user_id": human_id,
                                "display_name": human_name,
                                "participant_type": "human",
                                "expertise": [],
                            })];
                            for rt in reg.iter() {
                                participants.push(serde_json::json!({
                                    "user_id": rt.persona_id().to_string(),
                                    "display_name": rt.agent_name(),
                                    "participant_type": "persona",
                                    "expertise": [],
                                }));
                            }
                            let n = participants.len();
                            match exec
                                .execute_json(
                                    "voice/register-session",
                                    serde_json::json!({
                                        "session_id": call,
                                        "room_id": call,
                                        "participants": participants,
                                    }),
                                )
                                .await
                            {
                                Ok(_) => continuum_base::probe!(
                                    class = "live.call.citizens_populated",
                                    call_id = %call,
                                    participants = n,
                                    "human joined — resident citizens registered into the call \
                                     (agents + avatar pumps spawn from here)"
                                ),
                                Err(e) => tracing::warn!(
                                    call_id = %call,
                                    error = %e,
                                    probe_class = "live.call.citizens_populate_failed",
                                    "citizen population failed — the call stays human-only this join"
                                ),
                            }
                        });
                    }
                }
                let Ok(uid) = uuid::Uuid::parse_str(user_id) else {
                    // A non-uuid participant id is a caller bug, not a runtime condition —
                    // say so rather than registering a citizen nobody can address later.
                    tracing::warn!(
                        call_id = %call_id,
                        user_id = %user_id,
                        probe_class = "live.register.bad_user_id",
                        "call join carried a non-uuid user id — session NOT registered for                          this participant; she will render as unregistered in the live view"
                    );
                    return;
                };
                let participant = crate::live::VoiceParticipant {
                    user_id: uid,
                    display_name: display_name.to_string(),
                    participant_type: if is_ai {
                        crate::live::SpeakerType::Persona
                    } else {
                        crate::live::SpeakerType::Human
                    },
                    expertise: Vec::new(),
                    // Audio-native is a MODEL capability the transport cannot know. Left
                    // false here and owned by whoever knows the model; claiming it from a
                    // join flag would make a text persona silently miss transcriptions.
                    is_audio_native: false,
                };
                if let Err(e) = voice_for_join.join_participant(call_id, participant) {
                    tracing::warn!(
                        call_id = %call_id,
                        error = %e,
                        probe_class = "live.register.failed",
                        "core-driven session registration FAILED — the call is live and the                          core cannot route to this participant"
                    );
                }
            },
        ));
    // SPEECH → PERCEPTION (the second STT dead link, closed 2026-09-01):
    // a transcribed human utterance posts as a ROOM MESSAGE from the human
    // (the call id IS the airc RoomId), through the full chat/send
    // composition — store + live feed + daemon say — so citizens perceive
    // speech exactly like text and the whole reply pipeline (directedness,
    // turns, voice-out) fires. Subtitles alone reached screens, no minds.
    {
        let exec_slot = live_registrar_executor.clone();
        mgr.set_transcript_sink(std::sync::Arc::new(
            move |call_id: &str, user_id: &str, _display_name: &str, text: &str| {
                let Some(exec) = exec_slot.cloned() else {
                    return;
                };
                let (room, speaker, said) =
                    (call_id.to_string(), user_id.to_string(), text.to_string());
                tokio::spawn(async move {
                    match exec
                        .execute_json(
                            "chat/send",
                            serde_json::json!({
                                "roomId": room,
                                "senderId": speaker,
                                "text": said,
                            }),
                        )
                        .await
                    {
                        Ok(_) => continuum_base::probe!(
                            class = "live.stt.utterance_to_room",
                            room = %room,
                            speaker = %speaker,
                            chars = said.len(),
                            "spoken words landed as a room message — citizens can now \
                             answer speech"
                        ),
                        Err(e) => tracing::warn!(
                            room = %room,
                            error = %e,
                            probe_class = "live.stt.utterance_to_room_failed",
                            "transcription could not post to the room — subtitles only \
                             for this utterance"
                        ),
                    }
                });
            },
        ));
    }
    std::sync::Arc::new(mgr)
}

fn spawn_human_audio_forwarder(
    rt_handle: &tokio::runtime::Handle,
    call_manager: &Arc<crate::live::transport::call_server::CallManager>,
) {
    // STT WIRE (closed 2026-09-01): bridge-heard human audio → the
    // CallServer's existing VAD → speech-end → transcription pipeline.
    // The bridge reader is a plain thread; this bounded channel + task is
    // its one hop into async land. Capacity ~2s of 20ms frames: a stalled
    // consumer drops frames (stay current) instead of building a backlog
    // of stale speech.
    let (tx, mut rx) =
        tokio::sync::mpsc::channel::<crate::live::transport::bridge_client::HumanAudioChunk>(128);
    crate::live::transport::bridge_client::install_human_audio_forwarder(tx);
    let manager = call_manager.clone();
    rt_handle.spawn(async move {
        while let Some(chunk) = rx.recv().await {
            manager
                .push_remote_human_audio(&chunk.call_id, &chunk.user_id, chunk.samples)
                .await;
        }
    });
}

fn spawn_call_server(
    rt_handle: &tokio::runtime::Handle,
    call_manager: &Arc<crate::live::transport::call_server::CallManager>,
) {
    let port = continuum_base::config_env::read("CONTINUUM_CALL_WS")
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(8790);
    let addr = format!("127.0.0.1:{port}");
    let manager = call_manager.clone();
    rt_handle.spawn(async move {
            if let Err(e) =
                crate::live::transport::call_server::start_call_server(&addr, manager).await
            {
                tracing::error!(addr = %addr, error = %e, "call server failed to start — live media plane unavailable (browser live face stays avatar-presence)");
            }
        });
}

/// Registers the VoiceModule and joins its consumers to the resource authority.
/// Returns the audio pool the IPC server state shares.
pub(super) fn register_voice_module(
    runtime: &Arc<Runtime>,
    livekit_manager: &Arc<crate::live::transport::bridge_client::LiveKitAgentManager>,
    resource_daemon: &Arc<crate::resources::ResourceDaemon>,
    gpu_manager: &Arc<GpuMemoryManager>,
    voice_service: &Arc<crate::live::session::voice_service::VoiceService>,
    call_manager: &Arc<crate::live::transport::call_server::CallManager>,
) -> Arc<crate::live::audio::buffer::AudioBufferPool> {
    // Phase 3: VoiceModule (wraps VoiceService, CallManager, AudioBufferPool)
    let audio_pool = Arc::new(crate::live::audio::buffer::AudioBufferPool::new());
    let voice_state = Arc::new(VoiceState::new(
        voice_service.clone(),
        livekit_manager.clone(),
        call_manager.clone(),
        audio_pool.clone(),
    ));
    // Voice joins the resource authority as a peer consumer (#56): under VRAM
    // pressure it REFUSES while a call is live (never kicks a human mid-call) and
    // sheds its idle STT/TTS models otherwise — serving tiers down first.
    resource_daemon.add_consumer(Arc::new(
        crate::modules::live_session_consumer::VoiceConsumer::new(
            voice_state.resource_lifecycle.clone(),
            gpu_manager.clone(),
        ),
    ));
    // The Bevy avatar renderer joins as the third peer consumer (#56): fat and
    // reclaimable (~3GB) when idle, but its output texture IS the LiveKit video
    // feed during a call — so under pressure while a call is live it REFUSES
    // (tearing it down would freeze the avatar mid-call) and sheds the renderer
    // only when nothing is rendering. Shares the same live-session lifecycle as
    // voice, so both sides of a call are protected together. Only under
    // `avatar-3d`: without the renderer there is no residency to account or shed.
    #[cfg(feature = "avatar-3d")]
    resource_daemon.add_consumer(Arc::new(crate::modules::bevy_consumer::BevyConsumer::new(
        voice_state.resource_lifecycle.clone(),
        gpu_manager.clone(),
    )));
    // Live-call perception joins as a peer consumer (#56): it holds RAM — the per-source
    // frame rings every persona perceives from — read live from the process-global
    // PerceptionRegistry. Under pressure it evicts oldest ring frames, always keeping each
    // source's head so a live perceiver is never blinded (room-as-now survives a reclaim).
    // The vision-describe it triggers is SERVING's VRAM, not perception's; this accounts
    // only what perception owns.
    resource_daemon.add_consumer(Arc::new(
        crate::modules::perception_consumer::PerceptionConsumer::new(
            crate::media::perception_registry(),
        ),
    ));
    // Benchmark staging joins as a peer consumer (#56) — and it is the one that should ALWAYS
    // lose. It holds host RAM only while a suite is being fetched and projected, and every row
    // it holds is already on disk, so yielding costs a re-read and nothing a human or citizen
    // can perceive. Where serving weighs a tier-down and bevy/voice REFUSE during a live call,
    // staging releases unconditionally: it is the cheapest victim on the box and must be taken
    // first. Until this existed, a benchmark and a live call simply raced `malloc` — which is
    // exactly how staging OOMed this machine on 2026-08-19.
    resource_daemon.add_consumer(Arc::new(
        crate::cognition::measurement_staging::StagingConsumer::new(
            crate::cognition::measurement_staging::staging_area(),
        ),
    ));
    runtime.register(Arc::new(VoiceModule::new(voice_state)));
    audio_pool
}
