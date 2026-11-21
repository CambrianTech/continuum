//! Speech-to-Text (STT) Adapter System
//!
//! Modular STT with swappable backends:
//! - Moonshine (local, default)
//! - OpenAI Realtime (streaming, semantic VAD)
//! - Stub (tests and keyless hosts)
//!
//! Uses trait-based polymorphism (OpenCV-style) for runtime flexibility.

mod moonshine;
mod openai_realtime;
mod stub;

pub use moonshine::MoonshineStt;
pub use openai_realtime::{OpenAIApiKey, OpenAIRealtimeSTT, TurnDetection, TurnDetectionType};
pub use stub::StubSTT;

use crate::clog_info;
use async_trait::async_trait;
use continuum_base::part::{Identity, Part};
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Global STT registry
static STT_REGISTRY: OnceCell<Arc<RwLock<STTRegistry>>> = OnceCell::new();

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

    /// The model's vocabulary has no such sequence marker, so decoding could not begin or end.
    #[error("the model's tokens.txt has no '{0}' piece")]
    VocabLacksPiece(&'static str),

    /// The decoder produced no logits to choose a token from.
    #[error("decoder returned empty logits")]
    EmptyLogits,

    /// The engine could not say which language it decoded in.
    #[error("language detection failed: {0}")]
    LanguageDetection(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// The engine named a language id it has no code for.
    #[error("engine returned unknown language id {0}")]
    UnknownLanguage(i32),

    /// A model path the engine's C API cannot take, because it is not UTF-8.
    #[error("model path {0:?} is not UTF-8")]
    NonUtf8ModelPath(std::path::PathBuf),
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

/// Speech-to-Text adapter trait
///
/// Implement this for each STT backend (Whisper, Deepgram, etc.)
#[async_trait]
pub trait SpeechToText: Part + Send + Sync {
    /// Check if adapter is ready for use
    fn is_initialized(&self) -> bool;

    /// Initialize the adapter (load models, connect to API, etc.)
    async fn initialize(&self) -> Result<(), STTError>;

    /// Transcribe audio samples
    ///
    /// # Arguments
    /// * `samples` - Audio samples as f32 (-1.0 to 1.0), 16kHz mono
    /// * `language` - Language code (e.g., "en") or None for auto-detection
    async fn transcribe(
        &self,
        samples: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptResult, STTError>;

    /// Get supported languages
    fn supported_languages(&self) -> Vec<&'static str> {
        vec!["en"] // Default to English only
    }

    /// Get configuration parameters
    fn get_param(&self, _name: &str) -> Option<String> {
        None
    }

    /// Set configuration parameter
    fn set_param(&self, _name: &str, _value: &str) -> Result<(), STTError> {
        Ok(())
    }

    /// Shut down the adapter, releasing loaded models and GPU allocations.
    ///
    /// In-flight inference that already cloned the model Arc continues safely.
    /// Memory is freed when the last Arc drops.
    /// Calling `initialize()` after `shutdown()` reloads models transparently.
    ///
    /// Default: no-op (adapters that don't hold heavy resources can skip this).
    async fn shutdown(&self) -> Result<(), STTError> {
        Ok(())
    }
}

/// STT Registry - manages available adapters
pub struct STTRegistry {
    adapters: HashMap<&'static str, Arc<dyn SpeechToText>>,
    active: Option<&'static str>,
}

impl STTRegistry {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
            active: None,
        }
    }

    /// Register an adapter
    pub fn register(&mut self, adapter: Arc<dyn SpeechToText>) {
        let name = adapter.identity().name;
        clog_info!("STT: Registering adapter '{}'", name);
        self.adapters.insert(name, adapter);

        // Auto-select first registered adapter
        if self.active.is_none() {
            self.active = Some(name);
        }
    }

    /// Set the active adapter
    pub fn set_active(&mut self, name: &'static str) -> Result<(), STTError> {
        if self.adapters.contains_key(name) {
            self.active = Some(name);
            clog_info!("STT: Active adapter set to '{}'", name);
            Ok(())
        } else {
            Err(STTError::AdapterNotFound(name.to_string()))
        }
    }

    /// Get the active adapter
    pub fn get_active(&self) -> Option<Arc<dyn SpeechToText>> {
        self.active
            .and_then(|name| self.adapters.get(name))
            .cloned()
    }

    /// Get adapter by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn SpeechToText>> {
        self.adapters.get(name).cloned()
    }

    /// List all registered adapters
    pub fn list(&self) -> Vec<(&'static str, bool)> {
        self.adapters
            .iter()
            .map(|(name, adapter)| (*name, adapter.is_initialized()))
            .collect()
    }

    /// Every registered adapter's identity and whether it is loaded, read in one pass.
    pub fn identities(&self) -> Vec<(Identity, bool)> {
        self.adapters
            .values()
            .map(|adapter| (adapter.identity(), adapter.is_initialized()))
            .collect()
    }

    /// Is there an active adapter, and is it loaded?
    pub fn is_initialized(&self) -> bool {
        self.get_active().is_some_and(|a| a.is_initialized())
    }
}

impl Default for STTRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Global Registry Functions
// ============================================================================

/// Initialize the global STT registry with default adapters
pub fn init_registry() -> Arc<RwLock<STTRegistry>> {
    let registry = STT_REGISTRY.get_or_init(|| {
        let mut reg = STTRegistry::new();


        // Register OpenAI Realtime adapter - streaming + semantic VAD. It exists only with
        // its API key: a keyless adapter is never registered, so it never complains later.
        match OpenAIRealtimeSTT::from_env() {
            Ok(adapter) => reg.register(Arc::new(adapter)),
            Err(e) => clog_info!("STT: openai-realtime unavailable (OPENAI_API_KEY: {e})"),
        }

        // Register Moonshine adapter - fast local ONNX, sub-100ms on short audio
        reg.register(Arc::new(MoonshineStt::new()));

        // Register Stub adapter - for testing/development
        reg.register(Arc::new(StubSTT::new()));

        Arc::new(RwLock::new(reg))
    });

    clog_info!(
        "STT: Registry initialized with {} adapters",
        registry.read().adapters.len()
    );
    registry.clone()
}

/// Get the global registry
pub fn get_registry() -> Arc<RwLock<STTRegistry>> {
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
            // Not registered: gated off in this build, or (openai-realtime) no API key.
            misses.push(format!("{name}: not registered"));
            continue;
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

