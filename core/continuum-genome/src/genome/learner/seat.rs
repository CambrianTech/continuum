//! Seating a mind's shadow in the engine that serves her: measured, leased, then loaded
//! (card 4114a2c6).
//!
//! A shadow is the adapter she learns into, on the same resident weights serving uses. Its memory is
//! a claim on the one budget, so the order is fixed and every step is the existing owner's:
//! 1. the engine measures the shadow's weights without writing or allocating anything (dry run);
//! 2. the core leases exactly those bytes from its resource authority, under the serving lifecycle
//!    gate (`training_admission`), so a relaunch cannot race the read, and puts the leases in
//!    custody at once ([`super::custody`]);
//! 3. the engine mirrors those leases as a residency, checked against its own incarnation (a
//!    relaunch between 2 and 3 is refused as `stale_engine`, and the leases drop);
//! 4. the engine writes and loads the shadow inside that residency, refusing any byte past it.
//!
//! The seat holds a claim on its residency; its leases stay in custody until the engine confirms
//! the residency released. Leaving is the reverse order: [`Seat::unload`], then [`Seat::release`].

use super::custody::{Claim, Custody};
use crate::inference::llama_server::{
    CoveringLease, FreshShadow, LeaseKind, LearningEngine, LedgerAnswer, LedgerEngine, LoadedAdapter, ResidencyId,
    ResidencyRefusal, ResidencyRow, ResidencySubject, ResidencyTerms, RowKind,
};
use crate::resources::{LeaseGuard, ResourceKind};

/// A shadow seated in an engine, with the claim on the residency that pays for its weights.
pub struct Seat {
    pub engine: LearningEngine,
    pub ledger: LedgerEngine,
    pub claim: Claim,
    pub shadow: LoadedAdapter,
}

/// Why a shadow was not seated. Whatever the engine might still hold stays in custody.
#[derive(Debug)]
pub enum SeatRefusal {
    /// The engine has no learning ledger (older than the in-place routes).
    NoLedger,
    /// The engine could not measure the shadow (it does not fit this model, or the engine failed).
    Unmeasured(String),
    /// The resource authority did not grant the bytes, or the engine did not answer the residency.
    Unleased(String),
    /// The engine refused to mirror the leases.
    Unmirrored(ResidencyRefusal),
    /// The engine did not load the shadow inside its residency. Released when the engine confirmed
    /// it, otherwise left in custody as an orphan the owner settles.
    Unloaded(String),
}

impl std::fmt::Display for SeatRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeatRefusal::NoLedger => write!(f, "the engine has no learning ledger"),
            SeatRefusal::Unmeasured(why) => write!(f, "the shadow could not be measured: {why}"),
            SeatRefusal::Unleased(why) => write!(f, "the shadow's memory was not leased: {why}"),
            SeatRefusal::Unmirrored(why) => write!(f, "the engine refused the residency: {why:?}"),
            SeatRefusal::Unloaded(why) => write!(f, "the shadow was not loaded: {why}"),
        }
    }
}

/// The authorities a seat answers to: the one resource authority, the serving lifecycle gate leases
/// are read and granted under, and the custody that holds every learning lease. The owning task
/// builds it once.
pub struct Authority {
    pub resources: std::sync::Arc<crate::resources::ResourceDaemon>,
    pub serving: crate::inference::serving_lifecycle::LifecycleGate,
    pub custody: std::sync::Arc<Custody>,
}

