/**
 * Ai Key Status Command - Browser Implementation
 *
 * Report redacted API-key availability and fingerprints without exposing raw or masked secret values.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AiKeyStatusParams, AiKeyStatusResult } from '../shared/AiKeyStatusTypes';

export class AiKeyStatusBrowserCommand extends CommandBase<AiKeyStatusParams, AiKeyStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/key/status', context, subpath, commander);
  }

  async execute(params: AiKeyStatusParams): Promise<AiKeyStatusResult> {
    console.log('🌐 BROWSER: Delegating Ai Key Status to server');
    return await this.remoteExecute(params);
  }
}
