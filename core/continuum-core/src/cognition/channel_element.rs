//! ChannelElement — one room message as a reference-passed, content-addressed
//! frame whose derived artifacts are computed ONCE and shared across every persona.
//!
//! This is the CBAR reference-passed-frame analog from
//! [`docs/architecture/CONCURRENT-MIND-AND-GOVERNOR.md`] §3.3. CBAR computes a
//! frame's grayscale / pyramid ONCE and lets every analyzer *reference* it; each
//! analyzer's interpretation is its own. Here the "frame" is a chat message and the
//! shared upstream artifacts are its embedding (and, later, vision-description /
//! transcription / summary). 14 personas reading the same message must not embed it
//! 14 times — the artifact is a property of the CONTENT, not of the persona reading
//! it ([[embeddings-are-per-content-computed-once-shared]],
//! [[consolidate-before-concern-shared-elements-via-cache]]).
//!
//! ## What is and isn't stored here
//!
//! airc is the system of record for messages — the full history is durable and
//! searchable there. A `ChannelElement` is a *reference view* of an airc
//! [`TranscriptEvent`] plus its lazily-computed derived artifacts; it is **never** a
//! parallel copy of the message store ([[persona-is-a-client]], the compression
//! principle). The [`ChannelElementCache`] is a bounded working set of shared
//! artifacts keyed to airc's stable `EventId`, not a durable store — eviction just
//! means the next access re-resolves and re-shares.
//!
//! ## Why this is "compute once, lazily"
//!
//! The embedding accessor is a per-element [`tokio::sync::OnceCell`] (CBAR's lazy
//! getter): the first persona to need it computes it, every persona after holds the
//! same `Arc`. Because the element itself is shared (one `Arc<ChannelElement>` per
//! message, handed to every persona by the cache), the OnceCell *is*
//! compute-once-across-personas — no scheduler decides what to pre-compute; demand
//! pulls the value through exactly once. The injected embedder is the existing
//! `CachingEmbeddingProvider` in production, so identical text in *different* rooms
//! also collapses to one vector. Two memo layers compose: the per-element OnceCell
//! (fast, no map lookup) over the global content cache (cross-message, cross-eviction).

use std::sync::Arc;

use airc_core::TranscriptEvent;
use dashmap::DashMap;
use tokio::sync::OnceCell;
use uuid::Uuid;

use crate::cognition::embedding::EmbeddingProvider;

/// Soft bound on the shared element working set so a long-lived node servicing a
/// busy room can't grow it without limit (the substrate pressure doctrine — an
/// unbounded hot-path cache is a leak at scale). On overflow we evict one arbitrary
/// entry; airc remains the system of record, so an evicted element just re-resolves
/// on its next access. A proper LRU / `PagedResourcePool` tie-in is the follow-up.
const CHANNEL_ELEMENT_CACHE_MAX: usize = 20_000;

/// One room message + its lazily-computed, shared derived artifacts. Reference-
/// passed (`Arc<ChannelElement>`); one per airc message, handed to every persona.
pub struct ChannelElement {
    /// The airc message this element wraps. airc is the system of record; this is a
    /// reference view, never a parallel store.
    event: TranscriptEvent,
    /// Text body extracted once (None for non-text events — they carry no embedding).
    text: Option<String>,
    /// The envelope's TRUE author for a `chat/send` line (the human/web identity),
    /// `None` for a plain `say()` (transport peer IS the author). See `sender_id()`.
    logical_sender: Option<Uuid>,
    /// The embedder used to compute this element's vector. In production this is the
    /// content-addressed `CachingEmbeddingProvider`, so the compute-once property
    /// also holds across messages with identical text.
    embedder: Arc<dyn EmbeddingProvider>,
    /// Lazily-computed message embedding, memoized on the shared element so every
    /// persona reuses one vector. `None` once computed = no text / no signal.
    embedding: OnceCell<Option<Arc<Vec<f32>>>>,
}

