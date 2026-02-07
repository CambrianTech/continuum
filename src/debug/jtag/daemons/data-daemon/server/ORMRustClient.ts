/**
 * ORM Rust Client - IPC bridge to continuum-core DataModule
 *
 * Single-purpose client for data/* commands to the Rust continuum-core process.
 * Uses the same IPC protocol as RustCoreIPCClient but focused on ORM operations.
 *
 * ARCHITECTURE:
 * - TypeScript ORM.ts delegates to this client when shouldUseRust() returns true
 * - This client sends JSON requests to /tmp/continuum-core.sock
 * - Rust DataModule handles all database I/O with connection pooling
 * - NO FALLBACKS: If Rust fails, we fail. Period.
 *
 * CRITICAL: dbPath is REQUIRED for all operations - no defaults.
 */

import net from 'net';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../system/data/entities/BaseEntity';
import type {
  DataRecord,
  StorageQuery,
  StorageResult,
  StorageOperation,
  RecordData,
} from '../shared/DataStorageAdapter';
import { getServerConfig } from '../../../system/config/ServerConfig';
// NOTE: No SqlNamingConverter import - Rust SqliteAdapter handles all naming conversions

// Socket path for continuum-core
const SOCKET_PATH = '/tmp/continuum-core.sock';

/**
 * Rust StorageResult<T> - matches orm/types.rs StorageResult
 */
interface RustStorageResult<T> {
  success: boolean;
  data?: T;
  error?: string;
}

/**
 * Rust DataRecord - matches orm/types.rs DataRecord
 */
interface RustDataRecord {
  id: string;
  collection: string;
  data: Record<string, unknown>;
  metadata: {
    created_at: string;
    updated_at: string;
    version: number;
    tags?: string[];
    schema?: string;
    ttl?: number;
  };
}

/**
 * IPC Response wrapper - adds requestId for multiplexing
 */
interface RustIPCResponse<T = unknown> {
  success: boolean;
  result?: RustStorageResult<T>;
  error?: string;
  requestId?: number;
}

/**
 * ORMRustClient - Singleton IPC client for data operations
 */
export class ORMRustClient {
  private static instance: ORMRustClient | null = null;
  private socket: net.Socket | null = null;
  private buffer: Buffer = Buffer.alloc(0);
  private pendingRequests: Map<number, (result: RustIPCResponse<unknown>) => void> = new Map();
  private nextRequestId = 1;
  private connected = false;
  private connecting = false;
  private dbPath: string;

  private constructor() {
    // Get database path from config - REQUIRED, no fallback
    this.dbPath = getServerConfig().getDatabasePath();
  }

  /**
   * Get singleton instance
   */
  static getInstance(): ORMRustClient {
    if (!ORMRustClient.instance) {
      ORMRustClient.instance = new ORMRustClient();
    }
    return ORMRustClient.instance;
  }

  /**
   * Ensure connected to continuum-core
   */
  private async ensureConnected(): Promise<void> {
    if (this.connected) return;
    if (this.connecting) {
      // Wait for connection in progress
      await new Promise<void>((resolve, reject) => {
        const check = setInterval(() => {
          if (this.connected) {
            clearInterval(check);
            resolve();
          } else if (!this.connecting) {
            clearInterval(check);
            reject(new Error('Connection failed'));
          }
        }, 10);
      });
      return;
    }

    this.connecting = true;

    return new Promise((resolve, reject) => {
      this.socket = net.createConnection(SOCKET_PATH);

      this.socket.on('connect', () => {
        this.connected = true;
        this.connecting = false;
        console.log('[ORMRustClient] Connected to continuum-core');
        resolve();
      });

      this.socket.on('data', (data: Buffer) => {
        this.onData(data);
      });

      this.socket.on('error', (err) => {
        this.connecting = false;
        reject(err);
      });

      this.socket.on('close', () => {
        this.connected = false;
        this.connecting = false;
        this.socket = null;
      });

      // Connection timeout
      setTimeout(() => {
        if (!this.connected) {
          this.connecting = false;
          reject(new Error(`Connection timeout to ${SOCKET_PATH}`));
        }
      }, 5000);
    });
  }

