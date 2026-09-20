/**
 * Live Send Command - Server Implementation
 *
 * Sends a text message into the active live voice session.
 * Auto-discovers session and speaker identity.
 * Mirrors collaboration/chat/send but for voice sessions.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { getTSVoiceOrchestrator } from '@system/voice/server';
import { CollaborationLiveTranscription } from '../../transcription/shared/CollaborationLiveTranscriptionTypes';
import type { LiveSendParams, LiveSendResult } from '../shared/LiveSendTypes';
import { Commands } from '@system/core/shared/Commands';
import type { DataListResult } from '@commands/data/list/shared/DataListTypes';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import type { UserEntity } from '@system/data/entities/UserEntity';

export class LiveSendServerCommand extends CommandBase<LiveSendParams, LiveSendResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/live/send', context, subpath, commander);
  }

  async execute(params: LiveSendParams): Promise<LiveSendResult> {
    const orchestrator = getTSVoiceOrchestrator();

    // Auto-discover active session
    const sessionId = params.callSessionId ?? orchestrator.activeSessionId;
    if (!sessionId) {
      return transformPayload(params, {
        success: false,
        message: 'No active live session. Start a call first.',
        callSessionId: '',
        responderCount: 0,
      });
    }

    // Resolve speaker identity
    const speakerId = params.speakerId ?? params.userId ?? '';
    let speakerName = params.speakerName ?? '';

    if (!speakerName && speakerId) {
      try {
        const result = await DataList.execute<UserEntity>({
          collection: 'users',
          filter: { id: speakerId },
          limit: 1,
          dbHandle: 'default',
        });
        if (result.success && result.items?.length) {
          speakerName = result.items[0].displayName || result.items[0].uniqueId;
        }
      } catch {
        // Fall through to default
      }
    }
    if (!speakerName) speakerName = 'CLI';

    // Route through the existing transcription command
    const transcriptionResult = await CollaborationLiveTranscription.execute({
      callSessionId: sessionId,
      speakerId,
      speakerName,
      transcript: params.message,
      confidence: params.confidence ?? 1.0,
      language: 'en',
      timestamp: Date.now(),
    });

    const responderCount = transcriptionResult.success
      ? parseInt(transcriptionResult.message.match(/(\d+) AI/)?.[1] ?? '0', 10)
      : 0;

    return transformPayload(params, {
      success: transcriptionResult.success,
      message: transcriptionResult.success
        ? `Sent to live session → ${responderCount} AI responders`
        : `Failed: ${transcriptionResult.message}`,
      callSessionId: sessionId,
      responderCount,
    });
  }
}
