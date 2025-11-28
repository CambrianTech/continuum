/**
 * PersonaBody - Action/Execution Systems
 *
 * Part of Being Architecture (Mind/Body/Soul decomposition)
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { PersonaToolExecutor } from '../PersonaToolExecutor';
import { PersonaToolRegistry } from '../PersonaToolRegistry';

export interface PersonaUserForBody {
  readonly id: UUID;
  readonly displayName: string;
}

export class PersonaBody {
  public readonly toolExecutor: PersonaToolExecutor;
  public readonly toolRegistry: PersonaToolRegistry;

  constructor(personaUser: PersonaUserForBody) {
    this.toolExecutor = new PersonaToolExecutor(personaUser.id, personaUser.displayName);
    this.toolRegistry = new PersonaToolRegistry();
    this.toolRegistry.registerPersona(personaUser.id, 'assistant'); // Default to assistant role
  }
}
