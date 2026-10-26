//! A turn's pre-effect mark (stop-drain S2, card 68811445): each effect is recorded on the
//! turn's obligation row before it happens, and an unconfirmed record refuses the effect.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;

use super::{mark_effect, RecordedEffect};
use crate::cognition::act_observe::{Effect, EffectMark};
use continuum_client::Connection;

/// Consecutive obligation-store failures for one persona, and until when the store is not
/// tried again (doubling, capped). Nothing SLEEPS: the loop holds a turn_ingress permit for the
/// whole turn, so a wait here would hold it (Fable on #5492). While the backoff runs, admission
/// and marks refuse at once without touching the store, and the loop's own beat paces retries.
#[derive(Default)]
pub struct Failures {
    streak: AtomicU32,
    until_ms: AtomicU64,
}

/// The longest a failure waits before refusing (the cap of the doubling backoff). A bound on
/// the wait, not a tuned rate: it stays below one generation on the fastest tier we run (median
/// generation 66-88 s healthy on the M5, measured 10-26, CLAUDE.md "The speed path"), so a
/// backoff never costs more than the turn it guards; the probe's `streak` says how long the
/// store has been failing.
// derived-or-floor: a ceiling — below one generation on the fastest tier (66-88 s median, M5 10-26); it only bounds a backoff wait.
const BACKOFF_CAP_SECS: u64 = 30;

impl Failures {
    pub fn succeeded(&self) {
        self.streak.store(0, Ordering::Relaxed);
        self.until_ms.store(0, Ordering::Relaxed);
    }

    /// `Some(remaining ms)` while a backoff is in force: refuse now, try the store later.
    pub fn backing_off(&self) -> Option<u64> {
        let remaining = self.until_ms.load(Ordering::Relaxed).saturating_sub(continuum_base::time::EpochMs::now().millis());
        (remaining > 0).then_some(remaining)
    }

    pub fn failed(&self, persona: &str, step: &'static str, turn: Uuid) {
        let streak = self.streak.fetch_add(1, Ordering::Relaxed) + 1;
        let wait = (1u64 << streak.saturating_sub(1).min(u64::BITS - 1)).min(BACKOFF_CAP_SECS);
        self.until_ms.store(continuum_base::time::EpochMs::now().millis() + wait * 1000, Ordering::Relaxed);
        continuum_base::probe!(
            class = "persona.obligation.unconfirmed",
            persona = %persona,
            step = step,
            turn = %turn,
            streak = streak as u64,
            backoff_s = wait,
            "an obligation write could not be confirmed: the turn or its effect does not run (fail closed); the store is not retried until the backoff ends"
        );
    }
}

/// The mark for one turn: its row, and the effects recorded on it so far.
pub struct ObligationMark {
    conn: Arc<Connection<crate::runtime::InProcessTransport>>,
    persona_name: String,
    turn: Uuid,
    failures: Arc<Failures>,
    /// Set only AFTER a confirmed write: (first effect's time, every effect recorded).
    recorded: Mutex<Option<(u64, Vec<RecordedEffect>)>>,
}

impl ObligationMark {
    pub fn new(conn: Arc<Connection<crate::runtime::InProcessTransport>>, persona_name: &str, turn: Uuid, failures: Arc<Failures>) -> Self {
        Self { conn, persona_name: persona_name.to_string(), turn, failures, recorded: Mutex::new(None) }
    }
}

#[async_trait]
impl EffectMark for ObligationMark {
    fn turn(&self) -> Uuid {
        self.turn
    }

    async fn before(&self, effect: &Effect) -> Result<(), String> {
        if let Some(remaining) = self.failures.backing_off() {
            return Err(format!("the obligation store failed recently; not retried for another {}s", remaining.div_ceil(1000)));
        }
        let (first_at, mut all) = self
            .recorded
            .lock()
            .unwrap_or_else(|p| p.into_inner()) // poisoned: the record is plain data, still the truth of what was confirmed
            .clone()
            .unwrap_or_else(|| (continuum_base::time::EpochMs::now().millis(), Vec::new())); // unwrap_or_else: the first effect of the turn starts its record now
        all.push(RecordedEffect::from(effect));
        match mark_effect(self.conn.as_ref(), &self.persona_name, self.turn, first_at, &all).await {
            Ok(()) => {
                // The in-memory record moves only after the store confirmed it.
                *self.recorded.lock().unwrap_or_else(|p| p.into_inner()) = Some((first_at, all)); // poisoned: plain data, overwritten whole
                self.failures.succeeded();
                Ok(())
            }
            Err(error) => {
                self.failures.failed(&self.persona_name, "mark", self.turn);
                Err(error.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Fable on #5492): a failing store answered by SLEEPING while the loop
    // holds the turn's ingress permit. A failure must start a backoff that refuses at once, and
    // a success must clear it.
    #[test]
    fn a_failure_starts_a_backoff_that_refuses_at_once_and_success_clears_it() {
        let failures = Failures::default();
        assert!(failures.backing_off().is_none(), "a fresh store is tried");
        failures.failed("p", "mark", Uuid::nil());
        let first = failures.backing_off().expect("one failure backs off"); // expect: the invariant under test
        assert!(first <= 1_000, "the first backoff is one second, not more: {first}");
        failures.failed("p", "mark", Uuid::nil());
        assert!(failures.backing_off().expect("still backing off") > 1_000, "it doubles"); // expect: the invariant under test
        failures.succeeded();
        assert!(failures.backing_off().is_none(), "a success clears the backoff");
    }
}
