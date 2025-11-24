/**
 * Vector Search Types - Semantic Search for DataDaemon
 *
 * Extends DataStorageAdapter with vector similarity search capabilities.
 * Enables semantic discovery of memories, genome adapters, and any content.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { RecordData, UniversalFilter, StorageResult } from './DataStorageAdapter';

/**
 * Vector embedding - array of floats representing semantic meaning
 */
export type VectorEmbedding = number[];

/**
 * Embedding model configuration
 */
export interface EmbeddingModel {
  readonly name: string;           // e.g., 'all-minilm-l6-v2', 'nomic-embed-text'
  readonly dimensions: number;     // e.g., 384, 768
  readonly provider: 'ollama' | 'openai' | 'huggingface';
  readonly maxTokens?: number;
}

/**
 * Vector search query options
 */
export interface VectorSearchOptions {
  readonly collection: string;

  // Query can be text (will generate embedding) OR pre-computed vector
  readonly queryText?: string;
  readonly queryVector?: VectorEmbedding;

  // Search parameters
  readonly k?: number;                          // Number of results (default: 10)
  readonly similarityThreshold?: number;        // Min similarity 0-1 (default: 0.0)

  // Hybrid search (semantic + keyword/metadata)
  readonly hybridMode?: 'semantic' | 'keyword' | 'hybrid';
  readonly hybridRatio?: number;                // 0-1: weight of semantic vs keyword (default: 0.5)
  readonly filter?: UniversalFilter;            // Metadata filters (same as regular queries)

  // Model selection
  readonly embeddingModel?: EmbeddingModel;     // Override default model

  // Pagination
  readonly offset?: number;
  readonly limit?: number;
}

/**
 * Vector search result with similarity score
 */
export interface VectorSearchResult<T extends RecordData = RecordData> {
  readonly id: UUID;
  readonly data: T;
  readonly score: number;          // Similarity score 0-1 (1 = identical)
  readonly distance: number;       // Vector distance (lower = more similar)
  readonly metadata?: {
    readonly collection: string;
    readonly embeddingModel?: string;
    readonly queryTime?: number;   // ms
  };
}

/**
 * Full vector search response
 */
export interface VectorSearchResponse<T extends RecordData = RecordData> {
  readonly results: VectorSearchResult<T>[];
  readonly totalResults: number;
  readonly queryVector?: VectorEmbedding;       // The query vector used (for debugging)
  readonly metadata: {
    readonly collection: string;
    readonly searchMode: 'semantic' | 'keyword' | 'hybrid';
    readonly embeddingModel: string;
    readonly queryTime: number;                 // ms
    readonly cacheHit?: boolean;
  };
}

/**
 * Embedding generation request
 */
export interface GenerateEmbeddingRequest {
  readonly text: string;
  readonly model?: EmbeddingModel;
}

/**
 * Embedding generation response
 */
export interface GenerateEmbeddingResponse {
  readonly embedding: VectorEmbedding;
  readonly model: EmbeddingModel;
  readonly tokenCount?: number;
  readonly generationTime?: number;             // ms
}

/**
 * Index vector request - store embedding for a record
 */
export interface IndexVectorRequest {
  readonly collection: string;
  readonly id: UUID;
  readonly embedding: VectorEmbedding;
  readonly metadata?: {
    readonly embeddingModel?: string;
    readonly generatedAt?: string;
  };
}

/**
 * Backfill vectors request - generate embeddings for existing records
 */
export interface BackfillVectorsRequest {
  readonly collection: string;
  readonly textField: string;                   // Field to generate embeddings from (e.g., 'content')
  readonly filter?: UniversalFilter;            // Only backfill matching records
  readonly batchSize?: number;                  // Process N records at a time (default: 100)
  readonly model?: EmbeddingModel;
}

/**
 * Backfill vectors progress
 * Mutable object that accumulates state during backfill operation
 */
export interface BackfillVectorsProgress {
  total: number;
  processed: number;
  failed: number;
  elapsedTime: number;                 // ms
  estimatedRemaining?: number;         // ms
}

/**
 * Vector index statistics
 */
export interface VectorIndexStats {
  readonly collection: string;
  readonly totalRecords: number;
  readonly recordsWithVectors: number;
  readonly vectorDimensions: number;
  readonly embeddingModel?: string;
  readonly indexSize?: number;                  // bytes
  readonly lastUpdated?: string;
}

