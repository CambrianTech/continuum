//! Public submission/review commands on the existing work owner. Artifact
//! references carry content identity; publication does not imply blob verification.

use continuum_base::time::EpochMs;
use super::*;
use crate::persona::training_producer::reviewed::{self, SubmissionSelection};
use crate::runtime::{CommandExecutor, InProcessTransport, LateBound};
use continuum_client::Connection;

use crate::persona::work_board::submissions::*;

mod benchmark_adapter;
pub use benchmark_adapter::BenchmarkSubmissionAdapter;
mod latest_submission;
pub use latest_submission::*;
mod next_after_submission;
mod submodule_drift;
pub use next_after_submission::*;
mod staged_evidence_page;
pub use staged_evidence_page::*;
mod work_submission;

/// Her credit store, reached as her: the `@persona:` handle resolves through this identity.
fn credit_connection(
    executor: Option<std::sync::Arc<CommandExecutor>>,
    persona_id: Uuid,
) -> Result<Connection<InProcessTransport>, CommandError> {
    let executor = executor.ok_or_else(|| CommandError::Internal("work executor is not ready".into()))?;
    Ok(Connection::new(InProcessTransport::new(
        executor,
        Some(crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona_id))),
    )))
}

#[cfg(test)]
mod tests;
