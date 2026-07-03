//! `cognition/generate-recipe` — AI-driven recipe generation (typed, stateless).
//!
//! Oxidized generator arm (continuum#1262): given a natural-language description plus the
//! sentinel templates and existing recipe ids available at generation time, runs the
//! configured model to produce a [`RecipeDefinitionShape`] and the structural-validation
//! findings. Holds no module state — [`generate_recipe_with_ai`] is a free async function
//! over the params — so this is a stateless
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand) unit struct: `action_command!`
//! publishes both the descriptor and the runtime object via `inventory`, no `commands()`
//! ceremony.
//!
//! Wire note: the params ARE a [`GenerateRecipeOrchestratorParams`] — the whole
//! `{ request, provider?, model?, temperature? }` payload deserializes into it, matching
//! the legacy arm that read `request` via `p.json("request")` and the scalar overrides via
//! `p.str_opt` / `p.f32_opt`. No wire-shape change; the typed path just deserializes the
//! payload in one step and fails loud on a malformed request.
//!
//! Fail-loud note: inference-dispatch and parser failures propagate as
//! `CommandError::Internal` (provider outage is NOT masked as a parse error). Structural
//! validation findings do NOT fail — they ride back in `RecipeGenerationResponse` so the
//! caller can render them.
//!
//! `access: Internal` — substrate cognition IPC the host invokes to author a recipe, NOT a
//! persona toolbelt verb.

use crate::cognition::generate_recipe::{
    generate_recipe_with_ai, GenerateRecipeOrchestratorParams, RecipeGenerationResponse,
};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Generate a recipe from a natural-language description via AI. Given the description,
    /// available sentinel templates, existing recipe ids, and optional
    /// provider/model/temperature overrides, returns the parsed recipe shape plus any
    /// structural-validation findings. Host-invoked; not a persona toolbelt verb.
    pub struct GenerateRecipe;
    name: "cognition/generate-recipe",
    access: Internal,
    params: GenerateRecipeOrchestratorParams,
    output: RecipeGenerationResponse,
    run(_this, _ctx, params) => {
        generate_recipe_with_ai(params)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. generate-recipe is host-driven recipe
    // authoring cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(GenerateRecipe::NAME, "cognition/generate-recipe");
        assert_eq!(GenerateRecipe::ACCESS, AccessLevel::Internal);
    }
}
