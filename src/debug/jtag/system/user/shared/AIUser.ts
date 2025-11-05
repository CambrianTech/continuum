/**
 * AIUser - Abstract base class for AI users
 *
 * Follows ARCHITECTURE-RULES.md:
 * - Clean inheritance hierarchy
 * - Abstract base for different AI types
 */

import { BaseUser } from './BaseUser';
import type { UserEntity } from '../../data/entities/UserEntity';
import type { UserStateEntity } from '../../data/entities/UserStateEntity';
import type { IUserStateStorage } from '../storage/IUserStateStorage';

/**
 * AIUser abstract class
 * Base for AgentUser and PersonaUser
 */
export abstract class AIUser extends BaseUser {
  get isAI(): boolean {
    return true;
  }

  get canAutoRespond(): boolean {
    return this.entity.capabilities?.autoResponds ?? false;
  }
}