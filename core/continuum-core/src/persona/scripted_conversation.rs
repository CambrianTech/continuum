//! `ScriptedConversation` — system-level `PersonaConversation` impl
//! that feeds a pre-baked queue of events, records every `say()` call,
//! and exposes configurable prime behavior.
//!
//! Per [[test-fixtures-are-system-primitives]]: every test, replay
//! rig, demo binary, and future tool that needs a controllable
//! conversation surface leases THIS struct. There is no
//! `#[cfg(test)]` bespoke variant — that's the cancer this primitive
//! exists to refuse.
//!
//! ## Replaces
//!
//! Pre-#1517 the substrate had three bespoke `#[cfg(test)]` impls
//! across `service_loop::tests`: `StubConversation`,
//! `UnprimedConversation`, `FailingPrimeConversation`. All three
//! collapse into builder configurations of this one type.
//!
//! ## Shape
//!
//! ```ignore
//! use crate::persona::scripted_conversation::ScriptedConversation;
//! use crate::persona::service_loop::IncomingMessage;
//!
//! // Happy path: one inbound, then stream end.
//! let mut c = ScriptedConversation::new()
//!     .with_events(vec![
//!         Ok(Some(IncomingMessage { lamport: 1, peer_id: pid, text: "hello".into() })),
//!         Ok(None),
//!     ]);
//! c.prime().await?;
//! let msg = c.next_message().await?;
//!
//! // Prime failure: substrate refuses to start the loop.
//! let mut c = ScriptedConversation::new()
//!     .with_prime_failure("simulated airc daemon unreachable");
//!
//! // Unprimed-call detection: next_message yields a typed err if
//! // the caller forgot to prime first (mirrors AircPersonaConversation).
//! let mut c = ScriptedConversation::new()
//!     .with_events(vec![Ok(Some(msg))])
//!     .require_prime_before_next_message();
//! // Calling next_message without prime() first → Err.
//! ```
//!
//! ## Doctrine
//!
//! - [[test-fixtures-are-system-primitives]]: this is THE conversation
//!   stub. Lease, never reinvent.
//! - [[no-fallbacks-ever]]: every behavior is explicit (prime success
//!   vs failure, require-prime vs not); no silent defaults.
//! - [[every-error-is-an-opportunity-to-battle-harden]]: configurable
//!   failure modes mean every regression test can lock the contract
//!   it cares about.

use crate::persona::service_loop::{IncomingMessage, PersonaConversation};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use uuid::Uuid;

/// System-level, configurable `PersonaConversation` impl. Public
/// because every test in the substrate leases it; not behind
/// `#[cfg(test)]`.
pub struct ScriptedConversation {
    high_water: u64,
    events: Mutex<VecDeque<Result<Option<IncomingMessage>, String>>>,
    /// Every reply the loop posted, WITH the room it was posted into.
    /// The room is recorded because "she answered" and "she answered
    /// the room that asked" are different claims, and only the second
    /// one is the contract — see `said_in`.
    said: Mutex<Vec<(Uuid, String)>>,
    primed: AtomicUsize,
    prime_result: Mutex<Result<(), String>>,
    /// When set, `next_message` returns Err if called before `prime`
    /// — mirrors `AircPersonaConversation`'s caller-primes contract.
    require_prime: bool,
}

impl Default for ScriptedConversation {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptedConversation {
    /// Empty conversation: no events queued, prime succeeds, no
    /// pre-prime requirement. Builder methods configure the rest.
    pub fn new() -> Self {
        Self {
            high_water: 0,
            events: Mutex::new(VecDeque::new()),
            said: Mutex::new(Vec::new()),
            primed: AtomicUsize::new(0),
            prime_result: Mutex::new(Ok(())),
            require_prime: false,
        }
    }

    /// Replace the queued events. Each entry is what `next_message`
    /// yields on its next call:
    /// - `Ok(Some(msg))` → loop processes the message
    /// - `Ok(None)` → stream ends; loop returns cleanly
    /// - `Err(_)` → transient error; loop records and continues
    ///
    /// When the queue drains, subsequent `next_message` calls yield
    /// `Ok(None)` so the loop never hangs.
    pub fn with_events(self, events: Vec<Result<Option<IncomingMessage>, String>>) -> Self {
        *self.events.lock().unwrap() = VecDeque::from(events);
        self
    }

    /// Set the pre-attach high-water mark — what `high_water_mark`
    /// returns. Defaults to 0 (no pre-attach history).
    pub fn with_high_water(mut self, hw: u64) -> Self {
        self.high_water = hw;
        self
    }

    /// Make `prime()` return Err with this reason. Used by tests that
    /// verify prime-failure short-circuits the spawn path
    /// (`spawn_persona_service` returns Err; supervisor records
    /// `BootSlotFailure`).
    pub fn with_prime_failure(self, reason: impl Into<String>) -> Self {
        *self.prime_result.lock().unwrap() = Err(reason.into());
        self
    }

    /// Reject `next_message` calls until `prime()` has been invoked at
    /// least once — mirrors `AircPersonaConversation`'s caller-primes
    /// contract per [[no-fallbacks-ever]]. Used by the regression
    /// test that locks the absence of the belt-and-suspenders prime.
    pub fn require_prime_before_next_message(mut self) -> Self {
        self.require_prime = true;
        self
    }

    /// How many times `prime()` has been called. Tests assert this is
    /// exactly 1 to verify the caller-primes contract (one prime per
    /// loop invocation, no belt-and-suspenders).
    pub fn primed_count(&self) -> usize {
        self.primed.load(Ordering::SeqCst)
    }

