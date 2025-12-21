/**
 * DataWorkerClient - Type-Safe Client for Data Rust Worker
 *
 * This provides a production-ready interface for communicating with the
 * Rust data worker. It extends the generic WorkerClient with data-specific
 * methods and types.
 *
 * ARCHITECTURE:
 * - TypeScript: Orchestration, validation, decorators, events (unchanged)
 * - Rust Worker: I/O, connection pooling, storage detection, concurrency
 * - Communication: Unix domain socket (low overhead, high throughput)
 *
 * USAGE:
 * ```typescript
 * const client = new DataWorkerClient('/tmp/jtag-data-worker.sock');
 * await client.connect();
 *
 * // Open database handle
 * const { handle, storageType, pragmaMode } = await client.openDatabase({
 *   filename: '/Users/joel/.continuum/data/database.sqlite',
 *   adapterType: 'sqlite',
 *   storageType: 'auto-detect'
 * });
 *
 * // Create record
 * const { record } = await client.createRecord({
 *   handle,
 *   collection: 'users',
 *   record: { id, data, metadata }
 * });
 *
 * // Query records
 * const { records } = await client.queryRecords({
 *   handle,
 *   query: { collection: 'users', filter: { ... } }
 * });
 * ```
 */

import { WorkerClient, WorkerClientConfig } from '../WorkerClient.js';
import type {
  OpenDatabaseRequest,
  OpenDatabaseResponse,
  CloseDatabaseRequest,
  CloseDatabaseResponse,
  CreateRecordRequest,
  CreateRecordResponse,
  ReadRecordRequest,
  ReadRecordResponse,
  QueryRecordsRequest,
  QueryRecordsResponse,
  CountRecordsRequest,
  CountRecordsResponse,
  UpdateRecordRequest,
  UpdateRecordResponse,
  DeleteRecordRequest,
  DeleteRecordResponse,
  EnsureSchemaRequest,
  EnsureSchemaResponse,
  ListCollectionsRequest,
  ListCollectionsResponse,
  GetCollectionStatsRequest,
  GetCollectionStatsResponse,
  TruncateCollectionRequest,
  TruncateCollectionResponse,
  ClearAllRequest,
  ClearAllResponse,
  ExplainQueryRequest,
  ExplainQueryResponse,
  PingRequest,
  PingResponse
} from './DataWorkerMessageTypes.js';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID.js';
import type { RecordData, StorageQuery } from '../../../daemons/data-daemon/shared/DataStorageAdapter.js';

// ============================================================================
// DataWorkerClient Class
// ============================================================================

/**
 * Type-safe client for Data Rust worker.
 *
 * Handles all data operations: CRUD, schema management, query analysis.
 * Each operation maps to a DataStorageAdapter method.
 */
export class DataWorkerClient extends WorkerClient<unknown, unknown> {
  constructor(config: WorkerClientConfig | string) {
    // Allow simple socket path string or full config
    const fullConfig: WorkerClientConfig =
      typeof config === 'string'
        ? { socketPath: config }
        : config;

    super(fullConfig);
  }

  // ============================================================================
  // Database Handle Management
  // ============================================================================

  /**
   * Open a new database handle.
   *
   * The Rust worker will:
   * - Detect storage type (InternalSSD/ExternalSSD/SDCard)
   * - Configure SQLite pragmas appropriately
   * - Create connection pool
   * - Return handle UUID
   *
   * @param request - Database open request
   * @param userId - Optional userId context
   * @returns Promise with handle, storage type, pragma mode, pool size
   * @throws {WorkerError} if open fails
   */
  async openDatabase(
    request: OpenDatabaseRequest,
    userId?: string
  ): Promise<OpenDatabaseResponse> {
    const response = await this.send('open-database', request, userId);
    return response.payload as OpenDatabaseResponse;
  }

  /**
   * Close a database handle.
   *
   * The Rust worker will:
   * - Flush pending writes
   * - Close all connections in pool
   * - Release resources
   * - Remove handle from registry
   *
   * @param request - Database close request
   * @param userId - Optional userId context
   * @returns Promise confirming closure
   * @throws {WorkerError} if close fails
   */
  async closeDatabase(
    request: CloseDatabaseRequest,
    userId?: string
  ): Promise<CloseDatabaseResponse> {
    const response = await this.send('close-database', request, userId);
    return response.payload as CloseDatabaseResponse;
  }

