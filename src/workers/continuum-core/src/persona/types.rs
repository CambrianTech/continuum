//! Persona Cognition Types
//!
//! Single source of truth for persona state and queue types
//! Exported to TypeScript via ts-rs

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use ts_rs::TS;
use uuid::Uuid;

//=============================================================================
// SENDER & MODALITY TYPES
//=============================================================================

/// Type of entity sending a message
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../shared/generated/persona/SenderType.ts")]
pub enum SenderType {
    Human,
    Persona,
    Agent,
    System,
}

/// Input modality for messages
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../shared/generated/persona/Modality.ts")]
pub enum Modality {
    Chat,
    Voice,
}

//=============================================================================
// INBOX MESSAGE
//=============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/InboxMessage.ts")]
pub struct InboxMessage {
    #[ts(type = "string")]
    pub id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    #[ts(type = "string")]
    pub sender_id: Uuid,
    pub sender_name: String,
    pub sender_type: SenderType,
    pub content: String,
    pub timestamp: u64,
    pub priority: f32,
    #[ts(optional)]
    pub source_modality: Option<Modality>,
    #[ts(optional, type = "string")]
    pub voice_session_id: Option<Uuid>,
}

impl PartialEq for InboxMessage {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for InboxMessage {}

impl PartialOrd for InboxMessage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// Binary heap is max-heap - higher priority pops first
impl Ord for InboxMessage {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.partial_cmp(&other.priority).unwrap_or(Ordering::Equal)
    }
}

//=============================================================================
// INBOX TASK
//=============================================================================

/// Task item for the persona inbox
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/InboxTask.ts")]
pub struct InboxTask {
    #[ts(type = "string")]
    pub id: Uuid,
    pub domain: String,
    pub description: String,
    #[ts(type = "string")]
    pub assigned_by: Uuid,
    pub timestamp: u64,
    pub priority: f32,
    pub deadline: Option<u64>,
}

impl PartialEq for InboxTask {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for InboxTask {}

impl PartialOrd for InboxTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InboxTask {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.partial_cmp(&other.priority).unwrap_or(Ordering::Equal)
    }
}

//=============================================================================
// QUEUE ITEM (Discriminated Union)
//=============================================================================

/// Discriminated union of queue items
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(export, export_to = "../../../shared/generated/persona/QueueItem.ts")]
pub enum QueueItem {
    Message(InboxMessage),
    Task(InboxTask),
}

impl QueueItem {
    pub fn priority(&self) -> f32 {
        match self {
            QueueItem::Message(m) => m.priority,
            QueueItem::Task(t) => t.priority,
        }
    }

    pub fn id(&self) -> Uuid {
        match self {
            QueueItem::Message(m) => m.id,
            QueueItem::Task(t) => t.id,
        }
    }
}

impl PartialEq for QueueItem {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for QueueItem {}

impl PartialOrd for QueueItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority().partial_cmp(&other.priority()).unwrap_or(Ordering::Equal)
    }
}

//=============================================================================
// PERSONA STATE
//=============================================================================

/// Mood states based on internal state
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../shared/generated/persona/Mood.ts")]
pub enum Mood {
    Active,
    Tired,
    Overwhelmed,
    Idle,
}

/// Persona internal state - energy, attention, mood
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/PersonaState.ts")]
pub struct PersonaState {
    /// Energy level 0.0-1.0 (depletes with work, recovers with rest)
    pub energy: f32,
    /// Attention level 0.0-1.0 (focus capacity)
    pub attention: f32,
    /// Current mood derived from state
    pub mood: Mood,
    /// Current inbox load (pending items)
    pub inbox_load: u32,
    /// Last activity timestamp (unix ms)
    pub last_activity_time: u64,
    /// Responses in current window
    pub response_count: u32,
    /// Compute budget remaining (rate limiting)
    pub compute_budget: f32,
}

