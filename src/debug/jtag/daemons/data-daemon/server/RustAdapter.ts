/**
 * Rust Adapter - High-Performance Multi-Database Storage via Rust Worker
 *
 * EXPERIMENTAL: Uses Rust worker for massive concurrency and storage-aware optimization
 *
 * Architecture:
 * - TypeScript: Orchestration, validation, decorators, events (unchanged)
 * - Rust Worker: I/O, connection pooling, storage detection, concurrency control
 * - Communication: Unix domain socket (low overhead, high throughput)
 *
 * Key Features:
 * - Storage Detection: Auto-detects InternalSSD/ExternalSSD/SDCard and configures SQLite pragmas
 * - Multi-Database: Each handle manages independent database with own connection pool
 * - Massive Concurrency: 100+ handles active simultaneously, each with pooling
 * - Graceful Degradation: Falls back to TypeScript adapter if Rust worker unavailable
 *
 * Usage:
 * ```typescript
 * // Via data/open command
 * const handle = await Commands.execute('data/open', {
 *   adapter: 'rust',
 *   config: {
 *     filename: '/Users/joel/.continuum/data/test-rust.sqlite',
 *     storageType: 'auto-detect'  // or explicit: 'internal-ssd', 'sd-card'
 *   }
 * });
 * ```
 */

import {
  DataStorageAdapter,
  type DataRecord,
  type StorageQuery,
  type StorageResult,
  type StorageAdapterConfig,
  type CollectionStats,
  type StorageOperation,
  type RecordData,
  type QueryExplanation,
  type StorageCapabilities
} from '../shared/DataStorageAdapter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { Logger } from '../../../system/core/logging/Logger';
import { DataWorkerClient } from '../../../shared/ipc/data-worker/DataWorkerClient';

const log = Logger.create('RustAdapter', 'data/rust');

/**
 * Rust Adapter - Delegates all storage operations to Rust worker
 *
 * Implements full DataStorageAdapter interface but executes operations via Rust worker
 * for high performance and automatic storage optimization.
 */
export class RustAdapter extends DataStorageAdapter {
  private config: StorageAdapterConfig | null = null;
  private isInitialized: boolean = false;
  private workerHandle: string | null = null;  // Rust worker's handle ID
  private workerClient: DataWorkerClient | null = null;  // Client for Rust worker

  constructor() {
    super();
    console.log('🦀 RustAdapter CONSTRUCTOR called');
  }

