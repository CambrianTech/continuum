/**
 * wall/read Browser Command
 */

import { WallReadCommand } from '../shared/WallReadCommand';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { WallReadParams, WallReadResult } from '../../shared/WallTypes';

export class WallReadBrowserCommand extends WallReadCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/wall/read', context, subpath, commander);
  }

  async execute(params: WallReadParams): Promise<WallReadResult> {
    // Browser cannot read files - delegate to server
    throw new Error('wall/read must be executed on server');
  }
}
