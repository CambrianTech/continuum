/**
 * Run a recipe's sentinel template with role validation.
 *
 * Higher-level wrapper around sentinel/run that:
 * 1. Looks up the recipe by uniqueId
 * 2. Validates model availability for the recipe's role requirements
 * 3. Dispatches to sentinel/run with the matching template
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

export interface RecipeRunParams extends CommandParams {
  /** Recipe uniqueId (e.g., 'coding', 'creative-writing', 'research') */
  recipe: string;

  /** Which sentinel template to use (must be in recipe's sentinelTemplates list).
   *  If omitted and recipe has exactly one template, uses that. */
  template?: string;

  /** Template-specific config (passed to the template builder) */
  config: Record<string, unknown>;

  /** Skip role/capability validation (default: false) */
  skipValidation?: boolean;
}

export interface RecipeRunResult extends CommandResult {
  success: boolean;
  /** Sentinel handle for tracking progress */
  handle?: string;
  /** Recipe that was executed */
  recipe: string;
  /** Template that was used */
  template: string;
  /** Role validation results (if not skipped) */
  roleValidation?: {
    viable: boolean;
    unfilledRoles: string[];
  };
  /** Error message if failed */
  error?: string;
}

export const RecipeRun = {
  execute(params: CommandInput<RecipeRunParams>): Promise<RecipeRunResult> {
    return Commands.execute<RecipeRunParams, RecipeRunResult>('recipe/run', params as Partial<RecipeRunParams>);
  },
  commandName: 'recipe/run' as const,
} as const;
