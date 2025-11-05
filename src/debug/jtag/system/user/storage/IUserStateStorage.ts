/**
 * User State Storage Interface
 *
 * Abstraction for persisting UserState across different storage backends.
 * Follows ARCHITECTURE-RULES.md: Generic programming with proper abstractions.
 *
 * Implementations:
 * - LocalStorageStateBackend (browser)
 * - SQLiteStateBackend (persona, test)
 * - MemoryStateBackend (agent, ephemeral)
 */

import type { UserStateEntity } from '../../data/entities/UserStateEntity';
import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Storage backend interface for UserState persistence
 * All implementations must support save/load/delete operations
 */
export interface IUserStateStorage {
  /**
   * Save UserState to storage backend
   * @param state - UserState entity to persist
   * @returns Success status
   */
  save(state: UserStateEntity): Promise<{ success: boolean; error?: string }>;

  /**
   * Load UserState from storage backend
   * @param userId - User ID to load state for
   * @param deviceId - Device ID for multi-device support
   * @returns UserState entity or null if not found
   */
  load(userId: UUID, deviceId: string): Promise<UserStateEntity | null>;

  /**
   * Delete UserState from storage backend
   * @param userId - User ID to delete state for
   * @param deviceId - Device ID
   * @returns Success status
   */
  delete(userId: UUID, deviceId: string): Promise<{ success: boolean; error?: string }>;

  /**
   * Check if storage backend is available
   * @returns True if storage is accessible
   */
  isAvailable(): boolean;
}