/**
 * Vector Search Capabilities - extends StorageCapabilities
 */
export interface VectorSearchCapabilities {
  readonly supportsVectorSearch: boolean;
  readonly supportsHybridSearch: boolean;
  readonly supportsEmbeddingGeneration: boolean;
  readonly maxVectorDimensions: number;
  readonly supportedSimilarityMetrics: ('cosine' | 'euclidean' | 'dot-product')[];
  readonly embeddingProviders: ('ollama' | 'openai' | 'huggingface')[];
}

/**
 * Default embedding models
 */
export const DEFAULT_EMBEDDING_MODELS: Record<string, EmbeddingModel> = {
  'all-minilm': {
    name: 'all-minilm-l6-v2',
    dimensions: 384,
    provider: 'ollama',
    maxTokens: 512
  },
  'nomic-embed-text': {
    name: 'nomic-embed-text',
    dimensions: 768,
    provider: 'ollama',
    maxTokens: 8192
  },
  'text-embedding-3-small': {
    name: 'text-embedding-3-small',
    dimensions: 1536,
    provider: 'openai',
    maxTokens: 8191
  }
};

/**
 * Vector Search Adapter Interface Extension
 *
 * Adapters that support vector search should implement these methods.
 * Adapters without vector support can throw NotImplementedError.
 */
export interface VectorSearchAdapter {
  /**
   * Perform vector similarity search
   */
  vectorSearch<T extends RecordData>(
    options: VectorSearchOptions
  ): Promise<StorageResult<VectorSearchResponse<T>>>;

  /**
   * Generate embedding for text
   */
  generateEmbedding(
    request: GenerateEmbeddingRequest
  ): Promise<StorageResult<GenerateEmbeddingResponse>>;

  /**
   * Index vector for a record
   */
  indexVector(
    request: IndexVectorRequest
  ): Promise<StorageResult<boolean>>;

  /**
   * Backfill embeddings for existing records
   */
  backfillVectors(
    request: BackfillVectorsRequest,
    onProgress?: (progress: BackfillVectorsProgress) => void
  ): Promise<StorageResult<BackfillVectorsProgress>>;

  /**
   * Get vector index statistics
   */
  getVectorIndexStats(
    collection: string
  ): Promise<StorageResult<VectorIndexStats>>;

  /**
   * Get vector search capabilities
   */
  getVectorSearchCapabilities(): Promise<VectorSearchCapabilities>;
}

/**
 * Similarity metric functions
 */
export const SimilarityMetrics = {
  /**
   * Cosine similarity: measures angle between vectors (0-1, 1 = identical)
   */
  cosine(a: VectorEmbedding, b: VectorEmbedding): number {
    if (a.length !== b.length) {
      throw new Error(`Vector dimensions must match: ${a.length} vs ${b.length}`);
    }

    let dotProduct = 0;
    let normA = 0;
    let normB = 0;

    for (let i = 0; i < a.length; i++) {
      dotProduct += a[i] * b[i];
      normA += a[i] * a[i];
      normB += b[i] * b[i];
    }

    const denominator = Math.sqrt(normA) * Math.sqrt(normB);
    return denominator === 0 ? 0 : dotProduct / denominator;
  },

  /**
   * Euclidean distance: measures straight-line distance (lower = more similar)
   */
  euclidean(a: VectorEmbedding, b: VectorEmbedding): number {
    if (a.length !== b.length) {
      throw new Error(`Vector dimensions must match: ${a.length} vs ${b.length}`);
    }

    let sum = 0;
    for (let i = 0; i < a.length; i++) {
      const diff = a[i] - b[i];
      sum += diff * diff;
    }

    return Math.sqrt(sum);
  },

  /**
   * Dot product: measures magnitude * alignment (higher = more similar)
   */
  dotProduct(a: VectorEmbedding, b: VectorEmbedding): number {
    if (a.length !== b.length) {
      throw new Error(`Vector dimensions must match: ${a.length} vs ${b.length}`);
    }

    let sum = 0;
    for (let i = 0; i < a.length; i++) {
      sum += a[i] * b[i];
    }

    return sum;
  }
};
