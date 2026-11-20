//! Text-to-Speech (TTS) Adapter System
//!
//! Modular TTS with swappable backends, all LOCAL (priority order):
//! - Kokoro (local, ONNX, 82M - the backwards-compatibility floor, fast)
//! - Pocket (local, Candle, 117M - voice cloning, 8 preset voices; slow on CPU)
//! - Orpheus (local, Candle GGUF, 3B - LoRA-trainable, emotion tags)
//! - Piper (local, ONNX - offline fallback)
//! - Silence (testing only)
//!
//! No cloud voice, ever (Joel, 2026-10-03: "We are wanting to avoid cloud entirely").
//! Persona voices are headed to LoRA-learned voices on a local speech model
//! (docs/planning/VOICE-ENGINE-PLAN.md); these adapters are the floor until then.
//!
//! Uses trait-based polymorphism for runtime flexibility.

use super::backend::Backend;
use super::registry::{NotRegistered, Registry};

pub(crate) mod normalize;
mod kokoro;
mod orpheus;
mod phonemizer;
mod piper;
#[cfg(feature = "pocket-tts")]
mod pocket;
mod silence;

pub use kokoro::KokoroTTS;
pub use orpheus::OrpheusTts;
pub(crate) use phonemizer::Phonemizer;
pub use piper::PiperTTS;
#[cfg(feature = "pocket-tts")]
pub use pocket::PocketTTS;
pub use silence::SilenceTTS;

use crate::gpu::memory_manager::GpuMemoryManager;
use crate::{clog_info, clog_warn};
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use std::sync::{Arc, OnceLock};
use thiserror::Error;

/// GPU memory manager for TTS subsystem allocation tracking.
/// Set once during server startup, used by adapters during model loading.
static TTS_GPU_MANAGER: OnceLock<Arc<GpuMemoryManager>> = OnceLock::new();

/// Set the GPU memory manager for TTS model allocation tracking.
/// Call before TTS initialization to enable VRAM tracking.
pub fn set_gpu_manager(mgr: Arc<GpuMemoryManager>) {
    let _ = TTS_GPU_MANAGER.set(mgr);
}

/// Get the GPU memory manager (if set).
pub(crate) fn gpu_manager() -> Option<&'static Arc<GpuMemoryManager>> {
    TTS_GPU_MANAGER.get()
}

/// Deterministic hash for voice resolution.
/// Maps any string (UUID, uniqueId, name) to a stable usize.
/// Uses FNV-1a for speed and good distribution.
fn deterministic_hash(s: &str) -> usize {
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV prime
    }
    hash as usize
}

/// Safely truncate a UTF-8 string to at most `max_bytes` bytes at a char boundary.
/// IPA phoneme strings contain multi-byte characters (ˈ, ə, ɪ, etc.) — byte-slicing panics.
pub(crate) fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Global TTS registry
static TTS_REGISTRY: OnceCell<Arc<RwLock<Registry<dyn TextToSpeech>>>> = OnceCell::new();

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
    /// Resolved voice name (e.g., "af_bella" after UUID→voice resolution).
    /// Set by the top-level synthesize() wrappers, not by individual adapters.
    pub voice_name: Option<String>,
}

/// A text-to-speech backend: a [`Backend`] that synthesizes speech in its voices. Implement it for
/// each engine (Kokoro, Orpheus, Piper); loading, parameters and release are the backend
/// generation's.
#[async_trait]
pub trait TextToSpeech: Backend<Error = TTSError> {
    /// Synthesize `text` in `voice` (an engine voice id).
    async fn synthesize(&self, text: &str, voice: &str) -> Result<SynthesisResult, TTSError>;

    /// The voices it speaks in.
    fn available_voices(&self) -> Vec<VoiceInfo>;

    /// Get default voice ID
    fn default_voice(&self) -> &str {
        "default"
    }

