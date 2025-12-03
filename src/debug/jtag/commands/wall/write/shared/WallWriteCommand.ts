/**
 * wall/write Base Command
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { WallWriteParams, WallWriteResult } from '../../shared/WallTypes';

export abstract class WallWriteCommand extends CommandBase<WallWriteParams, WallWriteResult> {
  constructor(path: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(path, context, subpath, commander);
  }

  abstract execute(params: WallWriteParams): Promise<WallWriteResult>;
}
