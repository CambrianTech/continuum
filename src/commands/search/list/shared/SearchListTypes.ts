/**
 * Search List Command Types
 * Lists available search algorithms from Rust SearchModule
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

export interface SearchListParams extends CommandParams {
  // No additional params needed
}

export interface SearchListResult extends CommandResult {
  algorithms: string[];
}
