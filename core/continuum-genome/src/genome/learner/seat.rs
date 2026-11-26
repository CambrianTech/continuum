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
    CoveringLease, LeaseKind, LearningEngine, LedgerAnswer, LedgerEngine, LoadedAdapter, ResidencyId,
    ResidencyRefusal, ResidencyRow, ResidencySubject, ResidencyTerms, RowKind,
};
use crate::inference::serving_lifecycle::Admission;
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
    /// Not granted on this attempt, nothing held: wait for what it names (a capacity change, the
    /// serving lifecycle) holding no serving resource, then try again.
    AwaitingMemory(crate::inference::training_admission::TrainingWait),
    /// The engine refused to mirror the leases.
    Unmirrored(ResidencyRefusal),
    /// The engine did not load the shadow inside its residency. Released when the engine confirmed
    /// it, otherwise left in custody as an orphan the owner settles.
    Unloaded(String),
    /// The engine measuring the load is not the one the caller bound the change to.
    EngineReplaced,
    /// A stage could not be carved from its granted reserve (released, lapsed, of another kind, or
    /// holding less).
    Uncarved(crate::resources::LeaseError),
}

impl std::fmt::Display for SeatRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeatRefusal::NoLedger => write!(f, "the engine has no learning ledger"),
            SeatRefusal::Unmeasured(why) => write!(f, "the shadow could not be measured: {why}"),
            SeatRefusal::Unleased(why) => write!(f, "the shadow's memory was not leased: {why}"),
            SeatRefusal::AwaitingMemory(wait) => write!(f, "waiting for memory, holding nothing: {wait}"),
            SeatRefusal::Unmirrored(why) => write!(f, "the engine refused the residency: {why:?}"),
            SeatRefusal::Unloaded(why) => write!(f, "the shadow was not loaded: {why}"),
            SeatRefusal::EngineReplaced => write!(f, "the engine was replaced after the change was bound to it"),
            SeatRefusal::Uncarved(error) => write!(f, "a stage could not be carved from its reserve: {error:?}"),
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
    /// An authority of its own for a test: a mock resource daemon, an owner-less serving gate and an
    /// isolated custody.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn isolated(vram: u64) -> Self {
        let resources = crate::resources::ResourceDaemon::start(
            vec![std::sync::Arc::new(crate::resources::MockCapacitySource::new(crate::resources::ResourceKind::Vram, vram))],
            vec![],
            crate::resources::DaemonConfig::default(),
        );
        Self { resources, serving: crate::inference::serving_lifecycle::LifecycleGate::unowned(true), custody: Custody::isolated() }
    }
}

/// One gene to seat: the adapter file `name` (staged under the engine's train dir) for `consumer`,
/// every step bound to `bound`, the engine identity the caller read before deciding, and its memory
/// admitted inside `operation`, the serving operation the caller holds on the authority's gate.
pub struct GeneLoad<'a> {
    pub operation: &'a crate::inference::serving_lifecycle::ServingOperation,
    pub engine: LearningEngine,
    pub bound: &'a LedgerEngine,
    pub consumer: &'a str,
    pub name: &'a str,
}

/// Seat `load`: an engine replaced since its identity was read refuses instead of loading into the
/// wrong one (4.10 P4a).
pub async fn seat_gene(authority: &Authority, load: GeneLoad<'_>) -> Result<Seat, SeatRefusal> {
    let GeneLoad { operation, engine, bound, consumer, name } = load;
    let plan = match engine.plan_load(name).await {
        Ok(LedgerAnswer::Ledger(plan)) => plan,
        Ok(LedgerAnswer::NoLedger) => return Err(SeatRefusal::NoLedger),
        Err(error) => return Err(SeatRefusal::Unmeasured(error.to_string())),
    };
    if &plan.engine != bound {
        return Err(SeatRefusal::EngineReplaced);
    }
    let rows: Vec<ResidencyRow> = plan.weights.into_iter().map(|row| ResidencyRow::of(row, RowKind::Weights)).collect();
    let leases = lease_rows(authority, Admission::Within(&authority.serving, operation), consumer, &rows).await?;
    let claim = register(authority, &engine, &plan.engine, ResidencySubject::Load { name: name.to_owned() }, rows, leases).await?;
    let loaded = engine.seat_load(name, claim.residency()).await;
    seated(engine, plan.engine, claim, loaded).await
}

