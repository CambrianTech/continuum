//! What learning holds beyond a seated shadow's weights, leased before the engine allocates it
//! (card 4114a2c6).
//!
//! Two claims, both measured by the engine on a disposable scheduler (`/learn/preflight`) and leased
//! from the one resource authority:
//! - per mind, her optimizer state (gradient accumulators and AdamW moments), a residency on her
//!   loaded shadow;
//! - per engine, ONE training workspace shared by every mind's chunks (never N copies), reused while
//!   it covers what a preflight requires and replaced (the new one registered before the old is
//!   released) when a longer example needs more.
//! Each claim is priced for the example about to be learned, which the engine renders and counts
//! itself: never a guessed geometry, never the whole served window by default.
//! The first learn on an engine replaces the serving scheduler, so the workspace is leased in full,
//! with no credit for what serving held (`shared_compute_held` is evidence, never a lease row).

use super::custody::Claim;
use super::seat::{lease_rows, register, Authority, Seat, SeatRefusal};
use crate::inference::llama_server::{LedgerAnswer, PlannedRow, Preflight, ResidencyId, ResidencyRow, ResidencySubject, RowKind};

/// One mind's learning memory: the claim on her optimizer residency.
pub struct MindLearning {
    claim: Claim,
    rows: Vec<ResidencyRow>,
}

impl MindLearning {
    /// Release her optimizer residency once its shadow is unloaded (the engine frees the optimizer
    /// with it); its leases leave custody when the engine confirms. Unconfirmed, it is handed back.
    pub async fn release(self) -> Result<(), (Self, String)> {
        let Self { claim, rows } = self;
        claim.release().await.map_err(|(claim, why)| (Self { claim, rows }, why))
    }
}

/// The engine's one training workspace: the claim on its residency and the rows it covers.
pub struct EngineWorkspace {
    claim: Claim,
    rows: Vec<ResidencyRow>,
    /// Workspaces this one replaced whose release the engine has not confirmed yet: the engine moves
    /// its binding on the next learn that claims the new residency, not on registration, so each
    /// stays in custody until its release is accepted.
    superseded: Vec<Claim>,
}

impl EngineWorkspace {
    /// Whether this workspace covers every row a preflight requires.
    fn covers(&self, required: &[ResidencyRow]) -> bool {
        covered(&self.rows, required)
    }

    /// Release the superseded workspaces the engine no longer binds; each one it still binds (busy)
    /// or does not answer for stays claimed for the next settle.
    pub async fn settle(&mut self) {
        let mut kept = Vec::new();
        for old in std::mem::take(&mut self.superseded) {
            if let Err((old, _unconfirmed)) = old.release().await {
                kept.push(old);
            }
        }
        self.superseded = kept;
    }

    /// Release the workspace residency once no mind learns on its engine (the engine reclaims the
    /// buffer after its last optimizer), superseded ones first. Unconfirmed, it is handed back.
    pub async fn release(mut self) -> Result<(), (Self, String)> {
        self.settle().await;
        if !self.superseded.is_empty() {
            let why = format!("{} superseded workspace residencies are not released yet", self.superseded.len());
            return Err((self, why));
        }
        let Self { claim, rows, superseded } = self;
        claim.release().await.map_err(|(claim, why)| (Self { claim, rows, superseded }, why))
    }
}

/// Whether `held` rows cover every row `required` (same device, buffer type and host class, at
/// least the bytes).
fn covered(held: &[ResidencyRow], required: &[ResidencyRow]) -> bool {
    required.iter().all(|need| {
        held.iter().any(|row| row.device == need.device && row.buffer_type == need.buffer_type && row.host == need.host && row.bytes >= need.bytes)
    })
}

/// A replacement workspace's rows: every buffer the one it replaces was registered with and every
/// buffer this example needs, each at the larger of the two. A longer example can need more on one
/// buffer and less, or nothing, on another, and the engine refuses a replacement that does not
/// cover the old registration (`uncovered`).
fn widened(held: &[ResidencyRow], required: Vec<ResidencyRow>) -> Vec<ResidencyRow> {
    let mut rows: Vec<ResidencyRow> = held.to_vec();
    for need in required {
        match rows.iter_mut().find(|row| row.device == need.device && row.buffer_type == need.buffer_type && row.host == need.host) {
            Some(row) => row.bytes = row.bytes.max(need.bytes),
            None => rows.push(need),
        }
    }
    rows
}

/// The residencies a covered example learns inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Covered {
    pub optimizer: ResidencyId,
    pub workspace: ResidencyId,
}

