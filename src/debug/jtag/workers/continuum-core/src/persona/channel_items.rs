//! Concrete Queue Item Structs
//!
//! Three item types implementing QueueItemBehavior trait:
//! - VoiceQueueItem: Always urgent, never consolidates, never kicked
//! - ChatQueueItem: Per-room consolidation, mention urgency, RTOS aging
//! - TaskQueueItem: Dependency-aware, overdue urgency, related-task consolidation
//!
//! Each item carries all data needed for TS processing after dequeue.
//! Serialization via to_json() sends full item data through IPC.

use super::channel_types::{ActivityDomain, QueueItemBehavior};
use super::types::SenderType;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::time::{SystemTime, UNIX_EPOCH};
use ts_rs::TS;
use uuid::Uuid;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

//=============================================================================
// VOICE QUEUE ITEM
//=============================================================================

/// Voice: always urgent, never consolidates, never kicked.
/// Every utterance is unique and time-critical. FIFO within the channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceQueueItem {
    pub id: Uuid,
    pub room_id: Uuid,
    pub content: String,
    pub sender_id: Uuid,
    pub sender_name: String,
    pub sender_type: SenderType,
    pub voice_session_id: Uuid,
    pub timestamp: u64,
    pub enqueued_at: u64,
    pub priority: f32,
}

impl QueueItemBehavior for VoiceQueueItem {
    fn item_type(&self) -> &'static str { "voice" }
    fn domain(&self) -> ActivityDomain { ActivityDomain::Audio }
    fn id(&self) -> Uuid { self.id }
    fn timestamp(&self) -> u64 { self.timestamp }
    fn base_priority(&self) -> f32 { 1.0 }

    // No aging needed — already max priority
    fn aging_boost_ms(&self) -> f32 { 30_000.0 }
    fn max_aging_boost(&self) -> f32 { 0.0 }

    // Always urgent — bypasses cognitive scheduler
    fn is_urgent(&self) -> bool { true }

    // Never kicked — dropping voice mid-conversation is unacceptable
    fn can_be_kicked(&self) -> bool { false }
    fn kick_resistance(&self, _now_ms: u64, _enqueued_at_ms: u64) -> f32 { f32::INFINITY }

    fn as_any(&self) -> &dyn Any { self }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "voice",
            "id": self.id.to_string(),
            "roomId": self.room_id.to_string(),
            "content": self.content,
            "senderId": self.sender_id.to_string(),
            "senderName": self.sender_name,
            "senderType": self.sender_type,
            "voiceSessionId": self.voice_session_id.to_string(),
            "timestamp": self.timestamp,
            "priority": self.priority,
        })
    }
}

//=============================================================================
// CHAT QUEUE ITEM
//=============================================================================

/// Context from a prior message consolidated into this chat item.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/ConsolidatedContext.ts")]
pub struct ConsolidatedContext {
    #[ts(type = "string")]
    pub sender_id: Uuid,
    pub sender_name: String,
    pub content: String,
    pub timestamp: u64,
}

/// Chat: per-room consolidation, mention-based urgency, standard RTOS aging.
///
/// When multiple messages from the same room are queued, they consolidate.
/// The latest message is the "trigger" (what the AI responds to).
/// Prior messages become consolidated_context (the AI has full room context).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatQueueItem {
    pub id: Uuid,
    pub room_id: Uuid,
    pub content: String,
    pub sender_id: Uuid,
    pub sender_name: String,
    pub sender_type: SenderType,
    pub mentions: bool,
    pub timestamp: u64,
    pub enqueued_at: u64,
    pub priority: f32,
    /// Prior messages consolidated into this item (empty if not consolidated)
    pub consolidated_context: Vec<ConsolidatedContext>,
}

impl QueueItemBehavior for ChatQueueItem {
    fn item_type(&self) -> &'static str { "chat" }
    fn domain(&self) -> ActivityDomain { ActivityDomain::Chat }
    fn id(&self) -> Uuid { self.id }
    fn timestamp(&self) -> u64 { self.timestamp }
    fn base_priority(&self) -> f32 { self.priority }

