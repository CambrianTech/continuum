/**
 * Agent Stop Command - Browser Implementation
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { AgentStopParams, AgentStopResult } from '../shared/AgentStopTypes';

export class AgentStopBrowserCommand extends CommandBase<AgentStopParams, AgentStopResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('agent/stop', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<AgentStopResult> {
    return transformPayload(params, {
      success: false,
      handle: '',
      stopped: false,
      error: 'This command must be executed on the server',
    });
  }
}
