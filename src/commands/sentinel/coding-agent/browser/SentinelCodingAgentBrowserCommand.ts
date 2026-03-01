/**
 * Sentinel Coding Agent Command — Browser Implementation
 *
 * Browser simply delegates to server — coding agents run server-side.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '../../../../system/core/types/SystemScopes';
import type { SentinelCodingAgentParams, SentinelCodingAgentResult } from '../shared/SentinelCodingAgentTypes';

export class SentinelCodingAgentBrowserCommand extends CommandBase<SentinelCodingAgentParams, SentinelCodingAgentResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/coding-agent', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelCodingAgentResult> {
    return await this.remoteExecute({ ...params, userId: SYSTEM_SCOPES.SYSTEM });
  }
}
