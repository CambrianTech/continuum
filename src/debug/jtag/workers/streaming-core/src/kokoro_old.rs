//! Kokoro TTS Inference via ONNX Runtime
//!
//! Pure Rust implementation of Kokoro TTS using ONNX Runtime.
//! No Python bridges, no subprocess calls.
//!
//! # Architecture
//!
//! Kokoro is a lightweight (~82M params) TTS model with excellent quality.
//! Uses StyleTTS2-based architecture with iSTFTNet vocoder.
//!
//! Model: https://huggingface.co/hexgrad/Kokoro-82M
//!
//! # Usage
//!
//! ```rust,ignore
//! use streaming_core::kokoro;
//!
//! // Initialize (loads model)
//! kokoro::init_kokoro(None)?;
//!
//! // Synthesize
//! let audio = kokoro::synthesize("Hello world", "af", 1.0).await?;
//! ```

use ndarray;
use once_cell::sync::OnceCell;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tracing::{error, info, warn};

/// Global Kokoro session (loaded once, reused)
static KOKORO_SESSION: OnceCell<Arc<Mutex<KokoroModel>>> = OnceCell::new();

/// Kokoro model wrapper
struct KokoroModel {
    session: Session,
    sample_rate: u32,
}

/// Kokoro errors
#[derive(Error, Debug)]
pub enum KokoroError {
    #[error("Model not loaded: {0}")]
    ModelNotLoaded(String),

    #[error("Inference failed: {0}")]
    InferenceFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Speaker not found: {0}")]
    SpeakerNotFound(String),

    #[error("ONNX Runtime error: {0}")]
    OrtError(String),
}

impl From<ort::Error> for KokoroError {
    fn from(e: ort::Error) -> Self {
        KokoroError::OrtError(e.to_string())
    }
}

/// Available Kokoro voices
pub const KOKORO_VOICES: &[(&str, &str)] = &[
    ("af", "American Female (default)"),
    ("af_bella", "American Female - Bella"),
    ("af_nicole", "American Female - Nicole"),
    ("af_sarah", "American Female - Sarah"),
    ("af_sky", "American Female - Sky"),
    ("am_adam", "American Male - Adam"),
    ("am_michael", "American Male - Michael"),
    ("bf_emma", "British Female - Emma"),
    ("bf_isabella", "British Female - Isabella"),
    ("bm_george", "British Male - George"),
    ("bm_lewis", "British Male - Lewis"),
];

/// Get voice ID from name (returns default if not found)
pub fn normalize_voice(voice: Option<&str>) -> &'static str {
    match voice {
        Some(v) => {
            for (id, _) in KOKORO_VOICES {
                if *id == v {
                    return id;
                }
            }
            "af" // Default to American Female
        }
        None => "af",
    }
}

