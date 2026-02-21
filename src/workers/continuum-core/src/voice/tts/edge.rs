//! Edge-TTS Adapter
//!
//! Uses Microsoft Edge's free Read Aloud API for text-to-speech.
//! No API key required. Requires internet connection.
//! Requests raw PCM directly (no MP3 decoding needed).
//!
//! Latency: <200ms for short sentences (network-bound).
//! Quality: High (Microsoft Neural voices).
//! Voices: 300+ voices across 100+ languages.

use super::audio_utils;
use super::{SynthesisResult, TTSError, TextToSpeech, VoiceInfo};
use crate::audio_constants::AUDIO_SAMPLE_RATE;
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use tracing::{info, warn};

/// Cached voice list (fetched once on init)
struct EdgeState {
    voices: Vec<msedge_tts::voice::Voice>,
}

/// Edge-TTS Adapter
///
/// Online TTS using Microsoft Edge's neural voices.
/// Zero model files — all inference happens server-side.
/// Perfect fallback when local models are unavailable or too slow.
pub struct EdgeTTS {
    initialized: AtomicBool,
    state: RwLock<Option<EdgeState>>,
}

impl EdgeTTS {
    pub fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            state: RwLock::new(None),
        }
    }

    /// Convert raw PCM bytes (i16 LE) to Vec<i16>
    fn pcm_bytes_to_i16(bytes: &[u8]) -> Vec<i16> {
        bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect()
    }
}

