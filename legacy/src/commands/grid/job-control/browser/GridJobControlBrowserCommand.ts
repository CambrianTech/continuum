/**
 * Grid Job Control Command - Browser Implementation
 *
 * Control a running or queued forge job: pause (checkpoint + stop), resume (reload checkpoint + continue), cancel (kill + clean up). Uses the grid transport layer.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridJobControlParams, GridJobControlResult } from '../shared/GridJobControlTypes';

export class GridJobControlBrowserCommand extends CommandBase<GridJobControlParams, GridJobControlResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('grid/job-control', context, subpath, commander);
  }

  async execute(params: GridJobControlParams): Promise<GridJobControlResult> {
    console.log('🌐 BROWSER: Delegating Grid Job Control to server');
    return await this.remoteExecute(params);
  }
}
