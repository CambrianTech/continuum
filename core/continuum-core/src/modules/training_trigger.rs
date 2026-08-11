//! `TrainingTriggerModule` — substrate-native batching coordinator
//! that sits between curriculum producers (the teacher persona's
//! synthesis, the hippocampus's noteworthy drain, operator submits)
//! and the [`super::genome::GenomeModule`]'s `genome/job-create`
//! command.
//!
//! ## Why this module exists
//!
//! Per `[[teacher-synthesizes-in-academy-like-dreaming]]` +
//! `[[noteworthy-flag-feeds-memory-AND-curriculum]]`, examples
//! arrive in dribs — one engram at a time, one synthesis call at a
//! time. Firing `genome/job-create` per example would be wasteful;
//! waiting for the producer to accumulate a "right-sized" batch
//! pushes batching policy out of the substrate and into N callers
//! that would each implement it slightly differently. The substrate
//! should own that policy ONCE, here.
//!
//! ## What it does
//!
//! Owns a per-`(persona_id, trait_kind, base_model)` bucket of
//! accumulating [`TrainingExample`]s. The three verbs —
//! `genome/training-trigger/submit`, `/flush`, `/status` — are
//! migrated to the typed [`DynCommand`](crate::sdk_codegen::DynCommand)
//! registry under `commands/training_trigger/` (task #62). This module
//! now retains only the shared [`TrainingTriggerState`] (the buckets,
//! the per-key submit gate, the late-bound executor + the
//! `dispatch_job_create` helper); the verbs are dep-holding commands
//! built over that one `Arc<TrainingTriggerState>` so submit and flush
//! serialize on the SAME `PerKeyGate` and mutate the SAME buckets.
//! Its legacy `handle_command` arms now fail loud.
//!
//! ## Replaces what was deleted in #1572
//!
//! The TS-side `channel.rs` trigger this replaces was the
//! cautionary-tale shape:
//!
//! - Fired on raw chat events (wrong signal — chat isn't curated
//!   experience).
//! - Sent minimal params; the validator silently rejected.
//! - Fire-and-forget call site swallowed the rejection.
//!
//! This module is the opposite: substrate-native command surface,
//! typed inputs validated synchronously, typed outcome on the
//! return path, batch state is NEVER lost on dispatch failure (per
//! `[[no-fallbacks-ever]]`).
//!
//! ## Doctrinal alignment
//!
//! - `[[commands-are-dumb-daemons-are-smart]]` — the submit command
//!   is the dumb door; the smart bits (batching, threshold logic,
//!   dispatch) live in [`TrainingTriggerState`].
//! - `[[no-fallbacks-ever]]` — every code path returns a typed
//!   outcome or a typed error. Dispatch failure preserves bucket
//!   contents so the next caller's submit can re-trigger.
//! - `[[rust-is-the-core-node-is-the-shell]]` — entire path is
//!   substrate-side. The teacher persona (Rust) submits; the trigger
//!   (Rust) batches; the genome module (Rust) dispatches; the local
//!   trainer (Rust) produces the safetensors.

use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use uuid::Uuid;

use crate::genome::fine_tuning::types::{
    JobHandle, LoRAHyperparams, ScheduleParams, TrainingDataset, TrainingExample,
    TrainingJobRequest, TrainingSource,
};
use crate::runtime::{
    CommandExecutor, CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority,
    PerKeyGate, ServiceModule,
};
use crate::sdk_codegen::DynCommand;

/// Default per-bucket fire threshold. 16 examples is a healthy
/// LoRA-training floor — large enough to give SGD signal,
/// small enough that latency-to-first-layer stays minutes not hours
/// on the substrate-native trainer. Override per-submit via
/// `SubmitParams::min_examples`.
pub const DEFAULT_MIN_EXAMPLES: u32 = 16;

/// Default `validation_split` when the submit doesn't pin one. 0.0
/// is conservative — substrate-native datasets are often small
/// enough that a held-out split hurts more than helps; producers
/// who know their data should override.
pub const DEFAULT_VALIDATION_SPLIT: f32 = 0.0;

// ─── Bucket key + state ──────────────────────────────────────────────

