use super::*;

/// Response field name for voice responder IDs
pub(super) const VOICE_RESPONSE_FIELD_RESPONDER_IDS: &str = "responder_ids";

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
    pub(super) active_sessions: std::sync::Mutex<HashSet<String>>,
    /// #193 slice A legacy aliases: client-minted `session_id` → canonical airc
    /// `room_id`. The server is now AUTHORITATIVE that a call IS its airc room —
    /// every call is keyed by room_id. Until the client cutover (slice B) lands,
    /// clients may still address verbs by their minted session_id; this map lets
    /// [`VoiceState::canonical_call_id`] resolve those to the room. Entries are
    /// dropped when their call ends. Empty once slice B ships — that's the
    /// done-signal.
    pub(super) legacy_call_aliases: std::sync::Mutex<HashMap<String, String>>,
    /// Late-bound command executor for the vision describer the perception-ingest
    /// drain builds. Installed post-boot by the runtime (the #224 late-bind slot),
    /// read lazily on the first live-call frame — never present at `initialize`.
    pub(super) executor: LateBound<CommandExecutor>,
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
            .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: a poisoned set still answers membership; refusing would mute every call
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
            .push_persona_audio(call_id, user_id, display_name.unwrap_or(user_id), samples) // unwrap_or: no display name → the identity string labels the audio, honest and unique
            .await;
        Ok((num_samples, duration_ms, sample_rate))
    }

    pub(super) fn canonical_call_id(&self, id: &str) -> String {
        let aliases = self
            .legacy_call_aliases
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        aliases.get(id).cloned().unwrap_or_else(|| id.to_string())
    }
}

pub struct VoiceModule {
    pub(super) state: Arc<VoiceState>,
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
pub(super) async fn perception_ingest_drain(
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
        continuum_base::probe!(
            class = "live.perception.admitted",
            call_id = frame.call_id.as_str(),
            speaker = frame.speaker_id.as_str(),
            received_at_ms = frame.received_at_ms,
            admission_delay_ms = EpochMs::now().millis()
                .saturating_sub(frame.received_at_ms),
            viewers = viewers.len() as u64,
            "bridge receipt to perception fan-out (not remote capture latency)"
        );
        ingest.as_ref().expect("ingest built above").fan_out(
            &frame.speaker_id,
            &viewers,
            frame.jpeg,
            &frame.mime,
            frame.received_at_ms,
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
