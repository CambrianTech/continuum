//! ChannelDigest + per-channel bookmark — consolidation before a concern runs.
//!
//! From [`docs/architecture/CONCURRENT-MIND-AND-GOVERNOR.md`] §3.3: a persona does
//! NOT loop over a channel's messages one-by-one. It reads the channel's recent
//! window as ONE consolidated [`ChannelDigest`] — like a human clicking a Slack
//! channel: read the batch, form one impression, decide once. 25 new messages is
//! one digest, not 25 cognition loops.
//!
//! ## The split: shared content vs cheap per-persona relationship
//!
//! - The **elements** are shared ([`ChannelElement`], the reference-passed CBAR
//!   frame): their artifacts (embedding/…) are computed once and reused by every
//!   persona ([[consolidate-before-concern-shared-elements-via-cache]]).
//! - The **bookmark** is the persona's cheap per-channel state — a last-read
//!   cursor (the Slack unread marker), a *relationship* property, not content. The
//!   digest is a cheap per-persona slice of `Arc` references over the shared
//!   elements, anchored at the bookmark.
//!
//! ## The window
//!
//! The default digest is *everything since the persona's bookmark* (the unread)
//! plus up to `grounding` messages *before* the bookmark for context. `grounding`
//! (N-before-bookmark) is **recipe-defined** by the caller — a coordination room
//! defaults shallow, a deep-work room richer ([[room-purpose-is-per-recipe-not-an-enum]]);
//! the mechanism here carries no policy. Want more than the window? That is a
//! separate command (scrollback / search) — airc holds the full durable history,
//! so the window only bounds what is pulled into thought BY DEFAULT, never what is
//! retained ([[persona-is-a-client]]).
//!
//! ## airc is the system of record
//!
//! The digest reads the room's recent events via [`AircTranscriptReader`] (ONE
//! batch read), resolves them to shared elements, and splits at the bookmark. It
//! stores nothing durably — airc does. We sort by lamport ascending ourselves so
//! the split is correct regardless of the reader's ordering convention.

use std::sync::Arc;

use airc_core::TranscriptEvent;
use airc_lib::AircError;
use dashmap::DashMap;
use uuid::Uuid;

use crate::cognition::channel_element::{ChannelElement, ChannelElementCache};
use crate::persona::airc_source::AircTranscriptReader;

/// Default N-before-bookmark grounding when a caller has no recipe-specified value.
/// Recipe/RoomPurpose resolution belongs to the caller; this is only the floor.
pub const DEFAULT_GROUNDING: usize = 5;

/// Per-persona, per-channel last-read cursor — the Slack unread marker. Keyed by
/// (persona, room); the value is the lamport of the newest message the persona has
/// read. Cheap per-persona state (a relationship property), lock-free.
#[derive(Default)]
pub struct ChannelBookmarks {
    marks: DashMap<(Uuid, Uuid), u64>,
}

impl ChannelBookmarks {
    pub fn new() -> Self {
        Self::default()
    }

    /// The persona's last-read lamport for a channel. `0` = never read (everything
    /// is unread). Real airc lamports are >= 1, so `unread = lamport > bookmark`
    /// surfaces all messages for a fresh channel.
    pub fn last_read(&self, persona: Uuid, room: Uuid) -> u64 {
        self.marks.get(&(persona, room)).map(|m| *m).unwrap_or(0)
    }

    /// Advance the bookmark to `lamport` — MONOTONIC: it never moves backward, so a
    /// late-arriving lower lamport or a redundant advance can't "unread" messages.
    /// This is the command behind ignore / skip-to-end / mark-read (advance to the
    /// channel tip); pause/revisit simply does NOT advance.
    pub fn advance(&self, persona: Uuid, room: Uuid, lamport: u64) {
        let mut e = self.marks.entry((persona, room)).or_insert(0);
        if lamport > *e {
            *e = lamport;
        }
    }
}

/// A consolidated, per-persona view of a channel's recent activity: the unread
/// elements (since the persona's bookmark) plus up to N-before-bookmark elements
/// for grounding context. `elements` is chronological (lamport-ascending); each is
/// an `Arc` reference into the shared element cache (cheap to slice, shared by all).
pub struct ChannelDigest {
    pub room_id: Uuid,
    pub persona_id: Uuid,
    /// The last-read lamport at the moment this digest was built (the split point).
    pub bookmark: u64,
    /// The consolidated window, oldest-first. `Arc` references over shared elements.
    pub elements: Vec<Arc<ChannelElement>>,
    /// Index into `elements` where the unread (since-bookmark) run begins. Elements
    /// before it are N-before grounding context; `== elements.len()` when all read.
    pub unread_start: usize,
}

impl ChannelDigest {
    /// The unread elements — what's new since this persona last read the channel.
    pub fn unread(&self) -> &[Arc<ChannelElement>] {
        &self.elements[self.unread_start..]
    }

