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
import { UserStateEntity } from '../../../data/entities/UserStateEntity';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import type { DataRecord } from '../../../../daemons/data-daemon/shared/DataStorageAdapter';

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
      // Use DataDaemon static interface (avoids JTAGClient recursion during initialization)
      const existing = await ORM.read<UserStateEntity>(UserStateEntity.collection, state.id);

      if (existing) {
        // Update existing state
        await ORM.update<UserStateEntity>(UserStateEntity.collection, state.id, state);
      } else {
        // Create new state
        await ORM.store<UserStateEntity>(UserStateEntity.collection, state);
      }

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
      // Use DataDaemon static interface (avoids JTAGClient recursion during initialization)
      const result = await ORM.query<UserStateEntity>({
        collection: UserStateEntity.collection,
        filter: { userId, deviceId },
        limit: 1
      });

      if (result.success && result.data && result.data.length > 0) {
        return result.data[0].data;
      }

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
      // Use DataDaemon static interface (avoids JTAGClient recursion during initialization)
      const result = await ORM.query<UserStateEntity>({
        collection: UserStateEntity.collection,
        filter: { userId, deviceId },
        limit: 1
      });

      if (result.success && result.data && result.data.length > 0) {
        const stateId = result.data[0].id;
        await ORM.remove(UserStateEntity.collection, stateId);
      }

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