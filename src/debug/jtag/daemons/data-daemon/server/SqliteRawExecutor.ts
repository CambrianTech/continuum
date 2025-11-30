/**
 * SqliteRawExecutor - Low-level SQL execution utilities
 *
 * Handles raw SQL query and statement execution with promise-based interface.
 * Extracted from SqliteStorageAdapter for clean separation of concerns.
 */

import sqlite3 from 'sqlite3';

export class SqliteRawExecutor {
  /**
   * Execute SQL query (SELECT) and return all rows
   * @param db - Database connection from pool
   * @param sql - SQL query to execute
   * @param params - Query parameters
   */
  async runSql(db: sqlite3.Database, sql: string, params: any[] = []): Promise<any[]> {
    return new Promise((resolve, reject) => {
      db.all(sql, params, (err, rows) => {
        if (err) {
          console.error('❌ SQLite Query Error:', err.message);
          console.error('SQL:', sql);
          console.error('Params:', params);
          reject(err);
        } else {
          resolve(rows || []);
        }
      });
    });
  }

  /**
   * Execute SQL statement (INSERT, UPDATE, DELETE) and return result metadata
   * @param db - Database connection from pool
   * @param sql - SQL statement to execute
   * @param params - Statement parameters
   */
  async runStatement(db: sqlite3.Database, sql: string, params: any[] = []): Promise<{ lastID?: number; changes: number }> {
    console.log(`🔧 SQLite RUNSTATEMENT DEBUG: Executing SQL:`, { sql: sql.trim(), params });

    return new Promise((resolve, reject) => {
      db.run(sql, params, function(err) {
        if (err) {
          console.error('❌ SQLite Statement Error:', err.message);
          console.error('SQL:', sql);
          console.error('Params:', params);
          reject(err);
        } else {
          const result = { lastID: this.lastID, changes: this.changes };
          console.log(`✅ SQLite RUNSTATEMENT DEBUG: Success:`, result);
          resolve(result);
        }
      });
    });
  }
}
