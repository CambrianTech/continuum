/**
 * Search Execute Command Types
 * Executes text search via Rust SearchModule
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

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
