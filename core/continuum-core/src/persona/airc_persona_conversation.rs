//! Production [`PersonaConversation`] impl wrapping
//! `Arc<dyn AircCitizen>` — slice 11 of #133, re-shaped in slice 13.5
//! around the [`AircCitizen`] trait.
//!
//! This is where the substrate's transport-agnostic loop
//! ([`super::service_loop::serve_persona_loop`]) meets the live airc
//! daemon. The conversation trait stays the loop's boundary; this
//! struct is the one place the substrate calls
//! [`AircCitizen::subscribe`] / [`AircCitizen::say`] /
//! [`AircTranscriptReader::page_recent`] directly. Holding
//! `Arc<dyn AircCitizen>` instead of the concrete runtime keeps the
//! production projection symmetric with whatever stub a future test
//! plugs in.
//!
//! ## Why slice 11 isn't in slice 10
//!
//! - **Testability**: slice 10's loop runs against a stub
//!   conversation; if its `next_message` / `say` / `high_water_mark`
//!   needed an airc daemon, the loop wouldn't be unit-testable. The
//!   PersonaConversation trait gives slice 10 a no-daemon contract;
//!   slice 11 fulfills that contract for production.
//! - **Cleanly bisectable**: when the substrate misbehaves later, we
//!   know whether the loop logic broke (slice 10's tests) or the
//!   airc transport broke (slice 11's smoke path).
//!
//! ## Non-text events
//!
//! `next_message` filters out events with no text body. Binary
//! attachments, control envelopes, and image messages don't reach
//! the service loop — the slice-10 contract is text-in / text-out
//! today. Vision + audio land in later slices via separate
//! conversation trait methods (per
//! [[ai-namespace-multimodal-crutches]] — multi-modal as first-class
//! peer, not a hack on top of the text path).
//!
//! ## Subscribe lifecycle
//!
//! The airc subscribe stream is lazy: created on the FIRST call to
//! `next_message`, not at construction. This keeps
//! [`AircPersonaConversation::new`] cheap + infallible — useful for
//! the slice-12 supervisor that constructs one of these per hosted
//! persona at boot, before any of them have necessarily attached to
//! their rooms yet.

use crate::persona::airc_citizen::AircCitizen;
use crate::persona::service_loop::{IncomingMessage, PersonaConversation};
use airc_core::TranscriptEvent;
use airc_lib::FilteredEventStream;
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

/// Wraps an [`AircCitizen`] and projects it onto the substrate's
/// [`PersonaConversation`] contract. Owns the airc subscribe stream
/// across calls so successive `next_message` invocations are a
/// continuation (not a fresh resubscription that would drop in-flight
/// events).
/// What a poll of the inbound stream produced.
enum Polled {
    Event(Option<Result<std::sync::Arc<TranscriptEvent>, airc_lib::LiveLag>>),
    Membership,
    Quiet,
}

/// How long a live stream may stay silent before the durable store is asked
/// whether the room moved on without us (see `next_message`).
const QUIET_WATCHDOG: std::time::Duration = std::time::Duration::from_secs(90);
/// How often the store-backed catch-up pages every subscribed room's tail.
const CATCH_UP_EVERY: std::time::Duration = std::time::Duration::from_secs(60);
/// Events paged per room per catch-up tick.
const CATCH_UP_PAGE: usize = 32;
/// Event ids remembered per room to tell "seen" from "dropped" (bounded).
const SEEN_RING: usize = 128;

/// Per room: the WALL-TIME floor adopted at first sight (nothing older is ever
/// replayed) and a bounded ring of EVENT IDS actually forwarded (live or paged).
/// Neither lamport nor "max seen" can stand in for this: a lamport is the
/// publisher's logical clock, so a quiet human's line carries a smaller lamport
/// than the busy citizens' receipts around it and read as "old" (2026-09-04:
/// zero admissions across three builds); and live `event` frames keep arriving
/// after the daemon drops a `message`, so a max-seen mark leaps past the line.
#[derive(Default)]
struct SeenRooms {
    /// Per-room ring of (peer, text) fingerprints — the same line under two ids.
    texts: std::collections::HashMap<Uuid, std::collections::VecDeque<u64>>,
    floor_ms: std::collections::HashMap<Uuid, u64>,
    seen: std::collections::HashMap<Uuid, std::collections::VecDeque<Uuid>>,
}

impl SeenRooms {
    fn fingerprint(peer: Uuid, text: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        peer.hash(&mut h);
        text.hash(&mut h);
        h.finish()
    }

    fn note_text(&mut self, room: Uuid, fp: u64) {
        let ring = self.texts.entry(room).or_default();
        if ring.contains(&fp) {
            return;
        }
        if ring.len() == SEEN_RING {
            ring.pop_front();
        }
        ring.push_back(fp);
    }

    fn was_seen_text(&self, room: Uuid, fp: u64) -> bool {
        self.texts.get(&room).is_some_and(|r| r.contains(&fp))
    }

    fn note(&mut self, room: Uuid, event_id: Uuid) {
        let ring = self.seen.entry(room).or_default();
        if !ring.contains(&event_id) {
            ring.push_back(event_id);
            while ring.len() > SEEN_RING {
                ring.pop_front();
            }
        }
    }
    fn was_seen(&self, room: Uuid, event_id: Uuid) -> bool {
        self.seen.get(&room).is_some_and(|r| r.contains(&event_id))
    }
}

/// Page every subscribed room's durable tail and forward, into the inbox, the
/// events the live stream never delivered (above the room's floor, not in the
/// seen ring). Returns `(rooms paged, forwarded)`; `usize::MAX` forwarded means
/// the inbox is gone. Heartbeats and chunks are skipped here as at the door.
/// A forwarded line from outside the citizenry (or one naming nobody in
/// particular — the turn decides mentions) raises the citizen's directed-pending
/// flag so a parked self-work lane wait yields to it.
fn signal_if_directed(own: uuid::Uuid, event: &airc_core::TranscriptEvent) {
    let peer = event.peer_id.as_uuid();
    if peer == own {
        return;
    }
    // Only a line that would ADMIT as a turn may raise the flag: the board's
    // System events ride the same stream from the same non-citizen peers and
    // are filtered at the door — on 8c6733c65 they kept the flag up for the
    // whole roster (25 turns, 25 yields, 0 lanes acquired).
    let Ok(message) = perceptual_from_event(event) else {
        return;
    };
    // The SAME priority the loop head admits with (`TurnAttention`): a
    // human line or a line naming her. On 9e8320b60 this site still raised the
    // flag for every non-citizen line, so a holder parked in a lane wait yielded
    // to an agent status line that the loop head then declined — 13 work turns
    // abandoned in 9 ms in half an hour (`delib.gate.yielded_to_directed`).
    let registry = crate::persona::PersonaAircRuntimeRegistry::try_global();
    let mentioned = registry
        .as_ref()
        .and_then(|r| r.get(own))
        .map(|rt| {
            crate::persona::persona_identity::PersonaIdentity::new(own, rt.agent_name().to_string())
        })
        .is_some_and(|me| me.mentions(&message.text));
    let sender_is_human = crate::ipc::positron_presence::is_human_peer(peer);
    if crate::cognition::workspace::TurnAttention::for_message(mentioned, sender_is_human)
        .requires_priority()
    {
        crate::cognition::directed_pending::signal(own);
    }
}

/// A durable chat row as the transcript event the inbound seam admits: kind
/// Message, a text body, wall time, the row id as the event id. Lamport is 0 —
/// the loop head judges staleness by event id, never by this clock.
fn event_from_row(room: Uuid, row: crate::persona::durable_history::RoomRow) -> TranscriptEvent {
    use airc_core::{
        Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptKind,
    };
    let room_id = RoomId::from_uuid(room);
    TranscriptEvent {
        event_id: EventId::from_uuid(row.id),
        room_id,
        peer_id: PeerId::from_uuid(row.sender),
        client_id: ClientId::new(),
        kind: TranscriptKind::Message,
        occurred_at_ms: row.occurred_at_ms,
        lamport: 0,
        target: MentionTarget::Room(room_id),
        headers: Headers::default(),
        body: Some(Body::text(&row.text)),
        attachment: None,
        receipt: None,
        metadata: serde_json::Value::Null,
    }
}

