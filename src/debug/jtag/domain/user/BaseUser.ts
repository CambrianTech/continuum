/**
 * BaseUser - Abstract base class for all system citizens
 *
 * Follows "typing like Rust - strict, explicit, and predictable" principles.
 * Enforces compile-time validation and provides elegant extensibility.
 */

import { generateUUID, type UUID } from '../../system/core/types/CrossPlatformUUID';

/**
 * User Citizen Types - Actual entities in the system
 * System users handle automated system messages, instructions, and announcements
 */
export type UserCitizenType = 'human' | 'ai' | 'persona' | 'system';

/**
 * Core User Data - Shared across all citizen types
 */
export interface BaseUserData {
  readonly userId: UUID;
  readonly sessionId: UUID;
  readonly displayName: string;
  readonly citizenType: UserCitizenType;
  readonly capabilities: readonly string[];
  readonly createdAt: string;
  readonly lastActiveAt: string;
  readonly preferences: Record<string, unknown>;
  readonly isOnline: boolean;
}

/**
 * Abstract Base User - Foundation for all system citizens
 *
 * Provides common functionality while enforcing type-safe extensions.
 * Each citizen type extends this with specialized behavior and adapters.
 */
export abstract class BaseUser implements BaseUserData {
  protected constructor(protected readonly data: BaseUserData) {}

  // Immutable accessors
  get userId(): UUID { return this.data.userId; }
  get sessionId(): UUID { return this.data.sessionId; }
  get displayName(): string { return this.data.displayName; }
  get citizenType(): UserCitizenType { return this.data.citizenType; }
  get capabilities(): readonly string[] { return this.data.capabilities; }
  get createdAt(): string { return this.data.createdAt; }
  get lastActiveAt(): string { return this.data.lastActiveAt; }
  get preferences(): Record<string, unknown> { return this.data.preferences; }
  get isOnline(): boolean { return this.data.isOnline; }

  /**
   * Abstract methods - Must be implemented by concrete citizen types
   */
  abstract getAdapterType(): string;

  /**
   * Shared behaviors - Common to all citizens
   */
  updateOnlineStatus(isOnline: boolean): this {
    const newData = {
      ...this.data,
      isOnline,
      lastActiveAt: isOnline ? new Date().toISOString() : this.data.lastActiveAt
    };
    return this.createInstance(newData);
  }

  updateDisplayName(displayName: string): this {
    if (!displayName.trim()) {
      throw new Error('Display name cannot be empty');
    }

    const newData = {
      ...this.data,
      displayName: displayName.trim()
    };
    return this.createInstance(newData);
  }

  hasCapability(capability: string): boolean {
    return this.capabilities.includes(capability);
  }

  getDisplayNameOrFallback(): string {
    if (this.displayName && this.displayName.trim()) {
      return this.displayName;
    }
    return `Session-${this.sessionId.substring(0, 8)}`;
  }

  /**
   * Type guards - Type-safe citizen classification
   */
  isHuman(): this is HumanUser {
    return this.citizenType === 'human';
  }

  isAI(): this is AIUser {
    return this.citizenType === 'ai';
  }

  isSystem(): this is SystemUser {
    return this.citizenType === 'system';
  }

  /**
   * Data serialization - For storage and transport
   */
  toData(): BaseUserData {
    return this.data;
  }

  /**
   * Factory method - Create appropriate User instance from stored data
   * Note: Concrete implementations must be imported and registered
   */
  static fromData(data: BaseUserData): BaseUser {
    throw new Error('BaseUser.fromData() must be called via concrete user factory');
  }

  toString(): string {
    return `${this.constructor.name}(${this.userId}, "${this.displayName}")`;
  }

  /**
   * Abstract factory method - Each concrete class implements its own construction
   */
  protected abstract createInstance(data: BaseUserData): this;
}

/**
 * Forward declarations for type guards
 * Actual implementations in their respective files
 */
export interface HumanUser extends BaseUser {
  readonly citizenType: 'human';
}

export interface AIUser extends BaseUser {
  readonly citizenType: 'ai';
}

export interface SystemUser extends BaseUser {
  readonly citizenType: 'system';
}