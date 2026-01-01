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
  type QueryExplanation
} from '../shared/DataStorageAdapter';

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
      timeout: options.timeout || 30000
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
        console.warn('⚠️  Rust worker connection closed');
        this.socket = null;
        // TODO: Implement reconnection logic
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
   */
  private async sendCommand<T = any>(command: string, params: Record<string, any> = {}): Promise<RustResponse & { data?: T }> {
    if (!this.socket) {
      throw new Error('Not connected to Rust worker');
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
   * Create record - delegates to Rust worker
   */
  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    if (!this.adapterHandle) {
      return { success: false, error: 'Adapter not initialized' };
    }

    try {
      const response = await this.sendCommand('data/create', {
        handle: this.adapterHandle,
        collection: record.collection,
        data: record.data
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
    if (!this.adapterHandle) {
      return { success: false, error: 'Adapter not initialized' };
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
    if (!this.adapterHandle) {
      return { success: false, error: 'Adapter not initialized' };
    }

    try {
      const response = await this.sendCommand<{ items: T[]; count: number }>('data/list', {
        handle: this.adapterHandle,
        collection: query.collection,
        filter: query.filter,
        order_by: query.sort?.map(s => ({ field: s.field, direction: s.direction })),
        limit: query.limit,
        offset: query.offset
      });

      if (response.status !== 'ok') {
        return { success: false, error: response.message || 'Query failed' };
      }

      const records: DataRecord<T>[] = (response.data?.items || []).map((item: any) => ({
        id: item.id,
        collection: query.collection,
        data: item,
        metadata: {
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          version: 1
        }
      }));

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
   * Update record - not yet implemented in Rust worker
   */
  async update<T extends RecordData>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    incrementVersion?: boolean
  ): Promise<StorageResult<DataRecord<T>>> {
    // TODO: Implement data/update in Rust worker
    return { success: false, error: 'Update not yet implemented in Rust worker' };
  }

  /**
   * Delete record
   */
  async delete(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    if (!this.adapterHandle) {
      return { success: false, error: 'Adapter not initialized' };
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
  async ensureSchema(collection: string, schema?: unknown): Promise<StorageResult<boolean>> {
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
