//! `TrainingCompletionSentinel` — L3 of the dev-task continuous-learning loop: the
//! completion listener that turns a finished training job into a MEASURED, possibly
//! ADOPTED genome layer.
//!
//! ## What it closes
//!
//! L1 (tool-trace → training data) and L2 (producer → trigger → `genome/job-create`)
//! get a job dispatched. But nothing observed completion, so the loop stopped at
//! "trained" — the freshly-forged layer was never measured against the persona and
//! never paged in. This sentinel is the keystone that makes the single-machine loop
//! AUTOMATIC: `train-done → convert → cognition/eval → lift>0 → page-in`. Page-in
//! is local (no publish step), so L1+L2+L3 alone is a closed self-improvement loop
//! on one machine (`docs/genome/DEV-TASK-LOOP-CLOSURE-PLAN.md`).
//!
//! ## The CONVERT stage (format-driven, custodian-dispatched)
//!
//! A trainer doesn't necessarily emit a pageable gene. Apple's `mlx_lm.lora`
//! (the real owned trainer, #32) writes an MLX `adapters.safetensors` dir — the
//! A/B lane and `page_in` load a `gguf-lora`. So before eval the sentinel
//! NORMALIZES the completed artifact to a pageable gene, keyed on its
//! [`ArtifactFormat`] (declared by the producing adapter — never sniffed from the
//! provider string, smell #70): `MlxAdapterDir` is dispatched to the forge
//! custodian via `forge/export` (convert + register the gene); a `GgufLora` is
//! used as-is; a synthetic/provider-hosted artifact has no locally-loadable gene
//! and is kept out. The custodian is a TRAIT — local convert today, a grid GPU
//! node tomorrow — so heavy convert (and eventually training) offloads to the
//! mesh by construction. See [`resolve_pageable_gene_path`].
//!
//! ## The shape (canonical RTOS daemon)
//!
//! A `ServiceModule` with a `tick_interval` — the runtime owns the interval timer,
//! the `MissedTickBehavior::Skip` cadence, the per-tick `catch_unwind`, and the
//! quarantine (`runtime/runtime.rs`), so this module just declares the cadence and
//! does the work in [`tick`](ServiceModule::tick). `Background` priority: training
//! completion is rare and slow (minutes), so a slow poll is correct.
//!
//! ## Why poll, not subscribe
//!
//! The training-job model is poll-based by nature: `TrainingStatus::Completed` is
//! emitted on a `watch::Sender` whose receiver is PRIVATE inside the adapter's
//! `JobController`, and cloud adapters (OpenAI, Mistral) can only be ASKED, never
//! tell us. So the one uniform observation surface across every provider is to poll
//! the handle. Each tick snapshots the [`TrainingJobBoard`] (the in-flight handles
//! the L2 trigger registered), polls each via the same
//! [`FineTuningAdapter::poll`](crate::genome::fine_tuning::FineTuningAdapter) the
//! `genome/job-status` command uses, and acts on terminal status.
//!
//! ## The measure→decide gate (the humane discipline, honored)
//!
//! On `Completed { artifact }` the sentinel runs `cognition/eval` in A/B mode (base
//! vs the freshly-forged gene) — which forks an EPHEMERAL measurement copy and
//! leaves the live persona untouched ([[humane-snapshot-eval]] / task #59). It reads
//! the reported `lift` and pages the gene into the LIVE cycle ONLY when `lift > 0`.
//! A zero/negative lift (an overfit or regressing layer) is logged and KEPT OUT —
//! fail loud, never a silent adoption. This is exactly the "adopting the gene is a
//! separate, deliberate decision, never a side effect of measuring it" contract:
//! the eval measures, THIS sentinel decides.
//!
//! ## Off-tick chain
//!
//! `cognition/eval` runs a full gym A/B — minutes. Running it inline would stall the
//! tick task. So the sentinel CLAIMS the job (atomic remove from the board) the
//! instant it sees `Completed`, then spawns the eval→page-in chain off the tick. The
//! claim guarantees no later tick re-handles the same completion; the spawn keeps the
//! poll cadence crisp (mirrors the producer's best-effort spawn).

