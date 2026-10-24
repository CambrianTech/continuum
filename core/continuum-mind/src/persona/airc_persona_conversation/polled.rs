use super::*;

/// Wraps an [`AircCitizen`] and projects it onto the substrate's
/// [`PersonaConversation`] contract. Owns the airc subscribe stream
/// across calls so successive `next_message` invocations are a
/// continuation (not a fresh resubscription that would drop in-flight
/// events).
/// What a poll of the inbound stream produced.
pub(super) enum Polled {
    Event(Option<Result<AttentionEvent, airc_lib::LiveLag>>),
    Membership,
    Quiet,
}

/// How often the store-backed catch-up pages every subscribed room's tail.
pub(super) const CATCH_UP_EVERY: std::time::Duration = std::time::Duration::from_secs(60);


/// Event ids remembered per room to tell "seen" from "dropped" (bounded).
pub(super) const SEEN_RING: usize = 128;

/// Her perception region and its wake channel, registered on the core's one feed.
pub(super) struct MindFeed {
    pub(super) persona: Uuid,
    pub(super) region: Arc<std::sync::Mutex<crate::persona::perception_region::PerceptionRegion>>,
    pub(super) wake_tx: tokio::sync::mpsc::Sender<crate::cognition::conversation::Wake>,
    /// Her wakes until the loop's Wake consumer takes them (`take_wakes`).
    pub(super) wakes: Option<tokio::sync::mpsc::Receiver<crate::cognition::conversation::Wake>>,
}

impl MindFeed {
    pub(super) fn boot(persona: Uuid) -> Option<Self> {
        let Some(dir) = crate::persona::perception_feed::mind_dir(persona) else {
            continuum_base::probe!(
                class = "mind.feed.unbooted",
                persona = %persona,
                "no persistent home: her perception region cannot load her durable state"
            );
            return None;
        };
        // Turn budget 0: the awareness strip's context share is unread until the
        // loop's Wake consumer composes it, so it reports 0 rather than a guess.
        let now = EpochMs::now().millis();
        let (mut region, _strip) = crate::persona::perception_region::PerceptionRegion::boot(
            airc_core::PeerId::from_uuid(persona),
            &dir,
            0,
            now,
        );
        // Her private continuation is never in mind-state.json; it is sealed in her mind
        // store (PRIVACY-OF-THOUGHT.md sink 10). Restore it when no open one was saved.
        if region.continuation().is_none() {
            if let Some(private) = crate::persona::mind_store::sealed_continuation(persona) {
                region.restore_private_continuation(private, now);
            }
        }
        let (wake_tx, wakes) = tokio::sync::mpsc::channel(8);
        Some(Self { persona, region: Arc::new(std::sync::Mutex::new(region)), wake_tx, wakes: Some(wakes) })
    }
}

impl Drop for MindFeed {
    /// A conversation that ends takes her off the feed; the registry would otherwise
    /// keep feeding a region nobody reads.
    fn drop(&mut self) {
        crate::persona::perception_feed::unregister(self.persona);
    }
}

/// Per room: the WALL-TIME floor adopted at first sight (nothing older is ever
/// replayed) and a bounded ring of EVENT IDS actually forwarded (live or paged).
/// Neither lamport nor "max seen" can stand in for this: a lamport is the
/// publisher's logical clock, so a quiet human's line carries a smaller lamport
/// than the busy citizens' receipts around it and read as "old" (2026-09-04:
/// zero admissions across three builds); and live `event` frames keep arriving
/// after the daemon drops a `message`, so a max-seen mark leaps past the line.
#[derive(Default)]
pub(super) struct SeenRooms {
    /// Per-room ring of (peer, text) fingerprints — the same line under two ids.
    pub(super) texts: std::collections::HashMap<Uuid, std::collections::VecDeque<u64>>,
    pub(super) floor_ms: std::collections::HashMap<Uuid, u64>,
    pub(super) seen: std::collections::HashMap<Uuid, std::collections::VecDeque<Uuid>>,
}

impl SeenRooms {
    pub(super) fn fingerprint(peer: Uuid, text: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        peer.hash(&mut h);
        text.hash(&mut h);
        h.finish()
    }