async fn catch_up_from_store(
    runtime: &dyn AircCitizen,
    seen: &std::sync::Mutex<SeenRooms>,
    tx: &tokio::sync::mpsc::Sender<Result<std::sync::Arc<TranscriptEvent>, airc_lib::LiveLag>>,
) -> (usize, usize) {
    let rooms = runtime.subscribed_rooms().await.unwrap_or_default(); // unwrap_or: an unreadable room list = nothing to catch up this tick
    let mut forwarded = 0usize;
    let mut paged = 0usize;
    let mut failed = 0usize;
    let mut first_error: Option<String> = None;
    let mut sample_newest = 0u64;
    let mut sample: Option<(Uuid, u64, u64, usize, String, String, bool)> = None;
    let short = |u: &Uuid| u.to_string().chars().take(8).collect::<String>();
    let room_list = rooms.iter().map(short).collect::<Vec<_>>().join(",");
    let mut empty: Vec<String> = Vec::new();
    for room in rooms {
        // THE CORE'S OWN CHAT STORE is the durable page (see
        // `durable_history::room_rows`): the daemon's ring page was empty for
        // the busiest room on every citizen (2026-09-04, `catch_up_rooms`).
        let mut events = match crate::persona::durable_history::room_rows(room, CATCH_UP_PAGE).await
        {
            Ok(rows) => rows
                .into_iter()
                .map(|r| event_from_row(room, r))
                .collect::<Vec<_>>(),
            Err(error) => {
                // Observable, not silent: on the first build 7 of ~68 rooms paged and
                // the rest failed unseen, so eleven citizens never saw the operator.
                failed += 1;
                if first_error.is_none() {
                    first_error = Some(format!("{room}: {error}"));
                }
                continue;
            }
        };
        paged += 1;
        events.sort_by_key(|e| e.occurred_at_ms);
        let Some(newest_ms) = events.iter().map(|e| e.occurred_at_ms).max() else {
            empty.push(short(&room));
            continue;
        };
        let floor_ms = *seen
            .lock()
            .unwrap_or_else(|e| e.into_inner()) // poisoned lock = read the last state, same policy as every lock in this crate
            .floor_ms
            .entry(room)
            .or_insert(newest_ms);
        // WHAT SHE SEES IN THE STORE (repeatability): the newest paged line per
        // tick for the room with the freshest tail — floor, newest, and the
        // line's identity — so "nothing admitted" is explainable from probes
        // alone (zero admissions across four builds were not).
        if newest_ms > sample_newest {
            sample_newest = newest_ms;
            let newest = events.iter().max_by_key(|e| e.occurred_at_ms);
            sample = newest.map(|e| {
                (
                    room,
                    floor_ms,
                    newest_ms,
                    events.len(),
                    e.peer_id
                        .as_uuid()
                        .to_string()
                        .chars()
                        .take(8)
                        .collect::<String>(),
                    e.body
                        .as_ref()
                        .and_then(|b| b.as_text().map(|t| t.chars().take(60).collect::<String>()))
                        .unwrap_or_else(|| "<non-text>".to_string()), // unwrap_or: a non-text body renders as a marker, never dropped
                    seen.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .was_seen(room, e.event_id.as_uuid()), // poisoned lock = read the last state, same policy as every lock in this crate
                )
            });
        }
        for event in events {
            if event.occurred_at_ms <= floor_ms
                || seen.lock().unwrap_or_else(|e| e.into_inner()).was_seen(room, event.event_id.as_uuid())  // poisoned lock = read the last state, same policy as every lock in this crate
                || crate::persona::airc_citizen::is_heartbeat(&event)
                || crate::airc::realtime_wire::is_stream_chunk(&event)
            {
                continue;
            }
            // A line the operator's `chat/send` persisted under its own message id
            // arrives live under the say's EVENT id: same peer, same text, two ids.
            // The fingerprint ring keeps it one turn.
            let fingerprint = crate::airc::realtime_wire::room_turn_from_event(&event)
                .ok()
                .map(|(peer, text)| SeenRooms::fingerprint(peer, &text));
            {
                let mut s = seen.lock().unwrap_or_else(|e| e.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
                if let Some(fp) = fingerprint {
                    if s.was_seen_text(room, fp) {
                        continue;
                    }
                    s.note_text(room, fp);
                }
                s.note(room, event.event_id.as_uuid());
            }
            signal_if_directed(runtime.peer_id(), &event);
            if tx.send(Ok(std::sync::Arc::new(event))).await.is_err() {
                return (paged, usize::MAX);
            }
            forwarded += 1;
        }
    }
    if let Some((room, floor_ms, newest_ms, count, peer, head, was_seen)) = sample {
        crate::probe!(
            class = "persona.inbound.catch_up_page_sample",
            room = %room,
            floor_ms = floor_ms,
            newest_ms = newest_ms,
            count = count as u64,
            newest_peer = %peer,
            newest_head = %head,
            newest_was_seen = was_seen,
            forwarded = forwarded as u64,
            "store catch-up: the freshest paged room this tick"
        );
    }
    // WHICH ROOMS SHE PAGES, and which come back empty — the run room missing
    // from this list (or listed and empty) is the difference between "the store
    // catch-up works" and "she cannot hear the room she is working in".
    crate::probe!(
        class = "persona.inbound.catch_up_rooms",
        rooms = %room_list,
        empty = %empty.join(","),
        paged = paged as u64,
        "the rooms this tick paged, and the ones whose page held no conversation"
    );
    if failed > 0 {
        crate::probe!(
            class = "persona.inbound.catch_up_page_failed",
            failed = failed as u64,
            paged = paged as u64,
            first_error = %first_error.unwrap_or_default(), // unwrap_or: failed>0 guarantees one was recorded
            "store catch-up: some subscribed rooms could not be paged"
        );
    }
    (paged, forwarded)
}

pub struct AircPersonaConversation {
    runtime: Arc<dyn AircCitizen>,
    /// The subscription snapshot owned by this conversation. `None` is
    /// unprimed; an empty set is primed but parked until the next membership
    /// epoch. Neither state may implicitly join a default room.
    rooms: Option<Vec<Uuid>>,
    /// A consumed epoch whose async reattach has not finished. `next_message`
    /// is cancellation-safe: a competing service-loop wake cannot lose it.
    refresh_pending: bool,
    /// The persona's own peer_id, captured at construction. Used by
    /// `next_message` to skip self-loop echoes WITHIN the projection
    /// — the service loop ALSO skips by persona's instance peer_id;
    /// the redundancy lets the conversation be honest about whose
    /// stream it's projecting (defense in depth, costs nothing).
    own_peer_id: uuid::Uuid,
    /// Lazy-initialized subscribe stream. `None` before the first
    /// `next_message`; `Some` once the daemon attach succeeds. Per-
    /// citizen stream — never shared across personas.
    ///
    /// Transient transport loss (daemon restart, socket drop) is healed
    /// ONE layer down, inside [`airc_lib`]'s `subscribe()`: its drain
    /// task re-attaches with a resume cursor + capped backoff and
    /// replays the gap's durable events (see `airc-lib/src/daemon.rs`).
    /// So this stream stays live across daemon bounces and the persona
    /// never goes deaf — `next_message` only ever sees a terminal `None`
    /// when airc-lib DELIBERATELY ends the subscription (a decode /
    /// wire-schema fault it surfaces loud, card 807193ab), which we
    /// re-surface rather than mask.
    /// The in-process inbox the PUMP fills — never the daemon stream itself.
    /// The loop polls this only between turns; the pump drains the daemon
    /// continuously, so the daemon never sees a slow subscriber (2026-09-04:
    /// every boot, 12 citizens heard for ~2 min, then the first turns started,
    /// nobody polled for minutes, the daemon's queue filled, the subscriber was
    /// marked lagged and never recovered — the live plane looked dead).
    inbox: Option<
        tokio::sync::mpsc::Receiver<Result<std::sync::Arc<TranscriptEvent>, airc_lib::LiveLag>>,
    >,
    /// The drain task behind `inbox`; aborted and replaced on every re-open.
    pump: Option<tokio::task::JoinHandle<()>>,
    /// Per-room floors and seen rings, SHARED across pump restarts. A pump
    /// re-open (membership change) must not re-adopt floors: on the previous
    /// build every re-open reset them to "now", so a line already in the page
    /// read as old forever (zero admissions across four builds).
    seen: std::sync::Arc<std::sync::Mutex<SeenRooms>>,
    /// Membership-change cue (P0 20b44763): when the citizen joins a room at
    /// RUNTIME (benchmark dispatch moving her into a fresh run room), this
    /// epoch moves and `next_message` re-opens the stream so the new room's
    /// events reach her. Without it the stream keeps the spawn-time channel
    /// snapshot forever and every later-joined room is born deaf — measured
    /// live 2026-08-15: three bench rounds, 12 addressed kickoffs each, zero
    /// turns. NOT the forbidden auto-resubscribe fallback documented on the
    /// terminal-`None` arm below: that masks a wire fault; this answers a
    /// REAL membership event, the system-law event-driven shape.
    membership_epoch: tokio::sync::watch::Receiver<u64>,
    /// Highest lamport this conversation has SEEN on its raw stream (or replayed).
    /// The rejoin replay's dedupe watermark: only events strictly newer are ever
    /// replayed, so a reopen can never re-feed history as fresh perception.
    last_lamport: u64,
    /// Per room: the lamport FLOOR adopted at first sight (nothing older is ever
    /// replayed) and the ring of lamports actually SEEN since. A newest-seen mark
    /// cannot stand in for this: live `event` frames keep arriving after the
    /// daemon drops a `message`, so "max seen" leaps past the dropped line and the
    /// catch-up reads it as old (measured 2026-09-04: zero admissions).
    /// The catch-up tick's deadline. PERSISTENT across `next_message` calls:
    /// the loop's wake `select!` drops and re-creates the `next_message` future
    /// on every self-tick (3–10 s), so a `sleep(60 s)` inside it restarted
    /// forever and neither the watchdog nor the catch-up ever fired (measured
    /// 2026-09-04: zero ticks in 7 minutes). A deadline held on `self` survives.
    next_catch_up: tokio::time::Instant,
    /// Room-turns recovered by the rejoin replay, yielded ahead of the live
    /// stream. See the epoch-reopen branch in `next_message`.
    rejoin_backlog: std::collections::VecDeque<IncomingMessage>,
}

impl AircPersonaConversation {
    /// Construct without contacting the daemon. The subscribe stream
    /// is built on first `next_message`; until then this is free.
    pub fn new(runtime: Arc<dyn AircCitizen>) -> Self {
        let own_peer_id = runtime.peer_id();
        let membership_epoch = runtime.membership_epoch();
        Self {
            runtime,
            rooms: None,
            refresh_pending: false,
            own_peer_id,
            inbox: None,
            pump: None,
            seen: std::sync::Arc::new(std::sync::Mutex::new(SeenRooms::default())),
            membership_epoch,
            last_lamport: 0,
            next_catch_up: tokio::time::Instant::now() + CATCH_UP_EVERY,
            rejoin_backlog: std::collections::VecDeque::new(),
        }
    }

    /// Borrow the underlying citizen — useful for the supervisor's
    /// registry-eviction path (slice 12) where the supervisor needs
    /// to look up the citizen back from the conversation for graceful
    /// shutdown.
    pub fn runtime(&self) -> &Arc<dyn AircCitizen> {
        &self.runtime
    }

    fn stop_stream(&mut self) {
        if let Some(old) = self.pump.take() {
            old.abort();
        }
        self.inbox = None;
    }

    /// Rebind the existing perception owner to its durable room set. The SDK's
    /// empty-set subscribe falls back to general, so an empty membership parks
    /// here instead. The existing epoch is also how this state resumes.
    async fn refresh_membership(&mut self) -> Result<bool, String> {
        let rooms = self
            .runtime
            .subscribed_rooms()
            .await
            .map_err(|e| format!("subscription set read failed: {e}"))?;
        let active = !rooms.is_empty();
        if active {
            let stream = self
                .runtime
                .subscribe_all_rooms()
                .await
                .map_err(|e| format!("subscribe failed: {e}"))?;
            self.install_stream(stream);
        } else {
            self.stop_stream();
            crate::probe!(
                class = "persona.inbound.no_rooms",
                persona = %self.own_peer_id,
                "no subscribed rooms — perception waits for a membership change"
            );
        }
        self.rejoin_backlog
            .retain(|message| rooms.contains(&message.room_id));
        self.rooms = Some(rooms);
        Ok(active)
    }

    /// Page the actual subscription snapshot, never a lazy default-room read.
    /// This also keeps a no-focus subscription from replaying another room.
    async fn page_subscribed_rooms(&self, limit: usize) -> Result<Vec<TranscriptEvent>, String> {
        let rooms = self
            .rooms
            .as_ref()
            .ok_or_else(|| "conversation history requested before prime()".to_string())?;
        let mut events = Vec::new();
        for room in rooms {
            events.extend(
                self.runtime
                    .page_recent_in(Some(airc_core::RoomId::from_uuid(*room)), limit)
                    .await
                    .map_err(|e| format!("page room {room} failed: {e}"))?,
            );
        }
        Ok(events)
    }

    /// Install a fresh daemon stream behind the PUMP: a task that drains it
    /// continuously into a bounded in-process inbox. Backpressure, if any, lands
    /// here (in-process, where a turn in progress is the only slow consumer) and
    /// never at the daemon, whose lag policy drops live pushes for good.
    fn install_stream(&mut self, mut stream: FilteredEventStream) {
        self.stop_stream();
        let (tx, rx) = tokio::sync::mpsc::channel(INBOX_CAPACITY);
        let persona = self.own_peer_id;
        let runtime = Arc::clone(&self.runtime);
        let seen_shared = std::sync::Arc::clone(&self.seen);
        self.pump = Some(tokio::spawn(async move {
            use futures::StreamExt as _;
            // The pump is PERCEPTION's own task: it forwards live frames as they
            // arrive AND, on its own clock, pages every subscribed room's durable
            // tail and forwards what the live stream dropped. The deliberation loop
            // (minutes inside a turn) cannot starve either — measured 2026-09-04: a
            // catch-up that lived inside the loop's select ticked twice in seven
            // minutes across twelve citizens.
            let seen = std::sync::Arc::clone(&seen_shared);
            let mut next_catch_up = tokio::time::Instant::now() + CATCH_UP_EVERY;
            // Live frames delivered since the last catch-up tick. A tick that admits
            // events from the store while this is still zero means the live stream
            // is silently dead: the daemon dropped the subscription (a restart)
            // and airc-lib reconnected the socket to nothing. Measured 2026-09-07
            // 03:11–03:17Z on the M5: after `airc update` restarted the daemon,
            // raw_event went to zero for 4 of 5 citizens with no stream end, no
            // resubscribe, and only the 60 s store path delivering (card e592b633).
            let mut live_since_tick: u64 = 0;
            let mut first_tick = true;
            let mut reopen_attempt: u32 = 0;
            loop {
                tokio::select! {
                    item = stream.next() => match item {
                        Some(item) => {
                            live_since_tick = live_since_tick.saturating_add(1);
                            reopen_attempt = 0;
                            if let Ok(ev) = &item {
                                let mut s = seen.lock().unwrap_or_else(|e| e.into_inner());  // poisoned lock = read the last state, same policy as every lock in this crate
                                s.note(ev.room_id.as_uuid(), ev.event_id.as_uuid());
                                if let Ok((peer, text)) =
                                    crate::airc::realtime_wire::room_turn_from_event(ev)
                                {
                                    s.note_text(ev.room_id.as_uuid(), SeenRooms::fingerprint(peer, &text));
                                }
                                drop(s);
                                if !crate::persona::airc_citizen::is_heartbeat(ev)
                                    && !crate::airc::realtime_wire::is_stream_chunk(ev)
                                {
                                    signal_if_directed(persona, ev);
                                }
                            }
                            if tx.send(item).await.is_err() {
                                return; // the conversation dropped its inbox — the pump is done
                            }
                        }
                        None => {
                            crate::probe!(
                                class = "persona.inbound.pump_ended",
                                persona = %persona,
                                "the daemon stream ended under the pump — re-opening it"
                            );
                            match reopen_live_stream(&*runtime, persona, "ended", &mut reopen_attempt).await {
                                Some(next) => {
                                    stream = next;
                                    // The next tick admits the backlog the citizen missed
                                    // while the stream was down — that is not a dead stream.
                                    first_tick = true;
                                    live_since_tick = 0;
                                }
                                None => return, // the pump gave up (probe pump_gave_up said so)
                            }
                        }
                    },
                    _ = tokio::time::sleep_until(next_catch_up) => {
                        next_catch_up = tokio::time::Instant::now() + CATCH_UP_EVERY;
                        let (paged, admitted) = catch_up_from_store(&*runtime, &seen, &tx).await;
                        crate::probe!(
                            class = "persona.inbound.catch_up_tick",
                            persona = %persona,
                            rooms_paged = paged as u64,
                            admitted = admitted as u64,
                            "store-backed catch-up tick (the pump's own clock)"
                        );
                        if admitted == usize::MAX {
                            return; // inbox gone
                        }
                        if live_stream_missed(live_since_tick, admitted, first_tick) {
                            crate::probe!(
                                class = "persona.inbound.stream_dead",
                                persona = %persona,
                                admitted = admitted as u64,
                                "the store admitted events the live stream never delivered — re-opening it"
                            );
                            // No gap replay here, unlike the membership-change path: the
                            // catch-up tick plus `seen` dedup already delivered the dead
                            // window — a replay would double-deliver.
                            match reopen_live_stream(&*runtime, persona, "missed_events", &mut reopen_attempt).await {
                                Some(next) => {
                                    stream = next;
                                    first_tick = true;
                                    live_since_tick = 0;
                                    continue;
                                }
                                None => return,
                            }
                        }
                        first_tick = false;
                        live_since_tick = 0;
                    }
                }
            }
        }));
        self.inbox = Some(rx);
    }
}

/// In-process inbox depth between the pump and the loop. A turn in progress
/// can leave the loop away for minutes; 8k events is hours of a busy room.
const INBOX_CAPACITY: usize = 8_192;

/// The live stream is dead when a catch-up tick admits events from the store
/// while no live frame arrived since the previous tick. The first tick after
/// (re)open is exempt: it legitimately admits the backlog.
fn live_stream_missed(live_since_tick: u64, admitted: usize, first_tick: bool) -> bool {
    !first_tick && live_since_tick == 0 && admitted > 0 && admitted != usize::MAX
}

/// Re-subscribe with a bounded backoff (1 s doubling to 30 s). `None` only
/// when the citizen's subscribe is refused for good.
async fn reopen_live_stream(
    runtime: &dyn AircCitizen,
    persona: uuid::Uuid,
    reason: &'static str,
    attempt: &mut u32,
) -> Option<FilteredEventStream> {
    loop {
        // Back off BETWEEN retries, never before the first attempt: a stream that
        // ended on a room join or a daemon restart is back in one hop.
        if *attempt > 0 {
            let delay = std::time::Duration::from_secs(1u64 << (*attempt).min(5))
                .min(std::time::Duration::from_secs(30));
            tokio::time::sleep(delay).await;
        }
        match runtime.subscribe_all_rooms().await {
            Ok(next) => {
                crate::probe!(
                    class = "persona.inbound.stream_reopened",
                    persona = %persona,
                    reason = reason,
                    attempt = *attempt,
                    "live stream re-opened by the pump"
                );
                *attempt = 0;
                return Some(next);
            }
            Err(e) => {
                *attempt = attempt.saturating_add(1);
                crate::probe!(
                    class = "persona.inbound.stream_reopen_failed",
                    persona = %persona,
                    attempt = *attempt,
                    error = %e,
                    "live stream re-open failed; retrying"
                );
                if *attempt >= 8 {
                    // The one branch that leaves a citizen without a live path for
                    // good — the condition card e592b633 was filed about. Say it
                    // in its own class, not buried in the retry rows.
                    crate::probe!(
                        class = "persona.inbound.pump_gave_up",
                        persona = %persona,
                        attempts = *attempt,
                        "live stream could not be re-opened after repeated refusals; the store catch-up is her only path now"
                    );
                    return None;
                }
            }
        }
    }
}

#[async_trait]
impl PersonaConversation for AircPersonaConversation {
    /// Eagerly opens the airc subscribe stream. Idempotent — calling
    /// twice is a no-op after the first.
    ///
    /// Replaces the slice-11 lazy-on-first-next_message subscribe.
    /// `serve_persona_loop` calls this once at boot so the daemon
    /// round-trip lands at startup instead of on the first cognition
    /// turn. The lazy branch in `next_message` stays as a fallback
    /// for callers that don't call `prime` first (e.g., direct
    /// integration tests). Per [[no-fallbacks-ever]] the fallback
    /// has identical semantics — it's not a degraded path, it's a
    /// later-binding path.
    async fn prime(&mut self) -> Result<(), String> {
        if self.rooms.is_some() {
            return Ok(());
        }
        if !self.refresh_membership().await? {
            return Ok(());
        }
        // #146 diagnostic: confirm the CHAT subscribe stream actually opened for
        // this persona. Post-reboot the personas were room-deaf (0 perceptual
        // decodes) while the core-positron raw-attach path received fine — this
        // pins whether prime() even ran per persona.
        crate::probe!(
            class = "persona.inbound.subscribe_opened",
            persona = %self.own_peer_id,
            "persona chat subscribe stream opened (#146)"
        );
        // Seed the rejoin-replay watermark at the CURRENT transcript head, so the
        // first runtime room-join can never replay pre-subscribe history as fresh
        // perception (the #131 "room starts at join" rule, preserved under replay).
        self.last_lamport = self.high_water_mark(64).await.unwrap_or(0); // unwrap_or: an unreadable watermark = 0 (never read), the documented floor
        Ok(())
    }

    async fn high_water_mark(&self, limit: usize) -> Result<u64, String> {
        let events = self.page_subscribed_rooms(limit).await?;
        Ok(events.iter().map(|e| e.lamport).max().unwrap_or(0)) // unwrap_or: an empty page has no lamport; 0 = never read
    }

    async fn next_message(&mut self) -> Result<Option<IncomingMessage>, String> {
        // Per [[no-fallbacks-ever]]: prime() is the substrate's
        // single contract for opening the subscribe stream. If a
        // caller reaches next_message without having primed, the
        // substrate refuses visibly — never silently lazy-subscribes.
        // Reviewer-driven fix to PR #1514: the lazy fallback that
        // used to live here was dead code in production (every caller
        // goes through serve_persona_loop, which primes at boot) AND
        // a doctrine violation (soft-language "for future callers"
        // is exactly the silent-degradation shape we refuse).
        // Per [[no-fallbacks-ever]]: prime() is the substrate's single
        // contract for opening the subscribe stream. If a caller reaches
        // next_message without having primed, the substrate refuses
        // visibly — never silently lazy-subscribes (PR #1514 reviewer
        // fix: the soft-language lazy fallback was a silent-degradation
        // shape we refuse).
        if self.rooms.is_none() {
            return Err(
                "AircPersonaConversation::next_message called before prime() — caller must \
                 invoke prime() before iterating (serve_persona_loop does this automatically \
                 at boot)"
                    .to_string(),
            );
        }

        // Skip self / non-text inline — they're not "next messages"
        // from the loop's perspective. Yielding them with the loop
        // having to re-filter would mean the loop's outcome counter
        // over-counts skips for events the conversation already
        // knows aren't relevant.
        loop {
            // Rejoin-replayed turns first — they are OLDER than anything the live
            // stream will yield, and ordering is what keeps an addressed kickoff
            // ahead of the chatter that follows it.
            if !self.refresh_pending && !self.membership_epoch.has_changed().unwrap_or(false) {
                if let Some(replayed) = self.rejoin_backlog.pop_front() {
                    return Ok(Some(replayed));
                }
            }
            // Wait on EITHER the next event OR a membership-epoch move. The
            // epoch branch is the P0 20b44763 fix: a room joined at runtime
            // (benchmark dispatch) never enters the existing stream's channel
            // snapshot, so on a membership change we re-open the stream with
            // the enlarged set. A closed epoch channel (stub citizens whose
            // default `membership_epoch` drops its sender) parks on
            // `pending()` — closed means "membership never changes", never
            // an event, or stubs would busy-loop on `changed() == Err`.
            let polled = if self.refresh_pending {
                Polled::Membership
            } else {
                let active = self.inbox.is_some();
                let inbox = &mut self.inbox;
                let epoch = &mut self.membership_epoch;
                tokio::select! {
                    biased;
                    _ = async {
                        if epoch.changed().await.is_err() {
                            std::future::pending::<()>().await;
                        }
                    } => Polled::Membership,
                    ev = async {
                        match inbox.as_mut() {
                            Some(inbox) => inbox.recv().await,
                            None => std::future::pending().await,
                        }
                    } => Polled::Event(ev),
                    // QUIET WATCHDOG (2026-09-04): live streams went silent ~2 min after
                    // prime — 12 citizens received 172 events in the first two minutes,
                    // 2 citizens 5 events in the next two, and an operator line at +12 min
                    // reached nobody — with no lag error ever surfacing here. The daemon
                    // marks a slow subscriber lagged and its "resume from the sink" never
                    // delivers. So: when nothing has arrived for a window, ask the durable
                    // store whether the room moved on; if it did, the stream is dead —
                    // re-open it and replay the gap, exactly as a membership change does.
                    _ = tokio::time::sleep_until(self.next_catch_up), if active => Polled::Quiet,
                }
            };
            let reason: &'static str = match polled {
                Polled::Event(ev) => match ev {
                    None => {
                        crate::probe!(
                        class = "persona.inbound.stream_ended",
                                    persona = %self.own_peer_id,
                                    "airc subscribe stream ended unrecoverably — airc-lib dropped \
                                     the subscription (likely wire-schema drift between continuum \
                                     and airc builds, or the EventStream was released). NOT \
                                     auto-resubscribing: airc-lib owns transient reconnection, so \
                                     a terminal end here is a real fault to fix, not a hiccup to heal."
                                );
                        return Ok(None);
                    }
                    Some(Err(lag)) => {
                        return Err(format!("live stream lag: {lag}"));
                    }
                    Some(Ok(event)) => {
                        if let Some(msg) = self.admit_event(event).await {
                            return Ok(Some(msg));
                        }
                        continue;
                    }
                },
                Polled::Membership => {
                    self.refresh_pending = true;
                    "room membership changed at runtime — refreshing the subscription snapshot"
                }
                Polled::Quiet => {
                    // The pump owns the store catch-up on its own clock; the loop's
                    // deadline is only a wake so a long idle never blocks the backlog.
                    // No forced re-open here: a re-open resets nothing now, but it is
                    // also not a remedy — the store catch-up is.
                    self.next_catch_up = tokio::time::Instant::now() + CATCH_UP_EVERY;
                    continue;
                }
            };
            {
                if !self.refresh_membership().await? {
                    self.refresh_pending = false;
                    continue;
                }
                crate::probe!(
                    class = "persona.inbound.resubscribed",
                    persona = %self.own_peer_id,
                    reason = reason,
                    "re-opening the subscribe stream"
                );
                // REPLAY THE GAP (2026-08-21, the FOURTH deaf-kickoff variant). The
                // reopened stream is live-tail: anything published between the
                // membership change and this reopen was delivered to nobody — and
                // the benchmark kickoff is published milliseconds after join_room,
                // so it lost this race BY CONSTRUCTION on every dispatch (event
                // durably in the room, `kickoffs: 1`, zero raw_event rows). The
                // reopen pages the recent transcript and queues every room turn
                // strictly newer than the watermark; the same decode + self-skip
                // rules as the live path apply, and the work-event bridge dedups
                // by event id, so a replayed card event cannot double-fire.
                match self.page_subscribed_rooms(32).await {
                    Ok(events) => {
                        let scanned = events.len();
                        let mut replayed = 0usize;
                        let mut events = events;
                        events.sort_by_key(|e| e.lamport);
                        for event in &events {
                            if event.lamport <= self.last_lamport
                                || crate::airc::realtime_wire::is_stream_chunk(event)
                            {
                                continue;
                            }
                            crate::modules::work::bridge_wire_work_event(event).await;
                            if let Ok(message) = perceptual_from_event(event) {
                                if message.peer_id != self.own_peer_id {
                                    replayed += 1;
                                    self.rejoin_backlog.push_back(message);
                                }
                            }
                            // Commit only after the async bridge and queue write;
                            // cancellation must not skip an unqueued receipt.
                            self.last_lamport = event.lamport;
                        }
                        crate::probe!(
                            class = "persona.inbound.rejoin_replayed",
                            persona = %self.own_peer_id,
                            scanned,
                            replayed,
                            watermark = self.last_lamport,
                            "membership refresh paged subscribed rooms and queued turns \
                             admitted by the existing replay watermark"
                        );
                    }
                    Err(e) => crate::probe!(
                    class = "persona.inbound.rejoin_page_failed",
                                persona = %self.own_peer_id,
                                error = %e,
                                "rejoin replay page failed — events published between join \
                                 and reopen stay unheard until something else surfaces them"
                            ),
                }
                self.refresh_pending = false;
                continue;
            };
        }
    }

    async fn say_in(&self, room_id: Uuid, text: &str) -> Result<(), String> {
        self.runtime
            .say_in(room_id, text)
            .await
            .map(|_event_id| ())
            .map_err(|e| format!("say failed: {e}"))
    }

    /// #170: the airc citizen behind this conversation streams — hand the
    /// forwarder our runtime handle so it can publish ephemeral token chunks.
    fn stream_citizen(
        &self,
    ) -> Option<std::sync::Arc<dyn crate::persona::airc_citizen::AircCitizen>> {
        Some(self.runtime.clone())
    }
}

