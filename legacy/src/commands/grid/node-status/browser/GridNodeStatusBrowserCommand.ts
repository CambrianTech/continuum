/**
 * Grid Node Status Command - Browser Implementation
 *
 * Query a grid node's current state: GPU utilization, running jobs, queue depth, temperature. Uses the grid transport layer (Tailscale now, Reticulum later).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridNodeStatusParams, GridNodeStatusResult } from '../shared/GridNodeStatusTypes';

export class GridNodeStatusBrowserCommand extends CommandBase<GridNodeStatusParams, GridNodeStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('grid/node-status', context, subpath, commander);
  }

  async execute(params: GridNodeStatusParams): Promise<GridNodeStatusResult> {
    console.log('🌐 BROWSER: Delegating Grid Node Status to server');
    return await this.remoteExecute(params);
  }
}
