/**
 * Genome Paging Adapter Register Command - Server Implementation
 *
 * Registers a LoRA adapter in the global adapter registry.
 * Accepts either a layerId (to hydrate from persisted GenomeLayerEntity)
 * or raw params for mock/legacy adapters.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { GenomePagingAdapterRegisterParams, GenomePagingAdapterRegisterResult } from '../shared/GenomePagingAdapterRegisterTypes';
import { createGenomePagingAdapterRegisterResultFromParams } from '../shared/GenomePagingAdapterRegisterTypes';
import { GenomeDaemon } from '../../../../system/genome/server/GenomeDaemon';
import { MockLoRAAdapter } from '../../../../system/genome/shared/MockLoRAAdapter';
import { GenomeLayerEntity } from '../../../../system/genome/entities/GenomeLayerEntity';
import { DataRead } from '@commands/data/read/shared/DataReadTypes';

function getDaemon(): GenomeDaemon {
  return GenomeDaemon.getInstance({
    totalMemoryMB: 8192,
    defaultPersonaQuotaMB: 1024,
    hysteresisSeconds: 60,
    enableThrashingProtection: true
  });
}

export class GenomePagingAdapterRegisterServerCommand extends CommandBase<GenomePagingAdapterRegisterParams, GenomePagingAdapterRegisterResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-paging-adapter-register', context, subpath, commander);
  }

  async execute(params: GenomePagingAdapterRegisterParams): Promise<GenomePagingAdapterRegisterResult> {
    try {
      const daemon = getDaemon();
      const registry = daemon.getRegistry();

      let adapterId = params.adapterId;
      let name = params.name;
      let domain = params.domain;
      let sizeMB = params.sizeMB;
      let priority = params.priority ?? 0.5;

      // If layerId is provided, hydrate adapter info from persisted GenomeLayerEntity
      if (params.layerId) {
        const readResult = await DataRead.execute<GenomeLayerEntity>({
          collection: GenomeLayerEntity.collection,
          id: params.layerId,
        });

        if (!readResult.success || !readResult.data) {
          return createGenomePagingAdapterRegisterResultFromParams(params, {
            success: false,
            registered: false,
            error: `GenomeLayerEntity not found for layerId: ${params.layerId}`,
          });
        }

        const entity = readResult.data;
        adapterId = entity.id;
        name = entity.name;
        domain = entity.traitType;
        sizeMB = entity.sizeMB;
      }

      // Create adapter from resolved params
      const adapter = new MockLoRAAdapter({
        id: adapterId,
        name,
        domain,
        sizeMB,
        priority,
      });

      // Register in registry
      registry.register(adapter);

      return createGenomePagingAdapterRegisterResultFromParams(params, {
        success: true,
        registered: true,
        adapterId,
      });
    } catch (error) {
      return createGenomePagingAdapterRegisterResultFromParams(params, {
        success: false,
        registered: false,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }
}
