//! Text-to-Speech (TTS) Adapter System
//!
//! Modular TTS with swappable backends:
//! - Kokoro (local, default)
//! - ElevenLabs API
//! - OpenAI TTS API
//! - Azure Cognitive Services
//! - Google Cloud TTS
//!
//! Uses trait-based polymorphism for runtime flexibility.

mod kokoro;

pub use kokoro::KokoroTTS;

use async_trait::async_trait;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Global TTS registry
static TTS_REGISTRY: OnceCell<Arc<RwLock<TTSRegistry>>> = OnceCell::new();

/// TTS errors
#[derive(Error, Debug)]
pub enum TTSError {
    #[error("Model not loaded: {0}")]
    ModelNotLoaded(String),

    #[error("Synthesis failed: {0}")]
    SynthesisFailed(String),

    #[error("Invalid text: {0}")]
    InvalidText(String),

    #[error("Voice not found: {0}")]
    VoiceNotFound(String),

    #[error("Adapter not found: {0}")]
    AdapterNotFound(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Voice information
#[derive(Debug, Clone)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    pub language: String,
    pub gender: Option<String>,
    pub description: Option<String>,
}

/// Synthesis result
#[derive(Debug, Clone)]
pub struct SynthesisResult {
    /// Audio samples as i16 PCM, 16kHz mono
    pub samples: Vec<i16>,
    /// Sample rate (always 16000 for consistency)
    pub sample_rate: u32,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Text-to-Speech adapter trait
///
/// Implement this for each TTS backend (Kokoro, ElevenLabs, etc.)
#[async_trait]
pub trait TextToSpeech: Send + Sync {
    /// Adapter name (e.g., "kokoro", "elevenlabs")
    fn name(&self) -> &'static str;

    /// Human-readable description
    fn description(&self) -> &'static str;

    /// Check if adapter is ready for use
    fn is_initialized(&self) -> bool;

    /// Initialize the adapter (load models, connect to API, etc.)
    async fn initialize(&self) -> Result<(), TTSError>;

    /// Synthesize speech from text
    ///
    /// # Arguments
    /// * `text` - Text to synthesize
    /// * `voice` - Voice ID (adapter-specific)
    async fn synthesize(&self, text: &str, voice: &str) -> Result<SynthesisResult, TTSError>;

    /// Get available voices
    fn available_voices(&self) -> Vec<VoiceInfo>;

    /// Get default voice ID
    fn default_voice(&self) -> &str {
        "default"
    }

    /// Get configuration parameter
    fn get_param(&self, _name: &str) -> Option<String> {
        None
    }

    /// Set configuration parameter
    fn set_param(&self, _name: &str, _value: &str) -> Result<(), TTSError> {
        Ok(())
    }
}

/// TTS Registry - manages available adapters
pub struct TTSRegistry {
    adapters: HashMap<&'static str, Arc<dyn TextToSpeech>>,
    active: Option<&'static str>,
}

impl TTSRegistry {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
            active: None,
        }
    }

    /// Register an adapter
    pub fn register(&mut self, adapter: Arc<dyn TextToSpeech>) {
        let name = adapter.name();
        tracing::info!("TTS: Registering adapter '{}'", name);
        self.adapters.insert(name, adapter);

        // Auto-select first registered adapter
        if self.active.is_none() {
            self.active = Some(name);
        }
    }

    /// Set the active adapter
    pub fn set_active(&mut self, name: &'static str) -> Result<(), TTSError> {
        if self.adapters.contains_key(name) {
            self.active = Some(name);
            tracing::info!("TTS: Active adapter set to '{}'", name);
            Ok(())
        } else {
            Err(TTSError::AdapterNotFound(name.to_string()))
        }
    }

    /// Get the active adapter
    pub fn get_active(&self) -> Option<Arc<dyn TextToSpeech>> {
        self.active
            .and_then(|name| self.adapters.get(name))
            .cloned()
    }

    /// Get adapter by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn TextToSpeech>> {
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
        self.get_active().map(|a| a.is_initialized()).unwrap_or(false)
    }
}

impl Default for TTSRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Global Registry Functions
// ============================================================================

/// Initialize the global TTS registry with default adapters
pub fn init_registry() {
    let registry = TTS_REGISTRY.get_or_init(|| {
        let mut reg = TTSRegistry::new();

        // Register Kokoro (local) adapter
        reg.register(Arc::new(KokoroTTS::new()));

        // Future: Register API-based adapters
        // reg.register(Arc::new(ElevenLabsTTS::new()));
        // reg.register(Arc::new(OpenAITTS::new()));

        Arc::new(RwLock::new(reg))
    });

    tracing::info!("TTS: Registry initialized with {} adapters",
        registry.read().adapters.len());
}

/// Get the global registry
pub fn get_registry() -> Arc<RwLock<TTSRegistry>> {
    TTS_REGISTRY
        .get()
        .cloned()
        .unwrap_or_else(|| {
            init_registry();
            TTS_REGISTRY.get().cloned().unwrap()
        })
}

/// Check if TTS is initialized (convenience function)
pub fn is_initialized() -> bool {
    get_registry().read().is_initialized()
}

/// Synthesize using the active adapter (convenience function)
pub async fn synthesize(text: &str, voice: &str) -> Result<SynthesisResult, TTSError> {
    let adapter = get_registry()
        .read()
        .get_active()
        .ok_or_else(|| TTSError::AdapterNotFound("No active TTS adapter".to_string()))?;

    adapter.synthesize(text, voice).await
}

/// Initialize the active adapter
pub async fn initialize() -> Result<(), TTSError> {
    let adapter = get_registry()
        .read()
        .get_active()
        .ok_or_else(|| TTSError::AdapterNotFound("No active TTS adapter".to_string()))?;

    adapter.initialize().await
}

/// Get available voices from active adapter
pub fn available_voices() -> Vec<VoiceInfo> {
    get_registry()
        .read()
        .get_active()
        .map(|a| a.available_voices())
        .unwrap_or_default()
}
