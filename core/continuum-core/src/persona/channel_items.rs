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
// MEDIA ITEM (for native multimodal — images, audio attached to a message)
//=============================================================================

/// One media attachment riding with a chat / voice item through Rust IPC.
///
/// We deliberately omit `base64` from this hop: chat-send already externalized
/// the bytes to disk via `MediaBlobService.externalize`, and PRG re-reads from
/// disk via `blob_hash` on the way back into the model. Sending base64 through
/// the inbox round-trip would balloon the IPC payload for no win — the disk
/// fetch is already on the critical path for the cache-hit case anyway.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/MediaItemRequest.ts"
)]
pub struct MediaItemRequest {
    /// "image", "audio", etc. Mirrors the TS `MediaItemLite.type`.
    #[serde(rename = "type")]
    pub kind: String,
    #[ts(optional)]
    pub mime_type: Option<String>,
    /// `sha256:hex` content-addressed handle resolvable via MediaBlobService.
    #[ts(optional)]
    pub blob_hash: Option<String>,
    /// Optional remote URL fallback (e.g. CDN-hosted asset).
    #[ts(optional)]
    pub url: Option<String>,
    /// Pre-computed text description from VisionDescriptionService.
    /// Lets text-only personas downstream get the bridge text without re-running inference.
    #[ts(optional)]
    pub description: Option<String>,
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
    #[serde(default)]
    pub media: Vec<MediaItemRequest>,
}

impl QueueItemBehavior for VoiceQueueItem {
    fn item_type(&self) -> &'static str {
        "voice"
    }
    fn domain(&self) -> ActivityDomain {
        ActivityDomain::Audio
    }
    fn id(&self) -> Uuid {
        self.id
    }
    fn timestamp(&self) -> u64 {
        self.timestamp
    }
    fn base_priority(&self) -> f32 {
        1.0
    }

    // No aging needed — already max priority
    fn aging_boost_ms(&self) -> f32 {
        30_000.0
    }
    fn max_aging_boost(&self) -> f32 {
        0.0
    }

    // Always urgent — bypasses cognitive scheduler
    fn is_urgent(&self) -> bool {
        true
    }

    // Never kicked — dropping voice mid-conversation is unacceptable
    fn can_be_kicked(&self) -> bool {
        false
    }
    fn kick_resistance(&self, _now_ms: u64, _enqueued_at_ms: u64) -> f32 {
        f32::INFINITY
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

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
            "media": self.media,
        })
    }
}

//=============================================================================
// CHAT QUEUE ITEM
//=============================================================================

/// Context from a prior message consolidated into this chat item.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ConsolidatedContext.ts"
)]
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
///
/// ## Lazy-cached derived state
///
/// Per `[[pass-by-reference-lazy-metadata-with-data]]`: the item carries
/// `OnceLock<Arc<...>>` cells for expensive derived state (currently
/// `embedding_cell`; future: STT for audio attachments, RAG chunks).
/// First consumer that calls `embedding()` triggers compute; every
/// subsequent consumer (multiple personas in the same room) gets the
/// cached Arc clone. The cell is `#[serde(skip)]` so wire format stays
/// clean — derived state is local-substrate-only, never crosses the IPC
/// boundary.
///
/// The struct's `Clone` impl creates a FRESH cell — clones get their own
/// cache. In practice items are shared via `Arc<ChatQueueItem>` (queue
/// pop returns the same Arc), so direct struct-level clones are rare;
/// when they happen they're a deliberate "I want a fresh copy" signal
/// and the fresh cache aligns with that intent.
#[derive(Debug, Serialize, Deserialize)]
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
    /// Native multimodal attachments riding with this message (images, audio).
    /// PRG resolves blob_hash → bytes on the model-input side.
    #[serde(default)]
    pub media: Vec<MediaItemRequest>,
    /// Lazy-cached content embedding. First demand triggers compute via
    /// `compute_chat_embedding`; every subsequent demand returns the
    /// cached Arc clone. Skipped from serialization (derived state).
    ///
    /// `pub` so cross-module struct literals can initialize it with
    /// `std::sync::OnceLock::new()`; consumers should call
    /// `ChatQueueItem::embedding()` rather than touching the cell
    /// directly. `OnceLock::set()` external use is safe (returns Err
    /// if already populated, can't corrupt the cache).
    #[serde(skip, default)]
    pub embedding_cell: std::sync::OnceLock<std::sync::Arc<Vec<f32>>>,

    /// Per-item compute-call counter, used by architecture proofs to
    /// witness `[[shared-decode-per-persona-perspective]]` structurally:
    /// N concurrent persona reads on this item must increment this by
    /// exactly 1 (compute fires once; subsequent demands hit the cache).
    ///
    /// Per-item (not global) so concurrent integration tests don't
    /// contaminate each other's measurements — the doctrine claim is
    /// "compute fires once per ITEM," and the witness is per-item.
    /// Arc-shared via `Arc<ChatQueueItem>` automatically; clones get a
    /// fresh counter (consistent with the fresh `embedding_cell` on
    /// clone — Clone semantics align across both lazy-state fields).
    ///
    /// `#[cfg(any(test, feature = "test-fixtures"))]` so production
    /// binaries cannot link the field — zero hot-path cost.
    #[cfg(any(test, feature = "test-fixtures"))]
    #[serde(skip, default)]
    pub compute_calls: std::sync::atomic::AtomicUsize,
}

