#![cfg(test)]
use super::*;
use std::sync::atomic::AtomicUsize;
use tempfile::TempDir;

/// A board source that counts every read, per room, and can refuse one room.
struct CountingFetch {
    boards: HashMap<RoomId, Vec<WorkCard>>,
    reads: Mutex<HashMap<RoomId, usize>>,
    total: AtomicUsize,
    refuse: Mutex<Option<RoomId>>,
}

impl CountingFetch {
    fn new(boards: HashMap<RoomId, Vec<WorkCard>>) -> Self {
        Self { boards, reads: Mutex::default(), total: AtomicUsize::new(0), refuse: Mutex::default() }
    }

    fn total(&self) -> usize {
        self.total.load(Ordering::SeqCst)
    }

    fn reads_of(&self, room: RoomId) -> usize {
        self.reads.lock().expect("reads lock").get(&room).copied().unwrap_or(0) // unwrap_or: a room never read counts zero reads
    }
}

#[async_trait]
impl BoardFetch for CountingFetch {
    async fn fetch(&self, room: &Room) -> Result<BoardSnapshot, AircError> {
        // Yield so concurrent askers really overlap one read in flight.
        tokio::task::yield_now().await;
        self.total.fetch_add(1, Ordering::SeqCst);
        *self.reads.lock().expect("reads lock").entry(room.channel).or_default() += 1;
        if *self.refuse.lock().expect("refuse lock") == Some(room.channel) {
            return Err(AircError::NotSubscribed(format!("{} refused in this fixture", room.name)));
        }
        Ok(snapshot(self.boards.get(&room.channel).cloned().unwrap_or_default())) // unwrap_or_default: an unseeded room is an empty board
    }
}

fn snapshot(cards: Vec<WorkCard>) -> BoardSnapshot {
    BoardSnapshot {
        cards,
        submission_reviews: Vec::new(),
        lanes: Vec::new(),
        workspaces: Vec::new(),
        repo_tracking: Vec::new(),
        pull_requests: Vec::new(),
        manager_hats: Vec::new(),
        agent_availability: Vec::new(),
        hygiene_reports: Vec::new(),
    }
}

fn card(owner: Option<PeerId>, expires_at_ms: Option<u64>) -> WorkCard {
    WorkCard {
        card_id: WorkCardId::new(),
        repo: airc_work::RepoId::new("acme/continuum").expect("valid repo id in fixture"),
        title: "t".to_string(),
        body: None,
        priority: airc_work::Priority::P2,
        lane_id: None,
        state: airc_work::CardState::InProgress,
        owner,
        claim_provenance: None,
        claim_id: owner.map(|_| airc_work::ClaimId::new()),
        claim_expires_at_ms: expires_at_ms,
        last_heartbeat_at_ms: None,
        pull_request: None,
        created_by: PeerId::new(),
        created_at_ms: 1,
        updated_at_ms: 1,
        reviews: None,
        submissions: Vec::new(),
        last_submission_rejection: None,
    }
}

/// 68 rooms (the M5's count), each with an unowned card and a teammate's live claim; her one
/// held card sits on room 5.
struct Fixture {
    _home: TempDir,
    rooms: Vec<Room>,
    me: PeerId,
    mine: WorkCardId,
    fetch: CountingFetch,
}

const LEASE_END_MS: u64 = 1_000_000;

fn fixture() -> Fixture {
    let home = TempDir::new().expect("tempdir");
    let rooms: Vec<Room> = (0..68)
        .map(|i| Room::from_name(home.path(), &format!("room-{i}")).expect("a room"))
        .collect();
    let me = PeerId::new();
    let teammate = PeerId::new();
    let held = card(Some(me), Some(LEASE_END_MS));
    let mine = held.card_id;
    let mut boards = HashMap::new();
    for (i, room) in rooms.iter().enumerate() {
        let mut cards = vec![card(None, None), card(Some(teammate), Some(LEASE_END_MS * 2))];
        if i == 5 {
            cards.push(held.clone());
        }
        boards.insert(room.channel, cards);
    }
    Fixture { _home: home, rooms, me, mine, fetch: CountingFetch::new(boards) }
}

// what this catches (card 522419e5, CB1): the per-tick rebuild. Every self-tick and claim
// renewal re-projected all ~68 boards (2.3 slow reads/s on the M5, p50 1.7 s). After the first
// sight, 100 held-work asks with nothing changed read NO board; one work event re-reads exactly
// that room, once, and the probe tallies say why each read happened.
#[tokio::test]
async fn a_hundred_self_ticks_read_no_board_and_one_change_reads_one_room() {
    let f = fixture();
    let index = BoardIndex::new(f.me);
    let now = 10;

    let owned = index.owned(&f.fetch, &f.rooms, now).await;
    assert_eq!(f.fetch.total(), 68, "startup reads each room once");
    assert_eq!(index.reads_for(ReadReason::FirstSight), 68);
    assert_eq!(owned.len(), 1);
    assert_eq!(owned[0].0.channel, f.rooms[5].channel);
    assert_eq!(owned[0].1.card_id, f.mine);

    for tick in 0..100 {
        let owned = index.owned(&f.fetch, &f.rooms, now + tick).await;
        assert_eq!(owned.len(), 1);
    }
    assert_eq!(f.fetch.total(), 68, "100 self-ticks with nothing changed read zero boards");

    index.room_changed(f.rooms[7].channel);
    let owned = index.owned(&f.fetch, &f.rooms, now + 200).await;
    assert_eq!(owned.len(), 1);
    assert_eq!(f.fetch.total(), 69, "one changed room is one read");
    assert_eq!(f.fetch.reads_of(f.rooms[7].channel), 2);
    assert_eq!(index.reads_for(ReadReason::RoomChanged), 1);
    index.owned(&f.fetch, &f.rooms, now + 300).await;
    assert_eq!(f.fetch.total(), 69, "and only once");
    assert_eq!(index.reads_total(), 69);
}

