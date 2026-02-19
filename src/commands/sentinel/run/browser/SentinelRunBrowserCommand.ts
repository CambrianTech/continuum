/**
 * Sentinel Run Command - Browser Implementation
 *
 * Browser simply delegates to server for sentinel execution.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '../../../../system/core/types/SystemScopes';
import type { SentinelRunParams, SentinelRunResult } from '../shared/SentinelRunTypes';

export class SentinelRunBrowserCommand extends CommandBase<SentinelRunParams, SentinelRunResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/run', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelRunResult> {
    // Delegate to server - sentinels run on server side
    return await this.remoteExecute({ ...params, userId: SYSTEM_SCOPES.SYSTEM });
  }
}
