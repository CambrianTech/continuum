//! Public submission/review commands on the existing work owner. Artifact
//! references carry content identity; publication does not imply blob verification.

use continuum_base::time::EpochMs;
use super::*;
use crate::persona::training_producer::reviewed::{self, SubmissionSelection};
use crate::runtime::{CommandExecutor, InProcessTransport, LateBound};
use continuum_client::Connection;

use crate::persona::work_board::submissions::*;

mod latest_submission;
pub use latest_submission::*;
mod next_after_submission;
pub use next_after_submission::*;
mod work_submission;

/// One record-then-publish at a time per persona (card fe8554d2). A submission reads what the
/// board holds, records its pending binding (superseding her unpublished earlier records on the
/// claim) and publishes, all inside this fence, so a concurrent submit of hers (two in one act's
/// batch) never supersedes a record whose publication is still running. One entry per persona.
async fn publication_fence(persona: Uuid) -> tokio::sync::OwnedMutexGuard<()> {
    use std::collections::HashMap;
    use std::sync::{Arc, LazyLock, Mutex};
    static FENCES: LazyLock<Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let fence = match FENCES.lock() {
        Ok(mut fences) => fences.entry(persona).or_default().clone(),
        // The map holds only fences; a panic while it was locked left it whole.
        Err(poisoned) => poisoned.into_inner().entry(persona).or_default().clone(),
    };
    fence.lock_owned().await
}

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
