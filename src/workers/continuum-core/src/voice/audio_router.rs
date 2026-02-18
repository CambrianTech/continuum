//! Audio Router
//!
//! Routes audio between participants based on their capabilities:
//! - Audio-native models receive raw audio streams
//! - Text-only models receive transcriptions
//! - TTS output is mixed back for audio-native models to hear
//!
//! This enables heterogeneous conversations where GPT-4o can hear
//! Claude's TTS output, and Claude gets transcriptions of GPT-4o's speech.

use super::capabilities::{AudioCapabilities, AudioRouting, InputRoute, ModelCapabilityRegistry, OutputRoute};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

/// Participant in a voice conversation with routing info
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutedParticipant {
    pub user_id: String,
    pub display_name: String,
    pub model_id: Option<String>,
    pub routing: AudioRouting,
    pub is_human: bool,
}

impl RoutedParticipant {
    /// Create a human participant (always needs audio output, provides audio input)
    pub fn human(user_id: String, display_name: String) -> Self {
        Self {
            user_id,
            display_name,
            model_id: None,
            routing: AudioRouting {
                model_id: "human".to_string(),
                capabilities: AudioCapabilities {
                    audio_input: true,  // Humans speak
                    audio_output: true, // Humans hear
                    realtime_streaming: true,
                    audio_perception: true,
                },
                input_route: InputRoute::RawAudio,
                output_route: OutputRoute::NativeAudio,
            },
            is_human: true,
        }
    }

    /// Create an AI participant with model-specific routing
    pub fn ai(
        user_id: String,
        display_name: String,
        model_id: &str,
        registry: &ModelCapabilityRegistry,
    ) -> Self {
        Self {
            user_id,
            display_name,
            model_id: Some(model_id.to_string()),
            routing: AudioRouting::for_model(model_id, registry),
            is_human: false,
        }
    }

    /// Check if this participant can hear raw audio
    pub fn can_hear_audio(&self) -> bool {
        self.routing.capabilities.audio_input
    }

    /// Check if this participant needs transcription
    pub fn needs_transcription(&self) -> bool {
        self.routing.capabilities.needs_stt()
    }

    /// Check if this participant needs TTS for their output
    pub fn needs_tts(&self) -> bool {
        self.routing.capabilities.needs_tts()
    }

    /// Check if this participant produces native audio
    pub fn produces_native_audio(&self) -> bool {
        self.routing.capabilities.audio_output && !self.is_human
    }
}

/// Events routed by the AudioRouter
#[derive(Debug, Clone)]
pub enum AudioEvent {
    /// Raw audio samples (for audio-capable participants)
    RawAudio {
        from_user_id: String,
        samples: Vec<f32>,
        sample_rate: u32,
    },

    /// Transcription (for text-only participants)
    Transcription {
        from_user_id: String,
        from_display_name: String,
        text: String,
        is_final: bool,
    },

    /// TTS audio to be mixed (when text model speaks)
    TTSAudio {
        from_user_id: String,
        from_display_name: String,
        text: String,
        samples: Vec<i16>,
        sample_rate: u32,
    },

    /// Native audio response from audio model
    NativeAudioResponse {
        from_user_id: String,
        from_display_name: String,
        samples: Vec<f32>,
        sample_rate: u32,
    },
}

/// Audio router for a voice conversation
pub struct AudioRouter {
    /// All participants in the conversation
    participants: RwLock<HashMap<String, RoutedParticipant>>,

    /// Model capability registry
    registry: Arc<ModelCapabilityRegistry>,

    /// Channel for audio events
    event_tx: broadcast::Sender<AudioEvent>,
}