impl Clone for ChatQueueItem {
    /// Clone creates a fresh `embedding_cell` (and a fresh
    /// `compute_calls` counter when the instrumentation feature is on).
    ///
    /// ⚠️ **Foot-gun warning**: cloning silently drops the lazy cache.
    /// If you clone a `ChatQueueItem` whose `embedding_cell` is
    /// populated and then call `.embedding()` on the clone, the compute
    /// fires AGAIN — you pay the cost a second time. The doctrine
    /// (`[[pass-by-reference-lazy-metadata-with-data]]`) says items
    /// should be SHARED via `Arc<ChatQueueItem>`, not cloned. Clone
    /// here is for the rare "I genuinely want a separate copy" case
    /// (e.g. consolidation building a new anchor from absorbed items).
    ///
    /// If you find yourself reaching for `.clone()` to pass an item
    /// to another consumer, you almost certainly want `Arc::clone(&arc)`
    /// instead — that's the cache-preserving path.
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            room_id: self.room_id,
            content: self.content.clone(),
            sender_id: self.sender_id,
            sender_name: self.sender_name.clone(),
            sender_type: self.sender_type,
            mentions: self.mentions,
            timestamp: self.timestamp,
            enqueued_at: self.enqueued_at,
            priority: self.priority,
            consolidated_context: self.consolidated_context.clone(),
            media: self.media.clone(),
            embedding_cell: std::sync::OnceLock::new(),
            #[cfg(any(test, feature = "test-fixtures"))]
            compute_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl ChatQueueItem {
    /// Return this item's content embedding. First call computes via
    /// `compute_chat_embedding(&self.content)` and caches; subsequent
    /// calls return the cached `Arc<Vec<f32>>` directly. Cost:
    ///
    /// - First demand: one decoder pass (typically the RAG/embedding
    ///   model on the substrate's `EmbeddingModule`)
    /// - Subsequent demands across N consumers: `Arc::clone`-cheap;
    ///   zero re-compute, regardless of persona count
    ///
    /// Per `[[shared-decode-per-persona-perspective]]`: this is the
    /// substrate-shared decode. Per-persona ranking against this
    /// embedding is the cheap per-persona perspective layer above it
    /// (lands in Delta 4 as `PersonaChannelView::interpret`).
    pub fn embedding(&self) -> std::sync::Arc<Vec<f32>> {
        self.embedding_cell
            .get_or_init(|| {
                // Per-item compute-call instrumentation (test-fixtures
                // only). Bumped INSIDE the OnceLock closure so it
                // tracks closure-execution count, not call-site count.
                // `std::sync::OnceLock::get_or_init` guarantees only
                // one closure runs even under N-way contention (uses
                // `Once::call_once_force` internally) — so this counter
                // pins the doctrine claim "compute fires once per item
                // regardless of concurrent reads."
                #[cfg(any(test, feature = "test-fixtures"))]
                self.compute_calls
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                std::sync::Arc::new(compute_chat_embedding(&self.content))
            })
            .clone()
    }

    /// Test-only accessor: how many times has `compute_chat_embedding`
    /// fired for THIS item? Used by architecture proofs to witness
    /// `[[shared-decode-per-persona-perspective]]` structurally.
    ///
    /// Per-item (not global) so concurrent integration tests don't
    /// contaminate each other's measurements.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn compute_call_count(&self) -> usize {
        self.compute_calls
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Pure content → embedding decoder. **SEAM, not the production
/// embedding path.** PR A ships a deterministic 8-dim hash-derived
/// placeholder so the lazy-cell substrate is observable in tests; the
/// real production path routes through the existing `EmbeddingModule`
/// at `modules/embedding.rs` and lands in a follow-up PR (tracked as
/// task #246 — substrate-wide `ChatItem::embedding()` async + wired
/// through ai::embedding).
///
/// Why this matters for the doctrine: the `[[shared-decode-per-
/// persona-perspective]]` claim is currently proven against this
/// placeholder. The lazy-cell PROPERTY (one OnceLock fires once,
/// N consumers share the Arc) holds under any pure decoder, so the
/// architectural seam is sound. But the COST claim (compute is
/// expensive; sharing it saves N-1× of compute) only becomes
/// meaningful when the real embedding compute is in place — until
/// then, the "expensive" decode is microsecond-cheap and the saving
/// is theoretical. PR D / #246 closure must keep the lazy-cell
/// contract AND route through EmbeddingModule.
///
/// CRITICAL property: this function is PURE. Caching is on the item's
/// OnceLock cell, not in the function. That keeps the decoder
/// trivially testable and the cache observable (`Arc::ptr_eq` between
/// two calls on the same item witnesses the share).
fn compute_chat_embedding(content: &str) -> Vec<f32> {
    // Deterministic 8-dim placeholder: rotate content bytes into f32
    // buckets so different content produces visibly different vectors.
    // Real implementation routes through ai::embedding when the
    // adapter integration lands (task #246). Per-item instrumentation
    // lives on `ChatQueueItem::compute_calls`, NOT here, so this
    // function stays pure.
    let mut v = vec![0.0_f32; 8];
    for (i, b) in content.bytes().enumerate() {
        v[i % 8] += (b as f32) / 255.0;
    }
    v
}

impl QueueItemBehavior for ChatQueueItem {
    fn item_type(&self) -> &'static str {
        "chat"
    }
    fn domain(&self) -> ActivityDomain {
        ActivityDomain::Chat
    }
    fn id(&self) -> Uuid {
        self.id
    }
    fn timestamp(&self) -> u64 {
        self.timestamp
    }
    fn base_priority(&self) -> f32 {
        self.priority
    }

    // Standard RTOS aging from defaults (30s to reach +0.5 boost)

    // Urgent only if persona is directly mentioned by name
    fn is_urgent(&self) -> bool {
        self.mentions
    }

    // Consolidate with other chat items from the SAME ROOM. The
    // `should_consolidate_with` default impl on the trait derives this
    // from `consolidation_key` — same-key items merge.
    fn consolidation_key(&self) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        // Mix in the item type first so chat-with-room=X cannot
        // key-collide with task-with-context=X (per trait docstring).
        "chat".hash(&mut h);
        self.room_id.hash(&mut h);
        Some(h.finish())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

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
            "media": self.media,
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

        // Trigger-selection strategy: if any item in this consolidation set
        // carries media (an image or audio attachment), the latest
        // media-bearing item becomes the trigger. Only when no item has
        // media does the trigger fall back to the strict "latest by
        // timestamp" rule.
        //
        // Why: prior to this rule, the trigger was always the most recent
        // message by wall-clock time. In an active room where multiple
        // personas reply to each other, an image sent at T₀ would become
        // a non-trigger by T₀+2s because text replies landed after it.
        // Media on non-trigger items was dropped (`media: trigger.media`),
        // so the vision/audio bytes never reached the model. The user
        // experience was "I shared an image and the AIs talked about
        // something unrelated."
        //
        // This strategy restores the human-intuitive behavior: when
        // someone shares visual/audible content in a room, the persona
        // responds to THAT as the primary signal, with surrounding text
        // chatter as consolidated_context. Per-item-type polymorphism —
        // VideoFrameQueueItem / GameMoveQueueItem can choose different
        // trigger rules appropriate to their domain.
        let latest_with_media = all_messages.iter().rev().find(|m| !m.media.is_empty());
        let trigger = latest_with_media
            .copied()
            .unwrap_or(*all_messages.last().unwrap());
        let prior: Vec<&ChatQueueItem> = all_messages
            .iter()
            .copied()
            .filter(|m| m.id != trigger.id)
            .collect();

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
        let max_priority = all_messages
            .iter()
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
            // Carry the trigger's media (the message we're actually responding to).
            // Prior consolidated messages had their own context-only role; their
            // attachments would compete for the model's vision budget without
            // adding usable signal for the current turn.
            media: trigger.media.clone(),
            // Consolidated item's content differs from any individual
            // member's content, so a fresh embedding cell is correct —
            // it'll be computed on first demand against the new content.
            embedding_cell: std::sync::OnceLock::new(),
            #[cfg(any(test, feature = "test-fixtures"))]
            compute_calls: std::sync::atomic::AtomicUsize::new(0),
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
    fn item_type(&self) -> &'static str {
        "task"
    }
    fn domain(&self) -> ActivityDomain {
        ActivityDomain::Background
    }
    fn id(&self) -> Uuid {
        self.id
    }
    fn timestamp(&self) -> u64 {
        self.timestamp
    }
    fn base_priority(&self) -> f32 {
        self.priority
    }

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

