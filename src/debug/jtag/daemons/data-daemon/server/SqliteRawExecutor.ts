/**
 * SqliteRawExecutor - Low-level SQL execution utilities
 *
 * Handles raw SQL query and statement execution with promise-based interface.
 * Extracted from SqliteStorageAdapter for clean separation of concerns.
 */

import sqlite3 from 'sqlite3';

export class SqliteRawExecutor {
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
   */
  async runStatement(sql: string, params: any[] = []): Promise<{ lastID?: number; changes: number }> {
    console.log(`🔧 SQLite RUNSTATEMENT DEBUG: Executing SQL:`, { sql: sql.trim(), params });
    if (!this.db) {
      console.error(`❌ SQLite RUNSTATEMENT DEBUG: Database not initialized!`);
      throw new Error('SQLite database not initialized');
    }

    return new Promise((resolve, reject) => {
      this.db!.run(sql, params, function(err) {
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
