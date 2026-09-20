/**
 * Agent List Command - Browser Implementation
 *
 * Routes to server which routes to Rust.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { AgentListParams, AgentListResult } from '../shared/AgentListTypes';

export class AgentListBrowserCommand extends CommandBase<AgentListParams, AgentListResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('agent/list', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<AgentListResult> {
    // Browser commands route through server
    return transformPayload(params, {
      success: false,
      agents: [],
      error: 'This command must be executed on the server',
    });
  }
}