    pub(super) fn note_text(&mut self, room: Uuid, fp: u64) {
        let ring = self.texts.entry(room).or_default();
        if ring.contains(&fp) {
            return;
        }
        if ring.len() == SEEN_RING {
            ring.pop_front();
        }
        ring.push_back(fp);
    }

    pub(super) fn was_seen_text(&self, room: Uuid, fp: u64) -> bool {
        self.texts.get(&room).is_some_and(|r| r.contains(&fp))
    }

    pub(super) fn note(&mut self, room: Uuid, event_id: Uuid) {
        let ring = self.seen.entry(room).or_default();
        if !ring.contains(&event_id) {
            ring.push_back(event_id);
            while ring.len() > SEEN_RING {
                ring.pop_front();
            }
        }
    }
    pub(super) fn was_seen(&self, room: Uuid, event_id: Uuid) -> bool {
        self.seen.get(&room).is_some_and(|r| r.contains(&event_id))
    }
}

/// One decoded attention envelope. The raw event stays owned until its async
/// work bridge completes; routing-only traffic never occupies the attention FIFO.
#[derive(Clone)]
pub(super) struct AttentionEvent {
    pub(super) event: Arc<TranscriptEvent>,
    pub(super) perception: Result<Arc<IncomingMessage>, &'static str>,
}

impl AttentionEvent {
    pub(super) fn classify(event: Arc<TranscriptEvent>) -> Option<Self> {
        if crate::persona::airc_citizen::is_heartbeat(&event)
            || crate::routing::airc_realtime_wire::is_stream_chunk(&event)
            || crate::routing::airc_realtime_wire::is_command_frame(&event)
        {
            return None;
        }
        let perception = match crate::cognition::conversation::room_event::room_work_from_event(&event) {
            Ok(Some(work)) => Ok(IncomingMessage {
                event_id: event.event_id.as_uuid(), lamport: event.lamport,
                peer_id: event.peer_id.as_uuid(), room_id: event.room_id.as_uuid(),
                text: String::new(), media: Vec::new(), work: Some(work),
            }),
            Ok(None) => perceptual_from_event(&event),
            Err(reason) => Err(reason),
        }.map(Arc::new);
        // Decode faults remain observable at the consumer. Known non-perception
        // belongs to its existing durable/telemetry owner, not a citizen's queue.
        match perception {
            Err("non_chat_schema" | "no_continuum_body_hint" | "work_presence" | "work_lease_renewal") => None,
            _ => Some(Self { event, perception }),
        }
    }
}

/// Page every subscribed room's durable tail and forward, into the inbox, the
/// events the live stream never delivered (above the room's floor, not in the
/// seen ring). Returns `(rooms paged, forwarded)`; `usize::MAX` forwarded means
/// the inbox is gone. Heartbeats and chunks are skipped here as at the door.
/// A forwarded line from outside the citizenry (or one naming nobody in
/// particular — the turn decides mentions) raises the citizen's directed-pending
/// flag so a parked self-work lane wait yields to it.
pub(super) fn signal_if_directed(own: uuid::Uuid, attention: &AttentionEvent) {
    let Ok(message) = &attention.perception else { return; };
    let peer = message.peer_id;
    if peer == own || message.work.is_some() {
        return;
    }
    // Only a line that would ADMIT as a turn may raise the flag: the board's
    // System events ride the same stream from the same non-citizen peers and
    // are filtered at the door — on 8c6733c65 they kept the flag up for the
    // whole roster (25 turns, 25 yields, 0 lanes acquired).
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
            crate::cognition::conversation::persona_identity::PersonaIdentity::new(own, rt.agent_name().to_string())
        })
        .is_some_and(|me| me.mentions(&message.text));
    let sender_is_human = crate::cognition::room_directory::is_human_peer(peer);
    if crate::cognition::workspace::TurnAttention::for_message(mentioned, sender_is_human)
        .requires_priority()
    {
        crate::cognition::directed_pending::signal(own);
    }
}

