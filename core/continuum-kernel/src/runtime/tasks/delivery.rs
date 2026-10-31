//! The shared owner-delivery seam from HER-TASKS T2. Notifications only request a
//! reconciliation; durable ownership supplies the results and consumption precedes ACK.
use super::{TaskEnd, TaskLedger};
use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use uuid::Uuid;

/// Implemented by the owner of the recipient's durable state, never by a transient
/// workspace or a notification channel. Success means the exact terminal result and
/// its outstanding wake obligation are durable. Returned IDs attest checkpointed
/// perception, not filing, and must belong to this page. Replays deduplicate by ID.
/// Consume the owner page in one commit; per-result checkpoints amplify storage work.
#[async_trait::async_trait]
pub trait TaskOwnerSink: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn consume(&self, owner: Uuid, ends: &[(Uuid, TaskEnd)]) -> Result<BTreeSet<Uuid>, Self::Error>;
}

/// Failure retains its boundary: a sink refusal cannot masquerade as an ACK failure,
/// and a failed ACK can be replayed after consumption without repeating its effects.
#[derive(Debug)]
pub enum DeliveryError<E> {
    Read(String),
    Consume { owner: Uuid, source: E },
    InvalidReceipt { task: Uuid },
    Acknowledge { task: Uuid, message: String },
}

impl<E: std::fmt::Display> std::fmt::Display for DeliveryError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(message) => write!(f, "task replay read failed: {message}"),
            Self::Consume { owner, source } => {
                write!(f, "owner {owner} consumption failed: {source}")
            }
            Self::Acknowledge { task, message } => {
                write!(f, "task {task} acknowledgement failed: {message}")
            }
            Self::InvalidReceipt { task } => write!(f, "perception receipt for task {task} is outside the delivered page"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for DeliveryError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Consume { source, .. } => Some(source),
            Self::Read(_) | Self::Acknowledge { .. } | Self::InvalidReceipt { .. } => None,
        }
    }
}

pub struct DeliveryPage {
    pub acknowledged: usize,
    pub after: Option<Uuid>,
    pub exhausted: bool,
}

impl TaskLedger {
    /// Deliver one bounded storage page. The existing lifecycle/scheduler owns when to
    /// continue or restart a sweep; this seam creates no timer, listener or executor.
    /// On failure restart the sweep: previously acknowledged tasks are excluded, while
    /// an unacknowledged consumption is replayed into the sink's durable deduplication.
    pub async fn deliver_page<S: TaskOwnerSink + ?Sized>(
        &self,
        owner: Uuid,
        after: Option<Uuid>,
        limit: NonZeroUsize,
        sink: &S,
    ) -> Result<DeliveryPage, DeliveryError<S::Error>> {
        let page = self
            .pending_page(owner, after, limit)
            .await
            .map_err(DeliveryError::Read)?;
        let perceived = if page.ends.is_empty() {
            BTreeSet::new()
        } else {
            sink.consume(owner, &page.ends)
                .await
                .map_err(|source| DeliveryError::Consume { owner, source })?
        };
        let offered: BTreeSet<_> = page.ends.iter().map(|(task, _)| *task).collect();
        // Validate the entire receipt before acknowledging any row.
        if let Some(task) = perceived.difference(&offered).next() {
            return Err(DeliveryError::InvalidReceipt { task: *task });
        }
        let mut acknowledged = 0;
        for task in perceived {
            self.acknowledge(task, owner)
                .await
                .map_err(|message| DeliveryError::Acknowledge { task, message })?;
            acknowledged += 1;
        }
        Ok(DeliveryPage {
            acknowledged,
            after: page.after,
            exhausted: page.exhausted,
        })
    }
}

#[cfg(test)]
mod tests;