use crate::cognition::learning_policy::LearningPolicy;
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use crate::ai::types::ActiveAdapterRequest;
use crate::cognition::eval::{CognitionEvalParams, EvalGene};
use crate::genome::fine_tuning::{
    ArtifactFormat, FineTuningRegistry, TrainingArtifact, TrainingJobBoard, TrainingStatus,
    WatchedJob,
};
use crate::routing::CallerIdentity;
use crate::runtime::{
    CommandExecutor, CommandResult, InProcessTransport, LateBound, ModuleConfig, ModuleContext,
    ModulePriority, ServiceModule,
};
use continuum_client::Connection;

/// How often to poll in-flight training jobs for completion. 15s: training takes
/// minutes, so a slow poll loses nothing and keeps the substrate quiet. The poll is
/// cheap (one `poll(&handle)` per in-flight job, typically 0); the eval chain it can
/// trigger is the only heavy work, and that runs OFF this tick.
const POLL_INTERVAL: Duration = Duration::from_secs(15);

/// L3 completion sentinel. Holds the [`FineTuningRegistry`] (to poll handles, the
/// same registry the `genome/job-*` commands use) and a late-bound
/// [`CommandExecutor`] (to dispatch `cognition/eval` AS the persona, installed at
/// boot by `install_executor_on_all`).
pub struct TrainingCompletionSentinel {
    registry: Arc<FineTuningRegistry>,
    executor: LateBound<CommandExecutor>,
}

impl TrainingCompletionSentinel {
    /// Build the sentinel over the shared fine-tuning registry. The executor is
    /// installed later at boot via [`ServiceModule::install_executor`].
    pub fn new(registry: Arc<FineTuningRegistry>) -> Self {
        Self {
            registry,
            executor: LateBound::new("training-completion-sentinel::executor"),
        }
    }

