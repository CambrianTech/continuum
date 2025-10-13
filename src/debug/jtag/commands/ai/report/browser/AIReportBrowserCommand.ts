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
    // AI reports are server-side only
    return {
      context: params.context,
      sessionId: params.sessionId,
      success: false,
      error: 'AI reports are server-side only - command must be executed on server',
      summary: {
        totalDecisions: 0,
        responseDecisions: 0,
        silentDecisions: 0,
        responseRate: 0,
        avgConfidence: 0,
        timeRange: {
          start: new Date().toISOString(),
          end: new Date().toISOString()
        }
      }
    };
  }
}
