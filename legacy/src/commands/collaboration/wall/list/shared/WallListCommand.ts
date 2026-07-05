/**
 * wall/list Base Command
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { WallListParams, WallListResult } from '../../shared/WallTypes';

export abstract class WallListCommand extends CommandBase<WallListParams, WallListResult> {
  constructor(path: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(path, context, subpath, commander);
  }

  abstract execute(params: WallListParams): Promise<WallListResult>;
}
