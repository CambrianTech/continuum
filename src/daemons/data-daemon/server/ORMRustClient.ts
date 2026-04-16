/**
 * ORM Rust Client - IPC bridge to continuum-core DataModule
 *
 * Pooled IPC client for data/* commands to the Rust continuum-core process.
 * Uses multiple socket connections for concurrent throughput.
 *
 * ARCHITECTURE:
 * - TypeScript ORM.ts delegates to this client when shouldUseRust() returns true
 * - This client maintains a POOL of socket connections to continuum-core
 * - Each request is dispatched to the least-busy connection
 * - Rust DataModule handles all database I/O with its own connection pooling
 * - NO FALLBACKS: If Rust fails, we fail. Period.
 *
 * WHY POOL: A single socket serializes ALL data operations from ALL agents,
 * widgets, and daemons. The Rust side (rayon thread pool) can handle
 * concurrent requests, but it's starved by one pipe. Multiple pipes = parallel I/O.
 *
 * CRITICAL: dbPath is REQUIRED for all operations - no defaults.
 */

import net from 'net';
import path from 'path';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../system/data/entities/BaseEntity';
import { getRegisteredEntity } from './EntityRegistry';
import { getFieldMetadata } from '../../../system/data/decorators/FieldDecorators';
import type {
  DataRecord,
  StorageQuery,
  StorageResult,
  StorageOperation,
  RecordData,
  JoinSpec,
} from '../shared/DataStorageAdapter';
import type { VectorSearchResult } from '../shared/VectorSearchTypes';
import { resolveCoreEndpoint, connectToCoreEndpoint, type CoreEndpoint } from '../../../workers/continuum-core/bindings/modules/base';

// Input type for joins (allows optional properties)
type JoinSpecInput = Partial<JoinSpec> & Pick<JoinSpec, 'collection' | 'alias' | 'localField' | 'foreignField'>;
import { getServerConfig } from '../../../system/config/ServerConfig';
// NOTE: No SqlNamingConverter import - Rust SqliteAdapter handles all naming conversions

// Endpoint resolution: honors CONTINUUM_CORE_URL env (tcp://host:port on Mac
// containerized-TS → native-Rust) or defaults to the Unix socket path.
const CORE_ENDPOINT = resolveCoreEndpoint();

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
  sendTime: number;
  stringifyMs: number;
  writeMs: number;
}

// ─── IPCConnection ──────────────────────────────────────────────────────────
// Single socket connection to continuum-core.
// Handles framing, multiplexing, and response parsing for one pipe.

class IPCConnection {
  private socket: net.Socket | null = null;
  private buffer: Buffer = Buffer.alloc(0);
  private pendingRequests: Map<number, (result: RustIPCResponse<unknown>) => void> = new Map();
  private pendingTimings: Map<number, IPCTiming> = new Map();
  private nextRequestId: number;
  private _connected = false;
  private _connecting = false;

  constructor(
    private endpoint: CoreEndpoint,
    private connectionIndex: number,
  ) {
    // Offset request IDs per connection to avoid confusion in logs
    this.nextRequestId = connectionIndex * 1_000_000 + 1;
  }

  get connected(): boolean { return this._connected; }
  get pendingCount(): number { return this.pendingRequests.size; }

