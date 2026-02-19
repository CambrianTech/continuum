/**
 * ORM - Unified Data Access Layer
 *
 * Single entry point for ALL data operations. Routes to Rust DataModule.
 *
 * CURRENT STATE (2026-02-09):
 * ✅ ALL operations route to Rust DataModule via ORMRustClient:
 *    - store, query, count, queryWithJoin, read, update, remove
 *    - batch, listCollections, clear, clearAll, truncate
 *    - vectorSearch (embedding + similarity search)
 *
 * 📝 Only remaining TypeScript code:
 *    - Paginated queries (stateful, requires TypeScript cursor management)
 *    - Event emission context (DataDaemon.jtagContext for browser routing)
 *
 * ⚠️  NO FALLBACKS POLICY ⚠️
 * ALL operations go to Rust. If Rust fails, it FAILS LOUDLY.
 * There is NO "try Rust, catch, use TypeScript" pattern.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { BaseEntity } from '../../../system/data/entities/BaseEntity';
import type {
  DataRecord,
  StorageQuery,
  StorageQueryWithJoin,
  StorageResult,
  StorageOperation,
  RecordData,
} from '../shared/DataStorageAdapter';
import type { OpenPaginatedQueryParams, PaginatedQueryHandle, PaginatedQueryPage } from '../shared/PaginatedQuery';
import type {
  VectorSearchOptions,
  VectorSearchResponse,
  GenerateEmbeddingRequest,
  GenerateEmbeddingResponse,
  IndexVectorRequest,
  BackfillVectorsRequest,
  BackfillVectorsProgress,
  VectorIndexStats,
  VectorSearchCapabilities,
} from '../shared/VectorSearchTypes';

// Import DataDaemon for delegation (TypeScript backend)
import { DataDaemon } from '../shared/DataDaemon';

// Import Events for CRUD event emission
import { Events } from '../../../system/core/shared/Events';
import { getDataEventName } from '../../../system/core/shared/EventConstants';

// Import config and logging
import {
  FORCE_TYPESCRIPT_BACKEND,
  shouldUseRust,
  shouldShadow,
  getBackendStatus,
} from '../shared/ORMConfig';

// Import type-safe collection names
import type { CollectionName } from '../../../shared/generated-collection-constants';

// Lazy import for Rust client (avoid circular deps)
let _rustClient: import('./ORMRustClient').ORMRustClient | null = null;
async function getRustClient(): Promise<import('./ORMRustClient').ORMRustClient> {
  if (!_rustClient) {
    const { ORMRustClient } = await import('./ORMRustClient');
    _rustClient = ORMRustClient.getInstance();
  }
  return _rustClient;
}

// Lazy import for Rust embedding client
let _embeddingClient: import('../../../system/core/services/RustEmbeddingClient').RustEmbeddingClient | null = null;
async function getEmbeddingClient(): Promise<import('../../../system/core/services/RustEmbeddingClient').RustEmbeddingClient> {
  if (!_embeddingClient) {
    const { RustEmbeddingClient } = await import('../../../system/core/services/RustEmbeddingClient');
    _embeddingClient = RustEmbeddingClient.instance;
  }
  return _embeddingClient;
}
import {
  logOperationStart,
  logOperationError,
  getMetricsSummary,
  printMetricsSummary,
} from '../shared/ORMLogger';

/**
 * ORM - Universal Data Access Layer
 *
 * USAGE:
 * ```typescript
 * import { ORM } from '@daemons/data-daemon/server/ORM';
 *
 * // Store entity
 * const user = await ORM.store<UserEntity>('users', userData);
 *
 * // Query entities
 * const messages = await ORM.query<ChatMessageEntity>({
 *   collection: 'chatMessages',
 *   filter: { roomId: 'general' },
 *   limit: 50
 * });
 * ```
 */
export class ORM {
  // ─── CRUD Operations ────────────────────────────────────────────────────────

