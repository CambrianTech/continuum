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
use airc_lib::EventStream;
use async_trait::async_trait;
use futures::StreamExt;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

/// Reconnect backoff floor — the first resubscribe after a daemon drop
/// fires fast (a daemon restart is usually sub-second).
const RECONNECT_BACKOFF_START_MS: u64 = 250;
/// Reconnect backoff ceiling — a permanently-gone daemon is retried at
/// this cadence forever (loud per attempt), so the persona re-homes the
/// instant its room returns instead of going dead. Cf.
/// [[grid-node-resilience]]: nodes drop for mundane reasons; heal.
const RECONNECT_BACKOFF_MAX_MS: u64 = 5_000;
/// How many recent transcript events to page on reconnect to recover
/// anything that landed during the gap. Matches the boot-time
/// `page_recent_limit` so reconnect catch-up has the same horizon as
/// first-attach catch-up.
const RECONNECT_CATCHUP_LIMIT: usize = 50;

/// Wraps an [`AircCitizen`] and projects it onto the substrate's
/// [`PersonaConversation`] contract. Owns the airc subscribe stream
/// across calls so successive `next_message` invocations are a
/// continuation (not a fresh resubscription that would drop in-flight
/// events).
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
    stream: Option<EventStream>,
    /// Highest lamport already yielded to the service loop (live or via
    /// catch-up). After a reconnect we page recent transcript and only
    /// replay events strictly above this — so a daemon drop doesn't
    /// re-deliver messages the persona already cognized, and doesn't
    /// lose the ones that landed during the gap.
    last_lamport: u64,
    /// Messages recovered by `page_recent` after a reconnect, queued in
    /// lamport order to drain before pulling from the live stream again.
    backlog: VecDeque<IncomingMessage>,
}

impl AircPersonaConversation {
    /// Construct without contacting the daemon. The subscribe stream
    /// is built on first `next_message`; until then this is free.
    pub fn new(runtime: Arc<dyn AircCitizen>) -> Self {
        let own_peer_id = runtime.peer_id();
        Self {
            runtime,
            own_peer_id,
            stream: None,
            last_lamport: 0,
            backlog: VecDeque::new(),
        }
    }