  async connect(): Promise<void> {
    if (this._connected) return;
    if (this._connecting) {
      await new Promise<void>((resolve, reject) => {
        const check = setInterval(() => {
          if (this._connected) { clearInterval(check); resolve(); }
          else if (!this._connecting) { clearInterval(check); reject(new Error('Connection failed')); }
        }, 10);
      });
      return;
    }

    this._connecting = true;

    return new Promise((resolve, reject) => {
      this.socket = connectToCoreEndpoint(this.endpoint);

      this.socket.on('connect', () => {
        this._connected = true;
        this._connecting = false;
        this.reconnectAttempts = 0;
        resolve();
      });

      this.socket.on('data', (data: Buffer) => this.onData(data));

      this.socket.on('error', (err) => {
        this._connecting = false;
        reject(err);
      });

      this.socket.on('close', () => {
        const wasPreviouslyConnected = this._connected;
        this._connected = false;
        this._connecting = false;
        this.socket = null;
        // Reject all pending requests on this connection
        for (const [id, callback] of this.pendingRequests) {
          callback({ success: false, error: 'Connection closed' });
        }
        this.pendingRequests.clear();
        this.pendingTimings.clear();
        // Auto-reconnect with exponential backoff if we were previously connected
        if (wasPreviouslyConnected) {
          this.scheduleReconnect();
        }
      });

      setTimeout(() => {
        if (!this._connected) {
          this._connecting = false;
          reject(new Error(`Connection timeout to ${this.endpoint.description} (conn #${this.connectionIndex})`));
        }
      }, 5000);
    });
  }

  private reconnectAttempts = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  private scheduleReconnect(): void {
    if (this.reconnectTimer) return; // already scheduled
    const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts), 30000); // 1s, 2s, 4s, ... max 30s
    this.reconnectTimer = setTimeout(async () => {
      this.reconnectTimer = null;
      try {
        await this.connect();
        this.reconnectAttempts = 0;
        console.log(`[IPC#${this.connectionIndex}] Reconnected to continuum-core`);
      } catch {
        this.reconnectAttempts++;
        if (this.reconnectAttempts < 10) {
          this.scheduleReconnect(); // try again with longer delay
        } else {
          console.error(`[IPC#${this.connectionIndex}] Gave up reconnecting after ${this.reconnectAttempts} attempts`);
        }
      }
    }, delay);
  }

  private onData(data: Buffer): void {
    // Append incoming data. When buffer is empty (common case after fully
    // consuming all frames), just assign directly — avoids Buffer.concat
    // allocation on every TCP packet, which was a memory leak under load.
    if (this.buffer.length === 0) {
      this.buffer = data;
    } else {
      this.buffer = Buffer.concat([this.buffer, data]);
    }

    while (this.buffer.length >= 4) {
      const totalLength = this.buffer.readUInt32BE(0);
      const frameEnd = 4 + totalLength;

      if (this.buffer.length < frameEnd) break;

      const payload = this.buffer.subarray(4, frameEnd);
      this.buffer = this.buffer.subarray(frameEnd);

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
          console.error(`[IPC#${this.connectionIndex}] ERROR response: ${response.error}`);
        }
        this.handleResponse(response, parseMs);
      } catch (e) {
        console.error(`[IPC#${this.connectionIndex}] Failed to parse response:`, e);
      }
    }

    // Release buffer memory when fully consumed
    if (this.buffer.length === 0) {
      this.buffer = Buffer.alloc(0);
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
        this.pendingTimings.delete(response.requestId);

        // Metrics tracked in ORMLogger — no stdout spam
      }
    }
  }

  async request<T>(command: Record<string, unknown>): Promise<RustIPCResponse<T>> {
    if (!this.socket || !this._connected) {
      throw new Error(`IPC connection #${this.connectionIndex} not connected`);
    }

    const requestId = this.nextRequestId++;
    const requestWithId = { ...command, requestId };
    const cmdName = command.command as string;

    const stringifyStart = Date.now();
    const json = JSON.stringify(requestWithId) + '\n';
    const stringifyMs = Date.now() - stringifyStart;

    return new Promise((resolve, reject) => {
      const timing: IPCTiming = {
        requestId,
        command: cmdName,
        sendTime: Date.now(),
        stringifyMs,
        writeMs: 0,
      };

      this.pendingTimings.set(requestId, timing);
      this.pendingRequests.set(requestId, (result) => resolve(result as RustIPCResponse<T>));

      const writeStart = Date.now();
      this.socket!.write(json, (err) => {
        timing.writeMs = Date.now() - writeStart;

        if (err) {
          this.pendingRequests.delete(requestId);
          this.pendingTimings.delete(requestId);
          reject(err);
        }
      });

      setTimeout(() => {
        if (this.pendingRequests.has(requestId)) {
          console.error(`[IPC#${this.connectionIndex}] TIMEOUT for ${cmdName} (id=${requestId}, pending=${this.pendingCount}, elapsed=${Date.now() - timing.sendTime}ms)`);
          this.pendingRequests.delete(requestId);
          this.pendingTimings.delete(requestId);
          reject(new Error(`Request timeout: ${cmdName}`));
        }
      }, IPC_TIMEOUT_MS);
    });
  }

  disconnect(): void {
    if (this.socket) {
      this.socket.end();
      this.socket = null;
      this._connected = false;
    }
  }
}

