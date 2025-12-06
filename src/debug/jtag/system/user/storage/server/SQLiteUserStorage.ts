/**
 * SQLiteUserStorage - SQLite implementation of IUserStorage
 *
 * Manages user's longterm.db for state, memories, and preferences
 * Used by both HumanUser and PersonaUser - no special cases
 */

import type { IUserStorage } from '../shared/IUserStorage';
import Database from 'better-sqlite3';

export class SQLiteUserStorage implements IUserStorage {
  private db: Database.Database | null = null;
  private readonly databasePath: string;

  constructor(databasePath: string) {
    this.databasePath = databasePath;
  }

  /**
   * Initialize database connection and create table if needed
   */
  private async ensureInitialized(): Promise<void> {
    if (this.db) return;

    // Open database
    this.db = new Database(this.databasePath);

    // Create key-value table if it doesn't exist
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS user_storage (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        updated_at INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_updated_at ON user_storage(updated_at);
    `);
  }

  /**
   * Store a value
   */
  async store<T>(key: string, value: T): Promise<{ success: boolean; error?: string }> {
    try {
      await this.ensureInitialized();

      const serialized = JSON.stringify(value);
      const timestamp = Date.now();

      this.db!.prepare(`
        INSERT OR REPLACE INTO user_storage (key, value, updated_at)
        VALUES (?, ?, ?)
      `).run(key, serialized, timestamp);

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Read a value
   */
  async read<T>(key: string): Promise<{ success: boolean; data?: T; error?: string }> {
    try {
      await this.ensureInitialized();

      const row = this.db!.prepare(`
        SELECT value FROM user_storage WHERE key = ?
      `).get(key) as { value: string } | undefined;

      if (!row) {
        return { success: true, data: undefined };
      }

      const data = JSON.parse(row.value) as T;
      return { success: true, data };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Delete a value
   */
  async delete(key: string): Promise<{ success: boolean; error?: string }> {
    try {
      await this.ensureInitialized();

      this.db!.prepare(`
        DELETE FROM user_storage WHERE key = ?
      `).run(key);

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * List all keys
   */
  async listKeys(): Promise<{ success: boolean; keys?: string[]; error?: string }> {
    try {
      await this.ensureInitialized();

      const rows = this.db!.prepare(`
        SELECT key FROM user_storage ORDER BY key
      `).all() as Array<{ key: string }>;

      const keys = rows.map(row => row.key);
      return { success: true, keys };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Check if storage is available
   */
  async isAvailable(): Promise<boolean> {
    try {
      await this.ensureInitialized();
      return this.db !== null;
    } catch {
      return false;
    }
  }

  /**
   * Get database path
   */
  getDatabasePath(): string {
    return this.databasePath;
  }

  /**
   * Close database connection
   */
  async close(): Promise<void> {
    if (this.db) {
      this.db.close();
      this.db = null;
    }
  }
}
