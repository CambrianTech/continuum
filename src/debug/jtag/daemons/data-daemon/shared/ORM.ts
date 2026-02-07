/**
 * ORM - Unified Data Access Layer
 *
 * Single entry point for ALL data operations. Routes to Rust ORM via IPC.
 *
 * ARCHITECTURE (from docs/RUST-ORM-ARCHITECTURE.md):
 * - Phase 1-4: ✅ COMPLETE - All CRUD operations route to Rust ORM
 * - Phase 5: IN PROGRESS - Cleanup dead TypeScript paths
 * - Phase 6: TODO - Remove DataDaemon once batch/vector/paginated ops moved to Rust
 *
 * CURRENT STATE:
 * - Core CRUD (store, query, read, update, remove, count, queryWithJoin) → Rust
 * - Batch operations → DataDaemon (TODO: move to Rust)
 * - Paginated queries → DataDaemon (TODO: move to Rust)
 * - Vector search → DataDaemon (TODO: move to Rust)
 * - Maintenance ops (clear, truncate) → DataDaemon
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

// DataDaemon still needed for: batch, paginated, vector, maintenance ops (Phase 6 TODO)
import { DataDaemon } from './DataDaemon';

// Import config and logging
import {
  FORCE_TYPESCRIPT_BACKEND,
  shouldUseRust,
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
   */
  static async store<T extends BaseEntity>(
    collection: CollectionName,
    data: T,
    _suppressEvents: boolean = false
  ): Promise<T> {
    const done = logOperationStart('store', collection, { id: (data as any).id });

    try {
      const client = await getRustClient();
      const result = await client.store<T>(collection, data);
      if (!result.success) {
        throw new Error(result.error || 'Rust store failed');
      }
      done();
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

    try {
      const client = await getRustClient();
      const result = await client.update<T>(collection, id, data, incrementVersion);
      done();
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
    _suppressEvents: boolean = false
  ): Promise<StorageResult<boolean>> {
    const done = logOperationStart('remove', collection, { id });

    try {
      const client = await getRustClient();
      const result = await client.remove(collection, id);
      done();
      return result;
    } catch (error) {
      logOperationError('remove', collection, error);
      throw error;
    }
  }

  // ─── Batch Operations ───────────────────────────────────────────────────────

  /**
   * Execute batch operations
   */
  static async batch(
    operations: StorageOperation[]
  ): Promise<StorageResult<any[]>> {
    const collections = [...new Set(operations.map(op => op.collection))];
    const done = logOperationStart('batch', collections.join(','), { count: operations.length });

    try {
      // Batch goes to TypeScript for now (mixed collections)
      const result = await DataDaemon.batch(operations);
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
   */
  static async listCollections(): Promise<StorageResult<string[]>> {
    return DataDaemon.listCollections();
  }

  // ─── Maintenance Operations ─────────────────────────────────────────────────

  /**
   * Clear all data from all collections
   */
  static async clear(): Promise<StorageResult<boolean>> {
    return DataDaemon.clear();
  }

  /**
   * Clear all data with detailed reporting
   */
  static async clearAll(): Promise<
    StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>
  > {
    return DataDaemon.clearAll();
  }

  /**
   * Truncate specific collection
   */
  static async truncate(collection: CollectionName): Promise<StorageResult<boolean>> {
    return DataDaemon.truncate(collection);
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