  /**
   * Process incoming binary data with length-prefixed framing
   */
  private onData(data: Buffer): void {
    this.buffer = Buffer.concat([this.buffer, data]);

    while (this.buffer.length >= 4) {
      const totalLength = this.buffer.readUInt32BE(0);
      const frameEnd = 4 + totalLength;

      if (this.buffer.length < frameEnd) break;

      const payload = this.buffer.subarray(4, frameEnd);
      this.buffer = this.buffer.subarray(frameEnd);

      // Find null separator for binary data
      const separatorIndex = payload.indexOf(0);
      const jsonBytes = separatorIndex !== -1
        ? payload.subarray(0, separatorIndex)
        : payload;

      try {
        const response = JSON.parse(jsonBytes.toString('utf8')) as RustIPCResponse;
        this.handleResponse(response);
      } catch (e) {
        console.error('[ORMRustClient] Failed to parse response:', e);
      }
    }
  }

  private handleResponse(response: RustIPCResponse): void {
    if (response.requestId !== undefined) {
      const callback = this.pendingRequests.get(response.requestId);
      if (callback) {
        callback(response);
        this.pendingRequests.delete(response.requestId);
      }
    }
  }

  /**
   * Send request to Rust and wait for response
   */
  private async request<T>(command: Record<string, unknown>): Promise<RustIPCResponse<T>> {
    await this.ensureConnected();

    if (!this.socket) {
      throw new Error('Not connected to continuum-core');
    }

    const requestId = this.nextRequestId++;
    const requestWithId = { ...command, requestId };

    return new Promise((resolve, reject) => {
      const json = JSON.stringify(requestWithId) + '\n';

      this.pendingRequests.set(requestId, (result) => {
        resolve(result as RustIPCResponse<T>);
      });

      this.socket!.write(json, (err) => {
        if (err) {
          this.pendingRequests.delete(requestId);
          reject(err);
        }
      });

      // Timeout after 30 seconds
      setTimeout(() => {
        if (this.pendingRequests.has(requestId)) {
          this.pendingRequests.delete(requestId);
          reject(new Error(`Request timeout: ${command.command}`));
        }
      }, 30000);
    });
  }

  // ─── CRUD Operations ────────────────────────────────────────────────────────

  /**
   * Store entity
   * NOTE: Passes camelCase data and collection names - Rust SqliteAdapter handles conversion
   */
  async store<T extends BaseEntity>(
    collection: string,
    data: T
  ): Promise<StorageResult<T>> {
    // Pass data as-is - Rust SqliteAdapter converts camelCase to snake_case
    const response = await this.request<RustDataRecord>({
      command: 'data/create',
      dbPath: this.dbPath,
      collection,  // Rust converts to snake_case table name
      id: data.id,  // BaseEntity guarantees id field
      data,         // Rust converts field names to snake_case
    });

    if (!response.success) {
      console.error('[ORMRustClient.store] Store failed:', response.error);
      return { success: false, error: response.error || 'Store failed' };
    }

    return { success: true, data };
  }