    /// The grounding context — up to N elements the persona had already read,
    /// kept so the unread run isn't read without context.
    pub fn grounding(&self) -> &[Arc<ChannelElement>] {
        &self.elements[..self.unread_start]
    }

    /// Is there anything new to attend to?
    pub fn has_unread(&self) -> bool {
        self.unread_start < self.elements.len()
    }

    /// The lamport of the newest element in the window, if any — what a persona
    /// advances its bookmark to after engaging (ignore/skip/respond all mark-read).
    pub fn tip_lamport(&self) -> Option<u64> {
        self.elements.last().map(|e| e.event().lamport)
    }
}

/// Builds per-persona channel digests from shared elements + bookmarks. Holds the
/// shared [`ChannelElementCache`] (so artifacts are computed once across personas)
/// and the [`ChannelBookmarks`]; the reader + recipe-defined grounding are passed
/// per build.
pub struct ChannelDigestBuilder {
    cache: Arc<ChannelElementCache>,
    bookmarks: Arc<ChannelBookmarks>,
}

impl ChannelDigestBuilder {
    pub fn new(cache: Arc<ChannelElementCache>, bookmarks: Arc<ChannelBookmarks>) -> Self {
        Self { cache, bookmarks }
    }

    /// The shared bookmark store (so callers advance the same cursors this builder
    /// splits on).
    pub fn bookmarks(&self) -> &Arc<ChannelBookmarks> {
        &self.bookmarks
    }

    /// Build the consolidated digest for `persona_id` on `room_id`.
    ///
    /// ONE batch read (`page_recent`), filtered to the channel, resolved to shared
    /// elements, sorted lamport-ascending, then split at the persona's bookmark:
    /// everything after it is unread; up to `grounding` elements before it are kept
    /// for context. `grounding` is the recipe-defined N (use [`DEFAULT_GROUNDING`]
    /// absent a recipe).
    pub async fn build(
        &self,
        persona_id: Uuid,
        room_id: Uuid,
        reader: &dyn AircTranscriptReader,
        fetch_limit: usize,
        grounding: usize,
    ) -> Result<ChannelDigest, AircError> {
        let events = reader.page_recent(fetch_limit).await?;
        Ok(self.build_from_events(persona_id, room_id, events, grounding))
    }

    /// Build a digest from PRE-FETCHED events — lets a caller that already paged
    /// the transcript (e.g. to DERIVE the room when the context didn't carry it)
    /// reuse those events instead of paging twice. The room split logic is identical.
    pub fn build_from_events(
        &self,
        persona_id: Uuid,
        room_id: Uuid,
        events: Vec<TranscriptEvent>,
        grounding: usize,
    ) -> ChannelDigest {
        let bookmark = self.bookmarks.last_read(persona_id, room_id);

        // Filter to THIS channel — lamport is per-room, so mixing rooms would make
        // the bookmark split meaningless. Resolve to shared elements (consolidation:
        // one batch, artifacts computed once and shared across personas).
        let in_room: Vec<TranscriptEvent> = events
            .into_iter()
            .filter(|e| e.room_id.as_uuid() == room_id)
            .collect();
        let mut elements = self.cache.get_or_insert_batch(in_room);

        // Sort lamport-ascending ourselves: correctness must not depend on the
        // reader's ordering convention.
        elements.sort_by_key(|e| e.event().lamport);

        // Split: first element strictly newer than the bookmark begins the unread
        // run; keep up to `grounding` elements before it for context.
        let first_unread = elements
            .iter()
            .position(|e| e.event().lamport > bookmark)
            .unwrap_or(elements.len());
        let grounding_start = first_unread.saturating_sub(grounding);
        let unread_start = first_unread - grounding_start;
        let elements = elements.split_off(grounding_start);

        ChannelDigest {
            room_id,
            persona_id,
            bookmark,
            elements,
            unread_start,
        }
    }
}

