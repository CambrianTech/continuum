//! THE WORK-BOARD INDEX: the boards of the rooms a citizen stands in, each read ONCE and
//! re-read only when that room's board changed. Her held work, her owned cards, the room
//! that carries a card: every self-tick and claim renewal asks this index, never the boards.
//!
//! ### Why (card 522419e5, the store-route sweep's CB1)
//!
//! "Her held cards" used to be re-derived on every self-tick and every claim renewal by
//! projecting the WHOLE board of EVERY subscribed room (about 68 on the M5), filtering
//! `owner == me` in Rust, and then walking the rooms a second time to find the focus card's
//! room. Measured on the M5 core log (probe `board.read.phase`, 2026-11-07): about 2.3 slow
//! board reads a second, p50 1.7 s, p90 3.4 s, worst 151 s, each holding airc's single store
//! connection, which starved every other reader and fed the timeout-as-death recovery loop.
//! The cost grew with rooms times board size, not with her 0-2 cards.
//!
//! ### The shape
//!
//! - One index per citizen peer ([`for_peer`]), holding the last read of each room's board
//!   and HER cards on it, precomputed at read time, so a held-work query is O(rooms + her
//!   cards) and performs no read while nothing changed.
//! - A room is stale only when (a) it was never read, (b) a work EVENT for that room crossed
//!   the core's one feed ([`room_changed`], called by `airc::inbound_attach`, which attaches
//!   every room a resident stands in), (c) she wrote to it herself ([`wrote`], [`wrote_card`]:
//!   read-your-writes, before the echo returns), or (d) a claim lease on the cached board
//!   crossed its expiry since the read, so a heartbeat whose echo was missed cannot make a
//!   live claim read lapsed. Only the stale room is re-read.
//! - One read per stale room at a time: concurrent askers wait on the leader's result.
//! - Every read is probed with its reason (`board.index.read`), the live receipt that the
//!   per-tick rebuild is gone.
//!
//! The board fold itself stays airc's (`project_room_work_board`); this index owns only WHEN
//! a room is read and what of it her loops keep.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use airc_core::{PeerId, RoomId};
use airc_lib::{Airc, AircError, Room, WorkCard, WorkCardId};
use airc_work::BoardSnapshot;
use async_trait::async_trait;
use continuum_base::time::EpochMs;
use tokio::sync::watch;

#[cfg(test)]
mod tests;

/// Why a room's board was read: the field the live read groups `board.index.read` by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadReason {
    /// Never read by this index (startup, a newly joined room).
    FirstSight,
    /// A work event for this room arrived since the last read.
    RoomChanged,
    /// A claim lease on the cached board crossed its expiry since the read.
    LeaseCrossed,
}

impl ReadReason {
    fn label(self) -> &'static str {
        match self {
            Self::FirstSight => "first_sight",
            Self::RoomChanged => "room_changed",
            Self::LeaseCrossed => "lease_crossed",
        }
    }

    fn slot(self) -> usize {
        match self {
            Self::FirstSight => 0,
            Self::RoomChanged => 1,
            Self::LeaseCrossed => 2,
        }
    }
}

/// The one read the index makes: a room's whole board, folded by airc. Production is
/// `Airc::project_room_work_board`; tests count calls.
#[async_trait]
pub trait BoardFetch: Send + Sync {
    async fn fetch(&self, room: &Room) -> Result<BoardSnapshot, AircError>;
}

