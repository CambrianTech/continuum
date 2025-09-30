/**
 * PersonaUser - AI with dedicated SQLite storage (child process isolation)
 *
 * Follows ARCHITECTURE-RULES.md:
 * - Clean inheritance from AIUser
 * - Dedicated SQLite per persona
 * - Child process isolation
 */

import { AIUser } from './AIUser';
import type { UserEntity } from '../../data/entities/UserEntity';
import type { UserStateEntity } from '../../data/entities/UserStateEntity';
import type { IUserStateStorage } from '../storage/IUserStateStorage';
import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * PersonaUser class for AI personas
 * Each persona has its own SQLite database in .continuum/personas/{id}/state.sqlite
 * Suitable for child process isolation and long-term memory
 */
export class PersonaUser extends AIUser {
  /**
   * Persona-specific identifier (links to persona entity)
   */
  public readonly personaId: UUID;

  constructor(
    entity: UserEntity,
    state: UserStateEntity,
    storage: IUserStateStorage,
    personaId: UUID
  ) {
    super(entity, state, storage);

    // Validate that entity type is 'persona'
    if (entity.type !== 'persona') {
      throw new Error(`PersonaUser requires entity.type='persona', got '${entity.type}'`);
    }

    this.personaId = personaId;
  }

  /**
   * Persona-specific identification
   */
  get isPersona(): boolean {
    return true;
  }

  /**
   * Get persona database path
   * @returns Path to persona's dedicated SQLite database
   */
  getPersonaDatabasePath(): string {
    return `.continuum/personas/${this.personaId}/state.sqlite`;
  }
}