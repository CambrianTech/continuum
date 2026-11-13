use super::*;

impl CallManager {
    /// Install the core-side registrar. Called once at boot, where the session service
    /// and the call manager both exist; absent in tests, where a call needs no session.
    pub fn set_session_registrar(&mut self, registrar: SessionRegistrar) {
        self.session_registrar = Some(registrar);
    }

    /// Install the transcript→room sink. Called once at boot beside the
    /// registrar; absent in tests, where subtitles alone are the contract.
    pub fn set_transcript_sink(&mut self, sink: TranscriptSink) {
        self.transcript_sink = Some(sink);
    }

    /// Every LIVE call and how many participants are in it — the read side the live-call
    /// projection needs (#58).
    ///
    /// There was no reader here either, and that absence is load-bearing: nothing could
    /// compare "calls the CallServer is actually running" against "sessions the
    /// orchestrator thinks are registered". That comparison IS the defect made visible.
    /// A live call with no matching registration is precisely the state where a persona
    /// sits in the room, present and silent, `isInCall()` returns false, and her responses
    /// are dropped with nothing anywhere saying why.
    ///
    /// Returns a snapshot rather than a guard: the caller is a periodic projection, and
    /// holding these locks across an await would put a rendering concern in the audio path.
    pub async fn live_calls(&self) -> Vec<(String, usize)> {
        let calls = self.calls.read().await;
        let participants = self.participant_calls.read().await;
        calls
            .keys()
            .map(|call_id| {
                let n = participants.values().filter(|c| *c == call_id).count();
                (call_id.clone(), n)
            })
            .collect()
    }

    pub fn new() -> Self {
        Self {
            calls: RwLock::new(HashMap::new()),
            participant_calls: RwLock::new(HashMap::new()),
            audio_loops: RwLock::new(HashMap::new()),
            video_source_shutdowns: RwLock::new(HashMap::new()),
            audio_router: AudioRouter::new(),
            capability_registry: Arc::new(ModelCapabilityRegistry::new()),
            persona_audio_handles: RwLock::new(HashMap::new()),
            session_registrar: None,
            transcript_sink: None,
        }
    }

    /// Get or create a call, starting audio loop if new
    pub(super) async fn get_or_create_call(&self, call_id: &str) -> Arc<RwLock<Call>> {
        let mut calls = self.calls.write().await;
        if let Some(call) = calls.get(call_id) {
            call.clone()
        } else {
            let call = Arc::new(RwLock::new(Call::new(call_id.to_string())));
            calls.insert(call_id.to_string(), call.clone());

            // Start server-driven audio loop for this call
            self.start_audio_loop(call_id.to_string(), call.clone())
                .await;

            // Real video sources are now connected (#193/#172): the avatar pump tees
            // each persona's Bevy frame into this call's video plane via
            // `push_avatar_frame`. The SMPTE test pattern is retired to an opt-in
            // debug affordance — it used to auto-start and mask the real avatar with
            // 160x120 color bars for every native client. Set CONTINUUM_CALL_TEST_PATTERN=1
            // to bring it back for plumbing checks on a call with no real source.
            if continuum_base::config_env::read("CONTINUUM_CALL_TEST_PATTERN")
                .map(|v| v.trim() == "1" || v.trim().eq_ignore_ascii_case("true"))
                .unwrap_or(false)
            {
                if let Some(shutdown_tx) = Self::start_test_video_source_for(&call, call_id) {
                    let mut shutdowns = self.video_source_shutdowns.write().await;
                    shutdowns
                        .entry(call_id.to_string())
                        .or_default()
                        .push(shutdown_tx);
                }
            }

            call
        }
    }

    /// Start the server-driven audio loop (sends audio at fixed intervals)
    pub(super) async fn start_audio_loop(&self, call_id: String, call: Arc<RwLock<Call>>) {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // Configure call with shutdown signal
        {
            let mut c = call.write().await;
            c.set_shutdown(shutdown_tx);
        }

        // Get config once
        let frame_duration_ms = {
            let c = call.read().await;
            c.config.frame_duration_ms
        };

        let call_clone = call.clone();
        let call_id_clone = call_id.clone();

        // Spawn the audio loop task
        let handle = tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_millis(frame_duration_ms));