    /// Resolve voice identifier to an adapter-specific voice.
    ///
    /// Generic at the base — the adapter can take:
    /// 1. A named voice ("af", "am_adam") → used directly
    /// 2. A numeric seed ("42") → modulo into available voices
    /// 3. A uniqueId/UUID or any string → deterministic hash → modulo into available voices
    ///
    /// Each adapter's `available_voices()` defines the voice set.
    /// Consistent by uniqueId: same input always produces same voice.
    fn resolve_voice(&self, voice: &str) -> String {
        let voices = self.available_voices();

        // 1. Known voice name → use directly
        if voices.iter().any(|v| v.id == voice) {
            return voice.to_string();
        }

        // 2. Numeric seed → modulo into voice list
        if let Ok(seed) = voice.parse::<usize>() {
            if !voices.is_empty() {
                return voices[seed % voices.len()].id.clone();
            }
        }

        // 3. Arbitrary string (uniqueId, UUID, etc.) → deterministic hash → modulo
        if !voices.is_empty() {
            let seed = deterministic_hash(voice);
            return voices[seed % voices.len()].id.clone();
        }

        // Fallback to adapter's default
        self.default_voice().to_string()
    }

}

impl From<NotRegistered> for TTSError {
    fn from(missing: NotRegistered) -> Self {
        TTSError::AdapterNotFound(missing.name.to_string())
    }
}

// ============================================================================
// Global Registry Functions
// ============================================================================

/// Initialize the global TTS registry with default adapters
pub fn init_registry() -> Arc<RwLock<Registry<dyn TextToSpeech>>> {
    let registry = TTS_REGISTRY.get_or_init(|| {
        let mut reg = Registry::<dyn TextToSpeech>::new("TTS");

        // Register Pocket-TTS (local, Candle, 100M) - fast CPU TTS with voice cloning
        // ≤600ms TTFA, 8 preset voices, clone any voice from 5-15s WAV reference audio.
        // Optional (feature `pocket-tts`); the priority lists below skip it when absent.
        #[cfg(feature = "pocket-tts")]
        reg.register(Arc::new(PocketTTS::new()));

        // Register Orpheus (local, Candle GGUF, 3B) - expressive with emotion tags
        // LoRA-trainable (Llama architecture), <laugh> <sigh> <gasp> emotion control
        reg.register(Arc::new(OrpheusTts::new()));

        // Register Kokoro (local, ONNX, 82M) - fast offline fallback
        reg.register(Arc::new(KokoroTTS::new()));

        // Register Piper (local, ONNX) - fallback
        reg.register(Arc::new(PiperTTS::new()));

        // Register Silence adapter - testing fallback
        reg.register(Arc::new(SilenceTTS::new()));

        Arc::new(RwLock::new(reg))
    });

    clog_info!(
        "TTS: Registry initialized with {} adapters",
        registry.read().len()
    );
    registry.clone()
}

/// Get the global registry
pub fn get_registry() -> Arc<RwLock<Registry<dyn TextToSpeech>>> {
    match TTS_REGISTRY.get() {
        Some(registry) => registry.clone(),
        None => init_registry(),
    }
}

/// Check if TTS is initialized (convenience function)
pub fn is_initialized() -> bool {
    get_registry().read().is_initialized()
}

/// Resolve a voice identifier with explicit gender filtering.
///
/// When `gender_hint` is provided (e.g., "male" or "female"), filters the voice list
/// by gender before hashing. This ensures voice matches the avatar's gender.
/// The gender hint flows from the caller who derives it from the agent's identity
/// using `gender_from_identity()` — the same function the avatar system uses.
fn resolve_voice_gendered(
    adapter: &dyn TextToSpeech,
    voice: &str,
    gender_hint: Option<&str>,
) -> String {
    let voices = adapter.available_voices();

    // 1. Known voice name → use directly (explicit choice overrides gender)
    if voices.iter().any(|v| v.id == voice) {
        return voice.to_string();
    }

    // 2. Numeric seed → modulo into voice list (legacy path)
    if let Ok(seed) = voice.parse::<usize>() {
        if !voices.is_empty() {
            return voices[seed % voices.len()].id.clone();
        }
    }

    // 3. UUID/arbitrary string → filter by gender hint, then hash.
    if !voices.is_empty() {
        let seed = deterministic_hash(voice);

        if let Some(gender) = gender_hint {
            let gendered: Vec<&VoiceInfo> = voices
                .iter()
                .filter(|v| v.gender.as_deref() == Some(gender))
                .collect();

            if !gendered.is_empty() {
                return gendered[seed % gendered.len()].id.clone();
            }
        }

        return voices[seed % voices.len()].id.clone();
    }

    adapter.default_voice().to_string()
}

