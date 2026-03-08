//! Per-Room Message Cache — echo chamber detection & content deduplication
//!
//! Replaces TS PersonaMessageGate's in-memory Map and ContentDeduplicator.
//! All operations are O(1) amortized via ring buffers and hash sets.
//!
//! Features:
//! - Per-room recent message ring buffer (echo chamber detection)
//! - Content hash dedup (prevents duplicate responses within time window)
//! - Configurable thresholds via compile-time constants

use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use uuid::Uuid;

// =============================================================================
// TIMING CONFIG (mirrors PersonaTimingConfig.ts — single source of truth in Rust)
// =============================================================================

/// Echo chamber: lookback window (2 minutes)
const ECHO_CHAMBER_WINDOW_MS: u64 = 2 * 60 * 1000;
/// Echo chamber: AI message count threshold before gating
const ECHO_CHAMBER_AI_THRESHOLD: usize = 5;
/// Max messages cached per room
const MAX_CACHED_PER_ROOM: usize = 50;
/// Content dedup: time window (60 seconds)
const CONTENT_DEDUP_WINDOW_MS: u64 = 60_000;
/// Content dedup: max tracked entries
const CONTENT_DEDUP_MAX_ENTRIES: usize = 50;

// =============================================================================
// CACHED MESSAGE (lightweight — only fields needed for gating decisions)
// =============================================================================

#[derive(Debug, Clone)]
pub struct CachedMessage {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub sender_type: SenderCategory,
    pub sender_name: String,
    pub content_text: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenderCategory {
    Human,
    AI,
}

// =============================================================================
// ECHO CHAMBER RESULT
// =============================================================================

#[derive(Debug, Clone)]
pub struct EchoChamberResult {
    pub is_echo_chamber: bool,
    pub ai_message_count: usize,
    pub has_human_recently: bool,
    pub check_time_us: u64,
}

// =============================================================================
// CONTENT DEDUP RESULT
// =============================================================================

#[derive(Debug, Clone)]
pub struct ContentDedupResult {
    pub is_duplicate: bool,
    pub check_time_us: u64,
}

// =============================================================================
// RECENT MESSAGE CACHE
// =============================================================================

/// Per-room ring buffer of recent messages.
/// Used for echo chamber detection and post-inference adequacy checks.
pub struct RecentMessageCache {
    rooms: HashMap<Uuid, VecDeque<CachedMessage>>,
}

impl RecentMessageCache {
    pub fn new() -> Self {
        Self {
            rooms: HashMap::new(),
        }
    }

    /// Add a message to the cache. Ring buffer — oldest evicted when full.
    pub fn push(&mut self, room_id: Uuid, msg: CachedMessage) {
        let room = self.rooms.entry(room_id).or_insert_with(|| VecDeque::with_capacity(MAX_CACHED_PER_ROOM));
        if room.len() >= MAX_CACHED_PER_ROOM {
            room.pop_front();
        }
        room.push_back(msg);
    }

    /// Check for echo chamber: AI-only conversation without human participation.
    /// Returns true if should NOT respond (echo chamber detected).
    pub fn check_echo_chamber(
        &self,
        room_id: Uuid,
        sender_is_human: bool,
        is_mentioned: bool,
        now_ms: u64,
    ) -> EchoChamberResult {
        let start = Instant::now();

        // Humans and direct mentions always pass
        if sender_is_human || is_mentioned {
            return EchoChamberResult {
                is_echo_chamber: false,
                ai_message_count: 0,
                has_human_recently: sender_is_human,
                check_time_us: start.elapsed().as_micros() as u64,
            };
        }

        let cutoff = now_ms.saturating_sub(ECHO_CHAMBER_WINDOW_MS);

        let (ai_count, has_human) = self.rooms.get(&room_id)
            .map(|msgs| {
                let mut ai = 0usize;
                let mut human = false;
                for m in msgs.iter().rev() {
                    if m.timestamp_ms < cutoff { break; }
                    match m.sender_type {
                        SenderCategory::Human => human = true,
                        SenderCategory::AI => ai += 1,
                    }
                }
                (ai, human)
            })
            .unwrap_or((0, false));

        EchoChamberResult {
            is_echo_chamber: !has_human && ai_count >= ECHO_CHAMBER_AI_THRESHOLD,
            ai_message_count: ai_count,
            has_human_recently: has_human,
            check_time_us: start.elapsed().as_micros() as u64,
        }
    }

