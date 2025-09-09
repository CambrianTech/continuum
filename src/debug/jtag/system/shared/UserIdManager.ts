/**
 * UserIdManager - Persistent User ID Management
 * 
 * Solves the critical architecture confusion between Session ID and User ID:
 * - Session ID: Changes each browser restart/connection
 * - User ID: Persistent identity that survives across sessions ("me")
 * 
 * Uses LocalStorage for browser-side persistence and fake-users.json for user data.
 */

import type { UUID } from '../core/types/CrossPlatformUUID';

export interface User {
  readonly userId: UUID;
  readonly displayName: string;
  readonly email: string;
  readonly role: 'admin' | 'user' | 'moderator' | 'system';
  readonly isOnline: boolean;
  readonly createdAt: string;
  readonly lastSeen: string;
  readonly avatar: string;
  readonly status: 'active' | 'away' | 'busy' | 'offline';
  readonly preferences: {
    theme: 'dark' | 'light' | 'auto';
    notifications: boolean;
    sounds: boolean;
  };
}

export interface FakeUsersData {
  version: string;
  lastUpdated: string;
  description: string;
  users: User[];
  defaultCurrentUser: {
    userId: UUID;
    note: string;
  };
}

export class UserIdManager {
  private static readonly STORAGE_KEY = 'continuum_user_id';
  private static readonly FAKE_USERS_PATH = 'data/fake-users.json';
  private static instance: UserIdManager | null = null;
  
  private currentUserId: UUID | null = null;
  private fakeUsersData: FakeUsersData | null = null;

  private constructor() {
    // Private constructor for singleton pattern
  }

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
        console.log(`🔧 CLAUDE-USER-ID-DEBUG: Loaded persistent User ID from LocalStorage: ${stored}`);
        this.currentUserId = stored as UUID;
        return this.currentUserId;
      }
    }

    // If not in storage, try to load default from fake-users.json
    await this.loadFakeUsersData();
    if (this.fakeUsersData?.defaultCurrentUser?.userId) {
      this.currentUserId = this.fakeUsersData.defaultCurrentUser.userId;
      this.storeCurrentUserId(this.currentUserId);
      console.log(`🔧 CLAUDE-USER-ID-DEBUG: Using default User ID from fake-users.json: ${this.currentUserId}`);
      return this.currentUserId;
    }

    // Fallback: Generate a persistent User ID and store it
    this.currentUserId = 'user-joel-12345' as UUID; // Use the same ID as in fake-users.json
    this.storeCurrentUserId(this.currentUserId);
    console.log(`🔧 CLAUDE-USER-ID-DEBUG: Generated fallback persistent User ID: ${this.currentUserId}`);
    return this.currentUserId;
  }

  /**
   * Store User ID in LocalStorage for persistence
   */
  private storeCurrentUserId(userId: UUID): void {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(UserIdManager.STORAGE_KEY, userId);
      console.log(`🔧 CLAUDE-USER-ID-DEBUG: Stored User ID in LocalStorage: ${userId}`);
    }
  }

  /**
   * Get user data by User ID
   */
  public async getUserById(userId: UUID): Promise<User | null> {
    await this.loadFakeUsersData();
    if (!this.fakeUsersData) {
      return null;
    }

    return this.fakeUsersData.users.find(user => user.userId === userId) || null;
  }

  /**
   * Get current user data (for "me")
   */
  public async getCurrentUser(): Promise<User | null> {
    const userId = await this.getCurrentUserId();
    return this.getUserById(userId);
  }

  /**
   * Load fake users data from JSON file
   */
  private async loadFakeUsersData(): Promise<void> {
    if (this.fakeUsersData) {
      return; // Already loaded
    }

    try {
      // In browser environment, we need to load via JTAG command
      if (typeof window !== 'undefined' && (window as any).jtag) {
        const jtag = (window as any).jtag;
        const result = await jtag.commands['file/load']({
          filePath: UserIdManager.FAKE_USERS_PATH
        });
        
        if (result.success && result.content) {
          this.fakeUsersData = JSON.parse(result.content);
          console.log(`🔧 CLAUDE-USER-ID-DEBUG: Loaded fake users data from ${UserIdManager.FAKE_USERS_PATH} (${this.fakeUsersData?.users?.length || 0} users)`);
        }
      } else {
        // In server environment, try to load directly via filesystem
        try {
          const fs = eval('require')('fs');
          const path = eval('require')('path');
          
          // Construct absolute path from current working directory
          const fullPath = path.resolve(process.cwd(), UserIdManager.FAKE_USERS_PATH);
          console.log(`🔧 CLAUDE-USER-ID-DEBUG: Trying to load fake users from: ${fullPath}`);
          
          if (fs.existsSync(fullPath)) {
            const content = fs.readFileSync(fullPath, 'utf8');
            this.fakeUsersData = JSON.parse(content);
            console.log(`🔧 CLAUDE-USER-ID-DEBUG: Loaded fake users data from filesystem (${this.fakeUsersData?.users?.length || 0} users)`);
          } else {
            console.warn(`⚠️ UserIdManager: Fake users file not found at ${fullPath}`);
          }
        } catch (fsError) {
          console.warn('⚠️ UserIdManager: Cannot load fake users data - no filesystem access:', fsError);
        }
      }
    } catch (error) {
      console.error('❌ UserIdManager: Failed to load fake users data:', error);
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
    console.log('🔧 CLAUDE-USER-ID-DEBUG: Reset User ID - will generate new on next access');
  }
}

// Export singleton instance for easy access
export const userIdManager = UserIdManager.getInstance();