/**
 * Collaboration Live Transcription Command - Server Implementation
 *
 * Relay voice transcription from browser to server for AI processing
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { Events } from '@system/core/shared/Events';
// import { ValidationError } from '@system/core/types/ErrorTypes';  // Uncomment when adding validation
import type { CollaborationLiveTranscriptionParams, CollaborationLiveTranscriptionResult } from '../shared/CollaborationLiveTranscriptionTypes';
import { createCollaborationLiveTranscriptionResultFromParams } from '../shared/CollaborationLiveTranscriptionTypes';

export class CollaborationLiveTranscriptionServerCommand extends CommandBase<CollaborationLiveTranscriptionParams, CollaborationLiveTranscriptionResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/live/transcription', context, subpath, commander);
  }

  async execute(params: CollaborationLiveTranscriptionParams): Promise<CollaborationLiveTranscriptionResult> {
    console.log(`[STEP 10] 🎙️ SERVER: Relaying transcription to VoiceOrchestrator: "${params.transcript.slice(0, 50)}..."`);

    // Emit the voice:transcription event on the SERVER Events bus
    // This allows VoiceOrchestrator (server-side) to receive the transcription
    // Use callSessionId (the call UUID) so VoiceOrchestrator can look up the session context
    Events.emit('voice:transcription', {
      sessionId: params.callSessionId,  // Call session UUID
      speakerId: params.speakerId,
      speakerName: params.speakerName,
      transcript: params.transcript,
      confidence: params.confidence,
      language: params.language,
      timestamp: params.timestamp
    });

    console.log(`[STEP 10] ✅ Transcription event emitted on server Events bus`);

    // Return successful result
    return createCollaborationLiveTranscriptionResultFromParams(params, {
      success: true,
      message: `Transcription relayed to VoiceOrchestrator: "${params.transcript.slice(0, 30)}..."`
    });
  }
}