  /**
   * Store entity in collection
   * Emits data:{collection}:created event via DataDaemon's jtagContext for browser routing
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  static async store<T extends BaseEntity>(
    collection: CollectionName,
    data: T,
    suppressEvents: boolean = false,
    dbPath?: string
  ): Promise<T> {
    const done = logOperationStart('store', collection, { id: (data as any).id });

    try {
      const client = await getRustClient();
      const result = await client.store<T>(collection, data, dbPath);
      if (!result.success) {
        throw new Error(result.error || 'Rust store failed');
      }
      done();

      // Emit event using DataDaemon's jtagContext for proper browser routing
      if (!suppressEvents && DataDaemon.jtagContext) {
        const eventName = getDataEventName(collection, 'created');
        Events.emit(DataDaemon.jtagContext, eventName, result.data)
          .catch(err => console.error(`ORM.store event emit failed for ${collection}:`, err));
      }

      return result.data!;
    } catch (error) {
      logOperationError('store', collection, error);
      throw error;
    }
  }

  /**
   * Query entities from collection
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  static async query<T extends BaseEntity>(
    query: StorageQuery,
    dbPath?: string
  ): Promise<StorageResult<DataRecord<T>[]>> {
    const done = logOperationStart('query', query.collection, {
      filter: query.filter,
      limit: query.limit,
    });

    try {
      const client = await getRustClient();
      const result = await client.query<T>(query, dbPath);
      done();
      return result;
    } catch (error) {
      logOperationError('query', query.collection, error);
      throw error;
    }
  }

  /**
   * Count entities matching query (uses SQL COUNT, not fetch-all)
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  static async count(query: StorageQuery, dbPath?: string): Promise<StorageResult<number>> {
    const done = logOperationStart('count', query.collection, { filter: query.filter });

    // FORCED RUST PATH - no fallback
    try {
      const client = await getRustClient();
      const result = await client.count(query, dbPath);
      done();
      return result;
    } catch (error) {
      logOperationError('count', query.collection, error);
      throw error;
    }
  }

  /**
   * Query with JOINs for optimal loading
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  static async queryWithJoin<T extends RecordData>(
    query: StorageQueryWithJoin,
    dbPath?: string
  ): Promise<StorageResult<DataRecord<T>[]>> {
    const done = logOperationStart('query', query.collection, { joins: query.joins?.length });

    // FORCED RUST PATH - no fallback
    try {
      const client = await getRustClient();
      const result = await client.queryWithJoin<T & BaseEntity>(query, dbPath);
      done();
      return result;
    } catch (error) {
      logOperationError('query', query.collection, error);
      throw error;
    }
  }

  /**
   * Read single entity by ID
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  static async read<T extends BaseEntity>(
    collection: CollectionName,
    id: UUID,
    dbPath?: string
  ): Promise<T | null> {
    const done = logOperationStart('read', collection, { id });

    // FORCED RUST PATH - no fallback
    try {
      const client = await getRustClient();
      const result = await client.read<T>(collection, id, dbPath);
      done();
      return result;
    } catch (error) {
      logOperationError('read', collection, error);
      throw error;
    }
  }

  /**
   * Update entity
   * Emits data:{collection}:updated event with FULL entity (fetched after update)
   * @param incrementVersion - If true, increment version on update (default: true)
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   * @param suppressEvents - If true, skip event emission (useful for bulk updates like seeding)
   */
  static async update<T extends BaseEntity>(
    collection: CollectionName,
    id: UUID,
    data: Partial<T>,
    incrementVersion: boolean = true,
    dbPath?: string,
    suppressEvents: boolean = false
  ): Promise<T> {
    const done = logOperationStart('update', collection, { id, fields: Object.keys(data) });

    // FORCED RUST PATH - no fallback
    try {
      const client = await getRustClient();
      await client.update<T>(collection, id, data, incrementVersion, dbPath);

      // Fetch the FULL entity to return and emit (not just partial update data)
      const fullEntity = await client.read<T>(collection, id, dbPath);
      if (!fullEntity) {
        throw new Error(`Update succeeded but entity ${id} not found in ${collection}`);
      }

      done();

      // Emit event with FULL entity using DataDaemon's jtagContext for proper browser routing
      if (!suppressEvents && DataDaemon.jtagContext) {
        const eventName = getDataEventName(collection, 'updated');
        Events.emit(DataDaemon.jtagContext, eventName, fullEntity)
          .catch(err => console.error(`ORM.update event emit failed for ${collection}:`, err));
      }

      return fullEntity;
    } catch (error) {
      logOperationError('update', collection, error);
      throw error;
    }
  }

