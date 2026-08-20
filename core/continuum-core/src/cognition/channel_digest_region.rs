//! ChannelDigestRegion — a brain region that pre-stages per-channel digests into
//! a ready-buffer, so the cognition hot path READS a settled snapshot instead of
//! consolidating inline.
//!
//! This is slice 2's pre-staging concern from
//! [`docs/architecture/CONCURRENT-MIND-AND-GOVERNOR.md`] §3 / §4: "make a recall
//! `BrainRegion` emit into a ready-buffer; the workspace's recall path consumes the
//! snapshot instead of computing inline." The region runs on the governor's
//! schedule (own task, `catch_unwind`+timeout isolated, like slice 1), builds each
//! live persona's [`ChannelDigest`] (consolidation — one batch, shared elements),
//! and `publish`es it to a [`DashMapReadyBuffer`]. A consumer (the workspace) later
//! `peek`s the freshest digest — lock-free, never blocking ([[free-ungated-mind-biology-rtos-rust]]).
//!
//! ## Flood-safety
//!
//! Building a digest does NOT run inference — element embeddings are lazy
//! ([`ChannelElement::embedding`]) and only computed if/when something reads them.
//! The tick is page_recent I/O + a slice + a publish. So N personas × ticks can't
//! melt the model backend; this stays as flood-safe as the slice-1 governor.
//!
//! ## Abstraction for testability
//!
//! The region depends on [`PersonaChannelReader`], not the concrete
//! `PersonaAircRuntimeRegistry`, so it unit-tests against a stub without a live
//! airc daemon (adapter discipline). The real impl resolves a persona's runtime
//! (its `AircTranscriptReader`) + current room from the registry.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::cognition::channel_digest::{ChannelDigest, ChannelDigestBuilder, DEFAULT_GROUNDING};
use crate::persona::airc_source::AircTranscriptReader;
use crate::runtime::brain_region::{
    BrainRegion, CadenceHint, ComputeClass, MemoryClass, PressureProfile, RegionContext, RegionId,
    TickOutcome,
};
use crate::runtime::ready_buffer::{DashMapReadyBuffer, ReadyBuffer};

/// Default newest-events fetch cap per digest build (mirrors `AircRagSource`'s
/// production default). The recipe-defined grounding window slices within this.
const DEFAULT_FETCH_LIMIT: usize = 100;

/// Yields the personas to pre-stage for and, per persona, a transcript reader +
/// the channel (room) to digest. Implemented on `PersonaAircRuntimeRegistry` in
/// production; stubbed in tests so the region needs no live airc daemon.
pub trait PersonaChannelReader: Send + Sync {
    /// The live personas to pre-stage digests for.
    fn live_personas(&self) -> Vec<Uuid>;
    /// A persona's transcript reader + the room (channel) to digest, if live.
    /// `None` when the persona has no live runtime/slot.
    fn reader_and_room(&self, persona_id: Uuid) -> Option<(Arc<dyn AircTranscriptReader>, Uuid)>;
}

/// The shared ready-buffer type: (persona, room) → freshest consolidated digest.
pub type DigestBuffer = DashMapReadyBuffer<(Uuid, Uuid), Arc<ChannelDigest>>;

/// Pre-stages per-channel digests for every live persona.
pub struct ChannelDigestRegion {
    builder: Arc<ChannelDigestBuilder>,
    personas: Arc<dyn PersonaChannelReader>,
    digests: Arc<DigestBuffer>,
    grounding: usize,
    fetch_limit: usize,
}

impl ChannelDigestRegion {
    /// Build with the shared digest builder + the persona/channel source, owning a
    /// fresh ready-buffer (tests / standalone).
    pub fn new(
        builder: Arc<ChannelDigestBuilder>,
        personas: Arc<dyn PersonaChannelReader>,
    ) -> Self {
        Self::with_buffer(builder, personas, Arc::new(DashMapReadyBuffer::new()))
    }

    /// Build sharing an EXISTING ready-buffer — the production path, so the
    /// consumer (a RAG source) peeks the same buffer the region publishes into.
    pub fn with_buffer(
        builder: Arc<ChannelDigestBuilder>,
        personas: Arc<dyn PersonaChannelReader>,
        digests: Arc<DigestBuffer>,
    ) -> Self {
        Self {
            builder,
            personas,
            digests,
            grounding: DEFAULT_GROUNDING,
            fetch_limit: DEFAULT_FETCH_LIMIT,
        }
    }

    /// Override the N-before-bookmark grounding window (recipe-defined in time).
    pub fn with_grounding(mut self, grounding: usize) -> Self {
        self.grounding = grounding;
        self
    }

    /// Override the newest-events fetch cap.
    pub fn with_fetch_limit(mut self, fetch_limit: usize) -> Self {
        self.fetch_limit = fetch_limit;
        self
    }

