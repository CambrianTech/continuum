//! The reader a mind pages a room's conversation through: the contract, the one
//! perception-page filter, and the airc implementation of it.

use airc_core::TranscriptEvent;
use airc_lib::AircError;
use async_trait::async_trait;

/// Abstract reader over airc transcript events. Production impl rides on
/// `airc_lib::Airc`; tests use a stub that returns canned events without a daemon.
#[async_trait]
pub trait AircTranscriptReader: Send + Sync {
    /// Return up to `limit` most-recent CONVERSATIONAL transcript events
    /// (Message + Attachment), newest-first per airc convention.
    ///
    /// Kinds are filtered BEFORE the page limit (#297): a raw newest-`limit`
    /// page counts ephemeral StreamChunk frames (~4/sec per talking persona),
    /// so active residents' own streaming evicted every durable message from
    /// their window within a minute — working personas were DEAF to direction
    /// while the room was busy (glass-boxed live 2026-08-01: a resident asked
    /// the same question 3× because four direction messages never entered her
    /// page while the attach cursor advanced normally). The diagnostic
    /// signature: cross-machine delivery perfect, local perception stale —
    /// the wire is fine, the WINDOW is flooded. Presence / receipts /
    /// lifecycle ride their own sources; this page is the room's
    /// conversation, never its firehose.
    async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError>;

    /// Room-scoped page (#367): return the same conversational page, but for
    /// the given room when `Some` — the TURN's room, not the reader's
    /// current-room pointer. `page_recent` follows the persona's landing-room
    /// pointer, so a turn happening anywhere else (a bench room, a project
    /// room she is subscribed to but not "in") paged the DEFAULT room's
    /// transcript — and everything downstream (digest, derived-room fallback,
    /// the recall query built from this window) followed the wrong room.
    ///
    /// Default impl ignores `room` and delegates — correct ONLY for test
    /// stubs whose canned events have no room dimension. Every production
    /// reader overrides this (the #262 lesson in `AircHandleAdapter`: a
    /// silently-inherited default is how regressions ship).
    async fn page_recent_in(
        &self,
        room: Option<airc_core::RoomId>,
        limit: usize,
    ) -> Result<Vec<TranscriptEvent>, AircError> {
        let _ = room;
        self.page_recent(limit).await
    }

    /// This reader's last-read lamport in `room` — the unread marker.
    ///
    /// THE CURSOR LIVES IN AIRC. It is durable runtime-consumer state
    /// (`runtime_cursor`, an ORM row keyed by consumer id), and airc's own API
    /// doc says why it exists: "intentionally store-backed so runtime delivery
    /// state does not sprawl into JSON sidecars." Continuum previously kept a
    /// PARALLEL `ChannelBookmarks` DashMap for this — process-memory only, so
    /// every cursor died on restart, and a second source of truth for a fact
    /// airc already owns. That is the defect this method removes.
    ///
    /// `0` = never read. Real airc lamports are >= 1.
    async fn read_cursor(&self, persona: uuid::Uuid, room: uuid::Uuid) -> Result<u64, AircError> {
        let _ = (persona, room);
        Ok(0)
    }

    /// Persist this reader's position in `room` at `event` — mark-read.
    ///
    /// Takes the EVENT, not a bare lamport, because airc's preferred path
    /// (`save_runtime_cursor_for_event`) carries the source event's room and
    /// kind and emits `SubscriptionAdvanced`, so a cursor move is visible to
    /// every other surface instead of being private to one process. A bare
    /// lamport would throw that away.
    async fn advance_read_cursor(
        &self,
        persona: uuid::Uuid,
        room: uuid::Uuid,
        event: &TranscriptEvent,
    ) -> Result<(), AircError> {
        let _ = (persona, room, event);
        Ok(())
    }
}

/// The durable consumer id for one reader's position in one room.
///
/// Namespaced string because that IS airc's convention for `runtime_cursor`
/// (its own rows look like `codex-hook:default`); the entity is keyed by a
/// consumer id, so this composes airc's key, it does not invent an identifier.
/// Both halves are rendered from real UUIDs — never a name, never a label.
pub fn read_cursor_consumer_id(persona: uuid::Uuid, room: uuid::Uuid) -> String {
    format!("persona:{persona}:room:{room}")
}

/// The kinds a perception page means: the room's conversation. ONE place
/// (compression) — every reader impl funnels through the [`airc_lib::Airc`]
/// impl below, which applies this filter.
pub fn perception_page_filter() -> airc_lib::EventFilter {
    let mut filter = airc_lib::EventFilter::current_room();
    filter.kinds.insert(airc_core::TranscriptKind::Message);
    filter.kinds.insert(airc_core::TranscriptKind::Attachment);
    filter
}

/// The same conversational-kinds filter, pinned to a specific room (#367).
/// `None` leaves the channel unset, which `page_recent_filtered` scopes to
/// the current room — the pre-#367 behavior, still right for genuinely
/// room-less work.
pub fn perception_page_filter_in(room: Option<airc_core::RoomId>) -> airc_lib::EventFilter {
    let mut filter = perception_page_filter();
    filter.channel = room;
    filter
}

/// `airc_lib::Airc` satisfies the reader contract via the kinds-filtered
/// page (daemon-side newest-N-of-kind since airc PR #1314). Orphan rule OK —
/// the trait is ours.
#[async_trait]
impl AircTranscriptReader for airc_lib::Airc {
    async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
        airc_lib::Airc::page_recent_filtered(self, perception_page_filter(), limit).await
    }

    async fn page_recent_in(
        &self,
        room: Option<airc_core::RoomId>,
        limit: usize,
    ) -> Result<Vec<TranscriptEvent>, AircError> {
        airc_lib::Airc::page_recent_filtered(self, perception_page_filter_in(room), limit).await
    }

    /// airc's durable `runtime_cursor` row IS the unread marker — no second store.
    async fn read_cursor(&self, persona: uuid::Uuid, room: uuid::Uuid) -> Result<u64, AircError> {
        Ok(
            airc_lib::Airc::load_runtime_cursor(self, &read_cursor_consumer_id(persona, room))
                .await?
                .map(|c| c.lamport)
                .unwrap_or(0),
        )
    }

    async fn advance_read_cursor(
        &self,
        persona: uuid::Uuid,
        room: uuid::Uuid,
        event: &TranscriptEvent,
    ) -> Result<(), AircError> {
        airc_lib::Airc::save_runtime_cursor_for_event(
            self,
            &read_cursor_consumer_id(persona, room),
            event,
        )
        .await
    }
}
