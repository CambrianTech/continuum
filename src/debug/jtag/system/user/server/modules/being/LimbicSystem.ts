/**
 * LimbicSystem - Memory, Emotion, Learning, Identity
 *
 * Part of Being Architecture (neuroanatomy-inspired decomposition)
 * Maps to limbic system: hippocampus, amygdala, emotional memory, identity
 *
 * Components:
 * - Long-term memory (PersonaMemory)
 * - Genome/skills (PersonaGenomeManager)
 * - Learning/training (TrainingDataAccumulator, PersonaTrainingManager)
 * - Memory consolidation (Hippocampus subprocess)
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { PersonaMemory } from '../cognitive/memory/PersonaMemory';
import { PersonaGenomeManager } from '../PersonaGenomeManager';
import { TrainingDataAccumulator } from '../TrainingDataAccumulator';
import { PersonaTrainingManager } from '../PersonaTrainingManager';
import { Hippocampus } from '../cognitive/memory/Hippocampus';
import type { GenomeEntity } from '../../../../genome/entities/GenomeEntity';
import { SubsystemLogger } from './logging/SubsystemLogger';
import type { UserEntity } from '../../../../data/entities/UserEntity';
import type { ModelConfig } from '../../../../../commands/user/create/shared/UserCreateTypes';
import type { JTAGClient } from '../../../../core/client/shared/JTAGClient';
import type { UserStateEntity } from '../../../../data/entities/UserStateEntity';

/**
 * Forward declaration of PersonaUser to avoid circular dependencies
 */
export interface PersonaUserForLimbic {
  readonly id: UUID;
  readonly displayName: string;
  readonly entity: UserEntity;
  readonly modelConfig: ModelConfig;
  readonly client?: JTAGClient;
  readonly state: UserStateEntity;
  readonly homeDirectory: string;
  saveState(): Promise<void>;
}

/**
 * LimbicSystem - Memory, emotion, and identity center
 *
 * Handles all memory, learning, and identity functions.
 * Independent of executive function (PrefrontalCortex) and motor execution (MotorCortex).
 */
export class LimbicSystem {
  private readonly logger: SubsystemLogger;

  // Long-term Memory
  public readonly memory: PersonaMemory;

  // Genome (identity, personality, skills)
  public readonly genomeManager: PersonaGenomeManager;

  // Learning systems
  public readonly trainingAccumulator: TrainingDataAccumulator;
  public readonly trainingManager: PersonaTrainingManager;

  // Memory consolidation subprocess (hippocampus)
  public readonly hippocampus: Hippocampus;

  // Identity
  private readonly personaId: UUID;
  private readonly displayName: string;

  constructor(personaUser: PersonaUserForLimbic) {
    this.personaId = personaUser.id;
    this.displayName = personaUser.displayName;

    // Initialize logger first
    this.logger = new SubsystemLogger('limbic', personaUser.id, personaUser.entity.uniqueId, {
      logDir: `${personaUser.homeDirectory}/logs`
    });
    this.logger.info('Limbic system initializing...');

    // Initialize memory systems
    // PersonaMemory(personaId, displayName, genomeConfig, client?, genomeLogger?)
    const genomeLogger = (message: string) => {
      this.logger.enqueueLog('genome.log', message);
    };

    this.memory = new PersonaMemory(
      personaUser.id,
      personaUser.displayName,
      {
        baseModel: personaUser.modelConfig.model || 'llama3.2:3b',
        memoryBudgetMB: 200,
        adaptersPath: './lora-adapters',
        initialAdapters: [
          {
            name: 'conversational',
            domain: 'chat',
            path: './lora-adapters/conversational.safetensors',
            sizeMB: 50,
            priority: 0.7
          },
          {
            name: 'typescript-expertise',
            domain: 'code',
            path: './lora-adapters/typescript-expertise.safetensors',
            sizeMB: 60,
            priority: 0.6
          },
          {
            name: 'self-improvement',
            domain: 'self',
            path: './lora-adapters/self-improvement.safetensors',
            sizeMB: 40,
            priority: 0.5
          }
        ]
      },
      personaUser.client,
      genomeLogger
    );

    // TrainingDataAccumulator(personaId, displayName, trainingLogger?)
    const trainingLogger = (message: string) => {
      this.logger.enqueueLog('training.log', message);
    };

    // PersonaGenomeManager(personaId, displayName, entityGetter, clientGetter, logger)
    this.genomeManager = new PersonaGenomeManager(
      personaUser.id,
      personaUser.displayName,
      () => personaUser.entity,
      () => personaUser.client,
      trainingLogger  // Use training.log for genome manager too (related to training)
    );

    this.trainingAccumulator = new TrainingDataAccumulator(personaUser.id, personaUser.displayName, trainingLogger);

    // PersonaTrainingManager(personaId, displayName, trainingAccumulator, stateGetter, saveStateCallback, logger)
    this.trainingManager = new PersonaTrainingManager(
      personaUser.id,
      personaUser.displayName,
      this.trainingAccumulator,
      () => personaUser.state,
      async () => {
        await personaUser.saveState();
        return { success: true };
      },
      trainingLogger  // Pass training logger
    );

    // Hippocampus(personaUser) - Note: Hippocampus requires full PersonaUser interface
    // This is safe because LimbicSystem is only instantiated by PersonaUser
    this.hippocampus = new Hippocampus(personaUser as any);

    this.logger.info('Limbic system initialized', {
      components: ['memory', 'genomeManager', 'trainingAccumulator', 'trainingManager', 'hippocampus']
    });
  }

  // ===== PUBLIC INTERFACE =====

  /**
   * Get genome for this persona
   * Loads the genome entity from database if genomeId is set
   */
  async getGenome(): Promise<GenomeEntity | null> {
    return await this.genomeManager.getGenome();
  }

  /**
   * Set genome for this persona
   * Updates the genomeId field and persists to database
   */
  async setGenome(genomeId: UUID): Promise<boolean> {
    return await this.genomeManager.setGenome(genomeId);
  }

  /**
   * Start memory consolidation subprocess
   * (Hippocampus bridges working memory → long-term memory)
   */
  async startMemoryConsolidation(): Promise<void> {
    await this.hippocampus.start();
    this.logger.info('Hippocampus started (memory consolidation active)');
  }

  /**
   * Shutdown limbic systems
   * Stops hippocampus subprocess and any background tasks
   */
  async shutdown(): Promise<void> {
    this.logger.info('Limbic system shutting down...');
    await this.hippocampus.stop();
    // Memory is just data access - no shutdown needed
    // Genome is just data access - no shutdown needed
    // Training accumulator is just data structure - no shutdown needed
    this.logger.info('Limbic system shutdown complete');
    this.logger.close();
  }

  /**
   * Get identity information
   */
  getIdentity(): { personaId: UUID; displayName: string } {
    return {
      personaId: this.personaId,
      displayName: this.displayName
    };
  }
}
