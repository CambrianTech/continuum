/**
 * AI Validate-Response Browser Command
 *
 * Delegates to server (no AI generation in browser)
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIValidateResponseParams, AIValidateResponseResult } from '../shared/AIValidateResponseTypes';

export class AIValidateResponseBrowserCommand extends CommandBase<AIValidateResponseParams, AIValidateResponseResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/validate-response', context, subpath, commander);
  }

  async execute(params: AIValidateResponseParams): Promise<AIValidateResponseResult> {
    // Browser delegates all AI validation to server
    return {
      context: params.context,
      sessionId: params.sessionId,
      error: 'AI validate-response must be called from server',
      decision: 'SUBMIT',
      confidence: 0.0,
      reason: 'Command not available in browser'
    };
  }
}
