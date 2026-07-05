/**
 * Recipe List Command — List available recipes with their templates and roles.
 *
 * Recipes are loaded from JSON files via RecipeLoader, not from the database.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { RecipeListParams, RecipeListResult, RecipeSummary } from '../shared/RecipeListTypes';
import { RecipeLoader } from '../../../../system/recipes/server/RecipeLoader';

export class RecipeListServerCommand extends CommandBase<RecipeListParams, RecipeListResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('recipe/list', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<RecipeListResult> {
    const listParams = params as RecipeListParams;
    const limit = listParams.limit || 50;

    const loader = RecipeLoader.getInstance();
    await loader.preloadAllRecipes();

    let recipes = loader.getAllRecipes();

    // Filter by search term
    if (listParams.search) {
      const term = listParams.search.toLowerCase();
      recipes = recipes.filter(r =>
        r.displayName.toLowerCase().includes(term) ||
        r.description?.toLowerCase().includes(term) ||
        r.uniqueId.toLowerCase().includes(term)
      );
    }

    // Filter by tags
    if (listParams.tags && listParams.tags.length > 0) {
      recipes = recipes.filter(r =>
        r.tags?.some(t => listParams.tags!.includes(t))
      );
    }

    // Filter to only recipes with sentinel templates
    if (listParams.withTemplates) {
      recipes = recipes.filter(r =>
        r.sentinelTemplates && r.sentinelTemplates.length > 0
      );
    }

    // Apply limit
    const limited = recipes.slice(0, limit);

    const summaries: RecipeSummary[] = limited.map(r => ({
      id: r.uniqueId,
      uniqueId: r.uniqueId,
      displayName: r.displayName,
      description: r.description || '',
      tags: r.tags || [],
      sentinelTemplates: r.sentinelTemplates || [],
      roleCount: r.roles?.length || 0,
      version: r.version,
    }));

    return transformPayload(params, {
      success: true,
      recipes: summaries,
      total: summaries.length,
    });
  }
}
