//! CBAR-style persona turn frame.
//!
//! A turn frame is the per-persona work unit above the raw inbox drain:
//! one bounded room slice, deterministic derived artifacts, and a shape
//! that can be recorded and replayed without booting inference.

use super::inbox::PersonaInboxFrame;
use super::types::InboxMessage;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidatedInboxMessage {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub sender_name: String,
    pub content: String,
    pub timestamp: u64,
}

impl From<&InboxMessage> for ConsolidatedInboxMessage {
    fn from(message: &InboxMessage) -> Self {
        Self {
            id: message.id,
            sender_id: message.sender_id,
            sender_name: message.sender_name.clone(),
            content: message.content.clone(),
            timestamp: message.timestamp,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidatedInboxChunk {
    pub persona_id: Uuid,
    pub room_id: Uuid,
    pub trigger_message_id: Uuid,
    pub messages: Vec<ConsolidatedInboxMessage>,
    pub transcript: String,
    pub source_count: usize,
    pub span_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RagAssemblySeed {
    pub persona_id: Uuid,
    pub room_id: Uuid,
    pub query_text: String,
    pub source_message_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaTurnFrameReplayRecord {
    pub schema_version: u32,
    pub persona_id: Uuid,
    pub room_id: Uuid,
    pub inbox_frame: PersonaInboxFrame,
    pub consolidated_inbox: ConsolidatedInboxChunk,
    pub rag_seed: RagAssemblySeed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaTurnFrame {
    inbox_frame: PersonaInboxFrame,
}

impl PersonaTurnFrame {
    pub fn from_inbox_frame(inbox_frame: PersonaInboxFrame) -> Self {
        Self { inbox_frame }
    }

    pub fn persona_id(&self) -> Uuid {
        self.inbox_frame.persona_id
    }

    pub fn room_id(&self) -> Uuid {
        self.inbox_frame.room_id
    }

    pub fn inbox_frame(&self) -> &PersonaInboxFrame {
        &self.inbox_frame
    }

    /// Consolidate the drained inbox into the single chat-like event a
    /// persona should reason over. Messages remain chronological; the trigger
    /// is the latest message in that bounded room frame.
    pub fn consolidated_inbox(&self) -> Option<ConsolidatedInboxChunk> {
        let trigger = self.inbox_frame.messages.last()?;
        let messages: Vec<ConsolidatedInboxMessage> = self
            .inbox_frame
            .messages
            .iter()
            .map(ConsolidatedInboxMessage::from)
            .collect();
        let transcript = messages
            .iter()
            .map(|message| format!("{}: {}", message.sender_name, message.content))
            .collect::<Vec<_>>()
            .join("\n");

        Some(ConsolidatedInboxChunk {
            persona_id: self.inbox_frame.persona_id,
            room_id: self.inbox_frame.room_id,
            trigger_message_id: trigger.id,
            source_count: messages.len(),
            span_ms: self.inbox_frame.metrics.frame_span_ms,
            messages,
            transcript,
        })
    }

    /// Build the deterministic seed used by RAG/hippocampus assembly. This is
    /// not retrieval and does not hide a fallback route; it is the replayable
    /// input contract that retrieval workers consume.
    pub fn rag_seed(&self) -> Option<RagAssemblySeed> {
        let chunk = self.consolidated_inbox()?;
        Some(RagAssemblySeed {
            persona_id: chunk.persona_id,
            room_id: chunk.room_id,
            query_text: chunk.transcript,
            source_message_ids: chunk
                .messages
                .iter()
                .map(|message| message.id)
                .collect::<Vec<_>>(),
        })
    }

    /// Capture the raw frame plus all derived lazy outputs needed for replay.
    /// Empty frames return `None` instead of synthesizing placeholder context.
    pub fn replay_record(&self) -> Option<PersonaTurnFrameReplayRecord> {
        Some(PersonaTurnFrameReplayRecord {
            schema_version: PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION,
            persona_id: self.persona_id(),
            room_id: self.room_id(),
            inbox_frame: self.inbox_frame.clone(),
            consolidated_inbox: self.consolidated_inbox()?,
            rag_seed: self.rag_seed()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::inbox::{PersonaInbox, PersonaInboxFrameMetrics};
    use crate::persona::{Modality, SenderType};

    fn message(
        room_id: Uuid,
        sender: &str,
        content: &str,
        timestamp: u64,
        priority: f32,
    ) -> InboxMessage {
        InboxMessage {
            id: Uuid::new_v4(),
            room_id,
            sender_id: Uuid::new_v4(),
            sender_name: sender.to_string(),
            sender_type: SenderType::Human,
            content: content.to_string(),
            timestamp,
            priority,
            source_modality: Some(Modality::Chat),
            voice_session_id: None,
        }
    }

    #[test]
    fn turn_frame_consolidates_drained_inbox_once() {
        let persona_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let inbox = PersonaInbox::new(persona_id);
        inbox.enqueue(message(room_id, "Joel", "first", 1_000, 0.5));
        inbox.enqueue(message(room_id, "Ava", "second", 1_010, 0.9));
        inbox.enqueue(message(room_id, "Joel", "third", 1_020, 0.7));

        let inbox_frame = inbox.drain_frame(100, 8).expect("frame drains");
        let turn_frame = PersonaTurnFrame::from_inbox_frame(inbox_frame);
        let chunk = turn_frame
            .consolidated_inbox()
            .expect("non-empty inbox yields chunk");

        assert_eq!(chunk.persona_id, persona_id);
        assert_eq!(chunk.room_id, room_id);
        assert_eq!(chunk.source_count, 3);
        assert_eq!(chunk.span_ms, 20);
        assert_eq!(
            chunk
                .messages
                .iter()
                .map(|message| message.content.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second", "third"]
        );
        assert_eq!(chunk.trigger_message_id, chunk.messages[2].id);
        assert_eq!(chunk.transcript, "Joel: first\nAva: second\nJoel: third");
        assert!(inbox.is_empty(), "one frame, not one inference per message");
    }

    #[test]
    fn rag_seed_is_replayable_from_serialized_turn_frame() {
        let persona_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let messages = vec![
            message(room_id, "Joel", "what changed?", 2_000, 0.8),
            message(room_id, "Mira", "the queue coalesced", 2_030, 0.7),
        ];
        let frame = PersonaInboxFrame {
            persona_id,
            room_id,
            messages,
            metrics: PersonaInboxFrameMetrics {
                queue_depth_before: 2,
                queue_depth_after: 0,
                messages_drained: 2,
                oldest_timestamp: 2_000,
                newest_timestamp: 2_030,
                frame_span_ms: 30,
                drain_duration_us: 12,
            },
        };
        let turn_frame = PersonaTurnFrame::from_inbox_frame(frame);
        let encoded = serde_json::to_string(&turn_frame).expect("serialize turn frame");
        let decoded: PersonaTurnFrame =
            serde_json::from_str(&encoded).expect("deserialize turn frame");

        let seed = decoded.rag_seed().expect("seed from replayed frame");
        assert_eq!(seed.persona_id, persona_id);
        assert_eq!(seed.room_id, room_id);
        assert_eq!(
            seed.query_text,
            "Joel: what changed?\nMira: the queue coalesced"
        );
        assert_eq!(seed.source_message_ids.len(), 2);
    }

    #[test]
    fn replay_record_captures_raw_frame_and_derived_outputs() {
        let persona_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let messages = vec![
            message(room_id, "Joel", "first", 3_000, 0.8),
            message(room_id, "Mira", "second", 3_040, 0.7),
        ];
        let source_ids = messages
            .iter()
            .map(|message| message.id)
            .collect::<Vec<_>>();
        let frame = PersonaInboxFrame {
            persona_id,
            room_id,
            messages,
            metrics: PersonaInboxFrameMetrics {
                queue_depth_before: 2,
                queue_depth_after: 0,
                messages_drained: 2,
                oldest_timestamp: 3_000,
                newest_timestamp: 3_040,
                frame_span_ms: 40,
                drain_duration_us: 7,
            },
        };
        let record = PersonaTurnFrame::from_inbox_frame(frame)
            .replay_record()
            .expect("non-empty frame records");

        assert_eq!(
            record.schema_version,
            PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION
        );
        assert_eq!(record.persona_id, persona_id);
        assert_eq!(record.room_id, room_id);
        assert_eq!(record.inbox_frame.metrics.messages_drained, 2);
        assert_eq!(
            record.consolidated_inbox.transcript,
            "Joel: first\nMira: second"
        );
        assert_eq!(record.rag_seed.source_message_ids, source_ids);

        let json = serde_json::to_value(&record).expect("record serializes");
        assert_eq!(json["schemaVersion"], 1);
        assert!(json.get("inboxFrame").is_some());
        assert!(json.get("consolidatedInbox").is_some());
        assert!(json.get("ragSeed").is_some());
    }

    #[test]
    fn empty_frame_does_not_synthesize_replay_record() {
        let frame = PersonaInboxFrame {
            persona_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            messages: vec![],
            metrics: PersonaInboxFrameMetrics {
                queue_depth_before: 0,
                queue_depth_after: 0,
                messages_drained: 0,
                oldest_timestamp: 0,
                newest_timestamp: 0,
                frame_span_ms: 0,
                drain_duration_us: 0,
            },
        };

        assert!(PersonaTurnFrame::from_inbox_frame(frame)
            .replay_record()
            .is_none());
    }
}
