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
use std::sync::Arc;

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
    async fn high_water_mark(&self, limit: usize) -> Result<u64, String> {
        let events = self
            .runtime
            .page_recent(limit)
            .await
            .map_err(|e| format!("page_recent failed: {e}"))?;
        Ok(events.iter().map(|e| e.lamport).max().unwrap_or(0))
    }

    async fn next_message(&mut self) -> Result<Option<IncomingMessage>, String> {
        // Subscribe on first call. Per the doc-comment, this is
        // intentional — the constructor must remain free so the
        // supervisor can build many of these at boot.
        if self.stream.is_none() {
            let stream = self
                .runtime
                .subscribe()
                .await
                .map_err(|e| format!("subscribe failed: {e}"))?;
            self.stream = Some(stream);
        }
        let stream = self.stream.as_mut().expect("stream initialized above");

        // Skip self / non-text inline — they're not "next messages"
        // from the loop's perspective. Yielding them with the loop
        // having to re-filter would mean the loop's outcome counter
        // over-counts skips for events the conversation already
        // knows aren't relevant.
        loop {
            match stream.next().await {
                None => return Ok(None),
                Some(Err(lag)) => {
                    // Lag is a transient — surface as Err so the loop
                    // increments turns_errored and continues. Matches
                    // the demo binary's `eprintln + continue` shape
                    // (bin/airc_chat_demo.rs:346) but typed.
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
