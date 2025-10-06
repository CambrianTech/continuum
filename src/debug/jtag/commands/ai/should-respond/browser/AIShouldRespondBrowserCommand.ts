/**
 * AI Should-Respond Browser Command
 *
 * Delegates to server (no AI generation in browser)
 */

import { AIShouldRespondCommand } from '../shared/AIShouldRespondCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIShouldRespondParams, AIShouldRespondResult } from '../shared/AIShouldRespondTypes';

export class AIShouldRespondBrowserCommand extends AIShouldRespondCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/should-respond', context, subpath, commander);
  }

  async execute(params: AIShouldRespondParams): Promise<AIShouldRespondResult> {
    // Browser delegates all AI decisions to server
    return {
      context: params.context,
      sessionId: params.sessionId,
      error: 'AI should-respond must be called from server',
      shouldRespond: false,
      confidence: 0.0,
      reason: 'Command not available in browser',
      factors: {
        mentioned: false,
        questionAsked: false,
        domainRelevant: false,
        recentlySpoke: false,
        othersAnswered: false
      }
    };
  }
}
