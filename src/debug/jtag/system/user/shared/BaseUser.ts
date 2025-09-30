/**
 * BaseUser - Abstract base class for all user types
 *
 * Follows ARCHITECTURE-RULES.md:
 * - Proper abstraction with shared/browser/server pattern
 * - Generic programming with type constraints
 * - Clean inheritance hierarchy
 *
 * Hierarchy:
 * BaseUser (abstract)
 * ├── HumanUser extends BaseUser
 * └── AIUser extends BaseUser (abstract)
 *     ├── AgentUser extends AIUser
 *     └── PersonaUser extends AIUser
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { UserEntity } from '../../data/entities/UserEntity';
import type { UserStateEntity } from '../../data/entities/UserStateEntity';
import type { IUserStateStorage } from '../storage/IUserStateStorage';

/**
 * BaseUser abstract class
 * Every connected client has a User object with entity and state
 */
export abstract class BaseUser {
  /**
   * User database entity (id, displayName, type, etc.)
   */
  public readonly entity: UserEntity;

  /**
   * User state (preferences, open tabs, UI state)
   */
  public readonly state: UserStateEntity;

  /**
   * Storage backend for persisting state
   */
  protected readonly storage: IUserStateStorage;

  constructor(entity: UserEntity, state: UserStateEntity, storage: IUserStateStorage) {
    this.entity = entity;
    this.state = state;
    this.storage = storage;
  }

  /**
   * Get user ID
   */
  get id(): UUID {
    return this.entity.id;
  }

  /**
   * Get user type
   */
  get type(): string {
    return this.entity.type;
  }

  /**
   * Get display name
   */
  get displayName(): string {
    return this.entity.displayName;
  }

  /**
   * Save state to storage backend
   */
  async saveState(): Promise<{ success: boolean; error?: string }> {
    return await this.storage.save(this.state);
  }

  /**
   * Load state from storage backend
   */
  async loadState(): Promise<void> {
    const loaded = await this.storage.load(this.entity.id, this.state.deviceId);
    if (loaded) {
      // Update current state with loaded values
      Object.assign(this.state, loaded);
    }
  }

  /**
   * Delete state from storage backend
   */
  async deleteState(): Promise<{ success: boolean; error?: string }> {
    return await this.storage.delete(this.entity.id, this.state.deviceId);
  }

  /**
   * Get user info for debugging
   */
  toString(): string {
    return `${this.constructor.name}(${this.displayName}, ${this.id.substring(0, 8)}...)`;
  }
}