    /// Get messages since a timestamp for a room (for post-inference adequacy).
    /// Returns messages from OTHER senders (not the requesting persona).
    pub fn messages_since(
        &self,
        room_id: Uuid,
        since_ms: u64,
        exclude_persona_id: Uuid,
        exclude_message_id: Uuid,
    ) -> Vec<&CachedMessage> {
        self.rooms.get(&room_id)
            .map(|msgs| {
                msgs.iter()
                    .rev()
                    .take_while(|m| m.timestamp_ms > since_ms)
                    .filter(|m| m.sender_id != exclude_persona_id && m.id != exclude_message_id)
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Default for RecentMessageCache {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// CONTENT DEDUPLICATOR (hash-based, time-windowed)
// =============================================================================

struct ContentEntry {
    hash: u64,
    room_id: Uuid,
    timestamp_ms: u64,
}

/// Fast content deduplication using hash comparison within a time window.
pub struct ContentDeduplicator {
    entries: VecDeque<ContentEntry>,
}

impl ContentDeduplicator {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(CONTENT_DEDUP_MAX_ENTRIES),
        }
    }

    /// Check if content is a duplicate (same hash posted to same room within window).
    pub fn is_duplicate(&self, content: &str, room_id: Uuid, now_ms: u64) -> ContentDedupResult {
        let start = Instant::now();
        let hash = Self::hash_content(content);
        let cutoff = now_ms.saturating_sub(CONTENT_DEDUP_WINDOW_MS);

        let is_dup = self.entries.iter()
            .rev()
            .take_while(|e| e.timestamp_ms > cutoff)
            .any(|e| e.hash == hash && e.room_id == room_id);

        ContentDedupResult {
            is_duplicate: is_dup,
            check_time_us: start.elapsed().as_micros() as u64,
        }
    }

    /// Record content for future dedup checks.
    pub fn record(&mut self, content: &str, room_id: Uuid, now_ms: u64) {
        let hash = Self::hash_content(content);

        if self.entries.len() >= CONTENT_DEDUP_MAX_ENTRIES {
            self.entries.pop_front();
        }

        self.entries.push_back(ContentEntry {
            hash,
            room_id,
            timestamp_ms: now_ms,
        });
    }

    /// FNV-1a hash — fast, good distribution for short strings.
    fn hash_content(content: &str) -> u64 {
        // Normalize: lowercase, collapse whitespace
        let normalized: String = content.to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
        for byte in normalized.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3); // FNV prime
        }
        hash
    }
}

impl Default for ContentDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn make_msg(sender_type: SenderCategory, timestamp_ms: u64) -> CachedMessage {
        CachedMessage {
            id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_type,
            sender_name: "Test".into(),
            content_text: "Hello".into(),
            timestamp_ms,
        }
    }

    // ── Echo Chamber Tests ──

    #[test]
    fn test_no_echo_chamber_with_human() {
        let cache = RecentMessageCache::new();
        let result = cache.check_echo_chamber(Uuid::new_v4(), true, false, now_ms());
        assert!(!result.is_echo_chamber);
    }

    #[test]
    fn test_no_echo_chamber_when_mentioned() {
        let cache = RecentMessageCache::new();
        let result = cache.check_echo_chamber(Uuid::new_v4(), false, true, now_ms());
        assert!(!result.is_echo_chamber);
    }

    #[test]
    fn test_echo_chamber_detected() {
        let mut cache = RecentMessageCache::new();
        let room = Uuid::new_v4();
        let now = now_ms();

        // 6 AI messages, no human
        for i in 0..6 {
            cache.push(room, make_msg(SenderCategory::AI, now - (6 - i) * 1000));
        }

        let result = cache.check_echo_chamber(room, false, false, now);
        assert!(result.is_echo_chamber);
        assert_eq!(result.ai_message_count, 6);
        assert!(!result.has_human_recently);
    }

