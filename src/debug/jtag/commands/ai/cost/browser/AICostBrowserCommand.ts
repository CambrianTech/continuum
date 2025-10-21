/**
 * AI Cost Browser Command
 *
 * Routes to server (cost queries are server-side only)
 */

import { AICostCommand } from '../shared/AICostCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AICostParams, AICostResult } from '../shared/AICostTypes';

export class AICostBrowserCommand extends AICostCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/cost', context, subpath, commander);
  }

  async execute(params: AICostParams): Promise<AICostResult> {
    // Cost queries are server-side only - return empty result
    return {
      context: params.context,
      sessionId: params.sessionId,
      success: false,
      error: 'AI cost queries are server-side only - command must be executed on server',
      summary: {
        totalCost: 0,
        totalGenerations: 0,
        totalTokens: 0,
        inputTokens: 0,
        outputTokens: 0,
        avgCostPerGeneration: 0,
        avgTokensPerGeneration: 0,
        avgResponseTime: 0,
        timeRange: {
          start: new Date().toISOString(),
          end: new Date().toISOString(),
          duration: '0s'
        }
      }
    };
  }
}
