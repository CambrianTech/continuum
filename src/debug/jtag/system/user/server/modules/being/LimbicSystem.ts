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
import type { GenomeLayerEntity } from '../../../../genome/entities/GenomeLayerEntity';
import { SubsystemLogger } from './logging/SubsystemLogger';
import type { UserEntity } from '../../../../data/entities/UserEntity';
import type { ModelConfig } from '../../../../../commands/user/create/shared/UserCreateTypes';
import type { JTAGClient } from '../../../../core/client/shared/JTAGClient';
import type { UserStateEntity } from '../../../../data/entities/UserStateEntity';
import type { DataReadParams, DataReadResult } from '../../../../../commands/data/read/shared/DataReadTypes';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { LOCAL_MODELS } from '../../../../../system/shared/Constants';

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

  // Client getter for database access
  private readonly getClient: () => JTAGClient | undefined;

  constructor(personaUser: PersonaUserForLimbic) {
    this.personaId = personaUser.id;
    this.displayName = personaUser.displayName;
    this.getClient = () => personaUser.client;

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
        baseModel: personaUser.modelConfig.model || LOCAL_MODELS.DEFAULT,
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
   * Load genome layers from database into PersonaGenome
   *
   * This bridges the database (GenomeEntity + GenomeLayerEntity) with the runtime
   * (PersonaGenome) by loading each layer reference and registering it as an adapter.
   *
   * Should be called during PersonaUser startup after construction.
   */
  async loadGenomeFromDatabase(): Promise<void> {
    this.logger.info('Loading genome from database...');

    // Get genome entity
    const genome = await this.genomeManager.getGenome();
    if (!genome) {
      this.logger.info('No genome assigned to persona');
      return;
    }

    const client = this.getClient();
    if (!client) {
      this.logger.warn('Cannot load genome layers - no client');
      return;
    }

    this.logger.info(`Genome found: ${genome.name} with ${genome.layers.length} layers`);

    // Load each layer and register as adapter
    let loadedCount = 0;
    for (const layerRef of genome.layers) {
      if (!layerRef.enabled) {
        this.logger.info(`Skipping disabled layer: ${layerRef.layerId}`);
        continue;
      }

      try {
        // Load layer entity from database
        const layerResult = await client.daemons.commands.execute<DataReadParams, DataReadResult<GenomeLayerEntity>>(
          DATA_COMMANDS.READ,
          {
            userId: client.userId,
            collection: 'genome_layers',
            id: layerRef.layerId,
            context: client.context,
            sessionId: client.sessionId,
            backend: 'server'
          }
        );

        if (!layerResult.success || !layerResult.found || !layerResult.data) {
          this.logger.warn(`Layer ${layerRef.layerId} not found in database`);
          continue;
        }

        const layer = layerResult.data;

        // Register adapter in PersonaGenome
        this.memory.genome.registerAdapter({
          name: layer.name,
          domain: layer.traitType || layerRef.traitType,
          path: layer.modelPath,
          sizeMB: layer.sizeMB || 10,
          priority: layerRef.weight,
        });

        // Activate the adapter immediately so it's available for inference
        // This moves it from availableAdapters to activeAdapters
        await this.memory.genome.activateSkill(layer.name);

        this.logger.info(`Registered and activated adapter: ${layer.name} (${layer.traitType}, path=${layer.modelPath})`);
        loadedCount++;

      } catch (error) {
        this.logger.error(`Failed to load layer ${layerRef.layerId}: ${error}`);
      }
    }

    this.logger.info(`Loaded and activated ${loadedCount}/${genome.layers.length} genome layers from database`);
  }

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
