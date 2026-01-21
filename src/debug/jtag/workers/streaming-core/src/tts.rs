//! TTS (Text-to-Speech) Adapter System
//!
//! Polymorphic adapter pattern for multiple TTS backends.
//! Supports streaming audio output for real-time voice synthesis.
//!
//! # Philosophy: Pure Rust, No Python
//!
//! All TTS adapters use native Rust inference via `candle` or similar.
//! No Python bridges, no subprocess calls, no FFI to Python.
//! This gives us:
//! - Zero-copy audio buffers
//! - Predictable latency (no GIL)
//! - Single binary deployment
//! - True streaming (token-by-token synthesis)
//!
//! # Supported Backends (by quality ranking from TTS Arena)
//!
//! | Rank | Model        | Win Rate | Notes                          |
//! |------|--------------|----------|--------------------------------|
//! | #1   | Kokoro v1.0  | 80.9%    | Primary - best naturalness     |
//! | #2   | Kokoro v0.19 | 75.8%    | Stable fallback                |
//! | #4   | XTTS-v2      | 61.6%    | Voice cloning capable          |
//! | #7   | Fish Speech  | 51.0%    | Good multilingual support      |
//! |      | F5-TTS       | -        | Fast, natural conversational   |
//! |      | StyleTTS2    | -        | Research-grade prosody         |
//!
//! # Usage
//!
//! ```rust,ignore
//! use streaming_core::tts::{TTSAdapterRegistry, KokoroAdapter};
//!
//! // Create registry and register adapters
//! let mut registry = TTSAdapterRegistry::new();
//! registry.register(Box::new(KokoroAdapter::new()));
//! registry.register(Box::new(FishSpeechAdapter::new()));
//!
//! // Get adapter by name
//! let tts = registry.get("kokoro").unwrap();
//!
//! // Synthesize with streaming
//! let mut stream = tts.synthesize_stream("Hello, world!").await?;
//! while let Some(chunk) = stream.next().await {
//!     // Process audio chunk (20ms frames)
//! }
//! ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;

/// Audio chunk from TTS synthesis
#[derive(Debug, Clone)]
pub struct TTSAudioChunk {
    /// PCM samples (mono, i16)
    pub samples: Vec<i16>,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Is this the final chunk?
    pub is_final: bool,
    /// Timestamp in microseconds (relative to synthesis start)
    pub timestamp_us: u64,
}

impl TTSAudioChunk {
    pub fn new(samples: Vec<i16>, sample_rate: u32, timestamp_us: u64, is_final: bool) -> Self {
        Self {
            samples,
            sample_rate,
            is_final,
            timestamp_us,
        }
    }

    /// Duration of this chunk in milliseconds
    pub fn duration_ms(&self) -> f32 {
        (self.samples.len() as f32 / self.sample_rate as f32) * 1000.0
    }
}

/// TTS synthesis parameters
#[derive(Debug, Clone)]
pub struct TTSParams {
    /// Speaker/voice ID (model-specific)
    pub speaker_id: Option<String>,
    /// Speech speed multiplier (1.0 = normal)
    pub speed: f32,
    /// Pitch adjustment (-1.0 to 1.0)
    pub pitch: f32,
    /// Output sample rate (default: 24000)
    pub sample_rate: u32,
    /// Reference audio for voice cloning (optional)
    pub reference_audio: Option<Vec<i16>>,
    /// Emotion/style tag (model-specific, e.g., "happy", "sad")
    pub emotion: Option<String>,
}

impl Default for TTSParams {
    fn default() -> Self {
        Self {
            speaker_id: None,
            speed: 1.0,
            pitch: 0.0,
            sample_rate: 24000,
            reference_audio: None,
            emotion: None,
        }
    }
}

/// TTS adapter errors
#[derive(Error, Debug)]
pub enum TTSError {
    #[error("Model not loaded: {0}")]
    ModelNotLoaded(String),

    #[error("Synthesis failed: {0}")]
    SynthesisFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Voice cloning not supported")]
    VoiceCloningNotSupported,

    #[error("Speaker not found: {0}")]
    SpeakerNotFound(String),

    #[error("Stream cancelled")]
    StreamCancelled,

    #[error("Backend error: {0}")]
    BackendError(String),
}

/// Streaming audio output
pub type TTSAudioStream = mpsc::Receiver<Result<TTSAudioChunk, TTSError>>;

