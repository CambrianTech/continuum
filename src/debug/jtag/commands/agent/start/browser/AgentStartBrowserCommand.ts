/**
 * Agent Start Command - Browser Implementation
 *
 * Routes to server which routes to Rust.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { AgentStartParams, AgentStartResult } from '../shared/AgentStartTypes';

export class AgentStartBrowserCommand extends CommandBase<AgentStartParams, AgentStartResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('agent/start', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<AgentStartResult> {
    return transformPayload(params, {
      success: false,
      handle: '',
      status: 'failed',
      error: 'This command must be executed on the server',
    });
  }
}