impl AudioRouter {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1000);

        Self {
            participants: RwLock::new(HashMap::new()),
            registry: Arc::new(ModelCapabilityRegistry::new()),
            event_tx,
        }
    }

    /// Add a participant to the conversation
    pub async fn add_participant(&self, participant: RoutedParticipant) {
        let user_id = participant.user_id.clone();
        let caps = &participant.routing.capabilities;

        info!(
            "AudioRouter: Adding {} ({}) - audio_in:{}, audio_out:{}, needs_stt:{}, needs_tts:{}",
            participant.display_name,
            participant.model_id.as_deref().unwrap_or("human"),
            caps.audio_input,
            caps.audio_output,
            caps.needs_stt(),
            caps.needs_tts()
        );

        self.participants.write().await.insert(user_id, participant);
    }

    /// Remove a participant
    pub async fn remove_participant(&self, user_id: &str) {
        self.participants.write().await.remove(user_id);
    }

    /// Subscribe to routed audio events
    pub fn subscribe(&self) -> broadcast::Receiver<AudioEvent> {
        self.event_tx.subscribe()
    }

    /// Route incoming audio from a participant
    ///
    /// This handles:
    /// 1. Mixing raw audio for participants that can hear
    /// 2. Transcribing for participants that need text
    pub async fn route_audio(&self, from_user_id: &str, samples: Vec<f32>, sample_rate: u32) {
        let participants = self.participants.read().await;

        // Get the sender info
        let sender = match participants.get(from_user_id) {
            Some(p) => p,
            None => {
                warn!("AudioRouter: Unknown sender {}", from_user_id);
                return;
            }
        };

        let from_display_name = sender.display_name.clone();

        // Determine what events to emit based on who needs what
        let mut need_transcription = false;
        let mut need_raw_audio = false;

        for (user_id, participant) in participants.iter() {
            if user_id == from_user_id {
                continue; // Don't route to self
            }

            if participant.can_hear_audio() {
                need_raw_audio = true;
            }
            if participant.needs_transcription() {
                need_transcription = true;
            }
        }

        drop(participants); // Release lock before sending events

        // Emit raw audio event for audio-capable participants
        if need_raw_audio {
            let _ = self.event_tx.send(AudioEvent::RawAudio {
                from_user_id: from_user_id.to_string(),
                samples: samples.clone(),
                sample_rate,
            });
        }

        // Transcription will be handled by the caller (VAD + STT pipeline)
        // We just note that it's needed
        if need_transcription {
            debug!(
                "AudioRouter: Audio from {} needs transcription for text models",
                from_display_name
            );
        }
    }

    /// Route a transcription to participants that need it
    pub async fn route_transcription(
        &self,
        from_user_id: &str,
        from_display_name: &str,
        text: &str,
        is_final: bool,
    ) {
        let _ = self.event_tx.send(AudioEvent::Transcription {
            from_user_id: from_user_id.to_string(),
            from_display_name: from_display_name.to_string(),
            text: text.to_string(),
            is_final,
        });
    }

    /// Route TTS audio (when a text model speaks)
    ///
    /// This audio should be:
    /// 1. Mixed into the call for humans to hear
    /// 2. Sent to audio-native models so they can hear it too
    pub async fn route_tts_audio(
        &self,
        from_user_id: &str,
        from_display_name: &str,
        text: &str,
        samples: Vec<i16>,
        sample_rate: u32,
    ) {
        info!(
            "AudioRouter: TTS from {} ({} samples) - routing to audio-capable participants",
            from_display_name,
            samples.len()
        );

        let _ = self.event_tx.send(AudioEvent::TTSAudio {
            from_user_id: from_user_id.to_string(),
            from_display_name: from_display_name.to_string(),
            text: text.to_string(),
            samples,
            sample_rate,
        });
    }

    /// Route native audio response from an audio model (like GPT-4o)
    ///
    /// This audio should be:
    /// 1. Mixed into the call for humans to hear
    /// 2. Sent to other audio-native models
    /// 3. Transcribed for text-only models
    pub async fn route_native_audio_response(
        &self,
        from_user_id: &str,
        from_display_name: &str,
        samples: Vec<f32>,
        sample_rate: u32,
    ) {
        info!(
            "AudioRouter: Native audio from {} ({} samples) - routing + transcribing",
            from_display_name,
            samples.len()
        );

        let _ = self.event_tx.send(AudioEvent::NativeAudioResponse {
            from_user_id: from_user_id.to_string(),
            from_display_name: from_display_name.to_string(),
            samples,
            sample_rate,
        });

        // Note: Caller should also run STT on this audio for text-only participants
    }

    /// Get routing summary for debugging
    pub async fn get_routing_summary(&self) -> String {
        let participants = self.participants.read().await;
        let mut summary = String::from("AudioRouter participants:\n");

        for (_, p) in participants.iter() {
            let model = p.model_id.as_deref().unwrap_or("human");
            let input = if p.can_hear_audio() { "audio" } else { "text" };
            let output = if p.needs_tts() { "TTS" } else { "native" };

            summary.push_str(&format!(
                "  - {} ({}): input={}, output={}\n",
                p.display_name, model, input, output
            ));
        }

        summary
    }

    /// Get participants that need a specific type of input
    pub async fn get_participants_needing_audio(&self) -> Vec<String> {
        self.participants
            .read()
            .await
            .iter()
            .filter(|(_, p)| p.can_hear_audio())
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub async fn get_participants_needing_transcription(&self) -> Vec<String> {
        self.participants
            .read()
            .await
            .iter()
            .filter(|(_, p)| p.needs_transcription())
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Create routing for a model by ID
    pub fn create_routing(&self, model_id: &str) -> AudioRouting {
        AudioRouting::for_model(model_id, &self.registry)
    }
}

impl Default for AudioRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_heterogeneous_conversation() {
        let router = AudioRouter::new();

        // Add human
        router
            .add_participant(RoutedParticipant::human(
                "user-1".into(),
                "Joel".into(),
            ))
            .await;

        // Add GPT-4o (audio native)
        router
            .add_participant(RoutedParticipant::ai(
                "ai-1".into(),
                "GPT-4o".into(),
                "gpt-4o-realtime",
                &router.registry,
            ))
            .await;

        // Add Claude (text only)
        router
            .add_participant(RoutedParticipant::ai(
                "ai-2".into(),
                "Claude".into(),
                "claude-3-sonnet",
                &router.registry,
            ))
            .await;

        let summary = router.get_routing_summary().await;
        println!("{}", summary);

        // Check routing
        let audio_receivers = router.get_participants_needing_audio().await;
        let text_receivers = router.get_participants_needing_transcription().await;

        // Human and GPT-4o should receive audio
        assert!(audio_receivers.contains(&"user-1".to_string()));
        assert!(audio_receivers.contains(&"ai-1".to_string()));

        // Claude should receive transcription
        assert!(text_receivers.contains(&"ai-2".to_string()));
    }
}
