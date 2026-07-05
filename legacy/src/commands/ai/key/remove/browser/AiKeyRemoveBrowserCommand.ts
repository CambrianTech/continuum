/**
 * Ai Key Remove Command - Browser Implementation
 *
 * Remove an API key for a cloud AI provider. Removes from ~/.continuum/config.env, clears process.env, and emits system:config:key-removed event to deactivate personas.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AiKeyRemoveParams, AiKeyRemoveResult } from '../shared/AiKeyRemoveTypes';

export class AiKeyRemoveBrowserCommand extends CommandBase<AiKeyRemoveParams, AiKeyRemoveResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/key/remove', context, subpath, commander);
  }

  async execute(params: AiKeyRemoveParams): Promise<AiKeyRemoveResult> {
    console.log('🌐 BROWSER: Delegating Ai Key Remove to server');
    return await this.remoteExecute(params);
  }
}