/// Synthesize using the active adapter (convenience function)
///
/// `gender_hint`: Optional "male" or "female" to filter voices by gender.
/// When provided, ensures the TTS voice matches the avatar's gender.
pub async fn synthesize(
    text: &str,
    voice: &str,
    gender_hint: Option<&str>,
) -> Result<SynthesisResult, TTSError> {
    let adapter = get_registry()
        .read()
        .get_active()
        .ok_or_else(|| TTSError::AdapterNotFound("No active TTS adapter".to_string()))?;

    let resolved = resolve_voice_gendered(adapter.as_ref(), voice, gender_hint);
    clog_info!(
        "TTS: voice '{}' → '{}' (gender={}) for '{}'",
        voice,
        resolved,
        gender_hint.unwrap_or("any"),
        adapter.identity().name
    );
    match adapter.synthesize(text, &resolved).await {
        Ok(mut result) => {
            result.voice_name = Some(resolved);
            Ok(result)
        }
        Err(active_err) => {
            // ADAPTER FALL-THROUGH, loudly (2026-09-02, caught by the FIRST
            // voice/selftest run): the then-primary cloud adapter (Edge, removed
            // 2026-10-03) returned empty audio while Kokoro sat fully provisioned
            // on disk — and one provider's outage silenced every citizen. A registry of providers
            // is not a fallback hiding a defect: the next INITIALIZED adapter
            // takes the utterance, the voice re-resolves in ITS voice space,
            // and the swap is probed per-utterance so a degraded provider is a
            // visible fact, never a silent one. `silence` is excluded — zeros
            // are for tests, not for a persona's mouth.
            let candidates = ["kokoro", "pocket", "orpheus", "piper"];
            for name in candidates {
                if name == adapter.identity().name {
                    continue;
                }
                let Some(next) = get_registry().read().get(name) else {
                    continue;
                };
                if !next.is_initialized() {
                    // Wake it: only the ACTIVE adapter initializes at boot, so
                    // a provisioned local engine (Kokoro, 2026-09-02) sat
                    // skippable exactly when it was needed. Bounded by the
                    // adapter's own init; a failed wake moves on.
                    if next.initialize().await.is_err() {
                        continue;
                    }
                }
                let re_resolved = resolve_voice_gendered(next.as_ref(), voice, gender_hint);
                if let Ok(mut result) = next.synthesize(text, &re_resolved).await {
                    continuum_base::probe!(
                        class = "tts.fallthrough",
                        from = adapter.identity().name,
                        to = name,
                        error = %active_err,
                        "active TTS adapter failed — utterance served by the next local adapter"
                    );
                    result.voice_name = Some(re_resolved);
                    return Ok(result);
                }
            }
            Err(active_err)
        }
    }
}

/// Initialize the active adapter, falling back to next adapter on failure.
///
/// Uses a defined priority order (not HashMap iteration order, which is random).
/// If `preferred` is specified, it's tried first before the default priority.
pub async fn initialize_with_preference(preferred: Option<&str>) -> Result<(), TTSError> {
    let registry = get_registry();

    // Priority: kokoro → pocket → orpheus → piper → silence. All local.
    // Kokoro: 82M ONNX, ~97ms — the backwards-compatibility floor, first because it is fast
    // Pocket: 117M Candle, voice cloning, 8 preset voices — but 23x slower than realtime on CPU, single Mutex
    // Orpheus: 3B GGUF, emotion tags, LoRA-trainable — CUSTOM VOICES
    // The LoRA-learned voice adapter takes the top seat when `voice/selftest` proves it
    // (docs/planning/VOICE-ENGINE-PLAN.md).
    let default_priority: Vec<&str> =
        vec!["kokoro", "pocket", "orpheus", "piper", "silence"];

    // Build final order: preferred first (if specified and exists), then remaining in priority
    let mut order: Vec<&str> = Vec::new();
    if let Some(pref) = preferred {
        if registry.read().get(pref).is_some() {
            order.push(pref);
        }
    }
    for name in &default_priority {
        if !order.contains(name) {
            order.push(name);
        }
    }

    for name in &order {
        let adapter = registry.read().get(name);
        if let Some(adapter) = adapter {
            match adapter.initialize().await {
                Ok(()) => {
                    // Use adapter.identity().name which returns &'static str
                    let static_name = adapter.identity().name;
                    clog_info!("TTS: '{}' initialized successfully", static_name);
                    let _ = registry.write().set_active(static_name);
                    return Ok(());
                }
                Err(e) => {
                    clog_warn!(
                        "TTS: '{}' failed to initialize: {}, trying next...",
                        name,
                        e
                    );
                }
            }
        }
    }

    Err(TTSError::ModelNotLoaded(
        "No TTS adapter could be initialized".into(),
    ))
}

