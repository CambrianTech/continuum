/**
 * HumanUser - Represents a human user (browser or test client)
 *
 * Follows ARCHITECTURE-RULES.md:
 * - Clean inheritance from BaseUser
 * - Environment-agnostic (shared directory)
 * - Storage backend injected (no hardcoded localStorage)
 */

import { BaseUser } from './BaseUser';
import type { UserEntity } from '../../data/entities/UserEntity';
import type { UserStateEntity } from '../../data/entities/UserStateEntity';
import type { IUserStateStorage } from '../storage/IUserStateStorage';

/**
 * HumanUser class for human users
 * Used by both browser clients and test clients
 */
export class HumanUser extends BaseUser {
  constructor(entity: UserEntity, state: UserStateEntity, storage: IUserStateStorage) {
    super(entity, state, storage);

    // Validate that entity type is 'human'
    if (entity.type !== 'human') {
      throw new Error(`HumanUser requires entity.type='human', got '${entity.type}'`);
    }
  }

  /**
   * Human-specific capabilities
   */
  get isHuman(): boolean {
    return true;
  }

  /**
   * Check if user is authenticated (has passkey)
   */
  get isAuthenticated(): boolean {
    // TODO: Implement passkey authentication check
    return false;
  }
}