#[async_trait]
impl TextToSpeech for EdgeTTS {
    fn name(&self) -> &'static str {
        "edge"
    }

    fn description(&self) -> &'static str {
        "Microsoft Edge neural TTS (online, free, 300+ voices)"
    }

    fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Relaxed)
    }

    async fn initialize(&self) -> Result<(), TTSError> {
        if self.is_initialized() {
            return Ok(());
        }

        // IMPORTANT: We intentionally DO NOT call msedge_tts::voice::get_voices_list() here.
        // That function uses isahc→curl→openssl-sys, which causes a SIGSEGV due to symbol
        // conflicts with LiveKit's bundled BoringSSL (both link OpenSSL into the same binary).
        // The synthesis path (msedge_tts::tts::client::connect) uses tungstenite WebSocket
        // which does NOT go through the conflicting OpenSSL path, so synthesis still works.
        info!("Edge-TTS: Initializing with known voice catalog (skipping HTTP voice list)");

        let mut state = self.state.write().map_err(|e| {
            TTSError::SynthesisFailed(format!("Failed to acquire state lock: {e}"))
        })?;
        // Use empty list — available_voices() returns hardcoded English voices as fallback
        *state = Some(EdgeState { voices: vec![] });

        self.initialized.store(true, Ordering::Relaxed);
        Ok(())
    }

    async fn synthesize(&self, text: &str, voice: &str) -> Result<SynthesisResult, TTSError> {
        if !self.is_initialized() {
            return Err(TTSError::ModelNotLoaded("Edge-TTS not initialized".into()));
        }

        if text.is_empty() {
            return Err(TTSError::InvalidText("Empty text".into()));
        }

        // Resolve voice name to Edge voice ID
        let voice_name = if voice == "default" || voice.is_empty() {
            "en-US-JennyNeural".to_string()
        } else if voice.contains('-') && voice.contains("Neural") {
            // Already a full Edge voice name
            voice.to_string()
        } else {
            // Try to match by short name from our voice list
            let state = self.state.read().map_err(|e| {
                TTSError::SynthesisFailed(format!("Failed to read state: {e}"))
            })?;
            let edge_state = state
                .as_ref()
                .ok_or_else(|| TTSError::ModelNotLoaded("Edge state not initialized".into()))?;

            edge_state
                .voices
                .iter()
                .find(|v| {
                    v.short_name
                        .as_deref()
                        .unwrap_or(&v.name)
                        .to_lowercase()
                        .contains(&voice.to_lowercase())
                })
                .map(|v| v.short_name.clone().unwrap_or_else(|| v.name.clone()))
                .unwrap_or_else(|| "en-US-JennyNeural".to_string())
        };

        info!(
            "Edge-TTS: Synthesizing with voice '{}': '{}'",
            voice_name,
            super::truncate_str(text, 50)
        );

        // msedge_tts uses blocking WebSocket (tungstenite). MUST run on spawn_blocking
        // to avoid deadlocking the tokio runtime (commands dispatch via rt_handle.spawn).
        // Wrapped in 15s timeout to prevent indefinite hangs on network issues.
        let text = text.to_string();
        let voice_name_owned = voice_name;

        tokio::time::timeout(
            std::time::Duration::from_secs(15),
            tokio::task::spawn_blocking(move || {
            let start = std::time::Instant::now();

            // Retry up to 2 times — Edge-TTS WebSocket can return empty audio on first try
            let max_attempts = 2;
            for attempt in 1..=max_attempts {
                let mut tts = msedge_tts::tts::client::connect()
                    .map_err(|e| TTSError::SynthesisFailed(format!("Edge-TTS connection failed: {e}")))?;

                let config = msedge_tts::tts::SpeechConfig {
                    voice_name: voice_name_owned.clone(),
                    audio_format: "raw-16khz-16bit-mono-pcm".to_string(),
                    pitch: 0,
                    rate: 0,
                    volume: 0,
                };

                let audio = tts
                    .synthesize(&text, &config)
                    .map_err(|e| TTSError::SynthesisFailed(format!("Edge-TTS synthesis failed: {e}")))?;

                let network_ms = start.elapsed().as_millis();
                let samples = Self::pcm_bytes_to_i16(&audio.audio_bytes);

                if samples.is_empty() {
                    warn!(
                        "Edge-TTS: empty audio on attempt {}/{} (audio_bytes={} bytes, metadata_count={})",
                        attempt, max_attempts, audio.audio_bytes.len(), audio.audio_metadata.len()
                    );
                    if attempt < max_attempts {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        continue;
                    }
                    return Err(TTSError::SynthesisFailed(
                        format!("Edge-TTS returned empty audio after {} attempts (raw bytes={})",
                            max_attempts, audio.audio_bytes.len()),
                    ));
                }

                let dur = audio_utils::duration_ms(samples.len(), AUDIO_SAMPLE_RATE);
                info!(
                    "Edge-TTS: {} samples ({}ms audio) in {}ms network",
                    samples.len(), dur, network_ms
                );

                return Ok(SynthesisResult {
                    samples,
                    sample_rate: AUDIO_SAMPLE_RATE,
                    duration_ms: dur,
                });
            }
            unreachable!()
        })
        )
        .await
        .map_err(|_| TTSError::SynthesisFailed("Edge-TTS timed out after 15s".into()))?
        .map_err(|e| TTSError::SynthesisFailed(format!("Edge-TTS task join: {e}")))?
    }

    fn available_voices(&self) -> Vec<VoiceInfo> {
        // Hardcoded catalog of commonly-used English Edge neural voices.
        // We cannot fetch the full list at runtime because msedge_tts::voice::get_voices_list()
        // uses isahc→curl→openssl-sys which SIGSEGVs when linked alongside LiveKit's BoringSSL.
        // This catalog covers the primary English voices; synthesis works with ANY valid Edge
        // voice name (e.g., "en-GB-SoniaNeural") even if it's not listed here.
        static KNOWN_VOICES: &[(&str, &str, &str)] = &[
            ("en-US-JennyNeural", "female", "en-US"),
            ("en-US-GuyNeural", "male", "en-US"),
            ("en-US-AriaNeural", "female", "en-US"),
            ("en-US-DavisNeural", "male", "en-US"),
            ("en-US-AmberNeural", "female", "en-US"),
            ("en-US-AnaNeural", "female", "en-US"),
            ("en-US-AndrewNeural", "male", "en-US"),
            ("en-US-EmmaNeural", "female", "en-US"),
            ("en-US-BrianNeural", "male", "en-US"),
            ("en-US-ChristopherNeural", "male", "en-US"),
            ("en-US-EricNeural", "male", "en-US"),
            ("en-US-MichelleNeural", "female", "en-US"),
            ("en-US-RogerNeural", "male", "en-US"),
            ("en-US-SteffanNeural", "male", "en-US"),
            ("en-GB-SoniaNeural", "female", "en-GB"),
            ("en-GB-RyanNeural", "male", "en-GB"),
            ("en-AU-NatashaNeural", "female", "en-AU"),
            ("en-AU-WilliamNeural", "male", "en-AU"),
            ("en-CA-ClaraNeural", "female", "en-CA"),
            ("en-CA-LiamNeural", "male", "en-CA"),
        ];

        KNOWN_VOICES
            .iter()
            .map(|(id, gender, locale)| VoiceInfo {
                id: id.to_string(),
                name: id.to_string(),
                language: locale.to_string(),
                gender: Some(gender.to_string()),
                description: Some(format!("Microsoft Edge neural voice ({locale})")),
            })
            .collect()
    }

    fn default_voice(&self) -> &str {
        "en-US-JennyNeural"
    }
}

