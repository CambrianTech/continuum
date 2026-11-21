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
}



/// Event ids remembered per room to tell "seen" from "dropped" (bounded).
pub(super) const SEEN_RING: usize = 128;

/// Her perception: booted from her durable state, then owned by her senses worker on the core's
/// one feed once her name is known (object model 5.7). Dropping it takes her off the feed.
pub(super) struct MindFeed {
    /// Her region, until her senses take it.
    booted: Option<crate::persona::perception_region::PerceptionRegion>,
    pub(super) senses: Option<crate::persona::senses::Resident>,
}

impl MindFeed {
    pub(super) fn boot(persona: Uuid) -> Option<Self> {
        let Some(dir) = crate::persona::senses::mind_dir(persona) else {
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
        Some(Self { booted: Some(region), senses: None })
    }
}


/// Per room: a bounded ring of the EVENT IDS forwarded, so an event the rejoin replay and
/// the feed both carry is admitted once. Neither lamport nor "max seen" can stand in for
/// this: a lamport is the publisher's logical clock, so a quiet human's line carries a
/// smaller lamport than the busy citizens' receipts around it and read as "old"
/// (2026-09-04: zero admissions across three builds).
#[derive(Default)]
pub(super) struct SeenRooms {
    pub(super) seen: std::collections::HashMap<Uuid, std::collections::VecDeque<Uuid>>,
}

impl SeenRooms {
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
        let mind = self.mind.as_mut()?;
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
        if mind.senses.is_none() {
            if let Some(region) = mind.booted.take() {
                match crate::persona::senses::start(persona, &agent_name, region) {
                    Ok(senses) => mind.senses = Some(senses),
                    Err(taken) => {
                        continuum_base::probe!(class = "mind.feed.unbooted", persona = %persona, topic = taken.0.0, "her mind's calling topic already has a writer; her senses did not start");
                        return None;
                    }
                }
            }
        }
        let senses = mind.senses.as_ref()?.handle().clone();
        let (tx, rx) = tokio::sync::mpsc::channel(INBOX_CAPACITY);
        if !senses.tell(crate::persona::senses::Sensed::Admit(tx)).await {
            return None;
        }
        let mut named = Vec::with_capacity(rooms.len());
        for room in rooms {
            let name = crate::persona::airc_citizen::room_name_by_id(*room)
                .await
                .unwrap_or_else(|| room.to_string()); // unwrap_or: an unnamed room is perceived under its id, never dropped
            named.push((*room, name));
        }
        if !senses.tell(crate::persona::senses::Sensed::Joined(named)).await {
            return None;
        }
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

    /// Rebind her perception to its durable room set: her senses learn the rooms, the core
    /// attaches any new one, and her inbox is fed from the one feed alone (object model 5.7).
    /// An empty membership parks here. The existing epoch is also how this state resumes.
    pub(super) async fn refresh_membership(&mut self) -> Result<bool, String> {
        let rooms = self
            .runtime
            .subscribed_rooms()
            .await
            .map_err(|e| format!("subscription set read failed: {e}"))?;
        let active = !rooms.is_empty();
        if active {
            // THE ONE FEED: the core's single attach per room, with its persisted cursor and
            // gap replay, is the only path into her inbox. The private subscription that ran
            // beside it, and the 60 s store catch-up and timed re-open that patched that
            // subscription's silent deaths, retired with this (Cormac, 2026-10-04: "retire
            // together once this path is proven live").
            match self.feed_events(&rooms).await {
                Some(feed) => self.install_stream(feed),
                None => self.stop_stream(),
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

    /// Install her feed behind the PUMP: a task that forwards each event into a bounded
    /// in-process inbox as it arrives. Backpressure, if any, lands here (in-process, where a
    /// turn in progress is the only slow consumer), never on the one feed.
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
        let seen = std::sync::Arc::clone(&self.seen);
        self.pump = Some(tokio::spawn(async move {
            use futures::StreamExt as _;
            // The pump is PERCEPTION's own task, so a deliberation loop minutes inside a
            // turn never holds the feed back.
            while let Some(item) = stream.next().await {
                let item = match item {
                    Ok(event) => match AttentionEvent::classify(event) {
                        Some(event) => Ok(event),
                        None => continue,
                    },
                    Err(error) => Err(error),
                };
                if let Ok(attention) = &item {
                    let ev = &attention.event;
                    let mut s = seen.lock().unwrap_or_else(|e| e.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
                    // The rejoin replay and the feed may carry one event: admit once.
                    if s.was_seen(ev.room_id.as_uuid(), ev.event_id.as_uuid()) {
                        continue;
                    }
                    s.note(ev.room_id.as_uuid(), ev.event_id.as_uuid());
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
            // Her feed ends only when her senses stop (her conversation is ending).
            continuum_base::probe!(class = "persona.inbound.pump_ended", persona = %persona, "her feed ended under the pump: her senses stopped");
        }));
        self.inbox = Some(rx);
    }
}
