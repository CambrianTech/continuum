/**
 * Genome Unregister Command - Server Implementation
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { GenomeUnregisterParams, GenomeUnregisterResult } from '../shared/GenomeUnregisterTypes';
import { createGenomeUnregisterResultFromParams } from '../shared/GenomeUnregisterTypes';
import { GenomeDaemon } from '../../../../system/genome/server/GenomeDaemon';

function getDaemon(): GenomeDaemon {
  return GenomeDaemon.getInstance({
    totalMemoryMB: 8192,
    defaultPersonaQuotaMB: 1024,
    hysteresisSeconds: 60,
    enableThrashingProtection: true
  });
}

export class GenomeUnregisterServerCommand extends CommandBase<GenomeUnregisterParams, GenomeUnregisterResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-unregister', context, subpath, commander);
  }

  async execute(params: GenomeUnregisterParams): Promise<GenomeUnregisterResult> {
    try {
      const daemon = getDaemon();

      await daemon.unregisterPersona(params.personaId);

      return createGenomeUnregisterResultFromParams(params, {
        success: true,
        unregistered: true
      });
    } catch (error) {
      return createGenomeUnregisterResultFromParams(params, {
        success: false,
        unregistered: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
