//! Voice Model Capabilities
//!
//! Defines what each AI model can do with audio:
//! - Audio input (can hear raw audio)
//! - Audio output (can generate audio directly)
//! - Text only (needs STT/TTS pipeline)
//!
//! This enables heterogeneous conversations where audio-native models
//! and text-based models can seamlessly interact.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Audio capabilities for a model
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioCapabilities {
    /// Can process raw audio input (hear)
    pub audio_input: bool,
    /// Can generate raw audio output (speak natively)
    pub audio_output: bool,
    /// Supports real-time streaming (low latency)
    pub realtime_streaming: bool,
    /// Can detect tone, emotion, non-speech sounds
    pub audio_perception: bool,
}

impl AudioCapabilities {
    /// Full audio-native model (GPT-4o, Gemini 2.0)
    pub const AUDIO_NATIVE: Self = Self {
        audio_input: true,
        audio_output: true,
        realtime_streaming: true,
        audio_perception: true,
    };

    /// Text-only model (Claude, most Ollama models)
    pub const TEXT_ONLY: Self = Self {
        audio_input: false,
        audio_output: false,
        realtime_streaming: false,
        audio_perception: false,
    };

    /// Audio input only (can hear but responds in text)
    pub const AUDIO_INPUT_ONLY: Self = Self {
        audio_input: true,
        audio_output: false,
        realtime_streaming: false,
        audio_perception: true,
    };

    /// Check if model needs STT for input
    pub fn needs_stt(&self) -> bool {
        !self.audio_input
    }

    /// Check if model needs TTS for output
    pub fn needs_tts(&self) -> bool {
        !self.audio_output
    }

    /// Check if this is a fully audio-native model
    pub fn is_audio_native(&self) -> bool {
        self.audio_input && self.audio_output
    }
}

impl Default for AudioCapabilities {
    fn default() -> Self {
        Self::TEXT_ONLY
    }
}

/// Known model capabilities registry
/// Maps model identifiers to their audio capabilities
pub struct ModelCapabilityRegistry {
    capabilities: HashMap<String, AudioCapabilities>,
}

impl ModelCapabilityRegistry {
    pub fn new() -> Self {
        let mut capabilities = HashMap::new();

        // OpenAI models
        capabilities.insert("gpt-4o".into(), AudioCapabilities::AUDIO_NATIVE);
        capabilities.insert("gpt-4o-realtime".into(), AudioCapabilities::AUDIO_NATIVE);
        capabilities.insert("gpt-4o-realtime-preview".into(), AudioCapabilities::AUDIO_NATIVE);
        capabilities.insert("gpt-4o-mini-realtime".into(), AudioCapabilities::AUDIO_NATIVE);
        capabilities.insert("gpt-4".into(), AudioCapabilities::TEXT_ONLY);
        capabilities.insert("gpt-4-turbo".into(), AudioCapabilities::TEXT_ONLY);
        capabilities.insert("gpt-3.5-turbo".into(), AudioCapabilities::TEXT_ONLY);

        // Google models
        capabilities.insert("gemini-2.5-flash-native-audio-preview".into(), AudioCapabilities::AUDIO_NATIVE);
        capabilities.insert("gemini-2.5-flash".into(), AudioCapabilities::AUDIO_NATIVE);
        capabilities.insert("gemini-live".into(), AudioCapabilities::AUDIO_NATIVE);
        capabilities.insert("gemini-2.0-flash".into(), AudioCapabilities::AUDIO_NATIVE);
        capabilities.insert("gemini-2.0-flash-exp".into(), AudioCapabilities::AUDIO_NATIVE);
        capabilities.insert("gemini-1.5-pro".into(), AudioCapabilities::AUDIO_INPUT_ONLY);
        capabilities.insert("gemini-1.5-flash".into(), AudioCapabilities::AUDIO_INPUT_ONLY);
        capabilities.insert("gemini-pro".into(), AudioCapabilities::TEXT_ONLY);

        // Anthropic models (text only for now)
        capabilities.insert("claude-3-opus".into(), AudioCapabilities::TEXT_ONLY);
        capabilities.insert("claude-3-sonnet".into(), AudioCapabilities::TEXT_ONLY);
        capabilities.insert("claude-3-haiku".into(), AudioCapabilities::TEXT_ONLY);
        capabilities.insert("claude-3.5-sonnet".into(), AudioCapabilities::TEXT_ONLY);

        // Local/Ollama models (text only)
        capabilities.insert("llama3".into(), AudioCapabilities::TEXT_ONLY);
        capabilities.insert("llama3.1".into(), AudioCapabilities::TEXT_ONLY);
        capabilities.insert("llama3.2".into(), AudioCapabilities::TEXT_ONLY);
        capabilities.insert("mistral".into(), AudioCapabilities::TEXT_ONLY);
        capabilities.insert("mixtral".into(), AudioCapabilities::TEXT_ONLY);
        capabilities.insert("codellama".into(), AudioCapabilities::TEXT_ONLY);
        capabilities.insert("deepseek-coder".into(), AudioCapabilities::TEXT_ONLY);
        capabilities.insert("qwen".into(), AudioCapabilities::TEXT_ONLY);

        // Groq (fast inference, text only)
        capabilities.insert("groq-llama3".into(), AudioCapabilities::TEXT_ONLY);
        capabilities.insert("groq-mixtral".into(), AudioCapabilities::TEXT_ONLY);

        // Alibaba Qwen3-Omni (audio native, open source)
        capabilities.insert("qwen3-omni".into(), AudioCapabilities::AUDIO_NATIVE);
        capabilities.insert("qwen3-omni-flash".into(), AudioCapabilities::AUDIO_NATIVE);
        capabilities.insert("qwen3-omni-flash-realtime".into(), AudioCapabilities::AUDIO_NATIVE);
        capabilities.insert("qwen3-omni-30b".into(), AudioCapabilities::AUDIO_NATIVE);

        // Amazon Nova Sonic (audio native)
        capabilities.insert("nova-sonic".into(), AudioCapabilities::AUDIO_NATIVE);
        capabilities.insert("amazon-nova-sonic".into(), AudioCapabilities::AUDIO_NATIVE);

        // Hume EVI (audio native with emotion)
        capabilities.insert("hume-evi".into(), AudioCapabilities::AUDIO_NATIVE);

        Self { capabilities }
    }