/// Project a live airc [`TranscriptEvent`] onto the substrate's
/// [`IncomingMessage`] contract — the ONE place that decides what counts
/// as a perceptual room turn for a persona.
///
/// Two on-wire shapes carry a room message:
///
/// 1. **Plain text** — a peer persona's [`AircCitizen::say`] emits a
///    `Body::Text`. Attribution is the transport `peer_id`.
/// 2. **`chat_transcript` envelope** — `chat/send` (a human, the web
///    client, any non-`say` caller) publishes the continuum realtime
///    envelope as `Body::Json`. `body.as_text()` is `None`, so a naive
///    text-only filter drops it — which made human chat structurally
///    invisible to locally-hosted personas (glass-box confirmed: the
///    event reached the subscribe stream with `as_text() == None`). Here
///    we decode the envelope and recover both the text and the TRUE
///    logical sender (`inline.senderId`), not the core's transport peer
///    that relayed the publish.
///
/// Everything else — presence, event-bridge, media-control, binary — is
/// not a room turn and yields a NAMED skip reason (the caller logs it —
/// never a bare drop; task #177 was diagnosed blind because the old
/// `Option` collapsed "no body-hint header", "envelope decode ERROR", and
/// "legit non-chat schema" into one silent `None`). This is the receive
/// half of the send/receive asymmetry noted in task #8 (converge
/// broadcast == RAG context): the sender keeps the rich `chat_transcript`
/// envelope for the web/replay/durable consumers; the persona learns to
/// read it rather than forcing a lossy plain-text downgrade on the send
/// side.
fn perceptual_from_event(event: &TranscriptEvent) -> Result<IncomingMessage, &'static str> {
    // Both on-wire room-turn shapes (say() text + chat_transcript envelope) and
    // all three named skip reasons live in the ONE decoder `room_turn_from_event`
    // (realtime_wire) — shared with the digest element and the positron
    // projection. This wrapper only adds the transcript's lamport.
    let (peer_id, text) = crate::airc::realtime_wire::room_turn_from_event(event)?;
    Ok(IncomingMessage {
        event_id: event.event_id.as_uuid(),
        lamport: event.lamport,
        peer_id,
        text,
        // The transport room is the turn's context (A.6) — without it the
        // service loop bound operator/CLI turns to a nil room and every
        // room-scoped source abstained.
        room_id: event.room_id.as_uuid(),
    })
}