  /**
   * Remove entity
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  static async remove(
    collection: CollectionName,
    id: UUID,
    suppressEvents: boolean = false,
    dbPath?: string
  ): Promise<StorageResult<boolean>> {
    const done = logOperationStart('remove', collection, { id });

    // FORCED RUST PATH - no fallback
    try {
      const client = await getRustClient();
      const result = await client.remove(collection, id, dbPath);
      done();

      // Emit event using DataDaemon's jtagContext for proper browser routing
      if (!suppressEvents && result.success && DataDaemon.jtagContext) {
        const eventName = getDataEventName(collection, 'deleted');
        Events.emit(DataDaemon.jtagContext, eventName, { id, collection })
          .catch(err => console.error(`ORM.remove event emit failed for ${collection}:`, err));
      }

      return result;
    } catch (error) {
      logOperationError('remove', collection, error);
      throw error;
    }
  }

  // ─── Batch Operations ───────────────────────────────────────────────────────

  /**
   * Execute batch operations
   * FORCED RUST PATH - no fallback
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  static async batch(
    operations: StorageOperation[],
    dbPath?: string
  ): Promise<StorageResult<any[]>> {
    const collections = [...new Set(operations.map(op => op.collection))];
    const done = logOperationStart('batch', collections.join(','), { count: operations.length });

    try {
      const client = await getRustClient();
      const result = await client.batch(operations, dbPath);
      done();
      return result;
    } catch (error) {
      logOperationError('batch', collections.join(','), error);
      throw error;
    }
  }

  // ─── Schema Operations ──────────────────────────────────────────────────────

  /**
   * List all collections
   * FORCED RUST PATH - no fallback
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  static async listCollections(dbPath?: string): Promise<StorageResult<string[]>> {
    const done = logOperationStart('listCollections', '*', {});
    try {
      const client = await getRustClient();
      const result = await client.listCollections(dbPath);
      done();
      return result;
    } catch (error) {
      logOperationError('listCollections', '*', error);
      throw error;
    }
  }

  // ─── Maintenance Operations ─────────────────────────────────────────────────

  /**
   * Clear all data from all collections
   * FORCED RUST PATH - no fallback
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  static async clear(dbPath?: string): Promise<StorageResult<boolean>> {
    const done = logOperationStart('clear', '*', {});
    try {
      const client = await getRustClient();
      const result = await client.clearAll(dbPath);
      done();
      return { success: result.success, data: result.success };
    } catch (error) {
      logOperationError('clear', '*', error);
      throw error;
    }
  }

  /**
   * Clear all data with detailed reporting
   * FORCED RUST PATH - no fallback
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  static async clearAll(dbPath?: string): Promise<
    StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>
  > {
    const done = logOperationStart('clearAll', '*', {});
    try {
      const client = await getRustClient();
      const result = await client.clearAll(dbPath);
      done();
      return result;
    } catch (error) {
      logOperationError('clearAll', '*', error);
      throw error;
    }
  }

  /**
   * Truncate specific collection
   * FORCED RUST PATH - no fallback
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  static async truncate(collection: CollectionName, dbPath?: string): Promise<StorageResult<boolean>> {
    const done = logOperationStart('truncate', collection, {});
    try {
      const client = await getRustClient();
      const result = await client.truncate(collection, dbPath);
      done();
      return result;
    } catch (error) {
      logOperationError('truncate', collection, error);
      throw error;
    }
  }

  // ─── Paginated Queries ──────────────────────────────────────────────────────

  /**
   * Open paginated query
   *
   * ✅ NOW ROUTED TO RUST
   *
   * Server-side cursor management eliminates IPC overhead per page.
   * Rust DashMap provides lock-free concurrent query state.
   */
  static async openPaginatedQuery(
    params: OpenPaginatedQueryParams,
    dbPath?: string
  ): Promise<PaginatedQueryHandle> {
    const done = logOperationStart('query', params.collection, { pageSize: params.pageSize });

    try {
      const client = await getRustClient();
      const result = await client.openPaginatedQuery({
        collection: params.collection,
        filter: params.filter,
        orderBy: params.orderBy,
        pageSize: params.pageSize,
        dbPath,
      });

      done();

      if (!result.success || !result.data) {
        throw new Error(result.error || 'Failed to open paginated query');
      }

      return {
        queryId: result.data.queryId as UUID,
        collection: result.data.collection,
        totalCount: result.data.totalCount,
        pageSize: result.data.pageSize,
        hasMore: result.data.hasMore,
      };
    } catch (error) {
      logOperationError('query', params.collection, error);
      throw error;
    }
  }

