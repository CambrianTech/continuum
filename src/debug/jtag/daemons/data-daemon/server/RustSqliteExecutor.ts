/**
 * RustSqliteExecutor - Rust Worker ORM Bridge
 *
 * Proper ORM abstraction: Sends high-level StorageQuery to Rust worker.
 * Rust worker translates to SQL internally (clean separation of concerns).
 *
 * Architecture:
 *   TypeScript (Application) → StorageQuery { collection, filter, sort, limit }
 *   RustSqliteExecutor → sends query-records message
 *   Rust Worker → translates to SQL, executes via rusqlite
 *   RustSqliteExecutor → receives DataRecords, returns to TypeScript
 *
 * Benefits:
 * - Storage-agnostic application layer (could swap to MongoDB, REST, etc.)
 * - SQL generation in ONE place (Rust adapter)
 * - Type-safe queries
 * - Concurrent query handling via Rust worker
 */

import * as net from 'net';
import { v4 as uuidv4 } from 'uuid';
import { Logger } from '../../../system/core/logging/Logger';
import type { SqlExecutor } from './SqlExecutor';
import type { StorageQuery } from '../shared/DataStorageAdapter';

const log = Logger.create('RustSqliteExecutor', 'sql');

// ============================================================================
// Message Protocol Types (matches Rust messages.rs)
// ============================================================================

interface JTAGRequest<T> {
  id: string;
  type: string;
  timestamp: string;
  payload: T;
}

interface JTAGResponse<T> {
  id: string;
  type: string;
  timestamp: string;
  payload: T;
  requestId: string;
  success: boolean;
  error?: string;
  errorType?: 'validation' | 'internal' | 'notFound';
}

interface SqlQueryPayload {
  sql: string;
  params: any[];
  dbPath?: string;
  dbHandle?: string;
}

interface SqlQueryResult {
  rows: any[];
}

interface SqlExecutePayload {
  sql: string;
  params: any[];
  dbPath?: string;
  dbHandle?: string;
}

interface SqlExecuteResult {
  changes: number;
  lastInsertId?: number;
}

// ORM-level payload types (proper abstraction)
interface QueryRecordsPayload {
  handle: string;
  query: {
    collection: string;
    filter?: Record<string, any>;
    sort?: Array<{ field: string; direction: string }>;
    limit?: number;
    offset?: number;
    cursor?: {
      field: string;
      value: any;
      direction: 'after' | 'before';
    };
  };
}

interface QueryRecordsResult {
  records: any[];
  totalCount?: number;
}

interface CountRecordsPayload {
  handle: string;
  query: {
    collection: string;
    filter?: Record<string, any>;
  };
}

interface CountRecordsResult {
  count: number;
}

// ============================================================================
// Rust SQL Executor
// ============================================================================

export class RustSqliteExecutor implements SqlExecutor {
  private socketPath: string;
  private dbPath?: string;
  private dbHandle?: string;

  constructor(dbPath?: string, dbHandle?: string, socketPath: string = '/tmp/data-worker.sock') {
    this.socketPath = socketPath;
    this.dbPath = dbPath;
    this.dbHandle = dbHandle;
  }

  /**
   * Update database path (implements SqlExecutor interface)
   */
  setDatabase(dbPath: string | null): void {
    this.dbPath = dbPath || undefined;
  }

  /**
   * Execute SQL query (SELECT) and return all rows
   * Replacement for SqliteRawExecutor.runSql()
   *
   * NOTE: This is legacy/transitional - should use queryRecords() for ORM queries
   */
  async runSql(sql: string, params: any[] = []): Promise<any[]> {
    const payload: SqlQueryPayload = {
      sql,
      params,
      dbPath: this.dbPath,
      dbHandle: this.dbHandle,
    };

    const result = await this.sendMessage<SqlQueryPayload, SqlQueryResult>('sql/query', payload);
    return result.rows;
  }

  /**
   * Execute SQL statement (INSERT, UPDATE, DELETE) and return result metadata
   * Replacement for SqliteRawExecutor.runStatement()
   *
   * NOTE: This is legacy/transitional - should use create/update/delete methods for ORM operations
   */
  async runStatement(sql: string, params: any[] = []): Promise<{ lastID?: number; changes: number }> {
    log.debug('Executing SQL:', { sql: sql.trim(), params });

    const payload: SqlExecutePayload = {
      sql,
      params,
      dbPath: this.dbPath,
      dbHandle: this.dbHandle,
    };

    const result = await this.sendMessage<SqlExecutePayload, SqlExecuteResult>('sql/execute', payload);

    const returnValue = {
      lastID: result.lastInsertId,
      changes: result.changes,
    };

    log.debug('Statement success:', returnValue);
    return returnValue;
  }