    // Standard RTOS aging from defaults (30s to reach +0.5 boost)

    // Urgent only if persona is directly mentioned by name
    fn is_urgent(&self) -> bool { self.mentions }

    // Consolidate with other chat items from the SAME ROOM
    fn should_consolidate_with(&self, other: &dyn QueueItemBehavior) -> bool {
        if other.item_type() != "chat" {
            return false;
        }
        // Downcast to check room_id
        if let Some(other_chat) = other.as_any().downcast_ref::<ChatQueueItem>() {
            other_chat.room_id == self.room_id
        } else {
            false
        }
    }

    fn as_any(&self) -> &dyn Any { self }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "chat",
            "id": self.id.to_string(),
            "roomId": self.room_id.to_string(),
            "content": self.content,
            "senderId": self.sender_id.to_string(),
            "senderName": self.sender_name,
            "senderType": self.sender_type,
            "mentions": self.mentions,
            "timestamp": self.timestamp,
            "priority": self.priority,
            "consolidatedContext": self.consolidated_context,
            "consolidatedCount": self.consolidated_context.len() + 1,
        })
    }
}

impl ChatQueueItem {
    /// Consolidate this item with others from the same room.
    /// Returns a new ChatQueueItem with merged context.
    ///
    /// Self = latest message (trigger). Others = prior context.
    /// The AI responds to the trigger but has full room context.
    pub fn consolidate_with_items(&self, others: &[&ChatQueueItem]) -> ChatQueueItem {
        // Collect all messages (self + others), sort by timestamp
        let mut all_messages: Vec<&ChatQueueItem> = others.to_vec();
        all_messages.push(self);
        all_messages.sort_by_key(|m| m.timestamp);

        // Latest message is the trigger
        let trigger = all_messages.last().unwrap();
        let prior = &all_messages[..all_messages.len() - 1];

        // Build consolidated context
        let mut context: Vec<ConsolidatedContext> = self.consolidated_context.clone();
        for msg in prior {
            context.push(ConsolidatedContext {
                sender_id: msg.sender_id,
                sender_name: msg.sender_name.clone(),
                content: msg.content.clone(),
                timestamp: msg.timestamp,
            });
        }
        context.sort_by_key(|c| c.timestamp);

        // Highest priority, carry forward mentions
        let max_priority = all_messages.iter()
            .map(|m| m.priority)
            .fold(f32::NEG_INFINITY, f32::max);
        let has_mentions = self.mentions || others.iter().any(|m| m.mentions);

        ChatQueueItem {
            id: trigger.id,
            room_id: trigger.room_id,
            content: trigger.content.clone(),
            sender_id: trigger.sender_id,
            sender_name: trigger.sender_name.clone(),
            sender_type: trigger.sender_type,
            mentions: has_mentions,
            timestamp: trigger.timestamp,
            enqueued_at: self.enqueued_at, // Preserve original enqueue time for aging
            priority: max_priority,
            consolidated_context: context,
        }
    }
}

//=============================================================================
// TASK QUEUE ITEM
//=============================================================================

/// Task: dependency-aware, overdue urgency, related-task consolidation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskQueueItem {
    pub id: Uuid,
    pub task_id: Uuid,
    pub assignee_id: Uuid,
    pub created_by: Uuid,
    pub task_domain: String,
    pub task_type: String,
    pub context_id: Uuid,
    pub description: String,
    pub priority: f32,
    pub status: String, // "pending", "in_progress", "completed", "blocked"
    pub timestamp: u64,
    pub enqueued_at: u64,
    pub due_date: Option<u64>,
    pub estimated_duration: Option<u64>,
    pub depends_on: Vec<Uuid>,
    pub blocked_by: Vec<Uuid>,
    pub related_task_ids: Vec<Uuid>,
    pub consolidated_count: u32,
}

