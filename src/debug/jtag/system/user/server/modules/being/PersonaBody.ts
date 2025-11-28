/**
 * PersonaBody - Action/Execution Systems
 *
 * Part of Being Architecture (Mind/Body/Soul decomposition)
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { PersonaToolExecutor } from '../PersonaToolExecutor';

export interface PersonaUserForBody {
  readonly id: UUID;
  readonly displayName: string;
}

export class PersonaBody {
  public readonly toolExecutor: PersonaToolExecutor;

  constructor(personaUser: PersonaUserForBody) {
    this.toolExecutor = new PersonaToolExecutor(personaUser.id, personaUser.displayName);
  }
}
