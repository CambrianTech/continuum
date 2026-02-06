/// VoiceModule — wraps voice synthesis, transcription, and call management.
///
/// Handles: voice/register-session, voice/on-utterance, voice/should-route-tts,
///          voice/synthesize, voice/speak-in-call, voice/synthesize-handle,
///          voice/play-handle, voice/discard-handle, voice/transcribe
///
/// Priority: Realtime — voice operations are time-critical.

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use crate::voice::{UtteranceEvent, VoiceParticipant};
use crate::voice::voice_service::VoiceService;
use crate::voice::call_server::CallManager;
use crate::voice::audio_buffer::AudioBufferPool;
use crate::logging::TimingGuard;
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
    pub call_manager: Arc<CallManager>,
    pub audio_pool: Arc<AudioBufferPool>,
}

impl VoiceState {
    pub fn new(
        voice_service: Arc<VoiceService>,
        call_manager: Arc<CallManager>,
        audio_pool: Arc<AudioBufferPool>,
    ) -> Self {
        Self { voice_service, call_manager, audio_pool }
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
        match command {
            "voice/register-session" => {
                let _timer = TimingGuard::new("module", "voice_register_session");

                let session_id = params.get("session_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing session_id")?;
                let room_id = params.get("room_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing room_id")?;
                let participants: Vec<VoiceParticipant> = params.get("participants")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                match self.state.voice_service.register_session(session_id, room_id, participants) {
                    Ok(_) => Ok(CommandResult::Json(serde_json::json!({ "registered": true }))),
                    Err(e) => Err(e),
                }
            }

            "voice/on-utterance" => {
                let _timer = TimingGuard::new("module", "voice_on_utterance");

                let event: UtteranceEvent = params.get("event")
                    .ok_or("Missing event")
                    .and_then(|v| serde_json::from_value(v.clone()).map_err(|_| "Invalid event format"))?;

                match self.state.voice_service.on_utterance(event) {
                    Ok(responder_ids) => Ok(CommandResult::Json(serde_json::json!({
                        VOICE_RESPONSE_FIELD_RESPONDER_IDS: responder_ids.into_iter().map(|id| id.to_string()).collect::<Vec<String>>()
                    }))),
                    Err(e) => Err(e),
                }
            }

            "voice/should-route-tts" => {
                let _timer = TimingGuard::new("module", "voice_should_route_tts");

                let session_id = params.get("session_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing session_id")?;
                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;

                match self.state.voice_service.should_route_tts(session_id, persona_id) {
                    Ok(should_route) => Ok(CommandResult::Json(serde_json::json!({ "should_route": should_route }))),
                    Err(e) => Err(e),
                }
            }

            "voice/synthesize" => {
                let _timer = TimingGuard::new("module", "voice_synthesize");

                let text = params.get("text")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing text")?;
                let voice = params.get("voice")
                    .and_then(|v| v.as_str());
                let adapter = params.get("adapter")
                    .and_then(|v| v.as_str());

                use crate::voice::tts_service;

                let result = tts_service::synthesize_speech_sync(text, voice, adapter);

                match result {
                    Ok(synthesis) => {
                        // Raw PCM bytes — NO base64, NO JSON encoding of audio.
                        let pcm_bytes: Vec<u8> = synthesis.samples.iter()
                            .flat_map(|s| s.to_le_bytes())
                            .collect();

                        log_info!(
                            "module", "voice_synthesize",
                            "Synthesized {} samples at {}Hz ({:.1}s) → {} bytes raw PCM",
                            synthesis.samples.len(),
                            synthesis.sample_rate,
                            synthesis.duration_ms as f64 / 1000.0,
                            pcm_bytes.len()
                        );

                        // Return binary result with metadata
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
                    Err(e) => {
                        log_error!("module", "voice_synthesize", "TTS failed: {}", e);
                        Err(format!("TTS synthesis failed: {}", e))
                    }
                }
            }

            "voice/speak-in-call" => {
                let _timer = TimingGuard::new("module", "voice_speak_in_call");

                let call_id = params.get("call_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing call_id")?;
                let user_id = params.get("user_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing user_id")?;
                let text = params.get("text")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing text")?;
                let voice = params.get("voice")
                    .and_then(|v| v.as_str());
                let adapter = params.get("adapter")
                    .and_then(|v| v.as_str());

                // Direct injection: synthesize + inject into call mixer.
                let result = self.state.call_manager.speak_in_call(
                    call_id,
                    user_id,
                    text,
                    voice,
                    adapter,
                ).await;

                match result {
                    Ok((num_samples, duration_ms, sample_rate)) => {
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
                    Err(e) => {
                        log_error!("module", "voice_speak_in_call", "Speak-in-call failed: {}", e);
                        Err(format!("Speak-in-call failed: {}", e))
                    }
                }
            }

            "voice/synthesize-handle" => {
                let _timer = TimingGuard::new("module", "voice_synthesize_handle");

                let text = params.get("text")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing text")?;
                let voice = params.get("voice")
                    .and_then(|v| v.as_str());
                let adapter = params.get("adapter")
                    .and_then(|v| v.as_str());

                use crate::voice::tts_service;

                let result = tts_service::synthesize_speech_sync(text, voice, adapter);

                match result {
                    Ok(synthesis) => {
                        let adapter_name = adapter.unwrap_or("default");
                        let info = self.state.audio_pool.store(
                            synthesis.samples,
                            synthesis.sample_rate,
                            synthesis.duration_ms,
                            adapter_name,
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
                    Err(e) => {
                        log_error!("module", "voice_synthesize_handle", "TTS failed: {}", e);
                        Err(format!("TTS synthesis failed: {}", e))
                    }
                }
            }

            "voice/play-handle" => {
                let _timer = TimingGuard::new("module", "voice_play_handle");

                let handle = params.get("handle")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing handle")?;
                let call_id = params.get("call_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing call_id")?;
                let user_id = params.get("user_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing user_id")?;

                use crate::voice::handle::Handle as VoiceHandle;

                let voice_handle: VoiceHandle = handle.parse()
                    .map_err(|e| format!("Invalid handle UUID: {}", e))?;

                // Retrieve audio from buffer pool
                let samples = self.state.audio_pool.get(&voice_handle)
                    .ok_or_else(|| format!("Audio handle not found or expired: {}", &handle[..8.min(handle.len())]))?;

                let sample_count = samples.len();
                let duration_ms = (sample_count as u64 * 1000) / crate::audio_constants::AUDIO_SAMPLE_RATE as u64;

                // Inject into call mixer
                let result = self.state.call_manager.inject_audio(call_id, user_id, samples).await;

                match result {
                    Ok(()) => {
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
                    Err(e) => {
                        log_error!("module", "voice_play_handle", "Failed to inject audio: {}", e);
                        Err(format!("Failed to inject audio: {}", e))
                    }
                }
            }

            "voice/discard-handle" => {
                let handle = params.get("handle")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing handle")?;

                use crate::voice::handle::Handle as VoiceHandle;

                let voice_handle: VoiceHandle = handle.parse()
                    .map_err(|e| format!("Invalid handle UUID: {}", e))?;

                let discarded = self.state.audio_pool.discard(&voice_handle);

                Ok(CommandResult::Json(serde_json::json!({
                    "discarded": discarded,
                })))
            }

            "voice/transcribe" => {
                let _timer = TimingGuard::new("module", "voice_transcribe");

                let audio = params.get("audio")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing audio")?;
                let language = params.get("language")
                    .and_then(|v| v.as_str());

                use crate::voice::stt_service;
                use base64::Engine;

                // Decode base64 audio
                let bytes = base64::engine::general_purpose::STANDARD.decode(audio)
                    .map_err(|e| {
                        log_error!("module", "voice_transcribe", "Base64 decode failed: {}", e);
                        format!("Base64 decode failed: {}", e)
                    })?;

                // Convert bytes to i16 samples
                if bytes.len() % 2 != 0 {
                    return Err("Audio data must be even length (16-bit samples)".to_string());
                }

                let samples: Vec<i16> = bytes.chunks_exact(2)
                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();

                // Transcribe using STT service
                log_info!(
                    "module", "voice_transcribe",
                    "Transcribing {} samples ({:.1}s)",
                    samples.len(),
                    samples.len() as f64 / crate::audio_constants::AUDIO_SAMPLE_RATE as f64
                );

                let result = stt_service::transcribe_speech_sync(&samples, language);

                match result {
                    Ok(transcript) => {
                        log_info!(
                            "module", "voice_transcribe",
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
                    Err(e) => {
                        log_error!("module", "voice_transcribe", "STT failed: {}", e);
                        Err(format!("STT failed: {}", e))
                    }
                }
            }

            _ => Err(format!("Unknown voice command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}
