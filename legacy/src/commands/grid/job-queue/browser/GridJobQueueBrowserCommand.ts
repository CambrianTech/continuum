/**
 * Grid Job Queue Command - Browser Implementation
 *
 * List all jobs on a grid node: queued, running, paused, completed, failed. Shows the full job lifecycle with alloy names, progress, and timing.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridJobQueueParams, GridJobQueueResult } from '../shared/GridJobQueueTypes';

export class GridJobQueueBrowserCommand extends CommandBase<GridJobQueueParams, GridJobQueueResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('grid/job-queue', context, subpath, commander);
  }

  async execute(params: GridJobQueueParams): Promise<GridJobQueueResult> {
    console.log('🌐 BROWSER: Delegating Grid Job Queue to server');
    return await this.remoteExecute(params);
  }
}
