use super::*;

impl CallManager {

    /// `live.stt.audio_dropped` once per second per (call, speaker, reason) WHILE
    /// dropping — the earlier once-ever dedupe reported a pre-join drop forever
    /// and hid the recovery (2026-09-05 15:2xZ: the row read as an ongoing drop
    /// an hour after the join). A summary that stops when the drop stops.
    pub(super) fn note_remote_audio_dropped(call_id: &str, user_id: &str, reason: &'static str) {
        use std::collections::HashMap;
        use std::sync::{Mutex, OnceLock};
        static TALLY: OnceLock<Mutex<HashMap<(String, String, &'static str), super::super::pump_tally::PumpTally>>> =
            OnceLock::new();
        let tally = TALLY.get_or_init(|| Mutex::new(HashMap::new()));
        let mut map = tally.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned tally map still tallies; the probe must keep speaking
        let t = map
            .entry((call_id.to_string(), user_id.to_string(), reason))
            .or_insert_with(|| super::super::pump_tally::PumpTally::new(std::time::Duration::from_secs(1)));
        if let Some(sum) = t.record(1, 0) {
            continuum_base::probe!(
                class = "live.stt.audio_dropped",
                module = "live",
                call_id = call_id,
                speaker = user_id,
                reason = reason,
                chunks = sum.frames,
                span_ms = sum.span_ms,
                "remote human audio dropped before VAD — captions cannot happen for this speaker while this repeats"
            );
        }
    }

    /// `live.stt.level` once per second per (source, speaker): the PEAK sample
    /// amplitude the VAD is about to see, by ingest path. The one number that
    /// says whether a silent transcript is a mute microphone, a dead path, or a
    /// detector threshold (2026-09-05: VAD read max_amp 26–77 of 32767 while
    /// Joel spoke, and nothing said which of the two ingest paths carried it).
    pub(crate) fn note_human_audio_level(source: &'static str, user_id: &str, samples: &[i16]) {
        use std::collections::HashMap;
        use std::sync::{Mutex, OnceLock};
        static TALLY: OnceLock<Mutex<HashMap<(&'static str, String), super::super::pump_tally::PumpTally>>> = OnceLock::new();
        let peak = samples.iter().map(|s| (*s as i32).unsigned_abs()).max().unwrap_or(0) as u64; // unwrap_or: an empty chunk has no peak; 0 is the honest reading
        let tally = TALLY.get_or_init(|| Mutex::new(HashMap::new()));
        let mut map = tally.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: same policy — keep speaking through a poisoned map
        let t = map
            .entry((source, user_id.to_string()))
            .or_insert_with(|| super::super::pump_tally::PumpTally::new(std::time::Duration::from_secs(1)));
        if let Some(sum) = t.record(samples.len() as u64, peak) {
            continuum_base::probe!(
                class = "live.stt.level",
                module = "live",
                source = source,
                speaker = user_id,
                peak = sum.max_us,
                samples = sum.bytes,
                chunks = sum.frames,
                span_ms = sum.span_ms,
                "peak amplitude of a human's audio by ingest path (i16 full scale 32767; speech peaks in the thousands)"
            );
        }
    }

    /// Transcribe speech samples and broadcast to all participants
    /// Returns the transcribed text (when non-empty) so the caller can also
    /// deliver it to cognition via the [`TranscriptSink`] — the broadcast here
    /// reaches SCREENS (subtitles); the sink reaches MINDS.
    pub(super) async fn transcribe_and_broadcast(
        transcription_tx: broadcast::Sender<TranscriptionEvent>,
        user_id: String,
        display_name: String,
        samples: Vec<i16>,
    ) -> Option<String> {
        // Check if STT is initialized
        if !stt::is_initialized() {
            clog_warn!("STT adapter not initialized - skipping transcription");
            return None;
        }

        clog_info!(
            "[STEP 5] 📝 Whisper transcription START for {} ({} samples, {:.1}s)",
            display_name,
            samples.len(),
            samples.len() as f32 / AUDIO_SAMPLE_RATE as f32
        );

        // Whisper takes f32 at the standard rate, which call audio already is (AUDIO_SAMPLE_RATE).
        let f32_samples = i16_to_f32(&samples);

        match stt::transcribe(f32_samples, Some("en")).await {
            Ok(result) => {
                let text = result.text.trim();
                if !text.is_empty() {
                    clog_info!(
                        "[STEP 5] 📝 Whisper transcription DONE: \"{}\" (confidence: {:.2})",
                        text,
                        result.confidence
                    );

                    // [STEP 6] Broadcast transcription to all participants
                    let event = TranscriptionEvent {
                        user_id: user_id.clone(),
                        display_name: display_name.clone(),
                        text: text.to_string(),
                        confidence: result.confidence,
                        language: result.language.clone(),
                    };

                    clog_info!("[STEP 6] 📡 Broadcasting transcription to WebSocket clients");

                    // ERROR RECOVERY: Broadcast with detailed error logging
                    // With 500-event buffer, failures should be extremely rare
                    // If this fails, it means ALL receivers are too slow (lagging by 500+ events)
                    if transcription_tx.send(event).is_err() {
                        clog_error!(
                            "[STEP 6] ❌ TRANSCRIPTION DROPPED: \"{}...\" from {}. \
                            ALL receivers are too slow - consider increasing buffer size or investigating blocking.",
                            text.chars().take(TEXT_PREVIEW_LENGTH).collect::<String>(),
                            display_name
                        );

                        // Critical issue already logged via tracing::error above
                    }
                    Some(text.to_string())
                } else {
                    clog_info!("📝 Empty transcription result from {}", display_name);
                    None
                }
            }
            Err(e) => {
                clog_error!("Transcription failed for {}: {}", display_name, e);
                None
            }
        }
    }

    /// Set mute state for a participant
    pub async fn set_mute(&self, handle: &Handle, muted: bool) {
        let call_id = {
            let participant_calls = self.participant_calls.read().await;
            participant_calls.get(handle).cloned()
        };

        if let Some(call_id) = call_id {
            let calls = self.calls.read().await;
            if let Some(call) = calls.get(&call_id) {
                let mut call = call.write().await;
                if let Some(participant) = call.mixer.get_participant_mut(handle) {
                    participant.muted = muted;
                    clog_info!("Participant {} muted: {}", handle.short(), muted);
                }
            }
        }
    }

    /// Synthesize text and inject directly into a call's mixer.
    /// Audio never leaves the Rust process — TypeScript only gets metadata back.
    /// Returns (num_samples, duration_ms, sample_rate) on success.
    pub async fn speak_in_call(
        &self,
        call_id: &str,
        user_id: &str,
        text: &str,
        voice: Option<&str>,
        adapter: Option<&str>,
    ) -> Result<(usize, u64, u32), String> {
        use crate::live::audio::tts_service;

        // Step 1: Verify participant is in this call
        let (handle, display_name) = {
            let calls = self.calls.read().await;
            let call = calls
                .get(call_id)
                .ok_or_else(|| format!("Call '{call_id}' not found"))?;
            let call = call.read().await;
            let handle = call
                .mixer
                .find_handle_by_user_id(user_id)
                .ok_or_else(|| format!("User '{user_id}' not in call '{call_id}'"))?;
            let display_name = call
                .mixer
                .get_participant(&handle)
                .map(|p| p.display_name.clone())
                .unwrap_or_else(|| user_id.to_string());
            (handle, display_name)
        };

        // Step 2: Synthesize (async — runs in current tokio context).
        //
        // EMOTION RIDES THE SAME SENTIMENT THE FACE USES (Stage A of
        // VOICE-ENGINE-PLAN's native-audio ladder, Joel 2026-09-02:
        // "intimately control the speech mannerisms"): the avatar's
        // sub-microsecond sentiment extraction maps to Orpheus emotion tags,
        // so voice and face express ONE state from one source — never two
        // analyzers drifting apart. Tag injection is Orpheus-only: other
        // engines would read the tag aloud as text.
        let spoken_text =
            crate::live::audio::emotion_tags::decorate_for_adapter(text, adapter);
        let synthesis =
            tts_service::synthesize_speech_async(&spoken_text, voice, adapter, None)
                .await
                .map_err(|e| format!("TTS failed: {e}"))?;

        let num_samples = synthesis.samples.len();
        let duration_ms = synthesis.duration_ms;
        let sample_rate = synthesis.sample_rate;

        clog_info!(
            "🔊 speak_in_call: Synthesized {} samples ({:.1}s) for {} in call {}",
            num_samples,
            duration_ms as f64 / 1000.0,
            display_name,
            call_id
        );

        // Step 3: Inject directly into the call mixer (audio stays in Rust)
        self.inject_tts_audio(call_id, &handle, &display_name, text, synthesis.samples)
            .await;

        Ok((num_samples, duration_ms, sample_rate))
    }

    /// Inject pre-synthesized audio (from buffer pool) into a call's mixer.
    /// Resolves user_id to participant Handle, then pushes samples.
    pub async fn inject_audio(
        &self,
        call_id: &str,
        user_id: &str,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        let (handle, display_name) = {
            let calls = self.calls.read().await;
            let call = calls
                .get(call_id)
                .ok_or_else(|| format!("Call '{call_id}' not found"))?;
            let call = call.read().await;
            let handle = call
                .mixer
                .find_handle_by_user_id(user_id)
                .ok_or_else(|| format!("User '{user_id}' not in call '{call_id}'"))?;
            let display_name = call
                .mixer
                .get_participant(&handle)
                .map(|p| p.display_name.clone())
                .unwrap_or_else(|| user_id.to_string());
            (handle, display_name)
        };

        let sample_count = samples.len();
        self.inject_tts_audio(call_id, &handle, &display_name, "(handle-based)", samples)
            .await;

        clog_info!(
            "inject_audio: {} samples into call {} for {}",
            sample_count,
            call_id,
            display_name
        );

        Ok(())
    }

    /// Add an ambient audio source to a call (TV, music, background noise).
    /// Returns a Handle for injecting audio and removing the source later.
    /// Ambient sources use AI ring buffer infrastructure for server-paced playback
    /// and are NEVER excluded from mix-minus (everyone hears them).
    pub async fn add_ambient_source(
        &self,
        call_id: &str,
        source_name: &str,
    ) -> Result<Handle, String> {
        let call = {
            let calls = self.calls.read().await;
            calls
                .get(call_id)
                .cloned()
                .ok_or_else(|| format!("Call '{call_id}' not found"))?
        };

        let handle = Handle::new();
        {
            let mut call = call.write().await;
            let stream = ParticipantStream::new_ambient(handle, source_name.to_string());
            call.mixer.add_participant(stream);
        }

        // Track ambient source -> call mapping
        {
            let mut participant_calls = self.participant_calls.write().await;
            participant_calls.insert(handle, call_id.to_string());
        }

        clog_info!(
            "🔊 Added ambient source '{}' ({}) to call {}",
            source_name,
            handle.short(),
            call_id
        );

        Ok(handle)
    }

    /// Remove an ambient audio source from a call
    pub async fn remove_ambient_source(&self, call_id: &str, handle: Handle) -> Result<(), String> {
        let call = {
            let calls = self.calls.read().await;
            calls
                .get(call_id)
                .cloned()
                .ok_or_else(|| format!("Call '{call_id}' not found"))?
        };

        {
            let mut call = call.write().await;
            call.mixer.remove_participant(&handle);
        }

        {
            let mut participant_calls = self.participant_calls.write().await;
            participant_calls.remove(&handle);
        }

        clog_info!(
            "🔊 Removed ambient source ({}) from call {}",
            handle.short(),
            call_id
        );

        Ok(())
    }

    /// Start a test video source for a call using the VideoSource trait.
    /// Generates SMPTE color bar frames at ~10fps to prove video streaming works.
    /// The source gets its own Handle (acts as a virtual participant for video only).
    /// Returns a shutdown sender — drop it or send () to stop the generator — or `None`
    /// when the call lock was busy and the source was not started.
    ///
    /// Accepts the Call Arc directly to avoid deadlocks when called from
    /// get_or_create_call() which already holds the calls write lock.
    pub(super) fn start_test_video_source_for(call: &Arc<RwLock<Call>>, call_id: &str) -> Option<mpsc::Sender<()>> {
        let video_tx = match call.try_read() {
            Ok(call_guard) => call_guard.video_tx.clone(),
            Err(_) => {
                clog_warn!(
                    "Test video source for call {} skipped: call lock busy right after creation",
                    call_id
                );
                return None;
            }
        };

        let source_handle = Handle::new();
        let source = Box::new(TestPatternSource::default_test());

        clog_info!("Starting {} for call {}", source.name(), call_id);

        Some(source.start(video_tx, source_handle))
    }

    /// Get call stats
    pub async fn get_stats(&self, handle: &Handle) -> Option<(usize, u64)> {
        let call_id = {
            let participant_calls = self.participant_calls.read().await;
            participant_calls.get(handle).cloned()
        };

        if let Some(call_id) = call_id {
            let calls = self.calls.read().await;
            if let Some(call) = calls.get(&call_id) {
                let call = call.read().await;
                return Some((call.mixer.participant_count(), call.samples_processed));
            }
        }
        None
    }

}

impl Default for CallManager {
    fn default() -> Self {
        Self::new()
    }
}
