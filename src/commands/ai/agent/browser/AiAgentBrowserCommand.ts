/**
 * AI Agent Command - Browser Implementation
 * ==========================================
 *
 * Browser delegates to server for agentic loop execution.
 * All LLM calls and tool execution happen server-side.
 */

import { AiAgentCommand } from '../shared/AiAgentCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AiAgentParams, AiAgentResult } from '../shared/AiAgentTypes';

export class AiAgentBrowserCommand extends AiAgentCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  async execute(params: AiAgentParams): Promise<AiAgentResult> {
    return this.remoteExecute<AiAgentParams, AiAgentResult>(params, 'ai/agent', 'server');
  }
}
