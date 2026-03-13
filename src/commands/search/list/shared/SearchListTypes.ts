/**
 * Search List Command Types
 * Lists available search algorithms from Rust SearchModule
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface SearchListParams extends CommandParams {
  // No additional params needed
}

export interface SearchListResult extends CommandResult {
  algorithms: string[];
}

/**
 * SearchList — Type-safe command executor
 *
 * Usage:
 *   import { SearchList } from '...shared/SearchListTypes';
 *   const result = await SearchList.execute({ ... });
 */
export const SearchList = {
  execute(params: CommandInput<SearchListParams>): Promise<SearchListResult> {
    return Commands.execute<SearchListParams, SearchListResult>('search/list', params as Partial<SearchListParams>);
  },
  commandName: 'search/list' as const,
} as const;

/**
 * Factory function for creating SearchListParams
 */
export const createSearchListParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SearchListParams, 'context' | 'sessionId' | 'userId'>
): SearchListParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SearchListResult with defaults
 */
export const createSearchListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SearchListResult, 'context' | 'sessionId' | 'userId'>
): SearchListResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart search/list-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSearchListResultFromParams = (
  params: SearchListParams,
  differences: Omit<SearchListResult, 'context' | 'sessionId' | 'userId'>
): SearchListResult => transformPayload(params, differences);

