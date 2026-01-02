/**
 * Rust Worker Storage Adapter
 *
 * Bridges TypeScript DataDaemon (entity logic) with Rust worker (fast storage).
 *
 * Architecture:
 * - TypeScript owns: Entity validation, decorators, events, domain logic
 * - Rust owns: Database I/O, connection pooling, concurrent operations
 *
 * Communication: Unix domain socket (low overhead, high throughput)
 */

import * as net from 'net';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import {
  DataStorageAdapter,
  type DataRecord,
  type StorageQuery,
  type StorageResult,
  type StorageAdapterConfig,
  type StorageCapabilities,
  type RecordData,
  type CollectionStats,
  type StorageOperation,
  type QueryExplanation,
  type CollectionSchema
} from '../shared/DataStorageAdapter';
import { SqlNamingConverter } from '../shared/SqlNamingConverter';
import {
  type VectorSearchOptions,
  type VectorSearchResponse,
  type VectorSearchResult as VectorSearchResultType,
  type VectorEmbedding,
  toNumberArray
} from '../shared/VectorSearchTypes';
import { RustEmbeddingClient } from '../../../system/core/services/RustEmbeddingClient';
import { Logger } from '../../../system/core/logging/Logger';

const log = Logger.create('RustWorkerStorageAdapter', 'data');

/**
 * Configuration for Rust worker connection
 * ALL fields are required - no defaults, caller must provide everything
 */
export interface RustWorkerConfig {
  /** Path to Rust worker Unix socket (e.g., /tmp/jtag-data-daemon-worker.sock) */
  socketPath: string;
  /** Absolute path to SQLite database file */
  dbPath: string;
  /** Connection/request timeout in ms */
  timeout: number;
}

/**
 * Rust worker request/response format (simpler than full JTAG protocol)
 * Uses serde's tag-based enum serialization
 */
interface RustResponse {
  status: 'ok' | 'error' | 'pong';
  data?: any;
  message?: string;
  uptime_seconds?: number;
}

/**
 * Rust Worker Storage Adapter - Fast concurrent storage via Rust process
 */
export class RustWorkerStorageAdapter extends DataStorageAdapter {
  private config!: RustWorkerConfig;
  private socket: net.Socket | null = null;
  private adapterHandle: string | null = null;  // Handle from adapter/open
  private pendingResponse: {
    resolve: (value: any) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout;
  } | null = null;

  private reconnecting: boolean = false;
  private buffer: string = '';

  /**
   * Convert object keys from camelCase to snake_case (for sending to Rust/SQL)
   */
  private toSnakeCaseObject(obj: Record<string, any>): Record<string, any> {
    const result: Record<string, any> = {};
    for (const [key, value] of Object.entries(obj)) {
      const snakeKey = SqlNamingConverter.toSnakeCase(key);
      result[snakeKey] = value;
    }
    return result;
  }

  /**
   * Convert object keys from snake_case to camelCase (for returning to TypeScript)
   */
  private toCamelCaseObject(obj: Record<string, any>): Record<string, any> {
    const result: Record<string, any> = {};
    for (const [key, value] of Object.entries(obj)) {
      const camelKey = SqlNamingConverter.toCamelCase(key);
      result[camelKey] = value;
    }
    return result;
  }

  constructor(config?: RustWorkerConfig) {
    super();
    if (config) {
      this.config = config;
    }
  }

