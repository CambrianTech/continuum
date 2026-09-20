/**
 * Collaboration Live Transcription Command - Browser Implementation
 *
 * Relay voice transcription from browser to server for AI processing
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CollaborationLiveTranscriptionParams, CollaborationLiveTranscriptionResult } from '../shared/CollaborationLiveTranscriptionTypes';

export class CollaborationLiveTranscriptionBrowserCommand extends CommandBase<CollaborationLiveTranscriptionParams, CollaborationLiveTranscriptionResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/live/transcription', context, subpath, commander);
  }

  async execute(params: CollaborationLiveTranscriptionParams): Promise<CollaborationLiveTranscriptionResult> {
    console.log('🌐 BROWSER: Delegating Collaboration Live Transcription to server');
    return await this.remoteExecute(params);
  }
}
