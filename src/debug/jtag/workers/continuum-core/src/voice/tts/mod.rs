//! Text-to-Speech (TTS) Adapter System
//!
//! Modular TTS with swappable backends:
//! - Kokoro (local, ONNX, 82M params - PRIMARY, fast)
//! - Piper (local, ONNX - fallback)
//! - Silence (fallback for testing)
//!
//! Uses trait-based polymorphism for runtime flexibility.

mod piper;
mod kokoro;
mod silence;
mod phonemizer;

pub use piper::PiperTTS;
pub use kokoro::KokoroTTS;
pub use silence::SilenceTTS;
pub(crate) use phonemizer::Phonemizer;

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
        self.get_active()
            .map(|a| a.is_initialized())
            .unwrap_or(false)
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

        // Register Kokoro (local, ONNX, 82M) - PRIMARY
        // Kokoro is the default because it:
        // - Fast (~97ms TTFB, 82M params)
        // - High quality voices (natural sounding)
        // - Uses espeak-ng phonemizer (deterministic)
        reg.register(Arc::new(KokoroTTS::new()));

        // Register Piper (local, ONNX) - fallback
        reg.register(Arc::new(PiperTTS::new()));

        // Register Silence adapter - testing fallback
        reg.register(Arc::new(SilenceTTS::new()));

        Arc::new(RwLock::new(reg))
    });

    tracing::info!(
        "TTS: Registry initialized with {} adapters",
        registry.read().adapters.len()
    );
}

/// Get the global registry
pub fn get_registry() -> Arc<RwLock<TTSRegistry>> {
    TTS_REGISTRY.get().cloned().unwrap_or_else(|| {
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

/// Initialize the active adapter, falling back to next adapter on failure
pub async fn initialize() -> Result<(), TTSError> {
    let registry = get_registry();
    let adapter_names: Vec<&'static str> = registry.read().list().iter().map(|(name, _)| *name).collect();

    for name in &adapter_names {
        let adapter = registry.read().get(name);
        if let Some(adapter) = adapter {
            match adapter.initialize().await {
                Ok(()) => {
                    tracing::info!("TTS: '{}' initialized successfully", name);
                    let _ = registry.write().set_active(name);
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!("TTS: '{}' failed to initialize: {}, trying next...", name, e);
                }
            }
        }
    }

    Err(TTSError::ModelNotLoaded("No TTS adapter could be initialized".into()))
}

/// Synthesize using a specific adapter by name (bypasses active adapter)
pub async fn synthesize_with(text: &str, voice: &str, adapter_name: &str) -> Result<SynthesisResult, TTSError> {
    let adapter = get_registry()
        .read()
        .get(adapter_name)
        .ok_or_else(|| TTSError::AdapterNotFound(format!("Adapter '{}' not found", adapter_name)))?;

    if !adapter.is_initialized() {
        adapter.initialize().await?;
    }

    adapter.synthesize(text, voice).await
}

/// Get available voices from active adapter
pub fn available_voices() -> Vec<VoiceInfo> {
    get_registry()
        .read()
        .get_active()
        .map(|a| a.available_voices())
        .unwrap_or_default()
}
