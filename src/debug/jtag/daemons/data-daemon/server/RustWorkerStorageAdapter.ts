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
 *
 * Type Safety: Response types generated from Rust via ts-rs (shared/generated/data-daemon/).
 * Rust is the single source of truth for the wire format.
 * Re-generate: cargo test --package data-daemon-worker export_bindings
 */

import * as net from 'net';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import {
  DataStorageAdapter,
  type DataRecord,
  type StorageQuery,
  type StorageQueryWithJoin,
  type JoinSpec,
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

// Generated types from Rust via ts-rs — single source of truth for IPC wire format
// Re-generate: cargo test --package data-daemon-worker export_bindings
import type {
  DataListResult,
  DataQueryResult,
  ListTablesResult,
  DataWriteResult,
  VectorSearchResult as RustVectorSearchResult,
  VectorSearchHit,
  AdapterOpenResult,
  BlobStoreResult,
  BlobStatsResult,
  BlobExistsResult,
  BlobDeleteResult,
} from '../../../shared/generated/data-daemon';

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
 * Rust worker response envelope — discriminated union matching Rust's Response enum.
 *
 * Rust source of truth: workers/data-daemon/src/main.rs
 * Uses serde's tag-based enum serialization (#[serde(tag = "status")])
 *
 * TypeScript narrows the type when you check `status`:
 *   if (response.status === 'ok') { response.data } // data exists
 *   if (response.status === 'error') { response.message } // message exists
 */
type RustResponse<T = unknown> =
  | { status: 'ok'; data: T }
  | { status: 'error'; message: string }
  | { status: 'pong'; uptime_seconds: number };

/**
 * A single pooled connection to the Rust worker.
 * Each connection has its own socket, buffer, and pending response slot.
 * The Rust worker spawns a thread per connection, so N connections = N-way parallelism.
 */
interface PooledConnection {
  id: number;
  socket: net.Socket;
  buffer: string;
  pendingResponse: {
    resolve: (value: any) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout;
  } | null;
  busy: boolean;
}

const POOL_SIZE = 8;

/**
 * Rust Worker Storage Adapter - Fast concurrent storage via Rust process
 *
 * Uses a connection pool (8 sockets by default) to the Rust worker.
 * Each connection maps to a Rust thread, enabling parallel database I/O.
 */
export class RustWorkerStorageAdapter extends DataStorageAdapter {
  private config!: RustWorkerConfig;
  private pool: PooledConnection[] = [];
  private adapterHandle: string | null = null;  // Handle from adapter/open (shared across pool)
  private waitQueue: Array<(conn: PooledConnection) => void> = [];

  // Pool utilization stats
  private _statsInterval: NodeJS.Timeout | null = null;
  private _requestCount = 0;
  private _waitCount = 0;  // Requests that had to wait for a connection
  private _totalAcquireMs = 0;
  private _totalRoundTripMs = 0;
  private _maxWaitQueueDepth = 0;

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
   * Also hydrates JSON string values — SQLite stores JSON as TEXT, so fields like
   * reactions="[]" or content="{...}" need to be parsed back to objects/arrays.
   */
  private toCamelCaseObject(obj: Record<string, any>): Record<string, any> {
    const result: Record<string, any> = {};
    for (const [key, value] of Object.entries(obj)) {
      const camelKey = SqlNamingConverter.toCamelCase(key);
      result[camelKey] = this.hydrateValue(value);
    }
    return result;
  }

  /**
   * Hydrate a single value — parse JSON strings back to objects/arrays.
   * SQLite TEXT columns containing JSON come back as raw strings from Rust.
   */
  private hydrateValue(value: any): any {
    if (typeof value !== 'string') return value;
    const trimmed = value.trim();
    if ((trimmed.startsWith('{') && trimmed.endsWith('}')) ||
        (trimmed.startsWith('[') && trimmed.endsWith(']'))) {
      try {
        return JSON.parse(trimmed);
      } catch {
        return value; // Not valid JSON, return as-is
      }
    }
    return value;
  }

  constructor(config?: RustWorkerConfig) {
    super();
    if (config) {
      this.config = config;
    }
  }

  /**
   * Initialize connection pool to Rust worker
   *
   * Opens POOL_SIZE concurrent socket connections. Each maps to a Rust thread,
   * enabling parallel database I/O. Opens the SQLite adapter once (handle is
   * shared across all connections via Rust's register_with_cache).
   */
  async initialize(config: StorageAdapterConfig): Promise<void> {
    const options = config.options as any;

    if (!options?.socketPath) {
      throw new Error('RustWorkerStorageAdapter requires socketPath in options');
    }
    if (!options?.dbPath) {
      throw new Error('RustWorkerStorageAdapter requires dbPath in options');
    }

    this.config = {
      socketPath: options.socketPath,
      dbPath: options.dbPath,
      timeout: options.timeout || 60000
    };

    // Open POOL_SIZE connections in parallel
    const connectPromises: Promise<PooledConnection>[] = [];
    for (let i = 0; i < POOL_SIZE; i++) {
      connectPromises.push(this.openConnection(i));
    }
    this.pool = await Promise.all(connectPromises);

    // Open SQLite adapter via the first connection (handle is shared in Rust)
    const response = await this.sendCommand<AdapterOpenResult>('adapter/open', {
      config: {
        adapter_type: 'sqlite',
        connection_string: this.config.dbPath
      }
    });

    if (response.status === 'ok' && response.data.handle) {
      this.adapterHandle = response.data.handle;
      log.info(`Opened SQLite adapter: ${this.config.dbPath} → handle ${this.adapterHandle} (${POOL_SIZE} connections)`);
    } else if (response.status === 'error') {
      throw new Error(`Failed to open adapter: ${response.message}`);
    } else {
      throw new Error('Failed to open adapter: unexpected response');
    }

    // Log pool utilization every 30 seconds
    this._statsInterval = setInterval(() => {
      if (this._requestCount === 0) return;
      const busyCount = this.pool.filter(c => c.busy).length;
      const avgAcquire = this._totalAcquireMs / this._requestCount;
      const avgRoundTrip = this._totalRoundTripMs / this._requestCount;
      log.info(`🦀 Pool stats: ${this._requestCount} reqs, ${this._waitCount} waited, ` +
        `avg acquire=${avgAcquire.toFixed(0)}ms, avg roundtrip=${avgRoundTrip.toFixed(0)}ms, ` +
        `busy=${busyCount}/${POOL_SIZE}, max queue=${this._maxWaitQueueDepth}`);
      // Reset for next interval
      this._requestCount = 0;
      this._waitCount = 0;
      this._totalAcquireMs = 0;
      this._totalRoundTripMs = 0;
      this._maxWaitQueueDepth = 0;
    }, 30_000);
  }

  /**
   * Open a single socket connection to the Rust worker
   */
  private openConnection(id: number): Promise<PooledConnection> {
    return new Promise((resolve, reject) => {
      const socket = net.createConnection(this.config.socketPath);
      const conn: PooledConnection = {
        id,
        socket,
        buffer: '',
        pendingResponse: null,
        busy: false,
      };

      socket.on('connect', () => {
        resolve(conn);
      });

      socket.on('data', (data) => {
        this.handleConnectionData(conn, data);
      });

      socket.on('error', (error) => {
        log.error(`Rust worker socket #${id} error: ${error.message}`);
      });

      socket.on('close', () => {
        log.warn(`Rust worker connection #${id} closed`);
        // Mark as not busy so it can be reconnected on next acquire
        conn.busy = false;
      });

      setTimeout(() => {
        if (socket.connecting) {
          reject(new Error(`Connection #${id} timeout: ${this.config.socketPath}`));
        }
      }, 10000);
    });
  }

  /**
   * Handle incoming data on a specific pooled connection
   */
  private handleConnectionData(conn: PooledConnection, data: Buffer): void {
    conn.buffer += data.toString();

    const lines = conn.buffer.split('\n');
    conn.buffer = lines.pop() || '';

    for (const line of lines) {
      if (!line.trim()) continue;

      try {
        const response = JSON.parse(line) as RustResponse;

        if (conn.pendingResponse) {
          clearTimeout(conn.pendingResponse.timeout);
          const pending = conn.pendingResponse;
          conn.pendingResponse = null;
          conn.busy = false;
          // Wake up next waiter if any
          if (this.waitQueue.length > 0) {
            const waiter = this.waitQueue.shift()!;
            waiter(conn);
          }
          pending.resolve(response);
        }
      } catch (error) {
        log.error(`Failed to parse response from Rust worker #${conn.id}: ${error}`);
      }
    }
  }

  /**
   * Acquire an available connection from the pool.
   * If all are busy, waits for one to become available.
   */
  private acquireConnection(): Promise<PooledConnection> {
    // Find first non-busy connection
    for (const conn of this.pool) {
      if (!conn.busy && conn.socket && !conn.socket.destroyed) {
        conn.busy = true;
        return Promise.resolve(conn);
      }
    }

    // All busy — wait for one to free up
    this._waitCount++;
    return new Promise((resolve) => {
      this.waitQueue.push((conn: PooledConnection) => {
        conn.busy = true;
        resolve(conn);
      });
    });
  }

  /**
   * Send command to Rust worker via the connection pool.
   * Acquires a connection, sends, waits for response, releases.
   *
   * Generic T should match the generated response type from ts-rs
   * (e.g., DataListResult, VectorSearchResult, ListTablesResult).
   */
  private async sendCommand<T = unknown>(command: string, params: Record<string, any> = {}): Promise<RustResponse<T>> {
    const acquireStart = Date.now();
    const conn = await this.acquireConnection();
    const acquireMs = Date.now() - acquireStart;

    this._requestCount++;
    this._totalAcquireMs += acquireMs;
    if (this.waitQueue.length > this._maxWaitQueueDepth) {
      this._maxWaitQueueDepth = this.waitQueue.length;
    }

    const request = {
      command,
      ...params
    };

    const sendStart = Date.now();

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        conn.pendingResponse = null;
        conn.busy = false;
        if (this.waitQueue.length > 0) {
          const waiter = this.waitQueue.shift()!;
          waiter(conn);
        }
        reject(new Error(`Request timeout: ${command}`));
      }, this.config.timeout);

      conn.pendingResponse = {
        resolve: (value: any) => {
          this._totalRoundTripMs += (Date.now() - sendStart);
          resolve(value);
        },
        reject,
        timeout
      };

      conn.socket.write(JSON.stringify(request) + '\n');
    });
  }

  /**
   * Ensure pool is connected and adapter handle is available
   */
  private async ensureConnected(): Promise<void> {
    if (this.pool.length === 0 || !this.adapterHandle) {
      throw new Error('RustWorkerStorageAdapter not initialized');
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

      const response = await this.sendCommand<DataWriteResult>('data/create', {
        handle: this.adapterHandle,
        collection: SqlNamingConverter.toTableName(record.collection),
        data: fullData
      });

      if (response.status !== 'ok') {
        const errorMsg = response.status === 'error' ? response.message : 'Create failed';
        return { success: false, error: errorMsg };
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
      const response = await this.sendCommand<DataListResult>('data/list', {
        handle: this.adapterHandle,
        collection,
        filter: { id },
        limit: 1
      });

      if (response.status !== 'ok' || !response.data.items?.length) {
        const errorMsg = response.status === 'error' ? response.message : 'Record not found';
        return { success: false, error: errorMsg };
      }

      const item = response.data.items[0] as any;

      // Hydrate: convert snake_case keys to camelCase and parse JSON string values
      let entityData: T;
      if (typeof item.data === 'string') {
        entityData = JSON.parse(item.data) as T;
      } else if (item.data && typeof item.data === 'object') {
        entityData = item.data as T;
      } else {
        const { id: _id, created_at, updated_at, version, ...rest } = item;
        entityData = this.toCamelCaseObject(rest) as T;
      }

      // Ensure id is always present in the data object
      if (!(entityData as any).id) {
        (entityData as any).id = id;
      }

      return {
        success: true,
        data: {
          id,
          collection,
          data: entityData,
          metadata: {
            createdAt: item.created_at || new Date().toISOString(),
            updatedAt: item.updated_at || new Date().toISOString(),
            version: item.version || 1
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

      const response = await this.sendCommand<DataListResult>('data/list', {
        handle: this.adapterHandle,
        collection: SqlNamingConverter.toTableName(query.collection),
        filter: snakeCaseFilter,
        order_by: snakeCaseOrderBy,
        limit: query.limit,
        offset: query.offset
      });

      if (response.status !== 'ok') {
        const errorMsg = response.status === 'error' ? response.message : 'Query failed';
        return { success: false, error: errorMsg };
      }

      const records: DataRecord<T>[] = (response.data.items || []).map((item: any) => {
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
          totalCount: (response.status === 'ok' ? response.data.count : 0) || records.length
        }
      };
    } catch (error: any) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Query records with JOIN support for loading related data
   *
   * Builds a SQL query with JOINs and executes via Rust data/query command.
   * Joined data is nested under the alias key in each result.
   *
   * @param query - Query with join specifications
   * @returns Records with joined data nested under alias keys
   */
  async queryWithJoin<T extends RecordData>(
    query: StorageQueryWithJoin
  ): Promise<StorageResult<DataRecord<T>[]>> {
    try {
      await this.ensureConnected();
    } catch (error: any) {
      return { success: false, error: `Connection failed: ${error.message}` };
    }

    try {
      const primaryTable = SqlNamingConverter.toTableName(query.collection);
      const primaryAlias = 'p';

      // Build SELECT clause
      const selectClauses: string[] = [`${primaryAlias}.*`];
      const joinAliasMap: Map<string, { alias: string; select?: readonly string[] }> = new Map();

      query.joins.forEach((join, index) => {
        const joinTable = SqlNamingConverter.toTableName(join.collection);
        const joinAlias = `j${index}`;
        joinAliasMap.set(join.alias, { alias: joinAlias, select: join.select });

        if (join.select && join.select.length > 0) {
          // Select specific fields with alias prefix
          join.select.forEach(field => {
            const snakeField = SqlNamingConverter.toSnakeCase(field);
            selectClauses.push(`${joinAlias}.${snakeField} AS ${join.alias}_${snakeField}`);
          });
        } else {
          // Select all fields from joined table (risky - could have name collisions)
          selectClauses.push(`${joinAlias}.*`);
        }
      });

      // Build JOIN clauses
      const joinClauses: string[] = [];
      query.joins.forEach((join, index) => {
        const joinTable = SqlNamingConverter.toTableName(join.collection);
        const joinAlias = `j${index}`;
        const joinType = join.type === 'inner' ? 'INNER JOIN' : 'LEFT JOIN';
        const localField = SqlNamingConverter.toSnakeCase(join.localField);
        const foreignField = SqlNamingConverter.toSnakeCase(join.foreignField);

        joinClauses.push(
          `${joinType} ${joinTable} ${joinAlias} ON ${primaryAlias}.${localField} = ${joinAlias}.${foreignField}`
        );
      });

      // Build WHERE clause
      let whereClause = '';
      if (query.filter && Object.keys(query.filter).length > 0) {
        const conditions = Object.entries(query.filter).map(([key, value]) => {
          const snakeKey = SqlNamingConverter.toSnakeCase(key);
          if (value === null) {
            return `${primaryAlias}.${snakeKey} IS NULL`;
          }
          const escapedValue = typeof value === 'string'
            ? `'${value.replace(/'/g, "''")}'`
            : value;
          return `${primaryAlias}.${snakeKey} = ${escapedValue}`;
        });
        whereClause = `WHERE ${conditions.join(' AND ')}`;
      }

      // Build ORDER BY clause
      let orderByClause = '';
      if (query.sort && query.sort.length > 0) {
        const orderParts = query.sort.map(s => {
          const snakeField = SqlNamingConverter.toSnakeCase(s.field);
          return `${primaryAlias}.${snakeField} ${s.direction.toUpperCase()}`;
        });
        orderByClause = `ORDER BY ${orderParts.join(', ')}`;
      }

      // Build LIMIT/OFFSET
      const limitClause = query.limit ? `LIMIT ${query.limit}` : '';
      const offsetClause = query.offset ? `OFFSET ${query.offset}` : '';

      // Assemble full SQL
      const sql = [
        `SELECT ${selectClauses.join(', ')}`,
        `FROM ${primaryTable} ${primaryAlias}`,
        ...joinClauses,
        whereClause,
        orderByClause,
        limitClause,
        offsetClause
      ].filter(Boolean).join(' ');

      log.debug(`queryWithJoin SQL: ${sql}`);

      // Execute via Rust data/query
      const result = await this.rawQuery(sql);

      // Transform results: nest joined data under alias keys
      const records: DataRecord<T>[] = result.items.map((row: any) => {
        // Extract primary entity fields (those without alias prefix)
        const primaryData: Record<string, any> = {};
        const joinedData: Record<string, Record<string, any>> = {};

        // Initialize nested objects for each join alias
        for (const join of query.joins) {
          joinedData[join.alias] = {};
        }

        for (const [key, value] of Object.entries(row)) {
          // Check if this is a joined field (has alias_ prefix)
          let isJoinedField = false;
          for (const join of query.joins) {
            if (key.startsWith(`${join.alias}_`)) {
              const fieldName = key.slice(join.alias.length + 1);
              const camelField = SqlNamingConverter.toCamelCase(fieldName);
              joinedData[join.alias][camelField] = value;
              isJoinedField = true;
              break;
            }
          }

          if (!isJoinedField) {
            const camelKey = SqlNamingConverter.toCamelCase(key);
            primaryData[camelKey] = value;
          }
        }

        // Merge joined data into primary data
        const entityData = {
          ...primaryData,
          ...joinedData
        } as T;

        return {
          id: row.id as UUID,
          collection: query.collection,
          data: entityData,
          metadata: {
            createdAt: row.created_at || new Date().toISOString(),
            updatedAt: row.updated_at || new Date().toISOString(),
            version: row.version || 1
          }
        };
      });

      return {
        success: true,
        data: records,
        metadata: {
          totalCount: result.count
        }
      };
    } catch (error: any) {
      log.error(`queryWithJoin failed: ${error.message}`);
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

      const response = await this.sendCommand<DataWriteResult>('data/update', {
        handle: this.adapterHandle,
        collection: SqlNamingConverter.toTableName(collection),
        id,
        data: updateData
      });

      if (response.status !== 'ok') {
        const errorMsg = response.status === 'error' ? response.message : 'Update failed';
        return { success: false, error: errorMsg };
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
      const response = await this.sendCommand<DataWriteResult>('data/delete', {
        handle: this.adapterHandle,
        collection,
        id
      });

      if (response.status !== 'ok') {
        const errorMsg = response.status === 'error' ? response.message : 'Delete failed';
        return { success: false, error: errorMsg };
      }

      return { success: true, data: true };
    } catch (error: any) {
      return { success: false, error: error.message };
    }
  }

  /**
   * List all collections (tables) in the database via Rust worker
   */
  async listCollections(): Promise<StorageResult<string[]>> {
    try {
      await this.ensureConnected();
    } catch (error: any) {
      return { success: false, error: `Connection failed: ${error.message}` };
    }

    try {
      const response = await this.sendCommand<ListTablesResult>('data/list_tables', {
        handle: this.adapterHandle,
      });

      if (response.status !== 'ok') {
        const errorMsg = response.status === 'error' ? response.message : 'List tables failed';
        return { success: false, error: errorMsg };
      }

      return { success: true, data: response.data.tables || [] };
    } catch (error: any) {
      return { success: false, error: error.message };
    }
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
   * Clear all data from all collections via Rust worker
   */
  async clear(): Promise<StorageResult<boolean>> {
    try {
      const tablesResult = await this.listCollections();
      if (!tablesResult.success || !tablesResult.data) {
        return { success: false, error: tablesResult.error || 'Failed to list tables' };
      }

      for (const table of tablesResult.data) {
        await this.truncate(table);
      }

      return { success: true, data: true };
    } catch (error: any) {
      return { success: false, error: error.message };
    }
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
   * Clear all data from all collections with reporting via Rust worker
   */
  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    try {
      const tablesResult = await this.listCollections();
      if (!tablesResult.success || !tablesResult.data) {
        return { success: false, error: tablesResult.error || 'Failed to list tables' };
      }

      const tablesCleared: string[] = [];
      for (const table of tablesResult.data) {
        const result = await this.truncate(table);
        if (result.success) {
          tablesCleared.push(table);
        }
      }

      return {
        success: true,
        data: { tablesCleared, recordsDeleted: 0 }
      };
    } catch (error: any) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Truncate specific collection (delete all rows) via Rust worker
   */
  async truncate(collection: string): Promise<StorageResult<boolean>> {
    try {
      await this.ensureConnected();
    } catch (error: any) {
      return { success: false, error: `Connection failed: ${error.message}` };
    }

    try {
      const response = await this.sendCommand<DataWriteResult>('data/truncate', {
        handle: this.adapterHandle,
        collection,
      });

      if (response.status !== 'ok') {
        const errorMsg = response.status === 'error' ? response.message : 'Truncate failed';
        return { success: false, error: errorMsg };
      }

      return { success: true, data: true };
    } catch (error: any) {
      return { success: false, error: error.message };
    }
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
      // Response type: RustVectorSearchResult (generated from Rust via ts-rs)
      const searchResult = await this.sendCommand<RustVectorSearchResult>('vector/search', {
        handle: this.adapterHandle,
        collection,
        query_vector: toNumberArray(queryVector),
        k,
        threshold,
        include_data: true  // OPTIMIZATION: Get full records in one Rust query
      });

      if (searchResult.status !== 'ok') {
        // Fallback message for collections without embeddings
        const errorMsg = searchResult.status === 'error' ? searchResult.message : '';
        if (errorMsg.includes('no such column: embedding')) {
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
          error: errorMsg || 'Vector search failed in Rust worker'
        };
      }

      const rustResults = searchResult.data.results;
      const corpusSize = searchResult.data.corpus_size;
      log.debug(`Vector search: Rust returned ${rustResults.length}/${corpusSize} results with inline data`);

      // 3. Map Rust results directly - no additional IPC round trips needed!
      // Rust already fetched full records with include_data=true
      // VectorSearchHit type generated from Rust via ts-rs
      const results: VectorSearchResultType<T>[] = rustResults
        .filter((r: VectorSearchHit) => r.data) // Only include results that have data
        .map((rustResult: VectorSearchHit) => {
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
    const response = await this.sendCommand<BlobStoreResult>('blob/store', {
      data,
      base_path: basePath
    });

    if (response.status !== 'ok') {
      const errorMsg = response.status === 'error' ? response.message : 'Blob store failed';
      throw new Error(errorMsg);
    }

    return {
      hash: response.data.hash,
      size: response.data.size,
      compressedSize: response.data.compressed_size,
      deduplicated: response.data.deduplicated,
      storedAt: response.data.stored_at,
    };
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
      const errorMsg = response.status === 'error' ? response.message : 'Blob retrieve failed';
      throw new Error(errorMsg);
    }

    return response.data;
  }

  /**
   * Check if blob exists
   * @param hash - Blob hash (sha256:...)
   * @param basePath - Optional custom blob storage path
   */
  async blobExists(hash: string, basePath?: string): Promise<boolean> {
    const response = await this.sendCommand<BlobExistsResult>('blob/exists', {
      hash,
      base_path: basePath
    });

    if (response.status !== 'ok') {
      const errorMsg = response.status === 'error' ? response.message : 'Blob exists check failed';
      throw new Error(errorMsg);
    }

    return response.data.exists;
  }

  /**
   * Delete blob by hash
   * @param hash - Blob hash (sha256:...)
   * @param basePath - Optional custom blob storage path
   * @returns true if deleted, false if not found
   */
  async blobDelete(hash: string, basePath?: string): Promise<boolean> {
    const response = await this.sendCommand<BlobDeleteResult>('blob/delete', {
      hash,
      base_path: basePath
    });

    if (response.status !== 'ok') {
      const errorMsg = response.status === 'error' ? response.message : 'Blob delete failed';
      throw new Error(errorMsg);
    }

    return response.data.deleted;
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
    const response = await this.sendCommand<BlobStatsResult>('blob/stats', {
      base_path: basePath
    });

    if (response.status !== 'ok') {
      const errorMsg = response.status === 'error' ? response.message : 'Blob stats failed';
      throw new Error(errorMsg);
    }

    // Map snake_case wire format (from Rust) to camelCase return type
    return {
      totalBlobs: response.data.total_blobs,
      totalCompressedBytes: response.data.total_compressed_bytes,
      shardCount: response.data.shard_count,
      basePath: response.data.base_path,
    };
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

  // =========================================================================
  // Raw SQL Query - For complex queries with JOINs
  // =========================================================================

  /**
   * Execute a raw SQL SELECT query via Rust worker
   *
   * Use for complex queries (JOINs, aggregations) that can't be expressed
   * via the standard query() method. Results are returned as raw rows,
   * caller is responsible for transformation.
   *
   * Security: Only SELECT queries allowed - Rust worker rejects modifications.
   *
   * @param sql - Raw SQL query (SELECT only)
   * @returns Array of row objects with snake_case column names
   */
  async rawQuery<T = Record<string, any>>(sql: string): Promise<{
    items: T[];
    count: number;
  }> {
    try {
      await this.ensureConnected();
    } catch (error: any) {
      throw new Error(`Connection failed: ${error.message}`);
    }

    const response = await this.sendCommand<DataQueryResult>('data/query', {
      handle: this.adapterHandle,
      sql
    });

    if (response.status !== 'ok') {
      const errorMsg = response.status === 'error' ? response.message : 'Raw query failed';
      throw new Error(errorMsg);
    }

    return {
      items: (response.data.items || []) as T[],
      count: response.data.count || 0
    };
  }

  /**
   * Execute a raw SQL SELECT query and transform results to camelCase
   *
   * Same as rawQuery() but converts column names from snake_case to camelCase.
   *
   * @param sql - Raw SQL query (SELECT only)
   * @returns Array of row objects with camelCase keys
   */
  async rawQueryCamelCase<T = Record<string, any>>(sql: string): Promise<{
    items: T[];
    count: number;
  }> {
    const result = await this.rawQuery(sql);

    return {
      items: result.items.map(row => this.toCamelCaseObject(row as Record<string, any>) as T),
      count: result.count
    };
  }

  /**
   * Close all pool connections to Rust worker
   */
  async close(): Promise<void> {
    // Close adapter in Rust first
    if (this.adapterHandle && this.pool.length > 0) {
      try {
        await this.sendCommand('adapter/close', { handle: this.adapterHandle });
        log.info(`Closed SQLite adapter: ${this.adapterHandle}`);
      } catch (error) {
        log.warn(`Failed to close adapter in Rust: ${error}`);
      }
      this.adapterHandle = null;
    }

    // Close all pool connections
    for (const conn of this.pool) {
      if (conn.pendingResponse) {
        clearTimeout(conn.pendingResponse.timeout);
        conn.pendingResponse.reject(new Error('Connection closed'));
        conn.pendingResponse = null;
      }
      if (conn.socket && !conn.socket.destroyed) {
        conn.socket.destroy();
      }
    }
    this.pool = [];

    // Reject all waiters
    for (const waiter of this.waitQueue) {
      // Can't fulfill — they'll get an error when they try to use the connection
    }
    this.waitQueue = [];
  }
}