  /**
   * Get next page from paginated query
   *
   * ✅ NOW ROUTED TO RUST
   */
  static async getNextPage<T extends BaseEntity>(
    queryId: UUID
  ): Promise<PaginatedQueryPage<T>> {
    const done = logOperationStart('query', '*', { queryId });

    try {
      const client = await getRustClient();
      const result = await client.getNextPage<T>(queryId);

      done();

      if (!result.success || !result.data) {
        throw new Error(result.error || 'Failed to get next page');
      }

      return {
        items: result.data.items,
        pageNumber: result.data.pageNumber,
        hasMore: result.data.hasMore,
        totalCount: result.data.totalCount,
      };
    } catch (error) {
      logOperationError('query', '*', error);
      throw error;
    }
  }

  /**
   * Close paginated query
   *
   * ✅ NOW ROUTED TO RUST
   */
  static async closePaginatedQuery(queryId: UUID): Promise<void> {
    try {
      const client = await getRustClient();
      await client.closePaginatedQuery(queryId);
    } catch (error) {
      console.error('Failed to close paginated query:', error);
    }
  }

  /**
   * Get active query handles (for debugging)
   *
   * Note: This still uses TypeScript for backward compatibility.
   * Rust query state is managed separately.
   */
  static getActiveQueries(): UUID[] {
    return DataDaemon.getActiveQueries();
  }

  // ─── Vector Search Operations ───────────────────────────────────────────────

  /**
   * Perform vector similarity search via Rust DataModule
   *
   * ✅ NOW ROUTED TO RUST (Phase 4e completion)
   *
   * Rust advantages:
   * - In-memory vector caching (no re-query on repeated searches)
   * - Rayon parallel cosine similarity (multi-threaded)
   * - SIMD-like loop unrolling for fast distance computation
   *
   * If queryText is provided (no queryVector), generates embedding via Rust EmbeddingModule first.
   */
  static async vectorSearch<T extends RecordData>(
    options: VectorSearchOptions
  ): Promise<StorageResult<VectorSearchResponse<T>>> {
    const done = logOperationStart('vectorSearch', options.collection, { k: options.k });

    try {
      const client = await getRustClient();

      // Get query vector - either provided or generate from text
      let queryVector: number[];

      if (options.queryVector) {
        // Use provided vector (convert Float32Array if needed)
        queryVector = Array.isArray(options.queryVector)
          ? options.queryVector
          : Array.from(options.queryVector);
      } else if (options.queryText) {
        // Generate embedding via Rust EmbeddingModule
        const embeddingResult = await ORM.generateEmbedding({ text: options.queryText });
        if (!embeddingResult.success || !embeddingResult.data) {
          done();
          return { success: false, error: embeddingResult.error || 'Failed to generate embedding' };
        }
        queryVector = Array.isArray(embeddingResult.data.embedding)
          ? embeddingResult.data.embedding
          : Array.from(embeddingResult.data.embedding);
      } else {
        done();
        return { success: false, error: 'vectorSearch requires queryText or queryVector' };
      }

      // Call Rust vector/search (dbPath resolved by caller)
      const result = await client.vectorSearch<T>(
        options.collection,
        queryVector,
        {
          k: options.k ?? 10,
          threshold: options.similarityThreshold ?? 0.0,
          includeData: true,
          dbPath: options.dbPath,
        }
      );

      done();

      if (!result.success || !result.data) {
        return { success: false, error: result.error };
      }

      // Wrap in VectorSearchResponse format for compatibility
      return {
        success: true,
        data: {
          results: result.data,
          totalResults: result.data.length,
          queryVector,
          metadata: {
            collection: options.collection,
            searchMode: 'semantic',
            embeddingModel: 'all-minilm',  // Rust EmbeddingModule default
            queryTime: 0,  // Rust logs this internally
          },
        },
      };
    } catch (error) {
      logOperationError('vectorSearch', options.collection, error);
      throw error;
    }
  }

