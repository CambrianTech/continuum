/**
 * Genome Compose Command - Server Implementation
 *
 * Composes multiple trained LoRA layers into a single stacked genome.
 * Optionally activates the composed genome on the persona, triggering
 * LRU eviction if memory pressure exceeds quota.
 *
 * Flow:
 * 1. Validate all layer IDs exist as GenomeLayerEntities
 * 2. Register each layer as a MockLoRAAdapter in the GenomeDaemon registry
 * 3. Create a composed genome entity tracking the layer stack
 * 4. Optionally activate via genome/paging-activate (triggers LRU eviction)
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type {
  GenomeComposeParams,
  GenomeComposeResult,
} from '../shared/GenomeComposeTypes';
import { createGenomeComposeResultFromParams } from '../shared/GenomeComposeTypes';
import { Commands } from '@system/core/shared/Commands';
import { GenomeLayerEntity } from '@system/genome/entities/GenomeLayerEntity';
import type { CompositionStrategy } from '@system/genome/shared/GenomeAssemblyTypes';

export class GenomeComposeServerCommand extends CommandBase<GenomeComposeParams, GenomeComposeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/compose', context, subpath, commander);
  }

  async execute(params: GenomeComposeParams): Promise<GenomeComposeResult> {
    const startTime = Date.now();
    const { personaId, layers, baseModel } = params;
    const strategy: CompositionStrategy = params.strategy ?? 'weighted-merge';
    const shouldActivate = params.activate !== false; // default true

    console.log(`🧬 GENOME COMPOSE: ${layers.length} layers, strategy=${strategy}, activate=${shouldActivate}`);

    if (!layers || layers.length === 0) {
      throw new ValidationError('layers', 'At least one layer is required for composition');
    }

    if (!personaId) {
      throw new ValidationError('personaId', 'personaId is required');
    }

    if (!baseModel) {
      throw new ValidationError('baseModel', 'baseModel is required');
    }

    // Step 1: Validate all layers exist
    const validatedLayers: Array<{
      layerId: string;
      name: string;
      domain: string;
      sizeMB: number;
      weight: number;
      ordering: number;
    }> = [];

    for (let i = 0; i < layers.length; i++) {
      const layerRef = layers[i];
      const readResult = await Commands.execute('data/read', {
        collection: GenomeLayerEntity.collection,
        id: layerRef.layerId,
      } as any) as any;

      if (!readResult?.success || !readResult?.data) {
        return createGenomeComposeResultFromParams(params, {
          success: false,
          error: `Layer not found: ${layerRef.layerId} (index ${i})`,
        });
      }

      const entity = readResult.data;
      validatedLayers.push({
        layerId: entity.id,
        name: entity.name,
        domain: entity.traitType,
        sizeMB: entity.sizeMB ?? 0,
        weight: layerRef.weight ?? 1.0,
        ordering: layerRef.ordering ?? i,
      });
    }

    console.log(`   Validated ${validatedLayers.length} layers:`);
    for (const layer of validatedLayers) {
      console.log(`     - ${layer.name} (${layer.domain}) weight=${layer.weight} size=${layer.sizeMB}MB`);
    }

    // Step 2: Register each layer in the paging registry (if not already registered)
    for (const layer of validatedLayers) {
      try {
        await Commands.execute('genome/paging-adapter-register', {
          layerId: layer.layerId,
          adapterId: layer.layerId,
          name: layer.name,
          domain: layer.domain,
          sizeMB: layer.sizeMB,
        } as any);
      } catch (err) {
        // Already registered is OK
        const msg = err instanceof Error ? err.message : String(err);
        if (!msg.includes('already registered') && !msg.includes('already exists')) {
          console.warn(`   Warning: failed to register ${layer.layerId}: ${msg}`);
        }
      }
    }

    // Step 3: Create composed genome entity
    const genomeName = params.name ??
      `composed-${personaId.slice(0, 8)}-${Date.now()}`;

    const genomeData = {
      personaId,
      name: genomeName,
      baseModel,
      strategy,
      layers: validatedLayers.map(l => ({
        layerId: l.layerId,
        weight: l.weight,
        ordering: l.ordering,
      })),
      layerCount: validatedLayers.length,
      totalSizeMB: validatedLayers.reduce((sum, l) => sum + l.sizeMB, 0),
      composedAt: new Date().toISOString(),
    };

    const createResult = await Commands.execute('data/create', {
      collection: 'composed_genomes',
      data: genomeData,
    } as any) as any;

    const genomeId = createResult?.data?.id;
    console.log(`   Created composed genome: ${genomeId}`);

    // Step 4: Activate on persona if requested
    let activated = false;
    let evictedAdapters: string[] | undefined;

    if (shouldActivate) {
      // Activate each layer individually on the persona
      // GenomeDaemon handles LRU eviction internally
      for (const layer of validatedLayers) {
        try {
          const activateResult = await Commands.execute('genome/paging-activate', {
            personaId,
            adapterId: layer.layerId,
          } as any) as any;

          if (activateResult?.success && activateResult?.loaded) {
            console.log(`   Activated ${layer.name} on persona`);
            if (activateResult.evictedAdapters?.length) {
              evictedAdapters = [
                ...(evictedAdapters ?? []),
                ...activateResult.evictedAdapters,
              ];
            }
          } else if (activateResult?.thrashingDetected) {
            console.warn(`   Thrashing detected for ${layer.name}, skipping activation`);
          }
        } catch (err) {
          console.warn(`   Activation failed for ${layer.name}: ${err instanceof Error ? err.message : err}`);
        }
      }
      activated = true;
    }

    const compositionTimeMs = Date.now() - startTime;

    console.log(`   Composition complete in ${compositionTimeMs}ms`);

    return createGenomeComposeResultFromParams(params, {
      success: true,
      genomeId,
      layerCount: validatedLayers.length,
      compositionTimeMs,
      activated,
      evictedAdapters,
      strategy,
    });
  }
}
