/**
 * AI Report Browser Command
 *
 * Routes to server (AI reports are server-side only)
 */

import { AIReportCommand } from '../shared/AIReportCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIReportParams, AIReportResult } from '../shared/AIReportTypes';

export class AIReportBrowserCommand extends AIReportCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/report', context, subpath, commander);
  }

  async execute(params: AIReportParams): Promise<AIReportResult> {
    // AI reports are server-side only - route to server
    return await this.remoteExecute({
      ...params,
      context: params.context ?? this.context,
      sessionId: params.sessionId ?? ''
    });
  }
}
