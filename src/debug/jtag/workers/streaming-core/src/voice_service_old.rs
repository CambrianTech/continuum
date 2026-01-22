//! Voice gRPC Service Implementation
//!
//! Implements the VoiceService from voice.proto:
//! - TTS synthesis (Kokoro, Fish-Speech, F5-TTS, StyleTTS2, XTTS-v2)
//! - STT transcription (Whisper)
//!
//! This is the gRPC endpoint that TypeScript VoiceGrpcClient connects to.

use crate::stt;
use crate::tts;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::{error, info};

// Generated protobuf code (from build.rs -> src/proto/)
pub mod voice_proto {
    include!("proto/voice.rs");
}

use voice_proto::voice_service_server::{VoiceService, VoiceServiceServer};
use voice_proto::{
    AdapterInfo, AudioChunk, ListAdaptersRequest, ListAdaptersResponse, LoadAdapterRequest,
    LoadAdapterResponse, PingRequest, PingResponse, SynthesizeRequest, SynthesizeResponse,
    TranscribeRequest, TranscribeResponse, UnloadAdapterRequest, UnloadAdapterResponse,
};

/// Voice service state
pub struct VoiceServiceImpl {
    // TODO: Update to new TTS adapter system
    // tts_registry: Arc<RwLock<tts::TTSRegistry>>,
}

impl VoiceServiceImpl {
    pub fn new() -> Self {
        Self {
            // TODO: Update to new TTS adapter system
            // tts_registry: tts::get_registry(),
        }
    }

    /// Create the gRPC server for this service
    pub fn into_server(self) -> VoiceServiceServer<Self> {
        VoiceServiceServer::new(self)
    }
}

impl Default for VoiceServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[tonic::async_trait]
impl VoiceService for VoiceServiceImpl {
    /// Health check
    async fn ping(&self, _request: Request<PingRequest>) -> Result<Response<PingResponse>, Status> {
        info!("Voice service ping");

        let registry = self.tts_registry.read().await;
        let adapter_count = registry.names().len() as i32;

        Ok(Response::new(PingResponse {
            message: "pong".to_string(),
            adapter_count,
        }))
    }

    /// Synthesize text to speech (batch mode - returns complete audio)
    async fn synthesize(
        &self,
        request: Request<SynthesizeRequest>,
    ) -> Result<Response<SynthesizeResponse>, Status> {
        let req = request.into_inner();
        info!("TTS synthesize: {} chars, adapter={}", req.text.len(), req.adapter);

        if req.text.is_empty() {
            return Err(Status::invalid_argument("Text cannot be empty"));
        }

        let adapter_name = if req.adapter.is_empty() {
            "kokoro".to_string()
        } else {
            req.adapter.clone()
        };

        // Get adapter from registry
        let registry = self.tts_registry.read().await;
        let adapter_arc = registry
            .get(&adapter_name)
            .ok_or_else(|| Status::not_found(format!("Adapter '{}' not found", adapter_name)))?;

        // Lock the adapter for use
        let mut adapter = adapter_arc.write().await;

        // Ensure adapter is loaded
        adapter
            .load()
            .await
            .map_err(|e| Status::internal(format!("Failed to load adapter: {}", e)))?;

        // Build params
        let sample_rate = if req.sample_rate <= 0 {
            24000u32
        } else {
            req.sample_rate as u32
        };

        let params = TTSParams {
            speaker_id: if req.voice.is_empty() {
                None
            } else {
                Some(req.voice)
            },
            speed: if req.speed <= 0.0 { 1.0 } else { req.speed },
            pitch: 0.0,
            sample_rate,
            reference_audio: None,
            emotion: None,
        };

        // Synthesize
        let samples = adapter
            .synthesize(&req.text, &params)
            .await
            .map_err(|e| Status::internal(format!("TTS synthesis failed: {}", e)))?;

        // Convert i16 samples to bytes (little-endian PCM16)
        let audio_bytes: Vec<u8> = samples
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();

        let duration_ms = (samples.len() as f32 / sample_rate as f32 * 1000.0) as i32;

        info!(
            "TTS synthesize complete: {} samples, {}ms",
            samples.len(),
            duration_ms
        );

        Ok(Response::new(SynthesizeResponse {
            audio: audio_bytes,
            sample_rate: sample_rate as i32,
            duration_ms,
            adapter: adapter_name,
        }))
    }

