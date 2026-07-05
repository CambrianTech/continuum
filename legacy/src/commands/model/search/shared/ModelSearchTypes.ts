/**
 * Model Search Command - Shared Types
 *
 * Search HuggingFace for base models by name, architecture, or size. Used to find compaction targets (e.g., 'Qwen 3.5 27B'). Different from adapter/search which finds LoRA adapters.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Model Search Command Parameters
 */
export interface ModelSearchParams extends CommandParams {
  // Search query (e.g., 'Qwen3.5', 'codellama', 'mistral 7b')
  query: string;
  // Max results to return. Default: 10.
  limit?: number;
  // Sort by: 'downloads', 'likes', 'recent'. Default: 'downloads'.
  sort?: string;
  // Minimum model size in billions of parameters (e.g., 7, 14, 27)
  minSize?: number;
  // Maximum model size in billions of parameters
  maxSize?: number;
}

/**
 * Factory function for creating ModelSearchParams
 */
export const createModelSearchParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Search query (e.g., 'Qwen3.5', 'codellama', 'mistral 7b')
    query: string;
    // Max results to return. Default: 10.
    limit?: number;
    // Sort by: 'downloads', 'likes', 'recent'. Default: 'downloads'.
    sort?: string;
    // Minimum model size in billions of parameters (e.g., 7, 14, 27)
    minSize?: number;
    // Maximum model size in billions of parameters
    maxSize?: number;
  }
): ModelSearchParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  limit: data.limit ?? 0,
  sort: data.sort ?? '',
  minSize: data.minSize ?? 0,
  maxSize: data.maxSize ?? 0,
  ...data
});

/**
 * Model Search Command Result
 */
export interface ModelSearchResult extends CommandResult {
  success: boolean;
  // Array of { id, author, downloads, likes, tags, pipeline_tag, lastModified }
  models: object;
  // Total number of results found
  totalCount: number;
  error?: JTAGError;
}

/**
 * Factory function for creating ModelSearchResult with defaults
 */
export const createModelSearchResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Array of { id, author, downloads, likes, tags, pipeline_tag, lastModified }
    models?: object;
    // Total number of results found
    totalCount?: number;
    error?: JTAGError;
  }
): ModelSearchResult => createPayload(context, sessionId, {
  models: data.models ?? {},
  totalCount: data.totalCount ?? 0,
  ...data
});

/**
 * Smart Model Search-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createModelSearchResultFromParams = (
  params: ModelSearchParams,
  differences: Omit<ModelSearchResult, 'context' | 'sessionId' | 'userId'>
): ModelSearchResult => transformPayload(params, differences);

/**
 * Model Search — Type-safe command executor
 *
 * Usage:
 *   import { ModelSearch } from '...shared/ModelSearchTypes';
 *   const result = await ModelSearch.execute({ ... });
 */
export const ModelSearch = {
  execute(params: CommandInput<ModelSearchParams>): Promise<ModelSearchResult> {
    return Commands.execute<ModelSearchParams, ModelSearchResult>('model/search', params as Partial<ModelSearchParams>);
  },
  commandName: 'model/search' as const,
} as const;