// ─── ORMRustClient ──────────────────────────────────────────────────────────
// Pool of IPC connections with least-busy routing.

/** Number of concurrent IPC socket connections to Rust.
 * Must exceed persona count (15+) since each persona can fire
 * multiple concurrent queries (RAG context, chat history, user lookup).
 * Previous value of 12 caused connection exhaustion under load.
 * Raised to 40: 15+ personas × 2-3 concurrent ops each under peak load. */
const POOL_SIZE = 40;

/** IPC request timeout in milliseconds.
 * 60s accommodates load spikes when all personas fire concurrently.
 * Previous 30s caused cascading timeouts under normal multi-persona load. */
const IPC_TIMEOUT_MS = 60_000;

export class ORMRustClient {
  private static instance: ORMRustClient | null = null;
  private connections: IPCConnection[] = [];
  private poolReady = false;
  private poolConnecting = false;
  private dbPath: string;

  /**
   * Negative result cache — prevents repeated IPC calls for records that don't exist.
   * Key: "collection:id", Value: timestamp when not-found was recorded.
   * Entries expire after NOT_FOUND_TTL_MS. Without this, stale references cause
   * "Record not found" loops that block persona responsiveness for hours (#482).
   */
  private static readonly NOT_FOUND_TTL_MS = 30_000; // 30 seconds
  private static readonly NOT_FOUND_MAX_ENTRIES = 2000;
  private notFoundCache = new Map<string, number>();

  private constructor() {
    this.dbPath = getServerConfig().getDatabasePath();
  }

  static getInstance(): ORMRustClient {
    if (!ORMRustClient.instance) {
      ORMRustClient.instance = new ORMRustClient();
    }
    return ORMRustClient.instance;
  }

  /**
   * Create and connect the pool of IPC connections.
   * All connections are opened in parallel on first use.
   */
  private async ensurePool(): Promise<void> {
    if (this.poolReady) return;
    if (this.poolConnecting) {
      await new Promise<void>((resolve, reject) => {
        const check = setInterval(() => {
          if (this.poolReady) { clearInterval(check); resolve(); }
          else if (!this.poolConnecting) { clearInterval(check); reject(new Error('Pool creation failed')); }
        }, 10);
      });
      return;
    }

    this.poolConnecting = true;
    try {
      // Create connections — connect as many as possible, don't fail if some drop
      for (let i = 0; i < POOL_SIZE; i++) {
        this.connections.push(new IPCConnection(CORE_ENDPOINT, i));
      }
      const results = await Promise.allSettled(
        this.connections.map(c => c.connect())
      );
      const connected = results.filter(r => r.status === 'fulfilled').length;
      if (connected === 0) {
        throw new Error('No IPC connections to continuum-core — is it running?');
      }
      if (connected < POOL_SIZE) {
        console.warn(`[ORM] ${connected}/${POOL_SIZE} IPC connections established (rest will auto-reconnect)`);
      }
      this.poolReady = true;
      this.startHealthCheck();
    } finally {
      this.poolConnecting = false;
    }
  }

  private healthCheckInterval: ReturnType<typeof setInterval> | null = null;