impl Authority {
    /// This core's authorities, as installed at start, with a fresh custody for the owning task.
    pub fn installed() -> Result<Self, SeatRefusal> {
        Ok(Self {
            resources: crate::resources::ResourceDaemon::global().ok_or_else(|| SeatRefusal::Unleased("the resource authority is not running".into()))?,
            serving: crate::inference::serving_lifecycle::LifecycleGate::global()
                .ok_or_else(|| SeatRefusal::Unleased("the serving lifecycle gate is not installed".into()))?,
            custody: Custody::new(),
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
    let claim = register(authority, &engine, &plan.engine, ResidencySubject::Fresh(shadow.clone()), rows, leases).await?;
    match engine.seat_fresh(&shadow, claim.residency()).await {
        Ok(loaded) => Ok(Seat { engine, ledger: plan.engine, claim, shadow: loaded }),
        // a lost reply is not proof that nothing loaded: settle unloads whatever the ledger shows bound
        Err(error) => match claim.settle().await {
            Ok(()) => Err(SeatRefusal::Unloaded(error.to_string())),
            Err((_orphaned, why)) => Err(SeatRefusal::Unloaded(format!("{error}; unconfirmed ({why}), left in custody"))),
        },
    }
}

/// Put `leases` in custody for a new residency over `rows` and mirror it in `engine` (incarnation
/// `ledger`). A refusal is the engine's word that it holds nothing, so those leases drop; an
/// unanswered registration stays in custody, orphaned, for the owner to settle.
pub(super) async fn register(
    authority: &Authority,
    engine: &LearningEngine,
    ledger: &LedgerEngine,
    subject: ResidencySubject,
    rows: Vec<ResidencyRow>,
    leases: Vec<(LeaseKind, u64, LeaseGuard)>,
) -> Result<Claim, SeatRefusal> {
    let covering: Vec<CoveringLease> =
        leases.iter().map(|(kind, bytes, guard)| CoveringLease { kind: *kind, lease_id: guard.lease_id().to_owned(), bytes: *bytes }).collect();
    let id = ResidencyId::issue();
    let claim = authority.custody.hold(engine, id, leases.into_iter().map(|(_, _, guard)| guard).collect());
    let terms = ResidencyTerms {
        id,
        engine: ledger.clone(),
        owner: crate::capacity::window_grant::GrantOwner::this_process().id(),
        subject,
        rows,
        leases: covering,
    };
    match engine.register_residency(&terms).await {
        Ok(LedgerAnswer::Ledger(registered)) => match registered.refused {
            None => Ok(claim),
            Some(why) => {
                claim.refused();
                Err(SeatRefusal::Unmirrored(why))
            }
        },
        Ok(LedgerAnswer::NoLedger) => {
            claim.refused();
            Err(SeatRefusal::NoLedger)
        }
        Err(error) => Err(SeatRefusal::Unleased(format!("the residency was not registered: {error}"))),
    }
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
    /// Unload the shadow (the engine releases its optimizer first). Its residency stays until
    /// released.
    pub async fn unload(&self) -> Result<(), String> {
        self.engine.unseat(self.shadow.id).await.map(|_| ()).map_err(|error| format!("unload refused: {error}"))
    }

    /// Release the weight residency of an unloaded shadow; its leases leave custody when the engine
    /// confirms. An unconfirmed release hands the seat back.
    pub async fn release(self) -> Result<(), (Self, String)> {
        let Self { engine, ledger, claim, shadow } = self;
        claim.release().await.map_err(|(claim, why)| (Self { engine, ledger, claim, shadow }, why))
    }
}

/// Release `residency`; `Ok` only when the engine confirms it holds nothing there: released, or a
/// residency it does not know (never registered, or a relaunched engine that lost it with its memory).
pub(super) async fn confirm_release(engine: &LearningEngine, residency: &ResidencyId) -> Result<(), String> {
    match engine.release_residency(&residency.to_string()).await {
        Ok(LedgerAnswer::Ledger(released)) => match released.refused {
            None | Some(ResidencyRefusal::UnknownResidency) => Ok(()),
            Some(why) => Err(format!("residency {residency} release refused: {why:?}")),
        },
        Ok(LedgerAnswer::NoLedger) => Err("the engine lost its learning ledger".into()),
        Err(error) => Err(format!("residency {residency} release failed: {error}")),
    }
}

#[cfg(test)]
mod tests;
