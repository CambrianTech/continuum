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

/// Decided credits read per tick. A decision is one small row and an acceptance lookup; the
/// page bounds one tick's store reads, and the cursor reaches every decision in turn.
const PAGE: usize = 8;

/// Who a persona is to this region: her name (the store's handle) and her store connection, as
/// her own caller identity. `None` when she is not resident here.
pub type Resolve<T> = Arc<dyn Fn(Uuid) -> Option<(String, Connection<T>)> + Send + Sync>;

pub struct CreditAcceptanceRegion<T: Transport + Send + Sync + 'static> {
    resolve: Resolve<T>,
    /// Where each persona's scan resumes: the last decision id read.
    cursors: parking_lot::Mutex<HashMap<Uuid, Uuid>>,
}

impl<T: Transport + Send + Sync + 'static> CreditAcceptanceRegion<T> {
    pub fn new(resolve: Resolve<T>) -> Self {
        Self { resolve, cursors: parking_lot::Mutex::new(HashMap::new()) }
    }

    /// One bounded step for `persona`: settle the owed credits in this page and say whether any
    /// still waits.
    pub async fn step(&self, persona: Uuid) -> CadenceHint {
        let Some((name, conn)) = (self.resolve)(persona) else { return CadenceHint::Sleep };
        let after = self.cursors.lock().get(&persona).copied();
        let (owed, last) = match reviewed::decided_unaccepted(&conn, &name, after, PAGE).await {
            Ok(page) => page,
            Err(error) => {
                continuum_base::probe!(
                    class = "training.credit.acceptance_unreadable",
                    persona = %name,
                    error = %error,
                    "her decided credits could not be read: retried on the next tick"
                );
                return CadenceHint::Slower;
            }
        };
        {
            let mut cursors = self.cursors.lock();
            match last {
                Some(last) => cursors.insert(persona, last),
                None => cursors.remove(&persona), // past her last decision: the next scan starts again
            };
        }
        let mut waiting = false;
        for id in owed {
            match reviewed::retry_decided(&conn, &name, persona, id).await {
                Ok(credit) if credit.state == reviewed::ReviewedCreditState::Accepted => {
                    continuum_base::probe!(
                        class = "training.credit.accepted_on_retry",
                        persona = %name,
                        submission = %id,
                        "a decided credit the trigger had not accepted is accepted, with no new review, registration or turn"
                    );
                }
                Ok(credit) => {
                    waiting |= credit.state == reviewed::ReviewedCreditState::AwaitingAcceptance;
                }
                Err(error) => {
                    waiting = true;
                    continuum_base::probe!(
                        class = "training.credit.retry_failed",
                        persona = %name,
                        submission = %id,
                        error = %error,
                        "a decided credit could not be settled again: retried on the next tick"
                    );
                }
            }
        }
        match (waiting, last) {
            (true, _) => CadenceHint::Slower,
            // More of her decisions remain past this page: keep the current pace through them.
            (false, Some(_)) => CadenceHint::Hold,
            (false, None) => CadenceHint::Sleep,
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
