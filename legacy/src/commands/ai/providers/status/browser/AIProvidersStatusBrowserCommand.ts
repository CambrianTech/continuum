/**
 * AI Providers Status Browser Command
 *
 * Forwards to server implementation via remoteExecute.
 * Server checks SecretManager for configured API keys.
 */

import { AIProvidersStatusCommand } from '../shared/AIProvidersStatusCommand';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { AIProvidersStatusParams, AIProvidersStatusResult } from '../shared/AIProvidersStatusTypes';

export class AIProvidersStatusBrowserCommand extends AIProvidersStatusCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/providers/status', context, subpath, commander);
  }

  async execute(params: AIProvidersStatusParams): Promise<AIProvidersStatusResult> {
    // Forward to server - only server has access to SecretManager
    return this.remoteExecute(params);
  }
}