// what this catches: a missed heartbeat echo making a live claim read lapsed forever. A cached
// lease that crosses its expiry re-reads that room once (the board decides), then never again.
#[tokio::test]
async fn a_lease_crossing_its_expiry_rereads_its_room_once() {
    let f = fixture();
    let index = BoardIndex::new(f.me);
    index.owned(&f.fetch, &f.rooms, 10).await;
    assert_eq!(f.fetch.total(), 68);
    index.owned(&f.fetch, &f.rooms, LEASE_END_MS - 1).await;
    assert_eq!(f.fetch.total(), 68, "before any lease ends nothing is stale");
    index.owned(&f.fetch, &f.rooms, LEASE_END_MS).await;
    assert_eq!(f.fetch.total(), 69, "only room 5 carries a lease ending here");
    assert_eq!(f.fetch.reads_of(f.rooms[5].channel), 2);
    assert_eq!(index.reads_for(ReadReason::LeaseCrossed), 1);
    index.owned(&f.fetch, &f.rooms, LEASE_END_MS + 1).await;
    assert_eq!(f.fetch.total(), 69, "a re-read past the crossing is current again");
}

// what this catches: an unreadable room erasing her holds in the other rooms, or never being
// retried (the old walk's `?` once lapsed every hold she had over one bad room).
#[tokio::test]
async fn an_unreadable_room_keeps_her_other_holds_and_is_retried_alone() {
    let f = fixture();
    let index = BoardIndex::new(f.me);
    *f.fetch.refuse.lock().expect("refuse lock") = Some(f.rooms[9].channel);
    let owned = index.owned(&f.fetch, &f.rooms, 10).await;
    assert_eq!(f.fetch.total(), 68);
    assert_eq!(owned.len(), 1, "one unreadable room does not erase her hold elsewhere");
    *f.fetch.refuse.lock().expect("refuse lock") = None;
    index.owned(&f.fetch, &f.rooms, 20).await;
    assert_eq!(f.fetch.total(), 69, "the unreadable room stays stale and is retried, alone");
}

// what this catches: the tick, the renewal and the room-board faculty asking for one stale
// room at the same moment and each reading it (three reads where one answers all).
#[tokio::test]
async fn concurrent_asks_for_one_stale_room_share_one_read() {
    let f = fixture();
    let index = BoardIndex::new(f.me);
    let room = &f.rooms[3];
    let (a, b, c) = tokio::join!(
        index.board(&f.fetch, room, 10),
        index.board(&f.fetch, room, 10),
        index.board(&f.fetch, room, 10),
    );
    assert!(a.is_ok() && b.is_ok() && c.is_ok());
    assert_eq!(f.fetch.reads_of(room.channel), 1);
}

// what this catches (card ed2c9417, Kimi 2026-10-06): a claim she holds on a room she
// does not subscribe to, renewed by the followed path but invisible to the work gate,
// so she read no_held_work and made a new review card every turn. Only a record whose
// room she did NOT walk, and whose card the walk did not find, is read on its own board.
#[test]
fn a_followed_claim_outside_her_rooms_joins_the_owned_walk_once() {
    let home = TempDir::new().expect("tempdir");
    let room = |name: &str| airc_lib::Room::from_name(home.path(), name).expect("a room");
    let claim = |room_name: &str, card: u128| crate::persona::held_claims::HeldClaim {
        room: room(room_name),
        card_id: uuid::Uuid::from_u128(card),
        claim_id: uuid::Uuid::from_u128(card + 0x1000),
        recorded_at_ms: 1,
        refusals: 0,
    };
    let walked_rooms = vec![room("academy").channel.as_uuid()];
    let recorded = vec![
        claim("org-room", 1), // her review card's room, not walked: read it
        claim("academy", 2),      // a walked room: the walk already saw it
        claim("org-room", 3), // not walked, but the walk found the card: no second read
    ];
    let walked_cards = vec![uuid::Uuid::from_u128(3)];
    let beyond = followed_beyond_rooms(&walked_rooms, &walked_cards, &recorded);
    assert_eq!(beyond.len(), 1, "{beyond:?}");
    assert_eq!(beyond[0].card_id, uuid::Uuid::from_u128(1));
}

// what this catches: read-your-writes. Her own claim or state change must show on her next
// ask, before its echo returns on her stream (work/create read her held card stale right after
// a claim). A write to a card re-reads only the room whose board carries it.
#[tokio::test]
async fn her_write_to_a_card_rereads_only_its_room() {
    let f = fixture();
    let index = BoardIndex::new(f.me);
    index.owned(&f.fetch, &f.rooms, 10).await;
    index.card_written(f.mine);
    index.owned(&f.fetch, &f.rooms, 20).await;
    assert_eq!(f.fetch.total(), 69);
    assert_eq!(f.fetch.reads_of(f.rooms[5].channel), 2);
}
