//! `ai/inference/{status,models,load,unload}` — the model-lifecycle command
//! surface, as thin PROJECTIONS of the canonical serving state.
//!
//! ## Single source of truth (post-Unsloth excision)
//! These commands never open a private transport. `status`/`models` read the
//! authoritative [`current_serving`] snapshot (the [`ServingSnapshot`] the
//! [`crate::modules::serving_daemon::ServingDaemonModule`] publishes on its
//! global watch) plus the on-disk catalog ([`crate::model_registry::catalog::models`]).
//! A model SWAP is owned by the serving daemon's `serving/pin` / `serving/unpin`
//! seam — a single-resident respawn on THIS host — so `load`/`unload` delegate
//! there rather than mutating a private gateway. ONE model-swap path, one status
//! source. The dead Unsloth gateway (`http://127.0.0.1:8888`) is fully excised
//! from this surface ([[fallbacks-are-illegal-fail-loud]]).
//!
//! ## Shape
//! Stateless [`ActionCommand`]s ([[command-infra-self-routing-schema-adapters]]).
//! The command NAMES stay provider-agnostic (`ai/inference/*`); the impl reads the
//! canonical state, so a second serving engine changes only what publishes the
//! snapshot, not these commands.
//!
//! ## Access
//! `status` / `models` are read-only → [`AccessLevel::AiSafe`] (any citizen may ask
//! "what's serving"). `load` / `unload` route to the Privileged serving-daemon swap
//! → [`AccessLevel::Privileged`].

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::inference::llama_server::current_serving;
use crate::model_registry::catalog::models as on_disk_models;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

// ─────────────────────────── shared status view ──────────────────

/// A projection of the canonical [`crate::inference::llama_server::ServingSnapshot`]
/// — what the inference engine is serving RIGHT NOW. The ONE status shape, derived
/// from the serving daemon's published snapshot, never probed over a private wire.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/InferenceStatusView.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct InferenceStatusView {
    /// The single model answering right now (`None` = nothing served yet).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_model: Option<String>,
    /// True once the engine is HTTP-ready with an active model.
    pub ready: bool,
    /// The live serving base (`http://host:port`) — the real engine, not a guess.
    pub base_url: String,
    /// The effective served context window the daemon fit to THIS host.
    pub served_context_window: u32,
    /// The LoRA genome layers loaded into the serving catalog (sorted paths).
    pub adapters: Vec<String>,
}

/// Read the canonical serving snapshot and project it. If the daemon has not yet
/// installed its watch (early boot), `current_serving()` returns the empty snapshot
/// — an honest `ready: false`, never a fabricated "serving" state.
fn current_status() -> InferenceStatusView {
    let s = current_serving();
    InferenceStatusView {
        active_model: s.active_model,
        ready: s.ready,
        base_url: s.base_url,
        served_context_window: s.served_context_window,
        adapters: s.adapters,
    }
}

// ─────────────────────────── ai/inference/status ─────────────────

/// `ai/inference/status` has no params — it projects the live serving snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/StatusParams.ts"
)]
pub struct StatusParams {}

#[derive(Default)]
pub struct AiInferenceStatus;

#[async_trait]
impl ActionCommand for AiInferenceStatus {
    const NAME: &'static str = "ai/inference/status";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Report which model the inference engine is serving right now (activeModel), \
         whether it is ready, the live serving base URL, the served context window, \
         and the LoRA genome layers loaded. Projected from the serving daemon's \
         canonical snapshot — this is how you confirm which brain is live.";
    type Params = StatusParams;
    type Output = InferenceStatusView;

    async fn run(&self, _ctx: &Ctx, _p: StatusParams) -> Result<InferenceStatusView, CommandError> {
        Ok(current_status())
    }
}

// ─────────────────────────── ai/inference/models ─────────────────

/// `ai/inference/models` has no params — it lists the catalog + what's serving.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/ModelsParams.ts"
)]
pub struct ModelsParams {}