/// The seat an adapter change inside `claim`'s residency came to.
async fn seated(engine: LearningEngine, ledger: LedgerEngine, claim: Claim, loaded: Result<LoadedAdapter, crate::inference::llama_server::LlamaServerError>) -> Result<Seat, SeatRefusal> {
    match loaded {
        Ok(loaded) => Ok(Seat { engine, ledger, claim, shadow: loaded }),
        // a lost reply is not proof that nothing loaded: settle unloads whatever the ledger shows bound
        Err(error) => match claim.release().await {
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
    let claim = authority.custody.hold(engine, ledger, id, leases.into_iter().map(|(_, _, guard)| guard).collect());
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
/// authority, under the serving lifecycle gate, in ONE attempt: a learner's step may hold her serving
/// resources, so it never waits here (Codex on #5905). A shortfall answers `AwaitingMemory`, and the
/// leases this call already took drop with it.
pub(super) async fn lease_rows(authority: &Authority, admission: Admission<'_>, consumer: &str, rows: &[ResidencyRow]) -> Result<Vec<(LeaseKind, u64, LeaseGuard)>, SeatRefusal> {
    let mut leases = Vec::new();
    for (kind, resource, host) in [(LeaseKind::Vram, ResourceKind::Vram, false), (LeaseKind::Ram, ResourceKind::Ram, true)] {
        let bytes: u64 = rows.iter().filter(|row| row.host == host).map(|row| row.bytes).sum();
        if bytes == 0 {
            continue;
        }
        let guard = crate::inference::training_admission::try_training_memory(&authority.resources, admission, consumer, resource, bytes)
            .map_err(SeatRefusal::Unleased)?
            .map_err(SeatRefusal::AwaitingMemory)?;
        leases.push((kind, bytes, guard));
    }
    Ok(leases)
}

/// What [`lease_memory`] leased: the rows it reserved and their guards, which go to
/// `learning::LearningMemory::from_rows` exactly as they are, so the view covers what was leased and
/// nothing else; and the workspace the context already holds, kept for diagnostics only.
pub struct LeasedMemory {
    pub rows: Vec<crate::inference::learning::MemoryRow>,
    pub leases: Vec<(LeaseKind, u64, LeaseGuard)>,
    pub held: Vec<crate::inference::learning::MemoryRow>,
}

impl LeasedMemory {
    /// The engine's view of these leases: a `LearningMemory` over exactly the rows they cover, owning
    /// nothing. A view is built per command (each lesson's admission is its own); the rows are copied
    /// into it because the view counts what native code charges against each.
    pub fn view(&self) -> Result<crate::inference::learning::LearningMemory, &'static str> {
        crate::inference::learning::LearningMemory::from_rows(self.rows.clone(), &self.leases)
    }
}

/// Lease what the in-process engine measured for a seat (`learning::SeatState::Measured` or
/// `Loaded`), one guard per memory class (device rows as VRAM, host rows as RAM), in ONE attempt
/// under the serving lifecycle gate, as [`lease_rows`] does for the HTTP engine. A shortfall answers
/// `AwaitingMemory` and the guards already taken drop with it.
///
/// Rows are partitioned by their typed role first: `SharedComputeHeld` is workspace the context
/// already holds, evidence and never a further lease, so it is neither leased nor handed to the view
/// (leasing it would charge held memory twice, and `from_rows` refuses it; Codex on dd98bfdda).
pub fn lease_memory(authority: &Authority, consumer: &str, rows: impl IntoIterator<Item = crate::inference::learning::MemoryRow>) -> Result<LeasedMemory, SeatRefusal> {
    lease_memory_within(authority, &[], consumer, rows)
}

/// [`lease_memory`], each class spent from `reserve` when it holds a granted reserve of that kind with
/// room for it (a seat's granted ask, carved stage by stage: no second ask, nothing a waiter can take
/// between), otherwise leased as `lease_memory` does.
pub fn lease_memory_within(authority: &Authority, reserve: &[LeaseGuard], consumer: &str, rows: impl IntoIterator<Item = crate::inference::learning::MemoryRow>) -> Result<LeasedMemory, SeatRefusal> {
    use crate::inference::learning::MemoryRole;
    let (held, rows): (Vec<_>, Vec<_>) = rows.into_iter().partition(|row| row.role == MemoryRole::SharedComputeHeld);
    let mut leases = Vec::new();
    for (kind, resource, host) in [(LeaseKind::Vram, ResourceKind::Vram, false), (LeaseKind::Ram, ResourceKind::Ram, true)] {
        let bytes = rows
            .iter()
            .filter(|row| row.host == host)
            .try_fold(0u64, |sum, row| sum.checked_add(row.bytes))
            .ok_or_else(|| SeatRefusal::Unleased("the measured rows overflow a byte count".into()))?;
        if bytes == 0 {
            continue;
        }
        let from = reserve.iter().find(|guard| guard.kind() == Some(resource) && guard.bytes() >= bytes);
        let attempt = match from {
            Some(reserve) => {
                let stage = crate::inference::training_admission::Carve { reserve, consumer, bytes };
                crate::inference::training_admission::try_carve(&authority.resources, Admission::Free(&authority.serving), stage).map_err(SeatRefusal::Uncarved)?
            }
            None => crate::inference::training_admission::try_training_memory(&authority.resources, Admission::Free(&authority.serving), consumer, resource, bytes).map_err(SeatRefusal::Unleased)?,
        };
        let guard = attempt.map_err(SeatRefusal::AwaitingMemory)?;
        leases.push((kind, bytes, guard));
    }
    Ok(LeasedMemory { rows, leases, held })
}

impl Seat {
    /// Unload the shadow (the engine releases its optimizer first). Its residency stays until
    /// released.
    pub async fn unload(&self) -> Result<(), crate::inference::llama_server::LlamaServerError> {
        self.engine.unseat(&self.ledger, self.shadow.id).await.map(|_| ())
    }

    /// Release the weight residency of an unloaded shadow; its leases leave custody when the engine
    /// confirms. An unconfirmed release hands the seat back.
    pub async fn release(self) -> Result<(), (Self, String)> {
        let Self { engine, ledger, claim, shadow } = self;
        claim.release().await.map_err(|(claim, why)| (Self { engine, ledger, claim, shadow }, why))
    }
}

#[cfg(test)]
mod tests;
