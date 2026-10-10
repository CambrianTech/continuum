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
//! AUTOMATIC: `train-done → convert → register → trial → her work decides`. Page-in
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
//! ## The decision is her work's (integrated, not parallel)
//!
//! This sentinel used to run `cognition/eval` in A/B mode on a forked copy of her mind
//! and page the gene in on `lift > 0`. Joel, 2026-09-27: "The point is integrated not
//! parallel." A score from a copy beside her life never reaches her turns, her rooms or
//! her learning. Now `Completed { artifact }` does three things and decides nothing:
//! a cheap pre-filter (reported validation or training loss must be finite), register
//! the gene so the serving engine loads it in place and dormant, and open a
//! [`GeneTrial`](crate::genome::gene_trial::GeneTrial). From then on each card she works
//! draws an arm; the room's outcome for the card, credited through the receipts' `genes`,
//! is what promotes or retires it.
//!
//! ## Off-tick chain
//!
//! the convert (an MLX export) can take minutes. Running it inline would stall the
//! tick task. So the sentinel CLAIMS the job (atomic remove from the board) the
//! instant it sees `Completed`, then spawns the register→trial chain off the tick. The
//! claim guarantees no later tick re-handles the same completion; the spawn keeps the
//! poll cadence crisp (mirrors the producer's best-effort spawn).

use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

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
/// [`CommandExecutor`] (to dispatch `forge/export` AS the persona, installed at
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

    /// Spawn the convert → register → open-trial chain for one completed job, OFF the
    /// tick (a convert can take minutes). The job has already been claimed off the board,
    /// so nothing else will re-handle it. Best-effort: any failure is on the probe stream
    /// and leaves her genome as it is.
    fn spawn_completion_chain(&self, job: WatchedJob, artifact: TrainingArtifact) {
        let Some(executor) = self.executor.cloned() else {
            // Before boot installs the executor (early boot / tests) we cannot run
            // the eval. Named, not silent: the job is already claimed, so this layer
            // is simply not measured this run. Re-dispatch on the next training cycle
            // would re-register a fresh handle.
            tracing::warn!(
                persona = %job.persona_id,
                trait_kind = %job.trait_kind,
                "training-completion-sentinel: executor not installed — cannot convert the completed job; no trial opened"
            );
            // no chain, no hold: the competence is not pending on anything
            TrainingJobBoard::global().adoption_done(job.handle.local_id);
            return;
        };

        tokio::spawn(async move {
            // The competence stays PENDING on the board until this chain ends, adopted or
            // refused, on every path out of it: a fill in the meantime joins the job
            // instead of minting beside it (card cb14cc13). Dropped = released.
            let _pending = AdoptionHold { job: job.handle.local_id };
            // Dispatch AS the persona (LocalPersona → Trusted, which may run the
            // Privileged convert) over the wired executor — the same
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

            // A finite reported loss is only a sanity gate, not evidence of improvement.
            // Preserve validation-first admission, including refusing non-finite validation
            // even when training loss is finite. Runs without validation use training loss.
            let (loss, loss_source) = match artifact.metrics.final_validation_loss {
                Some(loss) => (Some(loss), "validation"),
                None => (artifact.metrics.final_loss, "training"),
            };
            if !loss.is_some_and(f64::is_finite) {
                crate::probe!(
                    class = "genome.trial.refused",
                    persona = %job.persona_id,
                    gene = job.trait_kind.as_str(),
                    "the trained gene reported no finite loss: no trial opened, her genome unchanged"
                );
                return;
            }

            // STAMP the signature into the sidecar BEFORE the gene becomes live (Cormac on
            // #4794, point 6): from the moment the trial opens, a fill must find this gene
            // in the store, by distance, so it awaits the trial rather than minting beside
            // it. Best-effort: a failed stamp warns; the gene serves either way and routes
            // by fallback until the next adoption re-stamps.
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


            // INTEGRATED, NOT PARALLEL (Joel, 2026-09-27). The gene is registered, so the serving
            // engine loads it in place, dormant (#4467), and a TRIAL opens: from now on each
            // card she works draws an arm, and the room's outcome for the card is what promotes
            // or retires it (genome/gene_trial.rs). No eval copy of her mind scores it beside
            // her life. The same seam a reuse decision adopts an existing gene through.
            let adopted = crate::genome::gene_trial::Adoption::default_paths().and_then(|a| {
                a.adopt(
                    job.persona_id,
                    &job.trait_kind,
                    std::path::Path::new(&path_str),
                    &job.base_model,
                    chrono::Utc::now().timestamp_millis().max(0) as u64,
                )
            });
            let trial = match adopted {
                Ok(t) => t,
                Err(refusal) => {
                    crate::probe!(
                        class = "genome.trial.refused",
                        persona = %job.persona_id,
                        gene = job.trait_kind.as_str(),
                        refusal = %refusal,
                        "the trained gene was not adopted: no trial opened, her genome unchanged"
                    );
                    return;
                }
            };
            crate::probe!(
                class = "genome.trial.opened",
                persona = %job.persona_id,
                gene = job.trait_kind.as_str(),
                trial = %trial.id,
                base = job.base_model.as_str(),
                share_milli = trial.share_milli as u64,
                loss = loss.unwrap_or_default(), // probe field: guarded finite above
                loss_source = loss_source,
                "a trained gene opened a trial: it now works a share of her cards, and her work's outcomes decide it"
            );

        });
    }
}