/// Test-only shared fixture: a minimal room message `TranscriptEvent` at a
/// given lamport. ONE construction site for every test that needs to stage
/// channel events (this file's split tests, the vitals radiator's QUE test)
/// — never re-built per test file (CLAUDE.md test-fixture rule 5).
#[cfg(test)]
pub(crate) fn test_event_in(
    room: airc_core::RoomId,
    text: &str,
    lamport: u64,
) -> TranscriptEvent {
    use airc_core::{Body, ClientId, EventId, Headers, MentionTarget, PeerId, TranscriptKind};
    TranscriptEvent {
        event_id: EventId::new(),
        room_id: room,
        peer_id: PeerId::new(),
        client_id: ClientId::new(),
        kind: TranscriptKind::Message,
        occurred_at_ms: 1_000_000 + lamport,
        lamport,
        target: MentionTarget::Room(room),
        headers: Headers::default(),
        body: Some(Body::text(text)),
        attachment: None,
        receipt: None,
        metadata: serde_json::Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::embedding::EmbeddingProvider;
    use airc_core::RoomId;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// Trivial embedder for the shared element cache (digest tests don't assert on
    /// vectors; they assert on the window split + sharing).
    struct NoopEmbedder;
    #[async_trait]
    impl EmbeddingProvider for NoopEmbedder {
        fn id(&self) -> &str {
            "noop"
        }
        fn dim(&self) -> usize {
            1
        }
        async fn embed(&self, _text: &str) -> Vec<f32> {
            vec![0.0]
        }
    }

    /// Canned reader (same shape as AircRagSource's StubReader): returns fixed
    /// events without a daemon.
    struct StubReader {
        events: Mutex<Vec<TranscriptEvent>>,
    }
    impl StubReader {
        fn new(events: Vec<TranscriptEvent>) -> Self {
            Self {
                events: Mutex::new(events),
            }
        }
    }
    #[async_trait]
    impl AircTranscriptReader for StubReader {
        async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
            Ok(self.events.lock().unwrap().iter().take(limit).cloned().collect())
        }
    }

    fn builder() -> (ChannelDigestBuilder, Arc<ChannelBookmarks>) {
        let cache = Arc::new(ChannelElementCache::new(Arc::new(NoopEmbedder)));
        let bookmarks = Arc::new(ChannelBookmarks::new());
        (
            ChannelDigestBuilder::new(cache, bookmarks.clone()),
            bookmarks,
        )
    }

    // what this catches: a fresh channel (never read) surfaces ALL messages as
    // unread — the consolidation read, in one batch.
    #[tokio::test]
    async fn fresh_channel_all_unread() {
        let room = RoomId::new();
        let persona = Uuid::new_v4();
        let reader = StubReader::new(vec![
            test_event_in(room, "a", 1),
            test_event_in(room, "b", 2),
            test_event_in(room, "c", 3),
        ]);
        let (b, _) = builder();
        let d = b.build(persona, room.as_uuid(), &reader, 100, 0).await.unwrap();
        assert_eq!(d.unread().len(), 3);
        assert!(d.grounding().is_empty());
        assert!(d.has_unread());
        assert_eq!(d.tip_lamport(), Some(3));
    }

    // what this catches: THE BOOKMARK SPLIT — after reading up to lamport 2, only
    // the lamport-3 message is unread; the rest is below the cursor.
    #[tokio::test]
    async fn bookmark_splits_read_from_unread() {
        let room = RoomId::new();
        let persona = Uuid::new_v4();
        let reader = StubReader::new(vec![
            test_event_in(room, "a", 1),
            test_event_in(room, "b", 2),
            test_event_in(room, "c", 3),
        ]);
        let (b, marks) = builder();
        marks.advance(persona, room.as_uuid(), 2); // read through lamport 2
        let d = b.build(persona, room.as_uuid(), &reader, 100, 0).await.unwrap();
        assert_eq!(d.unread().len(), 1);
        assert_eq!(d.unread()[0].text(), Some("c"));
    }

    // what this catches: N-before-bookmark grounding — the unread run is delivered
    // WITH up to N already-read messages of context, not naked.
    #[tokio::test]
    async fn grounding_includes_n_before_bookmark() {
        let room = RoomId::new();
        let persona = Uuid::new_v4();
        let reader = StubReader::new(vec![
            test_event_in(room, "a", 1),
            test_event_in(room, "b", 2),
            test_event_in(room, "c", 3),
        ]);
        let (b, marks) = builder();
        marks.advance(persona, room.as_uuid(), 2);
        let d = b.build(persona, room.as_uuid(), &reader, 100, 1).await.unwrap();
        assert_eq!(d.grounding().len(), 1, "one before-bookmark for context");
        assert_eq!(d.grounding()[0].text(), Some("b"));
        assert_eq!(d.unread().len(), 1);
        assert_eq!(d.unread()[0].text(), Some("c"));
    }

    // what this catches: a fully-read channel yields NO unread but still returns
    // recent grounding — clicking a caught-up channel shows context, nothing new.
    #[tokio::test]
    async fn all_read_yields_grounding_only() {
        let room = RoomId::new();
        let persona = Uuid::new_v4();
        let reader = StubReader::new(vec![
            test_event_in(room, "a", 1),
            test_event_in(room, "b", 2),
            test_event_in(room, "c", 3),
        ]);
        let (b, marks) = builder();
        marks.advance(persona, room.as_uuid(), 3); // read everything
        let d = b.build(persona, room.as_uuid(), &reader, 100, 2).await.unwrap();
        assert!(!d.has_unread());
        assert_eq!(d.grounding().len(), 2, "last 2 read messages as context");
    }

    // what this catches: bookmark advance is MONOTONIC — a lower lamport never
    // moves it backward, so messages can't be re-surfaced as unread.
    #[tokio::test]
    async fn advance_is_monotonic() {
        let marks = ChannelBookmarks::new();
        let persona = Uuid::new_v4();
        let room = Uuid::new_v4();
        marks.advance(persona, room, 5);
        marks.advance(persona, room, 2); // older — must not move it back
        assert_eq!(marks.last_read(persona, room), 5);
    }

    // what this catches: the digest is PER-CHANNEL — events from another room are
    // excluded (lamport is per-room; mixing rooms would corrupt the split).
    #[tokio::test]
    async fn digest_filters_to_channel() {
        let room_a = RoomId::new();
        let room_b = RoomId::new();
        let persona = Uuid::new_v4();
        let reader = StubReader::new(vec![
            test_event_in(room_a, "a-only", 1),
            test_event_in(room_b, "b-only", 1),
            test_event_in(room_a, "a-two", 2),
        ]);
        let (b, _) = builder();
        let d = b.build(persona, room_a.as_uuid(), &reader, 100, 0).await.unwrap();
        assert_eq!(d.elements.len(), 2);
        assert!(d.elements.iter().all(|e| e.event().room_id == room_a));
    }

    // what this catches: THE #146 STARVATION MECHANISM, demonstrated (blind-room
    // incident #3, 2026-07-13). Every engagement — ignore/skip/RESPOND alike —
    // advances the bookmark to the channel TIP, and the grounding window keeps
    // only N already-read elements. So a low-frequency speaker (the operator)
    // posting into a high-velocity room gets: read-without-necessarily-rendering
    // on the next tick, then permanently displaced from the grounding window as
    // soon as N newer peer messages land. Four chatty personas produce N=5 in
    // seconds — operator messages verified present in `airc inbox`/`events list`
    // yet absent from every persona prompt. This test pins the mechanism so the
    // fix (attended ≠ fetched: advance the bookmark only past messages that
    // actually RENDERED into a prompt — or velocity-aware grounding) has a red/
    // green target: when the fix lands, flip the final assertion.
    #[tokio::test]
    async fn tip_advance_plus_grounding_window_starves_low_frequency_speakers() {
        let room = RoomId::new();
        let persona = Uuid::new_v4();
        let mut events = vec![test_event_in(room, "OPERATOR: your card is 0b1a6230", 10)];
        // Tick 1: persona engages (even a silent PASS marks read to tip).
        // Digest at this point still shows the operator message as unread.
        let (b, _) = builder();
        let reader = StubReader::new(events.clone());
        let d = b.build(persona, room.as_uuid(), &reader, 100, 5).await.unwrap();
        assert!(d.unread().iter().any(|e| e.text().unwrap().contains("OPERATOR")));
        b.bookmarks().advance(persona, room.as_uuid(), d.tip_lamport().unwrap());

        // Peers flood: 6 newer messages (> grounding=5), persona engages again.
        for (i, l) in (11..=16).enumerate() {
            events.push(test_event_in(room, &format!("peer chatter {i}"), l));
        }
        let reader = StubReader::new(events.clone());
        let d = b.build(persona, room.as_uuid(), &reader, 100, 5).await.unwrap();
        b.bookmarks().advance(persona, room.as_uuid(), d.tip_lamport().unwrap());

        // Next build: the operator message is GONE — not in unread (read long
        // ago), not in grounding (displaced by 5 newer read peer messages).
        for (i, l) in (17..=18).enumerate() {
            events.push(test_event_in(room, &format!("more chatter {i}"), l));
        }
        let reader = StubReader::new(events);
        let d = b.build(persona, room.as_uuid(), &reader, 100, 5).await.unwrap();
        assert!(
            !d.elements.iter().any(|e| e.text().unwrap_or("").contains("OPERATOR")),
            "documents the starvation: operator message evicted from the persona's \
             entire perceivable window while durably present in the store — #146"
        );
    }

    // what this catches: two personas' digests over the same channel reference the
    // SAME shared element Arcs — compute-once-across-personas holds at digest level,
    // not just in the cache.
    #[tokio::test]
    async fn personas_share_element_arcs() {
        let room = RoomId::new();
        let p1 = Uuid::new_v4();
        let p2 = Uuid::new_v4();
        let reader = StubReader::new(vec![test_event_in(room, "shared", 1)]);
        let (b, _) = builder();
        let d1 = b.build(p1, room.as_uuid(), &reader, 100, 0).await.unwrap();
        let d2 = b.build(p2, room.as_uuid(), &reader, 100, 0).await.unwrap();
        assert!(
            Arc::ptr_eq(&d1.elements[0], &d2.elements[0]),
            "both personas reference one shared element"
        );
    }
}
