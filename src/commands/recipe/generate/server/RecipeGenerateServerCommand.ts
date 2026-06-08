/**
 * Recipe Generate Command — thin TS shim around `cognition/generate-recipe`.
 *
 * Pre-#1295 this file was 371 LOC owning prompt construction, AI dispatch,
 * JSON parsing, structural validation, and FS I/O. Per the oxidization
 * mission (#1248 umbrella) the prompt+parser+validator moved to Rust at
 * `../core/continuum-core/src/cognition/generate_recipe/` and are exposed
 * via the `cognition/generate-recipe` IPC (#1298 PR-1, #1301 PR-2).
 *
 * What this file owns now (TS-shim concerns only):
 *   1. Validate the JTAG `description` parameter
 *   2. Gather runtime registry state — `TemplateRegistry.list()` for the
 *      available-templates carrier + `RecipeLoader.getInstance().getAllRecipes()`
 *      for the existing-recipe-IDs carrier — and pass both into Rust
 *   3. Call `Commands.execute('cognition/generate-recipe', ...)`
 *   4. On the post-Rust success path: extra sentinel-template existence
 *      check (TemplateRegistry.has — runtime-registry state Rust can't see),
 *      saveRecipe to disk, RecipeLoader.clearCache + reload
 *   5. Map the response into the existing `RecipeGenerateResult` JTAG envelope
 *
 * Outlier-validation pair with codex's #1284 (AIDecisionService) and
 * claude-tab-1's #1276 (VisionInferenceProvider). Same Rust+thin-TS-shim
 * pattern.
 */

import * as fs from 'fs';
import * as path from 'path';
import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { RecipeGenerateParams, RecipeGenerateResult } from '../shared/RecipeGenerateTypes';
import type { RecipeDefinition } from '../../../../system/recipes/shared/RecipeTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { TemplateRegistry } from '../../../../system/sentinel/pipelines/TemplateRegistry';
import { RecipeLoader } from '../../../../system/recipes/server/RecipeLoader';
import type {
  RecipeGenerationRequest,
  RecipeGenerationResponse,
  RecipeTemplateInfo,
} from '@shared/generated/cognition';

export class RecipeGenerateServerCommand extends CommandBase<RecipeGenerateParams, RecipeGenerateResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('recipe/generate', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<RecipeGenerateResult> {
    const genParams = params as RecipeGenerateParams;
    const { description, dryRun = false, hints, provider = 'anthropic' } = genParams;

    if (!description || description.trim().length === 0) {
      return transformPayload(params, {
        success: false,
        error: 'Description is required. Describe the recipe you want in natural language.',
      });
    }

    // Gather the runtime registry state Rust can't see directly. The
    // `cognition/generate-recipe` IPC accepts these as carriers so the
    // Rust prompt builder + validator stay pure (no global state).
    const availableTemplates: RecipeTemplateInfo[] = TemplateRegistry.list().map(t => ({
      name: t.name,
      description: t.description,
      requiredFields: t.requiredFields,
    }));
    const loader = RecipeLoader.getInstance();
    const existingRecipeIds: string[] = loader.getAllRecipes().map(r => r.uniqueId);

    const request: RecipeGenerationRequest = {
      description,
      availableTemplates,
      existingRecipeIds,
      hints: hints ?? undefined,
      uniqueIdOverride: genParams.uniqueId,
    };

    let response: RecipeGenerationResponse;
    try {
      // Two-generic signature: <TParams, TResult>. We don't have a typed
      // params struct (the IPC accepts the loose envelope), so use the
      // default CommandParams + cast the result through unknown to the
      // typed RecipeGenerationResponse.
      const ipcResult = await Commands.execute('cognition/generate-recipe', {
        request,
        provider,
        model: genParams.model,
      } as unknown as Record<string, unknown>);
      response = ipcResult as unknown as RecipeGenerationResponse;
    } catch (error) {
      // Inference / parse failures propagate from Rust as Err. Map to the
      // existing JTAG envelope shape so the CLI / programmatic callers
      // see the same error contract as pre-#1295.
      return transformPayload(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      });
    }

    const recipe = response.recipe as RecipeDefinition;
    const validationErrors = [...response.validationErrors];

    // Extra TS-side validation: sentinel-template existence is runtime-registry
    // state the Rust validator can't see (it only knows what's in the carrier
    // list it received). Run this AFTER Rust's structural validation so the
    // error list is comprehensive.
    if (recipe.sentinelTemplates) {
      for (const tmpl of recipe.sentinelTemplates) {
        if (!TemplateRegistry.has(tmpl)) {
          validationErrors.push(
            `sentinelTemplate "${tmpl}" is not registered. Available: ${TemplateRegistry.list().map(t => t.name).join(', ')}`,
          );
        }
      }
    }

    if (validationErrors.length > 0) {
      return transformPayload(params, {
        success: false,
        recipe,
        validationErrors,
        error: `Generated recipe has ${validationErrors.length} validation error(s).`,
      });
    }

    // Save (unless dryRun) — file I/O stays TS because it's a JTAG
    // framework concern, not a cognition concern.
    let savedTo: string | undefined;
    if (!dryRun) {
      savedTo = this.saveRecipe(recipe);
      loader.clearCache();
      await loader.loadRecipe(recipe.uniqueId);
    }

    return transformPayload(params, {
      success: true,
      recipe,
      savedTo,
    });
  }

  private saveRecipe(recipe: RecipeDefinition): string {
    const recipesDir = path.join(__dirname, '..', '..', '..', '..', 'system', 'recipes');
    const filePath = path.join(recipesDir, `${recipe.uniqueId}.json`);
    const json = JSON.stringify(recipe, null, 2) + '\n';
    fs.writeFileSync(filePath, json, 'utf-8');
    return filePath;
  }
}
