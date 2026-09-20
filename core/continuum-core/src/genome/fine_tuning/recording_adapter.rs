//! [`RecordingFineTuningAdapter`] — canonical test fixture that
//! captures every `TrainingJobRequest` dispatched to it.
//!
//! Gated behind `#[cfg(any(test, feature = "test-fixtures"))]` per
//! CLAUDE.md's test-discipline doctrine: mock / stub / fake adapters
//! are NEVER linked into production binaries. The cargo feature is
//! the contract.
//!
//! ## Why this exists as substrate fixture vs inline test stub
//!
//! Before this fixture: every test that needed to verify "did the
//! trigger actually dispatch the examples I submitted?" had to roll
//! its own stub adapter inline. The previous VDD conservation test
//! in `modules::training_trigger::tests::vdd` asserted
//! `trained_tokens > 0` as a proxy for conservation — but that's
//! tautological (`trained_tokens` is a function of schedule, not
//! example count) and Reviewer 1's BLOCK M2 flagged it as such.
//!
//! The real conservation invariant is: "every example I submit
//! flows through dispatch EXACTLY ONCE, in order, with no drops or
//! duplicates." That requires capturing the dispatched
//! `TrainingDataset` and asserting it equals the submitted set.
//! Hence this fixture.
//!
//! ## Why it's canonical (single fixture per concern)
//!
//! Per the test-discipline rules: "Reusable fixtures live in one
//! place per concern. `HeuristicInferenceAdapter` is the adapter
//! fixture. `RecordingRagSource` / `ReplayRagSource` are the RAG
//! fixtures. Don't write a parallel `MockInferenceAdapter` in your
//! test file." `RecordingFineTuningAdapter` is the
//! `FineTuningAdapter` fixture. Future tests that need to capture
//! dispatched fine-tuning requests import this one — they don't
//! roll a parallel stub.
//!
//! ## Future use beyond M2
//!
//! - Fix-3's deterministic Notify-based concurrency test (the C1/C2
//!   race that the smoke-level stress test couldn't force) plugs a
//!   `tokio::sync::Notify` into this fixture's `create_job` so the
//!   test can pause dispatch deterministically while a contending
//!   submit lands.
//! - The teacher-synthesis slice (#228 next step) uses this fixture
//!   to verify the curated examples flowing OUT of teacher synthesis
//!   land in the trigger's submit path correctly.

use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use async_trait::async_trait;
use uuid::Uuid;

use super::adapter::{FineTuningAdapter, FineTuningCapabilities, FineTuningError, TrainerHardware};
use super::types::{
    ArtifactFormat, JobHandle, JobMetrics, TrainingArtifact, TrainingJobRequest, TrainingStatus,
};

/// Provider id this fixture advertises. Tests using this fixture
/// register it with this id so the coordinator picks it
/// deterministically over LocalCandleFineTuner.
pub const RECORDING_PROVIDER_ID: &str = "recording-test-fixture";

/// Base-model prefix this fixture matches. Tests using this fixture
/// submit with `base_model` starting with this prefix so coordinator
/// routing is deterministic.
pub const RECORDING_BASE_PREFIX: &str = "recording-test";

/// Test fixture: a `FineTuningAdapter` that captures every job
/// request it receives and returns a fake `JobHandle` immediately.
///
/// `Clone` because the typical test pattern is to share the
/// captures across (a) the registry registration site and (b) the
/// test body that reads them after the trigger fires.
#[derive(Clone)]
pub struct RecordingFineTuningAdapter {
    captures: Arc<StdMutex<Vec<TrainingJobRequest>>>,
}

impl RecordingFineTuningAdapter {
    pub fn new() -> Self {
        Self {
            captures: Arc::new(StdMutex::new(Vec::new())),
        }
    }

    /// Shared handle to the captures vector. Use this on the test
    /// body to read what was dispatched.
    ///
    /// ## Concurrency note
    ///
    /// `create_job` pushes via `Arc<StdMutex<Vec<_>>>::lock().push()`.
    /// Under concurrent dispatches the push order reflects mutex-
    /// acquisition order, NOT caller spawn order. Serial tests can
    /// rely on order; concurrent tests should use set-membership
    /// assertions (HashSet of prompts) or have the caller embed a
    /// sequence number in the submitted request. The
    /// `concurrent_submits_to_same_key_serialize_without_loss_stress`
    /// test in training_trigger.rs::stress demonstrates the
    /// set-membership pattern.
    pub fn captures(&self) -> Arc<StdMutex<Vec<TrainingJobRequest>>> {
        self.captures.clone()
    }

    /// Helper: number of jobs captured so far.
    pub fn captured_job_count(&self) -> usize {
        self.captures.lock().unwrap().len()
    }

    /// Helper: total examples across every captured job. Tests
    /// asserting end-to-end conservation use this to check the SUM
    /// matches what was submitted.
    pub fn captured_example_count(&self) -> usize {
        self.captures
            .lock()
            .unwrap()
            .iter()
            .map(|req| req.dataset.examples.len())
            .sum()
    }
}

