/**
 * PersonaSoul - Memory, Learning, Identity
 *
 * Part of Being Architecture (Mind/Body/Soul decomposition)
 *
 * Responsibility: All aspects of long-term memory, learning, and identity.
 * Maps to limbic system in neuroscience (hippocampus, amygdala, cortical memory).
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

/**
 * Forward declaration of PersonaUser to avoid circular dependencies
 */
export interface PersonaUserForSoul {
  readonly id: UUID;
  readonly displayName: string;
  readonly entity: any; // UserEntity
  readonly modelConfig: any; // ModelConfig
  readonly client?: any; // JTAGClient
  readonly state: any; // UserStateEntity
  saveState(): Promise<void>;
}

/**
 * PersonaSoul - The limbic system of a persona
 *
 * Handles all memory, learning, and identity functions.
 * Independent of cognition (Mind) and execution (Body).
 */
export class PersonaSoul {
  // Long-term Memory
  public readonly memory: PersonaMemory;

  // Genome (identity, personality, skills)
  private readonly genomeManager: PersonaGenomeManager;

  // Learning systems
  public readonly trainingAccumulator: TrainingDataAccumulator;
  private readonly trainingManager: PersonaTrainingManager;

  // Memory consolidation subprocess (hippocampus)
  private readonly hippocampus: Hippocampus;

  // Identity
  private readonly personaId: UUID;
  private readonly displayName: string;

  constructor(personaUser: PersonaUserForSoul) {
    this.personaId = personaUser.id;
    this.displayName = personaUser.displayName;

    // Initialize memory systems
    // PersonaMemory(personaId, displayName, genomeConfig, client?)
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
      personaUser.client
    );

    // PersonaGenomeManager(personaId, displayName, entityGetter, clientGetter)
    this.genomeManager = new PersonaGenomeManager(
      personaUser.id,
      personaUser.displayName,
      () => personaUser.entity,
      () => personaUser.client
    );

    // TrainingDataAccumulator(personaId, displayName)
    this.trainingAccumulator = new TrainingDataAccumulator(personaUser.id, personaUser.displayName);

    // PersonaTrainingManager(personaId, displayName, trainingAccumulator, stateGetter, saveStateCallback)
    this.trainingManager = new PersonaTrainingManager(
      personaUser.id,
      personaUser.displayName,
      this.trainingAccumulator,
      () => personaUser.state,
      async () => {
        await personaUser.saveState();
        return { success: true };
      }
    );

    // Hippocampus(personaUser)
    this.hippocampus = new Hippocampus(personaUser as any);

    console.log(`🧬 ${this.displayName}: Soul initialized (memory, genome, learning, hippocampus)`);
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
    console.log(`🧠 ${this.displayName}: Hippocampus started (memory consolidation active)`);
  }

  /**
   * Shutdown soul systems
   * Stops hippocampus subprocess and any background tasks
   */
  async shutdown(): Promise<void> {
    await this.hippocampus.stop();
    // Memory is just data access - no shutdown needed
    // Genome is just data access - no shutdown needed
    // Training accumulator is just data structure - no shutdown needed
    console.log(`🧬 ${this.displayName}: Soul shutdown complete`);
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
