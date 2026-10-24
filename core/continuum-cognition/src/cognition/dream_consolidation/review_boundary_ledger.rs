//! The dream's view of the durable review-boundary ledger: accepted review boundaries waiting
//! to be consolidated, paged in and marked finished. The ledger is persona's (training
//! producer's ORM rows, under the resident's own caller identity); the dream only drives it.
//! A finished receipt means an episode-consolidation boundary finished, never that a training
//! job ran.

use async_trait::async_trait;
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
