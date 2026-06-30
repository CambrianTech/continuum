//! `TrainingJobBoard` — the in-flight training-job registry that bridges L2
//! (a job was dispatched) and L3 (the job completed → eval → page-in).
//!
//! ## Why this exists (the missing retention)
//!
//! The training-job model is POLL-based: `genome/job-create` returns a
//! [`JobHandle`]; status is observed by polling `genome/job-status` →
//! [`super::FineTuningAdapter::poll`]. There is NO completion event crossing the
//! command boundary — the per-job [`super::JobController`] lives privately inside
//! each adapter, and cloud adapters (OpenAI, Mistral) are inherently poll-only. So
//! the ONE uniform way to observe completion across every provider is to poll the
//! handle on an interval.
//!
//! But nothing retained the handles the trigger creates: `dispatch_job_create`
//! returned `(JobHandle, provider)` to its caller and the handle was dropped on the
//! floor. This board IS that retention. The L2 trigger registers each
//! freshly-dispatched job here ([`TrainingJobBoard::register`]); the L3 sentinel
//! ([`crate::modules::training_completion_sentinel`]) polls each one and, on
//! terminal completion, claims it ([`TrainingJobBoard::claim`]) and runs the
//! eval→page-in chain. Polling vs a `watch` channel is the honest fit: there is no
//! in-process completion event to subscribe to, and a cloud provider can only be
//! asked, never told.
//!
//! ## Why process-global
//!
//! The writer (the [`crate::modules::training_trigger`] module) and the reader (the
//! L3 sentinel module) are independently-constructed `ServiceModule`s with no shared
//! owner — exactly the shape [`crate::cognition::persona_workspace::global`] and the
//! channel-digest buffer already use. One board per core, reached through
//! [`TrainingJobBoard::global`].
//!
//! ## Claim-once semantics (no double-processing)
//!
//! The sentinel's tick removes a job from the board the moment it observes a
//! terminal status ([`claim`]), BEFORE spawning the (minutes-long) eval chain off
//! the tick. A subsequent tick can never re-handle the same completed job because
//! it is already gone. A terminal event leaves the board exactly once.

use std::sync::OnceLock;

use dashmap::DashMap;
use uuid::Uuid;

use super::types::JobHandle;

/// One in-flight training job, plus the context the L3 sentinel needs to run the
/// eval→page-in chain when it completes WITHOUT re-deriving any of it. Cloned out of
/// the board on snapshot; the `handle.local_id` is the board key.
#[derive(Debug, Clone)]
pub struct WatchedJob {
    /// The handle to poll — `handle.provider_id` looks the adapter back up in the
    /// [`super::FineTuningRegistry`], `handle.local_id` is the stable board key.
    pub handle: JobHandle,
    /// The persona whose genome this layer trains — the eval subject and the
    /// page-in target (its live `WorkspaceCycle`).
    pub persona_id: Uuid,
    /// The persona's display name — log/observability context only.
    pub persona_name: String,
    /// The base model the layer was forged against — log/observability context.
    pub base_model: String,
    /// The domain bucket (`DomainClassifier` output) this layer specializes — used
    /// as the gene NAME on page-in and the eval gene label.
    pub trait_kind: String,
    /// The gym that measures this trait — the `cognition/eval` `eval_set` JSONL path,
    /// carried verbatim from the [`super::types::TrainingJobRequest`]. The sentinel
    /// passes it to the A/B eval; `None` means the recipe declared no gym, so the
    /// gene is unmeasurable and the sentinel refuses to adopt it (never falls back to
    /// a default gym — [[fallbacks-are-illegal-fail-loud]]).
    pub eval_set: Option<String>,
}

/// Process-global registry of in-flight training jobs. DashMap-backed so the L2
/// writer and the L3 reader touch it lock-free from different tokio tasks.
#[derive(Debug, Default)]
pub struct TrainingJobBoard {
    jobs: DashMap<Uuid, WatchedJob>,
}

static GLOBAL: OnceLock<TrainingJobBoard> = OnceLock::new();

impl TrainingJobBoard {
    /// The one board for this core. Lazily built on first touch.
    pub fn global() -> &'static TrainingJobBoard {
        GLOBAL.get_or_init(TrainingJobBoard::default)
    }

    /// Register a freshly-dispatched job to watch. Called by the L2 trigger right
    /// after `genome/job-create` succeeds. Keyed by `handle.local_id` (the stable,
    /// substrate-side correlation id), so a re-register of the same job replaces
    /// rather than duplicates.
    pub fn register(&self, job: WatchedJob) {
        self.jobs.insert(job.handle.local_id, job);
    }

    /// Snapshot every job currently being watched (cloned). The sentinel polls each
    /// of these per tick; iterating a snapshot (not the live map) keeps the tick
    /// free of held DashMap guards across `await` ([[fallbacks-are-illegal-fail-loud]]
    /// sibling: never hold a lock across await).
    pub fn snapshot(&self) -> Vec<WatchedJob> {
        self.jobs.iter().map(|e| e.value().clone()).collect()
    }

    /// Atomically remove and return a job — the sentinel's "claim" the instant it
    /// observes a terminal status, BEFORE spawning the eval chain, so no later tick
    /// re-handles it. `None` if it was already claimed.
    pub fn claim(&self, local_id: Uuid) -> Option<WatchedJob> {
        self.jobs.remove(&local_id).map(|(_, job)| job)
    }

    /// How many jobs are in flight — observability + test assertions.
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Whether any job is in flight.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watched(local_id: Uuid, provider: &str) -> WatchedJob {
        WatchedJob {
            handle: JobHandle {
                provider_id: provider.to_string(),
                provider_job_id: "job-x".to_string(),
                local_id,
            },
            persona_id: Uuid::new_v4(),
            persona_name: "asha".to_string(),
            base_model: "qwen3-coder".to_string(),
            trait_kind: "code".to_string(),
            eval_set: Some("docs/genome/coder-eval.jsonl".to_string()),
        }
    }

    // what this catches: the claim-once contract. A registered job is visible in the
    // snapshot, claim() returns it exactly once and removes it, and a second claim
    // yields None — the invariant that prevents the L3 sentinel from running the
    // eval chain twice for one completed job.
    #[test]
    fn claim_returns_a_job_exactly_once() {
        let board = TrainingJobBoard::default();
        let id = Uuid::new_v4();
        board.register(watched(id, "local-candle"));

        assert_eq!(board.len(), 1, "registered job must be in flight");
        assert_eq!(board.snapshot().len(), 1, "snapshot sees the registered job");

        let claimed = board.claim(id).expect("first claim returns the job");
        assert_eq!(claimed.handle.local_id, id);
        assert!(board.is_empty(), "claim removes the job from the board");
        assert!(
            board.claim(id).is_none(),
            "a second claim of the same job must be None — no double-processing"
        );
    }

    // what this catches: re-register replaces (keyed by local_id), so a duplicate
    // dispatch of the same correlation id never inflates the in-flight count.
    #[test]
    fn register_is_keyed_by_local_id() {
        let board = TrainingJobBoard::default();
        let id = Uuid::new_v4();
        board.register(watched(id, "openai"));
        board.register(watched(id, "openai"));
        assert_eq!(board.len(), 1, "same local_id must not duplicate");
    }
}
