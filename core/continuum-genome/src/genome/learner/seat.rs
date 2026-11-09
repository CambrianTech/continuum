//! Seating a mind's shadow in the engine that serves her: measured, leased, then loaded
//! (card 4114a2c6).
//!
//! A shadow is the adapter she learns into, on the same resident weights serving uses. Its memory is
//! a claim on the one budget, so the order is fixed and every step is the existing owner's:
//! 1. the engine measures the shadow's weights without writing or allocating anything (dry run);
//! 2. the core leases exactly those bytes from its resource authority, under the serving lifecycle
//!    gate (`training_admission`), so a relaunch cannot race the read;
//! 3. the engine mirrors those leases as a residency, checked against its own incarnation (a
//!    relaunch between 2 and 3 is refused as `stale_engine`, and the leases drop);
//! 4. the engine writes and loads the shadow inside that residency, refusing any byte past it.
//!
//! The leases live in the [`Seat`]; dropping it releases them. Leaving is the reverse order:
//! [`Seat::unload`], then [`Seat::release`] the residency, then the leases drop, never the other way.

use crate::inference::llama_server::{
    CoveringLease, FreshShadow, LeaseKind, LearningEngine, LedgerAnswer, LedgerEngine, LoadedAdapter, ResidencyId,
    ResidencyRefusal, ResidencyRow, ResidencySubject, ResidencyTerms, RowKind,
};
use crate::resources::{LeaseGuard, ResourceKind};

/// A shadow seated in an engine, with the leases that pay for its weights.
pub struct Seat {
    pub engine: LearningEngine,
    pub ledger: LedgerEngine,
    pub residency: ResidencyId,
    pub shadow: LoadedAdapter,
    _leases: Vec<LeaseGuard>,
}

/// Why a shadow was not seated. Nothing is left allocated or leased after any of them.
#[derive(Debug)]
pub enum SeatRefusal {
    /// The engine has no learning ledger (older than the in-place routes).
    NoLedger,
    /// The engine could not measure the shadow (it does not fit this model, or the engine failed).
    Unmeasured(String),
    /// The resource authority did not grant the bytes.
    Unleased(String),
    /// The engine refused to mirror the leases.
    Unmirrored(ResidencyRefusal),
    /// The engine refused to write or load the shadow inside its residency, and confirmed nothing
    /// is live there (its release was accepted), so the leases are dropped.
    Unloaded(String),
    /// The engine's answer was lost or it would not confirm the residency empty: whatever it holds
    /// keeps its leases in `retained` until [`Retained::settle`] confirms the release.
    Unfenced { why: String, retained: Retained },
}

impl std::fmt::Display for SeatRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeatRefusal::NoLedger => write!(f, "the engine has no learning ledger"),
            SeatRefusal::Unmeasured(why) => write!(f, "the shadow could not be measured: {why}"),
            SeatRefusal::Unleased(why) => write!(f, "the shadow's memory was not leased: {why}"),
            SeatRefusal::Unmirrored(why) => write!(f, "the engine refused the residency: {why:?}"),
            SeatRefusal::Unloaded(why) => write!(f, "the shadow was not loaded: {why}"),
            SeatRefusal::Unfenced { why, retained } => write!(f, "the shadow's load is unconfirmed, {retained:?} keeps its leases: {why}"),
        }
    }
}

/// The authorities a seat answers to: the one resource authority, and the serving lifecycle gate
/// leases are read and granted under.
pub struct Authority {
    pub resources: std::sync::Arc<crate::resources::ResourceDaemon>,
    pub serving: crate::inference::serving_lifecycle::LifecycleGate,
}

impl Authority {
    /// This core's authorities, as installed at start.
    pub fn installed() -> Result<Self, SeatRefusal> {
        Ok(Self {
            resources: crate::resources::ResourceDaemon::global().ok_or_else(|| SeatRefusal::Unleased("the resource authority is not running".into()))?,
            serving: crate::inference::serving_lifecycle::LifecycleGate::global()
                .ok_or_else(|| SeatRefusal::Unleased("the serving lifecycle gate is not installed".into()))?,
        })
    }
}