/// TTS Adapter trait - implement for each backend
#[async_trait]
pub trait TTSAdapter: Send + Sync {
    /// Unique adapter name (e.g., "kokoro", "fish-speech")
    fn name(&self) -> &'static str;

    /// Human-readable description
    fn description(&self) -> &'static str;

    /// Does this adapter support voice cloning?
    fn supports_voice_cloning(&self) -> bool {
        false
    }

    /// Available speaker/voice IDs
    fn available_speakers(&self) -> Vec<String> {
        vec![]
    }

    /// Default sample rate for this model
    fn default_sample_rate(&self) -> u32 {
        24000
    }

    /// Load the model (call before synthesis)
    async fn load(&mut self) -> Result<(), TTSError>;

    /// Unload model to free memory
    async fn unload(&mut self) -> Result<(), TTSError>;

    /// Check if model is loaded
    fn is_loaded(&self) -> bool;

    /// Synthesize text to audio (blocking, returns all audio)
    async fn synthesize(&self, text: &str, params: &TTSParams) -> Result<Vec<i16>, TTSError>;

    /// Synthesize text to streaming audio (returns chunks as they're generated)
    async fn synthesize_stream(
        &self,
        text: &str,
        params: &TTSParams,
    ) -> Result<TTSAudioStream, TTSError>;

    /// Get/set parameter (OpenCV-style runtime configuration)
    fn get_param(&self, _name: &str) -> Option<String> {
        None
    }

    fn set_param(&mut self, _name: &str, _value: &str) -> Result<(), TTSError> {
        Ok(())
    }
}

// ============================================================================
// TTS ADAPTER IMPLEMENTATIONS (Stubs - implement with real backends)
// ============================================================================

/// Kokoro TTS Adapter - #1 on TTS Arena (80.9% win rate)
///
/// Lightweight, fast, extremely natural sounding.
/// https://huggingface.co/hexgrad/Kokoro-82M
///
/// # Implementation Strategy (Pure Rust)
///
/// Kokoro uses a StyleTTS2-based architecture. For Rust:
/// - Use `candle` for tensor operations and model inference
/// - Load ONNX export or convert weights to safetensors
/// - Vocoder (istftnet) runs natively in candle
///
/// No Python. No bridges. Pure Rust inference.
pub struct KokoroAdapter {
    model_path: Option<String>,
    loaded: bool,
    sample_rate: u32,
}

impl KokoroAdapter {
    pub fn new() -> Self {
        Self {
            model_path: None,
            loaded: false,
            sample_rate: 24000,
        }
    }

    pub fn with_model_path(mut self, path: String) -> Self {
        self.model_path = Some(path);
        self
    }
}

impl Default for KokoroAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TTSAdapter for KokoroAdapter {
    fn name(&self) -> &'static str {
        "kokoro"
    }

    fn description(&self) -> &'static str {
        "Kokoro v1.0 - #1 TTS Arena (80.9% win rate). Lightweight, fast, natural."
    }

    fn default_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn available_speakers(&self) -> Vec<String> {
        // Kokoro has multiple built-in voices
        vec![
            "af".to_string(),      // American Female
            "af_bella".to_string(),
            "af_nicole".to_string(),
            "af_sarah".to_string(),
            "af_sky".to_string(),
            "am_adam".to_string(), // American Male
            "am_michael".to_string(),
            "bf_emma".to_string(), // British Female
            "bf_isabella".to_string(),
            "bm_george".to_string(), // British Male
            "bm_lewis".to_string(),
        ]
    }

    async fn load(&mut self) -> Result<(), TTSError> {
        // TODO: Load Kokoro model via Python bridge or native implementation
        // Model: https://huggingface.co/hexgrad/Kokoro-82M
        self.loaded = true;
        Ok(())
    }

    async fn unload(&mut self) -> Result<(), TTSError> {
        self.loaded = false;
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    async fn synthesize(&self, text: &str, params: &TTSParams) -> Result<Vec<i16>, TTSError> {
        if !self.loaded {
            return Err(TTSError::ModelNotLoaded("kokoro".to_string()));
        }

        // TODO: Actual Kokoro inference
        // For now, return silence proportional to text length
        let duration_ms = text.len() as f32 * 60.0; // ~60ms per character
        let sample_rate = params.sample_rate;
        let num_samples = ((duration_ms / 1000.0) * sample_rate as f32) as usize;

        Ok(vec![0i16; num_samples])
    }

    async fn synthesize_stream(
        &self,
        text: &str,
        params: &TTSParams,
    ) -> Result<TTSAudioStream, TTSError> {
        if !self.loaded {
            return Err(TTSError::ModelNotLoaded("kokoro".to_string()));
        }

        let (tx, rx) = mpsc::channel(32);
        let text = text.to_string();
        let sample_rate = params.sample_rate;

        // Spawn streaming synthesis task
        tokio::spawn(async move {
            // TODO: Real streaming synthesis
            // For now, simulate by chunking silence
            let chunk_duration_ms = 20.0;
            let samples_per_chunk = ((chunk_duration_ms / 1000.0) * sample_rate as f32) as usize;
            let total_duration_ms = text.len() as f32 * 60.0;
            let num_chunks = (total_duration_ms / chunk_duration_ms).ceil() as usize;

            for i in 0..num_chunks {
                let is_final = i == num_chunks - 1;
                let timestamp_us = (i as f32 * chunk_duration_ms * 1000.0) as u64;

                let chunk = TTSAudioChunk::new(
                    vec![0i16; samples_per_chunk],
                    sample_rate,
                    timestamp_us,
                    is_final,
                );

                if tx.send(Ok(chunk)).await.is_err() {
                    break; // Receiver dropped
                }

                // Simulate real-time generation
                tokio::time::sleep(tokio::time::Duration::from_millis(
                    chunk_duration_ms as u64 / 2,
                ))
                .await;
            }
        });

        Ok(rx)
    }
}

