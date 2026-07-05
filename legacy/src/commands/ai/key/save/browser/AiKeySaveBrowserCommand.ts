/**
 * Ai Key Save Command - Browser Implementation
 *
 * Save an API key for a cloud AI provider. Persists to ~/.continuum/config.env, sets process.env, and emits system:config:key-added event to trigger persona creation.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AiKeySaveParams, AiKeySaveResult } from '../shared/AiKeySaveTypes';

export class AiKeySaveBrowserCommand extends CommandBase<AiKeySaveParams, AiKeySaveResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/key/save', context, subpath, commander);
  }

  async execute(params: AiKeySaveParams): Promise<AiKeySaveResult> {
    console.log('🌐 BROWSER: Delegating Ai Key Save to server');
    return await this.remoteExecute(params);
  }
}
