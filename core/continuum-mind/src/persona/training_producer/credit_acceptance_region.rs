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
//! fit (02:04:59Z). The tick now only starts settles, one in flight per decision and a page per
//! persona, and never waits on one; a settle's slot is reclaimed only under contention
//! ([`SettleSlots`]), so a hung submit cannot starve a healthy decision (Codex on #5547).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use continuum_client::{Connection, Transport};
use uuid::Uuid;

use crate::persona::training_producer::reviewed;
use crate::persona::training_producer::settle_slots::{self, Admit, SettleSlots};
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
    /// The settles running now, owned here: dropping the region aborts every one.
    slots: settle_slots::Shared,
}

impl<T: Transport + Send + Sync + 'static> CreditAcceptanceRegion<T> {
    pub fn new(resolve: Resolve<T>) -> Self {
        let slots = SettleSlots::shared(PAGE, crate::persona::substrate_governor::REGION_TICK_TIMEOUT);
        Self { resolve, cursors: parking_lot::Mutex::new(HashMap::new()), slots }
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

    /// One bounded step for `persona`. The scan moves past every decision it handled and never
    /// waits on a settle, so a slow or broken decision costs its own slot and never the ones after
    /// it (Codex on #5531); a settle past its quantum is cut and tried again, under the same
    /// submission id, when the scan comes round ([`SettleSlots`]).
    pub async fn step(&self, persona: Uuid) -> CadenceHint {
        // whoever has left, swept on any pass (she may never be ticked again): her settles go with her
        settle_slots::release_departed(&self.slots, |p| (self.resolve)(p).is_some());
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
        // at most a page of settles in flight for her (an overdue one yields its slot to a waiting
        // decision). The owed check is a store read, kept here; the settle runs off the tick.
        let (mut busy, mut finished) = (false, true);
        for id in &ids {
            match reviewed::owed_acceptance(&conn, &name, id).await {
                Ok(None) => self.slots.lock().settled(&(persona, id.clone())), // nothing owed: already accepted
                Err(error) => {
                    continuum_base::probe!(class = "training.credit.retry_failed", persona = %name, decision = %id, error = %error,
                        "a decided credit could not be read: passed over now, tried again on the next pass");
                    busy = true;
                }
                Ok(Some(owed)) => {
                    busy = true;
                    let settle = {
                        let (conn, name, id) = (conn.clone(), name.clone(), id.clone());
                        async move { Self::settle(&conn, &name, persona, &id, owed).await }
                    };
                    match settle_slots::try_start(&self.slots, &(persona, id.clone()), tokio::time::Instant::now(), settle) {
                        Admit::Running | Admit::Start { .. } => {}
                        Admit::Cut { victim: (_, decision) } => {
                            continuum_base::probe!(class = "training.credit.settle_cut", persona = %name, decision = %decision, for_decision = %id,
                                "a settle outran its quantum: aborted, and replayed under its submission id once its task has ended, with a doubled quantum");
                            if decision != *id {
                                finished = false; // cut to free a slot for this one: resume here once it frees
                                break;
                            }
                        }
                        Admit::Full => {
                            finished = false; // her settles hold every slot: resume here next pass
                            break;
                        }
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
