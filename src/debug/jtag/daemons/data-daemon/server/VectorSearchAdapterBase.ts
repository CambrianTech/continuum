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
 * - Vector search runs in Rust (80-860ms) - no TypeScript fallback
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
  toNumberArray
} from '../shared/VectorSearchTypes';
import { RustEmbeddingClient } from '../../../system/core/services/RustEmbeddingClient';
import { RustVectorSearchClient } from '../../../system/core/services/RustVectorSearchClient';
// NOTE: No SqlNamingConverter - Rust DataModule handles all naming conversions internally

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
    private readonly vectorOps: VectorStorageOperations,
    private readonly dbPath: string
  ) {
    if (!dbPath) {
      throw new Error('VectorSearchAdapterBase requires explicit dbPath - no fallbacks allowed');
    }
  }

  // ============================================================================
  // GENERIC IMPLEMENTATIONS - Work for all backends
  // ============================================================================

  /**
   * Perform vector similarity search
   *
   * RUST ONLY - No TypeScript fallback. Fails loudly if Rust worker unavailable.
   * Rust is ~100x faster (80-860ms vs 12-25 seconds for TypeScript).
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
      const embeddingStart = Date.now();
      if (options.queryText) {
        const embeddingResult = await this.generateEmbedding({
          text: options.queryText,
          model: options.embeddingModel
        });
        if (!embeddingResult.success || !embeddingResult.data) {
          return {
            success: false,
            error: embeddingResult.error || 'Failed to generate embedding for query text'
          };
        }
        queryVector = embeddingResult.data.embedding;
        console.debug(`🔍 VECTOR-SEARCH-TIMING: Embedding generated in ${Date.now() - embeddingStart}ms`);
      } else if (options.queryVector) {
        queryVector = options.queryVector;
      } else {
        return {
          success: false,
          error: 'Must provide either queryText or queryVector'
        };
      }

      // 2. Rust worker REQUIRED - fail fast if unavailable
      const rustAvailStart = Date.now();
      const rustClient = RustVectorSearchClient.instance;
      if (!await rustClient.isAvailable()) {
        return {
          success: false,
          error: 'Rust continuum-core not available. Start with: npm start'
        };
      }
      console.debug(`🔍 VECTOR-SEARCH-TIMING: Rust availability check in ${Date.now() - rustAvailStart}ms`);

      // 3. Execute vector search via Rust (no fallback)
      // Pass collection name directly - Rust DataModule handles naming conversions
      const queryArr = toNumberArray(queryVector);

      const rustSearchStart = Date.now();
      const rustResult = await rustClient.search(
        options.collection,
        queryArr,
        k,
        threshold,
        true,  // include_data - returns full records, avoids k IPC round trips
        this.dbPath  // Pass database path for per-persona databases
      );
      console.debug(`🔍 VECTOR-SEARCH-TIMING: Rust search in ${Date.now() - rustSearchStart}ms (corpus=${rustResult.corpus_size})`);
      console.debug(`🔍 VECTOR-SEARCH-TIMING: Total breakdown - embed=${Date.now() - embeddingStart}ms from start`);

      // 4. Convert Rust results to our format
      const results: VectorSearchResult<T>[] = rustResult.results.map(r => ({
        id: r.id as UUID,
        data: r.data as T,
        score: r.score,
        distance: 1 - r.score,
        metadata: {
          collection: options.collection,
          embeddingModel: options.embeddingModel?.name,
          queryTime: Date.now() - startTime
        }
      }));

      return {
        success: true,
        data: {
          results,
          totalResults: results.length,
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
      const availCheckStart = Date.now();
      if (!await rustClient.isAvailable()) {
        return {
          success: false,
          error: 'Rust embedding worker not available. Start with: ./workers/start-workers.sh'
        };
      }
      const availCheckTime = Date.now() - availCheckStart;

      const embedStart = Date.now();
      const embedding = await rustClient.embed(request.text);
      const embedTime = Date.now() - embedStart;

      console.debug(`🧬 EMBED-TIMING: availCheck=${availCheckTime}ms, embed=${embedTime}ms, total=${Date.now() - startTime}ms`);

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
