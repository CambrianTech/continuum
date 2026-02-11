/**
 * Sentinel Status Command - Browser Implementation
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { SentinelStatusParams, SentinelStatusResult } from '../shared/SentinelStatusTypes';

export class SentinelStatusBrowserCommand extends CommandBase<SentinelStatusParams, SentinelStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/status', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelStatusResult> {
    return await this.remoteExecute(params);
  }
}
