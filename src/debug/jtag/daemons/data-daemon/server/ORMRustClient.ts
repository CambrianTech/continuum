/**
 * ORM Rust Client - IPC bridge to continuum-core DataModule
 *
 * Single-purpose client for data/* commands to the Rust continuum-core process.
 * Uses the same IPC protocol as RustCoreIPCClient but focused on ORM operations.
 *
 * ARCHITECTURE:
 * - TypeScript ORM.ts delegates to this client when shouldUseRust() returns true
 * - This client sends JSON requests to continuum-core socket (from shared/config.ts)
 * - Rust DataModule handles all database I/O with connection pooling
 * - NO FALLBACKS: If Rust fails, we fail. Period.
 *
 * CRITICAL: dbPath is REQUIRED for all operations - no defaults.
 */

import net from 'net';
import path from 'path';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../system/data/entities/BaseEntity';
import type {
  DataRecord,
  StorageQuery,
  StorageResult,
  StorageOperation,
  RecordData,
  JoinSpec,
} from '../shared/DataStorageAdapter';
import type { VectorSearchResult } from '../shared/VectorSearchTypes';
import { SOCKETS } from '../../../shared/config';

// Input type for joins (allows optional properties)
type JoinSpecInput = Partial<JoinSpec> & Pick<JoinSpec, 'collection' | 'alias' | 'localField' | 'foreignField'>;
import { getServerConfig } from '../../../system/config/ServerConfig';
// NOTE: No SqlNamingConverter import - Rust SqliteAdapter handles all naming conversions

// Socket path for continuum-core - resolved from config
const SOCKET_PATH = path.isAbsolute(SOCKETS.CONTINUUM_CORE)
  ? SOCKETS.CONTINUUM_CORE
  : path.resolve(process.cwd(), SOCKETS.CONTINUUM_CORE);

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
 * Timing info for IPC performance analysis
 */
interface IPCTiming {
  requestId: number;
  command: string;
  sendTime: number;      // hrtime when request sent
  stringifyMs: number;   // JSON.stringify duration
  writeMs: number;       // Socket write duration
}

/**
 * ORMRustClient - Singleton IPC client for data operations
 */