/// Seat `shadow` for `consumer` (her learning's name on the resource board) in `engine`.
pub async fn seat(authority: &Authority, engine: LearningEngine, consumer: &str, shadow: FreshShadow) -> Result<Seat, SeatRefusal> {
    let plan = match engine.plan_fresh_shadow(&shadow).await {
        Ok(LedgerAnswer::Ledger(plan)) => plan,
        Ok(LedgerAnswer::NoLedger) => return Err(SeatRefusal::NoLedger),
        Err(error) => return Err(SeatRefusal::Unmeasured(error.to_string())),
    };
    let rows: Vec<ResidencyRow> = plan.weights.into_iter().map(|row| ResidencyRow::of(row, RowKind::Weights)).collect();
    let leases = lease_rows(authority, consumer, &rows).await?;
    let terms = ResidencyTerms {
        id: ResidencyId::issue(),
        engine: plan.engine.clone(),
        owner: crate::capacity::window_grant::GrantOwner::this_process().id(),
        subject: ResidencySubject::Fresh(shadow.clone()),
        rows,
        leases: leases.iter().map(|(kind, bytes, guard)| CoveringLease { kind: *kind, lease_id: guard.lease_id().to_owned(), bytes: *bytes }).collect(),
    };
    match engine.register_residency(&terms).await {
        Ok(LedgerAnswer::Ledger(registered)) => {
            if let Some(why) = registered.refused {
                return Err(SeatRefusal::Unmirrored(why));
            }
        }
        Ok(LedgerAnswer::NoLedger) => return Err(SeatRefusal::NoLedger),
        Err(error) => return Err(SeatRefusal::Unleased(format!("the residency was not registered: {error}"))),
    }
    let loaded = match engine.seat_fresh(&shadow, terms.id).await {
        Ok(loaded) => loaded,
        Err(error) => {
            // a lost reply is not proof that nothing loaded: the engine's release answer is the fence
            let retained = Retained { engine, residency: terms.id, _leases: leases.into_iter().map(|(_, _, guard)| guard).collect() };
            return Err(match retained.settle().await {
                Ok(()) => SeatRefusal::Unloaded(error.to_string()),
                Err((retained, why)) => SeatRefusal::Unfenced { why: format!("{error}; then {why}"), retained },
            });
        }
    };
    Ok(Seat { engine, ledger: plan.engine, residency: terms.id, shadow: loaded, _leases: leases.into_iter().map(|(_, _, guard)| guard).collect() })
}

/// Lease each class of the rows (device memory as VRAM, host memory as RAM) from the resource
/// authority, under the serving lifecycle gate.
pub(super) async fn lease_rows(authority: &Authority, consumer: &str, rows: &[ResidencyRow]) -> Result<Vec<(LeaseKind, u64, LeaseGuard)>, SeatRefusal> {
    let mut leases = Vec::new();
    for (kind, resource, host) in [(LeaseKind::Vram, ResourceKind::Vram, false), (LeaseKind::Ram, ResourceKind::Ram, true)] {
        let bytes: u64 = rows.iter().filter(|row| row.host == host).map(|row| row.bytes).sum();
        if bytes == 0 {
            continue;
        }
        let guard = crate::inference::training_admission::wait_for_training_memory(authority.resources.clone(), &authority.serving, consumer, resource, bytes, |_| {})
            .await
            .map_err(SeatRefusal::Unleased)?;
        leases.push((kind, bytes, guard));
    }
    Ok(leases)
}

impl Seat {
    /// Unload the shadow (the engine releases its optimizer first). Its residencies and leases stay
    /// until released, in that order.
    pub async fn unload(&self) -> Result<(), String> {
        self.engine.unseat(self.shadow.id).await.map(|_| ()).map_err(|error| format!("unload refused: {error}"))
    }

    /// Release the weight residency of an unloaded shadow; the leases go with the seat. A release
    /// the engine refuses (busy) keeps the seat, so its leases stay.
    pub async fn release(self) -> Result<(), (Self, String)> {
        match confirm_release(&self.engine, &self.residency).await {
            Ok(()) => Ok(()),
            Err(why) => Err((self, why)),
        }
    }
}

/// Release `residency`; `Ok` only when the engine confirms it (a busy, unknown or unanswered release
/// is an error, so whoever holds the leases keeps them).
pub(super) async fn confirm_release(engine: &LearningEngine, residency: &ResidencyId) -> Result<(), String> {
    match engine.release_residency(&residency.to_string()).await {
        Ok(LedgerAnswer::Ledger(released)) if released.refused.is_none() => Ok(()),
        Ok(LedgerAnswer::Ledger(released)) => Err(format!("residency {residency} release refused: {:?}", released.refused)),
        Ok(LedgerAnswer::NoLedger) => Err("the engine lost its learning ledger".into()),
        Err(error) => Err(format!("residency {residency} release failed: {error}")),
    }
}

/// A residency whose release the engine has not confirmed, holding its leases until it does: the
/// memory stays charged while the engine may still hold it, never dropped on a lost answer.
pub struct Retained {
    engine: LearningEngine,
    residency: ResidencyId,
    _leases: Vec<LeaseGuard>,
}

impl std::fmt::Debug for Retained {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Retained({})", self.residency)
    }
}

impl Retained {
    /// Settle it: unload whatever adapter the engine bound inside the residency, then release it.
    /// The leases drop only once the engine confirms the release; otherwise it is handed back.
    pub async fn settle(self) -> Result<(), (Self, String)> {
        let id = self.residency.to_string();
        let bound = match self.engine.learn_ledger().await {
            Ok(LedgerAnswer::Ledger(ledger)) => ledger.residencies.iter().find(|held| held.id == id).map(|held| held.lora_id).filter(|lora| *lora >= 0),
            Ok(LedgerAnswer::NoLedger) => return Err((self, "the engine lost its learning ledger".into())),
            Err(error) => return Err((self, format!("the ledger did not answer: {error}"))),
        };
        if let Some(lora) = bound {
            if let Err(error) = self.engine.unseat(lora).await {
                return Err((self, format!("unload of adapter {lora} refused: {error}")));
            }
        }
        match confirm_release(&self.engine, &self.residency).await {
            Ok(()) => Ok(()),
            Err(why) => Err((self, why)),
        }
    }
}

#[cfg(test)]
mod tests;
