/**
 * wall/write Browser Command
 *
 * Browser-side — delegates to server via remoteExecute
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { WallWriteParams, WallWriteResult } from '../../shared/WallTypes';

export class WallWriteBrowserCommand extends CommandBase<WallWriteParams, WallWriteResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/wall/write', context, subpath, commander);
  }

  async execute(params: WallWriteParams): Promise<WallWriteResult> {
    return await this.remoteExecute(params);
  }
}
