/**
 * ORM - Unified Data Access Layer
 *
 * Single entry point for ALL data operations. Replaces scattered DataDaemon.* calls.
 *
 * ARCHITECTURE (from docs/RUST-ORM-ARCHITECTURE.md):
 * - Phase 0: Migrate all DataDaemon.* calls to ORM.* (this phase)
 * - Phase 1: Schema validation between TS and Rust
 * - Phase 2: Shadow mode (run both, compare)
 * - Phase 3-5: Incremental cutover
 * - Phase 6: Cleanup
 *
 * ⚠️  NO FALLBACKS POLICY ⚠️
 * This code has ZERO fallback logic. If Rust is configured and fails, it FAILS LOUDLY.
 * There is NO "try Rust, catch, use TypeScript" pattern anywhere.
 * Backend selection is EXPLICIT and DETERMINISTIC based on config flags.
 * If you see fallback logic, DELETE IT IMMEDIATELY.
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

// Import DataDaemon for delegation
import { DataDaemon } from './DataDaemon';

// Import config and logging
import {
  FORCE_TYPESCRIPT_BACKEND,
  shouldUseRust,
  shouldShadow,
  getBackendStatus,
} from './ORMConfig';
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
    collection: string,
    data: T,
    suppressEvents: boolean = false
  ): Promise<T> {
    const done = logOperationStart('store', collection, { id: (data as any).id });

    try {
      if (shouldUseRust(collection)) {
        // TODO: Phase 4 - IPC to Rust ConnectionManager
        throw new Error('Rust ORM not implemented yet');
      }

      const result = await DataDaemon.store<T>(collection, data, suppressEvents);
      done();
      return result;
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
      if (shouldUseRust(query.collection)) {
        throw new Error('Rust ORM not implemented yet');
      }

      const result = await DataDaemon.query<T>(query);
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
      if (shouldUseRust(query.collection)) {
        throw new Error('Rust ORM not implemented yet');
      }

      const result = await DataDaemon.count(query);
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
      if (shouldUseRust(query.collection)) {
        throw new Error('Rust ORM not implemented yet');
      }

      const result = await DataDaemon.queryWithJoin<T>(query);
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
    collection: string,
    id: UUID
  ): Promise<T | null> {
    const done = logOperationStart('read', collection, { id });

    try {
      if (shouldUseRust(collection)) {
        throw new Error('Rust ORM not implemented yet');
      }

      const result = await DataDaemon.read<T>(collection, id);
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
    collection: string,
    id: UUID,
    data: Partial<T>,
    incrementVersion: boolean = true
  ): Promise<T> {
    const done = logOperationStart('update', collection, { id, fields: Object.keys(data) });

    try {
      if (shouldUseRust(collection)) {
        throw new Error('Rust ORM not implemented yet');
      }

      const result = await DataDaemon.update<T>(collection, id, data, incrementVersion);
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
    collection: string,
    id: UUID,
    suppressEvents: boolean = false
  ): Promise<StorageResult<boolean>> {
    const done = logOperationStart('remove', collection, { id });

    try {
      if (shouldUseRust(collection)) {
        throw new Error('Rust ORM not implemented yet');
      }

      const result = await DataDaemon.remove(collection, id, suppressEvents);
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
  static async truncate(collection: string): Promise<StorageResult<boolean>> {
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
   */
  static async vectorSearch<T extends RecordData>(
    options: VectorSearchOptions
  ): Promise<StorageResult<VectorSearchResponse<T>>> {
    const done = logOperationStart('vectorSearch', options.collection, { k: options.k });

    try {
      if (shouldUseRust(options.collection)) {
        throw new Error('Rust ORM not implemented yet');
      }

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
    collection: string
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
  static getDescriptionFieldForCollection(collection: string): string | null {
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
  static isRustEnabledFor(collection: string): boolean {
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
