/**
 * Agent Status Command - Browser Implementation
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { AgentStatusParams, AgentStatusResult } from '../shared/AgentStatusTypes';

export class AgentStatusBrowserCommand extends CommandBase<AgentStatusParams, AgentStatusResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('agent/status', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<AgentStatusResult> {
    return transformPayload(params, {
      success: false,
      handle: '',
      task: '',
      status: 'not_found',
      iteration: 0,
      startedAt: 0,
      error: 'This command must be executed on the server',
    });
  }
}
