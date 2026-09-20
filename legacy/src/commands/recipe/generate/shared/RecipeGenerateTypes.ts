/**
 * Generate a recipe from a natural language description.
 *
 * Uses an LLM to produce valid RecipeDefinition JSON from English,
 * validates the output against the schema, and saves it as a recipe file.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import type { RecipeDefinition } from '../../../../system/recipes/shared/RecipeTypes';

export interface RecipeGenerateParams extends CommandParams {
  /** Natural language description of the desired recipe */
  description: string;

  /** Optional uniqueId override (default: derived from description) */
  uniqueId?: string;

  /** AI provider to use for generation (default: 'anthropic') */
  provider?: string;

  /** Model to use (default: provider's best) */
  model?: string;

  /** If true, return generated JSON without saving to disk (default: false) */
  dryRun?: boolean;

  /** Optional hints about the recipe domain */
  hints?: {
    /** Category: 'coding', 'creative', 'research', 'education', 'game', etc. */
    category?: string;
    /** Sentinel templates to include (must exist in TemplateRegistry) */
    templates?: string[];
    /** Tags to apply */
    tags?: string[];
    /** Conversation pattern hint */
    pattern?: string;
  };
}

export interface RecipeGenerateResult extends CommandResult {
  success: boolean;
  /** The generated recipe definition */
  recipe?: RecipeDefinition;
  /** File path where the recipe was saved (if not dryRun) */
  savedTo?: string;
  /** Validation errors (if generation produced invalid JSON) */
  validationErrors?: string[];
  /** Error message if failed */
  error?: string;
}

export const RecipeGenerate = {
  execute(params: CommandInput<RecipeGenerateParams>): Promise<RecipeGenerateResult> {
    return Commands.execute<RecipeGenerateParams, RecipeGenerateResult>('recipe/generate', params as Partial<RecipeGenerateParams>);
  },
  commandName: 'recipe/generate' as const,
} as const;
