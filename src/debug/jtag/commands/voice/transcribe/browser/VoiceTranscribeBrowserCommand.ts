/**
 * Voice Transcribe Command - Browser Implementation
 *
 * Transcribe audio to text using Rust Whisper (STT). Wraps the streaming-core Whisper adapter for speech-to-text conversion.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { VoiceTranscribeParams, VoiceTranscribeResult } from '../shared/VoiceTranscribeTypes';

export class VoiceTranscribeBrowserCommand extends CommandBase<VoiceTranscribeParams, VoiceTranscribeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/transcribe', context, subpath, commander);
  }

  async execute(params: VoiceTranscribeParams): Promise<VoiceTranscribeResult> {
    console.log('🌐 BROWSER: Delegating Voice Transcribe to server');
    return await this.remoteExecute(params);
  }
}
