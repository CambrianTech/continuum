/**
 * SqliteTransactionManager - Transaction lifecycle management
 *
 * Handles BEGIN, COMMIT, ROLLBACK with nested transaction prevention.
 * Extracted from SqliteStorageAdapter for clean separation of concerns.
 */

import type { SqlExecutor } from './SqlExecutor';

export class SqliteTransactionManager {
  private inTransaction: boolean = false;

  constructor(private executor: SqlExecutor) {}

  /**
   * Begin a database transaction
   */
  private async beginTransaction(): Promise<void> {
    await this.executor.runStatement('BEGIN TRANSACTION');
  }

  /**
   * Commit a database transaction
   */
  private async commitTransaction(): Promise<void> {
    await this.executor.runStatement('COMMIT');
  }

  /**
   * Rollback a database transaction
   */
  private async rollbackTransaction(): Promise<void> {
    await this.executor.runStatement('ROLLBACK');
  }

  /**
   * Execute operations within a transaction for atomic consistency
   * Supports nested calls by only creating transaction if not already in one
   */
  async withTransaction<T>(operation: () => Promise<T>): Promise<T> {
    // If already in a transaction, just execute the operation without nesting
    if (this.inTransaction) {
      return await operation();
    }

    // Start new transaction
    this.inTransaction = true;
    await this.beginTransaction();

    try {
      const result = await operation();
      await this.commitTransaction();
      return result;
    } catch (error) {
      await this.rollbackTransaction();
      console.error(`❌ SQLite: Transaction rolled back due to error:`, error);
      throw error;
    } finally {
      this.inTransaction = false;
    }
  }

  /**
   * Check if currently in a transaction
   */
  isInTransaction(): boolean {
    return this.inTransaction;
  }
}
