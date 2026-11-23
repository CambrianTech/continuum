//! Her channel digest: a component that keeps her current room's digest ready for grounding to read,
//! rebuilt only when the room moved (object model 5.7: the first `BrainRegion` off the governor's tick).
//!
//! It watches her awareness strip on her mind's bus and knows the room's last moment and unread count
//! it built from ([`LastSeen`]): a strip whose room line is unchanged is known, and nothing is built.
//! When the line moved, it pages the room's recent transcript, builds the digest (no inference: element
//! embeddings are lazy) and publishes it into the shared digest buffer that grounding, nav and vitals
//! read. A chatty room never backs up the feed: the strip is a state, so lines that arrive while it is
//! building coalesce into the newest.

use std::sync::Arc;

use continuum_sage::component::{Component, Ctx, Known, LastSeen, Urgency, Wiring};
use continuum_sage::dataflow::Snapshot;
use continuum_sage::Qos;
use uuid::Uuid;

use crate::cognition::channel_digest::{ChannelDigestBuilder, DEFAULT_GROUNDING};
use crate::cognition::channel_digest_region::{DigestBuffer, PersonaChannelReader};
use crate::runtime::ready_buffer::ReadyBuffer;
use crate::persona::awareness::AwarenessSnapshot;

/// Her awareness strip, as the digest is handed it.
pub(super) struct Strip(Arc<AwarenessSnapshot>);

impl From<Snapshot<AwarenessSnapshot>> for Strip {
    fn from(snapshot: Snapshot<AwarenessSnapshot>) -> Self {
        Strip(snapshot.value)
    }
}

/// The moment of her room the digest was built from: the room, its newest timed moment, its unread.
type Built = (Uuid, u64, u32);

pub(super) struct Digest {
    pub(super) persona: Uuid,
    pub(super) builder: Arc<ChannelDigestBuilder>,
    pub(super) personas: Arc<dyn PersonaChannelReader>,
    pub(super) digests: Arc<DigestBuffer>,
    pub(super) built: LastSeen<Built>,
}