impl ChannelElement {
    /// Build an element around an airc event. Cheap — the embedding is NOT computed
    /// here; it is pulled lazily on first `embedding()` (CBAR lazy getter).
    ///
    /// Text recovery goes through BOTH on-wire room-turn shapes, via the same ONE
    /// `chat_transcript_message` decoder as persona perception and the positron
    /// projection: a peer's plain-text `say()` (`Body::Text`) AND a human/web
    /// `chat/send` (`Body::Json` chat_transcript envelope). Task #177 live diagnosis
    /// (2026-07-16): this was the THIRD surface with the text-only blindness — human
    /// chat lines became `text: None` elements, so every ChannelDigest (every tick's
    /// room context) silently omitted them and personas read an active room as
    /// "quiet". The envelope also carries the TRUE logical sender (the human's
    /// identity), which `sender_id()` exposes so digests attribute the words to the
    /// speaker, not to the core's relay peer.
    fn new(event: TranscriptEvent, embedder: Arc<dyn EmbeddingProvider>) -> Self {
        // The ONE room-turn decoder (realtime_wire::room_turn_from_event) recovers
        // text + logical sender for BOTH wire shapes. A non-turn (presence,
        // event-bridge, decode error) is simply a text-less element here; the
        // skip-reason visibility lives on the perception path.
        let (text, logical_sender) = match crate::airc::realtime_wire::room_turn_from_event(&event)
        {
            Ok((sender, text)) => (Some(text), Some(sender)),
            Err(_) => (None, None),
        };
        Self {
            event,
            text,
            logical_sender,
            embedder,
            embedding: OnceCell::new(),
        }
    }

    /// The airc message this element references.
    pub fn event(&self) -> &TranscriptEvent {
        &self.event
    }

    /// airc's stable identity for this message (the element's cache key).
    pub fn event_id(&self) -> Uuid {
        self.event.event_id.as_uuid()
    }

    /// The message text, if it has a text body.
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// Who actually said this: the envelope's logical sender for a `chat/send`
    /// (the human/web identity that authored the line), else the transport peer
    /// (a persona's own `say()`). Attribution recovery, never fabrication — both
    /// candidates are real identities on the event.
    pub fn sender_id(&self) -> Uuid {
        self.logical_sender
            .unwrap_or_else(|| self.event.peer_id.as_uuid())
    }

    /// The message embedding — computed ONCE for this element and shared by every
    /// persona holding the `Arc`. `None` for a non-text message. The vector may be
    /// empty if the embedder degraded to "no signal" (faithful to the embedding
    /// layer's contract); callers treat empty as zero relevance, never panic.
    pub async fn embedding(&self) -> Option<Arc<Vec<f32>>> {
        self.embedding
            .get_or_init(|| async {
                let text = self.text.as_ref()?;
                Some(Arc::new(self.embedder.embed(text).await))
            })
            .await
            .clone()
    }
}

/// The shared element working set: airc `EventId` → one `Arc<ChannelElement>`,
/// handed to every persona so artifacts are computed once and referenced by all.
/// Process-lifetime, bounded; not a durable store (airc is).
pub struct ChannelElementCache {
    elements: DashMap<Uuid, Arc<ChannelElement>>,
    embedder: Arc<dyn EmbeddingProvider>,
    max: usize,
}

