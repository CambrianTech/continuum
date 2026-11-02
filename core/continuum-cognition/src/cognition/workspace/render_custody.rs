//! A cycle's receipt custody is fixed at assembly. Private and measurement
//! cycles cannot borrow the live resident's checkpoint authority.
use super::WorkspaceCycle;
use crate::cognition::persona_workspace::{PersonaWorkspaceRegistry, ResidentTaskSink, TaskSinkError};
use crate::cognition::render_receipt::RenderReceipt;
use std::sync::{Arc, Weak};
use uuid::Uuid;

pub(super) enum RenderCustody {
    Isolated,
    Resident {
        registry: Weak<PersonaWorkspaceRegistry>,
        owner: Uuid,
        incarnation: Uuid,
    },
}

impl WorkspaceCycle {
    pub(crate) fn with_render_custody(
        mut self,
        registry: &Arc<PersonaWorkspaceRegistry>,
        owner: Uuid,
        incarnation: Uuid,
    ) -> Self {
        self.render_custody = RenderCustody::Resident {
            registry: Arc::downgrade(registry), owner, incarnation,
        };
        self
    }

    /// Persist the observations of this resident's served generations. Isolated
    /// cycles have no durable task acknowledgement authority. Registry teardown
    /// and receipt disagreement are explicit failures, never an owner substitution.
    pub async fn persist_render_receipts(&self, receipts: Vec<RenderReceipt>) -> Result<(), TaskSinkError> {
        let RenderCustody::Resident { registry, owner, incarnation } = &self.render_custody else {
            return Ok(());
        };
        if receipts.is_empty() { return Ok(()); }
        // Validate the complete batch before any receipt can mutate the ledger.
        if receipts.iter().any(|receipt| receipt.owner != *owner || receipt.incarnation != *incarnation) {
            return Err(TaskSinkError::CustodyMismatch { owner: *owner, incarnation: *incarnation });
        }
        let registry = registry.upgrade().ok_or(TaskSinkError::OwnerGone(*owner))?;
        let sink = ResidentTaskSink::new(registry);
        for receipt in receipts { sink.record_rendered(receipt).await?; }
        Ok(())
    }
}