  // ============================================================================
  // CRUD Operations
  // ============================================================================

  /**
   * Create a new record.
   *
   * @param request - Create record request with handle, collection, record
   * @param userId - Optional userId context
   * @returns Promise with created record
   * @throws {WorkerError} if create fails
   */
  async createRecord<T extends RecordData>(
    request: CreateRecordRequest<T>,
    userId?: string
  ): Promise<CreateRecordResponse<T>> {
    const response = await this.send('create-record', request, userId);
    return response.payload as CreateRecordResponse<T>;
  }

  /**
   * Read a single record by ID.
   *
   * @param request - Read record request with handle, collection, id
   * @param userId - Optional userId context
   * @returns Promise with record (undefined if not found)
   * @throws {WorkerError} if read fails
   */
  async readRecord<T extends RecordData>(
    request: ReadRecordRequest,
    userId?: string
  ): Promise<ReadRecordResponse<T>> {
    const response = await this.send('read-record', request, userId);
    return response.payload as ReadRecordResponse<T>;
  }

  /**
   * Query records with filters, sorting, pagination.
   *
   * @param request - Query request with handle and StorageQuery
   * @param userId - Optional userId context
   * @returns Promise with matching records and metadata
   * @throws {WorkerError} if query fails
   */
  async queryRecords<T extends RecordData>(
    request: QueryRecordsRequest,
    userId?: string
  ): Promise<QueryRecordsResponse<T>> {
    const response = await this.send('query-records', request, userId);
    return response.payload as QueryRecordsResponse<T>;
  }

  /**
   * Count records matching query without fetching data.
   *
   * Efficient alternative to queryRecords when only totalCount is needed.
   *
   * @param request - Count request with handle and StorageQuery
   * @param userId - Optional userId context
   * @returns Promise with count result
   * @throws {WorkerError} if count fails
   */
  async countRecords(
    request: CountRecordsRequest,
    userId?: string
  ): Promise<CountRecordsResponse> {
    const response = await this.send('count-records', request, userId);
    return response.payload as CountRecordsResponse;
  }

  /**
   * Update an existing record.
   *
   * @param request - Update request with handle, collection, id, data
   * @param userId - Optional userId context
   * @returns Promise with updated record
   * @throws {WorkerError} if update fails
   */
  async updateRecord<T extends RecordData>(
    request: UpdateRecordRequest<T>,
    userId?: string
  ): Promise<UpdateRecordResponse<T>> {
    const response = await this.send('update-record', request, userId);
    return response.payload as UpdateRecordResponse<T>;
  }

  /**
   * Delete a record by ID.
   *
   * @param request - Delete request with handle, collection, id
   * @param userId - Optional userId context
   * @returns Promise confirming deletion
   * @throws {WorkerError} if delete fails
   */
  async deleteRecord(
    request: DeleteRecordRequest,
    userId?: string
  ): Promise<DeleteRecordResponse> {
    const response = await this.send('delete-record', request, userId);
    return response.payload as DeleteRecordResponse;
  }

  // ============================================================================
  // Schema and Collection Management
  // ============================================================================

  /**
   * Ensure collection schema exists.
   *
   * The Rust worker will create tables/collections if needed.
   *
   * @param request - Schema request with handle, collection, schema
   * @param userId - Optional userId context
   * @returns Promise confirming schema exists
   * @throws {WorkerError} if schema creation fails
   */
  async ensureSchema(
    request: EnsureSchemaRequest,
    userId?: string
  ): Promise<EnsureSchemaResponse> {
    const response = await this.send('ensure-schema', request, userId);
    return response.payload as EnsureSchemaResponse;
  }

