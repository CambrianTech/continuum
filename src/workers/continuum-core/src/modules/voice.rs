//! VoiceModule — wraps voice synthesis, transcription, and call management.
//!
//! Handles: voice/register-session, voice/on-utterance, voice/should-route-tts,
//!          voice/synthesize, voice/speak-in-call, voice/synthesize-handle,
//!          voice/play-handle, voice/discard-handle, voice/transcribe,
//!          voice/transcribe-with-adapter, voice/stt-list,
//!          voice/test-audio-generate,
//!          voice/inject-audio, voice/ambient-add, voice/ambient-inject,
//!          voice/ambient-remove
//!
//! Priority: Realtime — voice operations are time-critical.

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use crate::voice::{UtteranceEvent, VoiceParticipant};
use crate::voice::voice_service::VoiceService;
use crate::voice::livekit_agent::LiveKitAgentManager;
use crate::voice::audio_buffer::AudioBufferPool;
use crate::logging::TimingGuard;
use crate::utils::params::Params;
use crate::{log_info, log_error};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

/// Response field name for voice responder IDs
const VOICE_RESPONSE_FIELD_RESPONDER_IDS: &str = "responder_ids";

/// Shared state for voice module.
pub struct VoiceState {
    pub voice_service: Arc<VoiceService>,
    pub livekit_manager: Arc<LiveKitAgentManager>,
    pub audio_pool: Arc<AudioBufferPool>,
}