  /**
   * Generate embedding for text via Rust EmbeddingModule
   *
   * Routes to continuum-core's fastembed (ONNX-based) for fast native embeddings.
   * ~5ms per embedding via native ONNX runtime.
   */
  static async generateEmbedding(
    request: GenerateEmbeddingRequest
  ): Promise<StorageResult<GenerateEmbeddingResponse>> {
    const done = logOperationStart('generateEmbedding', '*', { textLength: request.text.length });

    try {
      const client = await getEmbeddingClient();
      const startTime = Date.now();

      // Map model name if provided (TypeScript EmbeddingModel → Rust model name)
      const embedding = await client.embed(request.text);

      const generationTime = Date.now() - startTime;
      done();

      return {
        success: true,
        data: {
          embedding,
          model: request.model ?? {
            name: 'all-minilm',
            dimensions: embedding.length,
            provider: 'fastembed' as const,  // ONNX-based native embeddings via Rust
          },
          generationTime,
        },
      };
    } catch (error) {
      logOperationError('generateEmbedding', '*', error);
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  /**
   * Index vector for a record
   *
   * ✅ NOW ROUTED TO RUST
   *
   * Stores the embedding in the record's 'embedding' field via Rust DataModule.
   * Also invalidates the Rust vector cache for the collection.
   */
  static async indexVector(
    request: IndexVectorRequest
  ): Promise<StorageResult<boolean>> {
    const done = logOperationStart('indexVector', request.collection, { id: request.id });

    try {
      const client = await getRustClient();

      // Convert embedding to number[] if needed
      const embedding = Array.isArray(request.embedding)
        ? request.embedding
        : Array.from(request.embedding);

      const result = await client.indexVector(
        request.collection,
        request.id,
        embedding
      );

      done();
      return result;
    } catch (error) {
      logOperationError('indexVector', request.collection, error);
      throw error;
    }
  }

  /**
   * Backfill vectors for existing records
   *
   * ✅ NOW ROUTED TO RUST
   *
   * Uses batch embedding generation via EmbeddingModule for efficiency.
   * Note: Progress callback not supported in Rust implementation.
   */
  static async backfillVectors(
    request: BackfillVectorsRequest,
    _onProgress?: (progress: BackfillVectorsProgress) => void
  ): Promise<StorageResult<BackfillVectorsProgress>> {
    const done = logOperationStart('vectorSearch', request.collection, { batchSize: request.batchSize });

    try {
      const client = await getRustClient();
      const result = await client.backfillVectors({
        collection: request.collection,
        textField: request.textField,
        batchSize: request.batchSize,
        model: request.model?.name,
        filter: request.filter,
      });

      done();

      if (!result.success || !result.data) {
        return { success: false, error: result.error };
      }

      // Map Rust result to BackfillVectorsProgress format
      return {
        success: true,
        data: {
          total: result.data.total,
          processed: result.data.processed,
          failed: result.data.failed,
          elapsedTime: result.data.elapsedMs,
        },
      };
    } catch (error) {
      logOperationError('vectorSearch', request.collection, error);
      throw error;
    }
  }

  /**
   * Get vector index statistics
   *
   * ✅ NOW ROUTED TO RUST
   *
   * Returns stats about the vector index for a collection.
   */
  static async getVectorIndexStats(
    collection: CollectionName,
    dbPath?: string
  ): Promise<StorageResult<VectorIndexStats>> {
    const done = logOperationStart('vectorSearch', collection, {}); // Using vectorSearch op for stats

    try {
      const client = await getRustClient();
      const result = await client.getVectorIndexStats(collection, dbPath);

      done();

      if (!result.success || !result.data) {
        return { success: false, error: result.error };
      }

      // Map Rust stats to VectorIndexStats format
      return {
        success: true,
        data: {
          collection: result.data.collection,
          totalRecords: result.data.totalRecords,
          recordsWithVectors: result.data.recordsWithVectors,
          vectorDimensions: result.data.vectorDimensions,
        },
      };
    } catch (error) {
      logOperationError('vectorSearch', collection, error);
      throw error;
    }
  }

  /**
   * Get vector search capabilities
   */
  static async getVectorSearchCapabilities(): Promise<VectorSearchCapabilities | null> {
    return DataDaemon.getVectorSearchCapabilities();
  }

  // ─── Utility Methods ────────────────────────────────────────────────────────

  /**
   * Get description field for a collection
   */
  static getDescriptionFieldForCollection(collection: CollectionName): string | null {
    return DataDaemon.getDescriptionFieldForCollection(collection);
  }

  /**
   * Check if Rust ORM is enabled globally
   */
  static isRustEnabled(): boolean {
    return !FORCE_TYPESCRIPT_BACKEND;
  }

  /**
   * Check if Rust is enabled for a specific collection
   */
  static isRustEnabledFor(collection: CollectionName): boolean {
    return shouldUseRust(collection);
  }

  /**
   * Get backend status for all collections
   */
  static getBackendStatus(): Record<string, string> {
    return getBackendStatus();
  }

  /**
   * Get ORM metrics summary
   */
  static getMetrics(): Record<string, unknown> {
    return getMetricsSummary();
  }

  /**
   * Print metrics to console
   */
  static printMetrics(): void {
    printMetricsSummary();
  }
}