  /**
   * Periodic health check — counts live connections, logs warnings.
   * Dead connections auto-reconnect via IPCConnection.scheduleReconnect().
   */
  private startHealthCheck(): void {
    if (this.healthCheckInterval) return;
    this.healthCheckInterval = setInterval(() => {
      const alive = this.connections.filter(c => c.connected).length;
      if (alive === 0 && this.poolReady) {
        console.error('[ORM] CRITICAL: All IPC connections dead — Rust core may have crashed');
      } else if (alive < POOL_SIZE / 2) {
        console.warn(`[ORM] IPC health: ${alive}/${POOL_SIZE} connections alive`);
      }
    }, 10_000); // Check every 10 seconds
  }

  /**
   * Pick the least-busy connected connection.
   * If all connections are down (e.g. core restarted), waits for any
   * connection to come back rather than failing immediately.
   * Individual connections auto-reconnect via scheduleReconnect().
   */
  private async getConnection(): Promise<IPCConnection> {
    const conn = this.findBestConnection();
    if (conn) return conn;

    // All disconnected — kick off reconnection on all, then wait for any to come back.
    // This prevents message loss during core restart (10-30s reconnection window).
    for (const c of this.connections) {
      if (!c.connected) c.connect().catch(() => {});
    }

    const maxWaitMs = 15_000;
    const pollMs = 100;
    const deadline = Date.now() + maxWaitMs;

    while (Date.now() < deadline) {
      await new Promise(r => setTimeout(r, pollMs));
      const recovered = this.findBestConnection();
      if (recovered) return recovered;
    }

    throw new Error('All IPC connections to continuum-core failed (waited 15s for reconnection)');
  }

  private findBestConnection(): IPCConnection | null {
    let best: IPCConnection | null = null;
    let totalPending = 0;

    for (const conn of this.connections) {
      if (!conn.connected) continue;
      totalPending += conn.pendingCount;
      if (!best || conn.pendingCount < best.pendingCount) {
        best = conn;
      }
    }

    if (totalPending > 100) {
      console.warn(`[ORM] BACKPRESSURE: ${totalPending} pending IPC requests across ${POOL_SIZE} connections`);
    }

    return best;
  }

  /**
   * Send request to Rust via the least-busy connection
   */
  private async request<T>(command: Record<string, unknown>): Promise<RustIPCResponse<T>> {
    await this.ensurePool();
    const conn = await this.getConnection();
    return conn.request(command);
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
    // Invalidate not-found cache — record is being created
    if (data.id) this.invalidateNotFound(collection, data.id);

    const actualDbPath = dbPath ?? this.dbPath;
    const response = await this.request<RustDataRecord>({
      command: 'data/create',
      dbPath: actualDbPath,
      collection,
      id: data.id,
      data,
    });

    if (!response.success) {
      console.error('[ORMRustClient.store] Store failed:', response.error);
      return { success: false, error: response.error || 'Store failed' };
    }

    // Check operation-level success (StorageResult.success), not just IPC transport
    if (response.result && !response.result.success) {
      console.error('[ORMRustClient.store] Store failed at storage level:', response.result.error);
      return { success: false, error: response.result.error || 'Store failed at storage level' };
    }

    const rustRecord = response.result?.data;
    const mergedData = rustRecord
      ? { ...data, id: rustRecord.id ?? data.id } as T
      : data;

    return { success: true, data: mergedData };
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
      collection: query.collection,
      filter: query.filter,
      sort: query.sort,
      limit: query.limit,
      offset: query.offset,
      select: query.select,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Query failed' };
    }

    // Check operation-level success
    if (response.result && !response.result.success) {
      return { success: false, error: response.result.error || 'Query failed at storage level' };
    }

    const rustResult = response.result;
    const rawRecords: RustDataRecord[] = rustResult?.data ?? [];

