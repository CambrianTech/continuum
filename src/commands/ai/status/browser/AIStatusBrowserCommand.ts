/**
 * AI Status Browser Command
 *
 * Forwards to server implementation via executeRemote
 */

import { AIStatusCommand } from '../shared/AIStatusCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIStatusParams, AIStatusResult } from '../shared/AIStatusTypes';

export class AIStatusBrowserCommand extends AIStatusCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/status', context, subpath, commander);
  }

  async execute(params: AIStatusParams): Promise<AIStatusResult> {
    // Forward to server implementation
    return this.remoteExecute(params);
  }
}