impl Component for Digest {
    type Input = Strip;
    const NAME: &'static str = "channel-digest";
    const QOS: Qos = Qos::Utility;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.watch::<AwarenessSnapshot>(super::AWARENESS, continuum_sage::component::Key::ONE);
    }

    async fn handle(&mut self, Strip(strip): Strip, ctx: &mut Ctx<Self>) {
        let Some((reader, room)) = self.personas.reader_and_room(self.persona) else { return };
        let moment = strip
            .lines
            .iter()
            .find(|line| line.activity == room)
            .map(|line| (room, line.last_activity_ms, line.unread));
        // A room her strip does not list yet is its own first moment: built once, then known until it
        // appears on her strip.
        let moment = match moment {
            Some(moment) => moment,
            None => (room, 0, 0),
        };
        if self.built.novelty(&moment) == continuum_sage::component::Novelty::KNOWN {
            ctx.need(Urgency::Content);
            return;
        }
        // Her unread lines and the grounding before them: sized by what her strip says is unread,
        // never a fixed page.
        let fetch = moment.2 as usize + DEFAULT_GROUNDING;
        match self.builder.build(self.persona, room, reader.as_ref(), fetch, DEFAULT_GROUNDING).await {
            Ok(digest) => {
                self.digests.publish((self.persona, room), Arc::new(digest));
                self.built.learn(&moment);
            }
            Err(error) => continuum_base::probe!(
                class = "mind.digest.build_failed",
                persona = %self.persona,
                room = %room,
                error = %error,
                "her room's digest did not build; the next change of her room tries again"
            ),
        }
        ctx.need(Urgency::Content);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use airc_core::{Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptEvent, TranscriptKind};
    use airc_lib::AircError;
    use async_trait::async_trait;
    use continuum_sage::component::{presence_key, start, Bus, Key, Presence, PRESENCE};
    use continuum_sage::dataflow::{OwnerId, Provenance, RunId, StageId, StageIncarnation};

    use super::*;
    use crate::cognition::attention::AttentionDial;
    use crate::cognition::channel_element::ChannelElementCache;
    use crate::cognition::conversation::salience::Salience;
    use crate::cognition::conversation::transcript_reader::AircTranscriptReader;
    use crate::persona::awareness::{fold, ActivityLine};

    struct NoopEmbedder;

    #[async_trait]
    impl crate::ai::embedding::EmbeddingProvider for NoopEmbedder {
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

    /// Her room's transcript, counting each page: a page is a build.
    struct Room {
        events: Vec<TranscriptEvent>,
        pages: AtomicUsize,
    }

    #[async_trait]
    impl AircTranscriptReader for Room {
        async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
            self.pages.fetch_add(1, Ordering::Relaxed);
            Ok(self.events.iter().take(limit).cloned().collect())
        }
        async fn page_recent_in(&self, _room: Option<RoomId>, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
            self.page_recent(limit).await
        }
    }

    struct Channels {
        persona: Uuid,
        room: Uuid,
        reader: Arc<Room>,
    }

    impl PersonaChannelReader for Channels {
        fn live_personas(&self) -> Vec<Uuid> {
            vec![self.persona]
        }
        fn reader_and_room(&self, persona: Uuid) -> Option<(Arc<dyn AircTranscriptReader>, Uuid)> {
            let reader: Arc<dyn AircTranscriptReader> = self.reader.clone();
            (persona == self.persona).then_some((reader, self.room))
        }
    }

    fn line(room: Uuid, last_activity_ms: u64, unread: u32) -> ActivityLine {
        ActivityLine {
            activity: room,
            name: "general".into(),
            unread,
            salience: Salience::QUIET,
            last_activity_ms,
            surprise: Default::default(),
            waiting_on_me: Vec::new(),
        }
    }

    fn said(room: RoomId, text: &str, lamport: u64) -> TranscriptEvent {
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

    /// Wait until the digest has handled `count` strips in all. Each handled input publishes two
    /// presence frames (Acting, then Ready) after the Ready it starts with, so the frame number says
    /// how many were handled even when the reader coalesced past one of them.
    async fn handled(presence: &mut continuum_sage::dataflow::Reader<Presence>, count: u64) {
        loop {
            let through = match presence.latest() {
                Some(frame) => frame.revision.through,
                None => 0,
            };
            if through >= 1 + 2 * count {
                return;
            }
            if presence.changed().await.is_err() {
                return;
            }
        }
    }

    // what this catches: a digest rebuilt on a clock (the governor's tick) or on every unchanged
    // strip, a bookmark advance with no new post leaving her digest stale (Codex's review on
    // 20c6586616: a digest is her per-resident projection), and a quiet room she joins cold never
    // built. It builds when her room's moment moves, and only then.
    #[tokio::test]
    async fn her_digest_is_rebuilt_when_her_room_moves_and_only_then() {
        let persona = Uuid::from_u128(0xd1);
        let room = RoomId::new();
        let reader = Arc::new(Room { events: vec![said(room, "a", 1), said(room, "b", 2)], pages: AtomicUsize::new(0) });
        let digests: Arc<DigestBuffer> = Arc::new(DigestBuffer::new());
        let bus = Bus::new();
        let mut strip = bus.publisher::<AwarenessSnapshot>(crate::persona::senses::AWARENESS, Key::ONE).expect("a fresh bus"); // JUSTIFIED: this bus was made on the line above
        let mut presence = bus.reader::<Presence>(PRESENCE, presence_key("channel-digest"));
        let digest = Digest {
            persona,
            builder: Arc::new(ChannelDigestBuilder::new(Arc::new(ChannelElementCache::new(Arc::new(NoopEmbedder))))),
            personas: Arc::new(Channels { persona, room: room.as_uuid(), reader: reader.clone() }),
            digests: digests.clone(),
            built: LastSeen::default(),
        };
        let _running = start(digest, &bus, OwnerId(persona), 1);
        let by = || Provenance { by: RunId { incarnation: StageIncarnation { stage: StageId::named("senses"), owner: OwnerId(persona), epoch: 1 }, seq: 1 }, from: Vec::new() };
        let mut show = |lines: Vec<ActivityLine>| strip.publish(fold(lines, None, AttentionDial::default(), 1_000, 0), by());
        let r = room.as_uuid();

        show(vec![line(r, 1_000_002, 2)]);
        handled(&mut presence, 1).await;
        assert_eq!(reader.pages.load(Ordering::Relaxed), 1, "her room's first moment is built");
        assert!(digests.peek(&(persona, r)).is_some(), "and published where grounding reads it");

        show(vec![line(r, 1_000_002, 2)]);
        handled(&mut presence, 2).await;
        assert_eq!(reader.pages.load(Ordering::Relaxed), 1, "an unchanged room is known: nothing built");

        show(vec![line(r, 1_000_002, 0)]);
        handled(&mut presence, 3).await;
        assert_eq!(reader.pages.load(Ordering::Relaxed), 2, "her bookmark moved with no new post: rebuilt");

        let quiet = Uuid::from_u128(0xd2);
        show(vec![line(quiet, 5, 0)]);
        handled(&mut presence, 4).await;
        assert_eq!(reader.pages.load(Ordering::Relaxed), 3, "her room not on her strip yet (a cold join): built once");
        show(vec![line(quiet, 6, 0)]);
        handled(&mut presence, 5).await;
        assert_eq!(reader.pages.load(Ordering::Relaxed), 3, "and known after");
    }
}
