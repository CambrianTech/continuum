/**
 * PersonaMind - Cognition (State Tracking)
 *
 * Part of Being Architecture (Mind/Body/Soul decomposition)
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { PersonaStateManager } from '../PersonaState';
import { WorkingMemoryManager } from '../cognition/memory/WorkingMemoryManager';
import { PersonaSelfState } from '../cognition/PersonaSelfState';
import { SimplePlanFormulator } from '../cognition/reasoning/SimplePlanFormulator';
import { SubsystemLogger } from './logging/SubsystemLogger';

export interface PersonaUserForMind {
  readonly id: UUID;
  readonly displayName: string;
  readonly entity: { uniqueId: string };
  readonly homeDirectory: string;
}

export class PersonaMind {
  private readonly logger: SubsystemLogger;
  public readonly personaState: PersonaStateManager;
  public readonly workingMemory: WorkingMemoryManager;
  public readonly selfState: PersonaSelfState;
  public readonly planFormulator: SimplePlanFormulator;

  constructor(personaUser: PersonaUserForMind) {
    // Initialize logger first
    this.logger = new SubsystemLogger('mind', personaUser.id, personaUser.entity.uniqueId, {
      logDir: `${personaUser.homeDirectory}/logs`
    });
    this.logger.info('Mind subsystem initializing...');

    // PersonaState logs to mind.log (state tracking is part of cognition)
    this.personaState = new PersonaStateManager(personaUser.displayName, {
      enableLogging: true,
      logger: this.logger  // Share mind logger
    });
    this.workingMemory = new WorkingMemoryManager(personaUser.id);
    this.selfState = new PersonaSelfState(personaUser.id);
    this.planFormulator = new SimplePlanFormulator(personaUser.id, personaUser.displayName);

    this.logger.info('Mind subsystem initialized', {
      components: ['personaState', 'workingMemory', 'selfState', 'planFormulator']
    });
  }

  /**
   * Shutdown mind subsystem (cleanup)
   */
  shutdown(): void {
    this.logger.info('Mind subsystem shutting down...');
    this.logger.close();
  }
}