            clog_info!(
                "Audio loop started for call {} ({}ms frames)",
                call_id_clone,
                frame_duration_ms
            );

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // CRITICAL: Minimize write lock duration to prevent blocking incoming audio
                        // Only hold lock for mixing, NOT for broadcasting
                        let (frames, audio_tx) = {
                            let mut c = call_clone.write().await;

                            // Only tick if there are participants
                            if c.mixer.participant_count() == 0 {
                                continue;
                            }

                            // Pull per-sender audio frames (SFU pattern)
                            let frames = c.tick();
                            let audio_tx = c.audio_tx.clone();

                            (frames, audio_tx)
                        };  // <-- Write lock released here, before broadcasting

                        // Broadcast per-sender frames to all participants WITHOUT holding write lock
                        // Each receiver filters out their own handle (mix-minus)
                        for (sender_handle, user_id, audio) in frames {
                            if audio_tx.send((sender_handle, user_id, audio)).is_err() {
                                // No receivers — acceptable
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        clog_info!("Audio loop shutdown for call {}", call_id_clone);
                        break;
                    }
                }
            }
        });

        // Track the loop
        let mut loops = self.audio_loops.write().await;
        loops.insert(call_id, handle);
    }

    /// Stop audio loop for a call
    pub(super) async fn stop_audio_loop(&self, call_id: &str) {
        // Signal shutdown
        {
            let calls = self.calls.read().await;
            if let Some(call) = calls.get(call_id) {
                let mut c = call.write().await;
                c.shutdown().await;
            }
        }

        // Remove and abort the task
        let mut loops = self.audio_loops.write().await;
        if let Some(handle) = loops.remove(call_id) {
            handle.abort();
        }
    }

    /// Canonicalize a wire `call_id` to the airc [`RoomId`] string that keys ALL call
    /// state — the ONE gate that makes "the call room IS the airc room" true by
    /// construction (#193, [[all-rooms-are-airc-rooms-no-mirrors]],
    /// [[call-identity-is-airc-room-id-owned-by-airc-across-the-mesh]]). A `call_id` that
    /// is already an airc room uuid passes through as its canonical string; anything else
    /// is REJECTED loud — there is no parallel `call_id` namespace to fall back to.
    ///
    /// Name → RoomId derivation belongs at the CLIENT (which holds airc's mesh identity),
    /// not here: a foreign airc peer dialing this call can't derive OUR names, so the wire
    /// carries the airc RoomId. This keeps the id owned by airc across the whole mesh.
    pub(super) fn require_airc_room(call_id: &str) -> Result<String, String> {
        super::super::call_room::call_room_id_if_uuid(call_id)
            .map(|room| room.to_string())
            .ok_or_else(|| {
                format!(
                    "call_id '{call_id}' is not an airc room id — the call room IS the airc \
                     room; send the airc RoomId (uuid), not a name. A name resolves to its \
                     RoomId through airc (derive_room_id) BEFORE dialing the call."
                )
            })
    }

    /// Join a participant to a call
    /// is_ai: If true, creates AI participant with server-side audio buffering
    pub async fn join_call(
        &self,
        call_id: &str,
        user_id: &str,
        display_name: &str,
        is_ai: bool,
    ) -> Result<CallJoinResult, String> {
        // The gate: every call is keyed by its canonical airc RoomId, or refused. No rogue
        // call_id ever enters the maps.
        let call_id = Self::require_airc_room(call_id)?;
        let call_id = call_id.as_str();
        let call = self.get_or_create_call(call_id).await;
        let handle = Handle::new();

        // Add participant to call
        // AI participants get a ring buffer for server-paced audio playback
        // Human participants get VAD for speech detection
        {
            let mut call = call.write().await;
            let stream = if is_ai {
                clog_info!(
                    "🤖 Creating AI participant {} with ring buffer",
                    display_name
                );
                ParticipantStream::new_ai(handle, user_id.to_string(), display_name.to_string())
            } else {
                ParticipantStream::new(handle, user_id.to_string(), display_name.to_string())
            };

            // Initialize VAD for speech detection and transcription (humans only)
            if let Err(e) = call.mixer.add_participant_with_init(stream).await {
                clog_error!("Failed to initialize VAD for {}: {:?}", display_name, e);
            }
        }

        // Track participant -> call mapping
        {
            let mut participant_calls = self.participant_calls.write().await;
            participant_calls.insert(handle, call_id.to_string());
        }

        // TELL THE CORE (#58). This is the wire that was never run: join_call has always
        // known a call started, held the call id — which IS the airc RoomId, enforced by
        // require_airc_room above — and the joiner, and told nobody. Registration was left
        // to whichever client implemented it, so exactly one did (Node, in legacy/, tag legacy-archive-2026-10-23), and
        // every other client's citizens were structurally voiceless.
        //
        // Fired AFTER the participant is in the maps, so the registrar can never observe a
        // call the transport does not yet have. Absent registrar = unchanged behaviour;
        // the live-call view renders that call as unregistered rather than hiding it.
        if let Some(register) = &self.session_registrar {
            register(call_id, user_id, display_name, is_ai);
        }

        // Subscribe to all broadcast channels
        let (audio_rx, transcription_rx, video_rx, message_rx) = {
            let call = call.read().await;
            (
                call.audio_tx.subscribe(),
                call.transcription_tx.subscribe(),
                call.video_tx.subscribe(),
                call.message_tx.subscribe(),
            )
        };

        clog_info!(
            "Participant {} ({}) joined call {}",
            display_name,
            handle.short(),
            call_id
        );
        Ok(CallJoinResult {
            handle,
            audio_rx,
            transcription_rx,
            video_rx,
            message_rx,
        })
    }

    /// Join a participant to a call with model-specific capabilities
    /// This enables heterogeneous conversations where audio-native models (GPT-4o)
    /// can hear TTS from text-only models (Claude) and vice versa.
    pub async fn join_call_with_model(
        &self,
        call_id: &str,
        user_id: &str,
        display_name: &str,
        model_id: &str,
    ) -> Result<CallJoinResult, String> {
        // AI participants always get server-side buffering (join_call gates the airc room id).
        let result = self.join_call(call_id, user_id, display_name, true).await?;

        // Create routed participant with model capabilities
        let participant = RoutedParticipant::ai(
            user_id.to_string(),
            display_name.to_string(),
            model_id,
            &self.capability_registry,
        );

        // Log routing info
        let caps = &participant.routing.capabilities;
        clog_info!(
            "🎯 Model {} joined with routing: audio_in={}, audio_out={}, needs_stt={}, needs_tts={}",
            model_id,
            caps.audio_input,
            caps.audio_output,
            caps.needs_stt(),
            caps.needs_tts()
        );

        // Add to audio router for capability-based routing
        self.audio_router.add_participant(participant).await;

        Ok(result)
    }

    /// Inject TTS audio into a call (for text-only models speaking)
    /// This routes the TTS audio to all audio-capable participants so they can hear it.
    pub async fn inject_tts_audio(
        &self,
        call_id: &str,
        from_handle: &Handle,
        display_name: &str,
        text: &str,
        samples: Vec<i16>,
    ) {
        let call = {
            let calls = self.calls.read().await;
            calls.get(call_id).cloned()
        };

        if let Some(call) = call {
            // Add TTS audio to the mixer so it gets mixed for all participants
            let mut call = call.write().await;

            // Push the TTS audio as if it came from this participant
            // The mixer will include it in mix-minus for everyone else to hear
            call.mixer.push_audio(from_handle, samples.clone());

            clog_info!(
                "🔊 Injected TTS audio for {} into call {} ({} samples, \"{}\")",
                display_name,
                call_id,
                samples.len(),
                text.chars().take(TEXT_PREVIEW_LENGTH).collect::<String>()
            );

            // Also route through audio router for capability-aware handling
            self.audio_router
                .route_tts_audio(
                    &from_handle.to_string(),
                    display_name,
                    text,
                    samples,
                    AUDIO_SAMPLE_RATE,
                )
                .await;
        }
    }

    /// Leave a call
    pub async fn leave_call(&self, handle: &Handle) {
        let call_id = {
            let mut participant_calls = self.participant_calls.write().await;
            participant_calls.remove(handle)
        };

        if let Some(call_id) = call_id {
            let (should_cleanup, user_id) = {
                let calls = self.calls.read().await;
                if let Some(call) = calls.get(&call_id) {
                    let mut call = call.write().await;
                    let user_id = if let Some(stream) = call.mixer.remove_participant(handle) {
                        clog_info!(
                            "Participant {} ({}) left call {}",
                            stream.display_name,
                            handle.short(),
                            call_id
                        );
                        Some(stream.user_id.clone())
                    } else {
                        None
                    };
                    // Check if call is now empty
                    (call.mixer.participant_count() == 0, user_id)
                } else {
                    (false, None)
                }
            };

            // Remove from audio router if this was a model-aware participant
            if let Some(user_id) = user_id {
                self.audio_router.remove_participant(&user_id).await;
            }

            // Cleanup empty call
            if should_cleanup {
                self.stop_audio_loop(&call_id).await;

                // Stop all video sources for this call
                {
                    let mut shutdowns = self.video_source_shutdowns.write().await;
                    if let Some(sources) = shutdowns.remove(&call_id) {
                        for tx in sources {
                            let _ = tx.send(()).await;
                        }
                    }
                }

                // Unload idle avatar models now that video sources are shutting down.
                // render_loop::release_slot() handles its own unloads, but this catches
                // any slots that were loaded but never got a render loop (race on join/leave).
                // No renderer exists without `avatar-3d`, so nothing to unload there.
                #[cfg(feature = "avatar-3d")]
                {
                    if let Some(bevy) = crate::live::video::bevy_renderer::try_get() {
                        let _ = bevy
                            .command_sender()
                            .send(crate::live::video::bevy_renderer::AvatarCommand::UnloadIdle);
                    }
                }

                let mut calls = self.calls.write().await;
                calls.remove(&call_id);
                clog_info!("Call {} cleaned up (no participants)", call_id);
            }
        }
    }

    /// Push audio from a participant (buffered, mixed by audio loop)
    /// If speech ends, triggers transcription and broadcasts result
    pub async fn push_audio(&self, handle: &Handle, samples: Vec<i16>) {
        // STEP 1: Lookup call ID (fast, read-only)
        let call_id = {
            let participant_calls = self.participant_calls.read().await;
            participant_calls.get(handle).cloned()
        };

        if let Some(call_id) = call_id {
            // STEP 2: Push audio and check for speech end (minimized write lock)
            let (result, transcription_tx) = {
                let calls = self.calls.read().await;
                if let Some(call) = calls.get(&call_id) {
                    // Only hold write lock for the actual push operation
                    let mut call = call.write().await;
                    let result = call.push_audio(handle, samples);
                    let transcription_tx = call.transcription_tx.clone();
                    drop(call); // Explicitly release write lock early

                    (Some(result), Some(transcription_tx))
                } else {
                    (None, None)
                }
            };

            // STEP 3: Spawn transcription task if speech ended (no locks held)
            if let (Some(result), Some(transcription_tx)) = (result, transcription_tx) {
                if result.speech_ended {
                    if let (Some(user_id), Some(display_name), Some(speech_samples)) =
                        (result.user_id, result.display_name, result.speech_samples)
                    {
                        continuum_base::probe!(
                            class = "live.stt.speech_ended",
                            module = "live",
                            call_id = call_id.as_str(),
                            speaker = user_id.as_str(),
                            samples = speech_samples.len() as u64,
                            "VAD closed an utterance — whisper runs next (or the semaphore refuses)"
                        );
                        // Try to acquire semaphore permit (non-blocking)
                        // If we can't, drop this audio to prevent backlog
                        let semaphore = TRANSCRIPTION_SEMAPHORE.clone();
                        match semaphore.clone().try_acquire_owned() {
                            Ok(permit) => {
                                // Spawn transcription task with permit. The sink
                                // (when installed) turns the utterance into a ROOM
                                // message so citizens PERCEIVE speech — subtitles
                                // alone reached screens and no minds (2026-09-01).
                                let sink = self.transcript_sink.clone();
                                let room = call_id.clone();
                                tokio::spawn(async move {
                                    let text = Self::transcribe_and_broadcast(
                                        transcription_tx,
                                        user_id.clone(),
                                        display_name.clone(),
                                        speech_samples,
                                    )
                                    .await;
                                    continuum_base::probe!(
                                        class = "live.stt.transcribed",
                                        module = "live",
                                        call_id = room.as_str(),
                                        speaker = user_id.as_str(),
                                        chars = text.as_ref().map(|t| t.chars().count()).unwrap_or(0) as u64, // unwrap_or: None = whisper returned nothing; 0 chars IS that fact, the row says so
                                        to_minds = sink.is_some(),
                                        "whisper returned (chars 0 = empty/failed); to_minds = a transcript sink turns it into a room line"
                                    );
                                    if let (Some(sink), Some(text)) = (sink, text) {
                                        sink(&room, &user_id, &display_name, &text);
                                    }
                                    // Permit automatically released when dropped
                                    drop(permit);
                                });
                            }
                            Err(_) => {
                                // Queue full - drop this audio to stay current
                                continuum_base::probe!(
                                    class = "live.stt.refused",
                                    module = "live",
                                    call_id = call_id.as_str(),
                                    speaker = user_id.as_str(),
                                    reason = "transcription_queue_full"
                                );
                                clog_warn!(
                                    "🚨 Dropping audio from {} - transcription queue full ({} max)",
                                    display_name,
                                    MAX_CONCURRENT_TRANSCRIPTIONS
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Push a video frame from a participant (broadcast to all others via mix-minus)
    /// frame_data is [VideoFrameHeader (16 bytes)][pixel data] — no FrameKind prefix
    pub async fn push_video(&self, handle: &Handle, frame_data: Vec<u8>) {
        let call_id = {
            let participant_calls = self.participant_calls.read().await;
            participant_calls.get(handle).cloned()
        };

        if let Some(call_id) = call_id {
            let calls = self.calls.read().await;
            if let Some(call) = calls.get(&call_id) {
                let call = call.read().await;
                // Look up user_id from mixer — the sender's identity for client-side routing
                let user_id = call
                    .mixer
                    .find_user_id_by_handle(handle)
                    .unwrap_or_else(|| "unknown".to_string());
                // Broadcast with sender handle + user_id — receivers filter out their own frames
                if call.video_tx.send((*handle, user_id, frame_data)).is_err() {
                    // No receivers — this is fine, means nobody has video enabled
                }
            }
        }
    }

    /// Tee a pre-rendered avatar RGBA frame into a call's native video plane (#193/#172).
    ///
    /// A persona is not a native WS participant — she publishes to LiveKit via the
    /// bridge. But the SAME Bevy-rendered frame the avatar pump sends to LiveKit is
    /// tee'd here so native clients (positron web "Go live", the glass-box harness)
    /// see her real face instead of the placeholder test pattern. Render once, two
    /// sinks — no second Bevy slot, no parallel renderer.
    ///
    /// The caller (the pump) owns a stable `source_handle` (created once per pump)
    /// so mix-minus filtering is consistent across frames, and a monotonic
    /// `sequence` + `timestamp_ms` for the frame header. `user_id` is the persona's
    /// uuid — the label native clients key their tile on. No-op when no native Call
    /// exists yet (no WS client has the call open) or nobody is subscribed.
    pub async fn push_avatar_frame(
        &self,
        call_id: &str,
        user_id: &str,
        source_handle: Handle,
        sequence: u32,
        timestamp_ms: u32,
        rgba: &[u8],
        width: u16,
        height: u16,
    ) {
        let call = {
            let calls = self.calls.read().await;
            match calls.get(call_id) {
                Some(c) => c.clone(),
                None => return, // no native Call — nobody has this call open
            }
        };
        let video_tx = {
            let call = call.read().await;
            call.video_tx.clone()
        };
        let header = crate::live::types::VideoFrameHeader {
            width,
            height,
            pixel_format: crate::live::types::VideoPixelFormat::RGBA8,
            timestamp_ms,
            sequence,
        };
        let frame = crate::live::types::VideoFrame {
            header,
            data: rgba.to_vec(),
        };
        // No receivers is fine (call open by nobody) — same contract as push_video.
        let _ = video_tx.send((source_handle, user_id.to_string(), frame.to_bytes()));
    }

    /// Tee a persona's synthesized voice into a call's native audio plane (#193 audio
    /// convergence — the sibling of [`push_avatar_frame`]).
    ///
    /// A persona speaks via LiveKit (`speak_in_call` → bridge), but native clients
    /// (positron web "Go live", the glass-box harness) read the native mixer — which,
    /// with only a lone listener, plays HOLD MUSIC. This registers the persona as a
    /// virtual AI sender in the call's mixer and dumps her TTS PCM into its AI ring
    /// buffer; the audio loop then pulls it frame-by-frame and broadcasts it, and her
    /// presence makes the call no longer "alone" so the hold-music fill stops. Render
    /// once, two sinks: the SAME synthesized samples go to LiveKit and here.
    ///
    /// Self-healing: the per-(call,persona) handle is stable across utterances, and if
    /// the call was torn down + recreated (mixer no longer knows the handle) the
    /// persona is re-registered. No-op when no native Call exists yet.
    pub async fn push_persona_audio(
        &self,
        call_id: &str,
        user_id: &str,
        display_name: &str,
        samples: Vec<i16>,
    ) {
        let call = {
            let calls = self.calls.read().await;
            match calls.get(call_id) {
                Some(c) => c.clone(),
                None => return, // no native Call — nobody has this call open
            }
        };
        let key = format!("{call_id}\0{user_id}");
        let handle = {
            let mut handles = self.persona_audio_handles.write().await;
            *handles.entry(key).or_insert_with(Handle::new)
        };
        let mut call = call.write().await;
        // Ensure the persona is a live AI participant in THIS mixer (self-heal across
        // call recreation). AI participants carry a ring buffer sized for whole
        // utterances dumped at once — exactly this path.
        if call.mixer.find_user_id_by_handle(&handle).is_none() {
            call.mixer
                .add_participant(crate::live::audio::mixer::ParticipantStream::new_ai(
                    handle,
                    user_id.to_string(),
                    display_name.to_string(),
                ));
        }
        // Dump the whole utterance; the audio loop drains it frame-by-frame at cadence.
        let _ = call.push_audio(&handle, samples);
    }

    /// Bridge-fed HUMAN audio (the LiveKit media lane): route a remote
    /// participant's 16k PCM into the SAME VAD → speech-end → transcription
    /// pipeline the WS lane has always used.
    ///
    /// This closes the STT dead leg (found live 2026-09-01: the bridge heard
    /// the first audio frame and nothing followed — `bridge_client`'s frame
    /// loop accumulated perfect VAD-sized chunks into a `TODO` and dropped
    /// them, so citizens never answered speech). The human's handle + VAD
    /// already exist from her WS-control `join_call`; media frames just never
    /// reached them. One lookup, then the existing `push_audio` does speech-end
    /// detection and spawns transcription exactly as before — no second
    /// pipeline ([[the-same-bug-at-two-sites-is-a-missing-constraint-not-two-bugs]]).
    pub async fn push_remote_human_audio(
        &self,
        call_id: &str,
        user_id: &str,
        samples: Vec<i16>,
    ) {
        let call = {
            let calls = self.calls.read().await;
            calls.get(call_id).cloned()
        };
        let Some(call) = call else {
            // No native Call — nobody has this call open on this node. Silent
            // once; on 2026-09-05 Joel spoke into a live call and no caption
            // ever appeared, and this return was one of two places the audio
            // could vanish without a trace. Now it says so (once per speaker
            // per call, not per 10 ms frame).
            Self::note_remote_audio_dropped(call_id, user_id, "no_native_call");
            return;
        };
        let handle = {
            let call = call.read().await;
            call.mixer.find_handle_by_user_id(user_id)
        };
        let Some(handle) = handle else {
            // Media arrived before (or without) the WS-control join that mints
            // her handle + VAD — a real ordering that self-heals when the join
            // lands; frames until then are honestly droppable (they predate
            // her being a participant here). The bridge's speaker id is the
            // LiveKit identity; the mixer's is the WS join's user id — when the
            // two vocabularies differ this is where captions die.
            Self::note_remote_audio_dropped(call_id, user_id, "no_mixer_handle");
            return;
        };
        Self::note_human_audio_level("livekit", user_id, &samples);
        self.push_audio(&handle, samples).await;
    }
}
