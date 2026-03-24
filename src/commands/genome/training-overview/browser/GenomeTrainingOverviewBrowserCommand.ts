/**
 * Genome Training Overview Command - Browser Implementation
 *
 * Aggregate all training data across local and grid nodes in one call. Returns adapters with loss histories, academy sessions, and per-node stats. Used by the training dashboard to avoid sequential grid/send chains from the browser.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeTrainingOverviewParams, GenomeTrainingOverviewResult } from '../shared/GenomeTrainingOverviewTypes';

export class GenomeTrainingOverviewBrowserCommand extends CommandBase<GenomeTrainingOverviewParams, GenomeTrainingOverviewResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/training-overview', context, subpath, commander);
  }

  async execute(params: GenomeTrainingOverviewParams): Promise<GenomeTrainingOverviewResult> {
    console.log('🌐 BROWSER: Delegating Genome Training Overview to server');
    return await this.remoteExecute(params);
  }
}
