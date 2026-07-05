/**
 * wall/read Browser Command
 *
 * Browser-side — delegates to server via remoteExecute
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { WallReadParams, WallReadResult } from '../../shared/WallTypes';

export class WallReadBrowserCommand extends CommandBase<WallReadParams, WallReadResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/wall/read', context, subpath, commander);
  }

  async execute(params: WallReadParams): Promise<WallReadResult> {
    return await this.remoteExecute(params);
  }
}
