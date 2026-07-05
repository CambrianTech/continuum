/**
 * UserIdManager - Persistent User ID Management
 *
 * Solves the critical architecture confusion between Session ID and User ID:
 * - Session ID: Changes each browser restart/connection
 * - User ID: Persistent identity that survives across sessions ("me")
 *
 * Uses LocalStorage for browser-side persistence. The actual user identity
 * is resolved by the seed system via SystemIdentity (OS username).
 */

import type { UUID } from '../core/types/CrossPlatformUUID';

export class UserIdManager {
  private static readonly STORAGE_KEY = 'continuum_user_id';
  private static instance: UserIdManager | null = null;

  private currentUserId: UUID | null = null;

  private constructor() {}

  public static getInstance(): UserIdManager {
    if (!UserIdManager.instance) {
      UserIdManager.instance = new UserIdManager();
    }
    return UserIdManager.instance;
  }

  /**
   * Get or create persistent User ID for "me"
   * This User ID survives browser restarts, unlike Session ID
   */
  public async getCurrentUserId(): Promise<UUID> {
    if (this.currentUserId) {
      return this.currentUserId;
    }

    // Try to load from LocalStorage first
    if (typeof localStorage !== 'undefined') {
      const stored = localStorage.getItem(UserIdManager.STORAGE_KEY);
      if (stored) {
        this.currentUserId = stored as UUID;
        return this.currentUserId;
      }
    }

    // No stored ID — the session system will assign one on connect
    // Return a temporary placeholder; the real ID comes from the server
    return '' as UUID;
  }

  /**
   * Set the current user ID (called by session system after server resolves identity)
   */
  public setCurrentUserId(userId: UUID): void {
    this.currentUserId = userId;
    this.storeCurrentUserId(userId);
  }

  /**
   * Store User ID in LocalStorage for persistence
   */
  private storeCurrentUserId(userId: UUID): void {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(UserIdManager.STORAGE_KEY, userId);
    }
  }

  /**
   * Check if a message belongs to current user based on User ID (not Session ID)
   */
  public async isCurrentUserMessage(senderId: string): Promise<boolean> {
    const currentUserId = await this.getCurrentUserId();
    return senderId === currentUserId;
  }

  /**
   * Reset User ID (for testing or user switching)
   */
  public resetUserId(): void {
    this.currentUserId = null;
    if (typeof localStorage !== 'undefined') {
      localStorage.removeItem(UserIdManager.STORAGE_KEY);
    }
  }
}

// Export singleton instance for easy access
export const userIdManager = UserIdManager.getInstance();
