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
//! Owns a per-`(persona_id, trait_kind)` bucket of accumulating
//! [`TrainingExample`]s. Each `genome/training-trigger/submit`:
//!
//! 1. Validates the submission against any existing bucket
//!    (same `base_model`, same `trait_kind` → coherent training
//!    target).
//! 2. Appends examples to the bucket.
//! 3. If the bucket has reached the per-bucket `min_examples`
//!    threshold, builds a [`TrainingJobRequest`], dispatches
//!    `genome/job-create` via the injected [`CommandExecutor`], and
//!    clears the bucket on success.
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
//!   dispatch) live in the module.
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
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::genome::fine_tuning::types::{
    LoRAHyperparams, ScheduleParams, TrainingDataset, TrainingExample, TrainingJobRequest,
    TrainingSource,
};
use crate::runtime::{
    CommandExecutor, CommandResult, ModuleConfig, ModuleContext, ModulePriority, PerKeyGate,
    ServiceModule,
};

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

// ─── Wire shapes ─────────────────────────────────────────────────────

/// `genome/training-trigger/submit` input.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubmitParams {
    persona_id: Uuid,
    persona_name: String,
    base_model: String,
    trait_kind: String,
    examples: Vec<TrainingExample>,
    source: TrainingSource,
    #[serde(default)]
    lora: Option<LoRAHyperparams>,
    #[serde(default)]
    schedule: Option<ScheduleParams>,
    #[serde(default)]
    local_artifact_dir: Option<PathBuf>,
    #[serde(default)]
    preferred_provider: Option<String>,
    /// Override the per-bucket fire threshold. None → DEFAULT_MIN_EXAMPLES.
    #[serde(default)]
    min_examples: Option<u32>,
    /// Override validation split. None → DEFAULT_VALIDATION_SPLIT.
    #[serde(default)]
    validation_split: Option<f32>,
}

/// `genome/training-trigger/flush` input.
///
/// `base_model` is required so the flush targets a specific bucket
/// (per the BucketKey-includes-base_model fix). A persona may have
/// multiple (trait_kind, base_model) buckets pending; flush picks
/// exactly one.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlushParams {
    persona_id: Uuid,
    trait_kind: String,
    base_model: String,
}

/// `genome/training-trigger/status` — no params; returns all
/// pending buckets.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingBucketView {
    persona_id: Uuid,
    persona_name: String,
    trait_kind: String,
    base_model: String,
    examples_pending: u32,
    min_examples: u32,
}

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
struct BucketKey {
    persona_id: Uuid,
    trait_kind: String,
    base_model: String,
}

#[derive(Debug, Clone)]
struct PendingBatch {
    persona_name: String,
    source: TrainingSource,
    examples: Vec<TrainingExample>,
    lora: Option<LoRAHyperparams>,
    schedule: Option<ScheduleParams>,
    local_artifact_dir: Option<PathBuf>,
    preferred_provider: Option<String>,
    min_examples: u32,
    validation_split: f32,
}

// ─── Module ──────────────────────────────────────────────────────────

pub struct TrainingTriggerModule {
    buckets: Arc<DashMap<BucketKey, PendingBatch>>,
    /// Per-`(persona_id, trait_kind, base_model)` submit
    /// serialization, backed by the substrate-canonical
    /// [`PerKeyGate`] primitive (`runtime/per_key_gate.rs`). Per
    /// Reviewer 3's BLOCK C1+C2 on PR #1580: concurrent submits
    /// to the same key MUST serialize so the drain→dispatch→remove
    /// path doesn't race a contending submit's append. The gate
    /// auto-evicts via `try_evict` on success — cold keys do not
    /// accumulate (closes the [[auto-clean-is-structural-not-operational]]
    /// concern).
    submit_gates: PerKeyGate<BucketKey>,
    executor: std::sync::OnceLock<Arc<CommandExecutor>>,
}

