/**
 * MotorCortex - Action/Execution Systems
 *
 * Part of Being Architecture (neuroanatomy-inspired decomposition)
 * Maps to motor cortex: voluntary movement, tool use, action execution
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

export interface PersonaUserForMotorCortex {
  readonly id: UUID;
  readonly displayName: string;
  readonly entity: UserEntity;
  readonly modelConfig: ModelConfig;
  readonly client: JTAGClient | undefined;
  readonly mediaConfig: PersonaMediaConfig;
  readonly getSessionId: () => UUID | null;
  readonly homeDirectory: string;
  readonly logger: import('../PersonaLogger').PersonaLogger;
}

export class MotorCortex {
  private readonly logger: SubsystemLogger;
  public readonly toolExecutor: PersonaToolExecutor;
  public readonly toolRegistry: PersonaToolRegistry;
  public readonly responseGenerator: PersonaResponseGenerator;

  constructor(personaUser: PersonaUserForMotorCortex) {
    // Initialize logger first
    this.logger = new SubsystemLogger('motor-cortex', personaUser.id, personaUser.entity.uniqueId, {
      logDir: `${personaUser.homeDirectory}/logs`
    });
    this.logger.info('Motor cortex initializing...');

    // Create toolExecutor and toolRegistry first
    this.toolExecutor = new PersonaToolExecutor(personaUser);
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
      getSessionId: personaUser.getSessionId,
      logger: personaUser.logger
    });

    this.logger.info('Motor cortex initialized', {
      components: ['toolExecutor', 'toolRegistry', 'responseGenerator']
    });
  }

  /**
   * Shutdown motor cortex (cleanup)
   */
  shutdown(): void {
    this.logger.info('Motor cortex shutting down...');
    this.logger.close();
  }
}
