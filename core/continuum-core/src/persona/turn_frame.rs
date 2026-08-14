//! CBAR-style persona turn frame.
//!
//! A turn frame is the per-persona work unit above the raw inbox drain:
//! one bounded room slice, deterministic derived artifacts, and a shape
//! that can be recorded and replayed without booting inference.

use super::inbox::PersonaInboxFrame;
use super::types::InboxMessage;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// v1 = original schema (consolidated_inbox + rag_seed only).
/// v2 = adds response_prompt as an Optional field. Forward-compat:
/// v1 records deserialize cleanly into v2 with response_prompt =
/// None. Backwards-compat: v2 records still load on v1 readers
/// because old readers ignore unknown fields by default (serde
/// behavior).
pub const PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ConsolidatedInboxMessage.ts"
)]
pub struct ConsolidatedInboxMessage {
    #[ts(type = "string")]
    pub id: Uuid,
    #[ts(type = "string")]
    pub sender_id: Uuid,
    pub sender_name: String,
    pub content: String,
    #[ts(type = "number")]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ConsolidatedInboxChunk.ts"
)]
pub struct ConsolidatedInboxChunk {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    #[ts(type = "string")]
    pub trigger_message_id: Uuid,
    pub messages: Vec<ConsolidatedInboxMessage>,
    pub transcript: String,
    #[ts(type = "number")]
    pub source_count: usize,
    #[ts(type = "number")]
    pub span_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/RagAssemblySeed.ts"
)]
pub struct RagAssemblySeed {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    pub query_text: String,
    #[ts(type = "Array<string>")]
    pub source_message_ids: Vec<Uuid>,
}

/// Role of one prompt turn in the chat-style ResponsePrompt.
/// Matches the de-facto chat-completion role taxonomy (System /
/// User / Assistant). The persona module emits only User role
/// today (inbox messages); System comes from the persona's
/// IdentityState (filled in by the caller); Assistant comes from
/// the persona's prior outputs when self-reflection is wired
/// (future PR).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "lowercase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PromptRole.ts"
)]
pub enum PromptRole {
    System,
    User,
    Assistant,
}

/// One turn in the chat-style ResponsePrompt. Pairs a `PromptRole`
/// with a content string. Multimodal content (images, audio) lands
/// in a follow-up PR per the CBAR-SUBSTRATE multimodal contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PromptMessage.ts"
)]
pub struct PromptMessage {
    pub role: PromptRole,
    pub content: String,
}

/// Lazy output of `PersonaTurnFrame::response_prompt()`: the chat-
/// style prompt ready for inference. Inference adapters (PR-4
/// inference-llm + LlamaCppAdapter + cloud adapters) translate
/// this into their native request format.
///
/// The substrate owns this shape so prompt-building stays
/// replayable + deterministic — no per-adapter TS prompt-build
/// hacks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ResponsePrompt.ts"
)]
pub struct ResponsePrompt {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Persona identity / role instruction. PR-1 returns `None`;
    /// callers fill in from the persona's IdentityState (loaded
    /// separately from the turn frame). Future PR may load it
    /// lazily into the frame.
    pub system_prompt: Option<String>,
    pub messages: Vec<PromptMessage>,
    /// The inbox message that triggered this turn — used by
    /// sentinel attribution + replay to correlate the prompt back
    /// to the originating event.
    #[ts(type = "string")]
    pub trigger_message_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaTurnFrameReplayRecord.ts"
)]
pub struct PersonaTurnFrameReplayRecord {
    #[ts(type = "number")]
    pub schema_version: u32,
    #[ts(type = "string")]
    pub persona_id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    pub inbox_frame: PersonaInboxFrame,
    pub consolidated_inbox: ConsolidatedInboxChunk,
    pub rag_seed: RagAssemblySeed,
    /// v2 schema (PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION = 2):
    /// the inference-ready prompt captured at record time. v1
    /// records deserialize with None via `serde(default)`; v2
    /// records always populate via `PersonaTurnFrame::replay_record()`.
    ///
    /// Why on the replay record: prod replay needs to reproduce
    /// the exact prompt that fed inference. Building it lazily at
    /// replay time would depend on the inbox-message → prompt
    /// mapping logic remaining bit-identical across substrate
    /// versions, which isn't a contract anyone wants to maintain.
    /// Capturing the prompt at record time pins the input to
    /// inference for downstream attribution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_prompt: Option<ResponsePrompt>,
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