    const records = this.deserializeRecords<T>(rawRecords, query.collection);

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
      select: query.select,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Query with join failed' };
    }

    // Check operation-level success
    if (response.result && !response.result.success) {
      return { success: false, error: response.result.error || 'Query with join failed at storage level' };
    }

    const rustResult = response.result;
    const rawRecords: RustDataRecord[] = rustResult?.data ?? [];
    const records = this.deserializeRecords<T>(rawRecords, query.collection);

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
      collection: query.collection,
      filter: query.filter,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Count failed' };
    }

    // Check operation-level success
    if (response.result && !response.result.success) {
      return { success: false, error: response.result.error || 'Count failed at storage level' };
    }

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
    // Check negative cache — avoid repeated IPC for known-missing records (#482)
    const cacheKey = `${collection}:${id}`;
    const notFoundAt = this.notFoundCache.get(cacheKey);
    if (notFoundAt && (Date.now() - notFoundAt) < ORMRustClient.NOT_FOUND_TTL_MS) {
      return null;
    }

    const response = await this.request<RustDataRecord>({
      command: 'data/read',
      dbPath: dbPath ?? this.dbPath,
      collection,
      id,
    });

    if (!response.success || !response.result?.data) {
      // Cache the not-found result to prevent loop (#482)
      this.cacheNotFound(cacheKey);
      return null;
    }

    const item = response.result.data;
    let entityData: T;

    if (typeof item.data === 'string') {
      entityData = JSON.parse(item.data) as T;
    } else if (item.data && typeof item.data === 'object') {
      entityData = item.data as T;
    } else {
      const { id: _id, created_at: _ca, updated_at: _ua, version: _v, ...rest } = item as unknown as Record<string, unknown>;
      entityData = this.toCamelCaseObject(rest) as T;
    }

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
    // Invalidate not-found cache — record exists if being updated
    this.invalidateNotFound(collection, id);

    const response = await this.request<RustDataRecord>({
      command: 'data/update',
      dbPath: dbPath ?? this.dbPath,
      collection,
      id,
      data,
      incrementVersion,
    });

    if (!response.success) {
      throw new Error(response.error || 'Update failed');
    }

    // Check operation-level success (StorageResult.success), not just IPC transport
    if (response.result && !response.result.success) {
      throw new Error(response.result.error || 'Update failed at storage level');
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
      collection,
      id,
    });

    if (!response.success) {
      return { success: false, error: response.error || 'Delete failed' };
    }

    // Check operation-level success (StorageResult.success), not just IPC transport
    if (response.result && !response.result.success) {
      return { success: false, error: response.result.error || 'Delete failed at storage level' };
    }

    return { success: true, data: true };
  }

  /**
   * Batch operations
   * NOTE: Passes camelCase - Rust SqliteAdapter handles all naming conversion
   * @param dbPath - Optional database path for per-persona databases (defaults to main DB)
   */
  async batch(operations: StorageOperation[], dbPath?: string): Promise<StorageResult<unknown[]>> {
    const rustOps = operations.map(op => ({
      type: op.type,
      collection: op.collection,
      id: op.id,
      data: op.data,
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
      collection,
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

  // ─── Deserialization ────────────────────────────────────────────────────────

  /**
   * Shared record deserialization — single code path for query + queryWithJoin.
   * Rust sends data as objects (not strings) in the normal path.
   * Only falls back to JSON.parse when data arrives as a string (legacy/edge case).
   */
  private deserializeRecords<T extends BaseEntity>(
    rawRecords: RustDataRecord[],
    collection: string,
  ): DataRecord<T>[] {
    // Cache timestamp for this batch — avoid N×2 Date allocations
    const now = new Date().toISOString();

    // Look up @DateField metadata once per batch (not per record)
    const dateFields = this.getDateFieldNames(collection);

    return rawRecords.map((item: RustDataRecord) => {
      let entityData: T;

      if (item.data && typeof item.data === 'object') {
        // Fast path: Rust already sent a parsed object
        entityData = item.data as T;
      } else if (typeof item.data === 'string') {
        // Legacy/edge: data arrived as JSON string
        entityData = JSON.parse(item.data) as T;
      } else {
        // Fallback: extract entity fields from flat row
        const { id: _id, created_at: _ca, updated_at: _ua, version: _v, collection: _c, metadata: _m, ...rest } = item as unknown as Record<string, unknown>;
        entityData = this.toCamelCaseObject(rest) as T;
      }

      if (!entityData.id) {
        (entityData as BaseEntity).id = item.id as UUID;
      }

      // Hydrate @DateField ISO strings → Date objects
      // This is the architectural contract: @DateField means the field is a Date,
      // and the ORM is responsible for the string↔Date conversion at the boundary.
      if (dateFields.length > 0) {
        this.hydrateDateFields(entityData, dateFields);
      }

      return {
        id: item.id,
        collection,
        data: entityData,
        metadata: {
          createdAt: item.metadata?.created_at || now,
          updatedAt: item.metadata?.updated_at || now,
          version: item.metadata?.version || 1,
        },
      };
    });
  }

  // Cache date field names per collection to avoid repeated metadata lookups
  private _dateFieldCache = new Map<string, string[]>();

  /**
   * Get @DateField-decorated field names for an entity's collection.
   * Cached per collection — metadata never changes at runtime.
   */
  private getDateFieldNames(collection: string): string[] {
    const cached = this._dateFieldCache.get(collection);
    if (cached !== undefined) return cached;

    const entityClass = getRegisteredEntity(collection);
    if (!entityClass) {
      this._dateFieldCache.set(collection, []);
      return [];
    }

    const metadata = getFieldMetadata(entityClass);
    const dateFieldNames: string[] = [];
    for (const [fieldName, fieldMeta] of metadata) {
      if (fieldMeta.fieldType === 'date') {
        dateFieldNames.push(fieldName);
      }
    }

    this._dateFieldCache.set(collection, dateFieldNames);
    return dateFieldNames;
  }

  /**
   * Convert ISO string values to Date objects for @DateField-decorated fields.
   * Mutates entityData in-place for performance (no allocation).
   */
  private hydrateDateFields(entityData: Record<string, unknown>, dateFields: string[]): void {
    for (const field of dateFields) {
      const value = entityData[field];
      if (typeof value === 'string') {
        entityData[field] = new Date(value);
      } else if (typeof value === 'number') {
        entityData[field] = new Date(value);
      }
      // Already a Date or null/undefined → leave as-is
    }
  }

  // ─── Case Conversion Helpers ────────────────────────────────────────────────
  // NOTE: Only used for flat-row fallback (Rust normally returns parsed objects)

  private toCamelCaseObject(obj: Record<string, unknown>): Record<string, unknown> {
    const result: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj)) {
      result[this.snakeToCamel(key)] = value;
    }
    return result;
  }

  private snakeToCamel(s: string): string {
    return s.replace(/_([a-z])/g, (_, char) => char.toUpperCase());
  }

  // ─── Negative Result Cache (#482) ─────────────────────────────────

  /**
   * Record that a collection:id was not found. Prevents repeated IPC for stale references.
   */
  private cacheNotFound(key: string): void {
    this.notFoundCache.set(key, Date.now());
    // Evict oldest entries if cache exceeds max
    if (this.notFoundCache.size > ORMRustClient.NOT_FOUND_MAX_ENTRIES) {
      const entries = [...this.notFoundCache.entries()];
      entries.sort((a, b) => a[1] - b[1]);
      const toRemove = entries.slice(0, entries.length - ORMRustClient.NOT_FOUND_MAX_ENTRIES);
      for (const [k] of toRemove) {
        this.notFoundCache.delete(k);
      }
    }
  }

  /**
   * Invalidate not-found cache for a record (e.g., after store/update creates it).
   */
  private invalidateNotFound(collection: string, id: string): void {
    this.notFoundCache.delete(`${collection}:${id}`);
  }

  /**
   * Close all connections in the pool
   */
  disconnect(): void {
    for (const conn of this.connections) {
      conn.disconnect();
    }
    this.connections = [];
    this.poolReady = false;
    ORMRustClient.instance = null;
  }

  /**
   * Check if at least one connection in the pool is active
   */
  isConnected(): boolean {
    return this.poolReady && this.connections.some(conn => conn.connected);
  }
}