    /// Get capabilities for a model
    /// Returns TEXT_ONLY for unknown models (safe default)
    pub fn get(&self, model_id: &str) -> AudioCapabilities {
        // Try exact match first
        if let Some(caps) = self.capabilities.get(model_id) {
            return *caps;
        }

        // Try prefix matching for versioned models
        for (key, caps) in &self.capabilities {
            if model_id.starts_with(key) || key.starts_with(model_id) {
                return *caps;
            }
        }

        // Unknown model - assume text only (safest)
        AudioCapabilities::TEXT_ONLY
    }

    /// Register custom model capabilities
    pub fn register(&mut self, model_id: String, capabilities: AudioCapabilities) {
        self.capabilities.insert(model_id, capabilities);
    }

    /// List all audio-native models
    pub fn list_audio_native(&self) -> Vec<&str> {
        self.capabilities
            .iter()
            .filter(|(_, caps)| caps.is_audio_native())
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// List all models that can hear audio (input)
    pub fn list_audio_input(&self) -> Vec<&str> {
        self.capabilities
            .iter()
            .filter(|(_, caps)| caps.audio_input)
            .map(|(id, _)| id.as_str())
            .collect()
    }
}

impl Default for ModelCapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Determine the optimal audio routing for a participant
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioRouting {
    /// Model being used
    pub model_id: String,
    /// Model's capabilities
    pub capabilities: AudioCapabilities,
    /// Route for input (what the model receives)
    pub input_route: InputRoute,
    /// Route for output (how model response is delivered)
    pub output_route: OutputRoute,
}

/// How audio reaches the model
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InputRoute {
    /// Raw audio stream (for audio-native models)
    RawAudio,
    /// Transcription via STT (for text models)
    Transcription { adapter: String },
}

/// How model output becomes audio
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OutputRoute {
    /// Model generates audio directly
    NativeAudio,
    /// Text response converted via TTS
    TextToSpeech { adapter: String },
}

impl AudioRouting {
    /// Create routing for a model
    pub fn for_model(model_id: &str, registry: &ModelCapabilityRegistry) -> Self {
        let capabilities = registry.get(model_id);

        let input_route = if capabilities.audio_input {
            InputRoute::RawAudio
        } else {
            InputRoute::Transcription {
                adapter: "whisper".into(), // Default, can be overridden
            }
        };

        let output_route = if capabilities.audio_output {
            OutputRoute::NativeAudio
        } else {
            OutputRoute::TextToSpeech {
                adapter: "piper".into(), // Default local TTS
            }
        };

        Self {
            model_id: model_id.to_string(),
            capabilities,
            input_route,
            output_route,
        }
    }