    /// Synthesize text to speech (streaming mode - returns chunks)
    type SynthesizeStreamStream =
        Pin<Box<dyn Stream<Item = Result<AudioChunk, Status>> + Send + 'static>>;

    async fn synthesize_stream(
        &self,
        request: Request<SynthesizeRequest>,
    ) -> Result<Response<Self::SynthesizeStreamStream>, Status> {
        let req = request.into_inner();
        info!(
            "TTS synthesize stream: {} chars, adapter={}",
            req.text.len(),
            req.adapter
        );

        if req.text.is_empty() {
            return Err(Status::invalid_argument("Text cannot be empty"));
        }

        let adapter_name = if req.adapter.is_empty() {
            "kokoro".to_string()
        } else {
            req.adapter.clone()
        };

        let registry = self.tts_registry.clone();
        let text = req.text;
        let speaker_id = if req.voice.is_empty() {
            None
        } else {
            Some(req.voice)
        };
        let speed = if req.speed <= 0.0 { 1.0 } else { req.speed };
        let sample_rate = if req.sample_rate <= 0 {
            24000u32
        } else {
            req.sample_rate as u32
        };

        // Create stream that yields audio chunks
        let stream = async_stream::try_stream! {
            let registry = registry.read().await;
            let adapter_arc = registry
                .get(&adapter_name)
                .ok_or_else(|| Status::not_found(format!("Adapter '{}' not found", adapter_name)))?;

            let mut adapter = adapter_arc.write().await;

            adapter
                .load()
                .await
                .map_err(|e| Status::internal(format!("Failed to load adapter: {}", e)))?;

            let params = TTSParams {
                speaker_id,
                speed,
                pitch: 0.0,
                sample_rate,
                reference_audio: None,
                emotion: None,
            };

            // Use streaming synthesis
            let mut audio_stream = adapter
                .synthesize_stream(&text, &params)
                .await
                .map_err(|e| Status::internal(format!("TTS stream failed: {}", e)))?;

            let mut chunk_index = 0i32;
            while let Some(result) = audio_stream.recv().await {
                match result {
                    Ok(chunk) => {
                        let audio_bytes: Vec<u8> = chunk.samples
                            .iter()
                            .flat_map(|s| s.to_le_bytes())
                            .collect();

                        yield AudioChunk {
                            audio: audio_bytes,
                            is_last: chunk.is_final,
                            chunk_index,
                        };
                        chunk_index += 1;

                        if chunk.is_final {
                            break;
                        }
                    }
                    Err(e) => {
                        error!("TTS stream error: {}", e);
                        break;
                    }
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    /// Transcribe audio to text (Whisper STT)
    /// Runs on thread pool to avoid blocking async runtime
    async fn transcribe(
        &self,
        request: Request<TranscribeRequest>,
    ) -> Result<Response<TranscribeResponse>, Status> {
        let req = request.into_inner();
        info!(
            "STT transcribe: {} bytes, language={}, model={}",
            req.audio.len(),
            req.language,
            req.model
        );

        if req.audio.is_empty() {
            return Err(Status::invalid_argument("Audio cannot be empty"));
        }

        // Check if STT is initialized
        if !stt::is_initialized() {
            error!("STT adapter not initialized - model may not be loaded");
            return Err(Status::unavailable(
                "STT model not loaded. Place ggml-base.en.bin in models/whisper/"
            ));
        }

        // Decode audio from base64 (TypeScript sends base64)
        let audio_bytes = if req.audio.iter().all(|&b| b.is_ascii()) {
            // Looks like base64
            use base64::{Engine, engine::general_purpose::STANDARD};
            STANDARD.decode(&req.audio).map_err(|e| {
                Status::invalid_argument(format!("Invalid base64 audio: {}", e))
            })?
        } else {
            req.audio
        };

        // Convert bytes to i16 samples (little-endian PCM16)
        let i16_samples: Vec<i16> = audio_bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        // Convert to f32 for Whisper
        let f32_samples = stt::i16_to_f32(&i16_samples);

        // Resample to 16kHz if needed (Whisper native rate)
        let sample_rate = if req.sample_rate > 0 { req.sample_rate as u32 } else { 16000 };
        let samples = if sample_rate != 16000 {
            stt::resample_to_16k(&f32_samples, sample_rate)
        } else {
            f32_samples
        };

        // Run transcription (off main thread via spawn_blocking)
        let language = if req.language.is_empty() || req.language == "auto" {
            None
        } else {
            Some(req.language.as_str())
        };

        let result = stt::transcribe(samples, language).await.map_err(|e| {
            error!("Whisper transcription failed: {}", e);
            Status::internal(format!("Transcription failed: {}", e))
        })?;

        info!("STT result: '{}' ({})", result.text, result.language);

        // Convert segments
        let segments: Vec<voice_proto::Segment> = result
            .segments
            .iter()
            .map(|s| voice_proto::Segment {
                word: s.text.clone(),
                start: s.start_ms as f32 / 1000.0,
                end: s.end_ms as f32 / 1000.0,
                confidence: result.confidence,
            })
            .collect();

        Ok(Response::new(TranscribeResponse {
            text: result.text,
            language: result.language,
            confidence: result.confidence,
            segments,
        }))
    }

    /// List available TTS adapters
    async fn list_adapters(
        &self,
        _request: Request<ListAdaptersRequest>,
    ) -> Result<Response<ListAdaptersResponse>, Status> {
        let registry = self.tts_registry.read().await;
        let adapter_names = registry.names();

        let adapters: Vec<AdapterInfo> = adapter_names
            .iter()
            .map(|name| AdapterInfo {
                name: name.clone(),
                loaded: true, // TODO: Track loaded state properly
                voice_count: 1, // TODO: Get actual voice count
                memory_bytes: 0, // TODO: Track memory usage
            })
            .collect();

        Ok(Response::new(ListAdaptersResponse { adapters }))
    }

    /// Load a specific adapter into memory
    async fn load_adapter(
        &self,
        request: Request<LoadAdapterRequest>,
    ) -> Result<Response<LoadAdapterResponse>, Status> {
        let req = request.into_inner();
        info!("Loading adapter: {}", req.adapter);

        let start = std::time::Instant::now();

        let registry = self.tts_registry.read().await;
        let adapter_arc = registry
            .get(&req.adapter)
            .ok_or_else(|| Status::not_found(format!("Adapter '{}' not found", req.adapter)))?;

        let mut adapter = adapter_arc.write().await;

        match adapter.load().await {
            Ok(_) => {
                let load_time_ms = start.elapsed().as_millis() as i32;
                info!("Adapter {} loaded in {}ms", req.adapter, load_time_ms);
                Ok(Response::new(LoadAdapterResponse {
                    success: true,
                    error: String::new(),
                    load_time_ms,
                }))
            }
            Err(e) => {
                error!("Failed to load adapter {}: {}", req.adapter, e);
                Ok(Response::new(LoadAdapterResponse {
                    success: false,
                    error: e.to_string(),
                    load_time_ms: 0,
                }))
            }
        }
    }

    /// Unload an adapter to free memory
    async fn unload_adapter(
        &self,
        request: Request<UnloadAdapterRequest>,
    ) -> Result<Response<UnloadAdapterResponse>, Status> {
        let req = request.into_inner();
        info!("Unloading adapter: {}", req.adapter);

        // TODO: Implement proper unloading
        // For now, we don't actually unload - adapters stay in memory

        Ok(Response::new(UnloadAdapterResponse {
            success: true,
            error: String::new(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_voice_service_ping() {
        let service = VoiceServiceImpl::new();
        let response = service.ping(Request::new(PingRequest {})).await.unwrap();
        assert_eq!(response.into_inner().message, "pong");
    }

    #[tokio::test]
    async fn test_voice_service_list_adapters() {
        let service = VoiceServiceImpl::new();
        let response = service
            .list_adapters(Request::new(ListAdaptersRequest {}))
            .await
            .unwrap();

        let adapters = response.into_inner().adapters;
        // Registry should have default adapters registered
        assert!(!adapters.is_empty());
    }
}
