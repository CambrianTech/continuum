/**
 * Voice Stop Command - Browser Implementation
 *
 * Stop an active voice chat session
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { VoiceStopParams, VoiceStopResult } from '../shared/VoiceStopTypes';

export class VoiceStopBrowserCommand extends CommandBase<VoiceStopParams, VoiceStopResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/stop', context, subpath, commander);
  }

  async execute(params: VoiceStopParams): Promise<VoiceStopResult> {
    console.log('🌐 BROWSER: Delegating Voice Stop to server');
    return await this.remoteExecute(params);
  }
}
