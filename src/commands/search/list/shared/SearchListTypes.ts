/**
 * Search List Command Types
 * Lists available search algorithms from Rust SearchModule
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

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
