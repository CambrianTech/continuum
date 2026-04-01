/**
 * Grid Job Submit Command - Browser Implementation
 *
 * Submit a forge job to a grid node's queue. The node executes when ready (GPU free). Returns a job ID for tracking. Replaces direct SSH forge execution.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridJobSubmitParams, GridJobSubmitResult } from '../shared/GridJobSubmitTypes';

export class GridJobSubmitBrowserCommand extends CommandBase<GridJobSubmitParams, GridJobSubmitResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('grid/job-submit', context, subpath, commander);
  }

  async execute(params: GridJobSubmitParams): Promise<GridJobSubmitResult> {
    console.log('🌐 BROWSER: Delegating Grid Job Submit to server');
    return await this.remoteExecute(params);
  }
}
