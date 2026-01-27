//! Speech-to-Text (STT) Adapter System
//!
//! Modular STT with swappable backends:
//! - Whisper (local, default)
//! - OpenAI Whisper API
//! - Deepgram
//! - Google Cloud Speech
//! - Azure Cognitive Services
//!
//! Uses trait-based polymorphism (OpenCV-style) for runtime flexibility.

mod stub;
mod whisper;

pub use stub::StubSTT;
pub use whisper::WhisperSTT;

use async_trait::async_trait;
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
pub trait SpeechToText: Send + Sync {
    /// Adapter name (e.g., "whisper", "deepgram")
    fn name(&self) -> &'static str;

    /// Human-readable description
    fn description(&self) -> &'static str;

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
        let name = adapter.name();
        tracing::info!("STT: Registering adapter '{}'", name);
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
            tracing::info!("STT: Active adapter set to '{}'", name);
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

    /// Check if any adapter is initialized
    pub fn is_initialized(&self) -> bool {
        self.get_active()
            .map(|a| a.is_initialized())
            .unwrap_or(false)
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
pub fn init_registry() {
    let registry = STT_REGISTRY.get_or_init(|| {
        let mut reg = STTRegistry::new();

        // Register Whisper (local) adapter - primary production adapter
        reg.register(Arc::new(WhisperSTT::new()));

        // Register Stub adapter - for testing/development
        reg.register(Arc::new(StubSTT::new()));

        // Future: Register API-based adapters
        // reg.register(Arc::new(OpenAIWhisperSTT::new()));
        // reg.register(Arc::new(DeepgramSTT::new()));

        Arc::new(RwLock::new(reg))
    });

    tracing::info!(
        "STT: Registry initialized with {} adapters",
        registry.read().adapters.len()
    );
}

/// Get the global registry
pub fn get_registry() -> Arc<RwLock<STTRegistry>> {
    STT_REGISTRY.get().cloned().unwrap_or_else(|| {
        init_registry();
        STT_REGISTRY.get().cloned().unwrap()
    })
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
pub async fn initialize() -> Result<(), STTError> {
    let adapter = get_registry()
        .read()
        .get_active()
        .ok_or_else(|| STTError::AdapterNotFound("No active STT adapter".to_string()))?;

    adapter.initialize().await
}

// Audio utility functions moved to crate::utils::audio
// Use crate::utils::audio::{i16_to_f32, resample, resample_to_16k} instead
