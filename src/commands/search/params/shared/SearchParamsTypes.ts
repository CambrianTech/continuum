/**
 * Search Params Command Types
 * Get algorithm parameters from Rust SearchModule
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

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
