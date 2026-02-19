/**
 * Adapter Adopt Command - Server Implementation
 *
 * Add an adapter to a persona's genome, making it a permanent trait.
 * This command:
 * 1. Downloads the adapter from HuggingFace if needed
 * 2. Creates a GenomeLayerEntity for the adapter
 * 3. Adds the layer to the persona's GenomeEntity
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { AdapterAdoptParams, AdapterAdoptResult } from '../shared/AdapterAdoptTypes';
import { createAdapterAdoptResultFromParams } from '../shared/AdapterAdoptTypes';
import { Commands } from '@system/core/shared/Commands';
import { InferenceGrpcClient } from '@system/core/services/InferenceGrpcClient';
import { GenomeEntity, type GenomeLayerReference } from '@system/genome/entities/GenomeEntity';
import { GenomeLayerEntity } from '@system/genome/entities/GenomeLayerEntity';
import { COLLECTIONS } from '@system/data/config/DatabaseConfig';
import { generateUUID, type UUID } from '@system/core/types/CrossPlatformUUID';
import type { DataCreateParams, DataCreateResult } from '@commands/data/create/shared/DataCreateTypes';
import type { DataReadParams, DataReadResult } from '@commands/data/read/shared/DataReadTypes';
import type { DataUpdateParams, DataUpdateResult } from '@commands/data/update/shared/DataUpdateTypes';
import type { UserEntity } from '@system/data/entities/UserEntity';

import { DataRead } from '../../../data/read/shared/DataReadTypes';
import { DataCreate } from '../../../data/create/shared/DataCreateTypes';
import { DataUpdate } from '../../../data/update/shared/DataUpdateTypes';
export class AdapterAdoptServerCommand extends CommandBase<AdapterAdoptParams, AdapterAdoptResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('adapter/adopt', context, subpath, commander);
  }

  async execute(params: AdapterAdoptParams): Promise<AdapterAdoptResult> {
    console.log('🧬 Adapter Adopt: Adopting', params.adapterId);

    // Validate required parameters
    if (!params.adapterId || params.adapterId.trim() === '') {
      throw new ValidationError(
        'adapterId',
        `Missing required parameter 'adapterId'. ` +
        `Use adapter/search to find adapters, then provide the ID here.`
      );
    }

    const scale = params.scale ?? 1.0;
    const traitType = params.traitType ?? 'domain_expertise';
    const client = new InferenceGrpcClient();

    try {
      // Step 1: Get adapter info (download if needed)
      console.log('   Checking adapter availability...');
      const adapterId = params.adapterId.includes('/')
        ? params.adapterId.replace('/', '--')
        : params.adapterId;

      let adapterMetadata: Record<string, unknown> = {};
      let localPath = '';

      // Check if already installed locally
      const existingAdapters = await client.listAdapters();
      const existing = existingAdapters.find(a =>
        a.adapterId === adapterId || a.adapterId === params.adapterId
      );

      if (existing) {
        localPath = existing.path;
        console.log(`   Adapter already installed at ${localPath}`);
      } else if (params.adapterId.includes('/')) {
        // Download from HuggingFace
        console.log('   Downloading from HuggingFace...');
        const download = await client.downloadAdapter(params.adapterId, {
          adapterId,
          scale,
        });
        if (!download.success) {
          throw new Error(`Failed to download adapter: ${download.error}`);
        }
        localPath = download.localPath;
        if (download.metadata) {
          adapterMetadata = {
            baseModel: download.metadata.baseModel,
            rank: download.metadata.rank,
            alpha: download.metadata.alpha,
            targetModules: download.metadata.targetModules,
          };
        }
        console.log(`   Downloaded to ${localPath}`);
      } else {
        throw new Error(`Adapter not found locally and not a HuggingFace ID: ${params.adapterId}`);
      }

      // Step 2: Find target persona
      const targetPersonaId = params.personaId;

      if (!targetPersonaId) {
        throw new ValidationError(
          'personaId',
          `Missing required parameter 'personaId'. ` +
          `Specify which persona should adopt this adapter.`
        );
      }

      // Find the persona
      const personaResult = await DataRead.execute<UserEntity>({
          collection: COLLECTIONS.USERS,
          id: targetPersonaId,
        }
      ) as DataReadResult<UserEntity>;

      if (!personaResult.success || !personaResult.found || !personaResult.data) {
        throw new Error(`Persona not found: ${targetPersonaId}`);
      }
      const targetPersona = personaResult.data;
      // Note: ID is at result.id, not result.data.id (DataReadResult structure)
      const targetPersonaId_resolved = personaResult.id;
      console.log(`   Target persona: ${targetPersona.displayName} (ID: ${targetPersonaId_resolved})`);

      // Step 3: Create GenomeLayerEntity
      console.log('   Creating genome layer...');
      const layerId = generateUUID();
      const layer = new GenomeLayerEntity();
      layer.id = layerId;
      layer.name = params.adapterId;
      layer.description = `LoRA adapter: ${params.adapterId}`;
      layer.traitType = traitType;
      layer.source = params.adapterId.includes('/') ? 'downloaded' : 'system';
      layer.modelPath = localPath;
      layer.sizeMB = (adapterMetadata.rank as number) ? (adapterMetadata.rank as number) * 0.5 : 10;
      layer.rank = (adapterMetadata.rank as number) || 16;
      layer.creatorId = targetPersonaId_resolved;
      layer.createdAt = new Date();
      layer.updatedAt = new Date();
      // Required fields with defaults from entity constructor
      layer.embedding = Array(768).fill(0);
      layer.tags = [traitType, 'lora', 'adopted'];
      layer.generation = 0;
      layer.fitness = {
        accuracy: 0,
        efficiency: 0,
        usageCount: 0,
        successRate: 0,
        averageLatency: 0,
        cacheHitRate: 0,
      };
      layer.trainingMetadata = {
        epochs: 0,
        loss: 0,
        performance: 0,
        trainingDuration: 0,
      };

      const createLayerResult = await DataCreate.execute<GenomeLayerEntity>({
          collection: 'genome_layers',
          data: layer,
        }
      ) as DataCreateResult<GenomeLayerEntity>;

      if (!createLayerResult.success) {
        throw new Error(`Failed to create genome layer: ${createLayerResult.error}`);
      }
      console.log(`   Created layer ${layerId}`);

      // Step 4: Get or create GenomeEntity for persona
      let genome: GenomeEntity;
      let genomeName: string;
      let isNewGenome = false;

      if (targetPersona.genomeId) {
        // Load existing genome
        const genomeResult = await DataRead.execute<GenomeEntity>({
            collection: 'genomes',
            id: targetPersona.genomeId,
          }
        ) as DataReadResult<GenomeEntity>;

        if (genomeResult.success && genomeResult.found && genomeResult.data) {
          genome = genomeResult.data;
          // Also get genome.id from the result (same DataReadResult pattern)
          genome.id = genomeResult.id;
          genomeName = genome.name;
          console.log(`   Using existing genome: ${genomeName} (ID: ${genome.id})`);
        } else {
          // Genome reference exists but genome not found - create new one
          console.log(`   Genome ${targetPersona.genomeId} not found, creating new one`);
          genome = this.createNewGenome(targetPersonaId_resolved, targetPersona.displayName);
          genomeName = genome.name;
          isNewGenome = true;
        }
      } else {
        console.log(`   Target persona ID: "${targetPersonaId_resolved}" (${typeof targetPersonaId_resolved})`);
        genome = this.createNewGenome(targetPersonaId_resolved, targetPersona.displayName);
        genomeName = genome.name;
        isNewGenome = true;
        console.log(`   Creating new genome: ${genomeName}, personaId: "${genome.personaId}"`);
      }

      // Step 5: Add layer to genome
      const layerRef: GenomeLayerReference = {
        layerId,
        traitType,
        orderIndex: genome.layers.length,
        weight: scale,
        enabled: true,
      };
      genome.layers.push(layerRef);
      genome.metadata.lastModified = new Date();
      genome.updatedAt = new Date();

      // Step 6: Save genome
      if (!isNewGenome) {
        // Update existing genome
        const updateResult = await DataUpdate.execute<GenomeEntity>({
            collection: 'genomes',
            id: genome.id,
            data: { layers: genome.layers, metadata: genome.metadata, updatedAt: genome.updatedAt },
          }
        ) as DataUpdateResult<GenomeEntity>;
        if (!updateResult.success) {
          throw new Error(`Failed to update genome: ${updateResult.error}`);
        }
      } else {
        const createGenomeResult = await DataCreate.execute<GenomeEntity>({
            collection: 'genomes',
            data: genome,
          }
        ) as DataCreateResult<GenomeEntity>;
        if (!createGenomeResult.success) {
          throw new Error(`Failed to create genome: ${createGenomeResult.error}`);
        }

        // Update persona with genome ID
        await DataUpdate.execute<UserEntity>({
            collection: COLLECTIONS.USERS,
            id: targetPersonaId_resolved,
            data: { genomeId: genome.id },
          }
        );
      }

      console.log(`   ✅ Adapter adopted: ${genome.layers.length} layers in genome`);

      return createAdapterAdoptResultFromParams(params, {
        success: true,
        adapterId: params.adapterId,
        layerId,
        personaId: targetPersonaId_resolved,
        genomeName,
        layerCount: genome.layers.length,
        metadata: adapterMetadata,
      });

    } catch (error) {
      console.error('   ❌ Adapter adopt failed:', error);
      return createAdapterAdoptResultFromParams(params, {
        success: false,
        adapterId: params.adapterId,
        layerId: '',
        personaId: params.personaId || '',
        genomeName: '',
        layerCount: 0,
        metadata: {},
        error: {
          type: 'adapter_adopt_failed',
          message: String(error),
        },
      });
    } finally {
      client.close();
    }
  }

  /**
   * Create a new GenomeEntity for a persona
   */
  private createNewGenome(personaId: UUID, personaName: string): GenomeEntity {
    const genome = new GenomeEntity();
    genome.id = generateUUID();
    genome.name = `${personaName}'s Genome`;
    genome.description = `Genome for persona ${personaName}`;
    genome.personaId = personaId;
    genome.baseModel = 'llama-3.2-3b';
    genome.layers = [];
    genome.compositeEmbedding = Array(768).fill(0); // Required 768-dim embedding
    genome.metadata = {
      generation: 0,
      createdVia: 'manual',
      lastModified: new Date(),
    };
    genome.fitness = {
      overallAccuracy: 0,
      totalParameters: 0,
      totalSizeMB: 0,
      averageLatency: 0,
      usageCount: 0,
      successRate: 0,
    };
    genome.tags = [];
    genome.createdAt = new Date();
    genome.updatedAt = new Date();
    return genome;
  }
}
