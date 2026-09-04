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
use futures::StreamExt;
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

pub struct AircPersonaConversation {
    runtime: Arc<dyn AircCitizen>,
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
    stream: Option<FilteredEventStream>,
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
            own_peer_id,
            stream: None,
            membership_epoch,
            last_lamport: 0,
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
        if self.stream.is_some() {
            return Ok(());
        }
        let stream = self
            .runtime
            .subscribe_all_rooms()
            .await
            .map_err(|e| format!("subscribe failed: {e}"))?;
        // #146 diagnostic: confirm the CHAT subscribe stream actually opened for
        // this persona. Post-reboot the personas were room-deaf (0 perceptual
        // decodes) while the core-positron raw-attach path received fine — this
        // pins whether prime() even ran per persona.
        tracing::info!(
            persona = %self.own_peer_id,
            probe_class = "persona.inbound.subscribe_opened",
            "persona chat subscribe stream opened (#146)"
        );
        self.stream = Some(stream);
        // Seed the rejoin-replay watermark at the CURRENT transcript head, so the
        // first runtime room-join can never replay pre-subscribe history as fresh
        // perception (the #131 "room starts at join" rule, preserved under replay).
        self.last_lamport = self.high_water_mark(64).await.unwrap_or(0);
        Ok(())
    }

    async fn high_water_mark(&self, limit: usize) -> Result<u64, String> {
        let events = self
            .runtime
            .page_recent(limit)
            .await
            .map_err(|e| format!("page_recent failed: {e}"))?;
        Ok(events.iter().map(|e| e.lamport).max().unwrap_or(0))
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
        if self.stream.is_none() {
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
            if let Some(replayed) = self.rejoin_backlog.pop_front() {
                return Ok(Some(replayed));
            }
            // Wait on EITHER the next event OR a membership-epoch move. The
            // epoch branch is the P0 20b44763 fix: a room joined at runtime
            // (benchmark dispatch) never enters the existing stream's channel
            // snapshot, so on a membership change we re-open the stream with
            // the enlarged set. A closed epoch channel (stub citizens whose
            // default `membership_epoch` drops its sender) parks on
            // `pending()` — closed means "membership never changes", never
            // an event, or stubs would busy-loop on `changed() == Err`.
            let polled = {
                let stream = self
                    .stream
                    .as_mut()
                    .expect("stream checked Some at next_message entry");
                let epoch = &mut self.membership_epoch;
                tokio::select! {
                    ev = stream.next() => Polled::Event(ev),
                    _ = async {
                        if epoch.changed().await.is_err() {
                            std::future::pending::<()>().await;
                        }
                    } => Polled::Membership,
                    // QUIET WATCHDOG (2026-09-04): live streams went silent ~2 min after
                    // prime — 12 citizens received 172 events in the first two minutes,
                    // 2 citizens 5 events in the next two, and an operator line at +12 min
                    // reached nobody — with no lag error ever surfacing here. The daemon
                    // marks a slow subscriber lagged and its "resume from the sink" never
                    // delivers. So: when nothing has arrived for a window, ask the durable
                    // store whether the room moved on; if it did, the stream is dead —
                    // re-open it and replay the gap, exactly as a membership change does.
                    _ = tokio::time::sleep(QUIET_WATCHDOG) => Polled::Quiet,
                }
            };
            let reason: &'static str = match polled {
                Polled::Event(ev) => {
                    match ev {
                        None => {
                            tracing::error!(
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
                    }
                }
                Polled::Membership => "room membership changed at runtime — re-opening the subscribe \
                     stream with the enlarged channel snapshot (P0 20b44763)",
                Polled::Quiet => {
                    let hwm = self.high_water_mark(32).await.unwrap_or(self.last_lamport); // unwrap_or: a failed page reads as "nothing new" — the next tick asks again
                    if hwm <= self.last_lamport {
                        continue; // genuinely quiet: the room has not moved on
                    }
                    crate::probe!(
                        class = "persona.inbound.resubscribed_quiet",
                        persona = %self.own_peer_id,
                        last_lamport = self.last_lamport,
                        high_water = hwm,
                        "the live stream went silent while the room moved on — re-opening it \
                         and replaying the gap (the daemon's lag resume never delivered)"
                    );
                    "quiet stream behind the durable high-water mark — re-opened"
                }
            };
            {
                tracing::info!(
                    persona = %self.own_peer_id,
                    probe_class = "persona.inbound.resubscribed",
                    reason,
                );
                let stream = self
                    .runtime
                    .subscribe_all_rooms()
                    .await
                    .map_err(|e| format!("resubscribe after membership change failed: {e}"))?;
                self.stream = Some(stream);
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
                match self.runtime.page_recent(32).await {
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
                            self.last_lamport = event.lamport;
                            crate::modules::work::bridge_wire_work_event(event).await;
                            let Ok(message) = perceptual_from_event(event) else {
                                continue;
                            };
                            if message.peer_id == self.own_peer_id {
                                continue;
                            }
                            replayed += 1;
                            self.rejoin_backlog.push_back(message);
                        }
                        tracing::info!(
                            persona = %self.own_peer_id,
                            scanned,
                            replayed,
                            watermark = self.last_lamport,
                            probe_class = "persona.inbound.rejoin_replayed",
                            "membership-change reopen replayed the gap — room turns \
                             published between join and reopen are now perceivable \
                             instead of live-tail-lost"
                        );
                    }
                    Err(e) => tracing::warn!(
                        persona = %self.own_peer_id,
                        error = %e,
                        "rejoin replay page failed — events published between join \
                         and reopen stay unheard until something else surfaces them"
                    ),
                }
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
    use super::*;
    use crate::persona::airc_citizen::StubAircCitizen;

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
            headers.insert(airc_lib::HEADER_HEARTBEAT_KIND.to_string(), "alive".to_string());
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
    /// One live event through the door: heartbeat/chunk guards, the raw probe,
    /// the work-bridge, the decoder, the self-filter, the lamport advance.
    /// `Some(msg)` = a room turn to serve; `None` = consumed, keep polling.
    async fn admit_event(&mut self, event: std::sync::Arc<TranscriptEvent>) -> Option<IncomingMessage> {

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
        tracing::info!(
            persona = %self.own_peer_id,
            from_peer = %event.peer_id,
            body_kind,
            probe_class = "persona.inbound.raw_event",
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
                    tracing::warn!(
                        persona = %self.own_peer_id,
                        from_peer = %event.peer_id,
                        body_kind,
                        event_kind,
                        body_preview,
                        reason,
                        probe_class = "persona.inbound.filtered_non_turn",
                        "message-shaped event FAILED to decode — a peer may be structurally unheard (#177)"
                    );
                } else {
                    tracing::debug!(
                        persona = %self.own_peer_id,
                        from_peer = %event.peer_id,
                        body_kind,
                        event_kind,
                        body_preview,
                        reason,
                        probe_class = "persona.inbound.filtered_non_turn",
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
            tracing::debug!(
                persona = %self.own_peer_id,
                from_peer = %event.peer_id,
                text_len = message.text.len(),
                probe_class = "persona.inbound.skipped_self_authored",
                "skipped a message this persona is recorded as having said — \
                 if it was ADDRESSED to her, whoever sent it authored through \
                 her identity and she cannot hear it"
            );
            return None;
        }
        Some(message)
    }
}
