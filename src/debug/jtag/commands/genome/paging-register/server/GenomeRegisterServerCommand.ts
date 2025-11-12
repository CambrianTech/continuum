/**
 * Genome Register Command - Server Implementation
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { GenomeRegisterParams, GenomeRegisterResult } from '../shared/GenomeRegisterTypes';
import { createGenomeRegisterResultFromParams } from '../shared/GenomeRegisterTypes';
import { GenomeDaemon } from '../../../../system/genome/server/GenomeDaemon';

function getDaemon(): GenomeDaemon {
  return GenomeDaemon.getInstance({
    totalMemoryMB: 8192,
    defaultPersonaQuotaMB: 1024,
    hysteresisSeconds: 60,
    enableThrashingProtection: true
  });
}

export class GenomeRegisterServerCommand extends CommandBase<GenomeRegisterParams, GenomeRegisterResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-register', context, subpath, commander);
  }

  async execute(params: GenomeRegisterParams): Promise<GenomeRegisterResult> {
    try {
      const daemon = getDaemon();

      daemon.registerPersona(
        params.personaId,
        params.displayName,
        params.quotaMB,
        params.priority
      );

      return createGenomeRegisterResultFromParams(params, {
        success: true,
        registered: true
      });
    } catch (error) {
      return createGenomeRegisterResultFromParams(params, {
        success: false,
        registered: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