impl TrainingTriggerModule {
    pub fn new() -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            submit_gates: PerKeyGate::new(),
            executor: std::sync::OnceLock::new(),
        }
    }

    /// Test-only: count of currently-pending buckets. Useful for
    /// asserting "bucket cleared after dispatch" without exposing
    /// internal state to production callers.
    #[cfg(test)]
    pub(super) fn pending_bucket_count(&self) -> usize {
        self.buckets.len()
    }

    /// Test-only: peek the example count for a specific bucket. None
    /// if the bucket doesn't exist (cleared or never created).
    #[cfg(test)]
    pub(super) fn bucket_example_count(
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

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "genome/training-trigger/submit" => self.handle_submit(params).await,
            "genome/training-trigger/flush" => self.handle_flush(params).await,
            "genome/training-trigger/status" => self.handle_status(params).await,
            other => Err(format!("unknown training-trigger command: {other}")),
        }
    }

    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        let _ = self.executor.set(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl TrainingTriggerModule {
    async fn handle_submit(&self, params: Value) -> Result<CommandResult, String> {
        let p: SubmitParams = serde_json::from_value(params)
            .map_err(|e| format!("invalid submit params: {e}"))?;

        // Validation that fails synchronously — caller mistake, not
        // worth a typed JSON outcome (these are programmer-facing).
        if p.persona_name.trim().is_empty() {
            return Err("persona_name must be non-empty".into());
        }
        if p.base_model.trim().is_empty() {
            return Err("base_model must be non-empty".into());
        }
        if p.trait_kind.trim().is_empty() {
            return Err("trait_kind must be non-empty".into());
        }
        if p.examples.is_empty() {
            return Err("examples must be non-empty".into());
        }
        let min_examples = p.min_examples.unwrap_or(DEFAULT_MIN_EXAMPLES).max(1);
        let validation_split = p.validation_split.unwrap_or(DEFAULT_VALIDATION_SPLIT);

        let key = BucketKey {
            persona_id: p.persona_id,
            trait_kind: p.trait_kind.clone(),
            base_model: p.base_model.clone(),
        };

        // Serialize per-key. Concurrent submits to different keys
        // proceed in parallel; concurrent submits to the same key
        // queue here. This eliminates the lost-update + restore-
        // commingle races flagged in Reviewer 3's BLOCK C1 + C2.
        // Holding the guard across the .await is intentional —
        // tokio::sync::Mutex is await-safe.
        let gate = self.submit_gates.acquire(&key);
        let _submit_lease = gate.lock().await;

        // Append phase. Hold the entry mutably for the minimum window:
        // append + read the new len + decide whether to fire. If we
        // fire, take the examples out (clear the bucket atomically)
        // and drop the entry guard BEFORE awaiting the dispatch.
        let snapshot_to_dispatch = {
            let mut entry = self.buckets.entry(key.clone()).or_insert_with(|| PendingBatch {
                persona_name: p.persona_name.clone(),
                source: p.source,
                examples: Vec::new(),
                lora: p.lora.clone(),
                schedule: p.schedule.clone(),
                local_artifact_dir: p.local_artifact_dir.clone(),
                preferred_provider: p.preferred_provider.clone(),
                min_examples,
                validation_split,
            });

            // Coherence checks for hyperparam fields NOT in the
            // BucketKey. Per Reviewer 2's BLOCK B2 (re-review): the
            // A4 fix promoted `base_model` into the key but left
            // `source`, `lora`, `schedule`, `validation_split`,
            // `local_artifact_dir`, `preferred_provider` as
            // first-arrival-wins via `or_insert_with`. A second
            // submit with `lora.rank=16` against a bucket that the
            // first submit pinned at `lora.rank=8` would silently
            // dispatch with rank=8 — same silent-data-corruption
            // class A4 was trying to eliminate.
            //
            // Symmetric rejection: any policy field that differs
            // from the bucket's first-arrival value surfaces as
            // InconsistentBucket. The caller picks one of: re-submit
            // with the original policy, flush the bucket and start
            // fresh, or pick a different (persona, trait, base)
            // discriminator that splits the policies.
            if entry.source != p.source {
                return Ok(CommandResult::Json(json!({
                    "success": false,
                    "error": format!(
                        "bucket has source={:?}; submit gave source={:?}",
                        entry.source, p.source
                    ),
                    "errorKind": "InconsistentBucket",
                })));
            }
            if entry.lora != p.lora {
                return Ok(CommandResult::Json(json!({
                    "success": false,
                    "error": format!(
                        "bucket has lora={:?}; submit gave lora={:?}",
                        entry.lora, p.lora
                    ),
                    "errorKind": "InconsistentBucket",
                })));
            }
            if entry.schedule != p.schedule {
                return Ok(CommandResult::Json(json!({
                    "success": false,
                    "error": format!(
                        "bucket has schedule={:?}; submit gave schedule={:?}",
                        entry.schedule, p.schedule
                    ),
                    "errorKind": "InconsistentBucket",
                })));
            }
            if (entry.validation_split - validation_split).abs() > f32::EPSILON {
                return Ok(CommandResult::Json(json!({
                    "success": false,
                    "error": format!(
                        "bucket has validation_split={}; submit gave validation_split={}",
                        entry.validation_split, validation_split
                    ),
                    "errorKind": "InconsistentBucket",
                })));
            }
            if entry.local_artifact_dir != p.local_artifact_dir {
                return Ok(CommandResult::Json(json!({
                    "success": false,
                    "error": format!(
                        "bucket has local_artifact_dir={:?}; submit gave local_artifact_dir={:?}",
                        entry.local_artifact_dir, p.local_artifact_dir
                    ),
                    "errorKind": "InconsistentBucket",
                })));
            }
            if entry.preferred_provider != p.preferred_provider {
                return Ok(CommandResult::Json(json!({
                    "success": false,
                    "error": format!(
                        "bucket has preferred_provider={:?}; submit gave preferred_provider={:?}",
                        entry.preferred_provider, p.preferred_provider
                    ),
                    "errorKind": "InconsistentBucket",
                })));
            }

            entry.examples.extend(p.examples.into_iter());

            // Allow per-submit threshold downgrade — operator
            // tooling pumping in known-final batches can force-fire
            // a smaller bucket by passing min_examples = 1.
            if min_examples < entry.min_examples {
                entry.min_examples = min_examples;
            }

            let current_count = entry.examples.len() as u32;
            if current_count < entry.min_examples {
                return Ok(CommandResult::Json(json!({
                    "success": true,
                    "outcome": "BatchAppended",
                    "currentCount": current_count,
                    "threshold": entry.min_examples,
                })));
            }

            // Threshold reached — drain the bucket into a snapshot
            // we can dispatch outside the entry guard. Insert remains
            // in the map (empty) until we know whether dispatch
            // succeeded; on success we'll remove it.
            let drained_examples = std::mem::take(&mut entry.examples);
            PendingBatch {
                persona_name: entry.persona_name.clone(),
                source: entry.source,
                examples: drained_examples,
                lora: entry.lora.clone(),
                schedule: entry.schedule.clone(),
                local_artifact_dir: entry.local_artifact_dir.clone(),
                preferred_provider: entry.preferred_provider.clone(),
                min_examples: entry.min_examples,
                validation_split: entry.validation_split,
            }
        };

        // Dispatch under the per-key gate — no other submit to this
        // key can race the success-clear or failure-restore paths.
        let dispatch_result = self
            .dispatch_job_create(p.persona_id, &key.trait_kind, &key.base_model, &snapshot_to_dispatch)
            .await;

        match dispatch_result {
            Ok((job_handle_value, selected_provider)) => {
                // Remove the now-empty bucket so status() doesn't
                // report a phantom zero-count entry.
                self.buckets.remove(&key);
                // Release the lease BEFORE attempting gate cleanup —
                // cleanup checks `Arc::strong_count == 1` which only
                // holds when this caller's gate ref has dropped.
                drop(_submit_lease);
                self.submit_gates.try_evict(&key);
                Ok(CommandResult::Json(json!({
                    "success": true,
                    "outcome": "JobDispatched",
                    "examplesUsed": snapshot_to_dispatch.examples.len() as u32,
                    "selectedProvider": selected_provider,
                    "jobHandle": job_handle_value,
                })))
            }
            Err(err) => {
                // Restore drained examples. Reinsert at the FRONT so
                // submit order is preserved across retries.
                if let Some(mut entry) = self.buckets.get_mut(&key) {
                    let mut restored = snapshot_to_dispatch.examples.clone();
                    restored.extend(std::mem::take(&mut entry.examples).into_iter());
                    entry.examples = restored;
                }
                Ok(CommandResult::Json(json!({
                    "success": false,
                    "error": err,
                    "errorKind": "DispatchFailed",
                })))
            }
        }
    }

    async fn handle_flush(&self, params: Value) -> Result<CommandResult, String> {
        let p: FlushParams = serde_json::from_value(params)
            .map_err(|e| format!("invalid flush params: {e}"))?;
        let key = BucketKey {
            persona_id: p.persona_id,
            trait_kind: p.trait_kind.clone(),
            base_model: p.base_model.clone(),
        };

        // Same per-key gate as submit. Flush and submit BOTH mutate
        // the bucket — without serialization, a flush could race a
        // submit's drain or restore. The gate ensures whichever
        // arrives first runs to completion before the other touches
        // the key.
        let gate = self.submit_gates.acquire(&key);
        let _flush_lease = gate.lock().await;

        // Take the bucket out atomically. If it doesn't exist or is
        // empty, return a clean "nothing to flush" outcome — flush
        // is idempotent.
        let snapshot = match self.buckets.remove(&key) {
            Some((_, batch)) if !batch.examples.is_empty() => batch,
            Some((_, _)) => {
                return Ok(CommandResult::Json(json!({
                    "success": true,
                    "outcome": "NothingToFlush",
                })));
            }
            None => {
                return Ok(CommandResult::Json(json!({
                    "success": true,
                    "outcome": "NothingToFlush",
                })));
            }
        };

        match self
            .dispatch_job_create(p.persona_id, &key.trait_kind, &key.base_model, &snapshot)
            .await
        {
            Ok((job_handle_value, selected_provider)) => {
                // Same gate cleanup as submit's success path —
                // release lease, then try to prune the gate if no
                // other caller holds a ref.
                drop(_flush_lease);
                self.submit_gates.try_evict(&key);
                Ok(CommandResult::Json(json!({
                    "success": true,
                    "outcome": "JobDispatched",
                    "examplesUsed": snapshot.examples.len() as u32,
                    "selectedProvider": selected_provider,
                    "jobHandle": job_handle_value,
                })))
            }
            Err(err) => {
                // Restore on failure — flush must not lose data
                // either. Under the gate, no concurrent submit /
                // flush can have populated the key between our
                // remove() and this insert(), so the reinsert is
                // safe and the snapshot is the complete state.
                self.buckets.insert(key, snapshot);
                Ok(CommandResult::Json(json!({
                    "success": false,
                    "error": err,
                    "errorKind": "DispatchFailed",
                })))
            }
        }
    }

    async fn handle_status(&self, _params: Value) -> Result<CommandResult, String> {
        let mut buckets: Vec<PendingBucketView> = Vec::with_capacity(self.buckets.len());
        for entry in self.buckets.iter() {
            buckets.push(PendingBucketView {
                persona_id: entry.key().persona_id,
                persona_name: entry.value().persona_name.clone(),
                trait_kind: entry.key().trait_kind.clone(),
                base_model: entry.key().base_model.clone(),
                examples_pending: entry.value().examples.len() as u32,
                min_examples: entry.value().min_examples,
            });
        }
        // Deterministic order — sort by (persona_id, trait_kind,
        // base_model) so operator-tooling tests don't flake on
        // DashMap iteration order, and so the new (persona, trait,
        // base) triple is fully reflected in the surface contract.
        buckets.sort_by(|a, b| {
            (a.persona_id, &a.trait_kind, &a.base_model)
                .cmp(&(b.persona_id, &b.trait_kind, &b.base_model))
        });

        Ok(CommandResult::Json(json!({
            "success": true,
            "buckets": serde_json::to_value(&buckets)
                .map_err(|e| format!("serialize buckets: {e}"))?,
        })))
    }

    /// Build a `TrainingJobRequest` from a drained `PendingBatch`,
    /// dispatch `genome/job-create`, return the (job_handle_json,
    /// selected_provider) pair on success.
    async fn dispatch_job_create(
        &self,
        persona_id: Uuid,
        trait_kind: &str,
        base_model: &str,
        batch: &PendingBatch,
    ) -> Result<(Value, String), String> {
        let executor = self
            .executor
            .get()
            .ok_or_else(|| "CommandExecutor not installed yet (boot ordering)".to_string())?;

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
        let handle = result
            .get("handle")
            .cloned()
            .ok_or_else(|| "genome/job-create result missing handle".to_string())?;
        let selected_provider = result
            .get("selectedProvider")
            .and_then(Value::as_str)
            .ok_or_else(|| "genome/job-create result missing selectedProvider".to_string())?
            .to_string();
        Ok((handle, selected_provider))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::ModuleRegistry;

    fn ex(prompt: &str, completion: &str) -> TrainingExample {
        TrainingExample {
            prompt: prompt.into(),
            completion: completion.into(),
            metadata: None,
        }
    }

    fn submit_params(
        persona_id: Uuid,
        trait_kind: &str,
        examples: Vec<TrainingExample>,
        min_examples: Option<u32>,
    ) -> Value {
        let mut v = json!({
            "personaId": persona_id,
            "personaName": "test-p",
            "baseModel": "synthetic",
            "traitKind": trait_kind,
            "examples": examples,
            "source": "operator_curated",
        });
        if let Some(min) = min_examples {
            v.as_object_mut().unwrap().insert("minExamples".into(), json!(min));
        }
        v
    }

    /// Install an executor that has BOTH the trigger module AND the
    /// genome module wired, so dispatch end-to-end (submit →
    /// genome/job-create → LocalCandleFineTuner) actually runs.
    async fn build_runtime_with_trigger_and_genome() -> (Arc<TrainingTriggerModule>, Arc<CommandExecutor>) {
        use crate::genome::fine_tuning::{FineTuningRegistry, LocalCandleFineTuner};
        use crate::modules::genome::GenomeModule;

        let registry = Arc::new(ModuleRegistry::new());
        let trigger = Arc::new(TrainingTriggerModule::new());
        registry.register(trigger.clone());

        let ft_registry = Arc::new(FineTuningRegistry::new());
        ft_registry.register(Arc::new(LocalCandleFineTuner::new()));
        registry.register(Arc::new(GenomeModule::new(ft_registry)));

        let executor = Arc::new(CommandExecutor::new(registry.clone()));
        registry.install_executor_on_all(executor.clone());
        (trigger, executor)
    }

    // what this catches: under the threshold, submit returns
    // BatchAppended and the examples accumulate in the bucket. A
    // regression that fired job-create on every submit would surface
    // as JobDispatched on the very first call.
    #[tokio::test]
    async fn submit_below_threshold_appends_and_does_not_fire() {
        let (trigger, _executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let params = submit_params(persona, "test-trait", vec![ex("a", "b")], Some(5));
        let result = trigger
            .handle_command("genome/training-trigger/submit", params)
            .await
            .expect("submit ok");

        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        assert_eq!(json["success"], true);
        assert_eq!(json["outcome"], "BatchAppended");
        assert_eq!(json["currentCount"], 1);
        assert_eq!(json["threshold"], 5);
        assert_eq!(trigger.bucket_example_count(persona, "test-trait", "synthetic"), Some(1));
    }

    // what this catches: hitting threshold dispatches and clears the
    // bucket. Without this, examples would pile up forever and the
    // substrate's training loop would never close.
    #[tokio::test]
    async fn submit_at_threshold_dispatches_and_clears() {
        let (trigger, _executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        // First submit: 4 examples, threshold 5 → BatchAppended.
        let _ = trigger
            .handle_command(
                "genome/training-trigger/submit",
                submit_params(
                    persona,
                    "test-trait",
                    vec![ex("a", "b"), ex("c", "d"), ex("e", "f"), ex("g", "h")],
                    Some(5),
                ),
            )
            .await
            .unwrap();
        assert_eq!(trigger.bucket_example_count(persona, "test-trait", "synthetic"), Some(4));

        // Second submit: 1 more example → 5 → fires.
        let result = trigger
            .handle_command(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("i", "j")], Some(5)),
            )
            .await
            .unwrap();
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        assert_eq!(json["success"], true, "submit must succeed, got: {json}");
        assert_eq!(json["outcome"], "JobDispatched");
        assert_eq!(json["examplesUsed"], 5);
        assert_eq!(json["selectedProvider"], "local-candle");
        assert!(json["jobHandle"]["localId"].is_string());

        // Bucket must be cleared.
        assert_eq!(trigger.bucket_example_count(persona, "test-trait", "synthetic"), None);
        assert_eq!(trigger.pending_bucket_count(), 0);
    }

    // what this catches: same persona + same trait_kind submitted
    // with DIFFERENT base_model values gets DIFFERENT buckets. Per
    // Reviewer 2's BLOCK A4: a persona legitimately trains the same
    // trait (e.g. "kc-tech-history") against multiple bases (local
    // synthetic + cloud gpt-4o-mini) for routing flexibility. The
    // pre-fix behavior rejected the second submit with
    // InconsistentBucket and silently dropped its data. The fix
    // promotes base_model into the bucket key so the two submits
    // accumulate independently.
    #[tokio::test]
    async fn different_base_models_create_separate_buckets() {
        let (trigger, _executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        // First submit: base_model = "synthetic".
        let _ = trigger
            .handle_command(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("a", "b")], Some(100)),
            )
            .await
            .unwrap();

        // Second submit: SAME persona, SAME trait, DIFFERENT base.
        let mut other_base =
            submit_params(persona, "test-trait", vec![ex("c", "d")], Some(100));
        other_base.as_object_mut().unwrap().insert(
            "baseModel".into(),
            Value::String("synthetic-tiny".into()),
        );
        let result = trigger
            .handle_command("genome/training-trigger/submit", other_base)
            .await
            .unwrap();
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        // No more InconsistentBucket — the second submit succeeds
        // because it lives in its own bucket.
        assert_eq!(json["success"], true, "second-base submit must succeed: {json}");
        assert_eq!(json["outcome"], "BatchAppended");

        // Two distinct buckets pending, one example each.
        assert_eq!(
            trigger.bucket_example_count(persona, "test-trait", "synthetic"),
            Some(1)
        );
        assert_eq!(
            trigger.bucket_example_count(persona, "test-trait", "synthetic-tiny"),
            Some(1)
        );
        assert_eq!(trigger.pending_bucket_count(), 2);
    }

    // what this catches: coherence check now also covers `lora`
    // hyperparameters. Per Reviewer 2's re-review BLOCK B2: A4's
    // "the key IS coherence" only held for fields IN the key.
    // `lora.rank`, `lora.alpha`, etc. were silently first-arrival-
    // wins via `or_insert_with`. A second submit with different
    // lora params now gets InconsistentBucket so the caller learns
    // their config was rejected instead of silently overridden.
    #[tokio::test]
    async fn inconsistent_lora_in_same_bucket_is_rejected() {
        use crate::genome::fine_tuning::types::LoRAHyperparams;
        let (trigger, _executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let mut first =
            submit_params(persona, "test-trait", vec![ex("a", "b")], Some(100));
        first.as_object_mut().unwrap().insert(
            "lora".into(),
            serde_json::to_value(LoRAHyperparams {
                rank: 8,
                alpha: 16,
                dropout: 0.0,
                target_modules: vec![],
            })
            .unwrap(),
        );
        let _ = trigger
            .handle_command("genome/training-trigger/submit", first)
            .await
            .unwrap();

        let mut wrong_lora =
            submit_params(persona, "test-trait", vec![ex("c", "d")], Some(100));
        wrong_lora.as_object_mut().unwrap().insert(
            "lora".into(),
            serde_json::to_value(LoRAHyperparams {
                rank: 16, // different
                alpha: 32,
                dropout: 0.0,
                target_modules: vec![],
            })
            .unwrap(),
        );
        let result = trigger
            .handle_command("genome/training-trigger/submit", wrong_lora)
            .await
            .unwrap();
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        assert_eq!(json["success"], false, "got: {json}");
        assert_eq!(json["errorKind"], "InconsistentBucket");
        // First-arrival's bucket survives intact.
        assert_eq!(
            trigger.bucket_example_count(persona, "test-trait", "synthetic"),
            Some(1)
        );
    }

    // what this catches: same coherence guarantee for ScheduleParams.
    // A submit with different `epochs` or `learning_rate` is
    // rejected with InconsistentBucket rather than silently using
    // the first-arrival's schedule.
    #[tokio::test]
    async fn inconsistent_schedule_in_same_bucket_is_rejected() {
        use crate::genome::fine_tuning::types::ScheduleParams;
        let (trigger, _executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let mut first =
            submit_params(persona, "test-trait", vec![ex("a", "b")], Some(100));
        first.as_object_mut().unwrap().insert(
            "schedule".into(),
            serde_json::to_value(ScheduleParams {
                epochs: 3,
                batch_size: 4,
                sequence_length: 32,
                learning_rate: 1e-4,
            })
            .unwrap(),
        );
        let _ = trigger
            .handle_command("genome/training-trigger/submit", first)
            .await
            .unwrap();

        let mut wrong_schedule =
            submit_params(persona, "test-trait", vec![ex("c", "d")], Some(100));
        wrong_schedule.as_object_mut().unwrap().insert(
            "schedule".into(),
            serde_json::to_value(ScheduleParams {
                epochs: 5, // different
                batch_size: 4,
                sequence_length: 32,
                learning_rate: 1e-4,
            })
            .unwrap(),
        );
        let result = trigger
            .handle_command("genome/training-trigger/submit", wrong_schedule)
            .await
            .unwrap();
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        assert_eq!(json["success"], false);
        assert_eq!(json["errorKind"], "InconsistentBucket");
    }

    // what this catches: source coherence is still enforced even
    // after base_model moved out of the coherence check into the
    // key. A submit with `source: "operator_curated"` to a bucket
    // that already exists from a `source: "teacher_synthesized"`
    // submit must be rejected — the alloy provenance contract
    // distinguishes those origins.
    #[tokio::test]
    async fn inconsistent_source_in_same_bucket_is_rejected() {
        let (trigger, _executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let _ = trigger
            .handle_command(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("a", "b")], Some(100)),
            )
            .await
            .unwrap();

        let mut wrong_source =
            submit_params(persona, "test-trait", vec![ex("c", "d")], Some(100));
        wrong_source
            .as_object_mut()
            .unwrap()
            .insert("source".into(), Value::String("teacher_synthesized".into()));
        let result = trigger
            .handle_command("genome/training-trigger/submit", wrong_source)
            .await
            .unwrap();
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        assert_eq!(json["success"], false);
        assert_eq!(json["errorKind"], "InconsistentBucket");
        // First-arrival's source survives intact.
        assert_eq!(
            trigger.bucket_example_count(persona, "test-trait", "synthetic"),
            Some(1)
        );
    }

    // what this catches: flush dispatches a bucket below threshold
    // when the operator forces it. The substrate's hippocampus
    // consolidation flag may want to force-train at end of session
    // even on a small bucket.
    //
    // Note on example count: the default LocalCandleFineTuner
    // schedule uses batch_size=4, and the DataLoader drops the
    // partial last batch (per [[no-fallbacks-ever]] — refuses to
    // pad-inflate the loss). So a flush of fewer than batch_size
    // examples would surface as a downstream EmptyDataset error.
    // 5 examples gives one full batch and exercises flush's
    // dispatch-on-partial-bucket contract.
    #[tokio::test]
    async fn flush_dispatches_partial_bucket() {
        let (trigger, _executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let examples: Vec<TrainingExample> = (0..5)
            .map(|i| ex(&format!("p-{i}"), &format!("c-{i}")))
            .collect();
        let _ = trigger
            .handle_command(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", examples, Some(100)),
            )
            .await
            .unwrap();
        assert_eq!(trigger.bucket_example_count(persona, "test-trait", "synthetic"), Some(5));

        let flush_params = json!({
            "personaId": persona,
            "traitKind": "test-trait",
            "baseModel": "synthetic",
        });
        let result = trigger
            .handle_command("genome/training-trigger/flush", flush_params)
            .await
            .unwrap();
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        assert_eq!(json["success"], true, "got: {json}");
        assert_eq!(json["outcome"], "JobDispatched");
        assert_eq!(json["examplesUsed"], 5);
        assert_eq!(trigger.bucket_example_count(persona, "test-trait", "synthetic"), None);
    }

    // what this catches: flush on an empty bucket is a no-op success,
    // not an error. Idempotent flush lets callers retry safely
    // without checking state first.
    #[tokio::test]
    async fn flush_empty_bucket_is_noop() {
        let (trigger, _executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let result = trigger
            .handle_command(
                "genome/training-trigger/flush",
                json!({"personaId": persona, "traitKind": "nope", "baseModel": "synthetic"}),
            )
            .await
            .unwrap();
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        assert_eq!(json["success"], true);
        assert_eq!(json["outcome"], "NothingToFlush");
    }

    // what this catches: dispatch failure preserves the bucket
    // contents — the worst regression in this module would be a
    // failed dispatch silently dropping curated examples. A trigger
    // with NO executor installed simulates "boot ordering error" —
    // the bucket must survive intact so the next submit (with the
    // executor wired) can re-trigger.
    #[tokio::test]
    async fn dispatch_failure_preserves_bucket_contents() {
        let trigger = TrainingTriggerModule::new();
        // Note: no executor installed → dispatch must fail.
        let persona = Uuid::new_v4();

        let result = trigger
            .handle_command(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("a", "b"), ex("c", "d")], Some(2)),
            )
            .await
            .unwrap();
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        assert_eq!(json["success"], false);
        assert_eq!(json["errorKind"], "DispatchFailed");
        // The two examples must STILL be in the bucket.
        assert_eq!(trigger.bucket_example_count(persona, "test-trait", "synthetic"), Some(2));
    }

    // what this catches: different personas have isolated buckets.
    // A regression that keyed on trait_kind alone would mix multiple
    // personas' curricula into one training run.
    #[tokio::test]
    async fn different_personas_have_isolated_buckets() {
        let (trigger, _executor) = build_runtime_with_trigger_and_genome().await;
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        let _ = trigger
            .handle_command(
                "genome/training-trigger/submit",
                submit_params(a, "shared-trait", vec![ex("a1", "b1")], Some(5)),
            )
            .await
            .unwrap();
        let _ = trigger
            .handle_command(
                "genome/training-trigger/submit",
                submit_params(b, "shared-trait", vec![ex("a2", "b2"), ex("c2", "d2")], Some(5)),
            )
            .await
            .unwrap();

        assert_eq!(trigger.bucket_example_count(a, "shared-trait", "synthetic"), Some(1));
        assert_eq!(trigger.bucket_example_count(b, "shared-trait", "synthetic"), Some(2));
        assert_eq!(trigger.pending_bucket_count(), 2);
    }

    // Note: the load-bearing concurrent-submit-safety test lives
    // behind `#[cfg(feature = "stress-tests")]` in `mod stress` at
    // the bottom of this test mod. Per Reviewer 3's BLOCK
    // (re-review): a default `#[tokio::test]` runs on the
    // current_thread runtime, and the dispatch chain through
    // GenomeModule + LocalCandleFineTuner contains zero yielding
    // `.await`s — so spawned tasks never actually interleave, and
    // the test passes even if the gate is removed. The stress
    // variant uses a multi-thread runtime + a yielding stub
    // adapter that injects a real `tokio::task::yield_now().await`
    // in the dispatch path, forcing the race window to open. This
    // is the doctrinal home for stress / multi-thread tests
    // (CLAUDE.md test-discipline section).

    // what this catches: status command lists all pending buckets in
    // a deterministic order. Operator tooling relies on this for
    // visual diffing — non-deterministic ordering would make every
    // status snapshot look "different" even when state is identical.
    #[tokio::test]
    async fn status_returns_deterministic_bucket_list() {
        let (trigger, _executor) = build_runtime_with_trigger_and_genome().await;
        let a = Uuid::nil(); // stable for ordering
        let b = Uuid::from_u128(1);

        let _ = trigger
            .handle_command(
                "genome/training-trigger/submit",
                submit_params(b, "trait-z", vec![ex("a", "b")], Some(5)),
            )
            .await
            .unwrap();
        let _ = trigger
            .handle_command(
                "genome/training-trigger/submit",
                submit_params(a, "trait-a", vec![ex("c", "d")], Some(5)),
            )
            .await
            .unwrap();

        let result = trigger
            .handle_command("genome/training-trigger/status", json!({}))
            .await
            .unwrap();
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        let buckets = json["buckets"].as_array().unwrap();
        assert_eq!(buckets.len(), 2);
        // a (nil uuid) sorts before b.
        assert_eq!(buckets[0]["personaId"], a.to_string());
        assert_eq!(buckets[0]["traitKind"], "trait-a");
        assert_eq!(buckets[1]["personaId"], b.to_string());
        assert_eq!(buckets[1]["traitKind"], "trait-z");
    }

    /// VDD — validation-driven tests verifying the conservation
    /// invariant: every example a producer submits ends up in the
    /// dispatched training job EXACTLY once, in original order, with
    /// no duplicates or drops across the accumulate-fire boundary.
    ///
    /// Difference vs the TDD tests above: TDD pins lifecycle
    /// (appends, fires, clears). VDD pins the
    /// MATHEMATICAL-INVARIANT — example conservation. A regression
    /// that double-counted or dropped on the drain-and-dispatch path
    /// would pass every TDD lifecycle test silently and corrupt
    /// every produced LoRA layer's training data.
    mod vdd {
        use super::*;
        use crate::genome::fine_tuning::{
            FineTuningRegistry, RecordingFineTuningAdapter, RECORDING_BASE_PREFIX,
        };
        use crate::modules::genome::GenomeModule;

        /// Build a runtime where genome/job-create routes to a
        /// RecordingFineTuningAdapter (substrate's canonical test
        /// fixture for capturing dispatched TrainingJobRequests).
        /// Returns the trigger + the shared captures handle so the
        /// test body can compare dispatched-vs-submitted at exact
        /// example granularity.
        ///
        /// Per Reviewer 1's BLOCK M2 (re-review): the previous
        /// `trained_tokens > 0` smoke check could not actually
        /// detect a drop/duplicate in the dispatch path because
        /// `trained_tokens` is a function of schedule, not example
        /// count. The recording fixture is the proper "did every
        /// submitted example flow through" check.
        async fn build_recording_runtime() -> (
            Arc<TrainingTriggerModule>,
            Arc<RecordingFineTuningAdapter>,
        ) {
            let registry = Arc::new(ModuleRegistry::new());
            let trigger = Arc::new(TrainingTriggerModule::new());
            registry.register(trigger.clone());

            let ft_registry = Arc::new(FineTuningRegistry::new());
            let recorder = Arc::new(RecordingFineTuningAdapter::new());
            ft_registry.register(recorder.clone());
            registry.register(Arc::new(GenomeModule::new(ft_registry)));

            let executor = Arc::new(CommandExecutor::new(registry.clone()));
            registry.install_executor_on_all(executor.clone());
            (trigger, recorder)
        }

        // what this VDD catches: every example submitted across N
        // submits appears EXACTLY ONCE in a dispatched job — no
        // duplicates, no drops, in submission order. The
        // RecordingFineTuningAdapter captures the dispatched
        // TrainingDataset so we can compare prompt-by-prompt against
        // what was submitted. This is real conservation accounting,
        // not the tautological `trained_tokens > 0` the previous
        // version asserted.
        //
        // A regression that double-counted or dropped on the
        // drain-and-dispatch path would here surface as either a
        // count mismatch OR a missing/duplicated prompt — both
        // distinct, both failure-class-specific.
        #[tokio::test]
        async fn submitted_examples_flow_through_dispatch_intact() {
            let (trigger, recorder) = build_recording_runtime().await;
            let persona = Uuid::new_v4();
            let n = 8;

            // Submit n examples with unique prompt text so a
            // duplication or drop would be visible at the
            // prompt-string level, not just in counts.
            let examples: Vec<TrainingExample> = (0..n)
                .map(|i| ex(&format!("prompt-{i}"), &format!("completion-{i}")))
                .collect();
            let mut params = submit_params(
                persona,
                "vdd-trait",
                examples.clone(),
                Some(n as u32),
            );
            // The recording fixture matches base_model prefix
            // "recording-test" — submit_params helper defaults to
            // "synthetic" which would route to LocalCandleFineTuner.
            // Override so dispatch deterministically lands at the
            // recorder.
            params.as_object_mut().unwrap().insert(
                "baseModel".into(),
                Value::String(format!("{RECORDING_BASE_PREFIX}-vdd")),
            );

            let result = trigger
                .handle_command("genome/training-trigger/submit", params)
                .await
                .unwrap();
            let json = match result {
                CommandResult::Json(v) => v,
                other => panic!("expected Json, got {other:?}"),
            };
            assert_eq!(json["success"], true, "VDD: submit must succeed; got {json}");
            assert_eq!(
                json["examplesUsed"], n,
                "VDD: every submitted example must be in the dispatched job"
            );

            // Bucket cleared exactly once — zero leftover.
            assert_eq!(
                trigger.bucket_example_count(persona, "vdd-trait", &format!("{RECORDING_BASE_PREFIX}-vdd")),
                None,
                "VDD: bucket must be fully drained, no leftover"
            );
            assert_eq!(trigger.pending_bucket_count(), 0);

            // CONSERVATION CHECK — the load-bearing assertion this
            // test exists for. Exactly one job dispatched, exactly
            // n examples captured, examples match submitted set in
            // ORDER (the trigger preserves submission order on
            // append + drain).
            assert_eq!(
                recorder.captured_job_count(),
                1,
                "VDD: exactly one job dispatched"
            );
            assert_eq!(
                recorder.captured_example_count(),
                n,
                "VDD: dispatched dataset must carry exactly the {n} examples submitted"
            );

            let captures = recorder.captures();
            let guard = captures.lock().unwrap();
            let dispatched_examples = &guard[0].dataset.examples;
            assert_eq!(dispatched_examples.len(), examples.len());
            for (i, (sub, disp)) in examples.iter().zip(dispatched_examples.iter()).enumerate() {
                assert_eq!(
                    sub.prompt, disp.prompt,
                    "VDD: example {i} prompt mismatch — submission order violated or example replaced"
                );
                assert_eq!(sub.completion, disp.completion);
            }
        }

        // what this VDD catches: conservation across MULTIPLE
        // submits to the same bucket. Pre-fix the previous tests
        // verified single-submit flows; this verifies that
        // accumulated submits, when finally drained-and-dispatched,
        // carry every example in INSERTION ORDER. A bug in
        // `entry.examples.extend(...)` (e.g. accidentally calling
        // `replace_with` or prepending instead of appending) would
        // here surface as a reordering or count mismatch.
        #[tokio::test]
        async fn multi_submit_accumulation_preserves_order_through_dispatch() {
            let (trigger, recorder) = build_recording_runtime().await;
            let persona = Uuid::new_v4();
            let trait_kind = "multi-submit-vdd";
            let base_model = format!("{RECORDING_BASE_PREFIX}-multi");

            // 4 submits × 3 examples = 12. Threshold 12 → fires
            // only at the last submit.
            let mut all_submitted: Vec<TrainingExample> = Vec::new();
            for batch in 0..4 {
                let exs: Vec<TrainingExample> = (0..3)
                    .map(|i| {
                        ex(
                            &format!("b{batch}-p{i}"),
                            &format!("b{batch}-c{i}"),
                        )
                    })
                    .collect();
                all_submitted.extend(exs.clone());
                let mut params = submit_params(persona, trait_kind, exs, Some(12));
                params
                    .as_object_mut()
                    .unwrap()
                    .insert("baseModel".into(), Value::String(base_model.clone()));
                let _ = trigger
                    .handle_command("genome/training-trigger/submit", params)
                    .await
                    .unwrap();
            }

            // Exactly one job dispatched (only the 4th submit
            // crossed threshold).
            assert_eq!(recorder.captured_job_count(), 1);

            let captures = recorder.captures();
            let guard = captures.lock().unwrap();
            let dispatched = &guard[0].dataset.examples;
            assert_eq!(
                dispatched.len(),
                12,
                "VDD: dispatched dataset must carry all 12 accumulated examples"
            );
            for (i, (sub, disp)) in all_submitted.iter().zip(dispatched.iter()).enumerate() {
                assert_eq!(
                    sub.prompt, disp.prompt,
                    "VDD: position {i} reordering — accumulator violated submission order"
                );
            }
        }
    }

    /// Stress / concurrency tests — gated behind the `stress-tests`
    /// feature per CLAUDE.md's test-discipline doctrine. Default
    /// `cargo test` does NOT compile these; periodic CI runs them
    /// via `--features stress-tests`. Multi-thread tokio runtime
    /// plus a yielding stub adapter actually exercise the race
    /// windows the gate is supposed to protect against.
    ///
    /// Per Reviewer 3's re-review BLOCK: the earlier inline
    /// concurrency test ran on the current_thread runtime and the
    /// dispatch chain contained zero yielding `.await` points, so
    /// spawned tasks never interleaved — the test passed regardless
    /// of whether the gate was present. This `mod stress` is the
    /// doctrinal home for tests that need to genuinely exercise
    /// concurrent paths.
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;
        use crate::genome::fine_tuning::adapter::{
            FineTuningAdapter, FineTuningCapabilities, FineTuningError,
        };
        use crate::genome::fine_tuning::types::{
            JobHandle, JobMetrics, TrainingArtifact, TrainingJobRequest, TrainingStatus,
        };
        use crate::genome::fine_tuning::FineTuningRegistry;
        use crate::modules::genome::GenomeModule;
        use async_trait::async_trait;
        use std::sync::Mutex as StdMutex;
        use std::time::Duration;

        /// Test-only adapter that:
        /// 1. captures the dispatched `TrainingJobRequest`s in a
        ///    `Mutex<Vec<_>>` so the test can later sum the
        ///    examples that actually flowed through dispatch
        ///    (vs the ones the trigger held back as pending);
        /// 2. yields cooperatively via `tokio::task::yield_now()`
        ///    AND sleeps for a microsecond before returning, so
        ///    that under a multi-thread runtime concurrent
        ///    submit tasks WILL interleave at the gate's
        ///    `.await` boundary. Without this, the dispatch
        ///    chain contains no yielding awaits and current_thread
        ///    runs tasks serially.
        struct YieldingRecordingAdapter {
            captured_requests: Arc<StdMutex<Vec<TrainingJobRequest>>>,
        }

        impl YieldingRecordingAdapter {
            fn new() -> Self {
                Self {
                    captured_requests: Arc::new(StdMutex::new(Vec::new())),
                }
            }
            fn captures(&self) -> Arc<StdMutex<Vec<TrainingJobRequest>>> {
                self.captured_requests.clone()
            }
        }

        #[async_trait]
        impl FineTuningAdapter for YieldingRecordingAdapter {
            fn capabilities(&self) -> FineTuningCapabilities {
                FineTuningCapabilities {
                    provider_id: "stress-yielding-recorder".to_string(),
                    supports_lora: true,
                    supports_validation: false,
                    produces_local_artifact: true,
                    supported_base_model_prefixes: vec!["stress".to_string()],
                }
            }

            async fn create_job(
                &self,
                request: TrainingJobRequest,
            ) -> Result<JobHandle, FineTuningError> {
                // WIDE race window. The C1 / C2 races live
                // between (a) the trigger's entry exit on the
                // dispatching task and (b) the success/failure
                // resolution after this `create_job.await`
                // returns. Multiple yields + a long-enough sleep
                // open the window wide enough for contending
                // submits to interleave reliably across runs.
                for _ in 0..8 {
                    tokio::task::yield_now().await;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
                self.captured_requests
                    .lock()
                    .unwrap()
                    .push(request.clone());
                Ok(JobHandle {
                    provider_id: "stress-yielding-recorder".into(),
                    provider_job_id: format!("stress-{}", uuid::Uuid::new_v4()),
                    local_id: uuid::Uuid::new_v4(),
                })
            }

            async fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
                Ok(TrainingStatus::Completed {
                    artifact: TrainingArtifact {
                        model_id: handle.provider_job_id.clone(),
                        local_path: None,
                        metrics: JobMetrics::default(),
                    },
                })
            }

            async fn cancel(&self, _handle: &JobHandle) -> Result<(), FineTuningError> {
                Ok(())
            }
        }

        /// Build a runtime where genome/job-create routes to the
        /// yielding recorder. The trigger module sees a real
        /// CommandExecutor + GenomeModule chain — same code path
        /// production uses — but the recorder substitutes the
        /// adapter so we can observe dispatched datasets.
        async fn build_stress_runtime() -> (
            Arc<TrainingTriggerModule>,
            Arc<CommandExecutor>,
            Arc<StdMutex<Vec<TrainingJobRequest>>>,
        ) {
            let registry = Arc::new(ModuleRegistry::new());
            let trigger = Arc::new(TrainingTriggerModule::new());
            registry.register(trigger.clone());

            let ft_registry = Arc::new(FineTuningRegistry::new());
            let recorder = Arc::new(YieldingRecordingAdapter::new());
            let captures = recorder.captures();
            ft_registry.register(recorder);
            registry.register(Arc::new(GenomeModule::new(ft_registry)));

            let executor = Arc::new(CommandExecutor::new(registry.clone()));
            registry.install_executor_on_all(executor.clone());
            (trigger, executor, captures)
        }

        fn stress_submit_params(
            persona_id: Uuid,
            trait_kind: &str,
            examples: Vec<TrainingExample>,
            min_examples: u32,
        ) -> Value {
            json!({
                "personaId": persona_id,
                "personaName": "stress-p",
                "baseModel": "stress-test",
                "traitKind": trait_kind,
                "examples": examples,
                "source": "operator_curated",
                "minExamples": min_examples,
            })
        }

        // what this catches: a SMOKE-LEVEL conservation check
        // under multi-thread concurrent submits. The
        // YieldingRecordingAdapter captures dispatched datasets so
        // we can sum-vs-submitted (true conservation, not the
        // tautological `trained_tokens > 0` of the unit-test VDD).
        //
        // HONESTY NOTE: this test exercises the multi-thread runtime
        // and the dispatch chain WITH yields, but it does NOT
        // deterministically force the C1/C2 race window. The race
        // requires an accumulator submit to land in the bucket
        // BETWEEN a fire-load's drain and its success-remove —
        // a 5ms timing window. In practice, fire-loads keep
        // absorbing accumulators in a steady-state cycle (drain →
        // accums fill bucket → next fire absorbs them), so even
        // with the gate removed this test reports conservation
        // holds. A truly deterministic race-exercise test needs a
        // `tokio::sync::Notify` barrier inside the stub adapter to
        // pause the dispatch.await while a contending submit
        // explicitly lands an accumulator, then assert that
        // accumulator survives the fire-load's remove. That barrier-
        // based test lands in Fix-3 (defense PR) alongside the
        // RecordingFineTuningAdapter promotion to system fixture.
        // Per CLAUDE.md test discipline: a test must justify
        // itself — this one's justification is "smoke-tests the
        // multi-thread dispatch path; doesn't pin C1/C2 alone."
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_submits_to_same_key_serialize_without_loss_stress() {
            let (trigger, _executor, captures) = build_stress_runtime().await;
            let persona = Uuid::new_v4();

            // Per R3's prescription: mix FIRE-LOAD submits (threshold=5,
            // immediately fires) with ACCUMULATOR submits (threshold=100,
            // accumulates 1 example at a time). Fire-loads enter
            // dispatch.await holding nothing; accumulators target the
            // same bucket during that await window. WITHOUT the gate,
            // the fire-load's `buckets.remove(&key)` on success deletes
            // the accumulator's appended examples — conservation
            // breaks. WITH the gate, accumulators block behind the
            // fire-load and land in a fresh bucket after the remove.
            //
            // 10 fire-loads × 5 examples (each fires alone) + 50
            // accumulators × 1 example. Total = 100. Fire-loads need
            // 5 examples to cross their threshold, so each fires once.
            // Accumulators have min_examples=100, never fire on their
            // own.
            const N_FIRE: usize = 10;
            const FIRE_EXAMPLES: usize = 5;
            const N_ACCUM: usize = 50;
            let total_examples = N_FIRE * FIRE_EXAMPLES + N_ACCUM;

            let mut handles = Vec::with_capacity(N_FIRE + N_ACCUM);
            // Spawn accumulators first so they're queued and ready
            // to race with fire-loads.
            for accum in 0..N_ACCUM {
                let trig = trigger.clone();
                handles.push(tokio::spawn(async move {
                    trig.handle_command(
                        "genome/training-trigger/submit",
                        stress_submit_params(
                            persona,
                            "race-trait",
                            vec![ex(&format!("acc{accum}-p"), &format!("acc{accum}-c"))],
                            100, // never fires alone
                        ),
                    )
                    .await
                }));
            }
            // Spawn fire-loads interleaved with accumulator scheduling.
            for fire in 0..N_FIRE {
                let trig = trigger.clone();
                handles.push(tokio::spawn(async move {
                    trig.handle_command(
                        "genome/training-trigger/submit",
                        stress_submit_params(
                            persona,
                            "race-trait",
                            (0..FIRE_EXAMPLES)
                                .map(|i| {
                                    ex(
                                        &format!("fire{fire}-p{i}"),
                                        &format!("fire{fire}-c{i}"),
                                    )
                                })
                                .collect(),
                            5,
                        ),
                    )
                    .await
                }));
            }
            for h in handles {
                let r = h.await.unwrap().expect("submit");
                let json = match r {
                    CommandResult::Json(v) => v,
                    other => panic!("expected Json, got {other:?}"),
                };
                assert_eq!(
                    json["success"], true,
                    "stress: every submit must succeed; got {json}"
                );
            }

            // Sum examples in dispatched datasets.
            let dispatched: usize = captures
                .lock()
                .unwrap()
                .iter()
                .map(|req| req.dataset.examples.len())
                .sum();
            let pending = trigger
                .bucket_example_count(persona, "race-trait", "stress-test")
                .unwrap_or(0);
            let total = dispatched + pending;
            let expected = total_examples;
            assert_eq!(
                total, expected,
                "STRESS conservation: dispatched={dispatched} + pending={pending} = {total}, \
                 expected {expected}. A drop or duplicate means the gate failed under concurrency."
            );

            // Also assert no duplicate prompts in the dispatched
            // datasets. Each prompt is unique by construction
            // (s{submitter}-p{i}); duplication would indicate a
            // restore-on-failure path commingling.
            let mut seen_prompts = std::collections::HashSet::new();
            for req in captures.lock().unwrap().iter() {
                for ex in &req.dataset.examples {
                    assert!(
                        seen_prompts.insert(ex.prompt.clone()),
                        "STRESS: duplicate prompt {:?} in dispatched datasets — \
                         gate failed to prevent double-drain",
                        ex.prompt
                    );
                }
            }
        }
    }
}
