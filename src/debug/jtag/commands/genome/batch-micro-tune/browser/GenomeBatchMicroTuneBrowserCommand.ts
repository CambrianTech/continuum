/**
 * GenomeBatchMicroTuneBrowserCommand - Browser stub
 *
 * All micro-tuning happens server-side.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  GenomeBatchMicroTuneParams,
  GenomeBatchMicroTuneResult
} from '../shared/GenomeBatchMicroTuneTypes';

export class GenomeBatchMicroTuneBrowserCommand extends CommandBase<
  GenomeBatchMicroTuneParams,
  GenomeBatchMicroTuneResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-batch-micro-tune', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeBatchMicroTuneResult> {
    // Browser doesn't do training - forward to server
    return transformPayload(params, {
      success: false,
      error: 'Micro-tuning must happen on server'
    });
  }
}
