/**
 * Search Params Command Types
 * Get algorithm parameters from Rust SearchModule
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

export interface SearchParamsParams extends CommandParams {
  algorithm: string;  // 'bow', 'bm25', 'cosine'
}

export interface SearchParamsResult extends CommandResult {
  algorithm: string;
  params: string[];
  values: Record<string, unknown>;
}