    /// Spawn the eval→page-in chain for one completed job, OFF the tick (the eval is
    /// a minutes-long gym A/B). The job has already been claimed off the board, so
    /// nothing else will re-handle it. Best-effort: any failure is logged and leaves
    /// the live persona on its current genome — a failed measurement NEVER degrades
    /// her ([[humane-snapshot-eval]]).
    fn spawn_completion_chain(&self, job: WatchedJob, artifact: TrainingArtifact) {
        // FAIL LOUD at the earliest seam: a gene the substrate cannot fairly MEASURE
        // must never be paged into a live persona. The recipe declares its gym (the
        // `cognition/eval` `eval_set`) at submission and it rides the board to here;
        // when it's absent there is no honest A/B to gate on, so we REFUSE to adopt
        // rather than measuring against an arbitrary default gym
        // ([[fallbacks-are-illegal-fail-loud]]). Checked BEFORE the convert/eval spend
        // because an unmeasurable gene is wasted compute. The job is already claimed
        // off the board, so dropping it here simply leaves the living persona on her
        // current genome — never degraded by a measurement we couldn't run.
        let Some(eval_set) = job.eval_set.clone() else {
            tracing::warn!(
                persona = %job.persona_id,
                trait_kind = %job.trait_kind,
                "training-completion-sentinel: recipe declared no gym (eval_set) — gene is unmeasurable, NOT adopted (persona unchanged)"
            );
            return;
        };

        let Some(executor) = self.executor.cloned() else {
            // Before boot installs the executor (early boot / tests) we cannot run
            // the eval. Named, not silent: the job is already claimed, so this layer
            // is simply not measured this run. Re-dispatch on the next training cycle
            // would re-register a fresh handle.
            tracing::warn!(
                persona = %job.persona_id,
                trait_kind = %job.trait_kind,
                "training-completion-sentinel: executor not installed — cannot eval completed job; layer left unmeasured"
            );
            return;
        };

        tokio::spawn(async move {
            // Dispatch `cognition/eval` AS the persona (LocalPersona → Trusted, which
            // may run the Privileged eval) over the wired executor — the same
            // persona-is-a-client path the L2 producer uses ([[persona-is-a-client]]).
            let conn = Connection::new(InProcessTransport::new(
                executor,
                Some(CallerIdentity::local_persona(
                    crate::identity::PeerId::from_uuid(job.persona_id),
                )),
            ));

            // CONVERT stage. Normalize whatever the trainer produced into a PAGEABLE
            // gguf-lora gene before measuring it — an MLX adapter dir gets dispatched
            // to the forge custodian (local today, a grid GPU node tomorrow, same
            // `ForgeCustodian` trait); a gguf-lora is used as-is; a synthetic/
            // provider-hosted artifact has no locally-loadable gene and is kept out.
            // All fail-loud logging lives in the helper.
            let Some(path_str) = resolve_pageable_gene_path(&conn, &job, &artifact).await else {
                return;
            };

            let params = CognitionEvalParams {
                run_id: None,
                persona_id: crate::identity::PersonaRef::new(job.persona_id.to_string()),
                gene: Some(EvalGene {
                    name: job.trait_kind.clone(),
                    path: path_str.clone(),
                    scale: None,
                }),
                room_id: None,
                tasks: None,
                // The gym the recipe DECLARED for this trait — measured on its own
                // gym, never a default ([[fallbacks-are-illegal-fail-loud]]). Guarded
                // Some at the top of this fn.
                eval_set: Some(eval_set),
                base_model_id: None, // a gene names its own forged base
                reviewers: None,     // solo eval for training lift
                detach: None,
                max_acts: None,
                max_retries: None,
                workspace_root: None,
                capture_dir: None,
                // The L3 auto-eval MEASURES lift; it is not her life. Stated, not defaulted.
                learn: LearningPolicy::DoNotLearn,
                // #207: L3 auto-eval measures LIFT (base vs gene in one fork), which is
                // reproducible regardless of recall; keep memories intact (default).
                suppress_recall: None,
                note: Some(format!(
                    "L3 auto-eval (gene={}, base={}, provider={})",
                    job.trait_kind, job.base_model, job.handle.provider_id
                )),
            };
            let params = match serde_json::to_value(&params) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(
                        persona = %job.persona_id,
                        error = %e,
                        "training-completion-sentinel: failed to serialize eval params — NOT adopted"
                    );
                    return;
                }
            };

            let result = match conn
                .commands()
                .execute_value("cognition/eval", params)
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        persona = %job.persona_id,
                        trait_kind = %job.trait_kind,
                        error = %e,
                        "training-completion-sentinel: cognition/eval failed — layer NOT adopted (persona unchanged)"
                    );
                    return;
                }
            };

            // The result IS the serialized CognitionEvalResult (no envelope). `lift`
            // = candidate pass-rate − base pass-rate; present in A/B mode (a gene was
            // given). Absent lift means the A/B didn't run — keep out, fail loud.
            let Some(lift) = result.get("lift").and_then(Value::as_f64) else {
                tracing::warn!(
                    persona = %job.persona_id,
                    trait_kind = %job.trait_kind,
                    "training-completion-sentinel: eval returned no lift (A/B did not run) — layer NOT adopted"
                );
                return;
            };
            let pass_rate = result.get("pass_rate").and_then(Value::as_f64);
            let base_pass_rate = result.get("base_pass_rate").and_then(Value::as_f64);

            // THE GATE. Adopt only on a real improvement.
            if lift <= 0.0 {
                tracing::info!(
                    persona = %job.persona_id,
                    trait_kind = %job.trait_kind,
                    lift,
                    base_pass_rate = ?base_pass_rate,
                    gene_pass_rate = ?pass_rate,
                    "training-completion-sentinel: lift ≤ 0 — gene rejected, persona kept on current genome"
                );
                return;
            }

            // lift > 0: page the gene into the LIVE cycle. A wait-free atomic genome
            // swap — the persona's next generation runs base + this layer.
            let Some(cycle) = crate::cognition::persona_workspace::global().get(&job.persona_id)
            else {
                // De-spawned between train start and completion — don't adopt into a
                // ghost. Fail loud; the next time she's live + retrained the loop runs.
                tracing::warn!(
                    persona = %job.persona_id,
                    trait_kind = %job.trait_kind,
                    lift,
                    "training-completion-sentinel: gene cleared the gate but persona has no live cycle — NOT adopted"
                );
                return;
            };

            cycle.page_in(vec![ActiveAdapterRequest {
                name: job.trait_kind.clone(),
                path: path_str.clone(),
                domain: job.trait_kind.clone(),
                scale: 1.0,
            }]);

            // STAMP the signature into the sidecar at the same moment the gene
            // becomes live — adoption is the one event where the gene's path,
            // its minted signature, and its measured worth are all in hand.
            // Best-effort: a failed stamp warns; the gene serves either way and
            // routes by fallback until the next adoption re-stamps.
            if let Some(sig) = job.signature.clone() {
                match crate::genome::signature::signature_store_path() {
                    Ok(store) => {
                        if let Err(e) = crate::genome::signature::SignatureStore::stamp_at(
                            &store, &path_str, sig,
                        ) {
                            tracing::warn!(gene = %path_str, error = %e,
                                "adopted gene's signature failed to stamp — routes by fallback");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "signature store path unresolvable — signature not stamped");
                    }
                }
            }

            tracing::info!(
                persona = %job.persona_id,
                persona_name = %job.persona_name,
                trait_kind = %job.trait_kind,
                lift,
                base_pass_rate = ?base_pass_rate,
                gene_pass_rate = ?pass_rate,
                "training-completion-sentinel: lift > 0 — gene ADOPTED, paged into live persona (loop closed)"
            );
        });
    }
}

