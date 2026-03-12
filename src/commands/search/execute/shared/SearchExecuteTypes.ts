/**
 * Search Execute Command Types
 * Executes text search via Rust SearchModule
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

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
