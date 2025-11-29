/**
 * PersonaBody - Action/Execution Systems
 *
 * Part of Being Architecture (Mind/Body/Soul decomposition)
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { PersonaToolExecutor } from '../PersonaToolExecutor';
import { PersonaToolRegistry } from '../PersonaToolRegistry';
import { PersonaResponseGenerator } from '../PersonaResponseGenerator';
import type { UserEntity } from '../../../../data/entities/UserEntity';
import type { ModelConfig } from '../../../../../commands/user/create/shared/UserCreateTypes';
import type { JTAGClient } from '../../../../core/client/shared/JTAGClient';
import type { PersonaMediaConfig } from '../PersonaMediaConfig';
import { SubsystemLogger } from './logging/SubsystemLogger';

export interface PersonaUserForBody {
  readonly id: UUID;
  readonly displayName: string;
  readonly entity: UserEntity;
  readonly modelConfig: ModelConfig;
  readonly client: JTAGClient | undefined;
  readonly mediaConfig: PersonaMediaConfig;
  readonly getSessionId: () => UUID | null;
}

export class PersonaBody {
  private readonly logger: SubsystemLogger;
  public readonly toolExecutor: PersonaToolExecutor;
  public readonly toolRegistry: PersonaToolRegistry;
  public readonly responseGenerator: PersonaResponseGenerator;

  constructor(personaUser: PersonaUserForBody) {
    // Initialize logger first
    this.logger = new SubsystemLogger('body', personaUser.id, personaUser.entity.uniqueId);
    this.logger.info('Body subsystem initializing...');

    // Create toolExecutor and toolRegistry first
    this.toolExecutor = new PersonaToolExecutor(personaUser.id, personaUser.displayName);
    this.toolRegistry = new PersonaToolRegistry();
    this.toolRegistry.registerPersona(personaUser.id, 'assistant'); // Default to assistant role

    // Now create responseGenerator with references to toolExecutor/toolRegistry
    this.responseGenerator = new PersonaResponseGenerator({
      personaId: personaUser.id,
      personaName: personaUser.displayName,
      entity: personaUser.entity,
      modelConfig: personaUser.modelConfig,
      client: personaUser.client,
      toolExecutor: this.toolExecutor,
      toolRegistry: this.toolRegistry,
      mediaConfig: personaUser.mediaConfig,
      getSessionId: personaUser.getSessionId
    });

    this.logger.info('Body subsystem initialized', {
      components: ['toolExecutor', 'toolRegistry', 'responseGenerator']
    });
  }

  /**
   * Shutdown body subsystem (cleanup)
   */
  shutdown(): void {
    this.logger.info('Body subsystem shutting down...');
    this.logger.close();
  }
}
