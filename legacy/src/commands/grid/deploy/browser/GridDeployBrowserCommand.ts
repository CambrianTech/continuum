/**
 * Grid Deploy Command - Browser Implementation
 *
 * Pull latest code and rebuild on grid nodes. Runs git pull + npm run build:ts on each reachable node via SSH over Tailscale. Keeps all nodes in sync without manual SSH.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridDeployParams, GridDeployResult } from '../shared/GridDeployTypes';

export class GridDeployBrowserCommand extends CommandBase<GridDeployParams, GridDeployResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('grid/deploy', context, subpath, commander);
  }

  async execute(params: GridDeployParams): Promise<GridDeployResult> {
    console.log('🌐 BROWSER: Delegating Grid Deploy to server');
    return await this.remoteExecute(params);
  }
}