impl QueueItemBehavior for TaskQueueItem {
    fn item_type(&self) -> &'static str { "task" }
    fn domain(&self) -> ActivityDomain { ActivityDomain::Background }
    fn id(&self) -> Uuid { self.id }
    fn timestamp(&self) -> u64 { self.timestamp }
    fn base_priority(&self) -> f32 { self.priority }

    // Urgent if past due date
    fn is_urgent(&self) -> bool {
        self.due_date.is_some_and(|d| d < now_ms())
    }

    // Don't kick in-progress tasks
    fn can_be_kicked(&self) -> bool {
        self.status != "in_progress"
    }

    // Blocked tasks have zero kick resistance (kick blocked tasks first)
    fn kick_resistance(&self, now_ms: u64, enqueued_at_ms: u64) -> f32 {
        if !self.blocked_by.is_empty() {
            return 0.0;
        }
        self.effective_priority(now_ms, enqueued_at_ms)
    }

    // Consolidate related tasks: same task domain AND same context
    fn should_consolidate_with(&self, other: &dyn QueueItemBehavior) -> bool {
        if other.item_type() != "task" {
            return false;
        }
        if let Some(other_task) = other.as_any().downcast_ref::<TaskQueueItem>() {
            other_task.task_domain == self.task_domain
                && other_task.context_id == self.context_id
        } else {
            false
        }
    }

    fn as_any(&self) -> &dyn Any { self }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "task",
            "id": self.id.to_string(),
            "taskId": self.task_id.to_string(),
            "assigneeId": self.assignee_id.to_string(),
            "createdBy": self.created_by.to_string(),
            "taskDomain": self.task_domain,
            "taskType": self.task_type,
            "contextId": self.context_id.to_string(),
            "description": self.description,
            "priority": self.priority,
            "status": self.status,
            "timestamp": self.timestamp,
            "dueDate": self.due_date,
            "estimatedDuration": self.estimated_duration,
            "dependsOn": self.depends_on.iter().map(|u| u.to_string()).collect::<Vec<_>>(),
            "blockedBy": self.blocked_by.iter().map(|u| u.to_string()).collect::<Vec<_>>(),
            "relatedTaskIds": self.related_task_ids.iter().map(|u| u.to_string()).collect::<Vec<_>>(),
            "consolidatedCount": self.consolidated_count,
        })
    }
}

impl TaskQueueItem {
    /// Consolidate related tasks: keep highest priority as primary.
    pub fn consolidate_with_items(&self, others: &[&TaskQueueItem]) -> TaskQueueItem {
        let mut all_tasks: Vec<&TaskQueueItem> = others.to_vec();
        all_tasks.push(self);
        all_tasks.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal));

        let primary = all_tasks[0];

        let related: Vec<Uuid> = all_tasks.iter()
            .filter(|t| t.id != primary.id)
            .map(|t| t.task_id)
            .collect();

        TaskQueueItem {
            id: primary.id,
            task_id: primary.task_id,
            assignee_id: primary.assignee_id,
            created_by: primary.created_by,
            task_domain: primary.task_domain.clone(),
            task_type: primary.task_type.clone(),
            context_id: primary.context_id,
            description: primary.description.clone(),
            priority: primary.priority,
            status: primary.status.clone(),
            timestamp: primary.timestamp,
            enqueued_at: self.enqueued_at,
            due_date: primary.due_date,
            estimated_duration: primary.estimated_duration,
            depends_on: primary.depends_on.clone(),
            blocked_by: primary.blocked_by.clone(),
            related_task_ids: related,
            consolidated_count: all_tasks.len() as u32,
        }
    }
}

//=============================================================================
// IPC REQUEST TYPES — For receiving items from TypeScript
//=============================================================================

