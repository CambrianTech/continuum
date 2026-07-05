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
    // Cost queries are server-side only - route to server
    return await this.remoteExecute({
      ...params,
      context: params.context ?? this.context,
      sessionId: params.sessionId ?? ''
    });
  }
}
