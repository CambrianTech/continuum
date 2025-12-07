/**
 * wall/write Browser Command
 *
 * Browser-side stub - delegates to server
 */

import { WallWriteCommand } from '../shared/WallWriteCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { WallWriteParams, WallWriteResult } from '../../shared/WallTypes';

export class WallWriteBrowserCommand extends WallWriteCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('wall/write', context, subpath, commander);
  }

  async execute(params: WallWriteParams): Promise<WallWriteResult> {
    // Browser cannot write files - delegate to server
    throw new Error('wall/write must be executed on server');
  }
}