export class ORMRustClient {
  private static instance: ORMRustClient | null = null;
  private socket: net.Socket | null = null;
  private buffer: Buffer = Buffer.alloc(0);
  private pendingRequests: Map<number, (result: RustIPCResponse<unknown>) => void> = new Map();
  private pendingTimings: Map<number, IPCTiming> = new Map();
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
        const jsonStr = jsonBytes.toString('utf8');
        const parseStart = Date.now();
        const response = JSON.parse(jsonStr) as RustIPCResponse;
        const parseMs = Date.now() - parseStart;
        if (!response.success) {
          console.error(`[ORMRustClient] ERROR response: ${response.error}`);
        }
        this.handleResponse(response, parseMs);
      } catch (e) {
        console.error('[ORMRustClient] Failed to parse response:', e, 'raw:', jsonBytes.toString('utf8').substring(0, 200));
      }
    }
  }

  private handleResponse(response: RustIPCResponse, parseMs: number): void {
    if (response.requestId !== undefined) {
      const callback = this.pendingRequests.get(response.requestId);
      const timing = this.pendingTimings.get(response.requestId);

      if (callback) {
        callback(response);
        this.pendingRequests.delete(response.requestId);
      }

      if (timing) {
        const totalMs = Date.now() - timing.sendTime;
        const networkAndRustMs = totalMs - timing.stringifyMs - timing.writeMs - parseMs;
        this.pendingTimings.delete(response.requestId);

        // Log slow operations (>50ms threshold matches Rust)
        if (totalMs > 50) {
          console.warn(`[ORMRustClient] SLOW IPC: ${timing.command} total=${totalMs}ms (stringify=${timing.stringifyMs}ms write=${timing.writeMs}ms network+rust=${networkAndRustMs}ms parse=${parseMs}ms)`);
        }
      }
    }
  }

  /**
   * Send request to Rust and wait for response
   * Includes timing instrumentation to identify IPC bottlenecks
   */
  private async request<T>(command: Record<string, unknown>): Promise<RustIPCResponse<T>> {
    const connectStart = Date.now();
    await this.ensureConnected();
    const connectMs = Date.now() - connectStart;

    if (!this.socket) {
      throw new Error('Not connected to continuum-core');
    }

    const requestId = this.nextRequestId++;
    const requestWithId = { ...command, requestId };
    const cmdName = command.command as string;

    // Time JSON.stringify
    const stringifyStart = Date.now();
    const json = JSON.stringify(requestWithId) + '\n';
    const stringifyMs = Date.now() - stringifyStart;

    return new Promise((resolve, reject) => {
      // Track timing for this request
      const timing: IPCTiming = {
        requestId,
        command: cmdName,
        sendTime: Date.now(),
        stringifyMs,
        writeMs: 0,
      };

      this.pendingTimings.set(requestId, timing);

      this.pendingRequests.set(requestId, (result) => {
        resolve(result as RustIPCResponse<T>);
      });

      // Time socket write
      const writeStart = Date.now();
      this.socket!.write(json, (err) => {
        timing.writeMs = Date.now() - writeStart;

        if (err) {
          console.error(`[ORMRustClient] Write error for ${cmdName}:`, err);
          this.pendingRequests.delete(requestId);
          this.pendingTimings.delete(requestId);
          reject(err);
        }

        // Log slow connect/stringify/write (these should be <1ms each)
        if (connectMs > 5 || stringifyMs > 5 || timing.writeMs > 5) {
          console.warn(`[ORMRustClient] IPC overhead: ${cmdName} connect=${connectMs}ms stringify=${stringifyMs}ms write=${timing.writeMs}ms`);
        }
      });

      // Timeout after 30 seconds
      setTimeout(() => {
        if (this.pendingRequests.has(requestId)) {
          console.error(`[ORMRustClient] TIMEOUT for ${cmdName} (id=${requestId})`);
          this.pendingRequests.delete(requestId);
          this.pendingTimings.delete(requestId);
          reject(new Error(`Request timeout: ${cmdName}`));
        }
      }, 30000);
    });
  }

  // ─── CRUD Operations ────────────────────────────────────────────────────────

  /**
   * Store entity
   * NOTE: Passes camelCase data and collection names - Rust SqliteAdapter handles conversion
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  async store<T extends BaseEntity>(
    collection: string,
    data: T,
    dbPath?: string
  ): Promise<StorageResult<T>> {
    // Pass data as-is - Rust SqliteAdapter converts camelCase to snake_case
    const response = await this.request<RustDataRecord>({
      command: 'data/create',
      dbPath: dbPath ?? this.dbPath,
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
   * NOTE: Filter passed directly - Rust now accepts $eq/$gt format (MongoDB-style)
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  async query<T extends BaseEntity>(
    query: StorageQuery,
    dbPath?: string
  ): Promise<StorageResult<DataRecord<T>[]>> {
    const response = await this.request<RustDataRecord[]>({
      command: 'data/query',
      dbPath: dbPath ?? this.dbPath,
      collection: query.collection,  // Rust converts to snake_case table name
      filter: query.filter,          // Rust accepts $eq/$gt format directly
      sort: query.sort,              // Rust converts sort field names
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
   * Query entities with JOINs
   * NOTE: Passes camelCase - Rust SqliteAdapter handles all naming conversion
   * NOTE: Filter passed directly - Rust now accepts $eq/$gt format (MongoDB-style)
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  async queryWithJoin<T extends BaseEntity>(
    query: StorageQuery & { joins?: readonly JoinSpecInput[] },
    dbPath?: string
  ): Promise<StorageResult<DataRecord<T>[]>> {
    const response = await this.request<RustDataRecord[]>({
      command: 'data/queryWithJoin',
      dbPath: dbPath ?? this.dbPath,
      collection: query.collection,
      filter: query.filter,
      sort: query.sort,
      limit: query.limit,
      offset: query.offset,
      joins: query.joins,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Query with join failed' };
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
        const { id: _id, created_at: _ca, updated_at: _ua, version: _v, collection: _c, metadata: _m, ...rest } = item as unknown as Record<string, unknown>;
        entityData = this.toCamelCaseObject(rest) as T;
      }

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
   * NOTE: Filter passed directly - Rust now accepts $eq/$gt format (MongoDB-style)
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  async count(query: StorageQuery, dbPath?: string): Promise<StorageResult<number>> {
    const response = await this.request<number>({
      command: 'data/count',
      dbPath: dbPath ?? this.dbPath,
      collection: query.collection,  // Rust converts to snake_case
      filter: query.filter,          // Rust accepts $eq/$gt format directly
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
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  async read<T extends BaseEntity>(
    collection: string,
    id: UUID,
    dbPath?: string
  ): Promise<T | null> {
    const response = await this.request<RustDataRecord>({
      command: 'data/read',
      dbPath: dbPath ?? this.dbPath,
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
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  async update<T extends BaseEntity>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    incrementVersion: boolean = true,
    dbPath?: string
  ): Promise<T> {
    const response = await this.request<RustDataRecord>({
      command: 'data/update',
      dbPath: dbPath ?? this.dbPath,
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
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  async remove(
    collection: string,
    id: UUID,
    dbPath?: string
  ): Promise<StorageResult<boolean>> {
    const response = await this.request<boolean>({
      command: 'data/delete',
      dbPath: dbPath ?? this.dbPath,
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
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  async batch(operations: StorageOperation[], dbPath?: string): Promise<StorageResult<unknown[]>> {
    // Pass operations as-is - Rust converts collection and field names
    const rustOps = operations.map(op => ({
      type: op.type,
      collection: op.collection,  // Rust converts to snake_case
      id: op.id,
      data: op.data,              // Rust converts field names
    }));

    const response = await this.request<unknown[]>({
      command: 'data/batch',
      dbPath: dbPath ?? this.dbPath,
      operations: rustOps,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Batch failed' };
    }

    return { success: true, data: response.result?.data ?? [] };
  }

  /**
   * List collections
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  async listCollections(dbPath?: string): Promise<StorageResult<string[]>> {
    const response = await this.request<string[]>({
      command: 'data/list-collections',
      dbPath: dbPath ?? this.dbPath,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'List collections failed' };
    }

    return { success: true, data: response.result?.data ?? [] };
  }

  /**
   * Clear all data
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  async clearAll(dbPath?: string): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    interface ClearAllResult {
      tables_cleared: string[];
      records_deleted: number;
    }

    const response = await this.request<ClearAllResult>({
      command: 'data/clear-all',
      dbPath: dbPath ?? this.dbPath,
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
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  async truncate(collection: string, dbPath?: string): Promise<StorageResult<boolean>> {
    const response = await this.request<boolean>({
      command: 'data/truncate',
      dbPath: dbPath ?? this.dbPath,
      collection,  // Rust converts to snake_case table name
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Truncate failed' };
    }

    return { success: true, data: true };
  }

  // ─── Vector Search ─────────────────────────────────────────────────────────

  /**
   * Vector similarity search via Rust DataModule
   *
   * NOTE: Requires pre-computed query vector. Rust does NOT generate embeddings.
   * Use EmbeddingModule (embedding/generate) to get the query vector first.
   *
   * Rust advantages over TypeScript:
   * - In-memory vector caching (no re-query on repeated searches)
   * - Rayon parallel cosine similarity (multi-threaded)
   * - SIMD-like loop unrolling for fast distance computation
   *
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  async vectorSearch<T extends RecordData>(
    collection: string,
    queryVector: number[],
    options?: {
      k?: number;
      threshold?: number;
      includeData?: boolean;
      dbPath?: string;
    }
  ): Promise<StorageResult<VectorSearchResult<T>[]>> {
    interface RustVectorResult {
      results: Array<{
        id: string;
        score: number;
        distance: number;
        data?: Record<string, unknown>;
      }>;
      count: number;
      corpusSize: number;
    }

    const response = await this.request<RustVectorResult>({
      command: 'vector/search',
      dbPath: options?.dbPath ?? this.dbPath,
      collection,
      queryVector,
      k: options?.k ?? 10,
      threshold: options?.threshold ?? 0.0,
      includeData: options?.includeData ?? true,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Vector search failed' };
    }

    const rustResult = response.result?.data;
    if (!rustResult) {
      return { success: true, data: [] };
    }

    // Convert Rust results to TypeScript VectorSearchResult format
    const results: VectorSearchResult<T>[] = rustResult.results.map((r) => ({
      id: r.id as UUID,
      data: (r.data ? this.toCamelCaseObject(r.data) : {}) as T,
      score: r.score,
      distance: r.distance,
    }));

    return { success: true, data: results };
  }

  /**
   * Index vector for a record
   *
   * Stores the embedding in the record's 'embedding' field.
   * Also invalidates the vector cache for this collection.
   *
   * @param collection - Collection name
   * @param id - Record ID
   * @param embedding - Vector embedding to store
   * @param dbPath - Optional database path for per-persona databases
   */
  async indexVector(
    collection: string,
    id: UUID,
    embedding: number[],
    dbPath?: string
  ): Promise<StorageResult<boolean>> {
    const response = await this.request<{ success: boolean }>({
      command: 'vector/index',
      dbPath: dbPath ?? this.dbPath,
      collection,
      id,
      embedding,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Index vector failed' };
    }

    return { success: true, data: true };
  }

  /**
   * Get vector index statistics for a collection
   *
   * @param collection - Collection name
   * @param dbPath - Optional database path for per-persona databases
   */
  async getVectorIndexStats(
    collection: string,
    dbPath?: string
  ): Promise<StorageResult<{
    collection: string;
    totalRecords: number;
    recordsWithVectors: number;
    vectorDimensions: number;
    cachedVectors: number;
    lastUpdated: string;
  }>> {
    interface RustVectorStats {
      collection: string;
      totalRecords: number;
      recordsWithVectors: number;
      vectorDimensions: number;
      cachedVectors: number;
      lastUpdated: string;
    }

    const response = await this.request<RustVectorStats>({
      command: 'vector/stats',
      dbPath: dbPath ?? this.dbPath,
      collection,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Get vector stats failed' };
    }

    const stats = response.result?.data;
    if (!stats) {
      return { success: false, error: 'No stats returned' };
    }

    return {
      success: true,
      data: {
        collection: stats.collection,
        totalRecords: stats.totalRecords,
        recordsWithVectors: stats.recordsWithVectors,
        vectorDimensions: stats.vectorDimensions,
        cachedVectors: stats.cachedVectors,
        lastUpdated: stats.lastUpdated,
      },
    };
  }

  /**
   * Invalidate vector cache for a collection
   *
   * Call this when records with embeddings are modified outside of vector/index
   *
   * @param collection - Collection name
   * @param dbPath - Optional database path for per-persona databases
   */
  async invalidateVectorCache(
    collection: string,
    dbPath?: string
  ): Promise<StorageResult<boolean>> {
    const response = await this.request<{ success: boolean; cacheInvalidated: boolean }>({
      command: 'vector/invalidate-cache',
      dbPath: dbPath ?? this.dbPath,
      collection,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Invalidate cache failed' };
    }

    return { success: true, data: true };
  }

  // ─── Paginated Queries ──────────────────────────────────────────────────────

  /**
   * Open a paginated query - returns handle with queryId
   *
   * Advantages over TypeScript:
   * - No IPC overhead per page (state is Rust-side)
   * - DashMap for concurrent query state (lock-free reads)
   */
  async openPaginatedQuery(params: {
    collection: string;
    filter?: Record<string, unknown>;
    orderBy?: { field: string; direction: 'asc' | 'desc' }[];
    pageSize?: number;
    dbPath?: string;
  }): Promise<StorageResult<{
    queryId: string;
    collection: string;
    totalCount: number;
    pageSize: number;
    hasMore: boolean;
  }>> {
    const response = await this.request<{
      queryId: string;
      collection: string;
      totalCount: number;
      pageSize: number;
      hasMore: boolean;
    }>({
      command: 'data/query-open',
      dbPath: params.dbPath ?? this.dbPath,
      collection: params.collection,
      filter: params.filter,
      sort: params.orderBy?.map(o => ({ field: o.field, direction: o.direction })),
      pageSize: params.pageSize ?? 100,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Open paginated query failed' };
    }

    const result = response.result?.data;
    if (!result) {
      return { success: false, error: 'No result returned' };
    }

    return { success: true, data: result };
  }

  /**
   * Get next page from paginated query
   */
  async getNextPage<T>(queryId: string): Promise<StorageResult<{
    items: T[];
    pageNumber: number;
    hasMore: boolean;
    totalCount: number;
  }>> {
    interface RustPageResult {
      items: Array<{ id: string; data: Record<string, unknown>; metadata: Record<string, unknown> }>;
      pageNumber: number;
      hasMore: boolean;
      totalCount: number;
    }

    const response = await this.request<RustPageResult>({
      command: 'data/query-next',
      queryId,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Get next page failed' };
    }

    const result = response.result?.data;
    if (!result) {
      return { success: false, error: 'No result returned' };
    }

    // Convert items - extract entity data and convert to camelCase
    const items: T[] = result.items.map((item) => {
      const entityData = this.toCamelCaseObject(item.data as Record<string, unknown>) as T;
      if (!(entityData as Record<string, unknown>).id) {
        (entityData as Record<string, unknown>).id = item.id;
      }
      return entityData;
    });

    return {
      success: true,
      data: {
        items,
        pageNumber: result.pageNumber,
        hasMore: result.hasMore,
        totalCount: result.totalCount,
      },
    };
  }

  /**
   * Close paginated query and free resources
   */
  async closePaginatedQuery(queryId: string): Promise<StorageResult<boolean>> {
    const response = await this.request<{ success: boolean; queryId: string }>({
      command: 'data/query-close',
      queryId,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Close query failed' };
    }

    return { success: true, data: true };
  }

  // ─── Backfill Vectors ─────────────────────────────────────────────────────────

  /**
   * Backfill vectors - generate embeddings for records missing them
   *
   * Uses batch embedding generation in Rust for efficiency.
   */
  async backfillVectors(params: {
    collection: string;
    textField: string;
    batchSize?: number;
    model?: string;
    filter?: Record<string, unknown>;
    dbPath?: string;
  }): Promise<StorageResult<{
    collection: string;
    total: number;
    processed: number;
    skipped: number;
    failed: number;
    elapsedMs: number;
  }>> {
    const response = await this.request<{
      collection: string;
      total: number;
      processed: number;
      skipped: number;
      failed: number;
      elapsedMs: number;
    }>({
      command: 'vector/backfill',
      dbPath: params.dbPath ?? this.dbPath,
      collection: params.collection,
      textField: params.textField,
      batchSize: params.batchSize ?? 100,
      model: params.model,
      filter: params.filter,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Backfill vectors failed' };
    }

    const result = response.result?.data;
    if (!result) {
      return { success: false, error: 'No result returned' };
    }

    return { success: true, data: result };
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
