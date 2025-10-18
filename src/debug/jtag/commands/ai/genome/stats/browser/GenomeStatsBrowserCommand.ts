/**
 * Genome Stats Browser Command
 *
 * Delegates to server for genome performance monitoring.
 * Browser has no direct access to process pools or inference workers.
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import type { GenomeStatsParams, GenomeStatsResult } from '../shared/GenomeStatsTypes';

export class GenomeStatsBrowserCommand extends CommandBase<GenomeStatsParams, GenomeStatsResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/genome/stats', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeStatsResult> {
    // Browser delegates entirely to server
    return await this.remoteExecute(params);
  }
}