impl VoiceState {
    pub fn new(
        voice_service: Arc<VoiceService>,
        livekit_manager: Arc<LiveKitAgentManager>,
        audio_pool: Arc<AudioBufferPool>,
    ) -> Self {
        Self { voice_service, livekit_manager, audio_pool }
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

#[async_trait]
impl ServiceModule for VoiceModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "voice",
            priority: ModulePriority::Realtime,
            command_prefixes: &["voice/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        let p = Params::new(&params);

        match command {
            "voice/register-session" => {
                let _timer = TimingGuard::new("module", "voice_register_session");
                let session_id = p.str("session_id")?;
                let room_id = p.str("room_id")?;
                let participants: Vec<VoiceParticipant> = p.json_or("participants");

                self.state.voice_service.register_session(session_id, room_id, participants)?;

                // Spawn STT listener agent to subscribe to human audio in this call.
                // The listener runs VAD → STT → publishes transcriptions via LiveKit.
                let livekit_manager = self.state.livekit_manager.clone();
                let call_id = session_id.to_string();
                tokio::spawn(async move {
                    if let Err(e) = livekit_manager.join_as_listener(&call_id).await {
                        log_error!("module", "voice_register_session",
                            "Failed to spawn STT listener for call {}: {}",
                            &call_id[..8.min(call_id.len())], e);
                    }
                });

                Ok(CommandResult::Json(serde_json::json!({ "registered": true })))
            }

            "voice/on-utterance" => {
                let _timer = TimingGuard::new("module", "voice_on_utterance");
                let event: UtteranceEvent = p.json("event")?;

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

                use crate::voice::tts_service;
                let synthesis = tts_service::synthesize_speech_async(text, voice, adapter).await
                    .map_err(|e| {
                        log_error!("module", "voice_synthesize", "TTS failed: {}", e);
                        format!("TTS synthesis failed: {}", e)
                    })?;

                let pcm_bytes: Vec<u8> = synthesis.samples.iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect();

                log_info!(
                    "module", "voice_synthesize",
                    "Synthesized {} samples at {}Hz ({:.1}s) → {} bytes raw PCM",
                    synthesis.samples.len(), synthesis.sample_rate,
                    synthesis.duration_ms as f64 / 1000.0, pcm_bytes.len()
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
                let call_id = p.str("call_id")?;
                let user_id = p.str("user_id")?;
                let text = p.str("text")?;
                let voice = p.str_opt("voice");
                let adapter = p.str_opt("adapter");

                let (num_samples, duration_ms, sample_rate) = self.state.livekit_manager
                    .speak_in_call(call_id, user_id, text, voice, adapter)
                    .await
                    .map_err(|e| {
                        log_error!("module", "voice_speak_in_call", "Speak-in-call failed: {}", e);
                        format!("Speak-in-call failed: {}", e)
                    })?;

                log_info!(
                    "module", "voice_speak_in_call",
                    "Injected {} samples ({:.1}s) into call {} for user {}",
                    num_samples, duration_ms as f64 / 1000.0, call_id, user_id
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

                use crate::voice::tts_service;
                let synthesis = tts_service::synthesize_speech_async(text, voice, adapter).await
                    .map_err(|e| {
                        log_error!("module", "voice_synthesize_handle", "TTS failed: {}", e);
                        format!("TTS synthesis failed: {}", e)
                    })?;

                let adapter_name = adapter.unwrap_or("default");
                let info = self.state.audio_pool.store(
                    synthesis.samples, synthesis.sample_rate,
                    synthesis.duration_ms, adapter_name,
                );

                log_info!(
                    "module", "voice_synthesize_handle",
                    "Stored handle {} ({} samples, {}ms, {})",
                    &info.handle[..8], info.sample_count, info.duration_ms, info.adapter
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
                let call_id = p.str("call_id")?;
                let user_id = p.str("user_id")?;

                use crate::voice::handle::Handle as VoiceHandle;
                let voice_handle: VoiceHandle = handle.parse()
                    .map_err(|e| format!("Invalid handle UUID: {}", e))?;

                let samples = self.state.audio_pool.get(&voice_handle)
                    .ok_or_else(|| format!("Audio handle not found or expired: {}", &handle[..8.min(handle.len())]))?;

                let sample_count = samples.len();
                let duration_ms = (sample_count as u64 * 1000) / crate::audio_constants::AUDIO_SAMPLE_RATE as u64;

                self.state.livekit_manager.inject_audio(call_id, user_id, samples).await
                    .map_err(|e| {
                        log_error!("module", "voice_play_handle", "Failed to inject audio: {}", e);
                        format!("Failed to inject audio: {}", e)
                    })?;

                log_info!(
                    "module", "voice_play_handle",
                    "Played handle {} into call {} for user {} ({} samples, {}ms)",
                    &handle[..8], call_id, user_id, sample_count, duration_ms
                );
                Ok(CommandResult::Json(serde_json::json!({
                    "played": true,
                    "sample_count": sample_count,
                    "duration_ms": duration_ms
                })))
            }

            "voice/discard-handle" => {
                let handle = p.str("handle")?;

                use crate::voice::handle::Handle as VoiceHandle;
                let voice_handle: VoiceHandle = handle.parse()
                    .map_err(|e| format!("Invalid handle UUID: {}", e))?;

                let discarded = self.state.audio_pool.discard(&voice_handle);
                Ok(CommandResult::Json(serde_json::json!({ "discarded": discarded })))
            }

            "voice/transcribe" => {
                let _timer = TimingGuard::new("module", "voice_transcribe");
                let audio = p.str("audio")?;
                let language = p.str_opt("language");

                use crate::voice::stt_service;
                use base64::Engine;

                let bytes = base64::engine::general_purpose::STANDARD.decode(audio)
                    .map_err(|e| {
                        log_error!("module", "voice_transcribe", "Base64 decode failed: {}", e);
                        format!("Base64 decode failed: {}", e)
                    })?;

                if bytes.len() % 2 != 0 {
                    return Err("Audio data must be even length (16-bit samples)".to_string());
                }

                let samples: Vec<i16> = bytes.chunks_exact(2)
                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();

                log_info!(
                    "module", "voice_transcribe",
                    "Transcribing {} samples ({:.1}s)",
                    samples.len(),
                    samples.len() as f64 / crate::audio_constants::AUDIO_SAMPLE_RATE as f64
                );

                let transcript = stt_service::transcribe_speech_async(&samples, language).await
                    .map_err(|e| {
                        log_error!("module", "voice_transcribe", "STT failed: {}", e);
                        format!("STT failed: {}", e)
                    })?;

                log_info!(
                    "module", "voice_transcribe",
                    "Transcribed: \"{}\" (confidence: {:.2})",
                    transcript.text, transcript.confidence
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
                let call_id = p.str("call_id")?;
                let user_id = p.str("user_id")?;
                let samples: Vec<i16> = p.json("samples")?;

                self.state.livekit_manager.inject_audio(call_id, user_id, samples).await
                    .map_err(|e| {
                        log_error!("module", "voice_inject_audio", "inject-audio failed: {}", e);
                        format!("inject-audio failed: {}", e)
                    })?;

                log_info!("module", "voice_inject_audio", "Injected audio into call {} for {}", call_id, user_id);
                Ok(CommandResult::Json(serde_json::json!({ "success": true })))
            }

            "voice/ambient-add" => {
                let _timer = TimingGuard::new("module", "voice_ambient_add");
                let call_id = p.str("call_id")?;
                let source_name = p.str("source_name")?;

                let handle = self.state.livekit_manager.add_ambient_source(call_id, source_name).await
                    .map_err(|e| {
                        log_error!("module", "voice_ambient_add", "ambient-add failed: {}", e);
                        format!("ambient-add failed: {}", e)
                    })?;

                log_info!("module", "voice_ambient_add", "Added ambient source '{}' to call {}", source_name, call_id);
                Ok(CommandResult::Json(serde_json::json!({
                    "handle": handle,
                    "source_name": source_name,
                })))
            }

            "voice/ambient-inject" => {
                let _timer = TimingGuard::new("module", "voice_ambient_inject");
                let call_id = p.str("call_id")?;
                let handle_str = p.str("handle")?;
                let samples: Vec<i16> = p.json("samples")?;

                self.state.livekit_manager.inject_ambient(call_id, handle_str, samples).await
                    .map_err(|e| {
                        log_error!("module", "voice_ambient_inject", "ambient-inject failed: {}", e);
                        format!("ambient-inject failed: {}", e)
                    })?;

                Ok(CommandResult::Json(serde_json::json!({ "success": true })))
            }

            "voice/stt-list" => {
                let _timer = TimingGuard::new("module", "voice_stt_list");

                use crate::voice::stt;
                let registry = stt::get_registry();
                let reg = registry.read();

                let adapters: Vec<serde_json::Value> = reg.list().iter().map(|(name, initialized)| {
                    let desc = reg.get(name)
                        .map(|a| a.description().to_string())
                        .unwrap_or_default();
                    serde_json::json!({
                        "name": name,
                        "initialized": initialized,
                        "description": desc,
                    })
                }).collect();

                let active = reg.get_active()
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

                use crate::voice::stt;
                use crate::utils::audio::i16_to_f32;
                use base64::Engine;

                let bytes = base64::engine::general_purpose::STANDARD.decode(audio)
                    .map_err(|e| {
                        log_error!("module", "voice_transcribe_with_adapter", "Base64 decode failed: {}", e);
                        format!("Base64 decode failed: {}", e)
                    })?;

                if bytes.len() % 2 != 0 {
                    return Err("Audio data must be even length (16-bit samples)".to_string());
                }

                let samples: Vec<i16> = bytes.chunks_exact(2)
                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();

                log_info!(
                    "module", "voice_transcribe_with_adapter",
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
                        format!("STT adapter '{}' not found. Available: {:?}",
                            adapter_name,
                            reg.list().iter().map(|(n, _)| *n).collect::<Vec<_>>())
                    })?
                };

                // Initialize if not yet ready
                if !adapter.is_initialized() {
                    adapter.initialize().await.map_err(|e| {
                        log_error!("module", "voice_transcribe_with_adapter",
                            "Failed to initialize adapter '{}': {}", adapter_name, e);
                        format!("Failed to initialize adapter '{}': {}", adapter_name, e)
                    })?;
                }

                let f32_samples = i16_to_f32(&samples);
                let transcript = adapter.transcribe(f32_samples, language).await
                    .map_err(|e| {
                        log_error!("module", "voice_transcribe_with_adapter",
                            "STT failed with adapter '{}': {}", adapter_name, e);
                        format!("STT failed with adapter '{}': {}", adapter_name, e)
                    })?;

                log_info!(
                    "module", "voice_transcribe_with_adapter",
                    "Transcribed with '{}': \"{}\" (confidence: {:.2})",
                    adapter_name, transcript.text, transcript.confidence
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

                use crate::voice::vad::{TestAudioGenerator, NoiseType};
                use base64::Engine;

                let noise_type = NoiseType::from_name(noise_type_str, params)
                    .map_err(|e| format!("Invalid noise type: {}", e))?;

                let gen = TestAudioGenerator::default();
                let duration_samples = (crate::audio_constants::AUDIO_SAMPLE_RATE as u64
                    * duration_ms as u64 / 1000) as usize;

                let samples = gen.generate_noise(&noise_type, duration_samples);

                // Encode as base64 i16 LE PCM
                let pcm_bytes: Vec<u8> = samples.iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect();
                let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&pcm_bytes);

                log_info!(
                    "module", "voice_test_audio_generate",
                    "Generated {} noise: {} samples ({}ms)",
                    noise_type.label(), samples.len(), duration_ms
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "audio": audio_b64,
                    "samples": samples.len(),
                    "duration_ms": duration_ms,
                    "noise_type": noise_type.label(),
                    "sample_rate": crate::audio_constants::AUDIO_SAMPLE_RATE,
                })))
            }

            "voice/ambient-remove" => {
                let _timer = TimingGuard::new("module", "voice_ambient_remove");
                let call_id = p.str("call_id")?;
                let handle_str = p.str("handle")?;

                self.state.livekit_manager.remove_ambient_source(call_id, handle_str).await
                    .map_err(|e| {
                        log_error!("module", "voice_ambient_remove", "ambient-remove failed: {}", e);
                        format!("ambient-remove failed: {}", e)
                    })?;

                log_info!("module", "voice_ambient_remove", "Removed ambient source from call {}", call_id);
                Ok(CommandResult::Json(serde_json::json!({ "removed": true })))
            }

            _ => Err(format!("Unknown voice command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}
