//! Persona Cognition Engine
//!
//! Core cognitive processing in Rust - the "brain" that TypeScript calls into.
//! Handles:
//! - Priority scoring (fast-path decision)
//! - State management
//! - Message deduplication
//! - Service loop coordination
//!
//! Target: Sub-1ms decisions, freeing TypeScript for UI only.

use super::inbox::PersonaInbox;
use super::types::{InboxMessage, PersonaState, SenderType};
use crate::rag::RagEngine;
use dashmap::DashSet;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::watch;
use tracing::debug;
use ts_rs::TS;
use uuid::Uuid;

//=============================================================================
// DECISION TYPES
//=============================================================================

/// Decision result from cognition engine
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/CognitionDecision.ts")]
pub struct CognitionDecision {
    pub should_respond: bool,
    pub confidence: f32,
    pub reason: String,
    pub decision_time_ms: f64,
    pub fast_path_used: bool,
}

/// Priority calculation result
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/PriorityScore.ts")]
pub struct PriorityScore {
    pub score: f32,
    pub factors: PriorityFactors,
}

/// Factors contributing to priority
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/PriorityFactors.ts")]
pub struct PriorityFactors {
    pub recency_score: f32,
    pub mention_score: f32,
    pub room_score: f32,
    pub sender_score: f32,
    pub voice_boost: f32,
}

//=============================================================================
// COGNITION ENGINE
//=============================================================================

/// Persona cognition engine - fast decision making in Rust
pub struct PersonaCognitionEngine {
    persona_id: Uuid,
    persona_name: String,
    state: PersonaState,
    inbox: PersonaInbox,
    #[allow(dead_code)] // Will be used for RAG context building
    rag_engine: Arc<RagEngine>,

    // Deduplication
    evaluated_messages: DashSet<Uuid>,

    // Shutdown signal
    #[allow(dead_code)] // Will be used for service loop
    shutdown_rx: watch::Receiver<bool>,
}

impl PersonaCognitionEngine {
    pub fn new(
        persona_id: Uuid,
        persona_name: String,
        rag_engine: Arc<RagEngine>,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        Self {
            persona_id,
            persona_name: persona_name.clone(),
            state: PersonaState::new(),
            inbox: PersonaInbox::new(persona_id),
            rag_engine,
            evaluated_messages: DashSet::new(),
            shutdown_rx,
        }
    }

    /// Calculate priority for a message (sub-1ms target)
    pub fn calculate_priority(
        &self,
        content: &str,
        sender_type: SenderType,
        is_voice: bool,
        _room_id: Uuid,
        timestamp: u64,
    ) -> PriorityScore {
        let start = Instant::now();

        // Recency score (newer = higher, decay over 5 minutes)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let age_ms = now.saturating_sub(timestamp);
        let recency_score = 1.0 - (age_ms as f32 / 300_000.0).min(1.0);

        // Mention score (direct mentions = high priority)
        let mention_score = if self.is_mentioned(content) {
            1.0
        } else {
            0.3
        };

        // Sender score (humans > AI for response priority)
        let sender_score = match sender_type {
            SenderType::Human => 0.9,
            SenderType::Persona => 0.4,
            SenderType::Agent => 0.5,
            SenderType::System => 0.2,
        };

        // Voice boost (voice = more urgent)
        let voice_boost = if is_voice { 0.2 } else { 0.0 };

        // Room score (could be enhanced with recent room tracking)
        let room_score = 0.5;

        // Combine scores
        let base_score =
            recency_score * 0.2 + mention_score * 0.4 + sender_score * 0.2 + room_score * 0.2;

        let final_score = (base_score + voice_boost).min(1.0);

        debug!(
            "Priority calc for {} in {:.2}ms: {:.2} (mention={:.2}, sender={:.2}, recency={:.2})",
            &content[..content.len().min(30)],
            start.elapsed().as_secs_f64() * 1000.0,
            final_score,
            mention_score,
            sender_score,
            recency_score
        );

        PriorityScore {
            score: final_score,
            factors: PriorityFactors {
                recency_score,
                mention_score,
                room_score,
                sender_score,
                voice_boost,
            },
        }
    }

    /// Check if persona is mentioned in content
    fn is_mentioned(&self, content: &str) -> bool {
        let content_lower = content.to_lowercase();
        let name_lower = self.persona_name.to_lowercase();

        // Check @mention
        content_lower.contains(&format!("@{name_lower}"))
            || content_lower.contains(&name_lower)
    }

    /// Fast-path decision: should we even consider responding?
    pub fn fast_path_decision(&self, message: &InboxMessage) -> CognitionDecision {
        let start = Instant::now();

        // Check deduplication
        if self.evaluated_messages.contains(&message.id) {
            return CognitionDecision {
                should_respond: false,
                confidence: 1.0,
                reason: "Already evaluated".into(),
                decision_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                fast_path_used: true,
            };
        }

        self.fast_path_decision_core(message, start)
    }

