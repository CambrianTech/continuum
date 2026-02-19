/**
 * Recipe Load Command Types
 *
 * Loads recipe JSON files from system/recipes/*.json into database
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { RecipeEntity } from '@system/data/entities/RecipeEntity';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface RecipeLoadParams extends CommandParams {
  // Load specific recipe by uniqueId
  readonly recipeId?: string;  // 'general-chat', 'academy-collaborative', etc.

  // Or load all recipes
  readonly loadAll?: boolean;

  // Reload existing (update if already exists)
  readonly reload?: boolean;
}

export interface RecipeLoadResult extends CommandResult {
  readonly success: boolean;
  readonly loaded: RecipeEntity[];
  readonly skipped?: string[];  // Already exists + not reload mode
  readonly errors?: Array<{ recipeId: string; error: string }>;
}

/**
 * RecipeLoad — Type-safe command executor
 *
 * Usage:
 *   import { RecipeLoad } from '...shared/RecipeLoadTypes';
 *   const result = await RecipeLoad.execute({ ... });
 */
export const RecipeLoad = {
  execute(params: CommandInput<RecipeLoadParams>): Promise<RecipeLoadResult> {
    return Commands.execute<RecipeLoadParams, RecipeLoadResult>('workspace/recipe/load', params as Partial<RecipeLoadParams>);
  },
  commandName: 'workspace/recipe/load' as const,
} as const;
