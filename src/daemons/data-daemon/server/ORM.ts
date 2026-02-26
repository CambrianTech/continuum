/**
 * ORM - Unified Data Access Layer
 *
 * Single entry point for ALL data operations. Routes to Rust DataModule.
 *
 * HANDLE-BASED API:
 * Every operation requires a DbHandle. No implicit defaults.
 * - 'default' handle → main database
 * - UUID handles → per-persona or per-module databases (from data/open)
 * Handle → path resolution happens HERE, in ONE place.
 *
 * ⚠️  NO FALLBACKS POLICY ⚠️
 * ALL operations go to Rust. If Rust fails, it FAILS LOUDLY.
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

// Import handle types
import type { DbHandle } from './DatabaseHandleRegistry';
import { DatabaseHandleRegistry } from './DatabaseHandleRegistry';

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
 * // Main database (explicit 'default' handle)
 * const user = await ORM.store<UserEntity>('users', userData, false, 'default');
 *
 * // Per-persona database (handle from data/open)
 * const memories = await ORM.query<MemoryEntity>({ collection: 'memories' }, personaDbHandle);
 * ```
 */
export class ORM {

  /**
   * Resolve DbHandle → database file path.
   * SINGLE SOURCE OF TRUTH for handle resolution.
   * Throws if handle is invalid or unresolvable.
   */
  private static resolveHandle(handle: DbHandle): string {
    const registry = DatabaseHandleRegistry.getInstance();
    const dbPath = registry.getDbPath(handle);
    if (!dbPath) {
      throw new Error(`ORM: Invalid database handle '${handle}' — not found in DatabaseHandleRegistry. Did you call data/open first?`);
    }
    return dbPath;
  }

  // ─── CRUD Operations ────────────────────────────────────────────────────────

  /**
   * Store entity in collection
   * Emits data:{collection}:created event via DataDaemon's jtagContext for browser routing
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async store<T extends BaseEntity>(
    collection: CollectionName,
    data: T,
    suppressEvents: boolean = false,
    handle: DbHandle
  ): Promise<T> {
    const dbPath = ORM.resolveHandle(handle);
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
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async query<T extends BaseEntity>(
    query: StorageQuery,
    handle: DbHandle
  ): Promise<StorageResult<DataRecord<T>[]>> {
    const dbPath = ORM.resolveHandle(handle);
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
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async count(query: StorageQuery, handle: DbHandle): Promise<StorageResult<number>> {
    const dbPath = ORM.resolveHandle(handle);
    const done = logOperationStart('count', query.collection, { filter: query.filter });

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
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async queryWithJoin<T extends RecordData>(
    query: StorageQueryWithJoin,
    handle: DbHandle
  ): Promise<StorageResult<DataRecord<T>[]>> {
    const dbPath = ORM.resolveHandle(handle);
    const done = logOperationStart('query', query.collection, { joins: query.joins?.length });

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
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async read<T extends BaseEntity>(
    collection: CollectionName,
    id: UUID,
    handle: DbHandle
  ): Promise<T | null> {
    const dbPath = ORM.resolveHandle(handle);
    const done = logOperationStart('read', collection, { id });

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
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async update<T extends BaseEntity>(
    collection: CollectionName,
    id: UUID,
    data: Partial<T>,
    incrementVersion: boolean = true,
    handle: DbHandle,
    suppressEvents: boolean = false
  ): Promise<T> {
    const dbPath = ORM.resolveHandle(handle);
    const done = logOperationStart('update', collection, { id, fields: Object.keys(data) });

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
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async remove(
    collection: CollectionName,
    id: UUID,
    suppressEvents: boolean = false,
    handle: DbHandle
  ): Promise<StorageResult<boolean>> {
    const dbPath = ORM.resolveHandle(handle);
    const done = logOperationStart('remove', collection, { id });

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
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async batch(
    operations: StorageOperation[],
    handle: DbHandle
  ): Promise<StorageResult<any[]>> {
    const dbPath = ORM.resolveHandle(handle);
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
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async listCollections(handle: DbHandle): Promise<StorageResult<string[]>> {
    const dbPath = ORM.resolveHandle(handle);
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
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async clear(handle: DbHandle): Promise<StorageResult<boolean>> {
    const dbPath = ORM.resolveHandle(handle);
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
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async clearAll(handle: DbHandle): Promise<
    StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>
  > {
    const dbPath = ORM.resolveHandle(handle);
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
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async truncate(collection: CollectionName, handle: DbHandle): Promise<StorageResult<boolean>> {
    const dbPath = ORM.resolveHandle(handle);
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
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async openPaginatedQuery(
    params: OpenPaginatedQueryParams,
    handle: DbHandle
  ): Promise<PaginatedQueryHandle> {
    const dbPath = ORM.resolveHandle(handle);
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
   */
  static getActiveQueries(): UUID[] {
    return DataDaemon.getActiveQueries();
  }

  // ─── Vector Search Operations ───────────────────────────────────────────────

  /**
   * Perform vector similarity search via Rust DataModule
   *
   * Uses options.dbHandle for database routing (resolved via DatabaseHandleRegistry).
   * dbHandle is REQUIRED — no fallbacks.
   */
  static async vectorSearch<T extends RecordData>(
    options: VectorSearchOptions & { dbHandle: DbHandle }
  ): Promise<StorageResult<VectorSearchResponse<T>>> {
    const dbPath = ORM.resolveHandle(options.dbHandle);
    const done = logOperationStart('vectorSearch', options.collection, { k: options.k });

    try {
      const client = await getRustClient();

      // Get query vector - either provided or generate from text
      let queryVector: number[];

      if (options.queryVector) {
        queryVector = Array.isArray(options.queryVector)
          ? options.queryVector
          : Array.from(options.queryVector);
      } else if (options.queryText) {
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

      const result = await client.vectorSearch<T>(
        options.collection,
        queryVector,
        {
          k: options.k ?? 10,
          threshold: options.similarityThreshold ?? 0.0,
          includeData: true,
          dbPath,
        }
      );

      done();

      if (!result.success || !result.data) {
        return { success: false, error: result.error };
      }

      return {
        success: true,
        data: {
          results: result.data,
          totalResults: result.data.length,
          queryVector,
          metadata: {
            collection: options.collection,
            searchMode: 'semantic',
            embeddingModel: 'all-minilm',
            queryTime: 0,
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
   */
  static async generateEmbedding(
    request: GenerateEmbeddingRequest
  ): Promise<StorageResult<GenerateEmbeddingResponse>> {
    const done = logOperationStart('generateEmbedding', '*', { textLength: request.text.length });

    try {
      const client = await getEmbeddingClient();
      const startTime = Date.now();

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
            provider: 'fastembed' as const,
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
   */
  static async indexVector(
    request: IndexVectorRequest
  ): Promise<StorageResult<boolean>> {
    const done = logOperationStart('indexVector', request.collection, { id: request.id });

    try {
      const client = await getRustClient();

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
   * @param handle - Database handle (REQUIRED). Use 'default' for main DB.
   */
  static async getVectorIndexStats(
    collection: CollectionName,
    handle: DbHandle
  ): Promise<StorageResult<VectorIndexStats>> {
    const dbPath = ORM.resolveHandle(handle);
    const done = logOperationStart('vectorSearch', collection, {});

    try {
      const client = await getRustClient();
      const result = await client.getVectorIndexStats(collection, dbPath);

      done();

      if (!result.success || !result.data) {
        return { success: false, error: result.error };
      }

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
