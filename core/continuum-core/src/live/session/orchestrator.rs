use crate::clog_info;
use crate::live::types::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct VoiceOrchestrator {
    session_participants: Arc<Mutex<HashMap<Uuid, Vec<VoiceParticipant>>>>,
    session_contexts: Arc<Mutex<HashMap<Uuid, ConversationContext>>>,
}

impl Default for VoiceOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceOrchestrator {
    pub fn new() -> Self {
        Self {
            session_participants: Arc::new(Mutex::new(HashMap::new())),
            session_contexts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn register_session(
        &self,
        session_id: Uuid,
        room_id: Uuid,
        participants: Vec<VoiceParticipant>,
    ) {
        {
            let mut sessions = self
                .session_participants
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            sessions.insert(session_id, participants.clone());
        }
        {
            let mut contexts = self
                .session_contexts
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            contexts.insert(session_id, ConversationContext::new(session_id, room_id));
        }
        let audio_native_count = participants.iter().filter(|p| p.is_audio_native).count();
        let text_ai_count = participants
            .iter()
            .filter(|p| matches!(p.participant_type, SpeakerType::Persona) && !p.is_audio_native)
            .count();
        clog_info!(
            "Registered session {} with {} participants ({} text-based AI, {} audio-native)",
            &session_id.to_string()[..8],
            participants.len(),
            text_ai_count,
            audio_native_count
        );
    }

    /// Add ONE participant to a session, creating the session if this is the first
    /// (#58 — the core-driven registration path).
    ///
    /// Distinct from [`Self::register_session`] on purpose: that one takes the WHOLE
    /// participant list and REPLACES it, which is the right shape for a client handing
    /// over a fully-formed call. But people join one at a time, so driving that method
    /// per-join would clobber everyone already in the room — each arrival erasing the
    /// last. Joining is an append; only a client snapshotting the whole call is a replace.
    ///
    /// Idempotent per user: re-joining (a reconnect, a duplicate event) updates the
    /// existing entry rather than duplicating her, so a flaky client cannot inflate the
    /// roster.
    pub fn add_participant(&self, session_id: Uuid, room_id: Uuid, participant: VoiceParticipant) {
        {
            let mut sessions = self
                .session_participants
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let entry = sessions.entry(session_id).or_default();
            match entry.iter_mut().find(|p| p.user_id == participant.user_id) {
                Some(existing) => *existing = participant.clone(),
                None => entry.push(participant.clone()),
            }
        }
        {
            let mut contexts = self
                .session_contexts
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            contexts
                .entry(session_id)
                .or_insert_with(|| ConversationContext::new(session_id, room_id));
        }
        clog_info!(
            "Session {} += {} ({:?})",
            &session_id.to_string()[..8],
            participant.display_name,
            participant.participant_type
        );
    }

    /// Every registered session and its participants — the read side the live-call
    /// projection needs (#58).
    ///
    /// There was no reader at all until now, which is part of why registration being
    /// client-driven went unnoticed for so long: nothing could ask "which sessions does
    /// the core actually think are live?" and compare that to the calls the CallServer is
    /// running. That comparison IS the defect made visible — a call with no registration
    /// is why a persona sits in a room, present and silent, while `isInCall()` returns
    /// false and her responses are dropped.
    ///
    /// Returns a snapshot, not a guard: the caller is a periodic projection, and holding
    /// this lock across an await would put a rendering concern in the audio path.
    pub fn registered_sessions(&self) -> Vec<(Uuid, Vec<VoiceParticipant>)> {
        self.session_participants
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(id, ps)| (*id, ps.clone()))
            .collect()
    }

    pub fn unregister_session(&self, session_id: Uuid) {
        self.session_participants
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&session_id);
        self.session_contexts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&session_id);
        clog_info!("Unregistered session {}", &session_id.to_string()[..8]);
    }

    /// Process utterance and return ALL AI participant IDs (broadcast model)
    /// Each AI will decide if they want to respond via their own logic
    pub fn on_utterance(&self, event: UtteranceEvent) -> Vec<Uuid> {
        clog_info!(
            "Utterance from {}: \"{}...\"",
            event.speaker_name,
            crate::live::audio::tts::truncate_str(&event.transcript, 50)
        );

        // Get context
        let mut contexts = self
            .session_contexts
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let context = match contexts.get_mut(&event.session_id) {
            Some(ctx) => ctx,
            None => {
                clog_info!(
                    "No context for session {}",
                    crate::live::audio::tts::truncate_str(&event.session_id.to_string(), 8)
                );
                return Vec::new();
            }
        };

        // Update context
        context.add_utterance(event.clone());

        // Get participants
        let participants = self
            .session_participants
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let session_participants = match participants.get(&event.session_id) {
            Some(p) => p,
            None => {
                clog_info!(
                    "No participants for session {}",
                    &event.session_id.to_string()[..8]
                );
                return Vec::new();
            }
        };

        // Get TEXT-BASED AI participants (excluding speaker AND audio-native AIs)
        // Audio-native AIs (Gemini Live, Qwen3-Omni, GPT-4o Realtime) hear raw audio
        // through the mixer's mix-minus stream — sending them transcriptions too would
        // cause them to respond twice (once to audio, once to text).
        let ai_participants: Vec<&VoiceParticipant> = session_participants
            .iter()
            .filter(|p| {
                matches!(p.participant_type, SpeakerType::Persona)
                    && p.user_id != event.speaker_id
                    && !p.is_audio_native
            })
            .collect();

        if ai_participants.is_empty() {
            clog_info!(
                "No text-based AI participants to respond (audio-native AIs hear via mixer)"
            );
            return Vec::new();
        }

        // Broadcast to text-based AI participants only

        ai_participants.iter().map(|p| p.user_id).collect()
    }

    /// Every AI participant in the session — the video VIEWERS (who should SEE the
    /// call's frames). Unlike [`on_utterance`](Self::on_utterance)'s audio broadcast,
    /// this is NOT gated on `is_audio_native`: hearing is bridged per audio-capability,
    /// but VISION is universal — every persona sees, and the system bridges the pixels
    /// to a description for a non-vision model (the sensory-architecture contract). The
    /// speaker-skip (a persona not seeing its OWN frame) is applied downstream by the
    /// ingest fan-out, keyed on identity — here we return the full AI roster. Humans are
    /// excluded (they see through their own client, not a PerceptionBuffer). Empty for an
    /// unknown session.
    pub fn video_viewers(&self, session_id: Uuid) -> Vec<Uuid> {
        self.session_participants
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&session_id)
            .map(|ps| {
                ps.iter()
                    .filter(|p| {
                        matches!(
                            p.participant_type,
                            SpeakerType::Persona | SpeakerType::Agent
                        )
                    })
                    .map(|p| p.user_id)
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[path = "tests/orchestrator_tests.rs"]
mod orchestrator_tests;

#[cfg(test)]
mod old_tests {
    use super::*;

    #[test]
    fn test_register_session() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let participant = VoiceParticipant {
            user_id: Uuid::new_v4(),
            display_name: "Test AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec!["coding".to_string()],
            is_audio_native: false,
        };

        orchestrator.register_session(session_id, room_id, vec![participant]);

        let participants = orchestrator
            .session_participants
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        assert!(participants.contains_key(&session_id));
    }

    #[test]
    fn test_broadcast_to_text_based_ais_only() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let speaker_id = Uuid::new_v4();
        let text_ai_id = Uuid::new_v4();
        let audio_native_ai_id = Uuid::new_v4();

        let text_ai = VoiceParticipant {
            user_id: text_ai_id,
            display_name: "Helper AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
            is_audio_native: false,
        };

        let audio_native_ai = VoiceParticipant {
            user_id: audio_native_ai_id,
            display_name: "Gemini AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
            is_audio_native: true,
        };

        orchestrator.register_session(session_id, room_id, vec![text_ai, audio_native_ai]);

        let event = UtteranceEvent {
            session_id,
            speaker_id,
            speaker_name: "test-user".to_string(),
            speaker_type: SpeakerType::Human,
            transcript: "This is a statement".to_string(),
            confidence: 0.95,
            timestamp: 1000,
        };

        let responders = orchestrator.on_utterance(event);
        // Only text-based AI should receive transcription
        assert_eq!(responders.len(), 1);
        assert!(responders.contains(&text_ai_id));
        // Audio-native AI excluded (hears via mixer stream)
        assert!(!responders.contains(&audio_native_ai_id));
    }

    // what this catches: video_viewers returns EVERY AI persona/agent — including
    // audio-native ones (vision is universal, unlike the audio broadcast which excludes
    // them) — and EXCLUDES humans (they see through their own client). The speaker-skip
    // is applied downstream by the ingest fan-out, so a speaker in the roster is still
    // returned here. If this drifts, personas go blind on a call or a human's video is
    // wrongly routed into a PerceptionBuffer.
    #[test]
    fn video_viewers_are_every_ai_including_audio_native_never_humans() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::new_v4();
        let text_ai = Uuid::new_v4();
        let audio_native_ai = Uuid::new_v4();
        let human = Uuid::new_v4();

        orchestrator.register_session(
            session_id,
            Uuid::new_v4(),
            vec![
                VoiceParticipant {
                    user_id: text_ai,
                    display_name: "Helper".into(),
                    participant_type: SpeakerType::Persona,
                    expertise: vec![],
                    is_audio_native: false,
                },
                VoiceParticipant {
                    user_id: audio_native_ai,
                    display_name: "Gemini".into(),
                    participant_type: SpeakerType::Persona,
                    expertise: vec![],
                    is_audio_native: true,
                },
                VoiceParticipant {
                    user_id: human,
                    display_name: "Operator".into(),
                    participant_type: SpeakerType::Human,
                    expertise: vec![],
                    is_audio_native: false,
                },
            ],
        );

        let viewers = orchestrator.video_viewers(session_id);
        assert_eq!(viewers.len(), 2, "both AIs see; the human does not");
        assert!(viewers.contains(&text_ai));
        assert!(
            viewers.contains(&audio_native_ai),
            "audio-native AIs still SEE — vision is not gated on audio capability"
        );
        assert!(
            !viewers.contains(&human),
            "humans see via their own client, not a buffer"
        );

        // Unknown session → empty (a frame for a call we don't track goes nowhere).
        assert!(orchestrator.video_viewers(Uuid::new_v4()).is_empty());
    }

    #[test]
    fn test_broadcast_to_all_text_ais() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let speaker_id = Uuid::new_v4();
        let ai1_id = Uuid::new_v4();
        let ai2_id = Uuid::new_v4();

        let participant1 = VoiceParticipant {
            user_id: ai1_id,
            display_name: "Helper AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
            is_audio_native: false,
        };

        let participant2 = VoiceParticipant {
            user_id: ai2_id,
            display_name: "Teacher AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
            is_audio_native: false,
        };

        orchestrator.register_session(session_id, room_id, vec![participant1, participant2]);

        let event = UtteranceEvent {
            session_id,
            speaker_id,
            speaker_name: "test-user".to_string(),
            speaker_type: SpeakerType::Human,
            transcript: "This is a statement, not a question".to_string(),
            confidence: 0.95,
            timestamp: 1000,
        };

        let responders = orchestrator.on_utterance(event);
        assert_eq!(responders.len(), 2);
        assert!(responders.contains(&ai1_id));
        assert!(responders.contains(&ai2_id));
    }
}
