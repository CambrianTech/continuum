//! The owner that settles a decided credit the training trigger has not accepted yet, whether
//! or not she acts again (card 99b2773a).
//!
//! The trigger refuses a submit while it recovers its buckets or while a bucket is held at its
//! bound, and its holds lift as time passes (a trial settling, a timing window), not at an
//! event. Her next staged turn, a review event or her registration are not owners: an idle
//! citizen has none, and registration skips cards she no longer holds. So this region, ticked
//! for each resident by the existing [`crate::persona::substrate_governor::SubstrateGovernor`],
//! reads her decided credits a bounded page at a time, in id order from where it stopped, and
//! settles each one still owed an acceptance from its binding and recorded decision
//! ([`reviewed::retry_decided`]). It backs off by the governor's own ladder while one waits and
//! sleeps when none does; a sleeping pair is re-checked every 30 passes (`CadenceTable`'s
//! floor, one pass a second), so a decision persisted later is reached within that.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use continuum_client::{Connection, Transport};
use uuid::Uuid;

use crate::persona::training_producer::reviewed;
use crate::runtime::{
    BrainRegion, CadenceHint, ComputeClass, MemoryClass, Orientation, PressureProfile, PressureSignalKind, RegionContext,
    RegionId, TickOutcome,
};

/// Decision ids read per tick. The page bounds one tick's store reads; the cursor reaches every
/// decision in turn.
const PAGE: usize = 8;

/// One tick's working budget: half the governor's own bound on a region tick, so a slow settle
/// is cut here, with its progress kept, before the governor cancels the whole tick.
fn tick_budget() -> std::time::Duration {
    crate::persona::substrate_governor::REGION_TICK_TIMEOUT / 2
}

/// Who a persona is to this region: her name (the store's handle) and her store connection, as
/// her own caller identity. `None` when she is not resident here.
pub type Resolve<T> = Arc<dyn Fn(Uuid) -> Option<(String, Connection<T>)> + Send + Sync>;

pub struct CreditAcceptanceRegion<T: Transport + Send + Sync + 'static> {
    resolve: Resolve<T>,
    /// Where each persona's scan resumes: the last decision id attempted.
    cursors: parking_lot::Mutex<HashMap<Uuid, String>>,
}

/// What one decision came to.
enum Attempt {
    /// Nothing owed: already accepted.
    Settled,
    Accepted,
    /// Still owed: refused again, unreadable, or cut by the budget. Tried again on the next pass.
    Waiting,
}

impl<T: Transport + Send + Sync + 'static> CreditAcceptanceRegion<T> {
    pub fn new(resolve: Resolve<T>) -> Self {
        Self { resolve, cursors: parking_lot::Mutex::new(HashMap::new()) }
    }

    async fn attempt(conn: &Connection<T>, name: &str, persona: Uuid, id: &str) -> Attempt {
        let owed = match reviewed::owed_acceptance(conn, name, id).await {
            Ok(Some(submission)) => submission,
            Ok(None) => return Attempt::Settled,
            Err(error) => {
                continuum_base::probe!(class = "training.credit.retry_failed", persona = %name, decision = %id, error = %error,
                    "a decided credit could not be read: passed over now, tried again on the next pass");
                return Attempt::Waiting;
            }
        };
        match reviewed::retry_decided(conn, name, persona, owed).await {
            Ok(credit) if credit.state == reviewed::ReviewedCreditState::Accepted => {
                continuum_base::probe!(class = "training.credit.accepted_on_retry", persona = %name, submission = %owed,
                    "a decided credit the trigger had not accepted is accepted, with no new review, registration or turn");
                Attempt::Accepted
            }
            Ok(credit) => {
                if credit.state != reviewed::ReviewedCreditState::AwaitingAcceptance {
                    continuum_base::probe!(class = "training.credit.retry_failed", persona = %name, decision = %id, state = ?credit.state,
                        "a decision with no acceptance did not settle to a pending credit: passed over now, tried again on the next pass");
                }
                Attempt::Waiting
            }
            Err(error) => {
                continuum_base::probe!(class = "training.credit.retry_failed", persona = %name, decision = %id, error = %error,
                    "a decided credit could not be settled again: tried again on the next pass");
                Attempt::Waiting
            }
        }
    }

    /// One bounded step for `persona`. Progress is kept after every decision attempted, before
    /// its settle is awaited, so a slow or broken decision costs its own turn and never the
    /// ones after it; it is tried again, under the same submission id, when the scan comes
    /// round (Codex on #5531).
    pub async fn step(&self, persona: Uuid) -> CadenceHint {
        let Some((name, conn)) = (self.resolve)(persona) else { return CadenceHint::Sleep };
        let after = self.cursors.lock().get(&persona).cloned();
        let (ids, full) = match reviewed::decided_page(&conn, &name, after.as_deref(), PAGE).await {
            Ok(page) => page,
            Err(error) => {
                continuum_base::probe!(class = "training.credit.acceptance_unreadable", persona = %name, error = %error,
                    "her decided credits could not be listed: retried on the next tick");
                return CadenceHint::Slower;
            }
        };
        let deadline = tokio::time::Instant::now() + tick_budget();
        let (mut waiting, mut finished) = (false, true);
        for id in &ids {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                finished = false;
                break;
            }
            self.cursors.lock().insert(persona, id.clone());
            match tokio::time::timeout(deadline - now, Self::attempt(&conn, &name, persona, id)).await {
                Ok(Attempt::Settled | Attempt::Accepted) => {}
                Ok(Attempt::Waiting) => waiting = true,
                Err(_) => {
                    waiting = true;
                    continuum_base::probe!(class = "training.credit.retry_slow", persona = %name, decision = %id,
                        budget_ms = tick_budget().as_millis() as u64,
                        "settling a decided credit outlasted this tick's budget: passed over now, tried again on the next pass");
                }
            }
        }
        if finished && !full {
            self.cursors.lock().remove(&persona); // past her last decision: the next scan starts again
        }
        let more = !finished || full;
        match (waiting, more) {
            (true, _) => CadenceHint::Slower,
            (false, true) => CadenceHint::Hold,
            (false, false) => CadenceHint::Sleep,
        }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> BrainRegion for CreditAcceptanceRegion<T> {
    fn id(&self) -> RegionId {
        RegionId::from_static("credit-acceptance")
    }

    fn pressure_profile(&self) -> PressureProfile {
        PressureProfile {
            // A cursor per resident; no payload held between ticks.
            memory_class: MemoryClass::Light,
            // Store reads and one command per owed credit; the training itself is the trigger's.
            compute_class: ComputeClass::Bookkeeping,
            responds_to: vec![PressureSignalKind::SystemMemHigh],
        }
    }

    /// Settling a reviewed correction into her training is learning, never a reply owed.
    fn orientation(&self) -> Orientation {
        Orientation::Speciation
    }

    async fn tick(&self, ctx: &RegionContext) -> TickOutcome {
        let hint = match ctx.persona_scope {
            Some(persona) => self.step(persona).await,
            None => CadenceHint::Sleep,
        };
        TickOutcome { cadence_hint: Some(hint), ..TickOutcome::idle() }
    }
}
