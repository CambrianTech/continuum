/**
 * AgentUser - External AI portal connections (Claude, GPT, etc.)
 *
 * Follows ARCHITECTURE-RULES.md:
 * - Clean inheritance from AIUser
 * - Ephemeral state (in-memory storage)
 */

import { AIUser } from './AIUser';
import type { UserEntity } from '../../data/entities/UserEntity';
import type { UserStateEntity } from '../../data/entities/UserStateEntity';
import type { IUserStateStorage } from '../storage/IUserStateStorage';

/**
 * AgentUser class for external AI agents
 * Used for Claude, GPT, and other external AI portal connections
 */
export class AgentUser extends AIUser {
  constructor(entity: UserEntity, state: UserStateEntity, storage: IUserStateStorage) {
    super(entity, state, storage);

    // Validate that entity type is 'agent'
    if (entity.type !== 'agent') {
      throw new Error(`AgentUser requires entity.type='agent', got '${entity.type}'`);
    }
  }

  /**
   * Agent-specific identification
   */
  get isAgent(): boolean {
    return true;
  }
}