    /// Snapshot of every text the loop posted. Tests assert reply
    /// content + count. Room-agnostic view of the same storage as
    /// [`said_in`](Self::said_in) — not a second field.
    pub fn said(&self) -> Vec<String> {
        self.said
            .lock()
            .unwrap()
            .iter()
            .map(|(_room, text)| text.clone())
            .collect()
    }

    /// Every reply paired with the room it was posted INTO. This is
    /// the surface that can catch a persona answering the wrong
    /// audience — invisible to [`said`](Self::said), which only knows
    /// that she spoke.
    pub fn said_in(&self) -> Vec<(Uuid, String)> {
        self.said.lock().unwrap().clone()
    }
}

#[async_trait]
impl PersonaConversation for ScriptedConversation {
    async fn prime(&mut self) -> Result<(), String> {
        self.primed.fetch_add(1, Ordering::SeqCst);
        self.prime_result.lock().unwrap().clone()
    }

    async fn high_water_mark(&self, _limit: usize) -> Result<u64, String> {
        Ok(self.high_water)
    }

    async fn next_message(&mut self) -> Result<Option<IncomingMessage>, String> {
        if self.require_prime && self.primed.load(Ordering::SeqCst) == 0 {
            // Mirror AircPersonaConversation's typed-err shape. CRITICAL:
            // drain ONE event from the queue before returning Err. The
            // service loop counts this as `turns_errored` and continues;
            // if we didn't drain, the same Err would fire forever and
            // the loop would hang. Substrate-correct semantics: each
            // call consumes one queue item regardless of whether it
            // surfaces an event or an err. Per [[no-fallbacks-ever]] the
            // contract is "queue progress per call", not "queue progress
            // per success."
            let drained = self.events.lock().unwrap().pop_front();
            return match drained {
                Some(_) => Err(
                    "ScriptedConversation::next_message called before prime() — \
                     caller must invoke prime() before iterating"
                        .to_string(),
                ),
                None => Ok(None),
            };
        }
        self.events.lock().unwrap().pop_front().unwrap_or(Ok(None))
    }

    async fn say_in(&self, room_id: Uuid, text: &str) -> Result<(), String> {
        self.said.lock().unwrap().push((room_id, text.to_string()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_msg() -> IncomingMessage {
        IncomingMessage {
            lamport: 1,
            peer_id: Uuid::new_v4(),
            text: "hello".into(),
            room_id: Uuid::nil(),
        }
    }

    #[tokio::test]
    async fn default_prime_succeeds_and_counts() {
        let mut c = ScriptedConversation::new();
        assert_eq!(c.primed_count(), 0);
        c.prime().await.expect("default prime ok");
        assert_eq!(c.primed_count(), 1);
    }

    #[tokio::test]
    async fn with_prime_failure_returns_err() {
        let mut c = ScriptedConversation::new().with_prime_failure("simulated daemon unreachable");
        let err = c.prime().await.expect_err("must err");
        assert!(err.contains("simulated daemon unreachable"));
        assert_eq!(c.primed_count(), 1, "prime still counts attempts");
    }

    #[tokio::test]
    async fn events_drain_then_yield_none() {
        let mut c = ScriptedConversation::new().with_events(vec![Ok(Some(one_msg())), Ok(None)]);
        assert!(c.next_message().await.unwrap().is_some());
        assert!(c.next_message().await.unwrap().is_none());
        // Past end → still Ok(None), never hangs.
        assert!(c.next_message().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn require_prime_before_next_message_enforced() {
        // TWO events queued — first call (unprimed) drains the first
        // event AND returns Err. Second call (post-prime) returns the
        // second event. The drain-on-err semantics are load-bearing:
        // without them the service loop would re-fire the same Err
        // every iteration and never make progress.
        let mut c = ScriptedConversation::new()
            .with_events(vec![Ok(Some(one_msg())), Ok(Some(one_msg()))])
            .require_prime_before_next_message();
        let err = c.next_message().await.expect_err("unprimed err");
        assert!(err.contains("called before prime()"));
        c.prime().await.expect("prime ok");
        assert!(
            c.next_message().await.unwrap().is_some(),
            "post-prime call gets the SECOND event; first was drained by the Err path"
        );
    }

    #[tokio::test]
    // what this catches: replies are recorded in order AND against the room each
    // was actually posted into. The room half is the load-bearing one — a
    // persona answering the wrong room is invisible to `said()`, which only
    // knows that she spoke, and that blind spot is what let one-room perception
    // look like inattention for a whole evening (task #64).
    async fn say_records_in_order_and_remembers_the_room() {
        let asked_in = Uuid::new_v4();
        let elsewhere = Uuid::new_v4();
        let c = ScriptedConversation::new();
        c.say_in(asked_in, "one").await.unwrap();
        c.say_in(elsewhere, "two").await.unwrap();
        assert_eq!(c.said(), vec!["one".to_string(), "two".to_string()]);
        assert_eq!(
            c.said_in(),
            vec![
                (asked_in, "one".to_string()),
                (elsewhere, "two".to_string())
            ],
            "each reply is attributed to the room it went to, not merely counted"
        );
    }

    #[tokio::test]
    async fn high_water_mark_passes_through() {
        let c = ScriptedConversation::new().with_high_water(42);
        assert_eq!(c.high_water_mark(0).await.unwrap(), 42);
    }
}
