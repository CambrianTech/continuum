/**
 * RustSqliteExecutor - Rust Worker SQL Execution Bridge
 *
 * Replaces SqliteRawExecutor with Rust-backed SQL execution.
 * Provides identical interface but routes SQL to Rust worker for:
 * - Faster execution via rusqlite
 * - Concurrent query handling
 * - Connection pooling
 *
 * Architecture:
 *   TypeScript (DataDaemon) → builds SQL from decorators
 *   RustSqliteExecutor → sends SQL to Rust worker
 *   Rust Worker → executes SQL via rusqlite
 *   RustSqliteExecutor → receives rows, returns to TypeScript
 */

import * as net from 'net';
import { v4 as uuidv4 } from 'uuid';
import { Logger } from '../../../system/core/logging/Logger';
import type { SqlExecutor } from './SqlExecutor';

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
