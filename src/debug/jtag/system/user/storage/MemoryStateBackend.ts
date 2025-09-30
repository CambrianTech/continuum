/**
 * MemoryStateBackend - In-memory storage for ephemeral UserState
 *
 * Used by:
 * - AgentUser (external AI portals)
 * - Test clients (ephemeral state)
 *
 * Follows ARCHITECTURE-RULES.md: Simple, generic, no entity-specific logic
 */

import type { IUserStateStorage } from './IUserStateStorage';
import type { UserStateEntity } from '../../data/entities/UserStateEntity';
import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * In-memory storage backend for UserState
 * State is lost when process exits
 */
export class MemoryStateBackend implements IUserStateStorage {
  private states: Map<string, UserStateEntity> = new Map();

  /**
   * Create storage key from userId + deviceId
   */
  private createKey(userId: UUID, deviceId: string): string {
    return `${userId}:${deviceId}`;
  }

  /**
   * Save UserState to memory
   */
  async save(state: UserStateEntity): Promise<{ success: boolean; error?: string }> {
    try {
      const key = this.createKey(state.userId, state.deviceId);

      // Clone state to prevent external mutations
      const cloned = { ...state };
      this.states.set(key, cloned);

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Load UserState from memory
   */
  async load(userId: UUID, deviceId: string): Promise<UserStateEntity | null> {
    const key = this.createKey(userId, deviceId);
    const state = this.states.get(key);

    if (!state) {
      return null;
    }

    // Clone state to prevent external mutations
    return { ...state };
  }

  /**
   * Delete UserState from memory
   */
  async delete(userId: UUID, deviceId: string): Promise<{ success: boolean; error?: string }> {
    try {
      const key = this.createKey(userId, deviceId);
      this.states.delete(key);
      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Check if storage is available (always true for memory)
   */
  isAvailable(): boolean {
    return true;
  }

  /**
   * Clear all states (useful for testing)
   */
  clear(): void {
    this.states.clear();
  }

  /**
   * Get number of stored states (useful for testing)
   */
  size(): number {
    return this.states.size;
  }
}