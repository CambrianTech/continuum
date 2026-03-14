/**
 * Recipe Run Command — Execute a recipe's sentinel template with role validation.
 *
 * Flow:
 * 1. Load recipe by uniqueId
 * 2. Resolve which template to use
 * 3. Validate model availability for recipe roles (via RecipeAssembler)
 * 4. Dispatch to sentinel/run with the template
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { RecipeRunParams, RecipeRunResult } from '../shared/RecipeRunTypes';
import { RecipeLoader } from '../../../../system/recipes/server/RecipeLoader';
import { RecipeAssembler } from '../../../../system/sentinel/RecipeAssembler';
import { TemplateRegistry } from '../../../../system/sentinel/pipelines/TemplateRegistry';
import { Commands } from '@system/core/shared/Commands';

export class RecipeRunServerCommand extends CommandBase<RecipeRunParams, RecipeRunResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('recipe/run', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<RecipeRunResult> {
    const runParams = params as RecipeRunParams;
    const { recipe: recipeId, config, skipValidation = false } = runParams;

    // 1. Load recipe
    const loader = RecipeLoader.getInstance();
    const recipe = await loader.loadRecipe(recipeId);

    if (!recipe) {
      return transformPayload(params, {
        success: false,
        recipe: recipeId,
        template: '',
        error: `Recipe '${recipeId}' not found`,
      });
    }

    // 2. Resolve template
    const templates = recipe.sentinelTemplates || [];
    if (templates.length === 0) {
      return transformPayload(params, {
        success: false,
        recipe: recipeId,
        template: '',
        error: `Recipe '${recipeId}' has no sentinel templates. It's a chat-only activity.`,
      });
    }

    let templateName = runParams.template;
    if (!templateName) {
      if (templates.length === 1) {
        templateName = templates[0];
      } else {
        return transformPayload(params, {
          success: false,
          recipe: recipeId,
          template: '',
          error: `Recipe '${recipeId}' has ${templates.length} templates: ${templates.join(', ')}. Specify --template.`,
        });
      }
    }

    // Verify template is in the recipe's allowed list
    if (!templates.includes(templateName)) {
      return transformPayload(params, {
        success: false,
        recipe: recipeId,
        template: templateName,
        error: `Template '${templateName}' is not in recipe '${recipeId}'. Available: ${templates.join(', ')}`,
      });
    }

    // Verify template exists in registry
    if (!TemplateRegistry.has(templateName)) {
      return transformPayload(params, {
        success: false,
        recipe: recipeId,
        template: templateName,
        error: `Template '${templateName}' is not registered in TemplateRegistry`,
      });
    }

    // 3. Validate roles (optional)
    let roleValidation: RecipeRunResult['roleValidation'];
    if (!skipValidation && recipe.roles && recipe.roles.length > 0) {
      const assembler = new RecipeAssembler();
      const assembly = assembler.assembleTeam(recipe);
      roleValidation = {
        viable: assembly.viable,
        unfilledRoles: assembly.unfilledRoles.map(r => r.role),
      };

      if (!assembly.viable) {
        const missing = assembly.unfilledRoles.map(r =>
          `${r.role} (needs: ${r.missingCapabilities.join(', ')})`
        ).join('; ');
        return transformPayload(params, {
          success: false,
          recipe: recipeId,
          template: templateName,
          roleValidation,
          error: `Cannot fill required roles: ${missing}`,
        });
      }
    }

    // 4. Dispatch to sentinel/run
    try {
      const sentinelResult = await Commands.execute('sentinel/run', {
        type: 'pipeline' as const,
        template: templateName,
        templateConfig: config,
        async: true,
      } as Record<string, unknown>) as unknown as { success: boolean; handle?: string };

      return transformPayload(params, {
        success: sentinelResult.success,
        handle: sentinelResult.handle,
        recipe: recipeId,
        template: templateName,
        roleValidation,
      });
    } catch (error) {
      return transformPayload(params, {
        success: false,
        recipe: recipeId,
        template: templateName,
        roleValidation,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }
}
