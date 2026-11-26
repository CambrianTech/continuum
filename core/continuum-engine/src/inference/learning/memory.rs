//! Memory reserved for one learner command. The guards own the reservation and the
//! measured rows limit which native allocator may spend it.
use std::cell::Cell;

use llama::{BufferType, MemoryRole, MemoryRow, OptimizerLease, ReplyAdmission, Role, Slot};

use crate::inference::llama_server::LeaseKind;
use crate::resources::{LeaseGuard, LeaseView, ResourceKind};

/// Built from the same measured rows and guarded leases used to admit the command.
pub struct LearningMemory {
    rows: Vec<(MemoryRow, Cell<u64>)>,
    leases: Vec<(LeaseKind, u64, LeaseView)>,
}

impl LearningMemory {
    /// Refuse incomplete coverage before passing a non-owning view to the native learner.
    pub fn from_rows(
        rows: Vec<MemoryRow>,
        leases: &[(LeaseKind, u64, LeaseGuard)],
    ) -> Result<Self, &'static str> {
        if rows
            .iter()
            .any(|row| row.role == MemoryRole::SharedComputeHeld)
        {
            return Err("already-held workspace is evidence, not a reservation");
        }
        let mut views = Vec::with_capacity(leases.len());
        for (kind, resource, host) in [
            (LeaseKind::Ram, ResourceKind::Ram, true),
            (LeaseKind::Vram, ResourceKind::Vram, false),
        ] {
            let mut guards = leases.iter().filter(|(class, _, _)| *class == kind);
            let needed = rows
                .iter()
                .filter(|row| row.host == host)
                .try_fold(0u64, |sum, row| sum.checked_add(row.bytes))
                .ok_or("learning memory size overflow")?;
            match (needed, guards.next()) {
                (0, None) => {}
                (bytes, Some((_, reserved, guard))) if *reserved == bytes => {
                    let view = guard.view().ok_or("learning grant retired")?;
                    if view.kind() != resource
                        || view.bytes_at(continuum_base::time::EpochMs::now().millis()) < bytes
                    {
                        return Err(
                            "learning grant has insufficient coverage for this memory class",
                        );
                    }
                    views.push((kind, bytes, view));
                }
                _ => return Err("learning rows do not match the guarded reservation"),
            }
            if guards.next().is_some() {
                return Err("multiple learning guards for one memory class");
            }
        }
        Ok(Self {
            rows: rows.into_iter().map(|row| (row, Cell::new(0))).collect(),
            leases: views,
        })
    }

    fn row(&self, buffer: BufferType, kind: MemoryRole) -> Option<&(MemoryRow, Cell<u64>)> {
        let name = buffer.name().ok()?;
        let device = buffer.device().transpose().ok()?.unwrap_or("host"); // ggml names a device-less host allocator this way in the measurement.
        self.rows.iter().find(|(row, _)| {
            row.role == kind
                && row.host == buffer.is_host()
                && row.buffer_type == name
                && row.device == device
        })
    }

    fn covered(&self, host: bool) -> bool {
        let kind = if host {
            LeaseKind::Ram
        } else {
            LeaseKind::Vram
        };
        self.leases.iter().any(|(class, bytes, guard)| {
            *class == kind
                && guard.bytes_at(continuum_base::time::EpochMs::now().millis()) >= *bytes
        })
    }
}

impl ReplyAdmission for LearningMemory {
    fn workspace(&self, buffer: BufferType, required: u64, held: u64) -> bool {
        let row = self.row(buffer, MemoryRole::SharedComputeRequired);
        let fits = row.is_some_and(|(row, _)| required.max(held) <= row.bytes);
        let covered = row.is_some_and(|(row, _)| self.covered(row.host));
        if !(fits && covered) {
            // a refusal names which bound refused: no measured row for this buffer, a graph past the row
            // measured for it, or a lease no longer covering it (the M5 06:20Z: "optimizer workspace
            // allocation was refused" said none of these)
            continuum_base::probe!(class = "learning.workspace.refused", buffer = ?buffer.name(), required, held,
                measured = row.map_or(0, |(row, _)| row.bytes), has_row = row.is_some(), fits, covered,
                "a reply's workspace was refused before it allocated");
        }
        fits && covered
    }
}

fn optimizer_row(role: Role) -> MemoryRole {
    match role {
        Role::Static => MemoryRole::OptimizerStatic,
        Role::Host => MemoryRole::OptimizerHost,
    }
}

impl OptimizerLease for LearningMemory {
    fn admit(&self, slot: Slot, bytes: u64) -> bool {
        let Some((row, charged)) = self.row(slot.device, optimizer_row(slot.role)) else {
            return false;
        };
        let Some(next) = charged.get().checked_add(bytes) else {
            return false;
        };
        if next > row.bytes || !self.covered(row.host) {
            return false;
        }
        charged.set(next);
        true
    }

    fn release(&self, slot: Slot, bytes: u64) {
        if let Some((_, charged)) = self.row(slot.device, optimizer_row(slot.role)) {
            if let Some(left) = charged.get().checked_sub(bytes) {
                charged.set(left);
            }
        }
    }
}
