/**
 * Vector Search Command - Shared Types
 *
 * Performs semantic search over a collection using vector similarity.
 */

import type { CommandParams, JTAGPayload, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type {
  VectorSearchResult,
  VectorEmbedding,
  EmbeddingModel
} from '../../../../daemons/data-daemon/shared/VectorSearchTypes';
import type { RecordData } from '../../../../daemons/data-daemon/shared/DataStorageAdapter';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Vector search command parameters
 */
export interface VectorSearchParams extends CommandParams {
  readonly collection: string;

  // Database handle for per-persona databases (optional - uses main db if not provided)
  readonly dbHandle?: string;

  // Query can be text (will generate embedding) OR pre-computed vector
  readonly queryText?: string;
  readonly queryVector?: VectorEmbedding;

  // Search parameters
  readonly k?: number;                          // Number of results (default: 10)
  readonly similarityThreshold?: number;        // Min similarity 0-1 (default: 0.0)

  // Hybrid search (semantic + keyword/metadata)
  readonly hybridMode?: 'semantic' | 'keyword' | 'hybrid';
  readonly filter?: Record<string, any>;        // Metadata filters

  // Model selection
  readonly embeddingModel?: string;             // Model name: 'all-minilm' | 'nomic-embed-text'
  readonly embeddingProvider?: string;          // Provider: 'candle' | 'openai' | 'huggingface'
}

/**
 * Vector search command result
 */
export interface VectorSearchResult_CLI<T extends RecordData = RecordData> extends JTAGPayload {
  readonly success: boolean;
  readonly results: VectorSearchResult<T>[];
  readonly totalResults: number;
  readonly queryVector?: VectorEmbedding;
  readonly metadata: {
    readonly collection: string;
    readonly searchMode: 'semantic' | 'keyword' | 'hybrid';
    readonly embeddingModel: string;
    readonly queryTime: number;
  };
  readonly error?: string;
}

export const createVectorSearchParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<VectorSearchParams, 'context' | 'sessionId'>
): VectorSearchParams => createPayload(context, sessionId, data);

export const createVectorSearchResultFromParams = <T extends RecordData>(
  params: VectorSearchParams,
  differences: Omit<Partial<VectorSearchResult_CLI<T>>, 'context' | 'sessionId'>
): VectorSearchResult_CLI<T> => transformPayload(params, {
  success: false,
  results: [],
  totalResults: 0,
  metadata: {
    collection: params.collection,
    searchMode: params.hybridMode || 'semantic',
    embeddingModel: params.embeddingModel || 'all-minilm',
    queryTime: 0
  },
  ...differences
});

/**
 * VectorSearch — Type-safe command executor
 *
 * Usage:
 *   import { VectorSearch } from '...shared/VectorSearchTypes';
 *   const result = await VectorSearch.execute({ ... });
 */
export const VectorSearch = {
  execute<T extends RecordData = RecordData>(params: CommandInput<VectorSearchParams>): Promise<VectorSearchResult_CLI<T>> {
    return Commands.execute<VectorSearchParams, VectorSearchResult_CLI<T>>('data/vector-search', params as Partial<VectorSearchParams>);
  },
  commandName: 'data/vector-search' as const,
} as const;
