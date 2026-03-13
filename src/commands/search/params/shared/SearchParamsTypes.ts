/**
 * Search Params Command Types
 * Get algorithm parameters from Rust SearchModule
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface SearchParamsParams extends CommandParams {
  algorithm: string;  // 'bow', 'bm25', 'cosine'
}

export interface SearchParamsResult extends CommandResult {
  algorithm: string;
  params: string[];
  values: Record<string, unknown>;
}

/**
 * SearchParams — Type-safe command executor
 *
 * Usage:
 *   import { SearchParams } from '...shared/SearchParamsTypes';
 *   const result = await SearchParams.execute({ ... });
 */
export const SearchParams = {
  execute(params: CommandInput<SearchParamsParams>): Promise<SearchParamsResult> {
    return Commands.execute<SearchParamsParams, SearchParamsResult>('search/params', params as Partial<SearchParamsParams>);
  },
  commandName: 'search/params' as const,
} as const;

/**
 * Factory function for creating SearchParamsParams
 */
export const createSearchParamsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SearchParamsParams, 'context' | 'sessionId' | 'userId'>
): SearchParamsParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SearchParamsResult with defaults
 */
export const createSearchParamsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SearchParamsResult, 'context' | 'sessionId' | 'userId'>
): SearchParamsResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart search/params-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSearchParamsResultFromParams = (
  params: SearchParamsParams,
  differences: Omit<SearchParamsResult, 'context' | 'sessionId' | 'userId'>
): SearchParamsResult => transformPayload(params, differences);