/// Fish Speech Adapter - High quality multilingual TTS
///
/// https://github.com/fishaudio/fish-speech
///
/// # Implementation Strategy (Pure Rust)
///
/// Fish Speech uses VQGAN + transformer. For Rust:
/// - VQGAN encoder/decoder via candle
/// - Transformer via candle-transformers
/// - Load from safetensors checkpoint
pub struct FishSpeechAdapter {
    model_path: Option<String>,
    loaded: bool,
    sample_rate: u32,
}

impl FishSpeechAdapter {
    pub fn new() -> Self {
        Self {
            model_path: None,
            loaded: false,
            sample_rate: 44100, // Fish Speech outputs 44.1kHz
        }
    }

    pub fn with_model_path(mut self, path: String) -> Self {
        self.model_path = Some(path);
        self
    }
}

impl Default for FishSpeechAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TTSAdapter for FishSpeechAdapter {
    fn name(&self) -> &'static str {
        "fish-speech"
    }

    fn description(&self) -> &'static str {
        "Fish Speech V1.5 - High quality multilingual TTS with voice cloning."
    }

    fn supports_voice_cloning(&self) -> bool {
        true
    }

    fn default_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    async fn load(&mut self) -> Result<(), TTSError> {
        // TODO: Load Fish Speech model
        self.loaded = true;
        Ok(())
    }

    async fn unload(&mut self) -> Result<(), TTSError> {
        self.loaded = false;
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    async fn synthesize(&self, text: &str, params: &TTSParams) -> Result<Vec<i16>, TTSError> {
        if !self.loaded {
            return Err(TTSError::ModelNotLoaded("fish-speech".to_string()));
        }

        let duration_ms = text.len() as f32 * 60.0;
        let sample_rate = params.sample_rate;
        let num_samples = ((duration_ms / 1000.0) * sample_rate as f32) as usize;

        Ok(vec![0i16; num_samples])
    }

    async fn synthesize_stream(
        &self,
        text: &str,
        params: &TTSParams,
    ) -> Result<TTSAudioStream, TTSError> {
        if !self.loaded {
            return Err(TTSError::ModelNotLoaded("fish-speech".to_string()));
        }

        let (tx, rx) = mpsc::channel(32);
        let text = text.to_string();
        let sample_rate = params.sample_rate;

        tokio::spawn(async move {
            let chunk_duration_ms = 20.0;
            let samples_per_chunk = ((chunk_duration_ms / 1000.0) * sample_rate as f32) as usize;
            let total_duration_ms = text.len() as f32 * 60.0;
            let num_chunks = (total_duration_ms / chunk_duration_ms).ceil() as usize;

            for i in 0..num_chunks {
                let is_final = i == num_chunks - 1;
                let timestamp_us = (i as f32 * chunk_duration_ms * 1000.0) as u64;

                let chunk = TTSAudioChunk::new(
                    vec![0i16; samples_per_chunk],
                    sample_rate,
                    timestamp_us,
                    is_final,
                );

                if tx.send(Ok(chunk)).await.is_err() {
                    break;
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(
                    chunk_duration_ms as u64 / 2,
                ))
                .await;
            }
        });

        Ok(rx)
    }
}