/// IPC request to enqueue any item type. Discriminated by `item_type` field.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "item_type")]
#[ts(export, export_to = "../../../shared/generated/persona/ChannelEnqueueRequest.ts")]
pub enum ChannelEnqueueRequest {
    #[serde(rename = "voice")]
    Voice {
        id: String,
        room_id: String,
        content: String,
        sender_id: String,
        sender_name: String,
        sender_type: String,
        voice_session_id: String,
        #[ts(type = "number")]
        timestamp: u64,
        priority: f32,
    },
    #[serde(rename = "chat")]
    Chat {
        id: String,
        room_id: String,
        content: String,
        sender_id: String,
        sender_name: String,
        sender_type: String,
        mentions: bool,
        #[ts(type = "number")]
        timestamp: u64,
        priority: f32,
    },
    #[serde(rename = "task")]
    Task {
        id: String,
        task_id: String,
        assignee_id: String,
        created_by: String,
        task_domain: String,
        task_type: String,
        context_id: String,
        description: String,
        priority: f32,
        status: String,
        #[ts(type = "number")]
        timestamp: u64,
        due_date: Option<u64>,
        estimated_duration: Option<u64>,
        depends_on: Vec<String>,
        blocked_by: Vec<String>,
    },
}

impl ChannelEnqueueRequest {
    /// Convert IPC request to a boxed queue item.
    /// Returns Err if UUIDs are invalid.
    pub fn to_queue_item(&self) -> Result<Box<dyn QueueItemBehavior>, String> {
        let now = now_ms();
        match self {
            ChannelEnqueueRequest::Voice {
                id, room_id, content, sender_id, sender_name,
                sender_type, voice_session_id, timestamp, priority,
            } => {
                Ok(Box::new(VoiceQueueItem {
                    id: parse_uuid(id, "id")?,
                    room_id: parse_uuid(room_id, "room_id")?,
                    content: content.clone(),
                    sender_id: parse_uuid(sender_id, "sender_id")?,
                    sender_name: sender_name.clone(),
                    sender_type: parse_sender_type(sender_type)?,
                    voice_session_id: parse_uuid(voice_session_id, "voice_session_id")?,
                    timestamp: *timestamp,
                    enqueued_at: now,
                    priority: *priority,
                }))
            }
            ChannelEnqueueRequest::Chat {
                id, room_id, content, sender_id, sender_name,
                sender_type, mentions, timestamp, priority,
            } => {
                Ok(Box::new(ChatQueueItem {
                    id: parse_uuid(id, "id")?,
                    room_id: parse_uuid(room_id, "room_id")?,
                    content: content.clone(),
                    sender_id: parse_uuid(sender_id, "sender_id")?,
                    sender_name: sender_name.clone(),
                    sender_type: parse_sender_type(sender_type)?,
                    mentions: *mentions,
                    timestamp: *timestamp,
                    enqueued_at: now,
                    priority: *priority,
                    consolidated_context: Vec::new(),
                }))
            }
            ChannelEnqueueRequest::Task {
                id, task_id, assignee_id, created_by, task_domain,
                task_type, context_id, description, priority, status,
                timestamp, due_date, estimated_duration, depends_on, blocked_by,
            } => {
                let depends_on_uuids: Result<Vec<Uuid>, String> = depends_on.iter()
                    .map(|s| parse_uuid(s, "depends_on"))
                    .collect();
                let blocked_by_uuids: Result<Vec<Uuid>, String> = blocked_by.iter()
                    .map(|s| parse_uuid(s, "blocked_by"))
                    .collect();

                Ok(Box::new(TaskQueueItem {
                    id: parse_uuid(id, "id")?,
                    task_id: parse_uuid(task_id, "task_id")?,
                    assignee_id: parse_uuid(assignee_id, "assignee_id")?,
                    created_by: parse_uuid(created_by, "created_by")?,
                    task_domain: task_domain.clone(),
                    task_type: task_type.clone(),
                    context_id: parse_uuid(context_id, "context_id")?,
                    description: description.clone(),
                    priority: *priority,
                    status: status.clone(),
                    timestamp: *timestamp,
                    enqueued_at: now,
                    due_date: *due_date,
                    estimated_duration: *estimated_duration,
                    depends_on: depends_on_uuids?,
                    blocked_by: blocked_by_uuids?,
                    related_task_ids: Vec::new(),
                    consolidated_count: 1,
                }))
            }
        }
    }
}

