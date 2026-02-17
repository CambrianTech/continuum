/**
 * AI Agent Command - Shared Abstract Base
 * ========================================
 *
 * Universal agentic loop command. Server-only — browser delegates.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AiAgentParams, AiAgentResult } from './AiAgentTypes';

export abstract class AiAgentCommand extends CommandBase<AiAgentParams, AiAgentResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai-agent', context, subpath, commander);
  }

  abstract execute(params: AiAgentParams): Promise<AiAgentResult>;
}
