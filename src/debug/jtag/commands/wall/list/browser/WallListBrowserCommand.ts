/**
 * wall/list Browser Command
 */

import { WallListCommand } from '../shared/WallListCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { WallListParams, WallListResult } from '../../shared/WallTypes';

export class WallListBrowserCommand extends WallListCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('wall/list', context, subpath, commander);
  }

  async execute(params: WallListParams): Promise<WallListResult> {
    // Browser cannot read files - delegate to server
    throw new Error('wall/list must be executed on server');
  }
}