/// One bucket per `(persona_id, trait_kind, base_model)` triple.
///
/// Per Reviewer 2's BLOCK A4: keying on `(persona_id, trait_kind)`
/// only and rejecting the second submit with a different `base_model`
/// is the silent-data-loss class the trigger took pains to prevent
/// everywhere else. A persona legitimately trains the same trait
/// against multiple bases (local + cloud) for routing flexibility;
/// each base gets its own bucket and its own dispatched job. The key
/// IS the coherence guarantee — no runtime check needed.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct BucketKey {
    pub(crate) persona_id: Uuid,
    pub(crate) trait_kind: String,
    pub(crate) base_model: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingBatch {
    pub(crate) persona_name: String,
    pub(crate) source: TrainingSource,
    pub(crate) examples: Vec<TrainingExample>,
    pub(crate) lora: Option<LoRAHyperparams>,
    pub(crate) schedule: Option<ScheduleParams>,
    pub(crate) local_artifact_dir: Option<PathBuf>,
    pub(crate) preferred_provider: Option<String>,
    pub(crate) min_examples: u32,
    pub(crate) validation_split: f32,
    /// The gym that MEASURES this bucket's trait — the `cognition/eval` `eval_set`
    /// JSONL path, carried verbatim onto the dispatched
    /// [`TrainingJobRequest::eval_set`] so it rides the board to the L3 sentinel. A
    /// first-arrival-wins bucket policy field like `lora`/`schedule`: a later submit
    /// with a divergent gym is rejected `InconsistentBucket`, never silently merged.
    /// `None` means the recipe declared no gym — the sentinel then REFUSES to adopt
    /// rather than measuring against a default ([[fallbacks-are-illegal-fail-loud]]).
    pub(crate) eval_set: Option<String>,
}

/// The shared state the three `genome/training-trigger/*` commands
/// operate over. Held by [`TrainingTriggerModule`] as one
/// `Arc<TrainingTriggerState>` and cloned into each dep-holding
/// command object via [`commands`](TrainingTriggerModule::commands),
/// so submit and flush serialize on the SAME [`PerKeyGate`] and touch
/// the SAME buckets. The late-bound executor is installed once
/// (`install_executor`) and reaches every command through the shared
/// `Arc`.
pub struct TrainingTriggerState {
    pub(crate) buckets: Arc<DashMap<BucketKey, PendingBatch>>,
    /// Per-`(persona_id, trait_kind, base_model)` submit
    /// serialization, backed by the substrate-canonical
    /// [`PerKeyGate`] primitive (`runtime/per_key_gate.rs`). Per
    /// Reviewer 3's BLOCK C1+C2 on PR #1580: concurrent submits
    /// to the same key MUST serialize so the drain→dispatch→remove
    /// path doesn't race a contending submit's append. The gate
    /// auto-evicts via `try_evict` on success — cold keys do not
    /// accumulate (closes the [[auto-clean-is-structural-not-operational]]
    /// concern).
    pub(crate) submit_gates: PerKeyGate<BucketKey>,
    pub(crate) executor: LateBound<CommandExecutor>,
}

impl TrainingTriggerState {
    pub(crate) fn new() -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            submit_gates: PerKeyGate::new(),
            executor: LateBound::new("training-trigger::executor"),
        }
    }

    /// Test-only: count of currently-pending buckets. Useful for
    /// asserting "bucket cleared after dispatch" without exposing
    /// internal state to production callers.
    #[cfg(test)]
    pub(crate) fn pending_bucket_count(&self) -> usize {
        self.buckets.len()
    }

    /// Test-only: peek the example count for a specific bucket. None
    /// if the bucket doesn't exist (cleared or never created).
    #[cfg(test)]
    pub(crate) fn bucket_example_count(
        &self,
        persona_id: Uuid,
        trait_kind: &str,
        base_model: &str,
    ) -> Option<usize> {
        let key = BucketKey {
            persona_id,
            trait_kind: trait_kind.to_string(),
            base_model: base_model.to_string(),
        };
        self.buckets.get(&key).map(|b| b.examples.len())
    }

    /// Build a `TrainingJobRequest` from a drained `PendingBatch`,
    /// dispatch `genome/job-create` via the late-bound executor, and
    /// return the (job_handle, selected_provider) pair on success.
    /// Shared by submit and flush so both fire through exactly one
    /// dispatch path.
    pub(crate) async fn dispatch_job_create(
        &self,
        persona_id: Uuid,
        trait_kind: &str,
        base_model: &str,
        batch: &PendingBatch,
    ) -> Result<(JobHandle, String), String> {
        let executor = self.executor.require()?;

        let request = TrainingJobRequest {
            persona_id,
            persona_name: batch.persona_name.clone(),
            base_model: base_model.to_string(),
            trait_kind: trait_kind.to_string(),
            dataset: TrainingDataset {
                examples: batch.examples.clone(),
                source: batch.source,
                validation_split: batch.validation_split,
            },
            eval_set: batch.eval_set.clone(),
            lora: batch.lora.clone(),
            schedule: batch.schedule.clone(),
            local_artifact_dir: batch.local_artifact_dir.clone(),
        };

        let mut params = serde_json::to_value(&request)
            .map_err(|e| format!("serialize TrainingJobRequest: {e}"))?;
        if let Some(provider) = &batch.preferred_provider {
            if let Value::Object(ref mut map) = params {
                map.insert("preferredProvider".into(), Value::String(provider.clone()));
            }
        }

        let response = executor
            .execute_json("genome/job-create", params)
            .await
            .map_err(|e| format!("genome/job-create dispatch: {e}"))?;

        // Unwrap the GenomeModule envelope: { success, result: { handle, selectedProvider } }
        // or { success: false, error, errorKind }.
        let success = response
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !success {
            let err = response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("(no error message)")
                .to_string();
            return Err(format!("genome/job-create rejected: {err}"));
        }
        let result = response
            .get("result")
            .ok_or_else(|| "genome/job-create returned success without result".to_string())?;
        let handle_value = result
            .get("handle")
            .cloned()
            .ok_or_else(|| "genome/job-create result missing handle".to_string())?;
        let handle: JobHandle = serde_json::from_value(handle_value)
            .map_err(|e| format!("genome/job-create handle parse: {e}"))?;
        let selected_provider = result
            .get("selectedProvider")
            .and_then(Value::as_str)
            .ok_or_else(|| "genome/job-create result missing selectedProvider".to_string())?
            .to_string();

        // L2→L3 retention (the board write) lives at the ONE birth-seam every
        // training job funnels through — the `genome/job-create` command body that
        // this helper dispatches to above — NOT here. That keeps a single
        // registration point for the trigger path, a direct `uu genome/job-create`,
        // and any future caller alike (compression principle), instead of one writer
        // per caller that silently misses jobs born off this path.
        // ([[dev-task-learning-loop-gap-map]] L3, docs/genome/DEV-TASK-LOOP-CLOSURE-PLAN.md)
        Ok((handle, selected_provider))
    }
}

