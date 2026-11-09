//! What learning holds beyond a seated shadow's weights, leased before the engine allocates it
//! (card 4114a2c6).
//!
//! Two claims, both measured by the engine on a disposable scheduler (`/learn/preflight`) and leased
//! from the one resource authority:
//! - per mind, her optimizer state (gradient accumulators and AdamW moments), a residency on her
//!   loaded shadow;
//! - per engine, ONE training workspace shared by every mind's chunks (never N copies), leased
//!   once and reused while it covers what a preflight requires.
//! The first learn on an engine replaces the serving scheduler, so the workspace is leased in full,
//! with no credit for what serving held (`shared_compute_held` is evidence, never a lease row).

use super::seat::{lease_rows, Authority, Seat, SeatRefusal};
use crate::inference::llama_server::{
    CoveringLease, LedgerAnswer, PlannedRow, Preflight, ResidencyId, ResidencyRow, ResidencySubject, ResidencyTerms, RowKind,
};
use crate::resources::LeaseGuard;

/// One mind's learning memory: her optimizer residency and the leases that pay for it.
pub struct MindLearning {
    pub optimizer: ResidencyId,
    _leases: Vec<LeaseGuard>,
}

impl MindLearning {
    /// Release her optimizer residency once its shadow is unloaded (the engine frees the optimizer
    /// with it); the leases drop with this value. A busy release keeps it, leases and all.
    pub async fn release(self, seat: &Seat) -> Result<(), (Self, String)> {
        match seat.engine.release_residency(&self.optimizer.to_string()).await {
            Ok(LedgerAnswer::Ledger(released)) if released.refused.is_none() => Ok(()),
            Ok(LedgerAnswer::Ledger(released)) => {
                let why = format!("optimizer residency release refused: {:?}", released.refused);
                Err((self, why))
            }
            Ok(LedgerAnswer::NoLedger) => Err((self, "the engine lost its learning ledger".into())),
            Err(error) => Err((self, format!("optimizer residency release failed: {error}"))),
        }
    }
}

/// The engine's one training workspace: its residency, the rows it covers, and its leases.
pub struct EngineWorkspace {
    pub residency: ResidencyId,
    rows: Vec<ResidencyRow>,
    _leases: Vec<LeaseGuard>,
}

impl EngineWorkspace {
    /// Whether this workspace covers every row a preflight requires (same device, buffer type and
    /// host class, at least the bytes).
    fn covers(&self, required: &[ResidencyRow]) -> bool {
        required.iter().all(|need| {
            self.rows.iter().any(|row| {
                row.device == need.device && row.buffer_type == need.buffer_type && row.host == need.host && row.bytes >= need.bytes
            })
        })
    }
}

/// Lease and register what learning on `seat` needs: her optimizer residency, and the engine's
/// workspace when it does not hold one that covers this preflight yet.
pub async fn prepare(authority: &Authority, seat: &Seat, consumer: &str, workspace: &mut Option<EngineWorkspace>) -> Result<MindLearning, SeatRefusal> {
    let plan = match seat.engine.learning_preflight(&seat.ledger, seat.shadow.id).await {
        Ok(LedgerAnswer::Ledger(Preflight::Planned(plan))) => plan,
        Ok(LedgerAnswer::Ledger(Preflight::Refused { refusal, detail })) => {
            return Err(SeatRefusal::Unmeasured(format!("learning preflight refused ({refusal:?}): {}", detail.unwrap_or_default()))) // unwrap_or_default: a refusal without detail still names its kind
        }
        Ok(LedgerAnswer::NoLedger) => return Err(SeatRefusal::NoLedger),
        Err(error) => return Err(SeatRefusal::Unmeasured(error.to_string())),
    };
    let (optimizer_rows, workspace_rows) = split(&plan.requirements);
    if !workspace.as_ref().is_some_and(|held| held.covers(&workspace_rows)) {
        // A held workspace that no longer covers stays held until its replacement registers
        // (the engine moves its binding, then the old residency is released).
        let leases = lease_rows(authority, &format!("{consumer}:workspace"), &workspace_rows).await?;
        let residency = register(seat, ResidencySubject::Workspace, workspace_rows.clone(), &leases).await?;
        *workspace = Some(EngineWorkspace { residency, rows: workspace_rows, _leases: leases.into_iter().map(|(_, _, guard)| guard).collect() });
    }
    let leases = lease_rows(authority, consumer, &optimizer_rows).await?;
    let subject = ResidencySubject::Loaded { lora_id: seat.shadow.id, path: seat.shadow.path.clone() };
    let optimizer = register(seat, subject, optimizer_rows, &leases).await?;
    Ok(MindLearning { optimizer, _leases: leases.into_iter().map(|(_, _, guard)| guard).collect() })
}

/// A preflight's required rows, split into her optimizer's and the engine's workspace; the rows
/// that describe what serving holds now are evidence and lease nothing.
fn split(requirements: &[PlannedRow]) -> (Vec<ResidencyRow>, Vec<ResidencyRow>) {
    let mut optimizer = Vec::new();
    let mut workspace = Vec::new();
    for row in requirements {
        let kind = match row.kind.as_str() {
            "optimizer_static_required" => RowKind::OptimizerStaticRequired,
            "optimizer_host_required" => RowKind::OptimizerHostRequired,
            "shared_compute_required" => RowKind::SharedComputeRequired,
            _ => continue, // shared_compute_held: what serving holds, never a claim
        };
        let resident = ResidencyRow { device: row.device.clone(), buffer_type: row.buffer_type.clone(), kind, bytes: row.bytes, host: row.host };
        if kind == RowKind::SharedComputeRequired { workspace.push(resident) } else { optimizer.push(resident) }
    }
    (optimizer, workspace)
}

async fn register(
    seat: &Seat,
    subject: ResidencySubject,
    rows: Vec<ResidencyRow>,
    leases: &[(crate::inference::llama_server::LeaseKind, u64, LeaseGuard)],
) -> Result<ResidencyId, SeatRefusal> {
    let terms = ResidencyTerms {
        id: ResidencyId::issue(),
        engine: seat.ledger.clone(),
        owner: crate::capacity::window_grant::GrantOwner::this_process().id(),
        subject,
        rows,
        leases: leases.iter().map(|(kind, bytes, guard)| CoveringLease { kind: *kind, lease_id: guard.lease_id().to_owned(), bytes: *bytes }).collect(),
    };
    match seat.engine.register_residency(&terms).await {
        Ok(LedgerAnswer::Ledger(registered)) => match registered.refused {
            None => Ok(terms.id),
            Some(why) => Err(SeatRefusal::Unmirrored(why)),
        },
        Ok(LedgerAnswer::NoLedger) => Err(SeatRefusal::NoLedger),
        Err(error) => Err(SeatRefusal::Unleased(format!("the residency was not registered: {error}"))),
    }
}