    // Consolidate related tasks: same task domain AND same context.
    // The `should_consolidate_with` default impl on the trait derives
    // this from `consolidation_key` — same-key items merge.
    fn consolidation_key(&self) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        "task".hash(&mut h);
        self.task_domain.hash(&mut h);
        self.context_id.hash(&mut h);
        Some(h.finish())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

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
        all_tasks.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let primary = all_tasks[0];

        let related: Vec<Uuid> = all_tasks
            .iter()
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
// CODE QUEUE ITEM
//=============================================================================

/// Code: workspace-scoped coding tasks. Not urgent, never kicked, slow aging.
/// Consolidates multiple requests for the same workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeQueueItem {
    pub id: Uuid,
    pub room_id: Uuid,
    pub persona_id: Uuid,
    pub task_description: String,
    pub workspace_handle: String,
    pub priority: f32,
    pub is_review: bool,
    pub timestamp: u64,
    pub enqueued_at: u64,
}

impl QueueItemBehavior for CodeQueueItem {
    fn item_type(&self) -> &'static str {
        "code"
    }
    fn domain(&self) -> ActivityDomain {
        ActivityDomain::Code
    }
    fn id(&self) -> Uuid {
        self.id
    }
    fn timestamp(&self) -> u64 {
        self.timestamp
    }
    fn base_priority(&self) -> f32 {
        self.priority
    }

    // Slow aging — coding tasks are long-lived, 60s to reach max boost
    fn aging_boost_ms(&self) -> f32 {
        60_000.0
    }

    // Not urgent — coding is not real-time
    fn is_urgent(&self) -> bool {
        false
    }

    // Never kicked — don't drop active coding work
    fn can_be_kicked(&self) -> bool {
        false
    }
    fn kick_resistance(&self, _now_ms: u64, _enqueued_at_ms: u64) -> f32 {
        f32::INFINITY
    }

    // Consolidate multiple requests for the same workspace
    fn should_consolidate_with(&self, other: &dyn QueueItemBehavior) -> bool {
        if other.item_type() != "code" {
            return false;
        }
        if let Some(other_code) = other.as_any().downcast_ref::<CodeQueueItem>() {
            other_code.workspace_handle == self.workspace_handle
        } else {
            false
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "code",
            "id": self.id.to_string(),
            "roomId": self.room_id.to_string(),
            "personaId": self.persona_id.to_string(),
            "taskDescription": self.task_description,
            "workspaceHandle": self.workspace_handle,
            "priority": self.priority,
            "isReview": self.is_review,
            "timestamp": self.timestamp,
        })
    }
}

