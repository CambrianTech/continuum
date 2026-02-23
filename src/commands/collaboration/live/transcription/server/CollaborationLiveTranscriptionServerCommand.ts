/**
 * Collaboration Live Transcription Command - Server Implementation
 *
 * Relay voice transcription from browser to server for AI processing.
 * Also persists human transcriptions to chat_messages for export/history.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { Events } from '@system/core/shared/Events';
import { getRustVoiceOrchestrator } from '@system/voice/server';
import { ChatMessageEntity } from '@system/data/entities/ChatMessageEntity';
import { CallEntity } from '@system/data/entities/CallEntity';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { CollaborationLiveTranscriptionParams, CollaborationLiveTranscriptionResult } from '../shared/CollaborationLiveTranscriptionTypes';
import { createCollaborationLiveTranscriptionResultFromParams } from '../shared/CollaborationLiveTranscriptionTypes';
import { DataCreate } from '../../../../data/create/shared/DataCreateTypes';
import { DataList } from '../../../../data/list/shared/DataListTypes';

export class CollaborationLiveTranscriptionServerCommand extends CommandBase<CollaborationLiveTranscriptionParams, CollaborationLiveTranscriptionResult> {

  /** Cache callSessionId → roomId to avoid repeated DB lookups */
  private static sessionRoomCache: Map<string, UUID> = new Map();

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

    // Persist transcription to chat_messages (async, don't block response)
    this.persistTranscription(params).catch(err => {
      console.error('Failed to persist voice transcription:', err);
    });

    return createCollaborationLiveTranscriptionResultFromParams(params, {
      success: true,
      message: `Transcription → ${responderIds.length} AI responders`
    });
  }

  /**
   * Persist a human voice transcription as a ChatMessageEntity.
   * This makes live transcriptions visible to chat/export.
   */
  private async persistTranscription(params: CollaborationLiveTranscriptionParams): Promise<void> {
    const roomId = await this.resolveRoomId(params.callSessionId, params);
    if (!roomId) return; // Can't persist without a room

    const message = new ChatMessageEntity();
    message.roomId = roomId;
    message.senderId = params.speakerId as UUID;
    message.senderName = params.speakerName;
    message.senderType = 'human';
    message.content = { text: params.transcript };
    message.status = 'sent';
    message.priority = 'normal';
    message.timestamp = new Date(params.timestamp);
    message.reactions = [];
    message.metadata = {
      source: 'user',
      sourceModality: 'voice',
      voiceSessionId: params.callSessionId as UUID,
      speakerId: params.speakerId as UUID,
      speakerName: params.speakerName,
      confidence: params.confidence,
    };

    await DataCreate.execute<ChatMessageEntity>({
      collection: ChatMessageEntity.collection,
      data: message,
      context: params.context,
      sessionId: params.sessionId,
    });
  }

  /**
   * Resolve callSessionId → roomId, with caching.
   */
  private async resolveRoomId(callSessionId: string, params: CollaborationLiveTranscriptionParams): Promise<UUID | null> {
    const cached = CollaborationLiveTranscriptionServerCommand.sessionRoomCache.get(callSessionId);
    if (cached) return cached;

    const result = await DataList.execute<CallEntity>({
      collection: CallEntity.collection,
      filter: { id: callSessionId },
      limit: 1,
      context: params.context,
      sessionId: params.sessionId,
    });

    if (result.success && result.items && result.items.length > 0) {
      const roomId = result.items[0].roomId as UUID;
      CollaborationLiveTranscriptionServerCommand.sessionRoomCache.set(callSessionId, roomId);
      return roomId;
    }

    console.warn(`Could not resolve roomId for call session ${callSessionId}`);
    return null;
  }
}