#[async_trait]
impl BoardFetch for Airc {
    async fn fetch(&self, room: &Room) -> Result<BoardSnapshot, AircError> {
        Ok(self
            .project_room_work_board(room, airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
            .await?
            .snapshot())
    }
}

/// What a room's board must have seen to be current: the room's change count, bumped by each
/// work event and each of her own writes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Revision(u64);

/// One room's last good read.
struct RoomRead {
    board: Arc<BoardSnapshot>,
    /// Her cards on this board (owner = the index's peer), whole, in board order.
    mine: Vec<WorkCard>,
    at: Revision,
    /// The earliest claim expiry on this board that was still in the future when the read
    /// started: past it, a cached lease may be stale (a missed heartbeat echo).
    next_lease_crossing_ms: Option<u64>,
}

/// A read in flight, shared by every asker of the same room at the same revision.
#[derive(Clone)]
enum Flight {
    Pending,
    Done(Arc<BoardSnapshot>),
    Failed,
}

#[derive(Default)]
struct IndexState {
    changes: HashMap<RoomId, Revision>,
    reads: HashMap<RoomId, RoomRead>,
    flights: HashMap<RoomId, (Revision, watch::Receiver<Flight>)>,
}

impl IndexState {
    fn revision(&self, room: RoomId) -> Revision {
        self.changes.get(&room).copied().unwrap_or_default() // unwrap_or_default: no change to this room yet is revision zero
    }

    fn bump(&mut self, room: RoomId) {
        self.changes.entry(room).or_default().0 += 1;
    }