/// Holds a completed job's competence as pending on the board for the life of its
/// adoption chain; dropping it (any path out of the chain) releases it.
struct AdoptionHold {
    job: uuid::Uuid,
}

impl Drop for AdoptionHold {
    fn drop(&mut self) {
        crate::genome::fine_tuning::TrainingJobBoard::global().adoption_done(self.job);
    }
}

/// Normalize a completed training artifact into a PAGEABLE gguf-lora gene path —
/// the one shape the serving engine and `cycle.page_in` can load. The
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
        ArtifactFormat::MlxAdapterDir | ArtifactFormat::PeftAdapterDir => {
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
            let checkpoint_format = if artifact.format == ArtifactFormat::PeftAdapterDir {
                crate::forge::protocol::AdapterCheckpointFormat::Peft
            } else {
                crate::forge::protocol::AdapterCheckpointFormat::Mlx
            };
            let params = serde_json::json!({
                "checkpoint_format": checkpoint_format,
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
        // A gene never leaves her head by silence (GENE-REUSE-FORK-MINT §3d): there is no
        // expiry to run on this cadence; a landed gene settles for SETTLE_WINDOW_MS and
        // then simply stops holding its bucket.
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
                    .claim(
                        job.handle.local_id,
                        &TrainingStatus::Failed {
                            error: format!(
                                "provider {} is no longer registered",
                                job.handle.provider_id
                            ),
                        },
                    )
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
                Ok(status @ TrainingStatus::Completed { .. }) => {
                    // Claim BEFORE spawning so no later tick re-handles this job.
                    if let Some(job) =
                        TrainingJobBoard::global().claim(job.handle.local_id, &status)
                    {
                        if let TrainingStatus::Completed { artifact } = status {
                            self.spawn_completion_chain(job, artifact);
                        }
                    }
                }
                Ok(ref status @ TrainingStatus::Failed { ref error }) => {
                    if TrainingJobBoard::global()
                        .claim(job.handle.local_id, status)
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
                        .claim(job.handle.local_id, &TrainingStatus::Cancelled)
                        .is_some()
                    {
                        tracing::info!(
                            persona = %job.persona_id,
                            trait_kind = %job.trait_kind,
                            "training-completion-sentinel: training job cancelled — dropped"
                        );
                    }
                }
                Ok(
                    TrainingStatus::Queued
                    | TrainingStatus::WaitingForCapacity { .. }
                    | TrainingStatus::Running { .. },
                ) => {
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
        assert!(TrainingJobBoard::global()
            .claim(Uuid::nil(), &TrainingStatus::Cancelled)
            .is_none());
    }
}
