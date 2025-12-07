/**
 * AI Logs Browser Command
 *
 * Routes to server (AI logs are server-side only)
 */

import { AILogsCommand } from '../shared/AILogsCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AILogsParams, AILogsResult } from '../shared/AILogsTypes';

export class AILogsBrowserCommand extends AILogsCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/logs', context, subpath, commander);
  }

  async execute(params: AILogsParams): Promise<AILogsResult> {
    // AI decision logs are server-side only - route to server
    return await this.remoteExecute({
      ...params,
      context: params.context ?? this.context,
      sessionId: params.sessionId ?? ''
    });
  }
}
