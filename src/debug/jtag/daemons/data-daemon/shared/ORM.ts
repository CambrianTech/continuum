/**
 * ORM - Unified Data Access Layer
 *
 * Single entry point for ALL data operations. Replaces scattered DataDaemon.* calls.
 *
 * CURRENT STATE (2026-02-07):
 * ✅ Core CRUD operations are FORCED to Rust (no fallback):
 *    - store, query, count, queryWithJoin, read, update, remove
 * 📝 Specialized operations remain on TypeScript DataDaemon:
 *    - batch (multi-collection)
 *    - listCollections, clear, clearAll, truncate (maintenance)
 *    - paginated queries (stateful)
 *    - vector search (not yet in Rust DataModule)
 *
 * ⚠️  NO FALLBACKS POLICY ⚠️
 * Core CRUD has ZERO fallback logic. If Rust fails, it FAILS LOUDLY.
 * There is NO "try Rust, catch, use TypeScript" pattern for CRUD.
 * If you see fallback logic in CRUD methods, DELETE IT IMMEDIATELY.
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
} from './DataStorageAdapter';
import type { OpenPaginatedQueryParams, PaginatedQueryHandle, PaginatedQueryPage } from './PaginatedQuery';
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
} from './VectorSearchTypes';

// Import DataDaemon for delegation (TypeScript backend)
import { DataDaemon } from './DataDaemon';

// Import Events for CRUD event emission
import { Events } from '../../../system/core/shared/Events';
import { getDataEventName } from '../../../system/core/shared/EventConstants';

// Import config and logging
import {
  FORCE_TYPESCRIPT_BACKEND,
  shouldUseRust,
  shouldShadow,
  getBackendStatus,
} from './ORMConfig';

// Import type-safe collection names
import type { CollectionName } from '../../../shared/generated-collection-constants';

// Lazy import for Rust client (server-only, avoid circular deps)
let _rustClient: import('../server/ORMRustClient').ORMRustClient | null = null;
async function getRustClient(): Promise<import('../server/ORMRustClient').ORMRustClient> {
  if (!_rustClient) {
    const { ORMRustClient } = await import('../server/ORMRustClient');
    _rustClient = ORMRustClient.getInstance();
  }
  return _rustClient;
}
import {
  logOperationStart,
  logOperationError,
  getMetricsSummary,
  printMetricsSummary,
} from './ORMLogger';

/**
 * ORM - Universal Data Access Layer
 *
 * USAGE:
 * ```typescript
 * import { ORM } from '@daemons/data-daemon/shared/ORM';
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
   */
  static async store<T extends BaseEntity>(
    collection: CollectionName,
    data: T,
    suppressEvents: boolean = false
  ): Promise<T> {
    const done = logOperationStart('store', collection, { id: (data as any).id });

    try {
      const client = await getRustClient();
      const result = await client.store<T>(collection, data);
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
   */
  static async query<T extends BaseEntity>(
    query: StorageQuery
  ): Promise<StorageResult<DataRecord<T>[]>> {
    const done = logOperationStart('query', query.collection, {
      filter: query.filter,
      limit: query.limit,
    });

    try {
      const client = await getRustClient();
      const result = await client.query<T>(query);
      done();
      return result;
    } catch (error) {
      logOperationError('query', query.collection, error);
      throw error;
    }
  }

  /**
   * Count entities matching query (uses SQL COUNT, not fetch-all)
   */
  static async count(query: StorageQuery): Promise<StorageResult<number>> {
    const done = logOperationStart('count', query.collection, { filter: query.filter });

    // FORCED RUST PATH - no fallback
    try {
      const client = await getRustClient();
      const result = await client.count(query);
      done();
      return result;
    } catch (error) {
      logOperationError('count', query.collection, error);
      throw error;
    }
  }

  /**
   * Query with JOINs for optimal loading
   */
  static async queryWithJoin<T extends RecordData>(
    query: StorageQueryWithJoin
  ): Promise<StorageResult<DataRecord<T>[]>> {
    const done = logOperationStart('query', query.collection, { joins: query.joins?.length });

    // FORCED RUST PATH - no fallback
    try {
      const client = await getRustClient();
      const result = await client.queryWithJoin<T & BaseEntity>(query);
      done();
      return result;
    } catch (error) {
      logOperationError('query', query.collection, error);
      throw error;
    }
  }

  /**
   * Read single entity by ID
   */
  static async read<T extends BaseEntity>(
    collection: CollectionName,
    id: UUID
  ): Promise<T | null> {
    const done = logOperationStart('read', collection, { id });

    // FORCED RUST PATH - no fallback
    try {
      const client = await getRustClient();
      const result = await client.read<T>(collection, id);
      done();
      return result;
    } catch (error) {
      logOperationError('read', collection, error);
      throw error;
    }
  }

  /**
   * Update entity
   */
  static async update<T extends BaseEntity>(
    collection: CollectionName,
    id: UUID,
    data: Partial<T>,
    incrementVersion: boolean = true
  ): Promise<T> {
    const done = logOperationStart('update', collection, { id, fields: Object.keys(data) });

    // FORCED RUST PATH - no fallback
    try {
      const client = await getRustClient();
      const result = await client.update<T>(collection, id, data, incrementVersion);
      done();

      // Emit event using DataDaemon's jtagContext for proper browser routing
      if (DataDaemon.jtagContext) {
        const eventName = getDataEventName(collection, 'updated');
        Events.emit(DataDaemon.jtagContext, eventName, result)
          .catch(err => console.error(`ORM.update event emit failed for ${collection}:`, err));
      }

      return result;
    } catch (error) {
      logOperationError('update', collection, error);
      throw error;
    }
  }

  /**
   * Remove entity
   */
  static async remove(
    collection: CollectionName,
    id: UUID,
    suppressEvents: boolean = false
  ): Promise<StorageResult<boolean>> {
    const done = logOperationStart('remove', collection, { id });

    // FORCED RUST PATH - no fallback
    try {
      const client = await getRustClient();
      const result = await client.remove(collection, id);
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
   */
  static async batch(
    operations: StorageOperation[]
  ): Promise<StorageResult<any[]>> {
    const collections = [...new Set(operations.map(op => op.collection))];
    const done = logOperationStart('batch', collections.join(','), { count: operations.length });

    try {
      const client = await getRustClient();
      const result = await client.batch(operations);
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
   */
  static async listCollections(): Promise<StorageResult<string[]>> {
    const done = logOperationStart('listCollections', '*', {});
    try {
      const client = await getRustClient();
      const result = await client.listCollections();
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
   */
  static async clear(): Promise<StorageResult<boolean>> {
    const done = logOperationStart('clear', '*', {});
    try {
      const client = await getRustClient();
      const result = await client.clearAll();
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
   */
  static async clearAll(): Promise<
    StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>
  > {
    const done = logOperationStart('clearAll', '*', {});
    try {
      const client = await getRustClient();
      const result = await client.clearAll();
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
   */
  static async truncate(collection: CollectionName): Promise<StorageResult<boolean>> {
    const done = logOperationStart('truncate', collection, {});
    try {
      const client = await getRustClient();
      const result = await client.truncate(collection);
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
   */
  static async openPaginatedQuery(
    params: OpenPaginatedQueryParams
  ): Promise<PaginatedQueryHandle> {
    return DataDaemon.openPaginatedQuery(params);
  }

  /**
   * Get next page from paginated query
   */
  static async getNextPage<T extends BaseEntity>(
    queryId: UUID
  ): Promise<PaginatedQueryPage<T>> {
    return DataDaemon.getNextPage<T>(queryId);
  }

  /**
   * Close paginated query
   */
  static closePaginatedQuery(queryId: UUID): void {
    DataDaemon.closePaginatedQuery(queryId);
  }

  /**
   * Get active query handles (for debugging)
   */
  static getActiveQueries(): UUID[] {
    return DataDaemon.getActiveQueries();
  }

  // ─── Vector Search Operations ───────────────────────────────────────────────

  /**
   * Perform vector similarity search
   * NOTE: Vector search stays in TypeScript for now - not yet in Rust DataModule
   */
  static async vectorSearch<T extends RecordData>(
    options: VectorSearchOptions
  ): Promise<StorageResult<VectorSearchResponse<T>>> {
    const done = logOperationStart('vectorSearch', options.collection, { k: options.k });

    try {
      // Vector search not yet in Rust DataModule - use TypeScript
      const result = await DataDaemon.vectorSearch<T>(options);
      done();
      return result;
    } catch (error) {
      logOperationError('vectorSearch', options.collection, error);
      throw error;
    }
  }

  /**
   * Generate embedding for text
   */
  static async generateEmbedding(
    request: GenerateEmbeddingRequest
  ): Promise<StorageResult<GenerateEmbeddingResponse>> {
    return DataDaemon.generateEmbedding(request);
  }

  /**
   * Index vector for a record
   */
  static async indexVector(
    request: IndexVectorRequest
  ): Promise<StorageResult<boolean>> {
    return DataDaemon.indexVector(request);
  }

  /**
   * Backfill vectors for existing records
   */
  static async backfillVectors(
    request: BackfillVectorsRequest,
    onProgress?: (progress: BackfillVectorsProgress) => void
  ): Promise<StorageResult<BackfillVectorsProgress>> {
    return DataDaemon.backfillVectors(request, onProgress);
  }

  /**
   * Get vector index statistics
   */
  static async getVectorIndexStats(
    collection: CollectionName
  ): Promise<StorageResult<VectorIndexStats>> {
    return DataDaemon.getVectorIndexStats(collection);
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
