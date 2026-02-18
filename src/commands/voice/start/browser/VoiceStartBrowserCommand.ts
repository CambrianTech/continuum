/**
 * Voice Start Command - Browser Implementation
 *
 * Start voice chat session for real-time audio communication with AI
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { VoiceStartParams, VoiceStartResult } from '../shared/VoiceStartTypes';

export class VoiceStartBrowserCommand extends CommandBase<VoiceStartParams, VoiceStartResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/start', context, subpath, commander);
  }

  async execute(params: VoiceStartParams): Promise<VoiceStartResult> {
    console.log('🌐 BROWSER: Delegating Voice Start to server');
    return await this.remoteExecute(params);
  }
}
