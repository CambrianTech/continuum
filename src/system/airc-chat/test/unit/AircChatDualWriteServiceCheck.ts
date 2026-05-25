#!/usr/bin/env tsx

import { strict as assert } from 'node:assert';
import { ChatMessageEntity } from '@system/data/entities/ChatMessageEntity';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { AircChatDualWriteService } from '../../server/AircChatDualWriteService';
import type {
  AircChatPublishRequest,
  AircChatPublishResult,
  AircChatPublisher,
} from '../../server/AircChatPublisher';

class RecordingPublisher implements AircChatPublisher {
  requests: AircChatPublishRequest[] = [];

  async publish(request: AircChatPublishRequest): Promise<AircChatPublishResult> {
    this.requests.push(request);
    return {
      ok: true,
      eventId: request.envelope.eventId,
      roomId: request.envelope.roomId,
      publisher: 'airc-cli',
    };
  }
}

function makeMessage(): ChatMessageEntity {
  const message = new ChatMessageEntity();
  message.id = '55555555-5555-4555-8555-555555555555' as UUID;
  message.roomId = '66666666-6666-4666-8666-666666666666' as UUID;
  message.senderId = '77777777-7777-4777-8777-777777777777' as UUID;
  message.senderName = 'Helper AI';
  message.senderType = 'persona';
  message.timestamp = new Date('2026-05-24T18:00:00.000Z');
  message.content = { text: 'I can see the bus', media: [] };
  message.metadata = { source: 'bot' };
  return message;
}

async function run(): Promise<void> {
  const publisher = new RecordingPublisher();
  const service = new AircChatDualWriteService(publisher);

  const result = await service.publishStoredChatMessage({
    roomName: 'cambriantech',
    storedMessage: makeMessage(),
  });

  assert.equal(result.ok, true);
  assert.equal(publisher.requests.length, 1);
  assert.equal(publisher.requests[0].roomName, 'cambriantech');
  assert.equal(publisher.requests[0].envelope.roomId, '66666666-6666-4666-8666-666666666666');
  assert.equal(publisher.requests[0].envelope.payload.kind, 'existing_schema');
  assert.equal(publisher.requests[0].envelope.payload.payload.schema, 'chat_transcript');

  console.log('AircChatDualWriteService checks passed');
}

void run();
