/**
 * LocalStorageStateBackend - Browser localStorage storage for UserState
 *
 * Used by:
 * - HumanUser in browser (persistent across sessions)
 *
 * Follows ARCHITECTURE-RULES.md:
 * - Browser-specific code in browser/ directory
 * - No dynamic imports
 * - Generic implementation (no entity-specific logic)
 */

/* global localStorage */

import type { IUserStateStorage } from '../IUserStateStorage';
import type { UserStateEntity } from '../../../data/entities/UserStateEntity';
import type { UUID } from '../../../core/types/CrossPlatformUUID';

/**
 * localStorage storage backend for UserState
 * State persists across browser sessions
 */
export class LocalStorageStateBackend implements IUserStateStorage {
  private readonly storagePrefix = 'continuum-userstate';

  /**
   * Create storage key from userId + deviceId
   */
  private createKey(userId: UUID, deviceId: string): string {
    return `${this.storagePrefix}:${userId}:${deviceId}`;
  }

  /**
   * Save UserState to localStorage
   */
  async save(state: UserStateEntity): Promise<{ success: boolean; error?: string }> {
    try {
      if (!this.isAvailable()) {
        return { success: false, error: 'localStorage not available' };
      }

      const key = this.createKey(state.userId, state.deviceId);
      const serialized = JSON.stringify(state);

      localStorage.setItem(key, serialized);

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Load UserState from localStorage
   */
  async load(userId: UUID, deviceId: string): Promise<UserStateEntity | null> {
    try {
      if (!this.isAvailable()) {
        return null;
      }

      const key = this.createKey(userId, deviceId);
      const serialized = localStorage.getItem(key);

      if (!serialized) {
        return null;
      }

      const parsed = JSON.parse(serialized);

      // Reconstruct Date objects (JSON stringifies Dates as strings)
      if (parsed.createdAt) {
        parsed.createdAt = new Date(parsed.createdAt);
      }
      if (parsed.updatedAt) {
        parsed.updatedAt = new Date(parsed.updatedAt);
      }
      if (parsed.contentState?.lastUpdatedAt) {
        parsed.contentState.lastUpdatedAt = new Date(parsed.contentState.lastUpdatedAt);
      }

      return parsed as UserStateEntity;
    } catch (error) {
      console.error('LocalStorageStateBackend: Failed to load state:', error);
      return null;
    }
  }

  /**
   * Delete UserState from localStorage
   */
  async delete(userId: UUID, deviceId: string): Promise<{ success: boolean; error?: string }> {
    try {
      if (!this.isAvailable()) {
        return { success: false, error: 'localStorage not available' };
      }

      const key = this.createKey(userId, deviceId);
      localStorage.removeItem(key);

      return { success: true };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Check if localStorage is available
   */
  isAvailable(): boolean {
    try {
      if (typeof window === 'undefined' || typeof localStorage === 'undefined') {
        return false;
      }

      // Test if we can actually use localStorage (might be disabled)
      const test = '__storage_test__';
      localStorage.setItem(test, test);
      localStorage.removeItem(test);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Clear all UserState entries from localStorage (useful for testing)
   */
  clearAll(): void {
    if (!this.isAvailable()) {
      return;
    }

    const keys: string[] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (key?.startsWith(this.storagePrefix)) {
        keys.push(key);
      }
    }

    keys.forEach(key => localStorage.removeItem(key));
  }
}