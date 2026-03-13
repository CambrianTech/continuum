/**
 * Search Vector Command Types
 * Vector similarity search via Rust SearchModule
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

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

/**
 * Factory function for creating SearchVectorParams
 */
export const createSearchVectorParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SearchVectorParams, 'context' | 'sessionId' | 'userId'>
): SearchVectorParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SearchVectorResult with defaults
 */
export const createSearchVectorResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SearchVectorResult, 'context' | 'sessionId' | 'userId'>
): SearchVectorResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart search/vector-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSearchVectorResultFromParams = (
  params: SearchVectorParams,
  differences: Omit<SearchVectorResult, 'context' | 'sessionId' | 'userId'>
): SearchVectorResult => transformPayload(params, differences);