/// F5-TTS Adapter - Fast, natural conversational TTS
///
/// https://github.com/SWivid/F5-TTS
pub struct F5TTSAdapter {
    model_path: Option<String>,
    loaded: bool,
    sample_rate: u32,
}

impl F5TTSAdapter {
    pub fn new() -> Self {
        Self {
            model_path: None,
            loaded: false,
            sample_rate: 24000,
        }
    }

    pub fn with_model_path(mut self, path: String) -> Self {
        self.model_path = Some(path);
        self
    }
}

impl Default for F5TTSAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TTSAdapter for F5TTSAdapter {
    fn name(&self) -> &'static str {
        "f5-tts"
    }

    fn description(&self) -> &'static str {
        "F5-TTS - Fast, natural conversational speech synthesis."
    }

    fn supports_voice_cloning(&self) -> bool {
        true
    }

    fn default_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    async fn load(&mut self) -> Result<(), TTSError> {
        self.loaded = true;
        Ok(())
    }

    async fn unload(&mut self) -> Result<(), TTSError> {
        self.loaded = false;
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    async fn synthesize(&self, text: &str, params: &TTSParams) -> Result<Vec<i16>, TTSError> {
        if !self.loaded {
            return Err(TTSError::ModelNotLoaded("f5-tts".to_string()));
        }

        let duration_ms = text.len() as f32 * 60.0;
        let sample_rate = params.sample_rate;
        let num_samples = ((duration_ms / 1000.0) * sample_rate as f32) as usize;

        Ok(vec![0i16; num_samples])
    }

    async fn synthesize_stream(
        &self,
        text: &str,
        params: &TTSParams,
    ) -> Result<TTSAudioStream, TTSError> {
        if !self.loaded {
            return Err(TTSError::ModelNotLoaded("f5-tts".to_string()));
        }

        let (tx, rx) = mpsc::channel(32);
        let text = text.to_string();
        let sample_rate = params.sample_rate;

        tokio::spawn(async move {
            let chunk_duration_ms = 20.0;
            let samples_per_chunk = ((chunk_duration_ms / 1000.0) * sample_rate as f32) as usize;
            let total_duration_ms = text.len() as f32 * 60.0;
            let num_chunks = (total_duration_ms / chunk_duration_ms).ceil() as usize;

            for i in 0..num_chunks {
                let is_final = i == num_chunks - 1;
                let timestamp_us = (i as f32 * chunk_duration_ms * 1000.0) as u64;

                let chunk = TTSAudioChunk::new(
                    vec![0i16; samples_per_chunk],
                    sample_rate,
                    timestamp_us,
                    is_final,
                );

                if tx.send(Ok(chunk)).await.is_err() {
                    break;
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(
                    chunk_duration_ms as u64 / 2,
                ))
                .await;
            }
        });

        Ok(rx)
    }
}

/// StyleTTS2 Adapter - Research-grade prosody
///
/// https://github.com/yl4579/StyleTTS2
pub struct StyleTTS2Adapter {
    #[allow(dead_code)]
    model_path: Option<String>,
    loaded: bool,
    sample_rate: u32,
}

impl StyleTTS2Adapter {
    pub fn new() -> Self {
        Self {
            model_path: None,
            loaded: false,
            sample_rate: 24000,
        }
    }
}

