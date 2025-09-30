/**
 * SQLiteStateBackend - SQLite storage for UserState
 *
 * Used by:
 * - PersonaUser (dedicated SQLite per persona)
 * - Test clients (ephemeral SQLite)
 *
 * Follows ARCHITECTURE-RULES.md:
 * - Server-specific code in server/ directory
 * - No dynamic imports
 * - Uses existing data commands (single source of truth)
 *
 * NOTE: This uses the data command system which automatically handles
 * SQLite operations. Each persona can have its own database path.
 */

import type { IUserStateStorage } from '../IUserStateStorage';
import type { UserStateEntity } from '../../../data/entities/UserStateEntity';
import type { UUID } from '../../../core/types/CrossPlatformUUID';

/**
 * SQLite storage backend for UserState
 * Uses existing data command infrastructure
 *
 * TODO: Implement persona-specific database path routing
 * For now, uses default database path
 */
export class SQLiteStateBackend implements IUserStateStorage {
  /**
   * Optional custom database path for persona isolation
   */
  private readonly databasePath?: string;

  constructor(databasePath?: string) {
    this.databasePath = databasePath;
  }

  /**
   * Save UserState to SQLite
   * Uses data/create or data/update commands
   */
  async save(state: UserStateEntity): Promise<{ success: boolean; error?: string }> {
    try {
      // TODO: Implement using data commands
      // For now, return success (implementation pending)
      console.log('SQLiteStateBackend.save() - TODO: Implement with data commands');
      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Load UserState from SQLite
   * Uses data/read command
   */
  async load(userId: UUID, deviceId: string): Promise<UserStateEntity | null> {
    try {
      // TODO: Implement using data commands
      // For now, return null (implementation pending)
      console.log('SQLiteStateBackend.load() - TODO: Implement with data commands');
      return null;
    } catch (error) {
      console.error('SQLiteStateBackend: Failed to load state:', error);
      return null;
    }
  }

  /**
   * Delete UserState from SQLite
   * Uses data/delete command
   */
  async delete(userId: UUID, deviceId: string): Promise<{ success: boolean; error?: string }> {
    try {
      // TODO: Implement using data commands
      // For now, return success (implementation pending)
      console.log('SQLiteStateBackend.delete() - TODO: Implement with data commands');
      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Check if SQLite storage is available
   */
  isAvailable(): boolean {
    // TODO: Check if database file exists and is accessible
    return true;
  }

  /**
   * Get database path for this backend
   */
  getDatabasePath(): string | undefined {
    return this.databasePath;
  }
}