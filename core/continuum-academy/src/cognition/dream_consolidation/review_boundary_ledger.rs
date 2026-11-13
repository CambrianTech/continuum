//! The dream's view of the durable review-boundary ledger: accepted review boundaries waiting
//! to be consolidated, paged in and marked finished. The ledger is persona's (training
//! producer's ORM rows, under the resident's own caller identity); the dream only drives it.
//! A finished receipt means an episode-consolidation boundary finished, never that a training
//! job ran.

use async_trait::async_trait;
use parking_lot::RwLock;
use std::sync::Arc;
use continuum_client::ClientError;
use uuid::Uuid;

/// One page of pending boundaries and the cursor after it (`None` past the end).
#[derive(Debug)]
pub struct BoundaryPage {
    pub pending: Vec<Uuid>,
    pub next: Option<String>,
}

#[async_trait]
pub trait ReviewBoundaryLedger: Send + Sync {
    /// Make sure the ledger's storage exists (once per resident per process).
    async fn prepare(&self) -> Result<(), ClientError>;
    /// One indexed page after `after`, at most `limit` ids.
    async fn page(&self, after: Option<String>, limit: usize) -> Result<BoundaryPage, ClientError>;
    /// Record `ids` finished, with how many clusters were eligible.
    async fn finish(&self, ids: &[Uuid], eligible_clusters: usize) -> Result<(), ClientError>;
}

// The slot the persona layer fills at boot (`persona::cognition_extensions::install`): the
// dream owns the contract, cognition's core never names it.

/// The resident's durable review-boundary ledger (persona's training producer owns its rows).
pub type ResidentReviewBoundaries =
    fn(persona: Uuid) -> Result<Arc<dyn ReviewBoundaryLedger>, continuum_client::ClientError>;

static RESIDENT_REVIEW_BOUNDARIES: RwLock<Option<ResidentReviewBoundaries>> = RwLock::new(None);

pub fn install_resident_review_boundaries(ledger: ResidentReviewBoundaries) {
    *RESIDENT_REVIEW_BOUNDARIES.write() = Some(ledger);
}

/// Empty slot: probed, and `Ok(None)`: the resident has no durable credit store this tick
/// (the nonpersistent answer), so the dream consolidates without boundary recovery.
pub fn resident_review_boundaries(
    persona: Uuid,
) -> Result<Option<Arc<dyn ReviewBoundaryLedger>>, continuum_client::ClientError> {
    match *RESIDENT_REVIEW_BOUNDARIES.read() {
        Some(ledger) => ledger(persona).map(Some),
        None => {
            continuum_base::probe!(
                class = "cognition.extension.missing",
                slot = "resident_review_boundaries",
                "the dream asked for the resident's review-boundary ledger and nothing installed it; it consolidates without boundary recovery"
            );
            Ok(None)
        }
    }
}