pub(super) async fn catch_up_from_store(
    runtime: &dyn AircCitizen,
    seen: &std::sync::Mutex<SeenRooms>,
    tx: &tokio::sync::mpsc::Sender<Result<AttentionEvent, airc_lib::LiveLag>>,
) -> (usize, usize) {
    let rooms = runtime.subscribed_rooms().await.unwrap_or_default(); // unwrap_or: an unreadable room list = nothing to catch up this tick
    let mut forwarded = 0usize;
    let mut paged = 0usize;
    let mut failed = 0usize;
    let mut first_error: Option<String> = None;
    let mut sample_newest = 0u64;
    let mut sample: Option<(Uuid, u64, u64, usize, String, String, bool)> = None;
    let room_list = rooms.iter().map(|r| airc_core::shown_id::shown_form(*r)).collect::<Vec<_>>().join(",");
    let mut empty: Vec<String> = Vec::new();
    for room in rooms {
        // THE CORE'S OWN CHAT STORE is the durable page (see
        // `durable_history::room_rows`): the daemon's ring page was empty for
        // the busiest room on every citizen (2026-09-04, `catch_up_rooms`).
        let mut events = match crate::persona::durable_history::room_rows(room, crate::persona::durable_history::RoomPage::Newest, CATCH_UP_PAGE).await
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
            empty.push(airc_core::shown_id::shown_form(room));
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
                    airc_core::shown_id::shown_form(e.peer_id.as_uuid()),
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
            {
                continue;
            }
            let Some(attention) = AttentionEvent::classify(Arc::new(event)) else { continue; };
            let event = &attention.event;
            // A line the operator's `chat/send` persisted under its own message id
            // arrives live under the say's EVENT id: same peer, same text, two ids.
            // The fingerprint ring keeps it one turn.
            let fingerprint = attention.perception.as_ref().ok()
                .filter(|message| message.work.is_none())
                .map(|message| SeenRooms::fingerprint(message.peer_id, &message.render_content()));
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
            if tx.send(Ok(attention.clone())).await.is_err() {
                return (paged, usize::MAX);
            }
            signal_if_directed(runtime.peer_id(), &attention);
            forwarded += 1;
        }
    }
    if let Some((room, floor_ms, newest_ms, count, peer, head, was_seen)) = sample {
        continuum_base::probe!(
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
    continuum_base::probe!(
        class = "persona.inbound.catch_up_rooms",
        rooms = %room_list,
        empty = %empty.join(","),
        paged = paged as u64,
        "the rooms this tick paged, and the ones whose page held no conversation"
    );
    if failed > 0 {
        continuum_base::probe!(
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
    pub(super) runtime: Arc<dyn AircCitizen>,
    /// The subscription snapshot owned by this conversation. `None` is
    /// unprimed; an empty set is primed but parked until the next membership
    /// epoch. Neither state may implicitly join a default room.
    pub(super) rooms: Option<Vec<Uuid>>,
    /// A consumed epoch whose async reattach has not finished. `next_message`
    /// is cancellation-safe: a competing service-loop wake cannot lose it.
    pub(super) refresh_pending: bool,
    /// Input already perceived during work, still owned for later room attention.
    pub(super) perceived_backlog: std::collections::VecDeque<Arc<IncomingMessage>>,
    /// A live event being admitted; retained across cancellation at the work bridge.
    pub(super) ready_event: Option<AttentionEvent>,
    /// The persona's own peer_id, captured at construction. Used by
    /// `next_message` to skip self-loop echoes WITHIN the projection
    /// — the service loop ALSO skips by persona's instance peer_id;
    /// the redundancy lets the conversation be honest about whose
    /// stream it's projecting (defense in depth, costs nothing).
    pub(super) own_peer_id: uuid::Uuid,
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
    pub(super) inbox: Option<
        tokio::sync::mpsc::Receiver<Result<AttentionEvent, airc_lib::LiveLag>>,
    >,
    /// The drain task behind `inbox`; aborted and replaced on every re-open.
    pub(super) pump: Option<tokio::task::JoinHandle<()>>,
    /// Per-room floors and seen rings, SHARED across pump restarts. A pump
    /// re-open (membership change) must not re-adopt floors: on the previous
    /// build every re-open reset them to "now", so a line already in the page
    /// read as old forever (zero admissions across four builds).
    pub(super) seen: std::sync::Arc<std::sync::Mutex<SeenRooms>>,
    /// Membership-change cue (P0 20b44763): when the citizen joins a room at
    /// RUNTIME (benchmark dispatch moving her into a fresh run room), this
    /// epoch moves and `next_message` re-opens the stream so the new room's
    /// events reach her. Without it the stream keeps the spawn-time channel
    /// snapshot forever and every later-joined room is born deaf — measured
    /// live 2026-08-15: three bench rounds, 12 addressed kickoffs each, zero
    /// turns. NOT the forbidden auto-resubscribe fallback documented on the
    /// terminal-`None` arm below: that masks a wire fault; this answers a
    /// REAL membership event, the system-law event-driven shape.
    pub(super) membership_epoch: tokio::sync::watch::Receiver<u64>,
    /// Highest lamport this conversation has SEEN on its raw stream (or replayed).
    /// The rejoin replay's dedupe watermark: only events strictly newer are ever
    /// replayed, so a reopen can never re-feed history as fresh perception.
    pub(super) last_lamport: u64,
    /// Immutable attach cutoff; None means readiness has not completed.
    pub(super) initial_watermark: Option<u64>,
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
    pub(super) next_catch_up: tokio::time::Instant,
    /// Room-turns recovered by the rejoin replay, yielded ahead of the live
    /// stream. See the epoch-reopen branch in `next_message`.
    pub(super) rejoin_backlog: std::collections::VecDeque<IncomingMessage>,
    /// Her perception region on the core's ONE feed (EVENT-MIND.md §1b), booted at
    /// the first membership read. `None` until then, or when her durable dir cannot
    /// be resolved (probe `mind.feed.unbooted` says so).
    pub(super) mind: Option<MindFeed>,
}

/// What core's membership test reads of a conversation: its subscription snapshot, and
/// whether the pump and its inbox are running.
#[cfg(any(test, feature = "test-fixtures"))]
impl AircPersonaConversation {
    /// The room subscription snapshot (`None` unprimed, empty primed but parked).
    pub fn subscribed_rooms(&self) -> Option<&[Uuid]> {
        self.rooms.as_deref()
    }

    /// `(pump running, inbox open)`.
    pub fn pump_attached(&self) -> (bool, bool) {
        (self.pump.is_some(), self.inbox.is_some())
    }
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
            perceived_backlog: std::collections::VecDeque::new(),
            ready_event: None,
            own_peer_id,
            inbox: None,
            pump: None,
            seen: std::sync::Arc::new(std::sync::Mutex::new(SeenRooms::default())),
            membership_epoch,
            last_lamport: 0,
            initial_watermark: None,
            next_catch_up: tokio::time::Instant::now() + CATCH_UP_EVERY,
            rejoin_backlog: std::collections::VecDeque::new(),
            mind: None,
        }
    }

    /// Her events from the ONE feed for `rooms`. Boots her region on first use and
    /// re-registers with a fresh sender on every membership change (the pump that
    /// owned the previous receiver is replaced with it).
    pub(super) async fn feed_events(
        &mut self,
        rooms: &[Uuid],
    ) -> Option<
        impl futures::Stream<Item = Result<Arc<TranscriptEvent>, airc_lib::LiveLag>> + Send + 'static,
    > {
        use futures::StreamExt as _;
        let persona = self.own_peer_id;
        if self.mind.is_none() {
            self.mind = MindFeed::boot(persona);
        }
        let mind = self.mind.as_ref()?;
        // Her name, by the same lookup `signal_if_directed` uses: the region detects
        // "addressed to me" through `PersonaIdentity::mentions`, so a region without
        // her name would be blind to every @-mention. No name, no region.
        let Some(agent_name) = crate::persona::PersonaAircRuntimeRegistry::try_global()
            .and_then(|r| r.get(persona))
            .map(|rt| rt.agent_name().to_string())
        else {
            continuum_base::probe!(
                class = "mind.feed.unbooted",
                persona = %persona,
                "her runtime is not registered: no name to detect mentions with; the region waits for the next membership read"
            );
            return None;
        };
        let (tx, rx) = tokio::sync::mpsc::channel(INBOX_CAPACITY);
        crate::persona::perception_feed::register(
            persona,
            airc_core::PeerId::from_uuid(persona),
            &agent_name,
            Arc::clone(&mind.region),
            mind.wake_tx.clone(),
            Some(tx),
        );
        let mut named = Vec::with_capacity(rooms.len());
        for room in rooms {
            let name = crate::persona::airc_citizen::room_name_by_id(*room)
                .await
                .unwrap_or_else(|| room.to_string()); // unwrap_or: an unnamed room is perceived under its id, never dropped
            named.push((*room, name));
        }
        crate::persona::perception_feed::refresh_membership(persona, &named);
        Some(tokio_stream::wrappers::ReceiverStream::new(rx).map(Ok))
    }

    /// Borrow the underlying citizen — useful for the supervisor's
    /// registry-eviction path (slice 12) where the supervisor needs
    /// to look up the citizen back from the conversation for graceful
    /// shutdown.
    pub fn runtime(&self) -> &Arc<dyn AircCitizen> {
        &self.runtime
    }

    pub(super) fn stop_stream(&mut self) {
        if let Some(old) = self.pump.take() {
            old.abort();
        }
        self.inbox = None;
    }

    /// Rebind the existing perception owner to its durable room set. The SDK's
    /// empty-set subscribe falls back to general, so an empty membership parks
    /// here instead. The existing epoch is also how this state resumes.
    pub(super) async fn refresh_membership(&mut self) -> Result<bool, String> {
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
            // ONE FEED, phase-in: the core's single attach per room feeds her pump
            // beside the private subscription, deduped by event id in the pump. The
            // private subscription and its store catch-up retire together once this
            // path is proven live (Cormac, 2026-10-04: never a window with no path).
            match self.feed_events(&rooms).await {
                Some(feed) => self.install_stream(futures::stream::select(stream, feed)),
                None => self.install_stream(stream),
            }
        } else {
            self.stop_stream();
            continuum_base::probe!(
                class = "persona.inbound.no_rooms",
                persona = %self.own_peer_id,
                "no subscribed rooms — perception waits for a membership change"
            );
        }
        self.rejoin_backlog
            .retain(|message| rooms.contains(&message.room_id));
        self.perceived_backlog
            .retain(|message| rooms.contains(&message.room_id));
        if self
            .ready_event
            .as_ref()
            .is_some_and(|event| !rooms.contains(&event.event.room_id.as_uuid()))
        {
            self.ready_event = None;
        }
        self.rooms = Some(rooms);
        Ok(active)
    }

    /// Page the actual subscription snapshot, never a lazy default-room read.
    /// This also keeps a no-focus subscription from replaying another room.
    pub(super) async fn page_subscribed_rooms(&self, limit: usize) -> Result<Vec<TranscriptEvent>, String> {
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
    pub(super) fn install_stream(
        &mut self,
        stream: impl futures::Stream<Item = Result<Arc<TranscriptEvent>, airc_lib::LiveLag>>
            + Send
            + 'static,
    ) {
        let mut stream: futures::stream::BoxStream<
            'static,
            Result<Arc<TranscriptEvent>, airc_lib::LiveLag>,
        > = Box::pin(stream);
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
                            let item = match item {
                                Ok(event) => match AttentionEvent::classify(event) {
                                    Some(event) => Ok(event),
                                    None => continue,
                                },
                                Err(error) => Err(error),
                            };
                            if let Ok(attention) = &item {
                                let ev = &attention.event;
                                let mut s = seen.lock().unwrap_or_else(|e| e.into_inner());  // poisoned lock = read the last state, same policy as every lock in this crate
                                // Two sources may carry one event (the one feed and,
                                // until it retires, the private subscription): admit once.
                                if s.was_seen(ev.room_id.as_uuid(), ev.event_id.as_uuid()) {
                                    continue;
                                }
                                s.note(ev.room_id.as_uuid(), ev.event_id.as_uuid());
                                if let Some(message) = attention.perception.as_ref().ok().filter(|m| m.work.is_none()) {
                                    s.note_text(ev.room_id.as_uuid(), SeenRooms::fingerprint(message.peer_id, &message.render_content()));
                                }
                                drop(s);
                            }
                            let delivered = item.as_ref().ok().cloned();
                            if tx.send(item).await.is_err() {
                                return; // the conversation dropped its inbox — the pump is done
                            }
                            if let Some(event) = delivered {
                                signal_if_directed(persona, &event);
                            }
                        }
                        None => {
                            continuum_base::probe!(
                                class = "persona.inbound.pump_ended",
                                persona = %persona,
                                "the daemon stream ended under the pump — re-opening it"
                            );
                            match reopen_live_stream(&*runtime, persona, "ended", &mut reopen_attempt).await {
                                Some(next) => {
                                    stream = Box::pin(next);
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
                        continuum_base::probe!(
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
                            continuum_base::probe!(
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
                                    stream = Box::pin(next);
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
