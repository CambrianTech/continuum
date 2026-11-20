//! A turn changes the desired residency. One queued wake covers all changes
//! since the worker last read the phenotype state.
use super::GeneResidency;
use continuum_flow::{spawn_stage, Capacity, Overflow, PushError, Qos, Stage, StageInput, StageWorker};
use parking_lot::Mutex;
use std::sync::{Arc, Weak};

#[derive(Default)]
pub(super) struct Worker(Mutex<Option<Running>>);

struct Running {
    input: StageInput<()>,
    _worker: StageWorker<()>,
}

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
        let mut running = self.reconcile_worker.0.lock();
        if let Some(stage) = running.as_ref() {
            match stage.input.push(()) {
                Ok(_) | Err(PushError::Full(())) => return,
                Err(PushError::Stopped(())) => continuum_base::probe!(
                    class = "genome.page.worker_stopped",
                    "replacing the stopped gene residency worker and resubmitting its wake"
                ),
            }
        }
        if let Err(error) = tokio::runtime::Handle::try_current() {
            continuum_base::probe!(class = "genome.page.worker_unavailable", error = %error,
                "gene reconciliation needs a running executor");
            return;
        }
        let (input, worker) = spawn_stage(Reconcile(Arc::downgrade(self)), Capacity::latest());
        if let Err(PushError::Stopped(())) = input.push(()) {
            continuum_base::probe!(class = "genome.page.worker_stopped", "executor stopped during gene worker startup");
        }
        *running = Some(Running { input, _worker: worker });
    }
}
