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
import { GenomeEntity } from '../../../../genome/entities/GenomeEntity';
import { SubsystemLogger } from './logging/SubsystemLogger';
import type { UserEntity } from '../../../../data/entities/UserEntity';
import type { ModelConfig } from '../../../../data/entities/UserEntity';
import type { JTAGClient } from '../../../../core/client/shared/JTAGClient';
import type { UserStateEntity } from '../../../../data/entities/UserStateEntity';
import type { DataReadParams, DataReadResult } from '../../../../../commands/data/read/shared/DataReadTypes';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import { LOCAL_MODELS } from '../../../../../system/shared/Constants';
import { AdapterStore } from '../../../../../system/genome/server/AdapterStore';
import type { DbHandle } from '../../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import { GenomeLayerEntity } from '../../../../genome/entities/GenomeLayerEntity';
import { Events } from '../../../../core/shared/Events';

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
  /** Personal database handle (longterm.db) — set by Hippocampus, read by subsystems */
  personalDbHandle: DbHandle | null;
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

  // Inference model for compatibility checks
  private readonly inferenceModel: string;

  // Client getter for database access
  private readonly getClient: () => JTAGClient | undefined;

  constructor(personaUser: PersonaUserForLimbic) {
    this.personaId = personaUser.id;
    this.displayName = personaUser.displayName;
    this.inferenceModel = personaUser.modelConfig.model || LOCAL_MODELS.DEFAULT;
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

    // Discover real adapters from filesystem for this persona
    // AdapterStore is the SINGLE SOURCE OF TRUTH for what adapters exist
    // Only include adapters compatible with this persona's inference model
    const inferenceModel = personaUser.modelConfig.model || LOCAL_MODELS.DEFAULT;
    const discoveredAdapters = AdapterStore.latestCompatibleByDomain(personaUser.id, inferenceModel);
    const initialAdapters = Array.from(discoveredAdapters.values()).map(adapter => ({
      name: adapter.manifest.name,
      domain: adapter.manifest.traitType,
      path: adapter.dirPath,
      sizeMB: adapter.manifest.sizeMB,
      priority: 0.7,
    }));

    // Also log incompatible adapters so the user knows they exist but need retraining
    const allAdapters = AdapterStore.discoverForPersona(personaUser.id).filter(a => a.hasWeights);
    const incompatible = allAdapters.length - initialAdapters.length;

    if (initialAdapters.length > 0) {
      this.logger.info(`Discovered ${initialAdapters.length} compatible adapters (model=${inferenceModel}): [${initialAdapters.map(a => `${a.name} (${a.domain})`).join(', ')}]`);
    }
    if (incompatible > 0) {
      this.logger.info(`Skipped ${incompatible} incompatible adapters (trained on different base model)`);
    }

    // memoryBudgetMB: 0 means "let Rust decide from real GPU detection".
    // Rust GpuMemoryManager detects actual VRAM and divides inference budget by persona count.
    // If Rust has no GPU manager, it falls back to 200MB internally.
    this.memory = new PersonaMemory(
      personaUser.id,
      personaUser.displayName,
      () => personaUser.personalDbHandle,
      {
        baseModel: personaUser.modelConfig.model || LOCAL_MODELS.DEFAULT,
        memoryBudgetMB: 0,
        adaptersPath: AdapterStore.storeRoot,
        initialAdapters,
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

    // PersonaTrainingManager(personaId, displayName, baseModel, trainingAccumulator, stateGetter, saveStateCallback, logger)
    // Base model from persona's model config — QLoRA quantizes this to 4-bit for training,
    // so we can train on the same model used for inference (3B-8B fits on 8GB VRAM).
    const trainingBaseModel = personaUser.modelConfig.model || LOCAL_MODELS.DEFAULT;
    this.trainingManager = new PersonaTrainingManager(
      personaUser.id,
      personaUser.displayName,
      trainingBaseModel,
      this.trainingAccumulator,
      () => personaUser.state,
      async () => {
        await personaUser.saveState();
        return { success: true };
      },
      trainingLogger  // Pass training logger
    );

    // Wire post-training activation: when training completes, re-discover adapters from filesystem
    // This closes the loop: train → write adapter to disk → discover → register + activate → inference uses new weights
    this.trainingManager.onTrainingComplete = async (_layerId: string, domain: string) => {
      await this.hotLoadNewAdapters(domain);
    };

    // Subscribe to training completion events from ANY source (sentinel pipelines, academy, etc.)
    // This closes the loop for external training: sentinel calls genome/train → adapter saved →
    // TrainingCompletionHandler emits event → LimbicSystem re-discovers and hot-loads the adapter.
    // The onTrainingComplete callback above only fires for self-initiated training via PersonaTrainingManager.
    Events.subscribe('genome:training:complete', async (payload: any) => {
      if (payload.personaId !== this.personaId) return;
      this.logger.info(`External training complete: ${payload.traitType ?? 'unknown'} (handle=${payload.handle})`);
      await this.hotLoadNewAdapters(payload.traitType ?? 'unknown');
    });

    // Hippocampus(personaUser) - Note: Hippocampus requires full PersonaUser interface
    // This is safe because LimbicSystem is only instantiated by PersonaUser
    this.hippocampus = new Hippocampus(personaUser as any);

    this.logger.info('Limbic system initialized', {
      components: ['memory', 'genomeManager', 'trainingAccumulator', 'trainingManager', 'hippocampus']
    });
  }

  // ===== PUBLIC INTERFACE =====

  /**
   * Wait for Hippocampus DB initialization to complete.
   * THROWS if handle is null — persona cannot operate without longterm.db.
   * The handle is set on BaseUser.personalDbHandle by Hippocampus directly;
   * PersonaMemory reads it via a live reference — no propagation needed.
   */
  async ensureDbReady(): Promise<DbHandle> {
    const handle = await this.hippocampus.waitForDbInit();
    if (!handle) {
      throw new Error(`${this.displayName}: Hippocampus DB init failed — cannot start persona without longterm.db`);
    }
    return handle;
  }

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
            backend: 'server',
            dbHandle: 'default'
          }
        );

        if (!layerResult.success || !layerResult.found || !layerResult.data) {
          this.logger.warn(`Layer ${layerRef.layerId} not found in database`);
          continue;
        }

        const layer = layerResult.data;

        // Validate adapter path exists and is compatible with inference model
        if (!AdapterStore.isValidAdapterPath(layer.modelPath)) {
          this.logger.warn(`Skipping layer ${layer.name} — adapter path missing: ${layer.modelPath}`);
          continue;
        }

        // Check model compatibility via manifest if available
        const adapterInfo = AdapterStore.discoverAll().find(a => a.dirPath === layer.modelPath);
        if (adapterInfo && !AdapterStore.isCompatibleWithModel(adapterInfo, this.inferenceModel)) {
          this.logger.warn(`Skipping layer ${layer.name} — trained on ${adapterInfo.manifest.baseModel}, incompatible with ${this.inferenceModel}`);
          continue;
        }

        // Register adapter in PersonaGenome (layerId enables fitness tracking back to entity)
        this.memory.genome.registerAdapter({
          name: layer.name,
          domain: layer.traitType || layerRef.traitType,
          path: layer.modelPath,
          sizeMB: layer.sizeMB || 10,
          priority: layerRef.weight,
          layerId: layer.id,
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
   * Adopt an existing adapter from another persona (cross-persona sharing).
   *
   * Instead of training from scratch, reuse a GenomeLayerEntity that was already
   * trained by someone else and discovered via GenomeRegistry.findByCapability().
   *
   * Validates path existence and model compatibility before registering.
   * Returns true if adoption succeeded.
   */
  async adoptAdapter(layer: GenomeLayerEntity): Promise<boolean> {
    if (!AdapterStore.isValidAdapterPath(layer.modelPath)) {
      this.logger.warn(`Cannot adopt ${layer.name} — adapter path missing: ${layer.modelPath}`);
      return false;
    }

    const adapterInfo = AdapterStore.discoverAll().find(a => a.dirPath === layer.modelPath);
    if (adapterInfo && !AdapterStore.isCompatibleWithModel(adapterInfo, this.inferenceModel)) {
      this.logger.warn(`Cannot adopt ${layer.name} — incompatible with ${this.inferenceModel}`);
      return false;
    }

    this.memory.genome.registerAdapter({
      name: layer.name,
      domain: layer.traitType,
      path: layer.modelPath,
      sizeMB: layer.sizeMB || 10,
      priority: 0.7,
      layerId: layer.id,
    });

    await this.memory.genome.activateSkill(layer.name);
    this.logger.info(`Adopted adapter: ${layer.name} (${layer.traitType}) from ${layer.creatorId?.slice(0, 8) ?? 'unknown'}`);
    return true;
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
   * Hot-load newly trained adapters from filesystem.
   *
   * Re-discovers all compatible adapters for this persona, registers any new ones
   * with PersonaGenome, and activates them for immediate inference use.
   * Called after training completes (both self-initiated and sentinel-initiated).
   */
  private async hotLoadNewAdapters(domain: string): Promise<void> {
    this.logger.info(`Hot-loading adapters for domain: ${domain}`);
    const discovered = AdapterStore.latestCompatibleByDomain(this.personaId, this.inferenceModel);
    let newCount = 0;
    for (const [adapterDomain, adapter] of discovered) {
      if (!this.memory.genome.hasAdapter(adapter.manifest.name)) {
        this.memory.genome.registerAdapter({
          name: adapter.manifest.name,
          domain: adapter.manifest.traitType,
          path: adapter.dirPath,
          sizeMB: adapter.manifest.sizeMB,
          priority: 0.7,
        });
        await this.memory.genome.activateSkill(adapter.manifest.name);
        this.logger.info(`Hot-loaded adapter: ${adapter.manifest.name} (${adapterDomain})`);
        newCount++;
      }
    }
    if (newCount > 0) {
      this.logger.info(`Hot-loaded ${newCount} new adapter(s) from training`);
      await this.recalculateCompositeEmbedding();
    }
  }

  /**
   * Recalculate composite embedding for this persona's genome.
   * Loads all genome layer entities, averages their embeddings,
   * and persists the result to the GenomeEntity.
   */
  private async recalculateCompositeEmbedding(): Promise<void> {
    try {
      const genome = await this.genomeManager.getGenome();
      if (!genome || genome.layers.length === 0) return;

      // Load all layer embeddings
      const layerIds = genome.getEnabledLayers().map(l => l.layerId);
      const layerResult = await DataList.execute<GenomeLayerEntity>({
        collection: GenomeLayerEntity.collection,
        filter: { id: { $in: layerIds } },
        dbHandle: 'default',
        limit: layerIds.length,
      });

      if (!layerResult.success) return;

      const embeddings = layerResult.items
        .map(layer => layer.embedding)
        .filter(e => e && e.length > 0);

      if (embeddings.length === 0) return;

      const { embedding, dimension } = GenomeEntity.calculateCompositeEmbedding(embeddings);
      genome.compositeEmbedding = embedding;
      genome.embeddingDimension = dimension;

      this.logger.info(`Composite embedding recalculated: ${dimension}d from ${embeddings.length} layers`);
    } catch (error) {
      this.logger.warn(`Composite embedding recalculation failed (non-critical): ${error}`);
    }
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
