//! Test-audio generation and the voice selftest.

use super::super::*;

impl VoiceModule {
    pub(super) async fn cmd_test_audio_generate(&self, p: &Params<'_>) -> Result<CommandResult, String> {
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

    pub(super) async fn cmd_selftest(&self, p: &Params<'_>) -> Result<CommandResult, String> {
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
}
