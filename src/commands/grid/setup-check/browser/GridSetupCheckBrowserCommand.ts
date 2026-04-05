/**
 * Grid Setup Check Command - Browser Implementation
 *
 * Diagnose grid setup: Tailscale install, connectivity, HTTPS certs, peers, Docker grid profile, and actionable fix steps. Run this to see what's needed before enabling distributed compute.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridSetupCheckParams, GridSetupCheckResult } from '../shared/GridSetupCheckTypes';

export class GridSetupCheckBrowserCommand extends CommandBase<GridSetupCheckParams, GridSetupCheckResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('grid/setup-check', context, subpath, commander);
  }

  async execute(params: GridSetupCheckParams): Promise<GridSetupCheckResult> {
    console.log('🌐 BROWSER: Delegating Grid Setup Check to server');
    return await this.remoteExecute(params);
  }
}
