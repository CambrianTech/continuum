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
//!
//! A settle runs OFF the tick, as its own task, and is never cut by it (card a4e8756d). It used to
//! run inside the tick under half the governor's region bound. On Windows (2026-10-28 01:58Z) a
//! submit slower than that was cancelled mid-flight after the trigger had already accepted Kimi's
//! 28087ebb but before her acceptance was recorded; each pass cut it again until one happened to
//! fit (02:04:59Z). The tick now only starts settles (one in flight per decision) and never waits
//! on or cancels one.

use std::collections::{HashMap, HashSet};
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

/// Who a persona is to this region: her name (the store's handle) and her store connection, as
/// her own caller identity. `None` when she is not resident here.
pub type Resolve<T> = Arc<dyn Fn(Uuid) -> Option<(String, Connection<T>)> + Send + Sync>;

pub struct CreditAcceptanceRegion<T: Transport + Send + Sync + 'static> {
    resolve: Resolve<T>,
    /// Where each persona's scan resumes: the last decision id attempted.
    cursors: parking_lot::Mutex<HashMap<Uuid, String>>,
    /// The settles running now, by persona and decision: one at a time per decision.
    in_flight: Arc<parking_lot::Mutex<HashSet<(Uuid, String)>>>,
}

/// A settle's place in the in-flight set, given up however its task ends (a panic included).
struct Settling {
    in_flight: Arc<parking_lot::Mutex<HashSet<(Uuid, String)>>>,
    key: (Uuid, String),
}

impl Drop for Settling {
    fn drop(&mut self) {
        self.in_flight.lock().remove(&self.key);
    }
}

impl<T: Transport + Send + Sync + 'static> CreditAcceptanceRegion<T> {
    pub fn new(resolve: Resolve<T>) -> Self {
        Self { resolve, cursors: parking_lot::Mutex::new(HashMap::new()), in_flight: Arc::new(parking_lot::Mutex::new(HashSet::new())) }
    }

    /// Settle one owed decision again, under the submission id it was issued with: run off the
    /// tick, to completion. Its outcome is said here; a still-owed one is found again next pass.
    async fn settle(conn: &Connection<T>, name: &str, persona: Uuid, id: &str, owed: Uuid) {
        match reviewed::retry_decided(conn, name, persona, owed).await {
            Ok(credit) if credit.state == reviewed::ReviewedCreditState::Accepted => {
                continuum_base::probe!(class = "training.credit.accepted_on_retry", persona = %name, submission = %owed,
                    "a decided credit the trigger had not accepted is accepted, with no new review, registration or turn");
            }
            Ok(credit) => {
                if credit.state != reviewed::ReviewedCreditState::AwaitingAcceptance {
                    continuum_base::probe!(class = "training.credit.retry_failed", persona = %name, decision = %id, state = ?credit.state,
                        "a decision with no acceptance did not settle to a pending credit: tried again on the next pass");
                }
            }
            Err(error) => {
                continuum_base::probe!(class = "training.credit.retry_failed", persona = %name, decision = %id, error = %error,
                    "a decided credit could not be settled again: tried again on the next pass");
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
        // For each decision still owed an acceptance, start its settle unless one is running, with
        // at most a page of settles in flight for her. The owed check is a store read, kept here;
        // the settle (the submit) runs off the tick and is never waited on.
        let (mut busy, mut finished) = (false, true);
        for id in &ids {
            match reviewed::owed_acceptance(&conn, &name, id).await {
                Ok(None) => {} // nothing owed: already accepted
                Err(error) => {
                    continuum_base::probe!(class = "training.credit.retry_failed", persona = %name, decision = %id, error = %error,
                        "a decided credit could not be read: passed over now, tried again on the next pass");
                    busy = true;
                }
                Ok(Some(owed)) => {
                    busy = true;
                    let key = (persona, id.clone());
                    let start = {
                        let mut in_flight = self.in_flight.lock();
                        if in_flight.contains(&key) {
                            false // its settle is still running
                        } else if in_flight.iter().filter(|(p, _)| *p == persona).count() >= PAGE {
                            finished = false; // her settles are at their bound: resume here next pass
                            break;
                        } else {
                            in_flight.insert(key.clone());
                            true
                        }
                    };
                    if start {
                        let settling = Settling { in_flight: Arc::clone(&self.in_flight), key };
                        let (conn, name, id) = (conn.clone(), name.clone(), id.clone());
                        tokio::spawn(async move {
                            let _settling = settling;
                            Self::settle(&conn, &name, persona, &id, owed).await;
                        });
                    }
                }
            }
            // Handled (settled, unreadable, running or started): the scan resumes after it.
            self.cursors.lock().insert(persona, id.clone());
        }
        if finished && !full {
            self.cursors.lock().remove(&persona); // past her last decision: the next scan starts again
        }
        match (busy, full) {
            // Settles are running: look again after they have had time.
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