    /// Check if this routing needs the audio mixed for the model to hear
    pub fn needs_mixed_audio(&self) -> bool {
        self.capabilities.audio_input
    }

    /// Check if this routing needs TTS output routed to audio-native models
    pub fn tts_should_be_audible(&self) -> bool {
        // Text models produce TTS that should be heard by audio-native models
        !self.capabilities.audio_output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_defaults() {
        let registry = ModelCapabilityRegistry::new();

        // GPT-4o should be audio native
        let gpt4o = registry.get("gpt-4o-realtime-preview");
        assert!(gpt4o.is_audio_native());
        assert!(!gpt4o.needs_stt());
        assert!(!gpt4o.needs_tts());

        // Claude should be text only
        let claude = registry.get("claude-3-sonnet");
        assert!(!claude.is_audio_native());
        assert!(claude.needs_stt());
        assert!(claude.needs_tts());

        // Unknown model should be text only (safe default)
        let unknown = registry.get("some-unknown-model");
        assert!(unknown.needs_stt());
        assert!(unknown.needs_tts());
    }

    #[test]
    fn test_audio_routing() {
        let registry = ModelCapabilityRegistry::new();

        // GPT-4o gets raw audio, outputs native audio
        let gpt4o_routing = AudioRouting::for_model("gpt-4o", &registry);
        assert_eq!(gpt4o_routing.input_route, InputRoute::RawAudio);
        assert_eq!(gpt4o_routing.output_route, OutputRoute::NativeAudio);
        assert!(gpt4o_routing.needs_mixed_audio());

        // Claude gets transcription, outputs via TTS
        let claude_routing = AudioRouting::for_model("claude-3-sonnet", &registry);
        assert!(matches!(claude_routing.input_route, InputRoute::Transcription { .. }));
        assert!(matches!(claude_routing.output_route, OutputRoute::TextToSpeech { .. }));
        assert!(!claude_routing.needs_mixed_audio());
        assert!(claude_routing.tts_should_be_audible());
    }

    #[test]
    fn test_gemini_audio_input_only() {
        let registry = ModelCapabilityRegistry::new();

        // Gemini 1.5 can hear but outputs text
        let gemini = registry.get("gemini-1.5-pro");
        assert!(gemini.audio_input);
        assert!(!gemini.audio_output);
        assert!(!gemini.needs_stt()); // Can hear directly
        assert!(gemini.needs_tts());  // But needs TTS for output

        let routing = AudioRouting::for_model("gemini-1.5-pro", &registry);
        assert_eq!(routing.input_route, InputRoute::RawAudio);
        assert!(matches!(routing.output_route, OutputRoute::TextToSpeech { .. }));
    }

    #[test]
    fn test_qwen3_omni_audio_native() {
        let registry = ModelCapabilityRegistry::new();

        // Qwen3-Omni is fully audio native (open source)
        let qwen = registry.get("qwen3-omni-flash-realtime");
        assert!(qwen.is_audio_native());
        assert!(!qwen.needs_stt()); // Hears raw audio
        assert!(!qwen.needs_tts()); // Speaks raw audio

        // Routing should be raw audio in, native audio out
        let routing = AudioRouting::for_model("qwen3-omni", &registry);
        assert_eq!(routing.input_route, InputRoute::RawAudio);
        assert_eq!(routing.output_route, OutputRoute::NativeAudio);
        assert!(routing.needs_mixed_audio());
        assert!(!routing.tts_should_be_audible()); // Produces native audio, not TTS
    }

    #[test]
    fn test_nova_sonic_audio_native() {
        let registry = ModelCapabilityRegistry::new();

        // Amazon Nova Sonic is audio native
        let nova = registry.get("nova-sonic");
        assert!(nova.is_audio_native());
        assert!(!nova.needs_stt());
        assert!(!nova.needs_tts());
    }
}