    /// Fast-path decision WITHOUT dedup check.
    /// Used by full_evaluate() where the service cycle already did dedup.
    pub fn fast_path_decision_no_dedup(&self, message: &InboxMessage) -> CognitionDecision {
        let start = Instant::now();
        self.fast_path_decision_core(message, start)
    }

    /// Shared fast-path logic (dedup-agnostic).
    fn fast_path_decision_core(&self, message: &InboxMessage, start: Instant) -> CognitionDecision {
        // Check if sender is self
        if message.sender_id == self.persona_id {
            return CognitionDecision {
                should_respond: false,
                confidence: 1.0,
                reason: "Own message".into(),
                decision_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                fast_path_used: true,
            };
        }

        // Check state-based gating
        if !self.state.should_engage(message.priority) {
            return CognitionDecision {
                should_respond: false,
                confidence: 0.8,
                reason: format!(
                    "State gating: {:?} mood, priority {:.2} too low",
                    self.state.mood, message.priority
                ),
                decision_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                fast_path_used: true,
            };
        }

        // Direct mention = always respond
        if self.is_mentioned(&message.content) {
            self.evaluated_messages.insert(message.id);
            return CognitionDecision {
                should_respond: true,
                confidence: 0.95,
                reason: "Direct mention".into(),
                decision_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                fast_path_used: true,
            };
        }

        // Human sender with high priority = likely respond
        if message.sender_type == SenderType::Human && message.priority > 0.6 {
            self.evaluated_messages.insert(message.id);
            return CognitionDecision {
                should_respond: true,
                confidence: 0.7,
                reason: "Human sender, high priority".into(),
                decision_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                fast_path_used: true,
            };
        }

        // Mark as evaluated, defer to LLM decision
        self.evaluated_messages.insert(message.id);
        CognitionDecision {
            should_respond: true, // Let LLM decide
            confidence: 0.5,
            reason: "Deferred to LLM".into(),
            decision_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            fast_path_used: false,
        }
    }

    /// Enqueue message for processing
    pub fn enqueue_message(&self, message: InboxMessage) {
        debug!(
            "Enqueuing message {} with priority {:.2}",
            message.id, message.priority
        );
        self.inbox.enqueue(message);
        // Update inbox load
        // Note: Can't mutate state here without interior mutability
        // This would be handled by the service loop
    }

    /// Get current state (for TypeScript to read)
    pub fn state(&self) -> &PersonaState {
        &self.state
    }

    /// Update state from external event
    pub fn update_state(&mut self, f: impl FnOnce(&mut PersonaState)) {
        f(&mut self.state);
    }

    /// Get persona ID
    pub fn persona_id(&self) -> Uuid {
        self.persona_id
    }

    /// Check if a message has been evaluated (deduplication).
    pub fn has_evaluated_message(&self, message_id: Uuid) -> bool {
        self.evaluated_messages.contains(&message_id)
    }

    /// Mark a message as evaluated (deduplication).
    pub fn mark_message_evaluated(&self, message_id: Uuid) {
        self.evaluated_messages.insert(message_id);
    }
}

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::watch;

    async fn create_test_engine() -> PersonaCognitionEngine {
        let rag_engine = Arc::new(RagEngine::new());
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        PersonaCognitionEngine::new(
            Uuid::new_v4(),
            "TestBot".into(),
            rag_engine,
            shutdown_rx,
        )
    }

    #[tokio::test]
    async fn test_priority_calculation() {
        let engine = create_test_engine().await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // High priority: human sender, mentioned, recent
        let score = engine.calculate_priority(
            "@TestBot can you help?",
            SenderType::Human,
            false,
            Uuid::new_v4(),
            now,
        );
        assert!(score.score > 0.8, "Mentioned human message should be high priority");

        // Low priority: AI sender, not mentioned, old
        let score = engine.calculate_priority(
            "Just a random message",
            SenderType::Persona,
            false,
            Uuid::new_v4(),
            now - 300_000, // 5 mins old
        );
        assert!(score.score < 0.5, "Old AI message should be low priority");
    }

    #[tokio::test]
    async fn test_fast_path_mention() {
        let engine = create_test_engine().await;

        let msg = InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "Joel".into(),
            sender_type: SenderType::Human,
            content: "@TestBot please help".into(),
            timestamp: 1000,
            priority: 0.8,
            source_modality: None,
            voice_session_id: None,
        };

        let decision = engine.fast_path_decision(&msg);
        assert!(decision.should_respond);
        assert!(decision.fast_path_used);
        assert!(decision.confidence > 0.9);
    }

    #[tokio::test]
    async fn test_deduplication() {
        let engine = create_test_engine().await;

        let msg = InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "Joel".into(),
            sender_type: SenderType::Human,
            content: "Hello".into(),
            timestamp: 1000,
            priority: 0.5,
            source_modality: None,
            voice_session_id: None,
        };

        // First evaluation
        let decision1 = engine.fast_path_decision(&msg);
        assert!(decision1.should_respond);

        // Second evaluation (should be deduplicated)
        let decision2 = engine.fast_path_decision(&msg);
        assert!(!decision2.should_respond);
        assert_eq!(decision2.reason, "Already evaluated");
    }
}
