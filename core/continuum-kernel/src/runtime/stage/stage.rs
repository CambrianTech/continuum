//! The trait every brain function implements, and what a run can return.

use async_trait::async_trait;

use super::control::{ControlRequest, StopReceipt};
use super::cost::Cost;
use super::identity::{RunId, StageId};
use super::topic::{InputRevision, Provenance};
use super::triggers::Triggers;

/// What the substrate hands a run: its identity and the control requests it checks at its yield
/// points. The authorized capability for its admitted execution and residency is added by the
/// resource authority's half of the contract.
pub struct StageContext {
    pub run: RunId,
    pub control: tokio::sync::watch::Receiver<ControlRequest>,
}

impl StageContext {
    /// The request to honour at this yield point.
    pub fn requested(&self) -> ControlRequest {
        *self.control.borrow()
    }
}

/// What one run produced.
#[derive(Debug)]
pub enum RunOutcome<O> {
    /// Outputs to publish under the publication rule, each with its provenance.
    Produced(Vec<(O, Provenance)>),
    /// Work handed to the task owner; its completion re-triggers the stage.
    Dispatched,
    /// Nothing to do for these inputs.
    Idle,
    /// Stopped at a yield point; the receipt proves which stop.
    Stopped(StopReceipt),
}

/// A brain function on the substrate. Long work is a task (the shared task owner) whose
/// completion re-triggers the stage.
#[async_trait]
pub trait Stage: Send + Sync + 'static {
    /// The typed payload a run reads (a snapshot it was handed, or a page drained from an event
    /// topic); its revisions say exactly which versions it is.
    type Input: Send + Sync + Clone + 'static;
    type Output: Send + Sync + Clone + 'static;
    fn id(&self) -> StageId;
    fn triggers(&self) -> Triggers;
    fn cost(&self, revisions: &[InputRevision]) -> Cost;
    async fn run(
        &self,
        ctx: &StageContext,
        input: &Self::Input,
        revisions: &[InputRevision],
    ) -> RunOutcome<Self::Output>;
}