#[cfg(test)]
mod tests {
    /// what this catches: e0d64552 — the actual membership commands persisted
    /// their changes but bypassed the epoch that refreshes a living stream.
    /// This uses an isolated real scope, not a mocked membership notification.
    #[tokio::test]
    async fn membership_commands_refresh_the_callers_runtime_without_moving_focus() {
        use crate::modules::room::{RoomJoin, RoomJoinParams, RoomLeave, RoomLeaveParams};
        use crate::persona::airc_citizen::AircCitizen;
        use crate::persona::identity_provider::PersonaIdentitySource;
        use crate::persona::PersonaAircRuntime;
        use crate::persona::PersonaAircRuntimeRegistry;
        use crate::routing::CallerIdentity;
        use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};
        use airc_core::PeerId;

        let home = tempfile::tempdir().expect("isolated airc home and wire root");
        let airc = Arc::new(
            airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path())
                .await
                .expect("open without the installed daemon or account wire"),
        );
        let academy = airc.join("academy").await.expect("initial focus");
        let registry = PersonaAircRuntimeRegistry::new();
        let runtime = registry.register(PersonaAircRuntime::from_attached(
            airc.peer_id().as_uuid(),
            "membership-test",
            home.path().to_path_buf(),
            airc.clone(),
            academy.channel,
            PersonaIdentitySource::FreshlyMinted,
        ));
        let ctx = Ctx {
            caller: Some(CallerIdentity::local_persona(airc.peer_id())),
            ..Default::default()
        };
        let join = RoomJoin {
            registry: registry.clone(),
        };
        let leave = RoomLeave { registry };
        // The live conversation takes this receiver before subscribing. A raw
        // Airc mutation below would pass the durable assertions but fail this one.
        let mut epoch = runtime.membership_epoch();
        let mut conversation = AircPersonaConversation::new(runtime.clone());
        conversation
            .prime()
            .await
            .expect("prime before the membership command");
        let joined = join
            .run(
                &ctx,
                RoomJoinParams {
                    room: "widgets".into(),
                },
            )
            .await
            .expect("join through the authenticated command");
        assert!(epoch.has_changed().expect("runtime still owns the sender"));
        epoch.borrow_and_update();
        assert!(
            !joined.is_default,
            "joining must not move the existing focus"
        );
        let membership = airc.subscription_set().await.expect("joined subscriptions");
        assert!(membership
            .all()
            .any(|s| s.room_id.as_uuid().to_string() == joined.room_id));
        assert_eq!(
            membership.default_subscription().expect("default").room_id,
            academy.channel
        );

        // The command updates the same durable identity used after a restart.
        let reopened = airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path())
            .await
            .expect("reopen the same isolated scope");
        assert_eq!(reopened.peer_id(), airc.peer_id());
        assert_eq!(
            reopened
                .subscription_set()
                .await
                .expect("persisted membership"),
            membership
        );
        drop(reopened);

        join.run(
            &ctx,
            RoomJoinParams {
                room: "widgets".into(),
            },
        )
        .await
        .expect("repeated join is membership-idempotent");
        assert_eq!(
            airc.subscription_set().await.expect("same rooms"),
            membership
        );
        // Preserve the existing runtime contract: a successful repeat refreshes
        // the snapshot too, without duplicating membership or changing focus.
        assert!(epoch.has_changed().expect("sender alive"));
        epoch.borrow_and_update();

        for invalid in [" ".to_string(), uuid::Uuid::new_v4().to_string()] {
            assert!(join
                .run(&ctx, RoomJoinParams { room: invalid })
                .await
                .is_err());
            assert!(!epoch.has_changed().expect("sender alive"));
            assert_eq!(
                airc.subscription_set().await.expect("unchanged membership"),
                membership
            );
        }
        let unknown = Ctx {
            caller: Some(CallerIdentity::local_persona(PeerId::new())),
            ..Default::default()
        };
        assert!(matches!(
            join.run(
                &unknown,
                RoomJoinParams {
                    room: "unrelated".into()
                }
            )
            .await,
            Err(CommandError::NotFound(_))
        ));
        assert!(matches!(
            leave
                .run(
                    &unknown,
                    RoomLeaveParams {
                        room: Some("widgets".into())
                    }
                )
                .await,
            Err(CommandError::NotFound(_))
        ));
        assert!(!epoch
            .has_changed()
            .expect("an unknown caller cannot mutate this runtime"));
        assert_eq!(
            airc.subscription_set()
                .await
                .expect("same caller membership"),
            membership
        );

        let parted = leave
            .run(
                &ctx,
                RoomLeaveParams {
                    room: Some("widgets".into()),
                },
            )
            .await
            .expect("leave through the authenticated command");
        assert_eq!(parted.room_id, joined.room_id);
        assert_eq!(parted.remaining, 1);
        assert!(epoch
            .has_changed()
            .expect("part refreshes the live snapshot"));
        epoch.borrow_and_update();
        let remaining = airc.subscription_set().await.expect("parted subscriptions");
        assert!(!remaining
            .all()
            .any(|s| s.room_id.as_uuid().to_string() == joined.room_id));
        assert_eq!(
            remaining.default_subscription().expect("default").room_id,
            academy.channel
        );
        assert!(leave
            .run(
                &ctx,
                RoomLeaveParams {
                    room: Some("widgets".into())
                }
            )
            .await
            .is_err());
        assert!(!epoch
            .has_changed()
            .expect("a rejected repeat must not signal a change"));
        assert_eq!(
            airc.subscription_set()
                .await
                .expect("unchanged after refusal"),
            remaining
        );

        let parted_default = leave
            .run(&ctx, RoomLeaveParams::default())
            .await
            .expect("unnamed leave targets the existing default");
        assert_eq!(
            parted_default.room_id,
            academy.channel.as_uuid().to_string()
        );
        assert_eq!(parted_default.remaining, 0);
        assert!(epoch
            .has_changed()
            .expect("leaving the default also refreshes"));
        let empty = airc.subscription_set().await.expect("no rooms remain");
        assert!(empty.default_subscription().is_none());
        assert_eq!(empty.all().count(), 0);

        // Drive the ACTUAL epoch consumer. Before the fix this reattached with
        // the SDK's empty-set general fallback, then default-room replay durably
        // rejoined general. Asserting the snapshot and released pump proves the
        // epoch was processed, rather than merely timing out before the branch.
        let wait = std::time::Duration::from_secs(1);
        assert!(tokio::time::timeout(wait, conversation.next_message())
            .await
            .is_err());
        assert_eq!(conversation.rooms.as_deref(), Some(&[][..]));
        assert!(conversation.pump.is_none() && conversation.inbox.is_none());
        assert_eq!(
            airc.subscription_set().await.expect("parked membership"),
            empty
        );
        assert_eq!(
            conversation
                .high_water_mark(32)
                .await
                .expect("empty history"),
            0
        );

        let mut started_empty = AircPersonaConversation::new(runtime.clone());
        started_empty
            .prime()
            .await
            .expect("priming an empty citizen parks");
        assert_eq!(started_empty.rooms.as_deref(), Some(&[][..]));
        assert!(started_empty.pump.is_none() && started_empty.inbox.is_none());
        assert_eq!(
            airc.subscription_set().await.expect("empty after prime"),
            empty
        );

        let rejoined = join
            .run(
                &ctx,
                RoomJoinParams {
                    room: "widgets".into(),
                },
            )
            .await
            .expect("a later join resumes the same living conversation");
        assert!(tokio::time::timeout(wait, conversation.next_message())
            .await
            .is_err());
        let resumed = uuid::Uuid::parse_str(&rejoined.room_id).expect("room receipt UUID");
        assert_eq!(conversation.rooms.as_deref(), Some(&[resumed][..]));
        assert!(conversation.pump.is_some() && conversation.inbox.is_some());
        assert_eq!(
            airc.subscription_set()
                .await
                .expect("resumed membership")
                .all()
                .count(),
            1
        );

        leave
            .run(&ctx, RoomLeaveParams::default())
            .await
            .expect("finish parked");
        assert!(tokio::time::timeout(wait, conversation.next_message())
            .await
            .is_err());
        assert!(conversation.pump.is_none() && conversation.inbox.is_none());
        assert_eq!(
            airc.subscription_set()
                .await
                .expect("final empty membership")
                .all()
                .count(),
            0
        );
    }

    use super::*;

    // what this catches: the dead-live-stream detector. The first tick after an
    // open admits the backlog and must not re-open; a later tick that admits
    // store events while zero live frames arrived is the dead stream (the 03:11Z
    // daemon restart); a tick that admits nothing, or one where live frames flowed,
    // is healthy. usize::MAX is the inbox-gone sentinel, never a count.
    #[test]
    fn the_live_stream_is_dead_only_when_the_store_delivers_and_the_stream_did_not() {
        assert!(
            !live_stream_missed(0, 5, true),
            "first tick admits the backlog"
        );
        assert!(
            live_stream_missed(0, 5, false),
            "store delivered, stream silent"
        );
        assert!(!live_stream_missed(3, 5, false), "live frames flowed");
        assert!(!live_stream_missed(0, 0, false), "nothing to deliver");
        assert!(
            !live_stream_missed(0, usize::MAX, false),
            "inbox gone is not a count"
        );
    }
    use crate::persona::airc_citizen::StubAircCitizen;

    // what this catches: the store-backed catch-up's synthesized event must
    // decode as a room turn with the row's id, sender and text (the daemon page
    // was empty for the run room on 2026-09-04; this path is the hearing).
    #[test]
    fn a_durable_chat_row_becomes_an_admissible_turn() {
        let room = Uuid::new_v4();
        let row = crate::persona::durable_history::RoomRow {
            id: Uuid::new_v4(),
            sender: Uuid::new_v4(),
            occurred_at_ms: 1_788_513_127_000,
            text: "Joel here — which card do you hold?".to_string(),
        };
        let (id, sender) = (row.id, row.sender);
        let msg = perceptual_from_event(&event_from_row(room, row)).expect("a text row is a turn");
        assert_eq!((msg.event_id, msg.peer_id, msg.room_id), (id, sender, room));
        assert_eq!(msg.text, "Joel here — which card do you hold?");
    }

    // what this catches: the same line under two ids (the sender's message id
    // and the say's event id) admitting twice.
    #[test]
    fn the_same_line_under_a_second_id_is_seen_by_fingerprint() {
        let mut seen = SeenRooms::default();
        let room = Uuid::new_v4();
        let peer = Uuid::new_v4();
        let fp = SeenRooms::fingerprint(peer, "one line");
        assert!(!seen.was_seen_text(room, fp));
        seen.note_text(room, fp);
        assert!(seen.was_seen_text(room, fp));
        assert!(!seen.was_seen_text(room, SeenRooms::fingerprint(peer, "another line")));
    }

    /// Regression test for the slice-13.6 reviewer fix to PR #1514:
    /// `next_message` MUST refuse if `prime` wasn't called first.
    /// Per [[no-fallbacks-ever]] the lazy-subscribe fallback that
    /// used to live in next_message was a soft-language degradation
    /// path; this test locks the new typed-error contract.
    ///
    /// Construction is free; primed state stays false; the first
    /// `next_message` returns a typed `Err` naming the missing call.
    #[tokio::test]
    async fn next_message_without_prime_errors_visibly() {
        let citizen: Arc<dyn AircCitizen> = Arc::new(StubAircCitizen::new(uuid::Uuid::new_v4()));
        let mut conversation = AircPersonaConversation::new(citizen);
        let err = conversation
            .next_message()
            .await
            .expect_err("next_message must error when stream is unprimed");
        assert!(
            err.contains("prime"),
            "error must name the missing call: {err}"
        );
    }

    /// Perception of the two on-wire room-turn shapes. These lock the
    /// human→persona WAKE path fixed after the glass-box diagnosis showed
    /// a `chat/send` reaching the subscribe stream but being dropped as
    /// `as_text() == None`.
    mod perceptual {
        use super::super::perceptual_from_event;
        use crate::airc::realtime::AircRealtimeEnvelope;
        use crate::airc::realtime_wire::{body_for_envelope, headers_for_envelope};
        use airc_core::{
            Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptEvent,
            TranscriptKind,
        };
        use serde_json::json;
        use uuid::Uuid;

        fn event(peer: PeerId, body: Option<Body>, headers: Headers) -> TranscriptEvent {
            TranscriptEvent {
                event_id: EventId::from_u128(1),
                room_id: RoomId::from_u128(2),
                peer_id: peer,
                client_id: ClientId::from_u128(4),
                kind: TranscriptKind::Message,
                occurred_at_ms: 100,
                lamport: 7,
                target: MentionTarget::All,
                headers,
                body,
                attachment: None,
                receipt: None,
                metadata: serde_json::Value::Null,
            }
        }

        /// Encode an envelope exactly as `chat/send` → `airc/realtime-publish`
        /// would put it on the wire (Body::Json + continuum body-hint header),
        /// so the test exercises the real production decode.
        fn wire_event(peer: PeerId, envelope_json: serde_json::Value) -> TranscriptEvent {
            let envelope: AircRealtimeEnvelope =
                serde_json::from_value(envelope_json).expect("valid realtime envelope");
            let body = body_for_envelope(&envelope).expect("encode body");
            let headers = headers_for_envelope(&envelope);
            event(peer, Some(body), headers)
        }

        // what this catches: a peer's plain-text say() is perceived and
        // attributed to its transport peer_id — the direct path must keep
        // working (persona↔persona was never broken).
        #[test]
        fn plain_text_say_is_perceived() {
            let peer = PeerId::from_u128(42);
            let ev = event(peer, Some(Body::text("hello from a peer")), Headers::new());
            let msg = perceptual_from_event(&ev).expect("text body is a room turn");
            assert_eq!(msg.text, "hello from a peer");
            assert_eq!(msg.peer_id, peer.as_uuid());
            assert_eq!(msg.lamport, 7);
            // regression for the 2026-07-23 nil-room turn: the transport's
            // room_id MUST ride into the perceptual message (A.6) — dropping
            // it bound operator/CLI turns to a nil room where every
            // room-scoped RAG source abstained (no board, no kanban, no
            // roster) and the reply had no room to land in.
            assert_eq!(msg.room_id, RoomId::from_u128(2).as_uuid());
        }

        // what this catches: a presence heartbeat (`airc.heartbeat.kind` header) is
        // dropped at the door by the RECEIVE-side guard, and a plain say is not. The
        // subscribe-time `Not(Has(heartbeat))` filter arrived at the daemon inverted
        // (CambrianTech/airc#1368, 2026-09-04): citizens received ONLY heartbeats and
        // never a message, so every room was heard through the store page alone.
        #[test]
        fn heartbeat_is_dropped_at_the_door_and_a_say_is_not() {
            let peer = PeerId::from_u128(42);
            let mut headers = Headers::new();
            headers.insert(
                airc_lib::HEADER_HEARTBEAT_KIND.to_string(),
                "alive".to_string(),
            );
            let beat = event(peer, Some(Body::text("{\"kind\":\"alive\"}")), headers);
            assert!(crate::persona::airc_citizen::is_heartbeat(&beat));
            let say = event(peer, Some(Body::text("hello from a peer")), Headers::new());
            assert!(!crate::persona::airc_citizen::is_heartbeat(&say));
        }

        // what this catches: a live streaming token chunk (airc.stream.* headers,
        // text body) must NOT be perceived as a spoken room turn — card 65fca48d,
        // live 2026-07-24: chunks were stamped Durable by the publish path, came
        // back from page_recent as transcript, and flooded every persona's window
        // with per-250ms fragments ("Anwen: I", "Anwen: see that you're…"),
        // waking peers per fragment and leaking a streamed "PASS" sentinel as
        // room content. The settled utterance arrives separately via say(); the
        // chunk is typing-indicator-class traffic, skipped with a NAMED reason.
        #[test]
        fn stream_chunk_is_not_a_room_turn() {
            let peer = PeerId::from_u128(42);
            let mut headers = Headers::new();
            headers.insert(
                airc_lib::HEADER_STREAM_ID.into(),
                "b1946ac9-2a75-4a6f-9182-6b1c6e0e7a11".to_string(),
            );
            headers.insert(airc_lib::HEADER_STREAM_SEQ.into(), "3".to_string());
            headers.insert(
                airc_lib::HEADER_STREAM_KIND.into(),
                "text.token".to_string(),
            );
            let ev = event(peer, Some(Body::text(" see that you're")), headers);
            assert_eq!(
                perceptual_from_event(&ev),
                Err("stream_chunk"),
                "a streaming fragment must be skipped with its named reason — \
                 never surfaced as a spoken room line"
            );
        }

        // what this catches: THE bug. chat/send arrives as a Body::Json
        // chat_transcript envelope (as_text()==None). It MUST be perceived,
        // and attributed to the true logical sender (inline.senderId), not
        // the core transport peer that relayed the publish.
        #[test]
        fn chat_transcript_envelope_is_perceived_with_true_sender() {
            let relay_peer = PeerId::from_u128(7711); // the core's publish peer
            let human_sender = Uuid::from_u128(0xB0B);
            let ev = wire_event(
                relay_peer,
                json!({
                    "eventId": Uuid::from_u128(9).to_string(),
                    "roomId": Uuid::from_u128(2).to_string(),
                    "sourceId": human_sender.to_string(),
                    "createdAtMs": 100u64,
                    "delivery": "durable",
                    "payload": {
                        "kind": "existing_schema",
                        "payload": {
                            "schema": "chat_transcript",
                            "inline": {
                                "messageId": Uuid::from_u128(3).to_string(),
                                "text": "Asha, are you there?",
                                "senderId": human_sender.to_string(),
                                "replyToId": null,
                            }
                        }
                    }
                }),
            );
            let msg = perceptual_from_event(&ev).expect("chat_transcript is a room turn");
            assert_eq!(msg.text, "Asha, are you there?");
            assert_eq!(
                msg.peer_id, human_sender,
                "attribution must be the logical sender, not the relay peer"
            );
        }

        // what this catches: a non-message envelope (event-bridge, also
        // as_text()==None) must NOT be surfaced as a room turn — only actual
        // chat is perceptual, not every Json body on the stream.
        #[test]
        fn event_bridge_envelope_is_not_a_room_turn() {
            let ev = wire_event(
                PeerId::from_u128(5),
                json!({
                    "eventId": Uuid::from_u128(9).to_string(),
                    "roomId": Uuid::from_u128(2).to_string(),
                    "sourceId": "continuum-peer",
                    "createdAtMs": 100u64,
                    "delivery": "durable",
                    "payload": {
                        "kind": "existing_schema",
                        "payload": {
                            "schema": "event_bridge_payload",
                            "inline": { "eventName": "data:users:created", "id": "x" }
                        }
                    }
                }),
            );
            assert_eq!(
                perceptual_from_event(&ev),
                Err("non_chat_schema"),
                "an event-bridge envelope is a LEGIT skip — and the reason must say so, \
                 distinguishable from a decode error or a lost body-hint header (#177)"
            );
        }
    }
}

