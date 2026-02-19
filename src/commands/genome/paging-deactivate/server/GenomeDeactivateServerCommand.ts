/**
 * Genome Deactivate Command - Server Implementation
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { GenomeDeactivateParams, GenomeDeactivateResult } from '../shared/GenomeDeactivateTypes';
import { createGenomeDeactivateResultFromParams } from '../shared/GenomeDeactivateTypes';
import { GenomeDaemon } from '../../../../system/genome/server/GenomeDaemon';

function getDaemon(): GenomeDaemon {
  return GenomeDaemon.getInstance({
    totalMemoryMB: 8192,
    defaultPersonaQuotaMB: 1024,
    hysteresisSeconds: 60,
    enableThrashingProtection: true
  });
}

export class GenomeDeactivateServerCommand extends CommandBase<GenomeDeactivateParams, GenomeDeactivateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-deactivate', context, subpath, commander);
  }

  async execute(params: GenomeDeactivateParams): Promise<GenomeDeactivateResult> {
    try {
      const daemon = getDaemon();
      await daemon.unloadAdapter(params.personaId, params.adapterId);

      return createGenomeDeactivateResultFromParams(params, {
        success: true,
        unloaded: true
      });
    } catch (error) {
      return createGenomeDeactivateResultFromParams(params, {
        success: false,
        unloaded: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
