//! `ai/inference/{status,models,load,unload}` — the model-lifecycle command
//! surface over the inference gateway.
//!
//! These are the FIRST-CLASS, discoverable commands for "what can I run, which
//! model is active, load this one, unload that one" — the same operations the
//! keystone ([`crate::inference::unsloth_control`]) performs internally on the
//! hot path ([`ensure_model_active`]), now also exposed as consistent commands
//! so an operator (or a privileged peer) drives the gateway through the uniform
//! command surface (`cu ai/inference/status`) instead of hand-rolled HTTP. They
//! join the existing `ai/inference/{open,generate,close,inspect}` family —
//! lifecycle alongside the session handles, one namespace.
//!
//! ## Shape
//! Stateless [`ActionCommand`]s (no host [`ServiceModule`] —
//! [[command-infra-self-routing-schema-adapters]]): each builds an
//! [`UnslothHttp`] from config and delegates to its lifecycle methods. The
//! command NAMES are provider-agnostic (`ai/inference/*`); the impl targets the
//! configured gateway. A second provider with a loadable model lifecycle would
//! lift these calls onto a trait — generalize on the second implementor (the
//! outlier-validation rule), not before.
//!
//! ## Access
//! `status` / `models` are read-only → [`AccessLevel::AiSafe`] (any citizen may
//! ask "what's loaded"). `load` / `unload` MUTATE the shared engine
//! (single-resident: a load swaps the model EVERY persona sees) →
//! [`AccessLevel::Privileged`]. The persona hot path never calls these; it
//! self-heals through [`ensure_model_active`] inside the adapter.
//!
//! ## Degrade vs fail
//! These are operator queries, not the persona hot path: a gateway that is
//! unreachable → FAIL LOUD ([`CommandError::Internal`] naming the gateway), so a
//! human asking "what's loaded?" hears "the engine is down", never an empty list
//! that reads as "nothing loaded" ([[fallbacks-are-illegal-fail-loud]]). (The
//! hot path's `ensure_model_active` degrades instead — there, dropping to the
//! persona's lexical-recall path keeps the substrate alive.)

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::inference::unsloth_control::{InferenceStatus, LocalModel, UnslothError, UnslothHttp};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// Map a gateway failure to a LOUD command error naming the op — never a silent
/// empty result. Operator-facing commands must surface "the engine is down".
fn gateway_err(op: &str, e: UnslothError) -> CommandError {
    CommandError::Internal(format!("ai/inference/{op}: inference gateway {e}"))
}

// ─────────────────────────── ai/inference/status ─────────────────

/// `ai/inference/status` has no params — it asks the gateway about itself.
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
        "Report which model the inference engine is serving right now (active_model), \
         plus the resident/loading sets and the active model's context window. The \
         active model is the ONE that answers — the gateway ignores the per-request \
         model field, so this is how you confirm which brain is live.";
    type Params = StatusParams;
    type Output = InferenceStatus;

    async fn run(&self, _ctx: &Ctx, _p: StatusParams) -> Result<InferenceStatus, CommandError> {
        UnslothHttp::from_config()
            .status()
            .await
            .map_err(|e| gateway_err("status", e))
    }
}

// ─────────────────────────── ai/inference/models ─────────────────

/// `ai/inference/models` has no params — it lists what the gateway can load.
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
    /// Models discovered on disk that `ai/inference/load` can load.
    pub available: Vec<LocalModel>,
    /// The ids the gateway is serving right now (`/v1/models`) — a subset that
    /// is resident + ready, vs `available` which is everything on disk.
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
         (each id is what ai/inference/load takes), which ids are serving right now, \
         and which single model is active. Use to discover what you can switch to.";
    type Params = ModelsParams;
    type Output = ModelsResult;

    async fn run(&self, _ctx: &Ctx, _p: ModelsParams) -> Result<ModelsResult, CommandError> {
        // One client (shared connection pool) for all three calls this command makes.
        let api = UnslothHttp::from_config();
        let available = api
            .local_models()
            .await
            .map_err(|e| gateway_err("models", e))?;
        let serving = api.list_models().await.map_err(|e| gateway_err("models", e))?;
        let active = api.status().await.map_err(|e| gateway_err("models", e))?.active_model;
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
        "Load a model into the inference engine and make it active. The engine serves \
         one resident model, so this swaps what every persona generates with. Returns \
         the new engine status. Privileged — the persona hot path auto-loads its own \
         model and never calls this.";
    type Params = ModelRef;
    type Output = InferenceStatus;

    async fn run(&self, _ctx: &Ctx, p: ModelRef) -> Result<InferenceStatus, CommandError> {
        if p.model.trim().is_empty() {
            return Err(CommandError::Invalid(
                "ai/inference/load: 'model' is required (the id to load)".into(),
            ));
        }
        let api = UnslothHttp::from_config();
        api.load_model(&p.model)
            .await
            .map_err(|e| gateway_err("load", e))?;
        // Return the post-load status so the caller sees it became active.
        api.status().await.map_err(|e| gateway_err("load", e))
    }
}

// ─────────────────────────── ai/inference/unload ─────────────────

#[derive(Default)]
pub struct AiInferenceUnload;

#[async_trait]
impl ActionCommand for AiInferenceUnload {
    const NAME: &'static str = "ai/inference/unload";
    // Privileged: unloading frees the engine that every persona depends on.
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Unload a model from the inference engine to free VRAM. Privileged — affects \
         every persona depending on the engine. Returns the engine status after the \
         unload.";
    type Params = ModelRef;
    type Output = InferenceStatus;

    async fn run(&self, _ctx: &Ctx, p: ModelRef) -> Result<InferenceStatus, CommandError> {
        if p.model.trim().is_empty() {
            return Err(CommandError::Invalid(
                "ai/inference/unload: 'model' is required (the id to unload)".into(),
            ));
        }
        let api = UnslothHttp::from_config();
        api.unload_model(&p.model)
            .await
            .map_err(|e| gateway_err("unload", e))?;
        api.status().await.map_err(|e| gateway_err("unload", e))
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
    // open, but load/unload mutate the single shared engine and MUST stay
    // Privileged (an AiSafe load would let any persona swap everyone's brain).
    #[test]
    fn reads_are_aisafe_mutations_are_privileged() {
        assert_eq!(AiInferenceStatus::ACCESS, AccessLevel::AiSafe);
        assert_eq!(AiInferenceModels::ACCESS, AccessLevel::AiSafe);
        assert_eq!(AiInferenceLoad::ACCESS, AccessLevel::Privileged);
        assert_eq!(AiInferenceUnload::ACCESS, AccessLevel::Privileged);
    }

    // what this catches: a blank model id is rejected with a typed Invalid BEFORE
    // any network call — load/unload must never POST an empty model_path to the
    // gateway. (No gateway needed; the guard is pure.)
    #[tokio::test]
    async fn blank_model_is_rejected_without_network() {
        let ctx = Ctx::default();
        let err = AiInferenceLoad
            .run(&ctx, ModelRef { model: "  ".into() })
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)));
        let err = AiInferenceUnload
            .run(&ctx, ModelRef { model: String::new() })
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)));
    }
}