impl Default for StyleTTS2Adapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TTSAdapter for StyleTTS2Adapter {
    fn name(&self) -> &'static str {
        "styletts2"
    }

    fn description(&self) -> &'static str {
        "StyleTTS2 - Research-grade prosody and style control."
    }

    fn supports_voice_cloning(&self) -> bool {
        true
    }

    fn default_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    async fn load(&mut self) -> Result<(), TTSError> {
        self.loaded = true;
        Ok(())
    }

    async fn unload(&mut self) -> Result<(), TTSError> {
        self.loaded = false;
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    async fn synthesize(&self, text: &str, params: &TTSParams) -> Result<Vec<i16>, TTSError> {
        if !self.loaded {
            return Err(TTSError::ModelNotLoaded("styletts2".to_string()));
        }

        let duration_ms = text.len() as f32 * 60.0;
        let sample_rate = params.sample_rate;
        let num_samples = ((duration_ms / 1000.0) * sample_rate as f32) as usize;

        Ok(vec![0i16; num_samples])
    }

    async fn synthesize_stream(
        &self,
        text: &str,
        params: &TTSParams,
    ) -> Result<TTSAudioStream, TTSError> {
        if !self.loaded {
            return Err(TTSError::ModelNotLoaded("styletts2".to_string()));
        }

        let (tx, rx) = mpsc::channel(32);
        let text = text.to_string();
        let sample_rate = params.sample_rate;

        tokio::spawn(async move {
            let chunk_duration_ms = 20.0;
            let samples_per_chunk = ((chunk_duration_ms / 1000.0) * sample_rate as f32) as usize;
            let total_duration_ms = text.len() as f32 * 60.0;
            let num_chunks = (total_duration_ms / chunk_duration_ms).ceil() as usize;

            for i in 0..num_chunks {
                let is_final = i == num_chunks - 1;
                let timestamp_us = (i as f32 * chunk_duration_ms * 1000.0) as u64;

                let chunk = TTSAudioChunk::new(
                    vec![0i16; samples_per_chunk],
                    sample_rate,
                    timestamp_us,
                    is_final,
                );

                if tx.send(Ok(chunk)).await.is_err() {
                    break;
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(
                    chunk_duration_ms as u64 / 2,
                ))
                .await;
            }
        });

        Ok(rx)
    }
}

/// XTTS-v2 Adapter - Voice cloning specialist
///
/// https://huggingface.co/coqui/XTTS-v2
pub struct XTTSv2Adapter {
    #[allow(dead_code)]
    model_path: Option<String>,
    loaded: bool,
    sample_rate: u32,
}

impl XTTSv2Adapter {
    pub fn new() -> Self {
        Self {
            model_path: None,
            loaded: false,
            sample_rate: 24000,
        }
    }
}

impl Default for XTTSv2Adapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TTSAdapter for XTTSv2Adapter {
    fn name(&self) -> &'static str {
        "xtts-v2"
    }

    fn description(&self) -> &'static str {
        "XTTS-v2 - Multilingual voice cloning with 6-second samples."
    }

    fn supports_voice_cloning(&self) -> bool {
        true
    }

    fn default_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    async fn load(&mut self) -> Result<(), TTSError> {
        self.loaded = true;
        Ok(())
    }

    async fn unload(&mut self) -> Result<(), TTSError> {
        self.loaded = false;
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    async fn synthesize(&self, text: &str, params: &TTSParams) -> Result<Vec<i16>, TTSError> {
        if !self.loaded {
            return Err(TTSError::ModelNotLoaded("xtts-v2".to_string()));
        }

        let duration_ms = text.len() as f32 * 60.0;
        let sample_rate = params.sample_rate;
        let num_samples = ((duration_ms / 1000.0) * sample_rate as f32) as usize;

        Ok(vec![0i16; num_samples])
    }

    async fn synthesize_stream(
        &self,
        text: &str,
        params: &TTSParams,
    ) -> Result<TTSAudioStream, TTSError> {
        if !self.loaded {
            return Err(TTSError::ModelNotLoaded("xtts-v2".to_string()));
        }

        let (tx, rx) = mpsc::channel(32);
        let text = text.to_string();
        let sample_rate = params.sample_rate;

        tokio::spawn(async move {
            let chunk_duration_ms = 20.0;
            let samples_per_chunk = ((chunk_duration_ms / 1000.0) * sample_rate as f32) as usize;
            let total_duration_ms = text.len() as f32 * 60.0;
            let num_chunks = (total_duration_ms / chunk_duration_ms).ceil() as usize;

            for i in 0..num_chunks {
                let is_final = i == num_chunks - 1;
                let timestamp_us = (i as f32 * chunk_duration_ms * 1000.0) as u64;

                let chunk = TTSAudioChunk::new(
                    vec![0i16; samples_per_chunk],
                    sample_rate,
                    timestamp_us,
                    is_final,
                );

                if tx.send(Ok(chunk)).await.is_err() {
                    break;
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(
                    chunk_duration_ms as u64 / 2,
                ))
                .await;
            }
        });

        Ok(rx)
    }
}

// ============================================================================
// TTS ADAPTER REGISTRY
// ============================================================================

/// Registry for TTS adapters - allows runtime selection
pub struct TTSAdapterRegistry {
    adapters: HashMap<String, Arc<tokio::sync::RwLock<Box<dyn TTSAdapter>>>>,
    default_adapter: Option<String>,
}

