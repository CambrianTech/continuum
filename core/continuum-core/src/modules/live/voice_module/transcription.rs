//! STT: transcribe, adapter listing, adapter transcribe, transcript polling.

use super::super::*;

impl VoiceModule {
    pub(super) async fn cmd_transcribe(&self, p: &Params<'_>) -> Result<CommandResult, String> {
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

    pub(super) async fn cmd_stt_list(&self, _p: &Params<'_>) -> Result<CommandResult, String> {
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

    pub(super) async fn cmd_transcribe_with_adapter(&self, p: &Params<'_>) -> Result<CommandResult, String> {
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

    pub(super) async fn cmd_poll_transcriptions(&self, p: &Params<'_>) -> Result<CommandResult, String> {
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
}