/// Initialize the active adapter with default priority (kokoro → pocket → orpheus → piper → silence)
pub async fn initialize() -> Result<(), TTSError> {
    initialize_with_preference(None).await
}

/// Synthesize using a specific adapter by name (bypasses active adapter)
///
/// `gender_hint`: Optional "male" or "female" to filter voices by gender.
pub async fn synthesize_with(
    text: &str,
    voice: &str,
    adapter_name: &str,
    gender_hint: Option<&str>,
) -> Result<SynthesisResult, TTSError> {
    let adapter = get_registry()
        .read()
        .get(adapter_name)
        .ok_or_else(|| TTSError::AdapterNotFound(format!("Adapter '{adapter_name}' not found")))?;

    if !adapter.is_initialized() {
        adapter.initialize().await?;
    }

    let resolved = resolve_voice_gendered(adapter.as_ref(), voice, gender_hint);
    clog_info!(
        "TTS: voice '{}' → '{}' (gender={}) for '{}'",
        voice,
        resolved,
        gender_hint.unwrap_or("any"),
        adapter_name
    );
    let mut result = adapter.synthesize(text, &resolved).await?;
    result.voice_name = Some(resolved);
    Ok(result)
}

/// Get available voices from active adapter
pub fn available_voices() -> Vec<VoiceInfo> {
    get_registry()
        .read()
        .get_active()
        .map(|a| a.available_voices())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn resolve_voice_is_identity_seeded_and_unique() {
        // what this catches: [[procedural-persona-genesis]] Gap #2 — the voice is
        // resolved from a per-call SEED, and the persona-speak path now passes the
        // persona IDENTITY as that seed (not the constant "default"). So distinct
        // personas of the same gender get DISTINCT voices (unique) and the same
        // persona is STABLE + gender-coherent. Before the caller fix every persona
        // passed "default" → deterministic_hash("default") → ONE shared voice.
        let kokoro = KokoroTTS::new();
        let female: Vec<String> = kokoro
            .available_voices()
            .into_iter()
            .filter(|v| v.gender.as_deref() == Some("female"))
            .map(|v| v.id)
            .collect();
        assert!(
            female.len() >= 2,
            "need ≥2 female voices to prove distinctness"
        );

        // same identity → stable, gender-coherent voice
        let a = resolve_voice_gendered(&kokoro, "persona-alpha", Some("female"));
        let a2 = resolve_voice_gendered(&kokoro, "persona-alpha", Some("female"));
        assert_eq!(a, a2, "same identity seed must yield the same voice");
        assert!(
            female.contains(&a),
            "picked voice must be female (gender-coherent)"
        );

        // distinct identities → a SPREAD across the female pool, not all one voice
        let picked: std::collections::HashSet<String> = (0..50)
            .map(|i| resolve_voice_gendered(&kokoro, &format!("persona-{i}"), Some("female")))
            .collect();
        assert!(
            picked.len() >= 2,
            "identity-seeded voices must spread across the female pool, got {}",
            picked.len()
        );
        assert!(
            picked.iter().all(|v| female.contains(v)),
            "all picks stay female"
        );
    }

    #[test]
    fn test_silence_adapter_synthesize() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let silence = SilenceTTS::new();

        // Must initialize first
        rt.block_on(async { silence.initialize().await })
            .expect("Init should succeed");
        assert!(silence.is_initialized());

        let result = rt.block_on(async { silence.synthesize("test text", "default").await });

        let synthesis = result.expect("Silence adapter should always succeed");
        assert!(synthesis.samples.len() > 0, "Should produce some samples");
        assert_eq!(
            synthesis.sample_rate,
            continuum_base::audio_constants::AUDIO_SAMPLE_RATE
        );
        // All samples should be zero (silence)
        assert!(
            synthesis.samples.iter().all(|&s| s == 0),
            "Silence adapter should produce silence"
        );
    }

    #[test]
    fn test_tts_error_variants() {
        // Ensure error types are constructible and displayable
        let e1 = TTSError::ModelNotLoaded("test".into());
        assert!(format!("{}", e1).contains("test"));

        let e2 = TTSError::SynthesisFailed("synthesis error".into());
        assert!(format!("{}", e2).contains("synthesis error"));

        let e3 = TTSError::InvalidText("empty".into());
        assert!(format!("{}", e3).contains("empty"));

        let e4 = TTSError::VoiceNotFound("unknown".into());
        assert!(format!("{}", e4).contains("unknown"));

        let e5 = TTSError::AdapterNotFound("missing".into());
        assert!(format!("{}", e5).contains("missing"));
    }

    #[test]
    fn test_deterministic_hash_consistency() {
        // Same input → same hash (deterministic)
        let h1 = deterministic_hash("abc-123-def");
        let h2 = deterministic_hash("abc-123-def");
        assert_eq!(h1, h2, "Same input must produce same hash");

        // Different inputs → different hashes
        let h3 = deterministic_hash("xyz-456-ghi");
        assert_ne!(h1, h3, "Different inputs should produce different hashes");
    }

    #[test]
    fn test_deterministic_hash_uuid_like() {
        // UUID-like inputs should produce stable hashes
        let uuid1 = "a1b2c3d4-e5f6-4789-abcd-ef0123456789";
        let uuid2 = "f1e2d3c4-b5a6-4789-abcd-ef9876543210";
        let h1 = deterministic_hash(uuid1);
        let h2 = deterministic_hash(uuid2);
        assert_ne!(h1, h2, "Different UUIDs should hash differently");
        assert_eq!(h1, deterministic_hash(uuid1), "Same UUID always same hash");
    }

    #[test]
    fn test_resolve_voice_named() {
        // Named Kokoro voices should pass through
        let kokoro = KokoroTTS::new();
        assert_eq!(kokoro.resolve_voice("af"), "af");
        assert_eq!(kokoro.resolve_voice("am_adam"), "am_adam");
        assert_eq!(kokoro.resolve_voice("bf_emma"), "bf_emma");
    }

    #[test]
    fn test_resolve_voice_numeric_seed() {
        // Numeric seeds should modulo into voice list
        let kokoro = KokoroTTS::new();
        let voices = kokoro.available_voices();
        let voice_count = voices.len(); // 11 for Kokoro

        // seed 0 → first voice
        assert_eq!(kokoro.resolve_voice("0"), voices[0].id);
        // seed == count → wraps to first
        assert_eq!(kokoro.resolve_voice(&voice_count.to_string()), voices[0].id);
        // seed 1 → second voice
        assert_eq!(kokoro.resolve_voice("1"), voices[1].id);
    }

    #[test]
    fn test_resolve_voice_uuid_string() {
        // UUID strings should hash deterministically
        let kokoro = KokoroTTS::new();
        let uuid = "a1b2c3d4-e5f6-4789-abcd-ef0123456789";

        let v1 = kokoro.resolve_voice(uuid);
        let v2 = kokoro.resolve_voice(uuid);
        assert_eq!(v1, v2, "Same UUID must always resolve to same voice");

        // Result should be a valid Kokoro voice name
        let valid_ids: Vec<String> = kokoro
            .available_voices()
            .iter()
            .map(|v| v.id.clone())
            .collect();
        assert!(
            valid_ids.contains(&v1),
            "Resolved voice '{}' should be a valid Kokoro voice",
            v1
        );
    }

    #[test]
    fn test_resolve_voice_different_uuids_different_voices() {
        // Different UUIDs should (likely) produce different voices
        // Not guaranteed, but with 11 voices and good hash distribution, collision is unlikely
        let kokoro = KokoroTTS::new();
        let mut voices = std::collections::HashSet::new();

        let uuids = [
            "00000000-0000-0000-0000-000000000001",
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222",
            "33333333-3333-3333-3333-333333333333",
            "44444444-4444-4444-4444-444444444444",
            "55555555-5555-5555-5555-555555555555",
        ];

        for uuid in &uuids {
            voices.insert(kokoro.resolve_voice(uuid));
        }

        // With 6 UUIDs and 11 voices, we should get at least 3 distinct voices
        assert!(
            voices.len() >= 3,
            "Expected at least 3 distinct voices from 6 UUIDs, got {}: {:?}",
            voices.len(),
            voices
        );
    }
}
