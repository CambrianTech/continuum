/**
 * SqliteRawExecutor - Low-level SQL execution utilities
 *
 * Handles raw SQL query and statement execution with promise-based interface.
 * Extracted from SqliteStorageAdapter for clean separation of concerns.
 */

import sqlite3 from 'sqlite3';
import { Logger } from '../../../system/core/logging/Logger';
import type { SqlExecutor } from './SqlExecutor';

const log = Logger.create('SqliteRawExecutor', 'sql');

export class SqliteRawExecutor implements SqlExecutor {
  constructor(private db: sqlite3.Database | null) {}

  /**
   * Update database instance (used when reconnecting)
   */
  setDatabase(db: sqlite3.Database | null): void {
    this.db = db;
  }

  /**
   * Execute SQL query (SELECT) and return all rows
   */
  async runSql(sql: string, params: any[] = []): Promise<any[]> {
    if (!this.db) {
      throw new Error('SQLite database not initialized');
    }

    return new Promise((resolve, reject) => {
      this.db!.all(sql, params, (err, rows) => {
        if (err) {
          log.error('SQLite Query Error:', err.message);
          log.error('SQL:', sql);
          log.error('Params:', params);
          reject(err);
        } else {
          resolve(rows || []);
        }
      });
    });
  }

  /**
   * Execute SQL statement (INSERT, UPDATE, DELETE) and return result metadata
   */
  async runStatement(sql: string, params: any[] = []): Promise<{ lastID?: number; changes: number }> {
    log.debug('Executing SQL:', { sql: sql.trim(), params });
    if (!this.db) {
      log.error('Database not initialized!');
      throw new Error('SQLite database not initialized');
    }

    return new Promise((resolve, reject) => {
      this.db!.run(sql, params, function(err) {
        if (err) {
          log.error('SQLite Statement Error:', err.message);
          log.error('SQL:', sql);
          log.error('Params:', params);
          reject(err);
        } else {
          const result = { lastID: this.lastID, changes: this.changes };
          log.debug('Statement success:', result);
          resolve(result);
        }
      });
    });
  }
}
