//! Durable perception acceptance through the registered resident's one checkpoint (P2; Codex #5691
//! 773d34a4): a drained page is staged into her working memory (references, source cursor, gap
//! receipts and wake obligations together) and only the saved snapshot authorizes a wake. Replays
//! are idempotent and still checkpoint (an earlier staging may have survived a failed write).
use super::super::working_memory::{AcceptedPage, PerceptionAcceptError, SequenceDomain};
use super::*;

#[derive(Debug, thiserror::Error)]
pub enum PerceptionConsumptionError {
    #[error(transparent)]
    Owner(#[from] OwnerCheckpointError),
    #[error("perception for {0} was drained by a replaced memory incarnation")]
    Replaced(Uuid),
    #[error(transparent)]
    Accept(#[from] PerceptionAcceptError),
}

impl PersonaWorkspaceRegistry {
    /// Accept one drained page for `owner`'s memory `incarnation`: validated whole inside the
    /// checkpoint's exclusion, so replacement during the drain cannot accept into the next
    /// resident's memory.
    pub fn accept_perception(
        &self,
        owner: Uuid,
        incarnation: Uuid,
        page: AcceptedPage,
    ) -> Result<VolatileReceipt, PerceptionConsumptionError> {
        self.commit_owner_state(owner, |memory| {
            if memory.incarnation() != incarnation {
                return Err(PerceptionConsumptionError::Replaced(owner));
            }
            memory.stage_perception_page(page)?;
            Ok(())
        })
    }

    /// Retire lines a rendered-context receipt proves this incarnation was shown.
    pub fn record_perception_reads(
        &self,
        owner: Uuid,
        incarnation: Uuid,
        domain: SequenceDomain,
        seqs: &[u64],
    ) -> Result<VolatileReceipt, PerceptionConsumptionError> {
        self.commit_owner_state(owner, |memory| {
            if memory.incarnation() != incarnation {
                return Err(PerceptionConsumptionError::Replaced(owner));
            }
            memory.stage_perception_reads(domain, seqs)?;
            Ok(())
        })
    }
}
