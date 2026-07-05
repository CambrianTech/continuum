import type { AircRealtimeEnvelope } from '@shared/generated/airc/AircRealtimeEnvelope';
import type { ChatMessageEntity } from '@system/data/entities/ChatMessageEntity';
import { buildAircChatEnvelope } from '../shared/AircChatEnvelope';
import {
  AircCliChatPublisher,
  type AircChatPublishResult,
  type AircChatPublisher,
} from './AircChatPublisher';

export interface PublishStoredChatMessageInput {
  roomName: string;
  storedMessage: ChatMessageEntity;
}

export interface AircChatDualWriteResult {
  ok: boolean;
  envelope: AircRealtimeEnvelope;
  publish: AircChatPublishResult;
}

export class AircChatDualWriteService {
  constructor(private readonly publisher: AircChatPublisher = new AircCliChatPublisher()) {}

  async publishStoredChatMessage(input: PublishStoredChatMessageInput): Promise<AircChatDualWriteResult> {
    const envelope = buildAircChatEnvelope(input);
    const publish = await this.publisher.publish({
      roomName: input.roomName,
      envelope,
    });

    if (!publish.ok) {
      recordDualWriteFailure({
        messageId: input.storedMessage.id,
        roomId: input.storedMessage.roomId,
        eventId: envelope.eventId,
        error: publish.error,
      });
    }

    return {
      ok: publish.ok,
      envelope,
      publish,
    };
  }
}

interface DualWriteFailureDiagnostic {
  messageId: string;
  roomId: string;
  eventId: string;
  error: string;
}

function recordDualWriteFailure(diagnostic: DualWriteFailureDiagnostic): void {
  void import('@system/core/logging/Logger')
    .then(({ Logger }) => {
      Logger
        .create('AircChatDualWriteService', 'airc-chat')
        .error('chat dual-write to AIRC failed', diagnostic);
    })
    .catch(() => {
      // The command result already surfaces this failure. Logging is diagnostic only.
    });
}
