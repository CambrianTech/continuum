//! A turn changes the desired residency. One queued wake covers all changes
//! since the worker last read the phenotype state.
use super::GeneResidency;
use continuum_flow::{spawn_stage, Capacity, Overflow, PushError, Qos, Stage, StageInput, StageWorker};
use std::sync::{Arc, OnceLock, Weak};

#[derive(Default)]
pub(super) struct Worker(OnceLock<(StageInput<()>, StageWorker<()>)>);

struct Reconcile(Weak<GeneResidency>);

impl Stage for Reconcile {
    type Input = ();
    const QOS: Qos = Qos::Utility;
    const OVERFLOW: Overflow = Overflow::KeepNewest;

    async fn handle(&mut self, (): ()) {
        if let Some(owner) = self.0.upgrade() {
            owner.reconcile().await;
        }
    }
}

impl GeneResidency {
    pub(in crate::genome) fn request_reconcile(self: &Arc<Self>) {
        let (input, _) = self.reconcile_worker.0.get_or_init(|| {
            spawn_stage(Reconcile(Arc::downgrade(self)), Capacity::latest())
        });
        match input.push(()) {
            Ok(_) | Err(PushError::Full(())) => {}
            Err(PushError::Stopped(())) => continuum_base::probe!(
                class = "genome.page.worker_stopped",
                "gene residency worker stopped before it could read the new phenotype"
            ),
        }
    }
}
