/**
 * Vector Search Adapter Base - Generic Vector Search Logic
 *
 * Contains all backend-agnostic vector search implementation.
 * Backends (SQLite, PostgreSQL, MongoDB, JSON) only implement storage-specific operations.
 *
 * ARCHITECTURE:
 * - Uses composition with DataStorageAdapter (existing CRUD operations)
 * - Delegates vector storage to VectorStorageOperations interface
 * - Maximizes code reuse, minimizes backend-specific code
 *
 * BENEFITS:
 * - SQLite, PostgreSQL, JSON, MongoDB all share same search/embedding logic
 * - Backends only implement 4 methods (ensureVectorStorage, storeVector, getAllVectors, getVectorCount)
 * - No duplication of similarity metrics, backfill logic, or embedding generation
 *
 * USAGE:
 * ```typescript
 * // In SqliteStorageAdapter:
 * const vectorSearch = new VectorSearchAdapterBase(
 *   this,  // DataStorageAdapter for CRUD
 *   {      // VectorStorageOperations for vector-specific storage
 *     ensureVectorStorage: (coll, dims) => this.ensureVectorTable(coll, dims),
 *     storeVector: (coll, vec) => this.storeVectorInSQLite(coll, vec),
 *     getAllVectors: (coll) => this.getVectorsFromSQLite(coll),
 *     getVectorCount: (coll) => this.countVectorsInSQLite(coll)
 *   }
 * );
 * ```
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { DataStorageAdapter, RecordData, StorageResult } from '../shared/DataStorageAdapter';
import {
  type VectorSearchAdapter,
  type VectorSearchOptions,
  type VectorSearchResponse,
  type VectorSearchResult,
  type GenerateEmbeddingRequest,
  type GenerateEmbeddingResponse,
  type IndexVectorRequest,
  type BackfillVectorsRequest,
  type BackfillVectorsProgress,
  type VectorIndexStats,
  type VectorSearchCapabilities,
  type VectorEmbedding,
  DEFAULT_EMBEDDING_MODELS,
  toNumberArray,
  SimilarityMetrics
} from '../shared/VectorSearchTypes';
import { RustEmbeddingClient } from '../../../system/core/services/RustEmbeddingClient';

/**
 * Vector record stored in backend
 */
export interface StoredVector {
  readonly recordId: UUID;
  readonly embedding: VectorEmbedding;
  readonly model?: string;
  readonly generatedAt: string;
}

/**
 * Backend-specific vector storage operations
 */
export interface VectorStorageOperations {
  /**
   * Ensure vector table/collection exists
   * SQLite: CREATE TABLE, PostgreSQL: CREATE TABLE, MongoDB: create collection/index
   */
  ensureVectorStorage(collection: string, dimensions: number): Promise<void>;

  /**
   * Store vector for a record
   */
  storeVector(collection: string, vector: StoredVector): Promise<void>;

  /**
   * Retrieve all vectors from storage
   */
  getAllVectors(collection: string): Promise<StoredVector[]>;

  /**
   * Get vector count
   */
  getVectorCount(collection: string): Promise<number>;
}

/**
 * Generic vector search adapter using composition
 *
 * Uses existing DataStorageAdapter for record operations + backend-specific vector operations
 * This ensures we don't duplicate storage logic across SQL/JSON/MongoDB adapters
 */
export class VectorSearchAdapterBase implements VectorSearchAdapter {
  constructor(
    private readonly storageAdapter: DataStorageAdapter,
    private readonly vectorOps: VectorStorageOperations
  ) {}

  // ============================================================================
  // GENERIC IMPLEMENTATIONS - Work for all backends
  // ============================================================================