    /// The last read of `room` when it is current, else why it must be read now.
    fn freshness(&self, room: RoomId, now_ms: u64) -> Result<Arc<BoardSnapshot>, ReadReason> {
        let Some(read) = self.reads.get(&room) else {
            return Err(ReadReason::FirstSight);
        };
        if read.at != self.revision(room) {
            return Err(ReadReason::RoomChanged);
        }
        if read.next_lease_crossing_ms.is_some_and(|at| at <= now_ms) {
            return Err(ReadReason::LeaseCrossed);
        }
        Ok(Arc::clone(&read.board))
    }
}

/// What an asker does for one room, decided in one critical section.
enum Step {
    Current(Arc<BoardSnapshot>),
    Wait(watch::Receiver<Flight>),
    Lead(Revision, ReadReason, watch::Sender<Flight>),
}

/// One citizen's work-board index. See the module doc.
pub(crate) struct BoardIndex {
    me: PeerId,
    state: watch::Sender<IndexState>,
    reads: [AtomicU64; 3],
}

impl BoardIndex {
    fn new(me: PeerId) -> Self {
        Self {
            me,
            state: watch::channel(IndexState::default()).0,
            reads: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)],
        }
    }

    /// A work event for `room` arrived: its next ask re-reads it, no other room.
    pub fn room_changed(&self, room: RoomId) {
        self.state.send_modify(|state| state.bump(room));
    }

    /// A write to `card`: each room whose last read carries it is stale.
    pub fn card_written(&self, card: WorkCardId) {
        self.state.send_modify(|state| {
            let rooms: Vec<RoomId> = state
                .reads
                .iter()
                .filter(|(_, read)| read.board.cards.iter().any(|c| c.card_id == card))
                .map(|(room, _)| *room)
                .collect();
            for room in rooms {
                state.bump(room);
            }
        });
    }

    /// The board of `room`, read only when stale. An unreadable board is the read's own
    /// typed error, never an empty board.
    pub async fn board(
        &self,
        fetch: &dyn BoardFetch,
        room: &Room,
        now_ms: u64,
    ) -> Result<Arc<BoardSnapshot>, AircError> {
        let mut may_wait = true;
        loop {
            match self.decide(room.channel, now_ms, may_wait) {
                Step::Current(board) => return Ok(board),
                Step::Lead(at, reason, done) => {
                    return self.read(fetch, room, at, reason, done, now_ms).await;
                }
                Step::Wait(mut flight) => {
                    // The leader's result, when it lands. A failed or cancelled leader leaves
                    // this asker to lead its own read, so its error is its own typed error.
                    let landed = flight.wait_for(|f| !matches!(f, Flight::Pending)).await.map(|f| f.clone());
                    if let Ok(Flight::Done(board)) = landed {
                        return Ok(board);
                    }
                    may_wait = false;
                }
            }
        }
    }

    /// Every board of `rooms` that reads, each re-read only when stale, with its room. An
    /// unreadable room is skipped and probed (one bad room must not erase her holds in the
    /// others), and stays stale, so the next ask retries it.
    pub async fn boards(
        &self,
        fetch: &dyn BoardFetch,
        rooms: &[Room],
        now_ms: u64,
    ) -> Vec<(Room, Arc<BoardSnapshot>)> {
        let mut boards = Vec::with_capacity(rooms.len());
        for room in rooms {
            match self.board(fetch, room, now_ms).await {
                Ok(board) => boards.push((room.clone(), board)),
                Err(error) => continuum_base::probe!(
                    class = "persona.claim.board_room_unreadable",
                    room = %room.channel,
                    error = %error,
                    "a room's board did not read; her holds elsewhere still count, and this room is retried on the next ask"
                ),
            }
        }
        boards
    }

    /// Every card on the boards of `rooms` whose owner is her, in ANY state, whole, with its
    /// room: O(rooms + her cards) when nothing changed.
    pub async fn owned(
        &self,
        fetch: &dyn BoardFetch,
        rooms: &[Room],
        now_ms: u64,
    ) -> Vec<(Room, WorkCard)> {
        let mut owned = Vec::new();
        for room in rooms {
            if let Err(error) = self.board(fetch, room, now_ms).await {
                continuum_base::probe!(
                    class = "persona.claim.board_room_unreadable",
                    room = %room.channel,
                    error = %error,
                    "a room's board did not read; her holds elsewhere still count, and this room is retried on the next ask"
                );
                continue;
            }
            let state = self.state.borrow();
            if let Some(read) = state.reads.get(&room.channel) {
                owned.extend(read.mine.iter().map(|card| (room.clone(), card.clone())));
            }
        }
        owned
    }

    /// One critical section: current, join a live flight (when `may_wait`), or lead a read.
    fn decide(&self, room: RoomId, now_ms: u64, may_wait: bool) -> Step {
        // Every branch of the section below assigns `step`; this starting value is a flight
        // whose leader is already gone, which an asker treats as "lead it yourself".
        let mut step = Step::Wait(watch::channel(Flight::Failed).1);
        self.state.send_if_modified(|state| {
            let reason = match state.freshness(room, now_ms) {
                Ok(board) => {
                    step = Step::Current(board);
                    return false;
                }
                Err(reason) => reason,
            };
            let at = state.revision(room);
            if may_wait {
                if let Some((flight_at, flight)) = state.flights.get(&room) {
                    // A flight whose leader was dropped (cancelled) is no flight at all.
                    if *flight_at == at && flight.has_changed().is_ok() {
                        step = Step::Wait(flight.clone());
                        return false;
                    }
                }
            }
            let (done, flight) = watch::channel(Flight::Pending);
            state.flights.insert(room, (at, flight));
            step = Step::Lead(at, reason, done);
            true
        });
        step
    }

    async fn read(
        &self,
        fetch: &dyn BoardFetch,
        room: &Room,
        at: Revision,
        reason: ReadReason,
        done: watch::Sender<Flight>,
        now_ms: u64,
    ) -> Result<Arc<BoardSnapshot>, AircError> {
        let channel = room.channel;
        let started = std::time::Instant::now();
        let fetched = fetch.fetch(room).await;
        let read_ms = started.elapsed().as_millis() as u64;
        let reads_for_reason = self.reads[reason.slot()].fetch_add(1, Ordering::Relaxed) + 1;
        continuum_base::probe!(
            class = "board.index.read",
            peer = %self.me,
            room = %channel,
            reason = reason.label(),
            read_ms,
            ok = fetched.is_ok(),
            reads_for_reason,
            "a room's board was read because it was stale; a held-work ask with nothing changed reads none"
        );
        let result = fetched.map(Arc::new);
        let me = self.me;
        self.state.send_modify(|state| {
            if state.flights.get(&channel).is_some_and(|(flight_at, _)| *flight_at == at) {
                state.flights.remove(&channel);
            }
            if let Ok(board) = &result {
                state.reads.insert(
                    channel,
                    RoomRead {
                        mine: board.cards.iter().filter(|c| c.owner == Some(me)).cloned().collect(),
                        next_lease_crossing_ms: next_lease_crossing(board, now_ms),
                        board: Arc::clone(board),
                        at,
                    },
                );
            }
        });
        done.send_replace(match &result {
            Ok(board) => Flight::Done(Arc::clone(board)),
            Err(_) => Flight::Failed,
        });
        result
    }

    /// Reads made so far for `reason` (the probe's cumulative field).
    #[cfg(test)]
    fn reads_for(&self, reason: ReadReason) -> u64 {
        self.reads[reason.slot()].load(Ordering::Relaxed)
    }

    #[cfg(test)]
    fn reads_total(&self) -> u64 {
        self.reads.iter().map(|r| r.load(Ordering::Relaxed)).sum()
    }
}

