/**
 * Recipe Load Command Types
 *
 * Loads recipe JSON files from system/recipes/*.json into database
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { RecipeEntity } from '@system/data/entities/RecipeEntity';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

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

/**
 * Factory function for creating WorkspaceRecipeLoadParams
 */
export const createWorkspaceRecipeLoadParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<RecipeLoadParams, 'context' | 'sessionId' | 'userId'>
): RecipeLoadParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating WorkspaceRecipeLoadResult with defaults
 */
export const createWorkspaceRecipeLoadResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<RecipeLoadResult, 'context' | 'sessionId' | 'userId'>
): RecipeLoadResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart workspace/recipe/load-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createWorkspaceRecipeLoadResultFromParams = (
  params: RecipeLoadParams,
  differences: Omit<RecipeLoadResult, 'context' | 'sessionId' | 'userId'>
): RecipeLoadResult => transformPayload(params, differences);