//=============================================================================
// IPC REQUEST TYPES — For receiving items from TypeScript
//=============================================================================

/// IPC request to enqueue any item type. Discriminated by `item_type` field.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "item_type")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ChannelEnqueueRequest.ts"
)]
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
        #[serde(default)]
        media: Vec<MediaItemRequest>,
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
        #[serde(default)]
        media: Vec<MediaItemRequest>,
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
    #[serde(rename = "code")]
    Code {
        id: String,
        room_id: String,
        persona_id: String,
        task_description: String,
        workspace_handle: String,
        priority: f32,
        is_review: bool,
        #[ts(type = "number")]
        timestamp: u64,
    },
}

impl ChannelEnqueueRequest {
    /// Convert IPC request to a boxed queue item.
    /// Returns Err if UUIDs are invalid.
    pub fn to_queue_item(&self) -> Result<std::sync::Arc<dyn QueueItemBehavior>, String> {
        let now = now_ms();
        match self {
            ChannelEnqueueRequest::Voice {
                id,
                room_id,
                content,
                sender_id,
                sender_name,
                sender_type,
                voice_session_id,
                timestamp,
                priority,
                media,
            } => Ok(std::sync::Arc::new(VoiceQueueItem {
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
                media: media.clone(),
            })),
            ChannelEnqueueRequest::Chat {
                id,
                room_id,
                content,
                sender_id,
                sender_name,
                sender_type,
                mentions,
                timestamp,
                priority,
                media,
            } => Ok(std::sync::Arc::new(ChatQueueItem {
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
                media: media.clone(),
                embedding_cell: std::sync::OnceLock::new(),
                #[cfg(any(test, feature = "test-fixtures"))]
                compute_calls: std::sync::atomic::AtomicUsize::new(0),
            })),
            ChannelEnqueueRequest::Code {
                id,
                room_id,
                persona_id,
                task_description,
                workspace_handle,
                priority,
                is_review,
                timestamp,
            } => Ok(std::sync::Arc::new(CodeQueueItem {
                id: parse_uuid(id, "id")?,
                room_id: parse_uuid(room_id, "room_id")?,
                persona_id: parse_uuid(persona_id, "persona_id")?,
                task_description: task_description.clone(),
                workspace_handle: workspace_handle.clone(),
                priority: *priority,
                is_review: *is_review,
                timestamp: *timestamp,
                enqueued_at: now,
            })),
            ChannelEnqueueRequest::Task {
                id,
                task_id,
                assignee_id,
                created_by,
                task_domain,
                task_type,
                context_id,
                description,
                priority,
                status,
                timestamp,
                due_date,
                estimated_duration,
                depends_on,
                blocked_by,
            } => {
                let depends_on_uuids: Result<Vec<Uuid>, String> = depends_on
                    .iter()
                    .map(|s| parse_uuid(s, "depends_on"))
                    .collect();
                let blocked_by_uuids: Result<Vec<Uuid>, String> = blocked_by
                    .iter()
                    .map(|s| parse_uuid(s, "blocked_by"))
                    .collect();

                Ok(std::sync::Arc::new(TaskQueueItem {
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
            sender_name: "test-user".into(),
            sender_type: SenderType::Human,
            voice_session_id: Uuid::new_v4(),
            timestamp: now_ms(),
            enqueued_at: now_ms(),
            priority: 1.0,
            media: Vec::new(),
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
            media: Vec::new(),
            embedding_cell: std::sync::OnceLock::new(),
            #[cfg(any(test, feature = "test-fixtures"))]
            compute_calls: std::sync::atomic::AtomicUsize::new(0),
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
        assert_eq!(
            consolidated.consolidated_context[0].content,
            "First message"
        );
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
        assert!(
            (p60 - 0.8).abs() < 0.05,
            "Expected ~0.8 (capped), got {p60}"
        );
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
            sender_name: "test-user".into(),
            sender_type: "human".into(),
            mentions: true,
            timestamp: now_ms(),
            priority: 0.8,
            media: Vec::new(),
        };

        let item = req.to_queue_item().unwrap();
        assert_eq!(item.item_type(), "chat");
        assert!(item.is_urgent()); // mentions = true
        assert_eq!(item.domain(), ActivityDomain::Chat);
    }

    #[test]
    fn test_chat_media_roundtrip_through_request_and_json() {
        // Going-in: request with media → ChatQueueItem with media → to_json carries it.
        // This is the regression guard for the bug Joel hit on 2026-04-21:
        // vision bytes were enqueuing fine but the inbox round-trip stripped media,
        // so PRG always saw 0 attachments and Vision AI hallucinated descriptions.
        let blob_hash = "sha256:deadbeef".to_string();
        let req = ChannelEnqueueRequest::Chat {
            id: Uuid::new_v4().to_string(),
            room_id: Uuid::new_v4().to_string(),
            content: "look at this".into(),
            sender_id: Uuid::new_v4().to_string(),
            sender_name: "operator".into(),
            sender_type: "human".into(),
            mentions: false,
            timestamp: now_ms(),
            priority: 0.5,
            media: vec![MediaItemRequest {
                kind: "image".into(),
                mime_type: Some("image/jpeg".into()),
                blob_hash: Some(blob_hash.clone()),
                url: None,
                description: None,
            }],
        };

        let item = req.to_queue_item().unwrap();
        let json = item.to_json();
        let media = json.get("media").expect("media key present in JSON");
        let media_arr = media.as_array().expect("media is an array");
        assert_eq!(media_arr.len(), 1, "exactly one media item survives");
        let first = &media_arr[0];
        assert_eq!(first.get("type").and_then(|v| v.as_str()), Some("image"));
        assert_eq!(
            first.get("blobHash").and_then(|v| v.as_str()),
            Some(blob_hash.as_str())
        );
        assert_eq!(
            first.get("mimeType").and_then(|v| v.as_str()),
            Some("image/jpeg")
        );
    }

    /// proves: lazy embedding cell shares compute across consumers
    ///
    /// First `embedding()` call triggers compute; every subsequent call on
    /// the SAME `Arc<ChatQueueItem>` returns the cached `Arc<Vec<f32>>`.
    /// `Arc::ptr_eq` witnesses the share: if two calls returned different
    /// Arcs, the cell would be re-computing each time.
    ///
    /// Per `[[pass-by-reference-lazy-metadata-with-data]]`: this is the
    /// item-level witness for the doctrine — the data IS the cache.
    #[test]
    fn embedding_cell_returns_same_arc_across_calls() {
        let item = std::sync::Arc::new(ChatQueueItem {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            content: "hello world from the channel".into(),
            sender_id: Uuid::new_v4(),
            sender_name: "test-sender".into(),
            sender_type: SenderType::Human,
            mentions: false,
            timestamp: now_ms(),
            enqueued_at: now_ms(),
            priority: 0.5,
            consolidated_context: Vec::new(),
            media: Vec::new(),
            embedding_cell: std::sync::OnceLock::new(),
            #[cfg(any(test, feature = "test-fixtures"))]
            compute_calls: std::sync::atomic::AtomicUsize::new(0),
        });

        let e1 = item.embedding();
        let e2 = item.embedding();
        let e3 = item.embedding();

        // All three calls on the same item return the SAME underlying Arc.
        // If the cell were re-computing, ptr_eq would fail because each
        // compute would allocate a fresh Arc.
        assert!(
            std::sync::Arc::ptr_eq(&e1, &e2),
            "second embedding() call must return the cached Arc, not recompute"
        );
        assert!(
            std::sync::Arc::ptr_eq(&e2, &e3),
            "third embedding() call must return the cached Arc, not recompute"
        );

        // Same content → same embedding (verifies the placeholder decoder
        // is deterministic; real decoder will obviously also be
        // deterministic against the same input).
        assert_eq!(e1.len(), 8, "placeholder decoder produces 8-dim vectors");
    }

    /// proves: lazy embedding cell shares across multiple Arc consumers
    /// (multi-persona-in-room property)
    ///
    /// The item is shared via `Arc<ChatQueueItem>`. Cloning the Arc gives
    /// multiple handles to the SAME item. Each handle calling `embedding()`
    /// must hit the same OnceLock cell and return the same underlying
    /// `Arc<Vec<f32>>`. This is what makes "N personas in a room with M
    /// arrivals cost M × decode_cost, not N × M × decode_cost" true.
    #[test]
    fn embedding_cell_shared_across_arc_clones() {
        let item = std::sync::Arc::new(ChatQueueItem {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            content: "shared content across personas".into(),
            sender_id: Uuid::new_v4(),
            sender_name: "test-sender".into(),
            sender_type: SenderType::Human,
            mentions: false,
            timestamp: now_ms(),
            enqueued_at: now_ms(),
            priority: 0.5,
            consolidated_context: Vec::new(),
            media: Vec::new(),
            embedding_cell: std::sync::OnceLock::new(),
            #[cfg(any(test, feature = "test-fixtures"))]
            compute_calls: std::sync::atomic::AtomicUsize::new(0),
        });

        // Simulate 4 personas each holding their own Arc to the same item.
        let persona1 = std::sync::Arc::clone(&item);
        let persona2 = std::sync::Arc::clone(&item);
        let persona3 = std::sync::Arc::clone(&item);
        let persona4 = std::sync::Arc::clone(&item);

        let e1 = persona1.embedding();
        let e2 = persona2.embedding();
        let e3 = persona3.embedding();
        let e4 = persona4.embedding();

        // All four personas see the SAME cached embedding Arc — proves the
        // shared-decode property: the embedding was computed ONCE by the
        // first caller (whichever persona happened to demand it first),
        // and the other three got the cached share.
        assert!(std::sync::Arc::ptr_eq(&e1, &e2));
        assert!(std::sync::Arc::ptr_eq(&e2, &e3));
        assert!(std::sync::Arc::ptr_eq(&e3, &e4));
    }
}