    #[test]
    fn test_no_echo_chamber_with_human_in_window() {
        let mut cache = RecentMessageCache::new();
        let room = Uuid::new_v4();
        let now = now_ms();

        // 5 AI messages + 1 human
        for i in 0..5 {
            cache.push(room, make_msg(SenderCategory::AI, now - (6 - i) * 1000));
        }
        cache.push(room, make_msg(SenderCategory::Human, now - 500));

        let result = cache.check_echo_chamber(room, false, false, now);
        assert!(!result.is_echo_chamber);
        assert!(result.has_human_recently);
    }

    #[test]
    fn test_echo_chamber_old_messages_excluded() {
        let mut cache = RecentMessageCache::new();
        let room = Uuid::new_v4();
        let now = now_ms();

        // 6 AI messages but all >2min ago
        for i in 0..6 {
            cache.push(room, make_msg(SenderCategory::AI, now - ECHO_CHAMBER_WINDOW_MS - (6 - i) * 1000));
        }

        let result = cache.check_echo_chamber(room, false, false, now);
        assert!(!result.is_echo_chamber, "Old messages should not trigger echo chamber");
    }

    // ── Content Dedup Tests ──

    #[test]
    fn test_content_not_duplicate() {
        let dedup = ContentDeduplicator::new();
        let result = dedup.is_duplicate("Hello world", Uuid::new_v4(), now_ms());
        assert!(!result.is_duplicate);
    }

    #[test]
    fn test_content_duplicate_detected() {
        let mut dedup = ContentDeduplicator::new();
        let room = Uuid::new_v4();
        let now = now_ms();

        dedup.record("Hello world", room, now);
        let result = dedup.is_duplicate("Hello world", room, now + 1000);
        assert!(result.is_duplicate);
    }

    #[test]
    fn test_content_duplicate_case_insensitive() {
        let mut dedup = ContentDeduplicator::new();
        let room = Uuid::new_v4();
        let now = now_ms();

        dedup.record("Hello World", room, now);
        let result = dedup.is_duplicate("hello world", room, now + 1000);
        assert!(result.is_duplicate);
    }

    #[test]
    fn test_content_different_room_not_duplicate() {
        let mut dedup = ContentDeduplicator::new();
        let now = now_ms();

        dedup.record("Hello world", Uuid::new_v4(), now);
        let result = dedup.is_duplicate("Hello world", Uuid::new_v4(), now + 1000);
        assert!(!result.is_duplicate);
    }

    #[test]
    fn test_content_expired_not_duplicate() {
        let mut dedup = ContentDeduplicator::new();
        let room = Uuid::new_v4();
        let now = now_ms();

        dedup.record("Hello world", room, now);
        // 61 seconds later — outside window
        let result = dedup.is_duplicate("Hello world", room, now + CONTENT_DEDUP_WINDOW_MS + 1000);
        assert!(!result.is_duplicate);
    }

    #[test]
    fn test_ring_buffer_eviction() {
        let mut cache = RecentMessageCache::new();
        let room = Uuid::new_v4();
        let now = now_ms();

        // Push MAX_CACHED_PER_ROOM + 10 messages
        for i in 0..(MAX_CACHED_PER_ROOM + 10) {
            cache.push(room, make_msg(SenderCategory::AI, now + i as u64));
        }

        let room_msgs = cache.rooms.get(&room).unwrap();
        assert_eq!(room_msgs.len(), MAX_CACHED_PER_ROOM);
    }

    // ── Performance Tests ──

    #[test]
    fn test_echo_chamber_check_is_fast() {
        let mut cache = RecentMessageCache::new();
        let room = Uuid::new_v4();
        let now = now_ms();

        for i in 0..50 {
            cache.push(room, make_msg(SenderCategory::AI, now - (50 - i) * 1000));
        }

        let result = cache.check_echo_chamber(room, false, false, now);
        assert!(
            result.check_time_us < 1000,
            "Echo chamber check should be <1ms, was {}us",
            result.check_time_us
        );
    }

    #[test]
    fn test_content_dedup_check_is_fast() {
        let mut dedup = ContentDeduplicator::new();
        let room = Uuid::new_v4();
        let now = now_ms();

        for i in 0..50 {
            dedup.record(&format!("Message {i}"), room, now - (50 - i) * 1000);
        }

        let result = dedup.is_duplicate("New message", room, now);
        assert!(
            result.check_time_us < 1000,
            "Content dedup should be <1ms, was {}us",
            result.check_time_us
        );
    }
}
