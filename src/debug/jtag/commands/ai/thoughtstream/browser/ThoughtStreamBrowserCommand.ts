/**
 * AI ThoughtStream Browser Command
 */

import { ThoughtStreamCommand } from '../shared/ThoughtStreamCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ThoughtStreamParams, ThoughtStreamResult } from '../shared/ThoughtStreamTypes';

export class ThoughtStreamBrowserCommand extends ThoughtStreamCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/thoughtstream', context, subpath, commander);
  }

  async execute(params: ThoughtStreamParams): Promise<ThoughtStreamResult> {
    // ThoughtStream inspection is server-side only - route to server
    return await this.remoteExecute({
      ...params,
      context: params.context ?? this.context,
      sessionId: params.sessionId ?? ''
    });
  }
}
