//! The part of her that keeps her thread of each room she is in: a SAGE component on her mind's bus.
//!
//! It hears the one feed, takes each line of her rooms into that room's thread and publishes the new
//! revision, then hands the event on to her senses. So a wake for an event is published only after
//! the revision holding it exists: a turn built for it reads a thread that has it, by construction,
//! never by timing.

use std::collections::HashMap;
use std::sync::Arc;

use continuum_sage::component::{Bus, Component, Ctx, Key, Wiring};
use continuum_sage::dataflow::{Emitter, Event, Publisher, TopicName};
use continuum_sage::Qos;
use uuid::Uuid;

use super::{Line, RoomThread, Seed, Unread};
use crate::cognition::conversation::transcript_reader::AircTranscriptReader;
use crate::cognition::room::feed::{Arrival, FEED};

/// Her thread of a room, on her mind's bus, keyed by the room.
pub const THREAD: TopicName = TopicName("mind/thread");

/// Each room event of her rooms, handed on after her thread took it in: what her senses hear.
pub const THREADED: TopicName = TopicName("mind/threaded");

/// A room event her thread took in.
pub struct Threaded(pub Arc<Arrival>);

/// What wakes the part.
pub enum Threading {
    /// A room event from the one feed.
    Heard(Event<Arrival>),
    /// Her memberships: the rooms she is in.
    Joined(Vec<Uuid>),
    /// One airc page of a room, read off her worker.
    Read { room: Uuid, page: Result<Vec<airc_core::TranscriptEvent>, Unread> },
}

impl From<Event<Arrival>> for Threading {
    fn from(event: Event<Arrival>) -> Self {
        Threading::Heard(event)
    }
}

struct Room {
    publisher: Publisher<RoomThread>,
    now: RoomThread,
}

/// Her threads.
pub struct Thread {
    bus: Arc<Bus>,
    /// Where a room's page is read from: her own airc runtime; `None` where she has none here (her
    /// threads then say they are unread).
    reader: Option<Arc<dyn AircTranscriptReader>>,
    forward: Emitter<Threaded>,
    rooms: HashMap<Uuid, Room>,
    /// Rooms taken since her last input: their first revision is published and their page read
    /// on her next input (a constructor has no context to do either).
    unread: Vec<Uuid>,
    /// The last feed revision she heard: a later one that skips past it is a gap.
    heard_through: u64,
    /// How many of the feed's events she keeps unheard while busy.
    window: std::num::NonZeroUsize,
}

impl Thread {
    /// Her threads on `bus`, hearing the one feed with `window` of slack.
    /// Her threads on `bus`, hearing the one feed with `window` of slack, already threading `rooms`
    /// (the rooms her saved mind state holds), so no line in them can arrive before she threads it.
    pub fn new(bus: Arc<Bus>, reader: Option<Arc<dyn AircTranscriptReader>>, window: std::num::NonZeroUsize, rooms: &[Uuid]) -> Result<Self, continuum_sage::component::Taken> {
        let forward = bus.emitter::<Threaded>(THREADED)?;
        let mut thread = Self { bus, reader, forward, rooms: HashMap::new(), unread: Vec::new(), heard_through: 0, window };
        for room in rooms {
            thread.take(*room);
        }
        Ok(thread)
    }

    /// Start threading `room`: its one writer claimed, its page to be read. False when it already
    /// is threaded or its writer is someone else's.
    fn take(&mut self, room: Uuid) -> bool {
        if self.rooms.contains_key(&room) {
            return false;
        }
        // A room she left keeps its thread (and its one writer) until she drops: rejoining reads
        // it again rather than claiming a second writer.
        match self.bus.publisher::<RoomThread>(THREAD, Key::of(room)) {
            Ok(publisher) => {
                self.rooms.insert(room, Room { publisher, now: RoomThread::new(room) });
                self.unread.push(room);
                true
            }
            Err(taken) => {
                continuum_base::probe!(class = "mind.thread.unclaimed", room = %room, topic = taken.0.0, "her thread of this room has another writer; she does not thread it");
                false
            }
        }
    }