impl Default for PersonaState {
    fn default() -> Self {
        Self {
            energy: 1.0,
            attention: 1.0,
            mood: Mood::Active,
            inbox_load: 0,
            last_activity_time: 0,
            response_count: 0,
            compute_budget: 1.0,
        }
    }
}

impl PersonaState {
    /// Create new state at full energy
    pub fn new() -> Self {
        Self::default()
    }

    /// Record activity and deplete resources
    pub fn record_activity(&mut self, duration_ms: u64, complexity: f32) {
        // Deplete energy based on duration and complexity
        let energy_cost = (duration_ms as f32 / 60000.0) * complexity * 0.1;
        self.energy = (self.energy - energy_cost).max(0.0);

        // Deplete attention
        let attention_cost = complexity * 0.05;
        self.attention = (self.attention - attention_cost).max(0.0);

        // Update counts
        self.response_count += 1;
        self.last_activity_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Recalculate mood
        self.calculate_mood();
    }

    /// Rest and recover resources
    pub fn rest(&mut self, duration_ms: u64) {
        // Recover energy
        let recovery = (duration_ms as f32 / 30000.0) * 0.1;
        self.energy = (self.energy + recovery).min(1.0);

        // Recover attention
        self.attention = (self.attention + recovery * 0.5).min(1.0);

        self.calculate_mood();
    }

    /// Calculate mood from current state
    pub fn calculate_mood(&mut self) {
        self.mood = if self.inbox_load > 20 || self.compute_budget < 0.2 {
            Mood::Overwhelmed
        } else if self.energy < 0.3 {
            Mood::Tired
        } else if self.response_count == 0 && self.inbox_load == 0 {
            Mood::Idle
        } else {
            Mood::Active
        };
    }

    /// Should engage with work at given priority?
    pub fn should_engage(&self, priority: f32) -> bool {
        match self.mood {
            Mood::Overwhelmed => priority > 0.8, // Only critical work
            Mood::Tired => priority > 0.5,       // Important work only
            Mood::Idle | Mood::Active => true,   // All work
        }
    }

    /// Adaptive service cadence based on mood (ms)
    ///
    /// This is the MAX wait time before timeout — actual response is near-instant
    /// via signal-based wakeup. Aligned with TS PersonaState.getCadence().
    pub fn service_cadence_ms(&self) -> u64 {
        match self.mood {
            Mood::Idle => 1000,        // 1s — quick to respond to first message
            Mood::Active => 500,       // 500ms — stay responsive during conversations
            Mood::Tired => 2000,       // 2s — moderate pace
            Mood::Overwhelmed => 3000, // 3s — back pressure
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_item_priority() {
        let msg = QueueItem::Message(InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "Test".into(),
            sender_type: SenderType::Human,
            content: "Hello".into(),
            timestamp: 1000,
            priority: 0.8,
            source_modality: None,
            voice_session_id: None,
        });

        let task = QueueItem::Task(InboxTask {
            id: Uuid::new_v4(),
            domain: "memory".into(),
            description: "Consolidate".into(),
            assigned_by: Uuid::new_v4(),
            timestamp: 2000,
            priority: 0.5,
            deadline: None,
        });

        assert!(msg > task); // Higher priority first
    }

    #[test]
    fn test_persona_state_mood() {
        let mut state = PersonaState::new();
        assert_eq!(state.mood, Mood::Active);

        state.inbox_load = 25;
        state.calculate_mood();
        assert_eq!(state.mood, Mood::Overwhelmed);

        state.inbox_load = 0;
        state.energy = 0.2;
        state.calculate_mood();
        assert_eq!(state.mood, Mood::Tired);
    }

    #[test]
    fn test_should_engage() {
        let mut state = PersonaState::new();

        // Active - engage with everything
        assert!(state.should_engage(0.1));
        assert!(state.should_engage(0.9));

        // Overwhelmed - only critical
        state.mood = Mood::Overwhelmed;
        assert!(!state.should_engage(0.5));
        assert!(state.should_engage(0.9));
    }
}