    /// Build the chat-style prompt ready for inference. Each
    /// inbox message becomes one `PromptMessage` in chronological
    /// order; the persona's identity / system instruction is left
    /// as `None` for the caller to fill in from the persona's
    /// IdentityState (a separate concern not loaded into the turn
    /// frame).
    ///
    /// This is the deterministic chat-shape input the inference
    /// engine (PR-4 inference-llm) consumes via its
    /// `InferenceRequest.prompt_text` field. The substrate owns
    /// the prompt-build path; no TS PRG wraps a raw transcript
    /// into a model-specific prompt format. Per Joel's "Rust owns
    /// behavior" + "no TS shimming Rust outputs" rules.
    ///
    /// Returns `None` for empty frames (matches the
    /// consolidated_inbox + rag_seed contract — empty inbox = no
    /// turn to plan, not a placeholder synthesis).
    pub fn response_prompt(&self) -> Option<ResponsePrompt> {
        let chunk = self.consolidated_inbox()?;
        let messages: Vec<PromptMessage> = chunk
            .messages
            .iter()
            .map(|m| PromptMessage {
                // Every inbox message maps to a User-role prompt
                // turn from the persona's perspective. The
                // persona may have its own outgoing messages
                // in the room, but those would not be in this
                // persona's inbox — the inbox is what the
                // persona is asked to react to. PR-follow-up
                // may add Assistant/System role disambiguation
                // when the inbox carries the persona's own
                // prior outputs for self-reflection.
                role: PromptRole::User,
                content: format!("{}: {}", m.sender_name, m.content),
            })
            .collect();
        Some(ResponsePrompt {
            persona_id: chunk.persona_id,
            room_id: chunk.room_id,
            system_prompt: None,
            messages,
            trigger_message_id: chunk.trigger_message_id,
        })
    }

    /// Capture the raw frame plus all derived lazy outputs needed for replay.
    /// Empty frames return `None` instead of synthesizing placeholder context.
    ///
    /// v2 schema captures the response_prompt at record time so
    /// prod replay reproduces the exact inference input — see
    /// `PersonaTurnFrameReplayRecord.response_prompt` docstring.
    pub fn replay_record(&self) -> Option<PersonaTurnFrameReplayRecord> {
        Some(PersonaTurnFrameReplayRecord {
            schema_version: PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION,
            persona_id: self.persona_id(),
            room_id: self.room_id(),
            inbox_frame: self.inbox_frame.clone(),
            consolidated_inbox: self.consolidated_inbox()?,
            rag_seed: self.rag_seed()?,
            response_prompt: self.response_prompt(),
        })
    }
}

