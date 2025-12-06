/**
 * wall/read Base Command
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { WallReadParams, WallReadResult } from '../../shared/WallTypes';

export abstract class WallReadCommand extends CommandBase<WallReadParams, WallReadResult> {
  constructor(path: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(path, context, subpath, commander);
  }

  abstract execute(params: WallReadParams): Promise<WallReadResult>;
}