/// Find Kokoro model path
fn find_model_path(custom_path: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = custom_path {
        if path.exists() {
            return Some(path);
        }
    }

    // Check common locations
    let candidates = [
        PathBuf::from("models/kokoro/kokoro-v0_19.onnx"),
        PathBuf::from("models/kokoro/kokoro.onnx"),
        PathBuf::from("models/tts/kokoro.onnx"),
        dirs::data_dir()
            .unwrap_or_default()
            .join("kokoro/kokoro-v0_19.onnx"),
        PathBuf::from("/usr/local/share/kokoro/kokoro.onnx"),
    ];

    for path in candidates {
        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Initialize Kokoro TTS model (call once at startup)
pub fn init_kokoro(model_path: Option<PathBuf>) -> Result<(), KokoroError> {
    if KOKORO_SESSION.get().is_some() {
        info!("Kokoro already initialized");
        return Ok(());
    }

    let model_path = match find_model_path(model_path) {
        Some(path) => path,
        None => {
            warn!("Kokoro model not found. Download from:");
            warn!("  https://huggingface.co/hexgrad/Kokoro-82M/tree/main");
            warn!("Place ONNX file in: models/kokoro/kokoro-v0_19.onnx");
            return Err(KokoroError::ModelNotLoaded(
                "Kokoro ONNX model not found. See logs for download instructions.".into(),
            ));
        }
    };

    info!("Loading Kokoro model from: {:?}", model_path);

    // Initialize ONNX Runtime session
    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(num_cpus::get().min(4))?
        .commit_from_file(&model_path)?;

    info!("Kokoro model loaded successfully");

    let model = KokoroModel {
        session,
        sample_rate: 24000, // Kokoro outputs 24kHz audio
    };

    KOKORO_SESSION
        .set(Arc::new(Mutex::new(model)))
        .map_err(|_| KokoroError::ModelNotLoaded("Failed to set global session".into()))?;

    Ok(())
}

/// Check if Kokoro is initialized
pub fn is_kokoro_initialized() -> bool {
    KOKORO_SESSION.get().is_some()
}

/// Get Kokoro sample rate
pub fn sample_rate() -> u32 {
    24000
}

/// Synthesize text to audio (async wrapper, runs on blocking thread)
///
/// # Arguments
/// * `text` - Text to synthesize
/// * `voice` - Voice ID (e.g., "af", "am_adam")
/// * `speed` - Speed multiplier (1.0 = normal)
///
/// # Returns
/// Audio samples as i16 PCM, 24kHz mono
pub async fn synthesize(
    text: String,
    voice: Option<String>,
    speed: f32,
) -> Result<Vec<i16>, KokoroError> {
    let session = KOKORO_SESSION
        .get()
        .ok_or_else(|| KokoroError::ModelNotLoaded("Kokoro not initialized".into()))?
        .clone();

    // Run on blocking thread pool
    tokio::task::spawn_blocking(move || synthesize_sync(&session, &text, voice.as_deref(), speed))
        .await
        .map_err(|e| KokoroError::InferenceFailed(format!("Task join error: {}", e)))?
}

/// Synchronous synthesis (runs on blocking thread)
fn synthesize_sync(
    session: &Arc<Mutex<KokoroModel>>,
    text: &str,
    voice: Option<&str>,
    speed: f32,
) -> Result<Vec<i16>, KokoroError> {
    if text.is_empty() {
        return Err(KokoroError::InvalidInput("Text cannot be empty".into()));
    }

    let voice_id = normalize_voice(voice);
    let model = session.lock();

    // Prepare inputs for Kokoro ONNX model
    // Kokoro expects: text (string), voice (string), speed (f32)
    //
    // Note: The actual input format depends on which ONNX export you're using.
    // Some exports expect phonemes, others raw text.
    // This implementation assumes a text-input ONNX export.

    // Convert text to input tensor using ndarray
    let text_tokens = tokenize_for_kokoro(text);
    let text_array = ndarray::Array1::from_vec(text_tokens);

    // Voice embedding (simplified - real implementation would load voice embeddings)
    let voice_embedding = get_voice_embedding(voice_id);
    let voice_array = ndarray::Array1::from_vec(voice_embedding);

    // Speed tensor
    let speed_array = ndarray::Array1::from_vec(vec![speed]);

    // Run inference using ort v2 API
    let outputs = model
        .session
        .run(ort::inputs![
            "tokens" => text_array,
            "voice" => voice_array,
            "speed" => speed_array
        ]?)
        .map_err(|e| KokoroError::InferenceFailed(format!("ONNX inference failed: {}", e)))?;

    // Extract audio output (typically the first output named "audio" or index 0)
    let audio_output = outputs.iter().next()
        .ok_or_else(|| KokoroError::InferenceFailed("No audio output from model".into()))?
        .1;

    // Convert float output to i16 samples
    let (_, audio_data) = audio_output
        .try_extract_raw_tensor::<f32>()
        .map_err(|e| KokoroError::InferenceFailed(format!("Failed to extract audio: {}", e)))?;

    // Convert f32 [-1, 1] to i16
    let samples: Vec<i16> = audio_data
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect();

    info!(
        "Kokoro synthesized {} samples ({}ms) for '{}...'",
        samples.len(),
        samples.len() as f32 / model.sample_rate as f32 * 1000.0,
        &text[..text.len().min(30)]
    );

    Ok(samples)
}

/// Tokenize text for Kokoro model input
///
/// This is a simplified tokenization. Real implementation would use:
/// - Phoneme conversion (g2p)
/// - Proper text normalization
/// - Model-specific vocabulary
fn tokenize_for_kokoro(text: &str) -> Vec<i64> {
    // Simple character-level tokenization for now
    // Real Kokoro uses phoneme tokens
    text.chars()
        .filter_map(|c| {
            if c.is_ascii() {
                Some(c as i64)
            } else {
                None
            }
        })
        .collect()
}

/// Get voice embedding for a voice ID
///
/// Real implementation would load pre-computed embeddings from disk.
/// Each voice has a ~256-dim embedding that controls speaker characteristics.
fn get_voice_embedding(voice_id: &str) -> Vec<f32> {
    // Placeholder - return different "embeddings" based on voice ID
    // Real embeddings would be loaded from a .npy or .bin file
    let seed = voice_id.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
    let mut embedding = vec![0.0f32; 256];

    // Generate deterministic "random" embedding based on voice ID
    for (i, val) in embedding.iter_mut().enumerate() {
        *val = ((seed.wrapping_mul(i as u32 + 1) % 1000) as f32 / 1000.0) * 2.0 - 1.0;
    }

    embedding
}

/// Streaming synthesis result
pub struct KokoroStreamChunk {
    pub samples: Vec<i16>,
    pub is_final: bool,
}

/// Synthesize with streaming output
///
/// Yields audio chunks as they're generated.
/// Useful for real-time TTS where you want to start playback ASAP.
pub async fn synthesize_stream(
    text: String,
    voice: Option<String>,
    speed: f32,
) -> Result<tokio::sync::mpsc::Receiver<Result<KokoroStreamChunk, KokoroError>>, KokoroError> {
    let (tx, rx) = tokio::sync::mpsc::channel(32);

    // For now, generate all audio then chunk it
    // Real streaming would generate chunk-by-chunk
    tokio::spawn(async move {
        match synthesize(text, voice, speed).await {
            Ok(samples) => {
                // Chunk the audio into ~20ms frames
                let chunk_size = (24000 * 20) / 1000; // 480 samples = 20ms at 24kHz

                for (i, chunk_samples) in samples.chunks(chunk_size).enumerate() {
                    let is_final = (i + 1) * chunk_size >= samples.len();

                    let chunk = KokoroStreamChunk {
                        samples: chunk_samples.to_vec(),
                        is_final,
                    };

                    if tx.send(Ok(chunk)).await.is_err() {
                        break; // Receiver dropped
                    }

                    if is_final {
                        break;
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(Err(e)).await;
            }
        }
    });

    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_voice() {
        assert_eq!(normalize_voice(None), "af");
        assert_eq!(normalize_voice(Some("af")), "af");
        assert_eq!(normalize_voice(Some("am_adam")), "am_adam");
        assert_eq!(normalize_voice(Some("invalid")), "af"); // Falls back to default
    }

    #[test]
    fn test_tokenize() {
        let tokens = tokenize_for_kokoro("Hello");
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0], 'H' as i64);
    }

    #[test]
    fn test_voice_embedding() {
        let emb1 = get_voice_embedding("af");
        let emb2 = get_voice_embedding("am_adam");

        assert_eq!(emb1.len(), 256);
        assert_eq!(emb2.len(), 256);

        // Different voices should have different embeddings
        assert_ne!(emb1, emb2);

        // Same voice should have same embedding
        let emb1_again = get_voice_embedding("af");
        assert_eq!(emb1, emb1_again);
    }
}