impl Default for RecordingFineTuningAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FineTuningAdapter for RecordingFineTuningAdapter {
    fn capabilities(&self) -> FineTuningCapabilities {
        FineTuningCapabilities {
            provider_id: RECORDING_PROVIDER_ID.to_string(),
            supports_lora: true,
            supports_validation: false,
            // produces_local_artifact: true so the coordinator's
            // locality tie-break picks this fixture ahead of any
            // co-registered cloud adapter in tests.
            produces_local_artifact: true,
            supported_base_model_prefixes: vec![RECORDING_BASE_PREFIX.to_string()],
            // Test fixture — no real accelerator needed; selectable on
            // any host.
            requires: TrainerHardware::Any,
        }
    }

    async fn create_job(&self, request: TrainingJobRequest) -> Result<JobHandle, FineTuningError> {
        self.captures.lock().unwrap().push(request.clone());
        Ok(JobHandle {
            provider_id: RECORDING_PROVIDER_ID.to_string(),
            provider_job_id: format!("recording-{}", Uuid::new_v4()),
            local_id: Uuid::new_v4(),
        })
    }

    async fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
        Ok(TrainingStatus::Completed {
            artifact: TrainingArtifact {
                model_id: handle.provider_job_id.clone(),
                local_path: None,
                // Replay fixture — no real weights, so nothing to convert or
                // page in. Provider-hosted keeps the sentinel from expecting a
                // local gene.
                format: ArtifactFormat::ProviderHosted,
                metrics: JobMetrics::default(),
            },
        })
    }

    async fn cancel(&self, _handle: &JobHandle) -> Result<(), FineTuningError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::fine_tuning::types::{TrainingDataset, TrainingExample, TrainingSource};

    fn ex(p: &str, c: &str) -> TrainingExample {
        TrainingExample {
            prompt: p.into(),
            completion: c.into(),
            metadata: None,
        }
    }

    fn req(prompt: &str) -> TrainingJobRequest {
        TrainingJobRequest {
            persona_id: Uuid::nil(),
            persona_name: "t".into(),
            base_model: "recording-test".into(),
            trait_kind: "t".into(),
            dataset: TrainingDataset {
                examples: vec![ex(prompt, "c")],
                source: TrainingSource::OperatorCurated,
                validation_split: 0.0,
            },
            eval_set: None,
            lora: None,
            schedule: None,
            local_artifact_dir: None,
        }
    }

    // what this catches: the fixture captures EVERY create_job call
    // and exposes them via captures() in the order they were
    // submitted. Downstream consumers (the conservation VDD test in
    // training_trigger) depend on this ordering to verify "no
    // examples lost or reordered across the boundary."
    #[tokio::test]
    async fn captures_jobs_in_submission_order() {
        let adapter = RecordingFineTuningAdapter::new();
        let _ = adapter.create_job(req("first")).await.unwrap();
        let _ = adapter.create_job(req("second")).await.unwrap();
        let _ = adapter.create_job(req("third")).await.unwrap();

        let captures = adapter.captures();
        let guard = captures.lock().unwrap();
        assert_eq!(guard.len(), 3);
        assert_eq!(guard[0].dataset.examples[0].prompt, "first");
        assert_eq!(guard[1].dataset.examples[0].prompt, "second");
        assert_eq!(guard[2].dataset.examples[0].prompt, "third");
    }

    // what this catches: capabilities advertise EXACTLY the
    // recording-test prefix so coordinator routing in tests is
    // deterministic. A regression flipping the prefix or adding a
    // wildcard would make co-registered adapters in tests
    // non-deterministic.
    #[test]
    fn capabilities_advertise_recording_prefix_only() {
        let caps = RecordingFineTuningAdapter::new().capabilities();
        assert_eq!(caps.provider_id, RECORDING_PROVIDER_ID);
        assert_eq!(
            caps.supported_base_model_prefixes,
            vec![RECORDING_BASE_PREFIX.to_string()]
        );
        assert!(caps.produces_local_artifact);
        assert!(caps.supports_lora);
    }

    // what this catches: captured_example_count sums across all
    // captured jobs — used by the conservation VDD test as the
    // "dispatched" side of the count vs "submitted" comparison.
    #[tokio::test]
    async fn captured_example_count_sums_across_jobs() {
        let adapter = RecordingFineTuningAdapter::new();
        let mut req_a = req("a");
        req_a.dataset.examples = vec![ex("a-1", "x"), ex("a-2", "x"), ex("a-3", "x")];
        let mut req_b = req("b");
        req_b.dataset.examples = vec![ex("b-1", "y"), ex("b-2", "y")];

        let _ = adapter.create_job(req_a).await.unwrap();
        let _ = adapter.create_job(req_b).await.unwrap();

        assert_eq!(adapter.captured_job_count(), 2);
        assert_eq!(adapter.captured_example_count(), 5);
    }
}
