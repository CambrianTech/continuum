/**
 * List available recipes with optional filters.
 *
 * Recipes define collaborative workflow activities — what personas do together.
 * Each recipe can declare sentinel templates (workflows) and role requirements.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

export interface RecipeListParams extends CommandParams {
  /** Filter by tags (any match) */
  tags?: string[];

  /** Limit results (default: 50) */
  limit?: number;

  /** Search by name or description */
  search?: string;

  /** Only show recipes with sentinel templates */
  withTemplates?: boolean;
}

export interface RecipeSummary {
  id: string;
  uniqueId: string;
  displayName: string;
  description: string;
  tags: string[];
  sentinelTemplates: string[];
  roleCount: number;
  version?: number;
}

export interface RecipeListResult extends CommandResult {
  success: boolean;
  recipes: RecipeSummary[];
  total: number;
}

export const RecipeList = {
  execute(params: CommandInput<RecipeListParams>): Promise<RecipeListResult> {
    return Commands.execute<RecipeListParams, RecipeListResult>('recipe/list', params as Partial<RecipeListParams>);
  },
  commandName: 'recipe/list' as const,
} as const;