// ─── Module ──────────────────────────────────────────────────────────

pub struct TrainingTriggerModule {
    pub(crate) state: Arc<TrainingTriggerState>,
}

impl TrainingTriggerModule {
    pub fn new() -> Self {
        Self {
            state: Arc::new(TrainingTriggerState::new()),
        }
    }
}

impl Default for TrainingTriggerModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for TrainingTriggerModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "training-trigger",
            priority: ModulePriority::Normal,
            command_prefixes: &["genome/training-trigger/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    /// Expose the three dep-holding training-trigger verbs over this
    /// module's shared [`TrainingTriggerState`] — submit / flush /
    /// status all bind the SAME buckets + per-key gate.
    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        crate::commands::training_trigger::command_objects(self.state.clone())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // All three verbs are migrated to the typed registry
        // (commands/training_trigger/). They route via `route_object`
        // against THIS module's shared state (contributed by
        // `commands()`); the legacy path must fail loud.
        match command {
            "genome/training-trigger/submit"
            | "genome/training-trigger/flush"
            | "genome/training-trigger/status" => Err(format!(
                "'{command}' is migrated to the typed registry \
                 (commands/training_trigger/) — it must route via route_object, \
                 not the legacy handle_command path"
            )),
            other => Err(format!("unknown training-trigger command: {other}")),
        }
    }

    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        self.state.executor.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: every migrated verb now fails loud through the legacy
    // path, naming itself + pointing at the typed registry (no silent success that
    // would mask a routing regression). The behavioral tests live in the command
    // files (commands/training_trigger/*).
    #[tokio::test]
    async fn migrated_arms_fail_loud() {
        let module = TrainingTriggerModule::new();
        for command in [
            "genome/training-trigger/submit",
            "genome/training-trigger/flush",
            "genome/training-trigger/status",
        ] {
            let err = module
                .handle_command(command, Value::Null)
                .await
                .expect_err("migrated arm must fail loud");
            assert!(err.contains("migrated"), "for {command}: {err}");
            assert!(err.contains(command), "for {command}: {err}");
        }
    }

    // what this catches: the module contributes the three dep-holding verbs to the
    // typed object map (sharing its one state). A regression that drops the
    // `commands()` override — leaving them unroutable — is caught.
    #[test]
    fn contributes_the_typed_training_trigger_commands() {
        let module = TrainingTriggerModule::new();
        let names: Vec<&str> = module.commands().iter().map(|c| c.name()).collect();
        assert!(names.contains(&"genome/training-trigger/submit"));
        assert!(names.contains(&"genome/training-trigger/flush"));
        assert!(names.contains(&"genome/training-trigger/status"));
    }

    // what this catches: an unmigrated/unknown verb still errors (not a panic, not
    // a silent ok).
    #[tokio::test]
    async fn unknown_command_errors() {
        let module = TrainingTriggerModule::new();
        let result = module
            .handle_command("genome/training-trigger/nope", Value::Null)
            .await;
        assert!(result.is_err());
    }
}