/// Result of `ai/inference/models`: the loadable catalog + which are serving.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/ModelsResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ModelsResult {
    /// Loadable on-disk model ids (each is what `serving/pin` takes) — from the
    /// model registry's catalog, which OWNS "what can this host run".
    pub available: Vec<String>,
    /// The ids serving right now. The engine is single-resident, so this is the
    /// active model (0 or 1), sourced from the canonical serving snapshot.
    pub serving: Vec<String>,
    /// The single active (answering) model, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active: Option<String>,
}

#[derive(Default)]
pub struct AiInferenceModels;

#[async_trait]
impl ActionCommand for AiInferenceModels {
    const NAME: &'static str = "ai/inference/models";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "List the models the inference engine can run: the loadable on-disk catalog \
         (each id is what `serving/pin` takes), which id is serving right now, and \
         which single model is active. Use to discover what you can switch to.";
    type Params = ModelsParams;
    type Output = ModelsResult;

    async fn run(&self, _ctx: &Ctx, _p: ModelsParams) -> Result<ModelsResult, CommandError> {
        // Serving truth from the ONE canonical source; catalog from the registry
        // that owns on-disk discovery. No private transport, no dead gateway.
        let snapshot = current_serving();
        let active = snapshot.active_model.clone();
        let serving: Vec<String> = snapshot.active_model.into_iter().collect();
        let available: Vec<String> = on_disk_models().into_iter().map(|m| m.id).collect();
        Ok(ModelsResult {
            available,
            serving,
            active,
        })
    }
}

// ─────────────────────────── ai/inference/load ───────────────────

/// Params for `ai/inference/load` / `ai/inference/unload` — the model id.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/ModelRef.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    /// The model identifier to act on — the SAME id `ai/inference/models` lists
    /// and a persona generates with (a hub id like
    /// `continuum-ai/qwen3.5-4b-code-forged-GGUF`, not a filesystem path).
    pub model: String,
}

#[derive(Default)]
pub struct AiInferenceLoad;

#[async_trait]
impl ActionCommand for AiInferenceLoad {
    const NAME: &'static str = "ai/inference/load";
    // Privileged: a load swaps the single resident model that EVERY persona sees.
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Make a model the active single-resident brain. The serving daemon owns the \
         swap (it respawns the engine, fit-gated to this host's budget), so this \
         delegates to `serving/pin <model>` — the ONE model-swap path. Privileged; \
         the persona hot path auto-serves its own model and never calls this.";
    type Params = ModelRef;
    type Output = InferenceStatusView;

    async fn run(&self, _ctx: &Ctx, p: ModelRef) -> Result<InferenceStatusView, CommandError> {
        let model = p.model.trim();
        if model.is_empty() {
            return Err(CommandError::Invalid(
                "ai/inference/load: 'model' is required (the id to make active)".into(),
            ));
        }
        // The daemon owns the single-resident respawn + host fit-gate. Route the
        // caller to that ONE authority rather than mutate a private gateway.
        Err(CommandError::Invalid(format!(
            "ai/inference/load delegates to the serving daemon: call `serving/pin` with \
             model='{model}' — it fit-gates the swap to this host's budget and respawns \
             the engine. (The single-resident model swap has ONE owner, the serving daemon.)"
        )))
    }
}

// ─────────────────────────── ai/inference/unload ─────────────────

#[derive(Default)]
pub struct AiInferenceUnload;

#[async_trait]
impl ActionCommand for AiInferenceUnload {
    const NAME: &'static str = "ai/inference/unload";
    // Privileged: unpinning changes the engine that every persona depends on.
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Release a pinned model back to autonomic serving. The serving daemon owns \
         this, so it delegates to `serving/unpin` — the ONE path. Privileged; affects \
         every persona depending on the engine.";
    type Params = ModelRef;
    type Output = InferenceStatusView;