  /**
   * List all collections in the database.
   *
   * @param request - List request with handle
   * @param userId - Optional userId context
   * @returns Promise with collection names
   * @throws {WorkerError} if list fails
   */
  async listCollections(
    request: ListCollectionsRequest,
    userId?: string
  ): Promise<ListCollectionsResponse> {
    const response = await this.send('list-collections', request, userId);
    return response.payload as ListCollectionsResponse;
  }

  /**
   * Get statistics for a collection.
   *
   * @param request - Stats request with handle, collection
   * @param userId - Optional userId context
   * @returns Promise with collection statistics
   * @throws {WorkerError} if stats retrieval fails
   */
  async getCollectionStats(
    request: GetCollectionStatsRequest,
    userId?: string
  ): Promise<GetCollectionStatsResponse> {
    const response = await this.send('get-collection-stats', request, userId);
    return response.payload as GetCollectionStatsResponse;
  }

  /**
   * Truncate a collection (delete all records, keep structure).
   *
   * @param request - Truncate request with handle, collection
   * @param userId - Optional userId context
   * @returns Promise confirming truncation
   * @throws {WorkerError} if truncate fails
   */
  async truncateCollection(
    request: TruncateCollectionRequest,
    userId?: string
  ): Promise<TruncateCollectionResponse> {
    const response = await this.send('truncate-collection', request, userId);
    return response.payload as TruncateCollectionResponse;
  }

  /**
   * Clear all collections (delete all data, keep structure).
   *
   * @param request - Clear request with handle
   * @param userId - Optional userId context
   * @returns Promise with clear statistics
   * @throws {WorkerError} if clear fails
   */
  async clearAll(
    request: ClearAllRequest,
    userId?: string
  ): Promise<ClearAllResponse> {
    const response = await this.send('clear-all', request, userId);
    return response.payload as ClearAllResponse;
  }

  // ============================================================================
  // Query Analysis
  // ============================================================================

  /**
   * Explain query execution (dry-run).
   *
   * Shows what SQL query would be generated without executing it.
   * Useful for debugging and optimization.
   *
   * @param request - Explain request with handle, query
   * @param userId - Optional userId context
   * @returns Promise with query explanation
   * @throws {WorkerError} if explain fails
   */
  async explainQuery(
    request: ExplainQueryRequest,
    userId?: string
  ): Promise<ExplainQueryResponse> {
    const response = await this.send('explain-query', request, userId);
    return response.payload as ExplainQueryResponse;
  }

  // ============================================================================
  // Health Check
  // ============================================================================

  /**
   * Ping the worker to check if it's alive and responsive.
   *
   * This sends a lightweight health check request to the worker and returns
   * statistics about uptime, active handles, and performance.
   *
   * @returns Promise with worker health stats
   * @throws {WorkerError} if worker is frozen or unresponsive
   */
  async ping(): Promise<PingResponse> {
    const response = await this.send('ping', {});
    return response.payload as PingResponse;
  }
}

// ============================================================================
// Singleton Pattern (Optional)
// ============================================================================

/**
 * Shared singleton instance for application-wide use.
 * Call `DataWorkerClient.initialize()` once at startup.
 */
let sharedInstance: DataWorkerClient | null = null;

export namespace DataWorkerClient {
  /**
   * Initialize the shared data worker client.
   *
   * @param config - Configuration for worker client
   * @returns The shared instance
   */
  export function initialize(config: WorkerClientConfig | string): DataWorkerClient {
    if (sharedInstance) {
      throw new Error('DataWorkerClient already initialized');
    }
    sharedInstance = new DataWorkerClient(config);
    return sharedInstance;
  }

  /**
   * Get the shared data worker client instance.
   *
   * @throws {Error} if not initialized
   */
  export function getInstance(): DataWorkerClient {
    if (!sharedInstance) {
      throw new Error('DataWorkerClient not initialized. Call initialize() first.');
    }
    return sharedInstance;
  }

  /**
   * Check if shared instance is initialized.
   */
  export function isInitialized(): boolean {
    return sharedInstance !== null;
  }

  /**
   * Dispose of the shared instance (for testing).
   */
  export async function dispose(): Promise<void> {
    if (sharedInstance) {
      await sharedInstance.disconnect();
      sharedInstance = null;
    }
  }
}