impl ChannelElementCache {
    /// Build against an embedder (the `CachingEmbeddingProvider` in production).
    pub fn new(embedder: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            elements: DashMap::new(),
            embedder,
            max: CHANNEL_ELEMENT_CACHE_MAX,
        }
    }

    /// Build with an explicit soft cap (tests / isolated benches).
    pub fn with_max(embedder: Arc<dyn EmbeddingProvider>, max: usize) -> Self {
        Self {
            elements: DashMap::new(),
            embedder,
            max: max.max(1),
        }
    }

    /// Resolve the shared element for an airc message. The FIRST caller constructs
    /// it; every caller after (any persona) gets the SAME `Arc` — one element,
    /// shared, artifacts computed once. Keyed by airc's stable `EventId`.
    pub fn get_or_insert(&self, event: TranscriptEvent) -> Arc<ChannelElement> {
        let id = event.event_id.as_uuid();
        // Fast path: an already-shared element (lock-free read on a hit).
        if let Some(existing) = self.elements.get(&id) {
            return existing.clone();
        }
        // Bound memory before admitting a genuinely new key. Soft cap: a small
        // TOCTOU against concurrent inserts is acceptable for a working-set bound.
        if self.elements.len() >= self.max && !self.elements.contains_key(&id) {
            // Pick the victim in its OWN statement so the DashMap iterator's shard
            // read-guard is dropped at the `;` — BEFORE `remove()` takes a write
            // lock. Holding the iterator alive across `remove` (e.g. inside an
            // `if let` scrutinee, whose temporaries outlive the body) deadlocks:
            // the write waits on a read the same expression still holds.
            let victim = self.elements.iter().next().map(|e| *e.key());
            if let Some(victim) = victim {
                self.elements.remove(&victim);
            }
        }
        let embedder = self.embedder.clone();
        // entry() is atomic per key: if two personas race the same new message,
        // exactly one element is kept and both receive that same shared `Arc`.
        self.elements
            .entry(id)
            .or_insert_with(|| Arc::new(ChannelElement::new(event, embedder)))
            .clone()
    }

    /// Resolve a batch of messages to their shared elements at once — the
    /// consolidation read (a channel's recent window, not a per-message loop).
    pub fn get_or_insert_batch(
        &self,
        events: impl IntoIterator<Item = TranscriptEvent>,
    ) -> Vec<Arc<ChannelElement>> {
        events.into_iter().map(|e| self.get_or_insert(e)).collect()
    }

    /// Number of shared elements currently resident.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::{
        Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptKind,
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Counts how many times it actually computed — proves the cache/OnceCell
    /// prevents recomputation across personas. (Local fixture: the canonical
    /// `EmbeddingProvider` fixtures in `embedding.rs` are private to that module's
    /// test mod; this is the same trivial counting shape.)
    struct CountingEmbedder {
        calls: AtomicUsize,
    }
    impl CountingEmbedder {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
            }
        }
    }
    #[async_trait]
    impl EmbeddingProvider for CountingEmbedder {
        fn id(&self) -> &str {
            "counting"
        }
        fn dim(&self) -> usize {
            4
        }
        async fn embed(&self, text: &str) -> Vec<f32> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let n = text.len() as f32;
            vec![n, n + 1.0, n + 2.0, n + 3.0]
        }
    }

    fn make_event(text: Option<&str>, lamport: u64) -> TranscriptEvent {
        TranscriptEvent {
            event_id: EventId::new(),
            room_id: RoomId::new(),
            peer_id: PeerId::new(),
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms: 1_000_000 + lamport,
            lamport,
            target: MentionTarget::Room(RoomId::new()),
            headers: Headers::default(),
            body: text.map(Body::text),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    // what this catches: #177's THIRD blind surface, fixed. A human/web `chat/send`
    // rides the wire as a Body::Json chat_transcript envelope — as_text() is None, and
    // before this fix the element got `text: None`, so every ChannelDigest (every
    // tick's room context) silently omitted human chat and personas read an active
    // room as "quiet" (live, 2026-07-16: Atlas said exactly that 16 min after an
    // operator message). The element must recover the text through the ONE
    // chat_transcript decoder AND attribute the LOGICAL sender (the human identity in
    // the envelope), not the relay peer that published it.
    #[tokio::test]
    async fn chat_send_envelope_yields_text_and_logical_sender() {
        use crate::airc::realtime::{
            AircRealtimeDelivery, AircRealtimeEnvelope, AircRealtimePayload,
            AircRealtimePayloadRef, AircRealtimeSchema,
        };
        use crate::airc::realtime_wire::{body_for_envelope, headers_for_envelope};

        let human = Uuid::new_v4(); // the operator identity chat/send carries
        let envelope = AircRealtimeEnvelope {
            event_id: Uuid::new_v4().to_string(),
            room_id: RoomId::new().as_uuid(),
            source_id: human.to_string(),
            target_id: None,
            created_at_ms: 1,
            delivery: AircRealtimeDelivery::Durable,
            trace_id: None,
            payload: AircRealtimePayload::ExistingSchema {
                payload: AircRealtimePayloadRef::inline(
                    AircRealtimeSchema::ChatTranscript,
                    serde_json::json!({
                        "messageId": Uuid::new_v4().to_string(),
                        "text": "Asha — reply with what you are doing right now.",
                        "senderId": human.to_string(),
                    }),
                ),
            },
        };
        let mut event = make_event(None, 7); // Json body, NOT text
        event.headers = headers_for_envelope(&envelope);
        event.body = Some(body_for_envelope(&envelope).expect("envelope encodes"));

        let cache = ChannelElementCache::new(Arc::new(CountingEmbedder::new()));
        let element = cache.get_or_insert(event);
        assert_eq!(
            element.text(),
            Some("Asha — reply with what you are doing right now."),
            "a human chat line must be VISIBLE in the digest, not a text:None ghost"
        );
        assert_eq!(
            element.sender_id(),
            human,
            "attributed to the human who wrote it, not the relay peer"
        );

        // And the plain-say sibling keeps transport-peer attribution.
        let say = make_event(Some("hello"), 8);
        let say_peer = say.peer_id.as_uuid();
        let el = cache.get_or_insert(say);
        assert_eq!(
            el.sender_id(),
            say_peer,
            "a say() is authored by its transport peer"
        );
    }

    // what this catches: THE REFERENCE-PASSED FRAME — resolving the same airc
    // message twice (two personas) yields the SAME Arc, not two copies. This is
    // what makes "one element, shared by all" structurally true.
    #[tokio::test]
    async fn same_event_resolves_to_shared_arc() {
        let cache = ChannelElementCache::new(Arc::new(CountingEmbedder::new()));
        let event = make_event(Some("the deploy went red"), 1);
        let a = cache.get_or_insert(event.clone());
        let b = cache.get_or_insert(event); // a second persona, same message
        assert!(
            Arc::ptr_eq(&a, &b),
            "same message must be one shared element"
        );
        assert_eq!(cache.len(), 1);
    }

    // what this catches: THE OPTIMIZATION — the shared element's embedding is
    // computed ONCE no matter how many personas (or how many calls) read it. This
    // is the 14-personas-reuse-one-vector win, enforced by the per-element OnceCell.
    #[tokio::test]
    async fn embedding_computed_once_across_personas() {
        let embedder = Arc::new(CountingEmbedder::new());
        let cache = ChannelElementCache::new(embedder.clone());
        let event = make_event(Some("shared content"), 1);

        // Three personas each resolve the element and read its embedding.
        let p1 = cache.get_or_insert(event.clone());
        let p2 = cache.get_or_insert(event.clone());
        let p3 = cache.get_or_insert(event);
        let e1 = p1.embedding().await;
        let e2 = p2.embedding().await;
        let e3 = p3.embedding().await;

        assert_eq!(e1, e2);
        assert_eq!(e2, e3);
        assert_eq!(
            embedder.calls.load(Ordering::SeqCst),
            1,
            "embedded ONCE across three personas + repeated reads"
        );
    }

    // what this catches: a non-text message has no embedding and never invokes the
    // embedder — non-text events don't fabricate a zero vector.
    #[tokio::test]
    async fn non_text_element_has_no_embedding() {
        let embedder = Arc::new(CountingEmbedder::new());
        let cache = ChannelElementCache::new(embedder.clone());
        let element = cache.get_or_insert(make_event(None, 1));
        assert!(element.embedding().await.is_none());
        assert_eq!(embedder.calls.load(Ordering::SeqCst), 0);
    }

    // what this catches: distinct messages are distinct shared elements (the cache
    // isn't collapsing different messages onto one entry).
    #[tokio::test]
    async fn distinct_events_are_distinct_elements() {
        let cache = ChannelElementCache::new(Arc::new(CountingEmbedder::new()));
        let a = cache.get_or_insert(make_event(Some("first"), 1));
        let b = cache.get_or_insert(make_event(Some("second"), 2));
        assert!(!Arc::ptr_eq(&a, &b));
        assert_eq!(cache.len(), 2);
    }

    // what this catches: the working-set bound holds — past the soft cap, admitting
    // a new element evicts one, so the cache never grows without limit. airc stays
    // the system of record, so an evicted element just re-resolves later.
    #[tokio::test]
    async fn cache_evicts_past_soft_cap() {
        let cache = ChannelElementCache::with_max(Arc::new(CountingEmbedder::new()), 2);
        cache.get_or_insert(make_event(Some("a"), 1));
        cache.get_or_insert(make_event(Some("b"), 2));
        cache.get_or_insert(make_event(Some("c"), 3));
        assert_eq!(cache.len(), 2, "soft cap bounds the working set");
    }

    // what this catches: the batch consolidation read resolves a whole window of
    // messages in one call (a room's recent-N), not a per-message loop.
    #[tokio::test]
    async fn batch_resolves_a_window() {
        let cache = ChannelElementCache::new(Arc::new(CountingEmbedder::new()));
        let elements = cache.get_or_insert_batch(vec![
            make_event(Some("one"), 1),
            make_event(Some("two"), 2),
            make_event(Some("three"), 3),
        ]);
        assert_eq!(elements.len(), 3);
        assert_eq!(cache.len(), 3);
    }
}