    /// Publish each room taken since her last input, and read its page.
    fn read_taken(&mut self, ctx: &Ctx<Self>) {
        for room in std::mem::take(&mut self.unread) {
            // A line heard in it since already published its first revision.
            if let Some(entry) = self.rooms.get_mut(&room).filter(|entry| entry.now.revision == 0) {
                let now = entry.now.clone();
                Self::publish(entry, now, ctx);
            }
            self.read(room, ctx);
        }
    }

    fn publish(room: &mut Room, next: RoomThread, ctx: &Ctx<Self>) {
        room.now = next.clone();
        ctx.publish(&mut room.publisher, next);
    }

    /// Read one page of `room` off her worker; it comes back as [`Threading::Read`].
    fn read(&mut self, room: Uuid, ctx: &Ctx<Self>) {
        let Some(reader) = self.reader.clone() else {
            self.read_back(room, Err(Unread::NoRuntime), ctx);
            return;
        };
        let runtime = tokio::runtime::Handle::current();
        let needs = continuum_context::history::ROOM_HISTORY_NEED;
        ctx.off(
            move || runtime.block_on(reader.page_recent_in(Some(airc_core::RoomId::from_uuid(room)), needs)).map_err(|error| Unread::Airc(Arc::new(error))),
            move |page| Threading::Read { room, page },
        );
    }

    fn joined(&mut self, rooms: Vec<Uuid>) {
        for room in rooms {
            self.take(room);
        }
    }

    fn heard(&mut self, event: Event<Arrival>, ctx: &mut Ctx<Self>) {
        let through = event.revision.through;
        if self.heard_through != 0 && through > self.heard_through + 1 {
            // She missed feed events: any of her rooms may lack lines until read again.
            continuum_base::probe!(class = "mind.thread.gap", missed = through - self.heard_through - 1, "her feed window overran; her threads read their rooms again");
            let rooms: Vec<Uuid> = self.rooms.keys().copied().collect();
            for room in rooms {
                if let Some(entry) = self.rooms.get_mut(&room) {
                    let next = entry.now.gapped();
                    Self::publish(entry, next, ctx);
                }
                self.read(room, ctx);
            }
        }
        self.heard_through = through;
        let room = event.value.event.event().room_id.as_uuid();
        let Some(entry) = self.rooms.get_mut(&room) else { return };
        if let Some(next) = Line::of(event.value.event.event()).and_then(|line| entry.now.with([line], continuum_context::history::ROOM_HISTORY_NEED)) {
            Self::publish(entry, next, ctx);
        }
        ctx.emit(&mut self.forward, Threaded(Arc::clone(&event.value)));
    }

    fn read_back(&mut self, room: Uuid, page: Result<Vec<airc_core::TranscriptEvent>, Unread>, ctx: &Ctx<Self>) {
        let Some(entry) = self.rooms.get_mut(&room) else { return };
        let next = match page {
            Ok(page) => {
                // A page with nothing new leaves the lines as they are; only the seed changes.
                match entry.now.with(page.iter().filter_map(Line::of), continuum_context::history::ROOM_HISTORY_NEED) {
                    Some(merged) => merged.seeded(Seed::Read),
                    None => entry.now.seeded(Seed::Read),
                }
            }
            Err(why) => {
                continuum_base::probe!(class = "mind.thread.unread", room = %room, why = %why, "her room's airc page did not read: her thread holds only what she heard live");
                entry.now.seeded(Seed::Unread(why))
            }
        };
        Self::publish(entry, next, ctx);
    }
}

impl Component for Thread {
    type Input = Threading;
    const NAME: &'static str = "thread";
    const QOS: Qos = Qos::Interactive;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.hear_on::<Arrival>(crate::cognition::room::feed::node_bus(), FEED, self.window);
    }

    async fn handle(&mut self, input: Threading, ctx: &mut Ctx<Self>) {
        match input {
            Threading::Heard(event) => self.heard(event, ctx),
            Threading::Joined(rooms) => self.joined(rooms),
            Threading::Read { room, page } => self.read_back(room, page, ctx),
        }
        self.read_taken(ctx);
    }
}
