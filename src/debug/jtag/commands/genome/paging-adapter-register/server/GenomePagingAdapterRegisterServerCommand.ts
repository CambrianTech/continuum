/**
 * Genome Paging Adapter Register Command - Server Implementation
 *
 * Registers a mock LoRA adapter in the global adapter registry.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { GenomePagingAdapterRegisterParams, GenomePagingAdapterRegisterResult } from '../shared/GenomePagingAdapterRegisterTypes';
import { createGenomePagingAdapterRegisterResultFromParams } from '../shared/GenomePagingAdapterRegisterTypes';
import { GenomeDaemon } from '../../../../system/genome/server/GenomeDaemon';
import { MockLoRAAdapter } from '../../../../system/genome/shared/MockLoRAAdapter';

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

      // Create mock adapter
      const adapter = new MockLoRAAdapter({
        id: params.adapterId,
        name: params.name,
        domain: params.domain,
        sizeMB: params.sizeMB,
        priority: params.priority ?? 0.5
      });

      // Register in registry
      registry.register(adapter);

      return createGenomePagingAdapterRegisterResultFromParams(params, {
        success: true,
        registered: true,
        adapterId: params.adapterId
      });
    } catch (error) {
      return createGenomePagingAdapterRegisterResultFromParams(params, {
        success: false,
        registered: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