/// PURE: the earliest live claim expiry on `board` strictly after `read_at_ms`.
fn next_lease_crossing(board: &BoardSnapshot, read_at_ms: u64) -> Option<u64> {
    board
        .cards
        .iter()
        .filter(|c| c.owner.is_some())
        .filter_map(|c| c.claim_expires_at_ms)
        .filter(|at| *at > read_at_ms)
        .min()
}

/// Every citizen's index on this core, by peer.
static INDEXES: LazyLock<Mutex<HashMap<PeerId, Arc<BoardIndex>>>> = LazyLock::new(Default::default);

/// The index of `peer`'s boards (created empty on first ask: every room first-sight).
pub(crate) fn for_peer(peer: PeerId) -> Arc<BoardIndex> {
    let mut indexes = INDEXES.lock().unwrap_or_else(|e| e.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
    Arc::clone(indexes.entry(peer).or_insert_with(|| Arc::new(BoardIndex::new(peer))))
}

/// A work event for `room` crossed the core's ONE FEED (`airc::inbound_attach`, the seam every
/// resident's rooms are attached to, at most one stream per room): every index re-reads that
/// room on its next ask, and no other room.
pub fn room_changed(room: RoomId) {
    let indexes: Vec<Arc<BoardIndex>> = INDEXES
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // poisoned lock = read the last state, same policy as every lock in this crate
        .values()
        .cloned()
        .collect();
    for index in indexes {
        index.room_changed(room);
    }
}

/// HER OWN WRITE to `room`'s board: her next ask re-reads it, before the write's echo comes
/// back on her stream (read-your-writes; the echo marks every index again, once).
pub fn wrote(airc: &Airc, room: &Room) {
    for_peer(airc.peer_id()).room_changed(room.channel);
}

/// HER OWN WRITE to `card`, wherever it lives: every room whose last read carries the card is
/// re-read on her next ask. A card on no board she has read needs no mark: that board is read
/// at first sight.
pub fn wrote_card(airc: &Airc, card: WorkCardId) {
    for_peer(airc.peer_id()).card_written(card);
}

/// The rooms `airc`'s scope subscribes to, in subscription order.
async fn subscribed_rooms(airc: &Airc) -> Result<Vec<Room>, AircError> {
    Ok(airc.subscription_set().await?.all().map(|sub| sub.as_room()).collect())
}

/// Every card the boards she stands in show with owner = her, in ANY state, whole, with its
/// room: her subscribed rooms, plus the rooms her FOLLOWED claims landed in (card ed2c9417: a
/// claim that followed its card to a room she does not subscribe to is still hers; one owned
/// set feeds the work gate and the renewal, so they cannot disagree). Read through her index.
pub async fn scoped_board_owned_by(airc: &Airc) -> Result<Vec<(Room, WorkCard)>, AircError> {
    let me = airc.peer_id();
    let index = for_peer(me);
    let now_ms = EpochMs::now().millis(); // pre-1970 reads 0: every lease then reads as not yet crossed, so the read stays current
    let rooms = subscribed_rooms(airc).await?;
    let mut owned = index.owned(airc, &rooms, now_ms).await;
    let walked_rooms: Vec<uuid::Uuid> = rooms.iter().map(|room| room.channel.as_uuid()).collect();
    let walked_cards: Vec<uuid::Uuid> = owned.iter().map(|(_, card)| card.card_id.as_uuid()).collect();
    let followed = followed_beyond_rooms(
        &walked_rooms,
        &walked_cards,
        &crate::persona::held_claims::held(me.as_uuid()),
    );
    for claim in followed {
        let found = index
            .owned(airc, std::slice::from_ref(&claim.room), now_ms)
            .await
            .into_iter()
            .find(|(_, card)| card.card_id.as_uuid() == claim.card_id);
        owned.extend(found);
    }
    Ok(owned)
}

/// The cards she HOLDS right now (owner = her, lease live, not settled), with the room whose
/// board carries each: renewal must never infer a room from her changing current room.
pub async fn scoped_board_held_by(airc: &Airc) -> Result<Vec<(Room, WorkCard)>, AircError> {
    let now_ms = EpochMs::now().millis(); // pre-1970 reads 0: every lease then reads live, the conservative side
    Ok(crate::cognition::card_holder::held_of_owned(scoped_board_owned_by(airc).await?, now_ms))
}

/// The cards she holds, by the board alone. The one source for "her held work": the pull's
/// WIP=1 guard, the renewal loop, anything that asks. Never the work roster: it is rebuilt
/// after a boot and reads empty for its first seconds (2026-09-13 08:40:50Z, a second pull).
pub async fn board_held_by(airc: &Airc) -> Result<Vec<WorkCard>, AircError> {
    Ok(scoped_board_held_by(airc).await?.into_iter().map(|(_, card)| card).collect())
}

/// ONE card as the boards she stands in carry it, with its room: the wake's "what became of my
/// card". `None` when no readable subscribed board has it.
pub async fn card(airc: &Airc, card_id: WorkCardId) -> Option<(Room, WorkCard)> {
    let rooms = subscribed_rooms(airc).await.ok()?;
    let now_ms = EpochMs::now().millis(); // pre-1970 reads 0: leases read as not yet crossed
    for_peer(airc.peer_id())
        .boards(airc, &rooms, now_ms)
        .await
        .into_iter()
        .find_map(|(room, board)| {
            board.cards.iter().find(|c| c.card_id == card_id).cloned().map(|card| (room, card))
        })
}

/// The board of one subscribed room, through her index. `None` names the scope's current room.
/// A room she is not subscribed to is `NotSubscribed`, never another room's board.
pub async fn room_board(airc: &Airc, room: Option<uuid::Uuid>) -> Result<Arc<BoardSnapshot>, AircError> {
    let room = match room {
        Some(id) => subscribed_rooms(airc)
            .await?
            .into_iter()
            .find(|r| r.channel.as_uuid() == id)
            .ok_or_else(|| {
                AircError::NotSubscribed(format!(
                    "work board requested for room {id}, which this scope is not subscribed to; refusing to substitute the default room's board"
                ))
            })?,
        None => airc.current_room().await?,
    };
    let now_ms = EpochMs::now().millis(); // pre-1970 reads 0: leases read as not yet crossed
    for_peer(airc.peer_id()).board(airc, &room, now_ms).await
}

/// PURE: the recorded claims a subscription walk could not have seen. Their room is not one
/// she walked, and the walk did not already find the card. Those are the ones
/// [`scoped_board_owned_by`] reads on their own room's board.
pub(crate) fn followed_beyond_rooms(
    walked_rooms: &[uuid::Uuid],
    walked_cards: &[uuid::Uuid],
    recorded: &[crate::persona::held_claims::HeldClaim],
) -> Vec<crate::persona::held_claims::HeldClaim> {
    recorded
        .iter()
        .filter(|h| !walked_rooms.contains(&h.room.channel.as_uuid()))
        .filter(|h| !walked_cards.contains(&h.card_id))
        .cloned()
        .collect()
}
