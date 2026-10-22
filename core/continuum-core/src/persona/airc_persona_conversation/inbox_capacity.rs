use super::*;

/// In-process inbox depth between the pump and the loop. A turn in progress
/// can leave the loop away for minutes; 8k events is hours of a busy room.
pub(crate) const INBOX_CAPACITY: usize = 8_192;

/// The live stream is dead when a catch-up tick admits events from the store
/// while no live frame arrived since the previous tick. The first tick after
/// (re)open is exempt: it legitimately admits the backlog.
pub(super) fn live_stream_missed(live_since_tick: u64, admitted: usize, first_tick: bool) -> bool {
    !first_tick && live_since_tick == 0 && admitted > 0 && admitted != usize::MAX
}

/// Re-subscribe with a bounded backoff (1 s doubling to 30 s). `None` only
/// when the citizen's subscribe is refused for good.
pub(super) async fn reopen_live_stream(
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
                continuum_base::probe!(
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
                continuum_base::probe!(
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
                    continuum_base::probe!(
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

impl AircPersonaConversation {
    pub(super) async fn next_message_inner(
        &mut self,
        wait: bool,
        take_retained: bool,
    ) -> Result<Option<IncomingMessage>, String> {
        let mut ready_scan_remaining = CATCH_UP_PAGE;
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
            if !wait
                && !self.refresh_pending
                && !matches!(self.membership_epoch.has_changed(), Ok(true))
            // Closed receiver means fixed membership, matching the parked changed() branch below.
            {
                if ready_scan_remaining == 0 {
                    // A scheduling slice is not an empty inbox. Yield cooperatively;
                    // the caller's deadline may cancel us with ready_event retained.
                    tokio::task::yield_now().await;
                    ready_scan_remaining = CATCH_UP_PAGE;
                }
            }
            // Rejoin-replayed turns first — they are OLDER than anything the live
            // stream will yield, and ordering is what keeps an addressed kickoff
            // ahead of the chatter that follows it.
            if !self.refresh_pending && !matches!(self.membership_epoch.has_changed(), Ok(true)) {
                // A closed fixed-membership receiver cannot announce another refresh.
                if take_retained {
                    if let Some(message) = self.perceived_backlog.pop_front() {
                        return Ok(Some(Arc::unwrap_or_clone(message)));
                    }
                }
                if let Some(event) = self.ready_event.clone() {
                    let message = self.admit_event(event).await;
                    self.ready_event = None;
                    // Charge an examined event, not the earlier pass that only
                    // retained it across awaits. Cancellation retains ownership.
                    if !wait {
                        ready_scan_remaining -= 1;
                    }
                    if message.is_some() {
                        return Ok(message);
                    }
                    continue;
                }
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
                    _ = std::future::ready(()), if !wait => return Ok(None),
                }
            };
            let reason: &'static str = match polled {
                Polled::Event(ev) => {
                    match ev {
                        None => {
                            continuum_base::probe!(
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
                            // Own the event before any await in admission. A cancelled
                            // ready-drain resumes this same event on its next call.
                            self.ready_event = Some(event);
                            continue;
                        }
                    }
                }
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
                // The attach set itself: which channels this persona's live stream now
                // carries. Without it, "she never hears another peer in her project room"
                // (Kimi, 2026-10-04: a durable chat in 700663d5 reached the core's chat
                // projector live and never her raw_event) could not be split into "not in
                // her set" vs "in her set, not delivered".
                let attached = self.rooms.as_deref().unwrap_or_default(); // unwrap_or_default: refresh_membership just set it; none = an empty set, reported as 0
                continuum_base::probe!(
                    class = "persona.inbound.resubscribed",
                    persona = %self.own_peer_id,
                    reason = reason,
                    attached_rooms = attached.len() as u64,
                    attached = %attached.iter().map(|r| r.to_string()).collect::<Vec<_>>().join(","),
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
                // rules as the live path apply. Card-state events dedup by event
                // id; reviewed credit retains its durable destination identity.
                match self.page_subscribed_rooms(32).await {
                    Ok(events) => {
                        let scanned = events.len();
                        let mut replayed = 0usize;
                        let mut events = events;
                        events.sort_by_key(|e| e.lamport);
                        for event in &events {
                            if event.lamport <= self.last_lamport
                                || crate::routing::airc_realtime_wire::is_stream_chunk(event)
                            {
                                continue;
                            }
                            crate::modules::work::bridge_wire_work_event(
                                event,
                                self.runtime.peer_id(),
                            )
                            .await;
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
                        continuum_base::probe!(
                            class = "persona.inbound.rejoin_replayed",
                            persona = %self.own_peer_id,
                            scanned,
                            replayed,
                            watermark = self.last_lamport,
                            "membership refresh paged subscribed rooms and queued turns \
                             admitted by the existing replay watermark"
                        );
                    }
                    Err(e) => continuum_base::probe!(
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
}

#[async_trait]
impl PersonaConversation for AircPersonaConversation {
    fn take_wakes(
        &mut self,
    ) -> Option<tokio::sync::mpsc::Receiver<crate::persona::perception_region::Wake>> {
        self.mind.as_mut().and_then(|m| m.wakes.take())
    }

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
        if self.initial_watermark.is_some() {
            return Ok(());
        }
        let active = self.refresh_membership().await?;
        // Resolve readiness once, before the host reports an attached mind.
        // Never replace an unreadable cutoff with zero or leave a pump behind.
        let cutoff = match self.high_water_mark(64).await {
            Ok(cutoff) => cutoff,
            Err(error) => {
                self.stop_stream();
                self.rooms = None;
                return Err(format!("initial conversation watermark failed: {error}"));
            }
        };
        self.last_lamport = cutoff;
        self.initial_watermark = Some(cutoff);
        continuum_base::probe!(
            class = "persona.inbound.primed",
            persona = %self.own_peer_id,
            subscribed = active,
            watermark = cutoff,
            "persona conversation primed with a verified attach cutoff"
        );
        Ok(())
    }

    async fn initial_water_mark(&self, _limit: usize) -> Result<u64, String> {
        self.initial_watermark.ok_or_else(||
            "conversation initial watermark requested before successful prime()".to_string())
    }

    async fn high_water_mark(&self, limit: usize) -> Result<u64, String> {
        let events = self.page_subscribed_rooms(limit).await?;
        Ok(events.iter().map(|e| e.lamport).max().unwrap_or(0)) // unwrap_or: an empty page has no lamport; 0 = never read
    }

    async fn next_message(&mut self) -> Result<Option<IncomingMessage>, String> {
        self.next_message_inner(true, true).await
    }

    async fn perceive_ready(
        &mut self,
    ) -> Result<&std::collections::VecDeque<Arc<IncomingMessage>>, String> {
        for _ in 0..CATCH_UP_PAGE {
            if self.perceived_backlog.len() >= INBOX_CAPACITY {
                return Err(
                    "retained room attention reached inbox capacity; input remains queued"
                        .to_string(),
                );
            }
            match self.next_message_inner(false, false).await? {
                Some(message) => self.perceived_backlog.push_back(Arc::new(message)),
                None => break,
            }
        }
        Ok(&self.perceived_backlog)
    }

    async fn say_in(&self, room_id: Uuid, text: &str) -> Result<(), String> {
        // Speech in her mind room reaches its only member, her (PRIVACY-OF-THOUGHT.md §4):
        // it is never published, so no airc channel (not even one named by the room's
        // uuid) ever carries it. Saying it to the room is publishing; that is her act.
        let me = self.own_peer_id;
        if crate::persona::mind_room::is_private_room(me, room_id) {
            crate::persona::mind_room::note_withheld(me, "speech");
            return Ok(());
        }
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
pub(super) fn perceptual_from_event(event: &TranscriptEvent) -> Result<IncomingMessage, &'static str> {
    // Both on-wire room-turn shapes (say() text + chat_transcript envelope) and
    // all three named skip reasons live in the ONE decoder `room_turn_from_event`
    // (realtime_wire) — shared with the digest element and the positron
    // projection. This wrapper only adds the transcript's lamport.
    let turn = crate::persona::room_event::room_content_from_event(event)?;
    // Work presence remains in the room's durable state/RAG projection. It is
    // not a new utterance to append after every action: doing that bypassed
    // AircRagSource's existing presence folding and filled active turns with
    // other residents' work broadcasts. Media-bearing messages remain input.
    if turn.media.is_empty()
        && crate::persona::presence_glyph::is_presence_line(&turn.text)
    {
        return Err("work_presence");
    }
    Ok(IncomingMessage {
    work: None,
        media: turn.media,
        event_id: event.event_id.as_uuid(),
        lamport: event.lamport,
        peer_id: turn.sender,
        text: turn.text,
        // The transport room is the turn's context (A.6) — without it the
        // service loop bound operator/CLI turns to a nil room and every
        // room-scoped source abstained.
        room_id: event.room_id.as_uuid(),
    })
}

impl AircPersonaConversation {
    /// The newest lamport across the current subscription snapshot (two events
    /// per room). An empty snapshot has no tip; a failed read counts as not moved.
    pub(super) async fn rooms_high_water_mark(&self) -> u64 {
        self.high_water_mark(2).await.unwrap_or(self.last_lamport) // unwrap_or: a failed page reads as not moved
    }

    /// One classified event through the async bridge, diagnostics and self-filter.
    /// The caller retains its envelope across cancellation until this completes.
    /// `Some(msg)` = a room turn to serve; `None` = consumed, keep polling.
    pub(super) async fn admit_event(
        &mut self,
        attention: AttentionEvent,
    ) -> Option<IncomingMessage> {
        let event = attention.event;
        // #146 diagnostic: every classified envelope reaching attention. Absence
        // is not evidence of a delivery gap: a healthy stream may contain only
        // non-perceptual frames filtered by the shared admission boundary.
        // Retained decode faults and self-filter outcomes remain observable here.
        let body_kind = match event.body.as_ref() {
            None => "none",
            Some(b) if b.as_text().is_some() => "text",
            Some(_) => "json",
        };
        continuum_base::probe!(
            class = "persona.inbound.raw_event",
            persona = %self.own_peer_id,
            from_peer = %event.peer_id,
            body_kind,
            "classified attention envelope reached consumer (#146)"
        );
        // Card-state transitions bridge onto the internal bus HERE —
        // the persona subscribe streams are the only channel-complete
        // receiver this core has (the daemon attach covers ONE room),
        // and this runs BEFORE the perceptual filter and the self-skip
        // so a citizen's own `work/state` echo counts. State events dedup by
        // event id; reviewed-credit delivery selects the publisher observer
        // and retains its durable retry identity.
        crate::modules::work::bridge_wire_work_event(&event, self.runtime.peer_id()).await;
        self.last_lamport = self.last_lamport.max(event.lamport);
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
        let message = match attention.perception {
            Ok(message) => message,
            Err(reason) => {
                continuum_base::probe!(
                    class = "persona.inbound.filtered_non_turn",
                    persona = %self.own_peer_id, from_peer = %event.peer_id,
                    body_kind, event_kind, body_preview, reason,
                    "attention event FAILED to decode; source envelope retained through admission"
                );
                return None;
            }
        };
        if let Some(work) = &message.work {
            continuum_base::probe!(
                class = "persona.inbound.work_fact",
                persona = %self.own_peer_id, from_peer = %event.peer_id,
                room = %event.room_id, work = ?work,
                "a typed work fact admitted as perception"
            );
        }
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
            continuum_base::probe!(
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
        Some(Arc::unwrap_or_clone(message))
    }
}