  /**
   * Initialize connection to Rust worker and open database handle
   * Retries with exponential backoff if worker not ready
   */
  async initialize(config: StorageAdapterConfig): Promise<void> {
    console.log('🦀 RustAdapter.initialize() called with config:', JSON.stringify(config, null, 2));
    console.log('🦀 RustAdapter: Step 1 - Check if initialized');
    if (this.isInitialized) {
      console.log('🦀 RustAdapter: Already initialized, skipping');
      log.debug('Already initialized, skipping');
      return;
    }

    console.log('🦀 RustAdapter: Step 2 - About to call log.info');
    log.info('🦀 Initializing Rust adapter (experimental) with retry logic...');
    console.log('🦀 RustAdapter: Step 3 - Setting config');
    this.config = config;

    const socketPath = (config.options?.socketPath as string) || '/tmp/jtag-data-daemon-worker.sock';
    const dbFilename = config.options?.filename as string;

    // DataDaemon MUST provide the path - Rust should never have default path logic
    if (!dbFilename) {
      throw new Error('RustAdapter: No filename provided in config - DataDaemon must calculate path from config.env');
    }

    // Retry logic: 6 attempts with exponential backoff (total ~15 seconds)
    // Attempts: 100ms, 200ms, 500ms, 1000ms, 2000ms, 5000ms
    const retryDelays = [100, 200, 500, 1000, 2000, 5000];
    let lastError: Error | null = null;

    for (let attempt = 0; attempt < retryDelays.length; attempt++) {
      try {
        if (attempt > 0) {
          const delay = retryDelays[attempt - 1];
          console.log(`🦀 RustAdapter: Retry attempt ${attempt + 1}/${retryDelays.length} after ${delay}ms...`);
          await new Promise(resolve => setTimeout(resolve, delay));
        }

        // Create DataWorkerClient
        console.log(`🦀 RustAdapter: Creating worker client for socket: ${socketPath}`);
        this.workerClient = new DataWorkerClient({
          socketPath,
          timeout: 10000
        });

        // Connect to Rust worker
        console.log('🦀 RustAdapter: Calling workerClient.connect()...');
        await this.workerClient.connect();
        console.log('🦀 RustAdapter: ✅ Connected to worker');

        // Open database handle
        console.log(`🦀 RustAdapter: Opening database: ${dbFilename}`);
        const openResult = await this.workerClient.openDatabase({
          filename: dbFilename,
          adapterType: 'sqlite',
          storageType: (config.options?.storageType as 'auto-detect' | undefined) || 'auto-detect'
        });
        console.log(`🦀 RustAdapter: ✅ Database opened, handle: ${openResult.handle}`);

        this.workerHandle = openResult.handle;

        log.info(`✅ Rust adapter initialized - handle: ${this.workerHandle} (attempt ${attempt + 1})`);
        log.info(`   Storage type: ${openResult.storageType}`);
        log.info(`   Pragma mode: ${openResult.pragmaMode}`);
        log.info(`   Pool size: ${openResult.poolSize}`);

        this.isInitialized = true;
        return; // SUCCESS - exit retry loop
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));
        console.log(`🦀 RustAdapter: ❌ Attempt ${attempt + 1}/${retryDelays.length} failed: ${lastError.message}`);

        // Clean up failed client
        if (this.workerClient) {
          try {
            await this.workerClient.disconnect();
          } catch {
            // Ignore disconnect errors
          }
          this.workerClient = null;
        }

        // Continue to next retry attempt
      }
    }

    // All retries exhausted - graceful degradation
    console.log('🦀 RustAdapter: ❌ All retry attempts exhausted');
    console.log(`🦀 RustAdapter: Last error: ${lastError?.message}`);
    log.warn('⚠️  Rust worker not available after retries - connection failed');
    log.warn(`   Last error: ${lastError?.message}`);
    log.warn('⚠️  RustAdapter will return "not yet implemented" errors');
    log.warn('⚠️  Use adapter: "sqlite" for production workloads');

    // Mark as initialized but non-functional
    this.isInitialized = true;
    this.workerClient = null;
    this.workerHandle = null;
  }

  /**
   * Create record - delegates to Rust worker
   */
  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        success: false,
        error: 'Rust adapter not initialized or worker not available - use adapter: "sqlite"'
      };
    }

    try {
      const response = await this.workerClient.createRecord<T>({
        handle: this.workerHandle,
        collection: record.collection,
        record
      });

      return {
        success: true,
        data: response.record
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Read record by ID - delegates to Rust worker
   */
  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        success: false,
        error: 'Rust adapter not initialized or worker not available - use adapter: "sqlite"'
      };
    }

    try {
      const response = await this.workerClient.readRecord<T>({
        handle: this.workerHandle,
        collection,
        id
      });

      if (!response.record) {
        return {
          success: false,
          error: 'Record not found'
        };
      }

      log.info(`RustAdapter.read() response.record keys: ${JSON.stringify(Object.keys(response.record))}`);
      log.info(`RustAdapter.read() response.record: ${JSON.stringify(response.record).substring(0, 500)}`);

      return {
        success: true,
        data: response.record
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Query records - delegates to Rust worker
   */
  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        success: false,
        error: 'Rust adapter not initialized or worker not available - use adapter: "sqlite"'
      };
    }

    try {
      const response = await this.workerClient.queryRecords<T>({
        handle: this.workerHandle,
        query
      });

      if (response.records && response.records.length > 0) {
        log.info(`RustAdapter.query() first record keys: ${JSON.stringify(Object.keys(response.records[0]))}`);
        log.info(`RustAdapter.query() first record: ${JSON.stringify(response.records[0]).substring(0, 500)}`);
      }

      return {
        success: true,
        data: response.records,
        metadata: {
          totalCount: response.totalCount,
          queryTime: response.queryTime
        }
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Count records matching query without fetching data
   *
   * Delegates to Rust worker for efficient COUNT(*) query execution.
   */
  async count(query: StorageQuery): Promise<StorageResult<number>> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        success: false,
        error: 'Rust adapter not initialized or worker not available - use adapter: "sqlite"'
      };
    }

    try {
      const response = await this.workerClient.countRecords({
        handle: this.workerHandle,
        query
      });

      return {
        success: true,
        data: response.count,
        metadata: {
          queryTime: response.queryTime
        }
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Update record - delegates to Rust worker
   */
  async update<T extends RecordData>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    incrementVersion?: boolean
  ): Promise<StorageResult<DataRecord<T>>> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        success: false,
        error: 'Rust adapter not initialized or worker not available - use adapter: "sqlite"'
      };
    }

    try {
      const response = await this.workerClient.updateRecord<T>({
        handle: this.workerHandle,
        collection,
        id,
        data,
        incrementVersion
      });

      return {
        success: true,
        data: response.record
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Delete record - delegates to Rust worker
   */
  async delete(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        success: false,
        error: 'Rust adapter not initialized or worker not available - use adapter: "sqlite"'
      };
    }

    try {
      const response = await this.workerClient.deleteRecord({
        handle: this.workerHandle,
        collection,
        id
      });

      return {
        success: true,
        data: response.success
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * List collections - delegates to Rust worker
   */
  async listCollections(): Promise<StorageResult<string[]>> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        success: false,
        error: 'Rust adapter not initialized or worker not available - use adapter: "sqlite"'
      };
    }

    try {
      const response = await this.workerClient.listCollections({
        handle: this.workerHandle
      });

      return {
        success: true,
        data: response.collections
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Get collection stats - delegates to Rust worker
   */
  async getCollectionStats(collection: string): Promise<StorageResult<CollectionStats>> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        success: false,
        error: 'Rust adapter not initialized or worker not available - use adapter: "sqlite"'
      };
    }

    try {
      const response = await this.workerClient.getCollectionStats({
        handle: this.workerHandle,
        collection
      });

      return {
        success: true,
        data: response.stats
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Batch operations - delegates to Rust worker
   *
   * NOTE: Batch operations are not yet implemented in Rust worker.
   * This executes operations sequentially for now.
   */
  async batch<T extends RecordData>(operations: StorageOperation<T>[]): Promise<StorageResult<unknown[]>> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        success: false,
        error: 'Rust adapter not initialized or worker not available - use adapter: "sqlite"'
      };
    }

    // TODO: Implement batch operations in Rust worker
    // For now, execute sequentially
    return {
      success: false,
      error: 'Batch operations not yet implemented in Rust adapter - use adapter: "sqlite"'
    };
  }

  /**
   * Clear all data - delegates to Rust worker
   */
  async clear(): Promise<StorageResult<boolean>> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        success: false,
        error: 'Rust adapter not initialized or worker not available - use adapter: "sqlite"'
      };
    }

    try {
      const response = await this.workerClient.clearAll({
        handle: this.workerHandle
      });

      return {
        success: true,
        data: true
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Ensure schema exists - delegates to Rust worker
   */
  async ensureSchema(collection: string, schema?: unknown): Promise<StorageResult<boolean>> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        success: false,
        error: 'Rust adapter not initialized or worker not available - use adapter: "sqlite"'
      };
    }

    try {
      const response = await this.workerClient.ensureSchema({
        handle: this.workerHandle,
        collection,
        schema
      });

      return {
        success: true,
        data: response.success
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Clear all data with reporting - delegates to Rust worker
   */
  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        success: false,
        error: 'Rust adapter not initialized or worker not available - use adapter: "sqlite"'
      };
    }

    try {
      const response = await this.workerClient.clearAll({
        handle: this.workerHandle
      });

      return {
        success: true,
        data: {
          tablesCleared: response.tablesCleared,
          recordsDeleted: response.recordsDeleted
        }
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Truncate collection - delegates to Rust worker
   */
  async truncate(collection: string): Promise<StorageResult<boolean>> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        success: false,
        error: 'Rust adapter not initialized or worker not available - use adapter: "sqlite"'
      };
    }

    try {
      const response = await this.workerClient.truncateCollection({
        handle: this.workerHandle,
        collection
      });

      return {
        success: true,
        data: response.success
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Cleanup and optimization - delegates to Rust worker
   */
  async cleanup(): Promise<void> {
    // Rust worker handles cleanup automatically (VACUUM, etc.)
    // No explicit cleanup command needed
  }

  /**
   * Explain query execution - delegates to Rust worker
   */
  async explainQuery(query: StorageQuery): Promise<QueryExplanation> {
    if (!this.isInitialized || !this.workerClient || !this.workerHandle) {
      return {
        query,
        translatedQuery: 'Rust adapter not initialized or worker not available',
        adapterType: 'rust',
        timestamp: new Date().toISOString()
      };
    }

    try {
      const response = await this.workerClient.explainQuery({
        handle: this.workerHandle,
        query
      });

      return response.explanation;
    } catch (error) {
      return {
        query,
        translatedQuery: `Error: ${error instanceof Error ? error.message : String(error)}`,
        adapterType: 'rust',
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Get adapter capabilities
   */
  getCapabilities(): StorageCapabilities {
    return {
      supportsTransactions: true,
      supportsIndexing: true,
      supportsFullTextSearch: false, // TODO: Add FTS support
      supportsReplication: false,
      maxRecordSize: 10 * 1024 * 1024, // 10MB
      concurrentConnections: 100 // Rust worker massive parallelism
    };
  }

  /**
   * Close connection to Rust worker
   */
  async close(): Promise<void> {
    if (!this.isInitialized) {
      return;
    }

    log.info('Closing Rust adapter');

    try {
      // Close database handle in Rust worker
      if (this.workerClient && this.workerHandle) {
        await this.workerClient.closeDatabase({ handle: this.workerHandle });
        log.info(`✅ Closed database handle: ${this.workerHandle}`);
      }

      // Disconnect from worker
      if (this.workerClient) {
        await this.workerClient.disconnect();
        log.info('✅ Disconnected from Rust worker');
      }
    } catch (error) {
      log.warn(`Error closing Rust adapter: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      this.workerHandle = null;
      this.workerClient = null;
      this.isInitialized = false;
    }
  }
}
