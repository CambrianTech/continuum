//! Speech-to-Text (STT) Adapter System
//!
//! Modular STT with swappable backends:
//! - Whisper (local, default)
//! - OpenAI Realtime (streaming, semantic VAD)
//! - Deepgram
//! - Google Cloud Speech
//! - Azure Cognitive Services
//!
//! Uses trait-based polymorphism (OpenCV-style) for runtime flexibility.

mod moonshine;
mod openai_realtime;
mod stub;
// mod whisper;  // TEMPORARY: disabled with whisper-rs dep to test ggml collision

pub use moonshine::MoonshineStt;
pub use openai_realtime::{OpenAIRealtimeSTT, TurnDetection, TurnDetectionType};
pub use stub::StubSTT;
// pub use whisper::WhisperSTT;

use crate::clog_info;
use async_trait::async_trait;
use super::backend::Backend;
use super::registry::{NotRegistered, Registry};
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use std::sync::Arc;
use thiserror::Error;

/// Global STT registry
static STT_REGISTRY: OnceCell<Arc<RwLock<Registry<dyn SpeechToText>>>> = OnceCell::new();

/// STT errors
#[derive(Error, Debug)]
pub enum STTError {
    #[error("Model not loaded: {0}")]
    ModelNotLoaded(String),

    #[error("Inference failed: {0}")]
    InferenceFailed(String),

    #[error("Invalid audio: {0}")]
    InvalidAudio(String),

    #[error("Adapter not found: {0}")]
    AdapterNotFound(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Transcription result
#[derive(Debug, Clone)]
pub struct TranscriptResult {
    pub text: String,
    pub language: String,
    pub confidence: f32,
    pub segments: Vec<TranscriptSegment>,
}

/// Word/phrase segment with timing
#[derive(Debug, Clone)]
pub struct TranscriptSegment {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// A speech-to-text backend: a [`Backend`] that transcribes. Implement it for each engine (Whisper,
/// Moonshine, a cloud API); loading, parameters and release are the backend generation's.
#[async_trait]
pub trait SpeechToText: Backend<Error = STTError> {
    /// Transcribe audio samples: f32 in -1.0..1.0, 16 kHz mono; `language` a code ("en") or `None`
    /// to detect it.
    async fn transcribe(&self, samples: Vec<f32>, language: Option<&str>) -> Result<TranscriptResult, STTError>;

    /// The languages it transcribes.
    fn supported_languages(&self) -> Vec<&'static str> {
        vec!["en"]
    }
}

impl From<NotRegistered> for STTError {
    fn from(missing: NotRegistered) -> Self {
        STTError::AdapterNotFound(missing.name.to_string())
    }
}

// ============================================================================
// Global Registry Functions
// ============================================================================

/// Initialize the global STT registry with default adapters
pub fn init_registry() -> Arc<RwLock<Registry<dyn SpeechToText>>> {
    let registry = STT_REGISTRY.get_or_init(|| {
        let mut reg = Registry::<dyn SpeechToText>::new("STT");

        // TEMPORARY: Whisper registration disabled — whisper-rs dep is gated off
        // while we debug the ggml backend registry collision with our llama crate.
        // reg.register(Arc::new(WhisperSTT::new()));

        // Register OpenAI Realtime adapter - streaming + semantic VAD
        // Will be used when OPENAI_API_KEY is set and fast response needed
        reg.register(Arc::new(OpenAIRealtimeSTT::new()));

        // Register Moonshine adapter - fast local ONNX, sub-100ms on short audio
        reg.register(Arc::new(MoonshineStt::new()));

        // Register Stub adapter - for testing/development
        reg.register(Arc::new(StubSTT::new()));

        Arc::new(RwLock::new(reg))
    });

    clog_info!(
        "STT: Registry initialized with {} adapters",
        registry.read().len()
    );
    registry.clone()
}

/// Get the global registry
pub fn get_registry() -> Arc<RwLock<Registry<dyn SpeechToText>>> {
    match STT_REGISTRY.get() {
        Some(registry) => registry.clone(),
        None => init_registry(),
    }
}

/// Check if STT is initialized (convenience function)
pub fn is_initialized() -> bool {
    get_registry().read().is_initialized()
}

/// Transcribe using the active adapter (convenience function)
pub async fn transcribe(
    samples: Vec<f32>,
    language: Option<&str>,
) -> Result<TranscriptResult, STTError> {
    let adapter = get_registry()
        .read()
        .get_active()
        .ok_or_else(|| STTError::AdapterNotFound("No active STT adapter".to_string()))?;

    adapter.transcribe(samples, language).await
}

/// Initialize the active adapter
/// Adapter preference at init: LOCAL first (sovereignty — a fresh clone with
/// models on disk hears with zero keys), cloud second, stub last (tests only).
/// This is engine SELECTION by design (the TTS registry's shape), not a
/// mistake-masking fallback: each miss is named loudly, and the winner is
/// promoted to active so every later transcribe uses the engine that proved
/// it loads.
/// The STUB is deliberately ABSENT: it fabricates transcriptions (canned
/// text for any audio — the gunfire false-positive test's whole point), so
/// auto-selecting it would make silence indistinguishable from speech. Tests
/// that want it call `set_active("stub")` explicitly; a box with no real
/// engine errs loudly instead ([[fallbacks-are-illegal-fail-loud]]).
const INIT_PREFERENCE: &[&str] = &["moonshine", "openai-realtime"];

pub async fn initialize() -> Result<(), STTError> {
    // Try each preferred engine in order; first successful init wins and
    // becomes active. Before this, init consulted only the FIRST-registered
    // adapter — OpenAI, keyless — so a machine with a perfectly good local
    // moonshine/whisper model sat deaf behind a misleading "download
    // ggml-base" warning (found live 2026-08-31, the voice-spec audit).
    let mut misses: Vec<String> = Vec::new();
    for name in INIT_PREFERENCE {
        let Some(adapter) = get_registry().read().get(name) else {
            continue; // not registered in this build (whisper is gated off)
        };
        match adapter.initialize().await {
            Ok(()) => {
                get_registry().write().set_active(adapter.identity().name)?;
                if !misses.is_empty() {
                    clog_info!(
                        "STT: '{}' active (earlier engines unavailable: {})",
                        name,
                        misses.join("; ")
                    );
                }
                return Ok(());
            }
            Err(e) => misses.push(format!("{name}: {e}")),
        }
    }
    Err(STTError::AdapterNotFound(format!(
        "no STT engine could initialize — {}",
        misses.join("; ")
    )))
}

