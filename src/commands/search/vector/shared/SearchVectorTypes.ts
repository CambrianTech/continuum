/**
 * Search Vector Command Types
 * Vector similarity search via Rust SearchModule
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

export interface SearchVectorParams extends CommandParams {
  queryVector: number[];
  corpusVectors: number[][];
  normalize?: boolean;  // Defaults to true
  threshold?: number;   // Defaults to 0.0
}

export interface SearchVectorResult extends CommandResult {
  algorithm: string;  // Always 'cosine' for vector search
  scores: number[];
  rankedIndices: number[];
}

/**
 * SearchVector — Type-safe command executor
 *
 * Usage:
 *   import { SearchVector } from '...shared/SearchVectorTypes';
 *   const result = await SearchVector.execute({ ... });
 */
export const SearchVector = {
  execute(params: CommandInput<SearchVectorParams>): Promise<SearchVectorResult> {
    return Commands.execute<SearchVectorParams, SearchVectorResult>('search/vector', params as Partial<SearchVectorParams>);
  },
  commandName: 'search/vector' as const,
} as const;
