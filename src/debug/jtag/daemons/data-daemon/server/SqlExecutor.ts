/**
 * SqlExecutor - Common interface for SQL execution
 *
 * This interface defines the contract that both SqliteRawExecutor
 * and RustSqliteExecutor must implement, allowing them to be used
 * interchangeably by the storage adapter and managers.
 */

export interface SqlExecutor {
  /**
   * Execute SQL query (SELECT) and return all rows
   */
  runSql(sql: string, params?: any[]): Promise<any[]>;

  /**
   * Execute SQL statement (INSERT, UPDATE, DELETE) and return result metadata
   */
  runStatement(sql: string, params?: any[]): Promise<{ lastID?: number; changes: number }>;

  /**
   * Update database instance (used when reconnecting)
   * @param db - Database instance or path (null to disconnect)
   */
  setDatabase(db: any): void;
}