impl AircPersonaConversation {
    /// The newest lamport across the current subscription snapshot (two events
    /// per room). An empty snapshot has no tip; a failed read counts as not moved.
    async fn rooms_high_water_mark(&self) -> u64 {
        self.high_water_mark(2).await.unwrap_or(self.last_lamport) // unwrap_or: a failed page reads as not moved
    }

    /// One live event through the door: heartbeat/chunk guards, the raw probe,
    /// the work-bridge, the decoder, the self-filter, the lamport advance.
    /// `Some(msg)` = a room turn to serve; `None` = consumed, keep polling.
    async fn admit_event(
        &mut self,
        event: std::sync::Arc<TranscriptEvent>,
    ) -> Option<IncomingMessage> {
        // Presence heartbeats are dropped HERE, not at subscribe time — the
        // daemon inverted the subscribe-time filter (see `subscribe_every_room`),
        // and a receive-side check is correct either way. Before any decode/probe.
        if crate::persona::airc_citizen::is_heartbeat(&event) {
            return None;
        }
        // A stream chunk is NEVER a room turn — skip it at the door, before the
        // decode and before the raw-event line. Every persona's subscribe stream
        // receives every OTHER persona's token fragments: measured live during a
        // SWE solve, 2644 of 4776 filtered inbound events (55%) were chunks, fanned
        // out identically to all four personas (1195 each) and decoded-then-discarded
        // by each independently — O(personas x tokens) of work in the attention path
        // of a persona who is trying to concentrate, plus 65% of the probe stream.
        //
        // Deliberately NOT probed: routine traffic taking its expected path is not an
        // anomaly, and a probe here would rebuild the exact flood this removes. The
        // decoder still classifies chunks as `stream_chunk` for any caller that
        // reaches it by another route, so nothing goes dark — the reason string
        // remains the single source of truth.
        //
        // This is the receive-side half. The events still cross the wire; not sending
        // a peer's fragments to peers at all is airc-side (#275) and stays open.
        if crate::airc::realtime_wire::is_stream_chunk(&event) {
            return None;
        }
        // Every non-chunk event advances the rejoin-replay watermark, so a
        // later reopen replays only what this stream genuinely never saw.
        self.last_lamport = self.last_lamport.max(event.lamport);
        // #146 diagnostic: EVERY raw event this persona's subscribe
        // stream yields, before any filter. If this probe never fires
        // under a room burst, the stream is empty → airc-lib delivery
        // gap. If it fires but perceptual/self drops it, the gap is
        // continuum-side (decode/self-filter). One line per event,
        // greppable by probe_class, cheap enough for a live stream.
        let body_kind = match event.body.as_ref() {
            None => "none",
            Some(b) if b.as_text().is_some() => "text",
            Some(_) => "json",
        };
        crate::probe!(
            class = "persona.inbound.raw_event",
            persona = %self.own_peer_id,
            from_peer = %event.peer_id,
            body_kind,
            "persona subscribe stream yielded a raw event (#146)"
        );
        // Card-state transitions bridge onto the internal bus HERE —
        // the persona subscribe streams are the only channel-complete
        // receiver this core has (the daemon attach covers ONE room),
        // and this runs BEFORE the perceptual filter and the self-skip
        // so a citizen's own `work/state` echo counts. Once per wire
        // event process-wide (the bridge dedups by event id); this is
        // the single emitter the grade-on-done subscriber hears.
        crate::modules::work::bridge_wire_work_event(&event).await;
        // Recover a perceptual room turn. Two on-wire shapes
        // reach a persona's subscribe stream and both are
        // messages it must hear: a peer's plain-text `say()`
        // (`Body::Text`) and a `chat/send` from a human / the
        // web client / any non-`say` caller (the continuum
        // realtime envelope as `Body::Json`, `chat_transcript`
        // schema). `perceptual_from_event` decodes both; a
        // `None` means the event is not a room turn (presence,
        // event-bridge, media-control, binary) — skip it.
        // WHAT the rejected event actually was. `reason` names the branch
        // that refused it; these two name the SHAPE, which is what a fix has
        // to be written against.
        //
        // Measured 2026-08-13 (#410): every `airc msg` a human sends reaches
        // every citizen and is dropped as `no_continuum_body_hint`, because
        // `envelope_from_event` gates on HEADER_FORGE_BODY_HINT — a stamp only
        // continuum's own clients apply. Teaching the decoder the CLI's shape
        // needs that shape, and the reason string alone cannot supply it; a
        // decoder arm written against a GUESSED body is how presence frames
        // become fabricated perception. So: capture the kind and a bounded
        // preview, and let the next fix be written against a fact.
        //
        // Bounded to 160 chars and emitted only on the ALREADY-firing filtered
        // line — no new event, no new flood ([[the stream-chunk skip stays
        // deliberately unprobed]]).
        let event_kind = format!("{:?}", event.kind);
        let body_preview = match event.body.as_ref() {
            None => "<none>".to_string(),
            Some(b) => match b.as_text() {
                Some(t) => t.chars().take(160).collect(),
                None => serde_json::to_string(b)
                    .unwrap_or_else(|e| format!("<unserializable: {e}>"))
                    .chars()
                    .take(160)
                    .collect(),
            },
        };
        let message = match perceptual_from_event(&event) {
            Ok(message) => message,
            Err(reason) => {
                // A decode ERROR is loud — a message-shaped body we
                // failed to read is exactly the #177 blindness this
                // named-reason contract exists for. The two LEGIT
                // non-turn shapes (event-bridge frames, presence,
                // work-board events) flooded 38k INFO lines/day
                // across the roster and drowned real signal — they
                // stay observable at debug, counted by probe_class
                // either way.
                if reason == "envelope_decode_error" {
                    crate::probe!(
                        class = "persona.inbound.filtered_non_turn",
                        persona = %self.own_peer_id,
                        from_peer = %event.peer_id,
                        body_kind,
                        event_kind,
                        body_preview,
                        reason,
                        "message-shaped event FAILED to decode — a peer may be structurally unheard (#177)"
                    );
                } else {
                    crate::probe!(
                        class = "persona.inbound.filtered_non_turn",
                        persona = %self.own_peer_id,
                        from_peer = %event.peer_id,
                        body_kind,
                        event_kind,
                        body_preview,
                        reason,
                        "raw event was not a perceptual room turn — skipped (#146/#177)"
                    );
                }
                return None;
            }
        };
        // Skip our own turn, matched on the RESOLVED sender so a
        // self-authored chat_transcript is caught too — not just
        // a self `say()` (whose transport peer is us).
        //
        // PROBED, because this drop was SILENT and that cost a whole round
        // (2026-08-17). `benchmark/dispatch` authored `@Atlas (to you)`
        // kickoffs THROUGH Atlas (the operator has no self-peer, so
        // `curator_airc` borrows the lexicographically-first live citizen —
        // her). Every kickoff died right here: no error, no probe, no turn,
        // `kickoff_errors: []`, and hours spent looking at the grader, the
        // roster and the model. The skip is CORRECT — nobody answers their own
        // speech — but a message vanishing without a trace is how a structural
        // failure reads as "the citizen chose not to work"
        // ([[an-absence-is-an-unfinished-measurement]]).
        if message.peer_id == self.own_peer_id {
            crate::probe!(
                class = "persona.inbound.skipped_self_authored",
                persona = %self.own_peer_id,
                from_peer = %event.peer_id,
                text_len = message.text.len(),
                "skipped a message this persona is recorded as having said — \
                 if it was ADDRESSED to her, whoever sent it authored through \
                 her identity and she cannot hear it"
            );
            return None;
        }
        Some(message)
    }
}