  /**
   * Query records using ORM pattern (proper abstraction)
   *
   * Sends high-level StorageQuery to Rust worker, which:
   * 1. Translates filter operators to SQL WHERE clauses
   * 2. Translates sort to SQL ORDER BY
   * 3. Executes via rusqlite connection pool
   * 4. Returns raw data records
   *
   * This is the CORRECT abstraction - storage-agnostic application layer.
   */
  async queryRecords(query: StorageQuery): Promise<any[]> {
    const payload: QueryRecordsPayload = {
      handle: this.dbHandle || 'default',
      query: {
        collection: query.collection,
        filter: query.filter,
        sort: query.sort,
        limit: query.limit,
        offset: query.offset,
        cursor: query.cursor,
      },
    };

    log.debug('ORM query:', {
      collection: query.collection,
      filter: query.filter,
      cursor: query.cursor,
      limit: query.limit
    });

    const result = await this.sendMessage<QueryRecordsPayload, QueryRecordsResult>('query-records', payload);

    log.debug(`ORM query returned ${result.records.length} records`);
    return result.records;
  }

  /**
   * Count records using ORM pattern (proper abstraction)
   *
   * Sends high-level StorageQuery to Rust worker, which translates to COUNT(*) SQL.
   * Much more efficient than fetching all records and counting in TypeScript.
   */
  async countRecords(query: StorageQuery): Promise<number> {
    const payload: CountRecordsPayload = {
      handle: this.dbHandle || 'default',
      query: {
        collection: query.collection,
        filter: query.filter,
      },
    };

    log.debug('ORM count:', { collection: query.collection, filter: query.filter });

    const result = await this.sendMessage<CountRecordsPayload, CountRecordsResult>('count-records', payload);

    log.debug(`ORM count returned ${result.count}`);
    return result.count;
  }

  /**
   * Read single record by ID using ORM pattern
   *
   * Simple ID lookup - sends to Rust worker's read-record handler.
   */
  async readRecord(collection: string, id: string): Promise<any> {
    const payload = {
      handle: this.dbHandle || 'default',
      collection,
      id,
    };

    log.debug('ORM read:', { collection, id });

    const result = await this.sendMessage<any, { record: any }>('read-record', payload);

    log.debug('ORM read success');
    return result.record;
  }

  // ==========================================================================
  // Internal Communication
  // ==========================================================================

  /**
   * Send message to Rust worker and wait for response
   */
  private async sendMessage<P, R>(type: string, payload: P): Promise<R> {
    return new Promise((resolve, reject) => {
      const socket = net.createConnection(this.socketPath);
      const requestId = uuidv4();
      let responseData = '';

      const request: JTAGRequest<P> = {
        id: requestId,
        type,
        timestamp: new Date().toISOString(),
        payload,
      };

      socket.on('connect', () => {
        const message = JSON.stringify(request) + '\n';
        socket.write(message);
      });

      socket.on('data', (data) => {
        responseData += data.toString();

        // Check if we have a complete line-delimited message
        const newlineIndex = responseData.indexOf('\n');
        if (newlineIndex !== -1) {
          const line = responseData.substring(0, newlineIndex);
          try {
            const response: JTAGResponse<R> = JSON.parse(line);

            if (response.success) {
              resolve(response.payload);
            } else {
              const error = new Error(response.error || 'Unknown error from Rust worker');
              log.error('Rust worker error:', response.error);
              reject(error);
            }
          } catch (err) {
            log.error('Failed to parse response:', err);
            reject(new Error('Invalid response from Rust worker'));
          } finally {
            socket.end();
          }
        }
      });

      socket.on('error', (err) => {
        log.error('Socket error:', err);
        reject(new Error(`Failed to connect to Rust worker at ${this.socketPath}: ${err.message}`));
      });

      socket.on('timeout', () => {
        socket.destroy();
        reject(new Error('Rust worker request timeout'));
      });

      socket.setTimeout(30000); // 30 second timeout
    });
  }
}
