/**
 * User - Core domain object for users and sessions
 * 
 * Immutable data class with factory methods and validation.
 * "typing like Rust - strict, explicit, and predictable"
 */

import { generateUUID, type UUID } from '../../system/core/types/CrossPlatformUUID';

export interface UserData {
  readonly userId: UUID;
  readonly sessionId: UUID;
  readonly displayName: string;
  readonly userType: 'human' | 'ai' | 'system';
  readonly capabilities: readonly string[];
  readonly createdAt: string;
  readonly lastActiveAt: string;
  readonly preferences: Record<string, unknown>;
  readonly isOnline: boolean;
}

export class User implements UserData {
  private constructor(private readonly data: UserData) {}

  get userId(): UUID { return this.data.userId; }
  get sessionId(): UUID { return this.data.sessionId; }
  get displayName(): string { return this.data.displayName; }
  get userType(): 'human' | 'ai' | 'system' { return this.data.userType; }
  get capabilities(): readonly string[] { return this.data.capabilities; }
  get createdAt(): string { return this.data.createdAt; }
  get lastActiveAt(): string { return this.data.lastActiveAt; }
  get preferences(): Record<string, unknown> { return this.data.preferences; }
  get isOnline(): boolean { return this.data.isOnline; }

  /**
   * Reconstruct from stored data
   */
  static fromData(data: UserData): User {
    return new User(data);
  }

  /**
   * Get data for storage
   */
  toData(): UserData {
    return this.data;
  }

  /**
   * Update online status
   */
  setOnlineStatus(isOnline: boolean): User {
    return new User({
      ...this.data,
      isOnline,
      lastActiveAt: isOnline ? new Date().toISOString() : this.data.lastActiveAt
    });
  }

  /**
   * Update display name
   */
  updateDisplayName(displayName: string): User {
    if (!displayName.trim()) {
      throw new Error('Display name cannot be empty');
    }

    return new User({
      ...this.data,
      displayName: displayName.trim()
    });
  }

  /**
   * Check if user is AI
   */
  isAI(): boolean {
    return this.userType === 'ai';
  }

  /**
   * Check if user is human
   */
  isHuman(): boolean {
    return this.userType === 'human';
  }

  /**
   * Check if user has capability
   */
  hasCapability(capability: string): boolean {
    return this.capabilities.includes(capability);
  }

  /**
   * Get session-based display name fallback
   */
  getDisplayNameOrFallback(): string {
    if (this.displayName && this.displayName.trim()) {
      return this.displayName;
    }
    return `Session-${this.sessionId.substring(0, 8)}`;
  }

  toString(): string {
    return `User(${this.userId}, ${this.userType}, "${this.displayName}")`;
  }
}