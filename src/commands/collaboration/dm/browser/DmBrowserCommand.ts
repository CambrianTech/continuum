/**
 * DM Command - Browser Implementation
 * Delegates to server for actual room creation/lookup
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DmParams, DmResult } from '../shared/DmTypes';

export class DmBrowserCommand extends CommandBase<DmParams, DmResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/dm', context, subpath, commander);
  }

  async execute(params: DmParams): Promise<DmResult> {
    return await this.remoteExecute(params);
  }
}
