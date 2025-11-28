/**
 * PersonaMind - Cognition (State Tracking)
 *
 * Part of Being Architecture (Mind/Body/Soul decomposition)
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { PersonaStateManager } from '../PersonaState';

export interface PersonaUserForMind {
  readonly id: UUID;
  readonly displayName: string;
}

export class PersonaMind {
  public readonly personaState: PersonaStateManager;

  constructor(personaUser: PersonaUserForMind) {
    this.personaState = new PersonaStateManager(personaUser.displayName, {
      enableLogging: true
    });
  }
}