fn parse_uuid(s: &str, field: &str) -> Result<Uuid, String> {
    Uuid::parse_str(s).map_err(|e| format!("Invalid UUID for {field}: {e}"))
}

fn parse_sender_type(s: &str) -> Result<SenderType, String> {
    match s {
        "human" => Ok(SenderType::Human),
        "persona" => Ok(SenderType::Persona),
        "agent" => Ok(SenderType::Agent),
        "system" => Ok(SenderType::System),
        _ => Err(format!("Invalid sender_type: {s}")),
    }
}

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_voice() -> VoiceQueueItem {
        VoiceQueueItem {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            content: "Hello from voice".into(),
            sender_id: Uuid::new_v4(),
            sender_name: "Joel".into(),
            sender_type: SenderType::Human,
            voice_session_id: Uuid::new_v4(),
            timestamp: now_ms(),
            enqueued_at: now_ms(),
            priority: 1.0,
        }
    }

    fn make_chat(room_id: Uuid, mentions: bool, priority: f32) -> ChatQueueItem {
        ChatQueueItem {
            id: Uuid::new_v4(),
            room_id,
            content: "Chat message".into(),
            sender_id: Uuid::new_v4(),
            sender_name: "User".into(),
            sender_type: SenderType::Human,
            mentions,
            timestamp: now_ms(),
            enqueued_at: now_ms(),
            priority,
            consolidated_context: Vec::new(),
        }
    }

    fn make_task(domain: &str, context_id: Uuid) -> TaskQueueItem {
        TaskQueueItem {
            id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            assignee_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            task_domain: domain.into(),
            task_type: "review".into(),
            context_id,
            description: "Test task".into(),
            priority: 0.5,
            status: "pending".into(),
            timestamp: now_ms(),
            enqueued_at: now_ms(),
            due_date: None,
            estimated_duration: None,
            depends_on: Vec::new(),
            blocked_by: Vec::new(),
            related_task_ids: Vec::new(),
            consolidated_count: 1,
        }
    }

    #[test]
    fn test_voice_always_urgent() {
        let voice = make_voice();
        assert!(voice.is_urgent());
        assert!(!voice.can_be_kicked());
        assert_eq!(voice.base_priority(), 1.0);
        assert_eq!(voice.max_aging_boost(), 0.0);
        assert_eq!(voice.item_type(), "voice");
        assert_eq!(voice.domain(), ActivityDomain::Audio);
    }

    #[test]
    fn test_chat_mention_urgency() {
        let room = Uuid::new_v4();
        let with_mention = make_chat(room, true, 0.8);
        let without_mention = make_chat(room, false, 0.5);

        assert!(with_mention.is_urgent());
        assert!(!without_mention.is_urgent());
    }

    #[test]
    fn test_chat_same_room_consolidation() {
        let room = Uuid::new_v4();
        let other_room = Uuid::new_v4();
        let chat1 = make_chat(room, false, 0.5);
        let chat2 = make_chat(room, false, 0.7);
        let chat3 = make_chat(other_room, false, 0.6);

        // Same room: should consolidate
        assert!(chat1.should_consolidate_with(&chat2));
        // Different room: should NOT consolidate
        assert!(!chat1.should_consolidate_with(&chat3));
    }

    #[test]
    fn test_chat_consolidation_merges() {
        let room = Uuid::new_v4();
        let mut chat1 = make_chat(room, false, 0.5);
        chat1.content = "First message".into();
        chat1.timestamp = 1000;

        let mut chat2 = make_chat(room, true, 0.8);
        chat2.content = "Second message with @mention".into();
        chat2.timestamp = 2000;

        let consolidated = chat1.consolidate_with_items(&[&chat2]);

        // Trigger is the latest message (chat2, timestamp 2000)
        assert_eq!(consolidated.timestamp, 2000);
        assert_eq!(consolidated.content, "Second message with @mention");
        // Highest priority
        assert_eq!(consolidated.priority, 0.8);
        // Mentions carried forward
        assert!(consolidated.mentions);
        // Prior message is in context
        assert_eq!(consolidated.consolidated_context.len(), 1);
        assert_eq!(consolidated.consolidated_context[0].content, "First message");
    }

    #[test]
    fn test_task_overdue_urgency() {
        let ctx = Uuid::new_v4();
        let mut task = make_task("code", ctx);
        assert!(!task.is_urgent()); // No due date

        task.due_date = Some(now_ms() + 60_000); // Due in 1 min
        assert!(!task.is_urgent()); // Not yet overdue

        task.due_date = Some(now_ms() - 1000); // 1 second overdue
        assert!(task.is_urgent());
    }

    #[test]
    fn test_task_in_progress_not_kickable() {
        let ctx = Uuid::new_v4();
        let mut task = make_task("code", ctx);
        assert!(task.can_be_kicked()); // pending

        task.status = "in_progress".into();
        assert!(!task.can_be_kicked()); // in progress
    }

    #[test]
    fn test_task_same_domain_context_consolidation() {
        let ctx = Uuid::new_v4();
        let task1 = make_task("code", ctx);
        let task2 = make_task("code", ctx);
        let task3 = make_task("memory", ctx);
        let task4 = make_task("code", Uuid::new_v4());

        // Same domain + context: consolidate
        assert!(task1.should_consolidate_with(&task2));
        // Different domain: no
        assert!(!task1.should_consolidate_with(&task3));
        // Different context: no
        assert!(!task1.should_consolidate_with(&task4));
    }

    #[test]
    fn test_effective_priority_aging() {
        let room = Uuid::new_v4();
        let chat = make_chat(room, false, 0.3);

        let now = now_ms();
        let enqueued = now; // Just enqueued — no aging
        let p0 = chat.effective_priority(now, enqueued);
        assert!((p0 - 0.3).abs() < 0.01, "No aging expected, got {p0}");

        // After 15s (half of 30s aging window) → 0.25 boost
        let p15 = chat.effective_priority(now + 15_000, enqueued);
        assert!((p15 - 0.55).abs() < 0.05, "Expected ~0.55, got {p15}");

        // After 30s (full aging) → 0.5 boost → capped at 0.8
        let p30 = chat.effective_priority(now + 30_000, enqueued);
        assert!((p30 - 0.8).abs() < 0.05, "Expected ~0.8, got {p30}");

        // After 60s → still capped at 0.8 (max boost is 0.5)
        let p60 = chat.effective_priority(now + 60_000, enqueued);
        assert!((p60 - 0.8).abs() < 0.05, "Expected ~0.8 (capped), got {p60}");
    }

    #[test]
    fn test_voice_no_aging() {
        let voice = make_voice();
        let now = now_ms();
        let p0 = voice.effective_priority(now, now);
        let p60 = voice.effective_priority(now + 60_000, now);
        assert_eq!(p0, 1.0);
        assert_eq!(p60, 1.0); // No aging boost
    }

    #[test]
    fn test_voice_does_not_consolidate_with_chat() {
        let voice = make_voice();
        let chat = make_chat(Uuid::new_v4(), false, 0.5);
        assert!(!voice.should_consolidate_with(&chat));
    }

    #[test]
    fn test_ipc_request_roundtrip() {
        let req = ChannelEnqueueRequest::Chat {
            id: Uuid::new_v4().to_string(),
            room_id: Uuid::new_v4().to_string(),
            content: "Hello".into(),
            sender_id: Uuid::new_v4().to_string(),
            sender_name: "Joel".into(),
            sender_type: "human".into(),
            mentions: true,
            timestamp: now_ms(),
            priority: 0.8,
        };

        let item = req.to_queue_item().unwrap();
        assert_eq!(item.item_type(), "chat");
        assert!(item.is_urgent()); // mentions = true
        assert_eq!(item.domain(), ActivityDomain::Chat);
    }
}
