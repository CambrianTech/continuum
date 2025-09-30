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
  constructor(entity: UserEntity, state: UserStateEntity, storage: IUserStateStorage) {
    super(entity, state, storage);

    // Validate that entity type is AI-related
    if (entity.type !== 'agent' && entity.type !== 'persona') {
      throw new Error(`AIUser requires entity.type='agent' or 'persona', got '${entity.type}'`);
    }
  }

  /**
   * AI-specific capabilities
   */
  get isAI(): boolean {
    return true;
  }

  /**
   * Check if AI can auto-respond
   */
  get canAutoRespond(): boolean {
    return this.entity.capabilities?.autoResponds ?? false;
  }
}