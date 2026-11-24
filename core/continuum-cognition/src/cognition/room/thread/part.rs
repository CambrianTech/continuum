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

use continuum_grounding::room_thread::{RoomThread, Seed, Unread};

use super::line;
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
    /// One airc page of a room, read off her worker: the answer to her `reading`th read of it.
    Read { room: Uuid, reading: u64, page: Result<Vec<airc_core::TranscriptEvent>, Unread> },
}

impl From<Event<Arrival>> for Threading {
    fn from(event: Event<Arrival>) -> Self {
        Threading::Heard(event)
    }
}

struct Room {
    publisher: Publisher<RoomThread>,
    now: RoomThread,
    /// How many reads of the room's page she started: only the newest one's answer is taken, so
    /// a read that began before a gap can neither close it nor overwrite a newer read's outcome.
    reading: u64,
}

impl Room {
    /// Start a read of the room's page: its number.
    fn reading(&mut self) -> u64 {
        self.reading += 1;
        self.reading
    }

    /// The next revision from the answer to read `reading`; `None` when a newer read started since
    /// (its answer is the one taken).
    fn read_back(&self, reading: u64, page: Result<Vec<airc_core::TranscriptEvent>, Unread>) -> Option<RoomThread> {
        if reading != self.reading {
            return None;
        }
        Some(match page {
            // A page with nothing new leaves the lines as they are; only the seed changes.
            Ok(page) => match self.now.with(page.iter().filter_map(line), continuum_context::history::ROOM_HISTORY_NEED) {
                Some(merged) => merged.seeded(Seed::Read),
                None => self.now.seeded(Seed::Read),
            },
            Err(why) => {
                continuum_base::probe!(class = "mind.thread.unread", room = %self.now.room, why = %why, "her room's airc page did not read: her thread holds only what she heard live");
                self.now.seeded(Seed::Unread(why))
            }
        })
    }
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
                self.rooms.insert(room, Room { publisher, now: RoomThread::new(room), reading: 0 });
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
        let Some(entry) = self.rooms.get_mut(&room) else { return };
        let reading = entry.reading();
        let Some(reader) = self.reader.clone() else {
            self.read_back(room, reading, Err(Unread::NoRuntime), ctx);
            return;
        };
        let runtime = tokio::runtime::Handle::current();
        let needs = continuum_context::history::ROOM_HISTORY_NEED;
        ctx.off(
            move || runtime.block_on(reader.page_recent_in(Some(airc_core::RoomId::from_uuid(room)), needs)).map_err(|error| Unread::Airc(Arc::new(error))),
            move |page| Threading::Read { room, reading, page },
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
        if let Some(next) = line(event.value.event.event()).and_then(|line| entry.now.with([line], continuum_context::history::ROOM_HISTORY_NEED)) {
            Self::publish(entry, next, ctx);
        }
        ctx.emit(&mut self.forward, Threaded(Arc::clone(&event.value)));
    }

    fn read_back(&mut self, room: Uuid, reading: u64, page: Result<Vec<airc_core::TranscriptEvent>, Unread>, ctx: &Ctx<Self>) {
        let Some(entry) = self.rooms.get_mut(&room) else { return };
        match entry.read_back(reading, page) {
            Some(next) => Self::publish(entry, next, ctx),
            None => continuum_base::probe!(class = "mind.thread.read_superseded", room = %room, reading, newest = entry.reading, "a newer read of her room started; this answer is not taken"),
        }
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
            Threading::Read { room, reading, page } => self.read_back(room, reading, page, ctx),
        }
        self.read_taken(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused() -> Unread {
        Unread::Airc(Arc::new(airc_lib::AircError::NoCurrentRoom))
    }

    // what this catches: Codex on #6244: a seed read A that began before a gap, answering after the
    // gap's reread B began, must neither close the gap with its pre-gap page nor overwrite B's
    // outcome, whichever order the two answers arrive in and whichever of them failed.
    #[test]
    fn only_the_newest_read_of_a_room_is_taken_whatever_order_the_answers_come_in() {
        let bus = Bus::new();
        let room = Uuid::from_u128(9);
        for (a, b, a_first) in [(Ok(vec![]), Err(refused()), false), (Err(refused()), Ok(vec![]), false), (Ok(vec![]), Err(refused()), true), (Err(refused()), Ok(vec![]), true)] {
            let mut entry = Room { publisher: bus.publisher(THREAD, Key::of(Uuid::new_v4())).unwrap(), now: RoomThread::new(room), reading: 0 };
            let seed = entry.reading();
            entry.now = entry.now.gapped();
            let reread = entry.reading();
            let b_read = b.is_ok();
            let (first, second) = if a_first { ((seed, a), (reread, b)) } else { ((reread, b), (seed, a)) };
            for (reading, page) in [first, second] {
                let stale = reading == seed;
                match entry.read_back(reading, page) {
                    Some(next) => {
                        assert!(!stale, "the seed read began before the gap; its answer is not taken");
                        entry.now = next;
                    }
                    None => assert!(stale, "the newest read's answer is taken"),
                }
            }
            assert_eq!(entry.now.seed == Seed::Read, b_read, "the thread says what the newest read found");
            assert_eq!(entry.now.gapped, !b_read, "only a read begun after the gap closes it");
        }
    }
}
