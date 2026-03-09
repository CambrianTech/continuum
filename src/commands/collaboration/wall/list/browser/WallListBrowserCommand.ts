/**
 * wall/list Browser Command
 *
 * Browser-side — delegates to server via remoteExecute
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { WallListParams, WallListResult } from '../../shared/WallTypes';

export class WallListBrowserCommand extends CommandBase<WallListParams, WallListResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/wall/list', context, subpath, commander);
  }

  async execute(params: WallListParams): Promise<WallListResult> {
    return await this.remoteExecute(params);
  }
}
