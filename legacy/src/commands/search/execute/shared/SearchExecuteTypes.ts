/**
 * Search Execute Command Types
 * Executes text search via Rust SearchModule
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface SearchExecuteParams extends CommandParams {
  algorithm?: string;  // 'bow', 'bm25', 'cosine' - defaults to 'bm25'
  query: string;
  corpus: string[];
  params?: Record<string, unknown>;  // Algorithm-specific params
}

export interface SearchExecuteResult extends CommandResult {
  algorithm: string;
  scores: number[];
  rankedIndices: number[];
}

/**
 * SearchExecute — Type-safe command executor
 *
 * Usage:
 *   import { SearchExecute } from '...shared/SearchExecuteTypes';
 *   const result = await SearchExecute.execute({ ... });
 */
export const SearchExecute = {
  execute(params: CommandInput<SearchExecuteParams>): Promise<SearchExecuteResult> {
    return Commands.execute<SearchExecuteParams, SearchExecuteResult>('search/execute', params as Partial<SearchExecuteParams>);
  },
  commandName: 'search/execute' as const,
} as const;

/**
 * Factory function for creating SearchExecuteParams
 */
export const createSearchExecuteParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SearchExecuteParams, 'context' | 'sessionId' | 'userId'>
): SearchExecuteParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SearchExecuteResult with defaults
 */
export const createSearchExecuteResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SearchExecuteResult, 'context' | 'sessionId' | 'userId'>
): SearchExecuteResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart search/execute-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSearchExecuteResultFromParams = (
  params: SearchExecuteParams,
  differences: Omit<SearchExecuteResult, 'context' | 'sessionId' | 'userId'>
): SearchExecuteResult => transformPayload(params, differences);