impl ResponsePrompt {
    /// Flatten the chat-style prompt into a single plain-text
    /// prompt suitable for adapter-based inference engines that
    /// tokenize internally (LlamaCppAdapter + cloud adapters via
    /// `InferenceRequest.prompt_text`).
    ///
    /// Format: `system_prompt` on its own paragraph (if present),
    /// then each `PromptMessage` on its own line as
    /// `Role: content`. Role is lowercased to match the on-the-wire
    /// PromptRole serde format ("system", "user", "assistant").
    ///
    /// This is a deliberate "flatten now, structure later" decision:
    /// adapter-based engines re-structure into their native format
    /// internally; raw-token engines don't use prompt_text at all
    /// (they take prompt_tokens). The substrate's job is to give
    /// adapters a single deterministic text input that round-trips.
    pub fn to_prompt_text(&self) -> String {
        let mut out = String::new();
        if let Some(system) = self.system_prompt.as_deref() {
            if !system.is_empty() {
                out.push_str(system);
                out.push_str("\n\n");
            }
        }
        for (i, msg) in self.messages.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let role = match msg.role {
                PromptRole::System => "system",
                PromptRole::User => "user",
                PromptRole::Assistant => "assistant",
            };
            out.push_str(role);
            out.push_str(": ");
            out.push_str(&msg.content);
        }
        out
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
        inbox.enqueue(message(room_id, "Operator", "first", 1_000, 0.5));
        inbox.enqueue(message(room_id, "Ava", "second", 1_010, 0.9));
        inbox.enqueue(message(room_id, "Operator", "third", 1_020, 0.7));

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
        assert_eq!(
            chunk.transcript,
            "Operator: first\nAva: second\nOperator: third"
        );
        assert!(inbox.is_empty(), "one frame, not one inference per message");
    }

    #[test]
    fn rag_seed_is_replayable_from_serialized_turn_frame() {
        let persona_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let messages = vec![
            message(room_id, "Operator", "what changed?", 2_000, 0.8),
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
            "Operator: what changed?\nMira: the queue coalesced"
        );
        assert_eq!(seed.source_message_ids.len(), 2);
    }

    #[test]
    fn replay_record_captures_raw_frame_and_derived_outputs() {
        let persona_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let messages = vec![
            message(room_id, "Operator", "first", 3_000, 0.8),
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
            "Operator: first\nMira: second"
        );
        assert_eq!(record.rag_seed.source_message_ids, source_ids);

        let json = serde_json::to_value(&record).expect("record serializes");
        assert_eq!(
            json["schemaVersion"], 2,
            "schema bumped to 2 with response_prompt addition"
        );
        assert!(json.get("inboxFrame").is_some());
        assert!(json.get("consolidatedInbox").is_some());
        assert!(json.get("ragSeed").is_some());
        // v2: response_prompt populated for non-empty frames.
        assert!(
            json.get("responsePrompt").is_some(),
            "v2 schema populates response_prompt for non-empty frames"
        );
    }

    // ─── v2 schema response_prompt on replay_record tests ──────

    #[test]
    fn v1_replay_record_without_response_prompt_deserializes_cleanly() {
        // Simulates an old v1 record on disk: omits the
        // response_prompt field entirely. Should deserialize with
        // response_prompt = None (backwards-compat).
        let json = r#"{
            "schemaVersion": 1,
            "personaId": "00000000-0000-0000-0000-000000000001",
            "roomId": "00000000-0000-0000-0000-000000000002",
            "inboxFrame": {
                "personaId": "00000000-0000-0000-0000-000000000001",
                "roomId": "00000000-0000-0000-0000-000000000002",
                "metrics": {
                    "queueDepthBefore": 1,
                    "queueDepthAfter": 0,
                    "messagesDrained": 1,
                    "oldestTimestamp": 1,
                    "newestTimestamp": 1,
                    "frameSpanMs": 0,
                    "drainDurationUs": 1
                },
                "messages": []
            },
            "consolidatedInbox": {
                "personaId": "00000000-0000-0000-0000-000000000001",
                "roomId": "00000000-0000-0000-0000-000000000002",
                "triggerMessageId": "00000000-0000-0000-0000-000000000003",
                "messages": [],
                "transcript": "",
                "sourceCount": 0,
                "spanMs": 0
            },
            "ragSeed": {
                "personaId": "00000000-0000-0000-0000-000000000001",
                "roomId": "00000000-0000-0000-0000-000000000002",
                "queryText": "",
                "sourceMessageIds": []
            }
        }"#;
        let record: PersonaTurnFrameReplayRecord =
            serde_json::from_str(json).expect("v1 record deserializes");
        assert_eq!(record.schema_version, 1);
        assert!(
            record.response_prompt.is_none(),
            "v1 records have no response_prompt"
        );
    }

    #[test]
    fn v2_replay_record_populates_response_prompt_for_non_empty_frame() {
        let room_id = Uuid::new_v4();
        let frame = PersonaInboxFrame {
            persona_id: Uuid::new_v4(),
            room_id,
            messages: vec![message(room_id, "Operator", "hello", 1, 0.5)],
            metrics: PersonaInboxFrameMetrics {
                queue_depth_before: 1,
                queue_depth_after: 0,
                messages_drained: 1,
                oldest_timestamp: 1,
                newest_timestamp: 1,
                frame_span_ms: 0,
                drain_duration_us: 1,
            },
        };
        let record = PersonaTurnFrame::from_inbox_frame(frame)
            .replay_record()
            .expect("non-empty frame produces record");

        // v2 schema bump.
        assert_eq!(record.schema_version, 2);

        // response_prompt populated alongside the other lazy outputs.
        let prompt = record
            .response_prompt
            .as_ref()
            .expect("v2 record has response_prompt for non-empty frame");
        assert_eq!(prompt.messages.len(), 1);
        assert_eq!(prompt.messages[0].content, "Operator: hello");
    }

    #[test]
    fn v2_serialization_omits_response_prompt_when_none() {
        // Construct a record with response_prompt=None manually (the
        // empty-frame path doesn't produce records, so we construct
        // by hand to test the wire shape).
        let record = PersonaTurnFrameReplayRecord {
            schema_version: PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION,
            persona_id: Uuid::nil(),
            room_id: Uuid::nil(),
            inbox_frame: PersonaInboxFrame {
                persona_id: Uuid::nil(),
                room_id: Uuid::nil(),
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
            },
            consolidated_inbox: ConsolidatedInboxChunk {
                persona_id: Uuid::nil(),
                room_id: Uuid::nil(),
                trigger_message_id: Uuid::nil(),
                messages: vec![],
                transcript: String::new(),
                source_count: 0,
                span_ms: 0,
            },
            rag_seed: RagAssemblySeed {
                persona_id: Uuid::nil(),
                room_id: Uuid::nil(),
                query_text: String::new(),
                source_message_ids: vec![],
            },
            response_prompt: None,
        };
        let json = serde_json::to_value(&record).unwrap();
        // skip_serializing_if = "Option::is_none" → field absent on wire.
        assert!(
            json.get("responsePrompt").is_none(),
            "None response_prompt omits the field (skip_serializing_if)"
        );
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

    // ─── ResponsePrompt lazy output tests ──────────────────────

    #[test]
    fn response_prompt_returns_none_for_empty_frame() {
        let persona_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let frame = PersonaInboxFrame {
            persona_id,
            room_id,
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
            .response_prompt()
            .is_none());
    }

    #[test]
    fn response_prompt_carries_one_user_message_per_inbox_message() {
        let room_id = Uuid::new_v4();
        let frame = PersonaInboxFrame {
            persona_id: Uuid::new_v4(),
            room_id,
            messages: vec![
                message(room_id, "Operator", "first line", 1_000, 0.9),
                message(room_id, "Mira", "second line", 1_010, 0.8),
            ],
            metrics: PersonaInboxFrameMetrics {
                queue_depth_before: 2,
                queue_depth_after: 0,
                messages_drained: 2,
                oldest_timestamp: 1_000,
                newest_timestamp: 1_010,
                frame_span_ms: 10,
                drain_duration_us: 2,
            },
        };
        let prompt = PersonaTurnFrame::from_inbox_frame(frame)
            .response_prompt()
            .expect("non-empty frame produces ResponsePrompt");

        assert_eq!(prompt.messages.len(), 2);
        assert!(matches!(prompt.messages[0].role, PromptRole::User));
        assert!(matches!(prompt.messages[1].role, PromptRole::User));
        assert_eq!(prompt.messages[0].content, "Operator: first line");
        assert_eq!(prompt.messages[1].content, "Mira: second line");
    }

    #[test]
    fn response_prompt_system_prompt_is_none_pr1() {
        // Per the docstring: PR-1 returns None; callers fill in
        // from IdentityState. Pin so a future PR that auto-loads
        // it is a deliberate flip of this test.
        let room_id = Uuid::new_v4();
        let frame = PersonaInboxFrame {
            persona_id: Uuid::new_v4(),
            room_id,
            messages: vec![message(room_id, "Operator", "hi", 1, 0.5)],
            metrics: PersonaInboxFrameMetrics {
                queue_depth_before: 1,
                queue_depth_after: 0,
                messages_drained: 1,
                oldest_timestamp: 1,
                newest_timestamp: 1,
                frame_span_ms: 0,
                drain_duration_us: 1,
            },
        };
        let prompt = PersonaTurnFrame::from_inbox_frame(frame)
            .response_prompt()
            .unwrap();
        assert!(
            prompt.system_prompt.is_none(),
            "PR-1 leaves system_prompt for caller"
        );
    }

    #[test]
    fn response_prompt_trigger_matches_latest_message_id() {
        let room_id = Uuid::new_v4();
        let m1 = message(room_id, "Operator", "earlier", 1, 0.5);
        let m2 = message(room_id, "Mira", "trigger", 2, 0.5);
        let trigger_id = m2.id;
        let frame = PersonaInboxFrame {
            persona_id: Uuid::new_v4(),
            room_id,
            messages: vec![m1, m2],
            metrics: PersonaInboxFrameMetrics {
                queue_depth_before: 2,
                queue_depth_after: 0,
                messages_drained: 2,
                oldest_timestamp: 1,
                newest_timestamp: 2,
                frame_span_ms: 1,
                drain_duration_us: 1,
            },
        };
        let prompt = PersonaTurnFrame::from_inbox_frame(frame)
            .response_prompt()
            .unwrap();
        // trigger_message_id is the latest message (matches
        // consolidated_inbox semantics).
        assert_eq!(prompt.trigger_message_id, trigger_id);
    }

    #[test]
    fn response_prompt_round_trips_through_serde() {
        let room_id = Uuid::new_v4();
        let frame = PersonaInboxFrame {
            persona_id: Uuid::new_v4(),
            room_id,
            messages: vec![message(room_id, "Operator", "hi", 1, 0.5)],
            metrics: PersonaInboxFrameMetrics {
                queue_depth_before: 1,
                queue_depth_after: 0,
                messages_drained: 1,
                oldest_timestamp: 1,
                newest_timestamp: 1,
                frame_span_ms: 0,
                drain_duration_us: 1,
            },
        };
        let prompt = PersonaTurnFrame::from_inbox_frame(frame)
            .response_prompt()
            .unwrap();
        let json = serde_json::to_string(&prompt).unwrap();
        let back: ResponsePrompt = serde_json::from_str(&json).unwrap();
        assert_eq!(back, prompt);

        // Wire shape: camelCase fields + lowercase role.
        assert!(json.contains("\"systemPrompt\":"), "got {json}");
        assert!(json.contains("\"triggerMessageId\":"), "got {json}");
        assert!(json.contains("\"role\":\"user\""), "got {json}");
    }

    // ─── ResponsePrompt::to_prompt_text (Lane D turn-execute) ──

    fn prompt_with(system: Option<&str>, messages: Vec<(PromptRole, &str)>) -> ResponsePrompt {
        ResponsePrompt {
            persona_id: Uuid::nil(),
            room_id: Uuid::nil(),
            system_prompt: system.map(String::from),
            messages: messages
                .into_iter()
                .map(|(role, content)| PromptMessage {
                    role,
                    content: content.to_string(),
                })
                .collect(),
            trigger_message_id: Uuid::nil(),
        }
    }

    #[test]
    fn to_prompt_text_renders_each_message_as_role_colon_content() {
        let prompt = prompt_with(
            None,
            vec![
                (PromptRole::User, "Operator: hi"),
                (PromptRole::User, "Operator: how are you"),
            ],
        );
        let text = prompt.to_prompt_text();
        assert_eq!(text, "user: Operator: hi\nuser: Operator: how are you");
    }

    #[test]
    fn to_prompt_text_prepends_system_prompt_when_present() {
        let prompt = prompt_with(
            Some("You are Helper, a calm assistant."),
            vec![(PromptRole::User, "Operator: ping")],
        );
        let text = prompt.to_prompt_text();
        assert_eq!(
            text,
            "You are Helper, a calm assistant.\n\nuser: Operator: ping"
        );
    }

    #[test]
    fn to_prompt_text_skips_empty_system_prompt() {
        // Empty string is treated as "no system prompt" — no
        // double-newline noise on the wire.
        let prompt = prompt_with(Some(""), vec![(PromptRole::User, "hi")]);
        let text = prompt.to_prompt_text();
        assert_eq!(text, "user: hi");
    }

    #[test]
    fn to_prompt_text_handles_mixed_roles_in_order() {
        let prompt = prompt_with(
            None,
            vec![
                (PromptRole::System, "Be brief."),
                (PromptRole::User, "Operator: hi"),
                (PromptRole::Assistant, "Helper: hello"),
                (PromptRole::User, "Operator: thanks"),
            ],
        );
        let text = prompt.to_prompt_text();
        assert_eq!(
            text,
            "system: Be brief.\nuser: Operator: hi\nassistant: Helper: hello\nuser: Operator: thanks"
        );
    }

    #[test]
    fn to_prompt_text_handles_no_messages() {
        let prompt = prompt_with(Some("Solo system instruction."), vec![]);
        let text = prompt.to_prompt_text();
        assert_eq!(text, "Solo system instruction.\n\n");
    }

    #[test]
    fn to_prompt_text_empty_prompt_returns_empty_string() {
        let prompt = prompt_with(None, vec![]);
        assert_eq!(prompt.to_prompt_text(), "");
    }
}