    /// Share the ready-buffer with a consumer (the workspace's recall path peeks
    /// it). Same Arc — the consumer sees whatever the region last published.
    pub fn digests(&self) -> Arc<DigestBuffer> {
        self.digests.clone()
    }

    /// Lock-free read of the freshest pre-staged digest for a persona's channel.
    pub fn peek(&self, persona_id: Uuid, room_id: Uuid) -> Option<Arc<ChannelDigest>> {
        self.digests.peek(&(persona_id, room_id))
    }

    /// Pre-stage one persona's current-channel digest. Shared so the per-persona
    /// tick and any future global sweep use one path.
    async fn prestage(&self, persona_id: Uuid) -> TickOutcome {
        let Some((reader, room)) = self.personas.reader_and_room(persona_id) else {
            return TickOutcome::idle();
        };
        match self
            .builder
            .build(
                persona_id,
                room,
                reader.as_ref(),
                self.fetch_limit,
                self.grounding,
            )
            .await
        {
            Ok(digest) => {
                let has_unread = digest.has_unread();
                self.digests.publish((persona_id, room), Arc::new(digest));
                TickOutcome {
                    published: 1,
                    consumed_since_last: 0,
                    pressure_observed: None,
                    // Organic cadence: a quiet (all-read) channel asks to back off;
                    // fresh unread holds. The governor owns final policy, and a new
                    // burst (invalidation) is the real wake — this is only a hint.
                    cadence_hint: Some(if has_unread {
                        CadenceHint::Hold
                    } else {
                        CadenceHint::Slower
                    }),
                }
            }
            Err(e) => {
                tracing::warn!(
                    persona_id = %persona_id,
                    room = %room,
                    error = %e,
                    "channel-digest: build failed — region stays up, no digest staged this tick"
                );
                TickOutcome::idle()
            }
        }
    }
}

#[async_trait]
impl BrainRegion for ChannelDigestRegion {
    fn id(&self) -> RegionId {
        RegionId::from_static("channel-digest")
    }

    fn pressure_profile(&self) -> PressureProfile {
        PressureProfile {
            // Holds Arc references into the bounded shared element cache + one digest
            // per (persona, room). Light.
            memory_class: MemoryClass::Light,
            // page_recent I/O + sort + slice. No inference in the tick (embeddings
            // are lazy on the element), so this is plain CPU, not inference-class.
            compute_class: ComputeClass::Cpu,
            responds_to: Vec::new(),
        }
    }

    /// Per-persona pre-staging. The governor ticks this per live persona
    /// ([`RegionContext::for_persona`]); a global tick (no persona scope) is a no-op
    /// because digests are per (persona, channel).
    async fn tick(&self, ctx: &RegionContext) -> TickOutcome {
        match ctx.persona_scope {
            Some(persona_id) => self.prestage(persona_id).await,
            None => TickOutcome::idle(),
        }
    }
}

// ─── Real impl: resolve reader + room from the persona runtime registry ──────────

impl PersonaChannelReader for crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry {
    fn live_personas(&self) -> Vec<Uuid> {
        // Inherent method of the same name; qualify to avoid trait/inherent ambiguity.
        crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::live_personas(self)
    }

    fn reader_and_room(&self, persona_id: Uuid) -> Option<(Arc<dyn AircTranscriptReader>, Uuid)> {
        let runtime = self.get(persona_id)?;
        let room = runtime.default_room().as_uuid();
        // PersonaAircRuntime: AircTranscriptReader — upcast the Arc.
        let reader: Arc<dyn AircTranscriptReader> = runtime;
        Some((reader, room))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
        use crate::cognition::channel_element::ChannelElementCache;
    use crate::cognition::embedding::EmbeddingProvider;
    use airc_core::TranscriptEvent;
    use airc_core::{
        Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptKind,
    };
    use airc_lib::AircError;
    use std::sync::Mutex;

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

    struct StubReader {
        events: Mutex<Vec<TranscriptEvent>>,
        /// The reader's cursor — airc's durable one in production.
        cursor: Mutex<u64>,
    }
    #[async_trait]
    impl AircTranscriptReader for StubReader {
        async fn read_cursor(&self, _p: Uuid, _r: Uuid) -> Result<u64, AircError> {
            Ok(*self.cursor.lock().unwrap())
        }
        async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
            Ok(self
                .events
                .lock()
                .unwrap()
                .iter()
                .take(limit)
                .cloned()
                .collect())
        }
    }

    /// Stub channel source — one persona, one room, a canned reader. No daemon.
    struct StubChannels {
        persona: Uuid,
        room: Uuid,
        reader: Arc<dyn AircTranscriptReader>,
    }
    impl PersonaChannelReader for StubChannels {
        fn live_personas(&self) -> Vec<Uuid> {
            vec![self.persona]
        }
        fn reader_and_room(
            &self,
            persona_id: Uuid,
        ) -> Option<(Arc<dyn AircTranscriptReader>, Uuid)> {
            (persona_id == self.persona).then(|| (self.reader.clone(), self.room))
        }
    }