/// Make sure what learning `messages` (with `tools`) on `seat` needs is leased and mirrored: her
/// optimizer residency, and the engine's workspace, replaced when the one held does not cover this
/// example. Her optimizer is sized by her shadow's parameters, not by the example, so an optimizer
/// that stops covering is refused as unmeasured rather than replaced.
pub async fn cover(
    authority: &Authority,
    seat: &Seat,
    consumer: &str,
    messages: &[serde_json::Value],
    tools: Option<&serde_json::Value>,
    mind: &mut Option<MindLearning>,
    workspace: &mut Option<EngineWorkspace>,
) -> Result<Covered, SeatRefusal> {
    let plan = match seat.engine.learning_preflight(&seat.ledger, seat.shadow.id, messages, tools).await {
        Ok(LedgerAnswer::Ledger(Preflight::Planned(plan))) => plan,
        Ok(LedgerAnswer::Ledger(Preflight::Refused { refusal, detail })) => {
            return Err(SeatRefusal::Unmeasured(format!("learning preflight refused ({refusal:?}): {}", detail.unwrap_or_default()))) // unwrap_or_default: a refusal without detail still names its kind
        }
        Ok(LedgerAnswer::NoLedger) => return Err(SeatRefusal::NoLedger),
        Err(error) => return Err(SeatRefusal::Unmeasured(error.to_string())),
    };
    let (optimizer_rows, workspace_rows) = split(&plan.requirements);
    let workspace_residency = match workspace.as_ref() {
        Some(held) if held.covers(&workspace_rows) => held.claim.residency(),
        Some(held) => {
            // The engine hands the old workspace's custody to the new one only when the new
            // covers every row the old was registered with (claim_plan): never narrower.
            let rows = widened(&held.rows, workspace_rows);
            replace_workspace(authority, seat, consumer, rows, workspace).await?
        }
        None => replace_workspace(authority, seat, consumer, workspace_rows, workspace).await?,
    };
    let optimizer = match mind {
        Some(held) if covered(&held.rows, &optimizer_rows) => held.claim.residency(),
        Some(_) => {
            return Err(SeatRefusal::Unmeasured(format!(
                "her optimizer needs more than its measured residency for a {}-row reply in a {}-token window",
                plan.reply_rows, plan.served_window
            )))
        }
        None => {
            let leases = lease_rows(authority, consumer, &optimizer_rows).await?;
            let subject = ResidencySubject::Loaded { lora_id: seat.shadow.id, path: seat.shadow.path.clone() };
            let claim = register(authority, &seat.engine, &seat.ledger, subject, optimizer_rows.clone(), leases).await?;
            let optimizer = claim.residency();
            *mind = Some(MindLearning { claim, rows: optimizer_rows });
            optimizer
        }
    };
    Ok(Covered { optimizer, workspace: workspace_residency })
}

/// Lease and register a workspace covering `rows`; the engine binds the next learn to it, and the
/// one it replaces (if any) stays claimed until [`EngineWorkspace::settle`] confirms its release.
async fn replace_workspace(
    authority: &Authority,
    seat: &Seat,
    consumer: &str,
    rows: Vec<ResidencyRow>,
    workspace: &mut Option<EngineWorkspace>,
) -> Result<ResidencyId, SeatRefusal> {
    let leases = lease_rows(authority, &format!("{consumer}:workspace"), &rows).await?;
    let claim = register(authority, &seat.engine, &seat.ledger, ResidencySubject::Workspace, rows.clone(), leases).await?;
    let residency = claim.residency();
    let mut replacement = EngineWorkspace { claim, rows, superseded: Vec::new() };
    // the engine still binds the old one until a learn claims the new: it stays claimed, never released here
    if let Some(old) = workspace.take() {
        let EngineWorkspace { claim, superseded, .. } = old;
        replacement.superseded.extend(superseded);
        replacement.superseded.push(claim);
    }
    *workspace = Some(replacement);
    Ok(residency)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(device: &str, host: bool, bytes: u64) -> ResidencyRow {
        ResidencyRow { device: device.into(), buffer_type: format!("{device}-buft"), kind: RowKind::SharedComputeRequired, bytes, host }
    }

    // what this catches (fork #107 measurement): a replacement workspace sized to the new example
    // alone. A longer example can need more on the GPU and less on the host, or a buffer the old
    // one held not at all; the engine then refuses the replacement as 'uncovered'.
    #[test]
    fn a_replacement_workspace_covers_the_one_it_replaces_and_the_new_need() {
        let held = vec![row("gpu", false, 4_000), row("host", true, 7_000), row("cpu-extra", true, 512)];
        let need = vec![row("gpu", false, 9_000), row("host", true, 2_500)];
        let rows = widened(&held, need.clone());
        assert!(covered(&rows, &held), "the replacement covers every old row");
        assert!(covered(&rows, &need), "and this example's need");
        assert_eq!(rows.len(), 3, "a buffer is listed once, at the larger size");
        assert!(rows.iter().any(|r| r.device == "gpu" && r.bytes == 9_000));
        assert!(rows.iter().any(|r| r.device == "host" && r.bytes == 7_000));
    }
}