  /**
   * Query entities
   * NOTE: Passes camelCase - Rust SqliteAdapter handles all naming conversion
   */
  async query<T extends BaseEntity>(
    query: StorageQuery
  ): Promise<StorageResult<DataRecord<T>[]>> {
    // Convert filter from TypeScript $eq/$gt format to Rust eq/gt format
    const rustFilter = this.convertFilterForRust(query.filter as Record<string, unknown> | undefined);

    const response = await this.request<RustDataRecord[]>({
      command: 'data/query',
      dbPath: this.dbPath,
      collection: query.collection,  // Rust converts to snake_case table name
      filter: rustFilter,             // Converted to Rust format
      sort: query.sort,               // Rust converts sort field names
      limit: query.limit,
      offset: query.offset,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Query failed' };
    }

    // Rust returns: { result: { data: [...records...], success: true } }
    const rustResult = response.result;
    const rawRecords: RustDataRecord[] = rustResult?.data ?? [];

    const records: DataRecord<T>[] = rawRecords.map((item: RustDataRecord) => {
      let entityData: T;

      if (typeof item.data === 'string') {
        entityData = JSON.parse(item.data) as T;
      } else if (item.data && typeof item.data === 'object') {
        entityData = item.data as T;
      } else {
        // Extract entity data from flattened record
        const { id: _id, created_at: _ca, updated_at: _ua, version: _v, collection: _c, metadata: _m, ...rest } = item as unknown as Record<string, unknown>;
        entityData = this.toCamelCaseObject(rest) as T;
      }

      // Ensure id is set on entity data
      if (!entityData.id) {
        (entityData as BaseEntity).id = item.id as UUID;
      }

      return {
        id: item.id,
        collection: query.collection,
        data: entityData,
        metadata: {
          createdAt: item.metadata?.created_at || new Date().toISOString(),
          updatedAt: item.metadata?.updated_at || new Date().toISOString(),
          version: item.metadata?.version || 1,
        },
      };
    });

    return {
      success: true,
      data: records,
      metadata: { totalCount: records.length },
    };
  }

  /**
   * Count entities
   * NOTE: Passes camelCase - Rust SqliteAdapter handles all naming conversion
   */
  async count(query: StorageQuery): Promise<StorageResult<number>> {
    // Convert filter from TypeScript $eq/$gt format to Rust eq/gt format
    const rustFilter = this.convertFilterForRust(query.filter as Record<string, unknown> | undefined);

    const response = await this.request<number>({
      command: 'data/count',
      dbPath: this.dbPath,
      collection: query.collection,  // Rust converts to snake_case
      filter: rustFilter,             // Converted to Rust format
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Count failed' };
    }

    // Rust returns: { result: { data: number, success: true } }
    const count = response.result?.data ?? 0;
    return { success: true, data: count };
  }

  /**
   * Read single entity
   * NOTE: Passes camelCase - Rust SqliteAdapter handles all naming conversion
   */
  async read<T extends BaseEntity>(
    collection: string,
    id: UUID
  ): Promise<T | null> {
    const response = await this.request<RustDataRecord>({
      command: 'data/read',
      dbPath: this.dbPath,
      collection,  // Rust converts to snake_case table name
      id,
    });

    if (!response.success || !response.result?.data) {
      return null;
    }

    const item = response.result.data;
    let entityData: T;

    if (typeof item.data === 'string') {
      entityData = JSON.parse(item.data) as T;
    } else if (item.data && typeof item.data === 'object') {
      entityData = item.data as T;
    } else {
      // Extract entity data from flattened record
      const { id: _id, created_at: _ca, updated_at: _ua, version: _v, ...rest } = item as unknown as Record<string, unknown>;
      entityData = this.toCamelCaseObject(rest) as T;
    }

    // Ensure id is set on entity data
    if (!entityData.id) {
      (entityData as BaseEntity).id = id;
    }

    return entityData;
  }

  /**
   * Update entity
   * NOTE: Passes camelCase - Rust SqliteAdapter handles all naming conversion
   */
  async update<T extends BaseEntity>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    incrementVersion: boolean = true
  ): Promise<T> {
    const response = await this.request<RustDataRecord>({
      command: 'data/update',
      dbPath: this.dbPath,
      collection,  // Rust converts to snake_case table name
      id,
      data,        // Rust converts field names to snake_case
      incrementVersion,
    });

    if (!response.success) {
      throw new Error(response.error || 'Update failed');
    }

    return { id, ...data } as T;
  }

  /**
   * Remove entity
   * NOTE: Passes camelCase - Rust SqliteAdapter handles all naming conversion
   */
  async remove(
    collection: string,
    id: UUID
  ): Promise<StorageResult<boolean>> {
    const response = await this.request<boolean>({
      command: 'data/delete',
      dbPath: this.dbPath,
      collection,  // Rust converts to snake_case table name
      id,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Delete failed' };
    }

    return { success: true, data: true };
  }

  /**
   * Batch operations
   * NOTE: Passes camelCase - Rust SqliteAdapter handles all naming conversion
   */
  async batch(operations: StorageOperation[]): Promise<StorageResult<unknown[]>> {
    // Pass operations as-is - Rust converts collection and field names
    const rustOps = operations.map(op => ({
      type: op.type,
      collection: op.collection,  // Rust converts to snake_case
      id: op.id,
      data: op.data,              // Rust converts field names
    }));

    const response = await this.request<unknown[]>({
      command: 'data/batch',
      dbPath: this.dbPath,
      operations: rustOps,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Batch failed' };
    }

    return { success: true, data: response.result?.data ?? [] };
  }

  /**
   * List collections
   */
  async listCollections(): Promise<StorageResult<string[]>> {
    const response = await this.request<string[]>({
      command: 'data/list-collections',
      dbPath: this.dbPath,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'List collections failed' };
    }

    return { success: true, data: response.result?.data ?? [] };
  }

  /**
   * Clear all data
   */
  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    interface ClearAllResult {
      tables_cleared: string[];
      records_deleted: number;
    }

    const response = await this.request<ClearAllResult>({
      command: 'data/clear-all',
      dbPath: this.dbPath,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Clear all failed' };
    }

    const result = response.result?.data;
    return {
      success: true,
      data: {
        tablesCleared: result?.tables_cleared ?? [],
        recordsDeleted: result?.records_deleted ?? 0,
      },
    };
  }

  /**
   * Truncate collection
   * NOTE: Passes camelCase - Rust SqliteAdapter handles all naming conversion
   */
  async truncate(collection: string): Promise<StorageResult<boolean>> {
    const response = await this.request<boolean>({
      command: 'data/truncate',
      dbPath: this.dbPath,
      collection,  // Rust converts to snake_case table name
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Truncate failed' };
    }

    return { success: true, data: true };
  }

  // ─── Filter Conversion ──────────────────────────────────────────────────────
  // TypeScript uses $eq, $gt etc. Rust uses eq, gt etc.

  /**
   * Convert TypeScript filter format to Rust format
   * TypeScript: { roomId: "abc", timestamp: { $gte: "2024-01-01" } }
   * Rust: { roomId: "abc", timestamp: { gte: "2024-01-01" } }
   */
  private convertFilterForRust(filter: Record<string, unknown> | undefined): Record<string, unknown> | undefined {
    if (!filter) return undefined;

    const result: Record<string, unknown> = {};
    for (const [field, value] of Object.entries(filter)) {
      if (value === null || typeof value !== 'object') {
        // Direct value - pass through
        result[field] = value;
      } else if (this.isOperatorObject(value)) {
        // Operator object - convert $eq → eq, $gt → gt, etc.
        result[field] = this.convertOperatorObject(value as Record<string, unknown>);
      } else {
        // Nested object - pass through
        result[field] = value;
      }
    }
    return result;
  }

  /**
   * Check if value is an operator object (has $eq, $gt, etc.)
   */
  private isOperatorObject(value: unknown): boolean {
    if (typeof value !== 'object' || value === null) return false;
    const obj = value as Record<string, unknown>;
    return Object.keys(obj).some(k => k.startsWith('$'));
  }

  /**
   * Convert operator object from TypeScript to Rust format
   * { $eq: "abc" } → { eq: "abc" }
   * { $gte: "2024-01-01" } → { gte: "2024-01-01" }
   */
  private convertOperatorObject(obj: Record<string, unknown>): Record<string, unknown> {
    const operatorMap: Record<string, string> = {
      '$eq': 'eq',
      '$ne': 'ne',
      '$gt': 'gt',
      '$gte': 'gte',
      '$lt': 'lt',
      '$lte': 'lte',
      '$in': 'in',
      '$nin': 'notIn',
      '$exists': 'exists',
      '$regex': 'regex',
      '$contains': 'contains',
    };

    const result: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj)) {
      const rustKey = operatorMap[key] ?? key.replace(/^\$/, '');
      result[rustKey] = value;
    }
    return result;
  }

  // ─── Case Conversion Helpers ────────────────────────────────────────────────
  // NOTE: Only used for Rust response parsing (Rust returns snake_case, we need camelCase)

  /**
   * Convert snake_case object keys to camelCase for TypeScript consumption
   */
  private toCamelCaseObject(obj: Record<string, unknown>): Record<string, unknown> {
    const result: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj)) {
      const camelKey = this.snakeToCamel(key);
      result[camelKey] = this.hydrateValue(value);
    }
    return result;
  }

  /**
   * Convert snake_case string to camelCase
   */
  private snakeToCamel(s: string): string {
    return s.replace(/_([a-z])/g, (_, char) => char.toUpperCase());
  }

  /**
   * Parse JSON strings that were stored as text in SQLite
   */
  private hydrateValue(value: unknown): unknown {
    if (typeof value !== 'string') return value;
    const trimmed = value.trim();
    if (
      (trimmed.startsWith('{') && trimmed.endsWith('}')) ||
      (trimmed.startsWith('[') && trimmed.endsWith(']'))
    ) {
      try {
        return JSON.parse(trimmed);
      } catch {
        return value;
      }
    }
    return value;
  }

  /**
   * Close connection
   */
  disconnect(): void {
    if (this.socket) {
      this.socket.end();
      this.socket = null;
      this.connected = false;
    }
    ORMRustClient.instance = null;
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.connected;
  }
}