impl Default for EdgeTTS {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_tts_basics() {
        let adapter = EdgeTTS::new();
        assert_eq!(adapter.name(), "edge");
        assert!(!adapter.is_initialized());
        assert_eq!(adapter.default_voice(), "en-US-JennyNeural");
    }

    #[test]
    fn test_pcm_bytes_to_i16() {
        // i16 LE: 0x0100 = 256, 0xFF7F = 32767
        let bytes = vec![0x00, 0x01, 0xFF, 0x7F];
        let samples = EdgeTTS::pcm_bytes_to_i16(&bytes);
        assert_eq!(samples, vec![256, 32767]);
    }

    #[test]
    fn test_pcm_bytes_to_i16_empty() {
        let samples = EdgeTTS::pcm_bytes_to_i16(&[]);
        assert!(samples.is_empty());
    }

    #[test]
    fn test_pcm_bytes_to_i16_odd_byte_count() {
        // Odd number of bytes — last byte should be dropped by chunks_exact
        let bytes = vec![0x00, 0x01, 0xFF];
        let samples = EdgeTTS::pcm_bytes_to_i16(&bytes);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0], 256);
    }

    #[test]
    fn test_available_voices_before_init() {
        let adapter = EdgeTTS::new();
        let voices = adapter.available_voices();
        // Before init, should return at least the default voice
        assert!(!voices.is_empty());
        assert!(voices.iter().any(|v| v.id == "en-US-JennyNeural"));
    }

    /// Integration test — requires internet connection
    #[test]
    #[ignore]
    fn test_edge_tts_synthesize_live() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let adapter = EdgeTTS::new();

        rt.block_on(async {
            adapter.initialize().await.expect("Init should succeed");
        });

        assert!(adapter.is_initialized());

        let voices = adapter.available_voices();
        assert!(voices.len() > 10, "Should have many English voices");

        let result = rt.block_on(async {
            adapter
                .synthesize("Hello world, this is Edge TTS.", "default")
                .await
        });

        let synthesis = result.expect("Synthesis should succeed");
        assert!(synthesis.samples.len() > 1000, "Should produce audio");
        assert_eq!(synthesis.sample_rate, AUDIO_SAMPLE_RATE);
        assert!(
            synthesis.duration_ms > 200,
            "Should be >200ms for a sentence"
        );

        // Audio should not be silence
        let max_amp = synthesis.samples.iter().map(|s| s.abs()).max().unwrap_or(0);
        assert!(
            max_amp > 100,
            "Audio should not be silence, max amp: {}",
            max_amp
        );

        println!(
            "Edge-TTS: {} samples, {}Hz, {}ms, max amp: {}",
            synthesis.samples.len(),
            synthesis.sample_rate,
            synthesis.duration_ms,
            max_amp
        );
    }
}
