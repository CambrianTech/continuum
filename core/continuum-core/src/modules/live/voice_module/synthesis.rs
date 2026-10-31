//! TTS: synthesize, speak-in-call, and the synthesized-handle verbs.

use super::super::*;

impl VoiceModule {
    pub(super) async fn cmd_synthesize(&self, p: &Params<'_>) -> Result<CommandResult, String> {
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

    pub(super) async fn cmd_speak_in_call(&self, p: &Params<'_>) -> Result<CommandResult, String> {
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

    pub(super) async fn cmd_synthesize_handle(&self, p: &Params<'_>) -> Result<CommandResult, String> {
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

        Ok(CommandResult::Json(serde_json::json!({
            "handle": info.handle,
            "sample_count": info.sample_count,
            "sample_rate": info.sample_rate,
            "duration_ms": info.duration_ms,
            "adapter": info.adapter,
        })))
    }

    pub(super) async fn cmd_play_handle(&self, p: &Params<'_>) -> Result<CommandResult, String> {
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
                voice_handle.short()
            )
        })?;

        let sample_count = samples.len();
        let duration_ms =
            (sample_count as u64 * 1000) / continuum_base::audio_constants::AUDIO_SAMPLE_RATE as u64;

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
            voice_handle.short(),
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

    pub(super) async fn cmd_discard_handle(&self, p: &Params<'_>) -> Result<CommandResult, String> {
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
}
