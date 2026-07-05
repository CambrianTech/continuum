/**
 * AI Should Respond Fast Browser Command
 *
 * Delegates to server (bag-of-words scoring happens server-side)
 */

import { ShouldRespondFastCommand } from '../shared/ShouldRespondFastCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ShouldRespondFastParams, ShouldRespondFastResult } from '../shared/ShouldRespondFastTypes';

export class ShouldRespondFastBrowserCommand extends ShouldRespondFastCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/should-respond-fast', context, subpath, commander);
  }

  async execute(params: ShouldRespondFastParams): Promise<ShouldRespondFastResult> {
    // Browser delegates bag-of-words scoring to server
    return {
      context: params.context,
      sessionId: params.sessionId,
      success: false,
      error: 'AI should-respond-fast must be called from server',
      shouldRespond: false,
      score: 0,
      scoreBreakdown: {
        directMention: 0,
        domainKeywords: 0,
        conversationContext: 0,
        isQuestion: 0,
        unansweredQuestion: 0,
        roomActivity: 0,
        humanMessage: 0
      },
      signals: {
        wasMentioned: false,
        matchedKeywords: [],
        isQuestion: false,
        recentlyActive: false,
        isHumanMessage: false
      },
      reasoning: 'Command not available in browser'
    };
  }
}