    async fn run(&self, _ctx: &Ctx, p: ModelRef) -> Result<InferenceStatusView, CommandError> {
        let model = p.model.trim();
        if model.is_empty() {
            return Err(CommandError::Invalid(
                "ai/inference/unload: 'model' is required (the id to release)".into(),
            ));
        }
        Err(CommandError::Invalid(format!(
            "ai/inference/unload delegates to the serving daemon: call `serving/unpin` \
             (model='{model}') to return to autonomic serving. (The single-resident \
             engine has ONE owner, the serving daemon.)"
        )))
    }
}

// Stateless → self-register onto the ONE registry (descriptor + runtime object),
// no host module. Reads are AiSafe; load/unload are Privileged (declared above).
crate::register_stateless_command!(AiInferenceStatus);
crate::register_stateless_command!(AiInferenceModels);
crate::register_stateless_command!(AiInferenceLoad);
crate::register_stateless_command!(AiInferenceUnload);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the four lifecycle commands keep their canonical wire
    // names under the existing ai/inference/* family. A rename here silently
    // breaks every caller (cu, the persona tool surface, clients) — the names
    // are the contract.
    #[test]
    fn command_names_are_canonical() {
        assert_eq!(AiInferenceStatus::NAME, "ai/inference/status");
        assert_eq!(AiInferenceModels::NAME, "ai/inference/models");
        assert_eq!(AiInferenceLoad::NAME, "ai/inference/load");
        assert_eq!(AiInferenceUnload::NAME, "ai/inference/unload");
    }

    // what this catches: the access split is load-bearing for safety — reads are
    // open, but load/unload change the single shared engine and MUST stay
    // Privileged (an AiSafe swap would let any persona re-brain everyone).
    #[test]
    fn reads_are_aisafe_mutations_are_privileged() {
        assert_eq!(AiInferenceStatus::ACCESS, AccessLevel::AiSafe);
        assert_eq!(AiInferenceModels::ACCESS, AccessLevel::AiSafe);
        assert_eq!(AiInferenceLoad::ACCESS, AccessLevel::Privileged);
        assert_eq!(AiInferenceUnload::ACCESS, AccessLevel::Privileged);
    }

    // what this catches: status/models are PURE projections of the canonical
    // serving state — they never touch a network gateway, so they resolve even
    // with no serving daemon installed (early boot → honest empty snapshot),
    // never hang on a dead transport. Regression for the Unsloth :8888 excision.
    #[tokio::test]
    async fn status_and_models_project_canonical_state_without_network() {
        let ctx = Ctx::default();
        // No daemon installed in a unit test → empty snapshot, but the commands
        // still return Ok immediately (no dead-gateway probe).
        let status = AiInferenceStatus.run(&ctx, StatusParams {}).await.unwrap();
        assert!(!status.ready, "empty snapshot is honestly not-ready");
        let models = AiInferenceModels.run(&ctx, ModelsParams {}).await.unwrap();
        // serving mirrors active (single-resident); active None on empty snapshot.
        assert_eq!(models.active, None);
        assert!(models.serving.is_empty());
    }

    // what this catches: load/unload route to the ONE model-swap authority
    // (serving/pin·unpin), never a private/dead gateway. The error must NAME the
    // canonical command so a caller is redirected, not stranded. Blank id is
    // rejected first, before any delegation message.
    #[tokio::test]
    async fn load_unload_delegate_to_serving_pin() {
        let ctx = Ctx::default();
        // blank → typed Invalid up front
        let blank = AiInferenceLoad
            .run(&ctx, ModelRef { model: "  ".into() })
            .await
            .unwrap_err();
        assert!(matches!(blank, CommandError::Invalid(_)));
        // real id → delegated, error names serving/pin
        let load = AiInferenceLoad
            .run(
                &ctx,
                ModelRef {
                    model: "some/model-GGUF".into(),
                },
            )
            .await
            .unwrap_err();
        match load {
            CommandError::Invalid(m) => {
                assert!(m.contains("serving/pin"), "names the authority: {m}")
            }
            other => panic!("expected Invalid delegating to serving/pin, got {other:?}"),
        }
        let unload = AiInferenceUnload
            .run(
                &ctx,
                ModelRef {
                    model: "some/model-GGUF".into(),
                },
            )
            .await
            .unwrap_err();
        match unload {
            CommandError::Invalid(m) => {
                assert!(m.contains("serving/unpin"), "names the authority: {m}")
            }
            other => panic!("expected Invalid delegating to serving/unpin, got {other:?}"),
        }
    }
}