    fn event_in(room: RoomId, text: &str, lamport: u64) -> TranscriptEvent {
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

    /// Build a region whose stub channel source serves `events` in `room` for
    /// `persona`. The cursor is the READER's (airc's) — the stub reader carries it.
    fn region_for(
        persona: Uuid,
        room: RoomId,
        events: Vec<TranscriptEvent>,
    ) -> ChannelDigestRegion {
        region_read_through(persona, room, events, 0)
    }

    /// Same, with the reader already read through `cursor`.
    fn region_read_through(
        persona: Uuid,
        room: RoomId,
        events: Vec<TranscriptEvent>,
        cursor: u64,
    ) -> ChannelDigestRegion {
        let cache = Arc::new(ChannelElementCache::new(Arc::new(NoopEmbedder)));
        let builder = Arc::new(ChannelDigestBuilder::new(cache));
        let channels = Arc::new(StubChannels {
            persona,
            room: room.as_uuid(),
            reader: Arc::new(StubReader {
                events: Mutex::new(events),
                cursor: Mutex::new(cursor),
            }),
        });
        ChannelDigestRegion::new(builder, channels).with_grounding(0)
    }

    // what this catches: THE PRE-STAGING — a per-persona tick builds the digest and
    // publishes it to the ready-buffer; a consumer then peeks the settled snapshot
    // (the workspace-reads-a-snapshot seam) without recomputing.
    #[tokio::test]
    async fn tick_prestages_digest_for_consumer_to_peek() {
        let persona = Uuid::new_v4();
        let room = RoomId::new();
        let region = region_for(
            persona,
            room,
            vec![event_in(room, "a", 1), event_in(room, "b", 2)],
        );
        let outcome = region.tick(&RegionContext::for_persona(0, persona)).await;
        assert_eq!(outcome.published, 1);

        let digest = region
            .peek(persona, room.as_uuid())
            .expect("digest pre-staged");
        assert_eq!(digest.unread().len(), 2);
        assert_eq!(
            outcome.cadence_hint,
            Some(CadenceHint::Hold),
            "fresh unread holds cadence"
        );
    }

    // what this catches: a global tick (no persona scope) stages nothing — digests
    // are per (persona, channel), not global.
    #[tokio::test]
    async fn global_tick_is_idle() {
        let persona = Uuid::new_v4();
        let room = RoomId::new();
        let region = region_for(persona, room, vec![event_in(room, "a", 1)]);
        let outcome = region.tick(&RegionContext::global(0)).await;
        assert_eq!(outcome.published, 0);
        assert!(region.peek(persona, room.as_uuid()).is_none());
    }

    // what this catches: ticking for a persona with no live runtime is a safe no-op
    // (degrade-not-panic), not a publish or a crash.
    #[tokio::test]
    async fn unknown_persona_is_idle() {
        let persona = Uuid::new_v4();
        let room = RoomId::new();
        let region = region_for(persona, room, vec![event_in(room, "a", 1)]);
        let outcome = region
            .tick(&RegionContext::for_persona(0, Uuid::new_v4()))
            .await;
        assert_eq!(outcome.published, 0);
    }

    // what this catches: a quiet (fully-read) channel hints SLOWER — organic
    // back-off, the cadence side of "don't poll everyone on a clock."
    #[tokio::test]
    async fn quiet_channel_hints_slower() {
        let persona = Uuid::new_v4();
        let room = RoomId::new();
        // Already read everything (cursor at the tip) — that is what makes the
        // channel QUIET; the hint is about unread, not about emptiness.
        let region = region_read_through(
            persona,
            room,
            vec![event_in(room, "a", 1), event_in(room, "b", 2)],
            2,
        );
        let outcome = region.tick(&RegionContext::for_persona(0, persona)).await;
        assert_eq!(outcome.cadence_hint, Some(CadenceHint::Slower));
        // Still publishes (the grounding snapshot), just with no unread.
        let digest = region.peek(persona, room.as_uuid()).unwrap();
        assert!(!digest.has_unread());
    }

    // what this catches: re-ticking refreshes the SAME key in place (freshest-wins);
    // the consumer always peeks the latest, never a backlog.
    #[tokio::test]
    async fn retick_refreshes_in_place() {
        let persona = Uuid::new_v4();
        let room = RoomId::new();
        let region = region_for(persona, room, vec![event_in(room, "a", 1)]);
        region.tick(&RegionContext::for_persona(0, persona)).await;
        region.tick(&RegionContext::for_persona(1, persona)).await;
        assert_eq!(
            region.digests().len(),
            1,
            "one entry per (persona, channel), refreshed"
        );
    }
}