  /**
   * Perform vector similarity search
   *
   * Generic implementation using cosine similarity. Works for all backends.
   */
  async vectorSearch<T extends RecordData>(
    options: VectorSearchOptions
  ): Promise<StorageResult<VectorSearchResponse<T>>> {
    const startTime = Date.now();

    try {
      const k = options.k || 10;
      const threshold = options.similarityThreshold || 0.0;
      const hybridMode = options.hybridMode || 'semantic';

      // 1. Generate query vector if text provided
      let queryVector: VectorEmbedding;
      if (options.queryText) {
        const embeddingResult = await this.generateEmbedding({
          text: options.queryText,
          model: options.embeddingModel
        });
        if (!embeddingResult.success || !embeddingResult.data) {
          return {
            success: false,
            error: 'Failed to generate embedding for query text'
          };
        }
        queryVector = embeddingResult.data.embedding;
      } else if (options.queryVector) {
        queryVector = options.queryVector;
      } else {
        return {
          success: false,
          error: 'Must provide either queryText or queryVector'
        };
      }

      // 2. Fetch all vectors from storage (delegates to backend-specific implementation)
      const vectors = await this.vectorOps.getAllVectors(options.collection);

      if (vectors.length === 0) {
        return {
          success: true,
          data: {
            results: [],
            totalResults: 0,
            queryVector,
            metadata: {
              collection: options.collection,
              searchMode: hybridMode,
              embeddingModel: options.embeddingModel?.name || 'unknown',
              queryTime: Date.now() - startTime
            }
          }
        };
      }

      // 3. Compute cosine similarity in-process (faster than IPC to Rust for typical workloads)
      // V8 is highly optimized and JSON serialization overhead dominates for IPC
      const queryArr = toNumberArray(queryVector);

      const scored: Array<{ idx: number; score: number }> = [];
      for (let i = 0; i < vectors.length; i++) {
        const corpusArr = toNumberArray(vectors[i].embedding);
        const score = SimilarityMetrics.cosine(queryArr, corpusArr);
        if (score >= threshold) {
          scored.push({ idx: i, score });
        }
      }

      // 4. Sort by score descending and take top-k
      scored.sort((a, b) => b.score - a.score);
      const topK: Array<{ id: UUID; score: number; distance: number }> = [];
      for (let i = 0; i < Math.min(k, scored.length); i++) {
        const { idx, score } = scored[i];
        topK.push({
          id: vectors[idx].recordId,
          score,
          distance: 1 - score
        });
      }

      // 5. Fetch actual records (uses existing storage adapter!)
      const results: VectorSearchResult<T>[] = [];

      for (const sim of topK) {
        const recordResult = await this.storageAdapter.read<T>(options.collection, sim.id);
        if (recordResult.success && recordResult.data) {
          results.push({
            id: sim.id,
            data: recordResult.data.data,
            score: sim.score,
            distance: sim.distance,
            metadata: {
              collection: options.collection,
              embeddingModel: options.embeddingModel?.name,
              queryTime: Date.now() - startTime
            }
          });
        }
      }

      // 6. Apply metadata filters if provided (hybrid search)
      let filteredResults = results;
      if (options.filter && hybridMode !== 'semantic') {
        // TODO: Implement filter application on results
        // For now, return all results
      }

      return {
        success: true,
        data: {
          results: filteredResults,
          totalResults: filteredResults.length,
          queryVector,
          metadata: {
            collection: options.collection,
            searchMode: hybridMode,
            embeddingModel: options.embeddingModel?.name || DEFAULT_EMBEDDING_MODELS['all-minilm'].name,
            queryTime: Date.now() - startTime
          }
        }
      };
    } catch (error) {
      return {
        success: false,
        error: `Vector search failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Generate embedding for text using Rust worker (fastembed ONNX)
   *
   * ~5ms per embedding, ~1ms in batch mode. No HTTP overhead.
   * Fails loudly if Rust worker unavailable - no silent fallbacks.
   */
  async generateEmbedding(
    request: GenerateEmbeddingRequest
  ): Promise<StorageResult<GenerateEmbeddingResponse>> {
    const startTime = Date.now();

    try {
      const model = request.model || DEFAULT_EMBEDDING_MODELS['all-minilm'];
      const rustClient = RustEmbeddingClient.instance;

      // Check availability - fail fast if worker not running
      if (!await rustClient.isAvailable()) {
        return {
          success: false,
          error: 'Rust embedding worker not available. Start with: ./workers/start-workers.sh'
        };
      }

      const embedding = await rustClient.embed(request.text);

      return {
        success: true,
        data: {
          embedding,
          model,
          tokenCount: undefined, // Rust worker doesn't report token count
          generationTime: Date.now() - startTime
        }
      };
    } catch (error) {
      return {
        success: false,
        error: `Embedding generation failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Index vector for a record
   *
   * Generic implementation - delegates to vectorOps
   */
  async indexVector(
    request: IndexVectorRequest
  ): Promise<StorageResult<boolean>> {
    try {
      // Ensure vector storage exists (delegates to backend)
      await this.vectorOps.ensureVectorStorage(request.collection, request.embedding.length);

      // Store vector (delegates to backend)
      await this.vectorOps.storeVector(request.collection, {
        recordId: request.id,
        embedding: request.embedding,
        model: request.metadata?.embeddingModel,
        generatedAt: request.metadata?.generatedAt || new Date().toISOString()
      });

      return {
        success: true,
        data: true
      };
    } catch (error) {
      return {
        success: false,
        error: `Vector indexing failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Backfill vectors for existing records
   *
   * Generic implementation - works for all backends
   */
  async backfillVectors(
    request: BackfillVectorsRequest,
    onProgress?: (progress: BackfillVectorsProgress) => void
  ): Promise<StorageResult<BackfillVectorsProgress>> {
    const startTime = Date.now();

    try {
      const batchSize = request.batchSize || 100;
      const model = request.model || DEFAULT_EMBEDDING_MODELS['all-minilm'];

      // Get all records from collection (uses existing storage adapter!)
      const queryResult = await this.storageAdapter.query<RecordData>({
        collection: request.collection,
        filter: request.filter
      });

      if (!queryResult.success || !queryResult.data) {
        return {
          success: false,
          error: 'Failed to query records for backfill'
        };
      }

      const records = queryResult.data;
      const progress: BackfillVectorsProgress = {
        total: records.length,
        processed: 0,
        failed: 0,
        elapsedTime: 0
      };

      // Process in batches
      for (let i = 0; i < records.length; i += batchSize) {
        const batch = records.slice(i, Math.min(i + batchSize, records.length));

        for (const record of batch) {
          try {
            // Extract text from specified field (record.data contains the actual data)
            const text = record.data[request.textField];
            if (typeof text !== 'string') {
              progress.failed++;
              continue;
            }

            // Generate embedding
            const embeddingResult = await this.generateEmbedding({
              text,
              model
            });

            if (!embeddingResult.success || !embeddingResult.data) {
              progress.failed++;
              continue;
            }

            // Index vector
            await this.indexVector({
              collection: request.collection,
              id: record.id,
              embedding: embeddingResult.data.embedding,
              metadata: {
                embeddingModel: model.name,
                generatedAt: new Date().toISOString()
              }
            });

            progress.processed++;
          } catch (error) {
            progress.failed++;
          }
        }

        // Report progress
        progress.elapsedTime = Date.now() - startTime;
        progress.estimatedRemaining = progress.processed > 0
          ? ((progress.total - progress.processed) / progress.processed) * progress.elapsedTime
          : undefined;

        if (onProgress) {
          onProgress(progress);
        }
      }

      return {
        success: true,
        data: progress
      };
    } catch (error) {
      return {
        success: false,
        error: `Backfill failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Get vector index statistics
   *
   * Generic implementation - uses storageAdapter and vectorOps
   */
  async getVectorIndexStats(
    collection: string
  ): Promise<StorageResult<VectorIndexStats>> {
    try {
      // Get total records (uses existing storage adapter!)
      const statsResult = await this.storageAdapter.getCollectionStats(collection);
      const totalRecords = statsResult.success && statsResult.data ? statsResult.data.recordCount : 0;

      // Get records with vectors (delegates to backend)
      const recordsWithVectors = await this.vectorOps.getVectorCount(collection);

      // Get vector dimensions from first vector (delegates to backend)
      let vectorDimensions = 0;
      if (recordsWithVectors > 0) {
        const vectors = await this.vectorOps.getAllVectors(collection);
        if (vectors.length > 0) {
          vectorDimensions = vectors[0].embedding.length;
        }
      }

      return {
        success: true,
        data: {
          collection,
          totalRecords,
          recordsWithVectors,
          vectorDimensions
        }
      };
    } catch (error) {
      return {
        success: false,
        error: `Failed to get vector index stats: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Get vector search capabilities
   *
   * Generic implementation
   */
  async getVectorSearchCapabilities(): Promise<VectorSearchCapabilities> {
    return {
      supportsVectorSearch: true,
      supportsHybridSearch: true,
      supportsEmbeddingGeneration: true,
      maxVectorDimensions: 2048,
      supportedSimilarityMetrics: ['cosine', 'euclidean', 'dot-product'],
      embeddingProviders: ['ollama']
    };
  }
}
