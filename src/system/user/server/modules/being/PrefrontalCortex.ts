/**
 * PrefrontalCortex - Executive Function & Cognition
 *
 * Part of Being Architecture (neuroanatomy-inspired decomposition)
 * Maps to prefrontal cortex: planning, decision-making, working memory
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { PersonaStateManager } from '../PersonaState';
import { WorkingMemoryManager } from '../cognition/memory/WorkingMemoryManager';
import { PersonaSelfState } from '../cognition/PersonaSelfState';
import { SimplePlanFormulator } from '../cognition/reasoning/SimplePlanFormulator';
import { SubsystemLogger } from './logging/SubsystemLogger';

export interface PersonaUserForPrefrontal {
  readonly id: UUID;
  readonly displayName: string;
  readonly entity: { uniqueId: string };
  readonly homeDirectory: string;
  readonly logger: { enqueueLog(fileName: string, message: string): void };
}

export class PrefrontalCortex {
  private readonly logger: SubsystemLogger;
  public readonly personaState: PersonaStateManager;
  public readonly workingMemory: WorkingMemoryManager;
  public readonly selfState: PersonaSelfState;
  public readonly planFormulator: SimplePlanFormulator;

  constructor(personaUser: PersonaUserForPrefrontal) {
    // Initialize logger first
    this.logger = new SubsystemLogger('prefrontal', personaUser.id, personaUser.entity.uniqueId, {
      logDir: `${personaUser.homeDirectory}/logs`
    });
    this.logger.info('Prefrontal cortex initializing...');

    // PersonaState logs to mind.log (state tracking is part of cognition)
    this.personaState = new PersonaStateManager(personaUser.displayName, {
      enableLogging: true,
      logger: this.logger  // Share mind logger
    });
    // WorkingMemory logs to cognition.log (shared with decision-making)
    const cognitionLogger = (message: string) => {
      personaUser.logger.enqueueLog('cognition.log', message);
    };
    this.workingMemory = new WorkingMemoryManager(personaUser.id, cognitionLogger);
    this.selfState = new PersonaSelfState(personaUser.id);
    this.planFormulator = new SimplePlanFormulator(personaUser.id, personaUser.displayName);

    this.logger.info('Prefrontal cortex initialized', {
      components: ['personaState', 'workingMemory', 'selfState', 'planFormulator']
    });
  }

  /**
   * Shutdown prefrontal cortex (cleanup)
   */
  shutdown(): void {
    this.logger.info('Prefrontal cortex shutting down...');
    this.logger.close();
  }
}
