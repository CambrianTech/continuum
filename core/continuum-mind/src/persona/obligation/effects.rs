//! A turn's pre-effect mark (stop-drain S2, card 68811445): each effect is recorded on the
//! turn's obligation row before it happens, and an unconfirmed record refuses the effect.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;

use super::{mark_effect, RecordedEffect};
use crate::cognition::act_observe::{Effect, EffectMark};
use continuum_client::Connection;

/// Consecutive obligation-store failures for one persona. A failure waits before it answers
/// (doubling, capped), so a persistently failing store slows her loudly instead of letting a
/// retry spin; any success resets it.
#[derive(Default)]
pub struct Failures(AtomicU32);

/// The longest a failure waits before refusing (the cap of the doubling backoff). A bound on
/// the wait, not a tuned rate: it stays below one generation on the fastest tier we run (median
/// generation 66-88 s healthy on the M5, measured 10-26, CLAUDE.md "The speed path"), so a
/// backoff never costs more than the turn it guards; the probe's `streak` says how long the
/// store has been failing.
// derived-or-floor: a ceiling — below one generation on the fastest tier (66-88 s median, M5 10-26); it only bounds a backoff wait.
const BACKOFF_CAP_SECS: u64 = 30;

impl Failures {
    pub fn succeeded(&self) {
        self.0.store(0, Ordering::Relaxed);
    }

    pub async fn failed(&self, persona: &str, step: &'static str, turn: Uuid) {
        let streak = self.0.fetch_add(1, Ordering::Relaxed) + 1;
        let wait = (1u64 << streak.saturating_sub(1).min(u64::BITS - 1)).min(BACKOFF_CAP_SECS);
        continuum_base::probe!(
            class = "persona.obligation.unconfirmed",
            persona = %persona,
            step = step,
            turn = %turn,
            streak = streak as u64,
            backoff_s = wait,
            "an obligation write could not be confirmed: the turn or its effect does not run (fail closed)"
        );
        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
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
                self.failures.failed(&self.persona_name, "mark", self.turn).await;
                Err(error.to_string())
            }
        }
    }
}