impl TTSAdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
            default_adapter: None,
        }
    }

    /// Create registry with default adapters pre-registered
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Register adapters in quality order
        registry.register(Box::new(KokoroAdapter::new())); // #1
        registry.register(Box::new(FishSpeechAdapter::new())); // #7
        registry.register(Box::new(F5TTSAdapter::new()));
        registry.register(Box::new(StyleTTS2Adapter::new()));
        registry.register(Box::new(XTTSv2Adapter::new())); // #4

        // Set Kokoro as default (highest quality)
        registry.set_default("kokoro");

        registry
    }

    /// Register a TTS adapter
    pub fn register(&mut self, adapter: Box<dyn TTSAdapter>) {
        let name = adapter.name().to_string();
        self.adapters
            .insert(name, Arc::new(tokio::sync::RwLock::new(adapter)));
    }

    /// Set default adapter
    pub fn set_default(&mut self, name: &str) {
        if self.adapters.contains_key(name) {
            self.default_adapter = Some(name.to_string());
        }
    }

    /// Get adapter by name
    pub fn get(&self, name: &str) -> Option<Arc<tokio::sync::RwLock<Box<dyn TTSAdapter>>>> {
        self.adapters.get(name).cloned()
    }

    /// Get default adapter
    pub fn get_default(&self) -> Option<Arc<tokio::sync::RwLock<Box<dyn TTSAdapter>>>> {
        self.default_adapter
            .as_ref()
            .and_then(|name| self.get(name))
    }

    /// List all registered adapters
    pub fn list(&self) -> Vec<(&str, &str)> {
        // This is a bit awkward due to async RwLock, return names only
        self.adapters.keys().map(|k| (k.as_str(), k.as_str())).collect()
    }

    /// Get adapter names
    pub fn names(&self) -> Vec<String> {
        self.adapters.keys().cloned().collect()
    }
}

impl Default for TTSAdapterRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kokoro_adapter_basic() {
        let mut adapter = KokoroAdapter::new();
        assert_eq!(adapter.name(), "kokoro");
        assert!(!adapter.is_loaded());

        adapter.load().await.unwrap();
        assert!(adapter.is_loaded());

        let params = TTSParams::default();
        let audio = adapter.synthesize("Hello world", &params).await.unwrap();
        assert!(!audio.is_empty());

        adapter.unload().await.unwrap();
        assert!(!adapter.is_loaded());
    }

    #[tokio::test]
    async fn test_kokoro_streaming() {
        let mut adapter = KokoroAdapter::new();
        adapter.load().await.unwrap();

        let params = TTSParams::default();
        let mut stream = adapter
            .synthesize_stream("Hello world", &params)
            .await
            .unwrap();

        let mut chunk_count = 0;
        let mut found_final = false;

        while let Some(result) = stream.recv().await {
            let chunk = result.unwrap();
            chunk_count += 1;
            if chunk.is_final {
                found_final = true;
                break;
            }
        }

        assert!(chunk_count > 0);
        assert!(found_final);
    }

    #[tokio::test]
    async fn test_registry() {
        let registry = TTSAdapterRegistry::with_defaults();

        assert!(registry.get("kokoro").is_some());
        assert!(registry.get("fish-speech").is_some());
        assert!(registry.get("f5-tts").is_some());
        assert!(registry.get("styletts2").is_some());
        assert!(registry.get("xtts-v2").is_some());
        assert!(registry.get("nonexistent").is_none());

        // Default should be Kokoro
        let default = registry.get_default().unwrap();
        let adapter = default.read().await;
        assert_eq!(adapter.name(), "kokoro");
    }

    #[tokio::test]
    async fn test_available_speakers() {
        let adapter = KokoroAdapter::new();
        let speakers = adapter.available_speakers();
        assert!(!speakers.is_empty());
        assert!(speakers.contains(&"af".to_string())); // American Female
    }

    #[test]
    fn test_tts_params_default() {
        let params = TTSParams::default();
        assert_eq!(params.speed, 1.0);
        assert_eq!(params.pitch, 0.0);
        assert_eq!(params.sample_rate, 24000);
        assert!(params.speaker_id.is_none());
    }

    #[test]
    fn test_audio_chunk_duration() {
        let chunk = TTSAudioChunk::new(vec![0i16; 480], 24000, 0, false);
        assert!((chunk.duration_ms() - 20.0).abs() < 0.01); // 480 samples at 24kHz = 20ms
    }
}
