/**
 * Search Vector Command Types
 * Vector similarity search via Rust SearchModule
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

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
