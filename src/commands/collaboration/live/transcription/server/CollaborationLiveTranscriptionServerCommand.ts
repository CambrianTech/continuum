/**
 * Collaboration Live Transcription Command - Server Implementation
 *
 * Relay voice transcription from browser to server for AI processing
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { Events } from '@system/core/shared/Events';
import { getRustVoiceOrchestrator } from '@system/voice/server';
import type { CollaborationLiveTranscriptionParams, CollaborationLiveTranscriptionResult } from '../shared/CollaborationLiveTranscriptionTypes';
import { createCollaborationLiveTranscriptionResultFromParams } from '../shared/CollaborationLiveTranscriptionTypes';

export class CollaborationLiveTranscriptionServerCommand extends CommandBase<CollaborationLiveTranscriptionParams, CollaborationLiveTranscriptionResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/live/transcription', context, subpath, commander);
  }

  async execute(params: CollaborationLiveTranscriptionParams): Promise<CollaborationLiveTranscriptionResult> {
    // Emit event for any subscribers (VoiceOrchestrator TS subscribes to this)
    Events.emit('voice:transcription', {
      sessionId: params.callSessionId,
      speakerId: params.speakerId,
      speakerName: params.speakerName,
      transcript: params.transcript,
      confidence: params.confidence,
      language: params.language,
      timestamp: params.timestamp
    });

    // Route through Rust VoiceOrchestrator for AI participant notification
    const responderIds = await getRustVoiceOrchestrator().onUtterance({
      sessionId: params.callSessionId,
      speakerId: params.speakerId,
      speakerName: params.speakerName,
      speakerType: 'human',
      transcript: params.transcript,
      confidence: params.confidence,
      timestamp: params.timestamp,
    });

    // Emit directed events to each AI responder
    for (const targetId of responderIds) {
      Events.emit('voice:transcription:directed', {
        sessionId: params.callSessionId,
        speakerId: params.speakerId,
        speakerName: params.speakerName,
        transcript: params.transcript,
        confidence: params.confidence,
        timestamp: params.timestamp,
        targetPersonaId: targetId,
      });
    }

    return createCollaborationLiveTranscriptionResultFromParams(params, {
      success: true,
      message: `Transcription → ${responderIds.length} AI responders`
    });
  }
}