    /// Reconnect after the airc daemon dropped the subscribe stream
    /// (socket close → live stream yields `None`). Resubscribes with
    /// bounded exponential backoff — loud per attempt — then pages
    /// recent transcript to recover anything that landed during the
    /// gap, queueing it (lamport-ordered, deduped against
    /// `last_lamport`) into `backlog`.
    ///
    /// Per [[fallbacks-are-illegal-fail-loud]] this is NOT a silent
    /// degradation: it heals an expected transport hiccup (cf.
    /// [[grid-node-resilience]] — daemons restart on logout, redeploy,
    /// OOM) while naming every retry. It only returns once a fresh
    /// stream is in hand, so the caller's next pull resumes cleanly.
    async fn reconnect_and_catch_up(&mut self) {
        tracing::warn!(
            persona = %self.own_peer_id,
            last_lamport = self.last_lamport,
            "airc subscribe stream ended (daemon drop) — reconnecting"
        );
        let mut backoff = Duration::from_millis(RECONNECT_BACKOFF_START_MS);
        let mut attempt: u32 = 0;
        let new_stream = loop {
            attempt += 1;
            match self.runtime.subscribe().await {
                Ok(s) => break s,
                Err(e) => {
                    tracing::warn!(
                        persona = %self.own_peer_id,
                        attempt,
                        error = %e,
                        backoff_ms = backoff.as_millis() as u64,
                        "airc resubscribe failed — retrying after backoff"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff =
                        (backoff * 2).min(Duration::from_millis(RECONNECT_BACKOFF_MAX_MS));
                }
            }
        };
        self.stream = Some(new_stream);

        // Recover anything that landed during the gap. Failure here is
        // logged (not fatal) — we resume live; the only cost is the
        // gap's messages aren't replayed, which we name explicitly.
        let mut recovered: Vec<IncomingMessage> = Vec::new();
        match self.runtime.page_recent(RECONNECT_CATCHUP_LIMIT).await {
            Ok(events) => {
                for e in events {
                    if let Some(m) = keep_catchup(
                        e.peer_id.as_uuid(),
                        e.lamport,
                        e.body.as_ref().and_then(|b| b.as_text()),
                        self.own_peer_id,
                        self.last_lamport,
                    ) {
                        recovered.push(m);
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    persona = %self.own_peer_id,
                    error = %e,
                    "airc reconnect: page_recent catch-up failed — \
                     resuming live (gap messages may be unrecovered)"
                );
            }
        }
        recovered.sort_by_key(|m| m.lamport);
        for m in &recovered {
            self.last_lamport = self.last_lamport.max(m.lamport);
        }
        let recovered_n = recovered.len();
        self.backlog.extend(recovered);
        tracing::info!(
            persona = %self.own_peer_id,
            attempt,
            recovered = recovered_n,
            "airc stream reconnected — persona is listening again"
        );
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
            .subscribe()
            .await
            .map_err(|e| format!("subscribe failed: {e}"))?;
        self.stream = Some(stream);
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
        if self.stream.is_none() {
            return Err("AircPersonaConversation::next_message called before prime() — \
                 caller must invoke prime() before iterating (serve_persona_loop \
                 does this automatically at boot)"
                .to_string());
        }

        // Drain any catch-up recovered after a prior reconnect before
        // pulling from the live stream — preserves lamport order across
        // the gap so the loop sees a continuous timeline.
        if let Some(m) = self.backlog.pop_front() {
            return Ok(Some(m));
        }

        // Skip self / non-text inline — they're not "next messages"
        // from the loop's perspective. Yielding them with the loop
        // having to re-filter would mean the loop's outcome counter
        // over-counts skips for events the conversation already
        // knows aren't relevant.
        loop {
            // Bind the stream poll into an owned value FIRST so the
            // `&mut self.stream` borrow ends before any `&mut self`
            // call (reconnect) on the `None` arm. `stream.is_none()`
            // was ruled out above; reconnect always leaves it `Some`.
            let next = match self.stream.as_mut() {
                Some(s) => s.next().await,
                None => unreachable!("stream primed above and reconnect re-arms it"),
            };
            match next {
                None => {
                    // Daemon dropped the stream. Heal in place (loud,
                    // bounded backoff) instead of returning Ok(None) —
                    // which the service loop reads as Stop, killing the
                    // persona permanently. Cf. [[grid-node-resilience]].
                    self.reconnect_and_catch_up().await;
                    if let Some(m) = self.backlog.pop_front() {
                        return Ok(Some(m));
                    }
                    // Nothing to replay — resume polling the fresh stream.
                    continue;
                }
                Some(Err(lag)) => {
                    // Lag is a transient — surface as Err so the loop
                    // increments turns_errored and continues. The typed
                    // Err shape lets the loop log + resume per
                    // `[[no-fallbacks-ever]]` without silently masking
                    // the gap.
                    return Err(format!("live stream lag: {lag}"));
                }
                Some(Ok(event)) => {
                    if event.peer_id.as_uuid() == self.own_peer_id {
                        continue;
                    }
                    let Some(body) = event.body.as_ref() else {
                        continue;
                    };
                    let Some(text) = body.as_text() else {
                        continue;
                    };
                    self.last_lamport = self.last_lamport.max(event.lamport);
                    return Ok(Some(IncomingMessage {
                        lamport: event.lamport,
                        peer_id: event.peer_id.as_uuid(),
                        text: text.to_string(),
                    }));
                }
            }
        }
    }

    async fn say(&self, text: &str) -> Result<(), String> {
        self.runtime
            .say(text)
            .await
            .map(|_event_id| ())
            .map_err(|e| format!("say failed: {e}"))
    }
}

/// Decide whether a transcript event recovered via `page_recent` after
/// a reconnect should be replayed to the service loop. Pure over
/// primitives so the gate logic (the bug-prone part — lamport ordering +
/// self-echo filter) is unit-testable without standing up a live airc
/// `EventStream` or constructing a full `TranscriptEvent`.
///
/// Mirrors the live-stream filter in `next_message` exactly (skip self,
/// skip non-text), with one addition: `event_lamport > after_lamport`,
/// so catch-up never re-delivers a message already yielded before the
/// drop. The service loop's own `high_water` gate is a second line of
/// defense against overlap; this keeps the conversation honest too.
fn keep_catchup(
    event_peer: uuid::Uuid,
    event_lamport: u64,
    text: Option<&str>,
    own_peer: uuid::Uuid,
    after_lamport: u64,
) -> Option<IncomingMessage> {
    if event_lamport <= after_lamport {
        return None; // already seen before the drop
    }
    if event_peer == own_peer {
        return None; // self-echo
    }
    let text = text?; // non-text envelope (attachment / control)
    Some(IncomingMessage {
        lamport: event_lamport,
        peer_id: event_peer,
        text: text.to_string(),
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

    // what this catches: the reconnect catch-up gate must (1) drop
    // events at-or-below the last-yielded lamport so a daemon drop
    // doesn't re-deliver already-cognized messages, (2) drop the
    // persona's own echoes, (3) drop non-text envelopes, and (4) keep
    // genuinely-new peer text recovered during the gap. A regression in
    // any arm either floods the persona with duplicates on every
    // reconnect or silently swallows messages that landed during the
    // outage — both break "dependable over airc". Cf.
    // [[grid-node-resilience]], [[fallbacks-are-illegal-fail-loud]].
    #[test]
    fn catchup_gate_dedups_self_and_already_seen() {
        let me = uuid::Uuid::new_v4();
        let peer = uuid::Uuid::new_v4();
        let after = 100u64;

        // already seen (== high-water): dropped
        assert!(keep_catchup(peer, 100, Some("old"), me, after).is_none());
        // already seen (< high-water): dropped
        assert!(keep_catchup(peer, 42, Some("older"), me, after).is_none());
        // own echo above the water line: dropped
        assert!(keep_catchup(me, 200, Some("my own turn"), me, after).is_none());
        // non-text envelope above the water line: dropped
        assert!(keep_catchup(peer, 201, None, me, after).is_none());
        // genuinely new peer text during the gap: kept, fields intact
        let kept = keep_catchup(peer, 150, Some("hello after reconnect"), me, after)
            .expect("new peer text above the water line must be replayed");
        assert_eq!(kept.lamport, 150);
        assert_eq!(kept.peer_id, peer);
        assert_eq!(kept.text, "hello after reconnect");
    }
}
