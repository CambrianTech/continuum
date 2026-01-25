/// Voice Service gRPC Implementation
///
/// Implements the VoiceService proto using the new adapter system.
/// Routes TTS/STT requests to appropriate adapters (Piper, Kokoro, Whisper).

use crate::proto::voice::voice_service_server::VoiceService;
use crate::proto::voice::*;
use crate::stt;
use crate::tts;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use tonic::{Request, Response, Status};
use tracing::{error, info};

pub struct VoiceServiceImpl;

impl VoiceServiceImpl {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl VoiceService for VoiceServiceImpl {
    type SynthesizeStreamStream =
        Pin<Box<dyn Stream<Item = Result<AudioChunk, Status>> + Send + 'static>>;

    async fn ping(&self, _request: Request<PingRequest>) -> Result<Response<PingResponse>, Status> {
        let tts_registry = tts::get_registry();
        let stt_registry = stt::get_registry();

        let adapter_count = tts_registry.read().list().len() + stt_registry.read().list().len();

        Ok(Response::new(PingResponse {
            message: "pong".to_string(),
            adapter_count: adapter_count as i32,
        }))
    }

    async fn synthesize(
        &self,
        request: Request<SynthesizeRequest>,
    ) -> Result<Response<SynthesizeResponse>, Status> {
        let req = request.into_inner();
        info!("TTS request: adapter={}, text_len={}", req.adapter, req.text.len());

        // Use adapter from request or default to active adapter
        let adapter_name = if req.adapter.is_empty() {
            "piper"
        } else {
            req.adapter.as_str()
        };

        // Get the adapter from the registry
        let registry = tts::get_registry();
        let adapter = registry
            .read()
            .get(adapter_name)
            .ok_or_else(|| {
                error!("TTS adapter '{}' not found", adapter_name);
                Status::not_found(format!("TTS adapter '{}' not found", adapter_name))
            })?;

        // Get voice ID (default if empty)
        let voice = if req.voice.is_empty() {
            "default"
        } else {
            &req.voice
        };

        // Synthesize audio
        let result = adapter
            .synthesize(&req.text, voice)
            .await
            .map_err(|e| {
                error!("TTS synthesis failed: {}", e);
                Status::internal(format!("TTS synthesis failed: {}", e))
            })?;

        // Convert i16 samples to bytes (little-endian PCM)
        let mut audio_bytes = Vec::with_capacity(result.samples.len() * 2);
        for sample in &result.samples {
            audio_bytes.extend_from_slice(&sample.to_le_bytes());
        }

        info!(
            "TTS success: {} samples, {}ms, sample_rate={}",
            result.samples.len(),
            result.duration_ms,
            result.sample_rate
        );

        Ok(Response::new(SynthesizeResponse {
            audio: audio_bytes,  // Send raw PCM bytes (protobuf bytes type)
            sample_rate: result.sample_rate as i32,
            duration_ms: result.duration_ms as i32,
            adapter: adapter_name.to_string(),
        }))
    }

    async fn synthesize_stream(
        &self,
        _request: Request<SynthesizeRequest>,
    ) -> Result<Response<Self::SynthesizeStreamStream>, Status> {
        // Streaming TTS not yet implemented
        Err(Status::unimplemented("Streaming TTS not yet implemented"))
    }

    async fn transcribe(
        &self,
        request: Request<TranscribeRequest>,
    ) -> Result<Response<TranscribeResponse>, Status> {
        let req = request.into_inner();
        info!("STT request: audio_len={} bytes", req.audio.len());

        // req.audio is already raw PCM bytes (protobuf bytes type)
        let audio_bytes = &req.audio;

        // Convert bytes to f32 samples (16-bit PCM little-endian)
        let mut samples = Vec::with_capacity(audio_bytes.len() / 2);
        for chunk in audio_bytes.chunks_exact(2) {
            let sample_i16 = i16::from_le_bytes([chunk[0], chunk[1]]);
            let sample_f32 = sample_i16 as f32 / 32768.0; // Normalize to [-1.0, 1.0]
            samples.push(sample_f32);
        }

        // Get the STT adapter (default to whisper if available)
        let registry = stt::get_registry();
        let adapter = registry
            .read()
            .get_active()
            .ok_or_else(|| {
                error!("No STT adapter available");
                Status::unavailable("No STT adapter available")
            })?;

        // Get language hint
        let language = if req.language.is_empty() {
            None
        } else {
            Some(req.language.as_str())
        };

        // Transcribe audio
        let result = adapter
            .transcribe(samples, language)
            .await
            .map_err(|e| {
                error!("STT transcription failed: {}", e);
                Status::internal(format!("STT transcription failed: {}", e))
            })?;

        info!(
            "STT success: text='{}', confidence={}",
            result.text, result.confidence
        );

        // Convert segments
        let segments = result
            .segments
            .iter()
            .map(|s| Segment {
                word: s.text.clone(),
                start: s.start_ms as f32 / 1000.0,
                end: s.end_ms as f32 / 1000.0,
                confidence: result.confidence, // Use overall confidence for each segment
            })
            .collect();

        Ok(Response::new(TranscribeResponse {
            text: result.text,
            language: result.language,
            confidence: result.confidence,
            segments,
        }))
    }

    async fn list_adapters(
        &self,
        _request: Request<ListAdaptersRequest>,
    ) -> Result<Response<ListAdaptersResponse>, Status> {
        let tts_registry = tts::get_registry();
        let stt_registry = stt::get_registry();

        let mut adapters = Vec::new();

        for (name, is_init) in tts_registry.read().list() {
            adapters.push(AdapterInfo {
                name: format!("tts/{}", name),
                loaded: is_init,
                voice_count: 0, // TODO: Get voice count from adapter
                memory_bytes: 0, // TODO: Track memory usage
            });
        }

        for (name, is_init) in stt_registry.read().list() {
            adapters.push(AdapterInfo {
                name: format!("stt/{}", name),
                loaded: is_init,
                voice_count: 0,
                memory_bytes: 0,
            });
        }

        Ok(Response::new(ListAdaptersResponse { adapters }))
    }

    async fn load_adapter(
        &self,
        _request: Request<LoadAdapterRequest>,
    ) -> Result<Response<LoadAdapterResponse>, Status> {
        // Adapters are loaded at startup for now
        // TODO: Implement dynamic loading
        Err(Status::unimplemented(
            "Dynamic adapter loading not yet implemented",
        ))
    }

    async fn unload_adapter(
        &self,
        _request: Request<UnloadAdapterRequest>,
    ) -> Result<Response<UnloadAdapterResponse>, Status> {
        // Adapters can't be unloaded yet
        // TODO: Implement dynamic unloading for memory management
        Err(Status::unimplemented(
            "Dynamic adapter unloading not yet implemented",
        ))
    }
}
