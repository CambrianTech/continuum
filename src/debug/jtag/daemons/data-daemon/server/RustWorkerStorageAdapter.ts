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
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { v4 as uuidv4 } from 'uuid';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { getDatabasePath } from '../../../system/config/ServerConfig';
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
 */
export interface RustWorkerConfig {
  /** Path to Unix socket */
  socketPath: string;
  /** Optional database path (if not using default) */
  dbPath?: string;
  /** Database handle from registry */
  dbHandle?: string;
  /** Connection timeout in ms */
  timeout?: number;
}

/**
 * JTAG protocol message format (matches Rust JTAGRequest/JTAGResponse)
 */
interface JTAGRequest<T = any> {
  id: string;
  type: string;
  timestamp: string;
  payload: T;
}

interface JTAGResponse<T = any> {
  id: string;
  type: string;
  timestamp: string;
  payload: T;
  requestId: string;
  success: boolean;
  error?: string;
  errorType?: string;
}

/**
 * Rust Worker Storage Adapter - Fast concurrent storage via Rust process
 */
export class RustWorkerStorageAdapter extends DataStorageAdapter {
  private config!: RustWorkerConfig;
  private socket: net.Socket | null = null;
  private pendingRequests: Map<string, {
    resolve: (value: any) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout;
  }> = new Map();

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
   */
  async initialize(config: StorageAdapterConfig): Promise<void> {
    // Merge initialization config with constructor config
    this.config = {
      socketPath: this.config?.socketPath || '/tmp/data-worker.sock',
      dbPath: (config.options as any)?.dbPath || getDatabasePath(),
      dbHandle: (config.options as any)?.dbHandle || 'default',
      timeout: this.config?.timeout || 30000
    };

    await this.connect();
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
        const response: JTAGResponse = JSON.parse(line);
        const pending = this.pendingRequests.get(response.requestId);

        if (pending) {
          clearTimeout(pending.timeout);
          this.pendingRequests.delete(response.requestId);

          if (response.success) {
            pending.resolve(response.payload);
          } else {
            pending.reject(new Error(response.error || 'Unknown error'));
          }
        }
      } catch (error) {
        console.error('Failed to parse response from Rust worker:', error);
      }
    }
  }

  /**
   * Send message to Rust worker and wait for response
   */
  private async sendMessage<T>(type: string, payload: any): Promise<T> {
    if (!this.socket) {
      throw new Error('Not connected to Rust worker');
    }

    const requestId = uuidv4();
    const request: JTAGRequest = {
      id: requestId,
      type,
      timestamp: new Date().toISOString(),
      payload: {
        ...payload,
        dbHandle: this.config.dbHandle,
        dbPath: this.config.dbPath
      }
    };

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pendingRequests.delete(requestId);
        reject(new Error(`Request timeout: ${type}`));
      }, this.config.timeout);

      this.pendingRequests.set(requestId, { resolve, reject, timeout });

      // Send newline-delimited JSON
      this.socket!.write(JSON.stringify(request) + '\n');
    });
  }

  /**
   * Create record - delegates to Rust worker
   */
  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    try {
      // Send raw entity data to Rust (it doesn't need full DataRecord structure)
      const response = await this.sendMessage<{ data: T }>(DATA_COMMANDS.CREATE, {
        collection: record.collection,
        document: record.data
      });

      // Rust returns raw entity, wrap back in DataRecord with metadata
      return {
        success: true,
        data: {
          id: record.id,
          collection: record.collection,
          data: response.data,
          metadata: record.metadata
        }
      };
    } catch (error: any) {
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Read single record by ID
   */
  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    try {
      const response = await this.sendMessage<{ data: T | null }>(DATA_COMMANDS.READ, {
        collection,
        id
      });

      if (!response.data) {
        return {
          success: false,
          error: 'Record not found'
        };
      }

      // Wrap raw entity in DataRecord structure
      // Note: Rust doesn't return metadata separately yet, so we create default metadata
      return {
        success: true,
        data: {
          id,
          collection,
          data: response.data,
          metadata: {
            createdAt: new Date().toISOString(), // TODO: Get from Rust
            updatedAt: new Date().toISOString(),
            version: 1
          }
        }
      };
    } catch (error: any) {
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Query records with filters
   */
  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    try {
      const response = await this.sendMessage<{
        items: T[];
        total: number;
      }>(DATA_COMMANDS.LIST, {
        collection: query.collection,
        filter: query.filter,
        orderBy: query.sort,
        limit: query.limit,
        offset: query.offset
      });

      // Wrap raw entities in DataRecord structures
      const records: DataRecord<T>[] = response.items.map((item: any) => ({
        id: item.id,
        collection: query.collection,
        data: item,
        metadata: {
          createdAt: new Date().toISOString(), // TODO: Get from Rust
          updatedAt: new Date().toISOString(),
          version: 1
        }
      }));

      return {
        success: true,
        data: records,
        metadata: {
          totalCount: response.total
        }
      };
    } catch (error: any) {
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Update record
   */
  async update<T extends RecordData>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    incrementVersion?: boolean
  ): Promise<StorageResult<DataRecord<T>>> {
    try {
      const response = await this.sendMessage<{ data: T }>(DATA_COMMANDS.UPDATE, {
        collection,
        id,
        updates: data
      });

      return {
        success: true,
        data: {
          id,
          collection,
          data: response.data,
          metadata: {
            createdAt: new Date().toISOString(), // TODO: Get from Rust
            updatedAt: new Date().toISOString(),
            version: incrementVersion ? 2 : 1 // TODO: Track properly
          }
        }
      };
    } catch (error: any) {
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Delete record - TODO: Implement in Rust worker first
   */
  async delete(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    throw new Error('Delete not yet implemented in Rust worker');
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
    if (this.socket) {
      this.socket.destroy();
      this.socket = null;
    }

    // Reject all pending requests
    for (const [id, pending] of this.pendingRequests) {
      clearTimeout(pending.timeout);
      pending.reject(new Error('Connection closed'));
    }
    this.pendingRequests.clear();
  }
}
