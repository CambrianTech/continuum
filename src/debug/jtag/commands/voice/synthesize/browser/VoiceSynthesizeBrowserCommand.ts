/**
 * Voice Synthesize Command - Browser Implementation
 *
 * Synthesize text to speech using Rust TTS (Kokoro primary). Wraps the streaming-core TTS adapters for text-to-speech conversion.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { VoiceSynthesizeParams, VoiceSynthesizeResult } from '../shared/VoiceSynthesizeTypes';

export class VoiceSynthesizeBrowserCommand extends CommandBase<VoiceSynthesizeParams, VoiceSynthesizeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/synthesize', context, subpath, commander);
  }

  async execute(params: VoiceSynthesizeParams): Promise<VoiceSynthesizeResult> {
    console.log('🌐 BROWSER: Delegating Voice Synthesize to server');
    return await this.remoteExecute(params);
  }
}