  /**
   * Initialize connection to Rust worker
   *
   * REQUIRED in config.options:
   * - socketPath: Path to Rust worker Unix socket
   * - dbPath: Absolute path to SQLite database file
   */
  async initialize(config: StorageAdapterConfig): Promise<void> {
    const options = config.options as any;

    // Require socket and database paths - no defaults
    if (!options?.socketPath) {
      throw new Error('RustWorkerStorageAdapter requires socketPath in options');
    }
    if (!options?.dbPath) {
      throw new Error('RustWorkerStorageAdapter requires dbPath in options');
    }

    this.config = {
      socketPath: options.socketPath,
      dbPath: options.dbPath,
      timeout: options.timeout || 60000  // 60s - needed for large vector searches (3K+ vectors)
    };

    await this.connect();

    // Open SQLite adapter and store handle
    const response = await this.sendCommand<{ handle: string }>('adapter/open', {
      config: {
        adapter_type: 'sqlite',
        connection_string: this.config.dbPath
      }
    });

    if (response.status === 'ok' && response.data?.handle) {
      this.adapterHandle = response.data.handle;
      console.log(`✅ Opened SQLite adapter: ${this.config.dbPath} → handle ${this.adapterHandle}`);
    } else {
      throw new Error(`Failed to open adapter: ${response.message || 'Unknown error'}`);
    }
  }

  /**
   * Connect to Rust worker Unix socket
   */
  private async connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.socket = net.createConnection(this.config.socketPath);

      this.socket.on('connect', () => {
        console.log(`✅ Connected to Rust worker: ${this.config.socketPath}`);
        this.reconnecting = false;
        resolve();
      });

      this.socket.on('data', (data) => {
        this.handleData(data);
      });

      this.socket.on('error', (error) => {
        console.error('❌ Rust worker socket error:', error);
        if (!this.reconnecting) {
          reject(error);
        }
      });

      this.socket.on('close', () => {
        console.warn('⚠️  Rust worker connection closed, will reconnect on next request');
        this.socket = null;
        this.adapterHandle = null; // Need to reopen adapter after reconnect
      });