/// Normalize a completed training artifact into a PAGEABLE gguf-lora gene path —
/// the one shape `cognition/eval`'s A/B lane and `cycle.page_in` can load. The
/// decision is FORMAT-driven, never provider-string-matched (smell #70): each
/// trainer declared what it produced, so the sentinel asks the artifact's
/// [`ArtifactFormat`], not its provider id.
///
/// - [`ArtifactFormat::GgufLora`] → already pageable; its local path verbatim.
/// - [`ArtifactFormat::MlxAdapterDir`] → dispatch `forge/export` (`gguf-lora`) to
///   the forge CUSTODIAN, which converts the MLX adapter into a GGUF-lora AND
///   registers it in the serving manifest (the "5th wire"). The custodian is a
///   trait: `ForgeCustodianHttp` runs the convert locally today; a future
///   `GridForgeCustodian` routes the SAME request to a GPU node on the mesh — so a
///   slow machine offloads the heavy convert by construction, no caller change.
///   We dispatch the command (persona-is-a-client), never reach into forge's
///   private convert fn — `forge/export` already owns convert + register.
/// - [`ArtifactFormat::CandleSafetensors`] / [`ArtifactFormat::ProviderHosted`] →
///   no locally-pageable gene for the A/B lane; fail loud and keep it OUT rather
///   than adopt something unmeasured ([[fallbacks-are-illegal-fail-loud]]).
///
/// Returns `None` (with a named log) whenever a pageable gene can't be produced;
/// the caller then leaves the live persona on her current genome.
async fn resolve_pageable_gene_path(
    conn: &Connection<InProcessTransport>,
    job: &WatchedJob,
    artifact: &TrainingArtifact,
) -> Option<String> {
    match artifact.format {
        ArtifactFormat::GgufLora => {
            let Some(path) = artifact.local_path.as_ref() else {
                tracing::warn!(
                    persona = %job.persona_id,
                    trait_kind = %job.trait_kind,
                    model_id = %artifact.model_id,
                    "training-completion-sentinel: gguf-lora artifact has no local path — cannot A/B-measure it; NOT adopted"
                );
                return None;
            };
            Some(path.to_string_lossy().to_string())
        }
        ArtifactFormat::MlxAdapterDir => {
            let Some(mlx_dir) = artifact.local_path.as_ref() else {
                tracing::warn!(
                    persona = %job.persona_id,
                    trait_kind = %job.trait_kind,
                    model_id = %artifact.model_id,
                    "training-completion-sentinel: MLX artifact has no local adapter dir — nothing to convert; NOT adopted"
                );
                return None;
            };
            let mlx_dir = mlx_dir.to_string_lossy().to_string();

            // The custodian converts the trained checkpoint and writes the gene
            // alongside it. `base_model_id` is REQUIRED for gguf-lora — the
            // converter needs the base architecture; forge/export fails loud
            // without it, and so do we by passing the watched base.
            let params = serde_json::json!({
                "checkpoint": mlx_dir,
                "save_directory": mlx_dir,
                "format": "gguf-lora",
                "base_model_id": job.base_model,
                "outtype": "f16",
            });
            let result = match conn.commands().execute_value("forge/export", params).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        persona = %job.persona_id,
                        trait_kind = %job.trait_kind,
                        base_model = %job.base_model,
                        error = %e,
                        "training-completion-sentinel: forge/export (gguf-lora) failed — gene not converted (custodian unreachable or convert error); NOT adopted"
                    );
                    return None;
                }
            };

            // forge/export registered the gene; `registered.path` is the on-disk
            // gguf-lora the serving lane loads. Absent = contract breach → keep out.
            let Some(path) = result
                .get("registered")
                .and_then(|r| r.get("path"))
                .and_then(Value::as_str)
            else {
                tracing::warn!(
                    persona = %job.persona_id,
                    trait_kind = %job.trait_kind,
                    "training-completion-sentinel: forge/export returned no registered gene path — NOT adopted"
                );
                return None;
            };
            tracing::info!(
                persona = %job.persona_id,
                trait_kind = %job.trait_kind,
                gene = %path,
                "training-completion-sentinel: MLX adapter converted to gguf-lora gene via custodian"
            );
            Some(path.to_string())
        }
        ArtifactFormat::CandleSafetensors => {
            tracing::warn!(
                persona = %job.persona_id,
                trait_kind = %job.trait_kind,
                model_id = %artifact.model_id,
                "training-completion-sentinel: Candle skeleton artifact is a synthetic-base LoRA, not a loadable gene (#231-#233) — NOT adopted"
            );
            None
        }
        ArtifactFormat::ProviderHosted => {
            tracing::warn!(
                persona = %job.persona_id,
                trait_kind = %job.trait_kind,
                model_id = %artifact.model_id,
                "training-completion-sentinel: provider-hosted artifact has no local gene for the A/B lane — NOT adopted"
            );
            None
        }
    }
}

#[async_trait]
impl ServiceModule for TrainingCompletionSentinel {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "training-completion-sentinel",
            priority: ModulePriority::Background,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(POLL_INTERVAL),
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    /// Poll every in-flight training job; on terminal status, claim it and act.
    /// Sequential is correct here — the in-flight set is tiny (typically 0) and this
    /// runs on a 15s background cadence; the only heavy work (the eval chain) is
    /// spawned off-tick so the poll loop never blocks on it.
    async fn tick(&self) -> Result<(), String> {
        let jobs = TrainingJobBoard::global().snapshot();
        if jobs.is_empty() {
            return Ok(());
        }

        for job in jobs {
            let Some(adapter) = self.registry.get(&job.handle.provider_id) else {
                // The provider that owns this handle is no longer registered — we
                // can never poll it again. Claim (drop) it so we don't spin on a dead
                // provider forever; fail loud naming the cause.
                if TrainingJobBoard::global()
                    .claim(job.handle.local_id)
                    .is_some()
                {
                    tracing::warn!(
                        persona = %job.persona_id,
                        provider = %job.handle.provider_id,
                        "training-completion-sentinel: no adapter for in-flight job's provider — dropping unpollable job"
                    );
                }
                continue;
            };

            match adapter.poll(&job.handle).await {
                Ok(TrainingStatus::Completed { artifact }) => {
                    // Claim BEFORE spawning so no later tick re-handles this job.
                    if let Some(job) = TrainingJobBoard::global().claim(job.handle.local_id) {
                        self.spawn_completion_chain(job, artifact);
                    }
                }
                Ok(TrainingStatus::Failed { error }) => {
                    if TrainingJobBoard::global()
                        .claim(job.handle.local_id)
                        .is_some()
                    {
                        tracing::warn!(
                            persona = %job.persona_id,
                            trait_kind = %job.trait_kind,
                            error = %error,
                            "training-completion-sentinel: training job failed — dropped, nothing to measure"
                        );
                    }
                }
                Ok(TrainingStatus::Cancelled) => {
                    if TrainingJobBoard::global()
                        .claim(job.handle.local_id)
                        .is_some()
                    {
                        tracing::info!(
                            persona = %job.persona_id,
                            trait_kind = %job.trait_kind,
                            "training-completion-sentinel: training job cancelled — dropped"
                        );
                    }
                }
                Ok(TrainingStatus::Queued | TrainingStatus::Running { .. }) => {
                    // Still in flight — leave it on the board for the next tick.
                }
                Err(e) => {
                    // Transient poll error (network blip, provider hiccup). Leave the
                    // job on the board and retry next tick — don't drop a job over a
                    // momentary failure.
                    tracing::debug!(
                        persona = %job.persona_id,
                        provider = %job.handle.provider_id,
                        error = %e,
                        "training-completion-sentinel: poll error — will retry next tick"
                    );
                }
            }
        }

        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // The sentinel exposes no commands — it is a pure background poller. Any
        // command routed here is a wiring bug; fail loud naming it.
        Err(format!(
            "training-completion-sentinel exposes no commands (got '{command}')"
        ))
    }

    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        self.executor.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // what this catches: the daemon contract — Background priority + a periodic tick
    // (the runtime only spawns a tick loop when tick_interval is Some), and no
    // command surface (an empty prefix set; commands fail loud). A regression that
    // dropped the tick_interval would silently stop the loop from ever polling.
    #[test]
    fn config_is_a_periodic_background_poller_with_no_commands() {
        let sentinel = TrainingCompletionSentinel::new(Arc::new(FineTuningRegistry::new()));
        let cfg = sentinel.config();
        assert_eq!(cfg.name, "training-completion-sentinel");
        assert!(
            matches!(cfg.priority, ModulePriority::Background),
            "completion polling is slow background work"
        );
        assert_eq!(
            cfg.tick_interval,
            Some(POLL_INTERVAL),
            "must declare a periodic tick or the runtime never polls"
        );
        assert!(
            cfg.command_prefixes.is_empty(),
            "the sentinel is a pure poller — it owns no command surface"
        );
    }

    // what this catches: an empty board makes tick a clean no-op (no panic, no
    // executor needed) — the common case on most ticks. Guards the early-return so
    // the poller is free when nothing is training.
    #[tokio::test]
    async fn tick_is_a_noop_when_no_jobs_are_in_flight() {
        // A fresh registry + the (process-global) board, which is empty in a unit
        // run unless another test registered — so assert via a private board would be
        // racy; instead assert tick succeeds, which is the contract on an empty set.
        let sentinel = TrainingCompletionSentinel::new(Arc::new(FineTuningRegistry::new()));
        // No executor installed, no jobs — tick must still succeed.
        assert!(sentinel.tick().await.is_ok());
        // sanity: a nil-id claim on an empty board yields nothing.
        assert!(TrainingJobBoard::global().claim(Uuid::nil()).is_none());
    }
}