      // Connection timeout
      setTimeout(() => {
        if (!this.socket || this.socket.connecting) {
          reject(new Error(`Connection timeout: ${this.config.socketPath}`));
        }
      }, this.config.timeout);
    });
  }

  /**
   * Handle incoming data from Rust worker (line-delimited JSON)
   */
  private handleData(data: Buffer): void {
    this.buffer += data.toString();

    // Process complete lines (messages are newline-delimited)
    const lines = this.buffer.split('\n');
    this.buffer = lines.pop() || ''; // Keep incomplete line in buffer

    for (const line of lines) {
      if (!line.trim()) continue;

      try {
        const response: RustResponse = JSON.parse(line);

        if (this.pendingResponse) {
          clearTimeout(this.pendingResponse.timeout);
          const pending = this.pendingResponse;
          this.pendingResponse = null;
          pending.resolve(response);
        }
      } catch (error) {
        console.error('Failed to parse response from Rust worker:', error);
      }
    }
  }

  /**
   * Send command to Rust worker and wait for response
   * Uses Rust's serde tag format: {"command": "name", ...params}
   * Auto-reconnects if connection was lost
   */
  private async sendCommand<T = any>(command: string, params: Record<string, any> = {}): Promise<RustResponse & { data?: T }> {
    // Auto-reconnect if socket is closed
    if (!this.socket) {
      console.log('🔄 Reconnecting to Rust worker...');
      await this.connect();

      // Reopen adapter after reconnect
      const response = await this.sendCommand<{ handle: string }>('adapter/open', {
        config: {
          adapter_type: 'sqlite',
          connection_string: this.config.dbPath
        }
      });

      if (response.status === 'ok' && response.data?.handle) {
        this.adapterHandle = response.data.handle;
        console.log(`✅ Reopened SQLite adapter: ${this.config.dbPath} → handle ${this.adapterHandle}`);
      } else {
        throw new Error(`Failed to reopen adapter: ${response.message || 'Unknown error'}`);
      }
    }

    const request = {
      command,
      ...params
    };

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pendingResponse = null;
        reject(new Error(`Request timeout: ${command}`));
      }, this.config.timeout);

      this.pendingResponse = { resolve, reject, timeout };

      // Send newline-delimited JSON
      this.socket!.write(JSON.stringify(request) + '\n');
    });
  }

  /**
   * Ensure we're connected and have an adapter handle (auto-reconnect)
   */
  private async ensureConnected(): Promise<void> {
    if (!this.socket || !this.adapterHandle) {
      // Force reconnect by calling sendCommand with a benign command
      // The reconnect logic in sendCommand will re-establish connection and adapter
      await this.sendCommand('ping', {});
    }
  }

  /**
   * Create record - delegates to Rust worker
   */
  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    try {
      await this.ensureConnected();
    } catch (error: any) {
      return { success: false, error: `Connection failed: ${error.message}` };
    }

    try {
      // Convert data keys to snake_case for SQL columns
      const snakeCaseData = this.toSnakeCaseObject(record.data as Record<string, any>);

      // Add id and metadata fields for storage
      const fullData = {
        id: record.id,
        ...snakeCaseData,
        created_at: record.metadata?.createdAt || new Date().toISOString(),
        updated_at: record.metadata?.updatedAt || new Date().toISOString(),
        version: record.metadata?.version || 1
      };

      const response = await this.sendCommand('data/create', {
        handle: this.adapterHandle,
        collection: SqlNamingConverter.toTableName(record.collection),
        data: fullData
      });

      if (response.status !== 'ok') {
        return { success: false, error: response.message || 'Create failed' };
      }

      return {
        success: true,
        data: {
          id: record.id,
          collection: record.collection,
          data: record.data,
          metadata: record.metadata
        }
      };
    } catch (error: any) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Read single record by ID - uses query with filter
   */
  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    try {
      await this.ensureConnected();
    } catch (error: any) {
      return { success: false, error: `Connection failed: ${error.message}` };
    }

    try {
      const response = await this.sendCommand<{ items: T[]; count: number }>('data/list', {
        handle: this.adapterHandle,
        collection,
        filter: { id },
        limit: 1
      });

      if (response.status !== 'ok' || !response.data?.items?.length) {
        return { success: false, error: 'Record not found' };
      }

      const item = response.data.items[0];
      // Ensure id is always present in the data object
      // Some callers expect data.data.id to be set
      if (!item.id) {
        item.id = id;
      }
      return {
        success: true,
        data: {
          id,
          collection,
          data: item,
          metadata: {
            createdAt: new Date().toISOString(),
            updatedAt: new Date().toISOString(),
            version: 1
          }
        }
      };
    } catch (error: any) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Query records with filters
   */
  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    try {
      await this.ensureConnected();
    } catch (error: any) {
      return { success: false, error: `Connection failed: ${error.message}` };
    }

    try {
      // Convert filter keys to snake_case for SQL
      const snakeCaseFilter = query.filter ? this.toSnakeCaseObject(query.filter) : undefined;

      // Convert sort field names to snake_case
      const snakeCaseOrderBy = query.sort?.map(s => ({
        field: SqlNamingConverter.toSnakeCase(s.field),
        direction: s.direction
      }));

      const response = await this.sendCommand<{ items: T[]; count: number }>('data/list', {
        handle: this.adapterHandle,
        collection: SqlNamingConverter.toTableName(query.collection),
        filter: snakeCaseFilter,
        order_by: snakeCaseOrderBy,
        limit: query.limit,
        offset: query.offset
      });

      if (response.status !== 'ok') {
        return { success: false, error: response.message || 'Query failed' };
      }

      const records: DataRecord<T>[] = (response.data?.items || []).map((item: any) => {
        // Two table formats:
        // 1. Simple entity: has 'data' column containing JSON string
        // 2. Entity-specific: has individual columns for each field

        let entityData: T;

        if (typeof item.data === 'string') {
          // Simple entity table - parse JSON from data column
          // Data inside is already camelCase (stored as-is)
          entityData = JSON.parse(item.data) as T;
        } else if (item.data && typeof item.data === 'object') {
          // Data is already an object (maybe pre-parsed by Rust)
          entityData = item.data as T;
        } else {
          // Entity-specific table - extract non-BaseEntity fields
          // Rust returns snake_case columns, convert to camelCase
          const { id, created_at, updated_at, version, ...rest } = item;
          entityData = this.toCamelCaseObject(rest) as T;
        }

        // Ensure id is always present in entityData
        // Some callers access data.id directly instead of the wrapper
        if (!(entityData as any).id) {
          (entityData as any).id = item.id;
        }

        return {
          id: item.id,
          collection: query.collection,
          data: entityData,
          metadata: {
            createdAt: item.created_at || new Date().toISOString(),
            updatedAt: item.updated_at || new Date().toISOString(),
            version: item.version || 1
          }
        };
      });

      return {
        success: true,
        data: records,
        metadata: {
          totalCount: response.data?.count || records.length
        }
      };
    } catch (error: any) {
      return { success: false, error: error.message };
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
    try {
      await this.ensureConnected();
    } catch (error: any) {
      return { success: false, error: `Connection failed: ${error.message}` };
    }

    try {
      // Convert data keys to snake_case for SQL columns
      const snakeCaseData = this.toSnakeCaseObject(data as Record<string, any>);

      // Add updated_at and version
      const updateData = {
        ...snakeCaseData,
        updated_at: new Date().toISOString(),
        version: incrementVersion ? { $increment: 1 } : undefined
      };

      const response = await this.sendCommand('data/update', {
        handle: this.adapterHandle,
        collection: SqlNamingConverter.toTableName(collection),
        id,
        data: updateData
      });

      if (response.status !== 'ok') {
        return { success: false, error: response.message || 'Update failed' };
      }

      return {
        success: true,
        data: {
          id,
          collection,
          data: data as T,
          metadata: {
            createdAt: new Date().toISOString(),
            updatedAt: new Date().toISOString(),
            version: 1
          }
        }
      };
    } catch (error: any) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Delete record
   */
  async delete(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    try {
      await this.ensureConnected();
    } catch (error: any) {
      return { success: false, error: `Connection failed: ${error.message}` };
    }

    try {
      const response = await this.sendCommand('data/delete', {
        handle: this.adapterHandle,
        collection,
        id
      });

      if (response.status !== 'ok') {
        return { success: false, error: response.message || 'Delete failed' };
      }

      return { success: true, data: true };
    } catch (error: any) {
      return { success: false, error: error.message };
    }
  }

  /**
   * List all collections - TODO: Implement in Rust worker
   */
  async listCollections(): Promise<StorageResult<string[]>> {
    throw new Error('List collections not yet implemented in Rust worker');
  }

  /**
   * Get collection statistics - TODO: Implement in Rust worker
   */
  async getCollectionStats(collection: string): Promise<StorageResult<CollectionStats>> {
    throw new Error('Collection stats not yet implemented in Rust worker');
  }

  /**
   * Batch operations - TODO: Optimize in Rust worker
   */
  async batch<T extends RecordData>(operations: StorageOperation<T>[]): Promise<StorageResult<unknown[]>> {
    // Naive implementation - execute sequentially
    // TODO: Send all operations to Rust worker in single message
    const results = [];
    for (const op of operations) {
      try {
        let result;
        switch (op.type) {
          case 'create':
            // Create DataRecord from operation data
            const createRecord: DataRecord<T> = {
              id: op.id!,
              collection: op.collection,
              data: op.data as T,
              metadata: {
                createdAt: new Date().toISOString(),
                updatedAt: new Date().toISOString(),
                version: 1
              }
            };
            result = await this.create(createRecord);
            break;
          case 'read':
            result = await this.read(op.collection, op.id!);
            break;
          case 'update':
            result = await this.update(op.collection, op.id!, op.data as Partial<T>);
            break;
          case 'delete':
            result = await this.delete(op.collection, op.id!);
            break;
        }
        results.push(result);
      } catch (error: any) {
        results.push({ success: false, error: error.message });
      }
    }
    return {
      success: true,
      data: results
    };
  }

  /**
   * Clear all data from all collections - TODO: Implement in Rust worker
   */
  async clear(): Promise<StorageResult<boolean>> {
    throw new Error('Clear not yet implemented in Rust worker');
  }

  /**
   * Ensure collection schema exists (no-op for now, SQLite is schemaless for our use)
   */
  async ensureSchema(collection: string, schema?: CollectionSchema): Promise<StorageResult<boolean>> {
    // Rust worker uses dynamic table creation on first insert
    // No need to explicitly create schema
    return {
      success: true,
      data: true
    };
  }

  /**
   * Clear all data from all collections with reporting - TODO: Implement in Rust worker
   */
  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    throw new Error('ClearAll not yet implemented in Rust worker');
  }

  /**
   * Truncate specific collection - TODO: Implement in Rust worker
   */
  async truncate(collection: string): Promise<StorageResult<boolean>> {
    throw new Error('Truncate not yet implemented in Rust worker');
  }

  /**
   * Cleanup and optimization - TODO: Implement in Rust worker
   */
  async cleanup(): Promise<void> {
    // Could trigger VACUUM or other maintenance in Rust worker
    // For now, no-op
  }

  /**
   * Explain query execution plan - TODO: Implement in Rust worker
   */
  async explainQuery(query: StorageQuery): Promise<QueryExplanation> {
    // Return mock explanation for now
    return {
      query,
      translatedQuery: 'EXPLAIN QUERY PLAN not yet implemented',
      adapterType: 'rust-worker',
      timestamp: new Date().toISOString()
    };
  }

  /**
   * Get adapter capabilities
   */
  getCapabilities(): StorageCapabilities {
    return {
      supportsTransactions: false, // TODO: Add transaction support
      supportsIndexing: true,
      supportsFullTextSearch: false, // TODO: Add FTS support
      supportsReplication: false,
      maxRecordSize: 10 * 1024 * 1024, // 10MB
      concurrentConnections: 10 // Rust worker connection pool size
    };
  }

  /**
   * Vector search using Rust data-daemon worker
   *
   * OPTIMIZED: Only the query vector (3KB for 384 dims) is sent to Rust.
   * Rust reads corpus vectors directly from SQLite (BLOB format) and computes
   * cosine similarity with rayon parallelism. Only top-k IDs and scores are
   * returned, then we fetch full records for those IDs.
   *
   * Process:
   * 1. Generate query embedding if text provided (uses EmbeddingService)
   * 2. Send query vector to Rust worker's vector/search command
   * 3. Rust reads vectors from SQLite, computes similarity in parallel
   * 4. Fetch full records for top-k IDs returned by Rust
   */
  async vectorSearch<T extends RecordData>(
    options: VectorSearchOptions
  ): Promise<StorageResult<VectorSearchResponse<T>>> {
    const startTime = Date.now();
    const collection = SqlNamingConverter.toTableName(options.collection);

    try {
      await this.ensureConnected();

      // 1. Get query vector
      let queryVector: VectorEmbedding;
      if (options.queryVector) {
        queryVector = options.queryVector;
      } else if (options.queryText) {
        // Generate embedding directly via Rust worker (fast, ~5ms)
        const client = RustEmbeddingClient.instance;
        if (!await client.isAvailable()) {
          return {
            success: false,
            error: 'Rust embedding worker not available'
          };
        }
        try {
          queryVector = await client.embed(options.queryText);
        } catch (error: any) {
          return {
            success: false,
            error: `Failed to generate query embedding: ${error.message}`
          };
        }
      } else {
        return {
          success: false,
          error: 'Must provide either queryText or queryVector'
        };
      }

      const k = options.k || 10;
      const threshold = options.similarityThreshold || 0.0;

      // 2. Send query vector to Rust worker with include_data=true
      // Rust reads corpus vectors from SQLite, computes similarity, AND fetches full records
      // This eliminates k IPC round trips - Rust returns everything in one response
      interface RustVectorSearchResponse {
        results: Array<{ id: string; score: number; distance: number; data?: Record<string, any> }>;
        count: number;
        corpus_size: number;
      }

      const searchResult = await this.sendCommand<RustVectorSearchResponse>('vector/search', {
        handle: this.adapterHandle,
        collection,
        query_vector: toNumberArray(queryVector),
        k,
        threshold,
        include_data: true  // OPTIMIZATION: Get full records in one Rust query
      });

      if (searchResult.status !== 'ok' || !searchResult.data) {
        // Fallback message for collections without embeddings
        if (searchResult.message?.includes('no such column: embedding')) {
          return {
            success: true,
            data: {
              results: [],
              totalResults: 0,
              queryVector,
              metadata: {
                collection: options.collection,
                searchMode: options.hybridMode || 'semantic',
                embeddingModel: options.embeddingModel?.name || 'unknown',
                queryTime: Date.now() - startTime
              }
            }
          };
        }
        return {
          success: false,
          error: searchResult.message || 'Vector search failed in Rust worker'
        };
      }

      const rustResults = searchResult.data.results;
      const corpusSize = searchResult.data.corpus_size;
      log.debug(`Vector search: Rust returned ${rustResults.length}/${corpusSize} results with inline data`);

      // 3. Map Rust results directly - no additional IPC round trips needed!
      // Rust already fetched full records with include_data=true
      type RustResult = { id: string; score: number; distance: number; data?: Record<string, any> };
      const results: VectorSearchResultType<T>[] = rustResults
        .filter((r: RustResult) => r.data) // Only include results that have data
        .map((rustResult: RustResult) => {
          // Convert snake_case keys from Rust/SQL to camelCase for TypeScript
          const entityData = this.toCamelCaseObject(rustResult.data!) as T;

          // Ensure id is present in entity data
          if (!(entityData as any).id) {
            (entityData as any).id = rustResult.id;
          }

          return {
            id: rustResult.id as UUID,
            data: entityData,
            score: rustResult.score,
            distance: rustResult.distance,
            metadata: {
              collection: options.collection,
              embeddingModel: options.embeddingModel?.name,
              queryTime: Date.now() - startTime
            }
          };
        });

      log.info(`Vector search: ${options.collection} found ${results.length}/${corpusSize} (threshold=${threshold}, k=${k})`);

      return {
        success: true,
        data: {
          results,
          totalResults: results.length,
          queryVector,
          metadata: {
            collection: options.collection,
            searchMode: options.hybridMode || 'semantic',
            embeddingModel: options.embeddingModel?.name || 'unknown',
            queryTime: Date.now() - startTime
          }
        }
      };
    } catch (error: any) {
      log.error(`Vector search failed: ${error.message}`);
      return {
        success: false,
        error: `Vector search failed: ${error.message}`
      };
    }
  }

  // =========================================================================
  // Blob Storage Methods - Content-addressable storage through Rust worker
  // =========================================================================

  /**
   * Store JSON data as compressed blob in content-addressable storage
   * @param data - JSON-serializable data to store
   * @param basePath - Optional custom blob storage path
   * @returns Blob reference with hash, size, compression info
   */
  async blobStore<T>(data: T, basePath?: string): Promise<{
    hash: string;
    size: number;
    compressedSize: number;
    deduplicated: boolean;
    storedAt: string;
  }> {
    const response = await this.sendCommand<{
      hash: string;
      size: number;
      compressedSize: number;
      deduplicated: boolean;
      storedAt: string;
    }>('blob/store', {
      data,
      base_path: basePath
    });

    if (response.status !== 'ok' || !response.data) {
      throw new Error(response.message || 'Blob store failed');
    }

    return response.data;
  }

  /**
   * Retrieve JSON data from blob by hash
   * @param hash - Blob hash (sha256:...)
   * @param basePath - Optional custom blob storage path
   * @returns Original JSON data
   */
  async blobRetrieve<T>(hash: string, basePath?: string): Promise<T> {
    const response = await this.sendCommand<T>('blob/retrieve', {
      hash,
      base_path: basePath
    });

    if (response.status !== 'ok') {
      throw new Error(response.message || 'Blob retrieve failed');
    }

    return response.data as T;
  }

  /**
   * Check if blob exists
   * @param hash - Blob hash (sha256:...)
   * @param basePath - Optional custom blob storage path
   */
  async blobExists(hash: string, basePath?: string): Promise<boolean> {
    const response = await this.sendCommand<{ exists: boolean }>('blob/exists', {
      hash,
      base_path: basePath
    });

    if (response.status !== 'ok') {
      throw new Error(response.message || 'Blob exists check failed');
    }

    return response.data?.exists ?? false;
  }

  /**
   * Delete blob by hash
   * @param hash - Blob hash (sha256:...)
   * @param basePath - Optional custom blob storage path
   * @returns true if deleted, false if not found
   */
  async blobDelete(hash: string, basePath?: string): Promise<boolean> {
    const response = await this.sendCommand<{ deleted: boolean }>('blob/delete', {
      hash,
      base_path: basePath
    });

    if (response.status !== 'ok') {
      throw new Error(response.message || 'Blob delete failed');
    }

    return response.data?.deleted ?? false;
  }

  /**
   * Get blob storage statistics
   * @param basePath - Optional custom blob storage path
   */
  async blobStats(basePath?: string): Promise<{
    totalBlobs: number;
    totalCompressedBytes: number;
    shardCount: number;
    basePath: string;
  }> {
    const response = await this.sendCommand<{
      totalBlobs: number;
      totalCompressedBytes: number;
      shardCount: number;
      basePath: string;
    }>('blob/stats', {
      base_path: basePath
    });

    if (response.status !== 'ok' || !response.data) {
      throw new Error(response.message || 'Blob stats failed');
    }

    return response.data;
  }

  /**
   * Store data as blob only if it exceeds threshold
   * @param data - Data to store
   * @param threshold - Size threshold in bytes (default: 4096)
   * @returns Either inline data or blob reference
   */
  async blobStoreIfLarge<T>(
    data: T,
    threshold: number = 4096
  ): Promise<{ isBlob: true; hash: string; size: number; compressedSize: number } | { isBlob: false; data: T }> {
    const json = JSON.stringify(data);
    const size = Buffer.byteLength(json, 'utf8');

    if (size < threshold) {
      return { isBlob: false, data };
    }

    const result = await this.blobStore(data);
    return {
      isBlob: true,
      hash: result.hash,
      size: result.size,
      compressedSize: result.compressedSize
    };
  }

  /**
   * Retrieve data that may be inline or in blob storage
   * @param inlineData - Data if stored inline
   * @param blobRef - Blob hash if stored externally
   */
  async blobRetrieveOrInline<T>(
    inlineData: T | null | undefined,
    blobRef: string | null | undefined
  ): Promise<T | null> {
    if (inlineData) {
      return inlineData;
    }

    if (blobRef) {
      return await this.blobRetrieve<T>(blobRef);
    }

    return null;
  }

  /**
   * Close connection to Rust worker
   */
  async close(): Promise<void> {
    // Close adapter in Rust first
    if (this.adapterHandle && this.socket) {
      try {
        await this.sendCommand('adapter/close', { handle: this.adapterHandle });
        console.log(`✅ Closed SQLite adapter: ${this.adapterHandle}`);
      } catch (error) {
        console.warn('⚠️  Failed to close adapter in Rust:', error);
      }
      this.adapterHandle = null;
    }

    // Close socket
    if (this.socket) {
      this.socket.destroy();
      this.socket = null;
    }

    // Reject pending response
    if (this.pendingResponse) {
      clearTimeout(this.pendingResponse.timeout);
      this.pendingResponse.reject(new Error('Connection closed'));
      this.pendingResponse = null;
    }
  }
}
