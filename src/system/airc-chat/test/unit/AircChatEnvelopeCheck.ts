#!/usr/bin/env tsx

import { strict as assert } from 'node:assert';
import { ChatMessageEntity } from '@system/data/entities/ChatMessageEntity';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import {
  AIRC_CHAT_SCHEMA_VERSION,
  buildAircChatEnvelope,
  serializeAircRealtimeEnvelope,
  type AircChatTranscriptInline,
} from '../../shared/AircChatEnvelope';

function makeMessage(): ChatMessageEntity {
  const message = new ChatMessageEntity();
  message.id = '11111111-1111-4111-8111-111111111111' as UUID;
  message.roomId = '22222222-2222-4222-8222-222222222222' as UUID;
  message.senderId = '33333333-3333-4333-8333-333333333333' as UUID;
  message.senderName = 'Joel';
  message.senderType = 'human';
  message.timestamp = new Date('2026-05-24T17:45:00.000Z');
  message.replyToId = '44444444-4444-4444-8444-444444444444' as UUID;
  message.content = {
    text: 'hello over AIRC',
    media: [
      {
        type: 'image',
        base64: 'must-not-cross-airc',
        blobHash: 'sha256:abc',
        url: '/media/abc.png',
        mimeType: 'image/png',
        filename: 'abc.png',
        size: 1234,
        width: 640,
        height: 480,
      },
    ],
  };
  message.metadata = {
    source: 'user',
    isSystemTest: false,
    deliveryReceipts: [{ userId: 'hidden', deliveredAt: new Date() }],
  };
  return message;
}

function inlineFrom(envelope: ReturnType<typeof buildAircChatEnvelope>): AircChatTranscriptInline {
  assert.equal(envelope.payload.kind, 'existing_schema');
  const inline = envelope.payload.payload.inline;
  assert.equal(typeof inline, 'object');
  assert.notEqual(inline, null);
  return inline as AircChatTranscriptInline;
}

function run(): void {
  const envelope = buildAircChatEnvelope({
    roomName: 'general',
    storedMessage: makeMessage(),
  });
  const inline = inlineFrom(envelope);

  assert.equal(envelope.delivery, 'durable');
  assert.equal(envelope.roomId, '22222222-2222-4222-8222-222222222222');
  assert.equal(envelope.sourceId, '33333333-3333-4333-8333-333333333333');
  assert.equal(envelope.traceId, '11111111-1111-4111-8111-111111111111');
  assert.equal(envelope.payload.payload.schema, 'chat_transcript');
  assert.equal(envelope.payload.payload.schemaVersion, AIRC_CHAT_SCHEMA_VERSION);

  assert.equal(inline.kind, 'continuum.chat.message');
  assert.equal(inline.messageId, '11111111-1111-4111-8111-111111111111');
  assert.equal(inline.roomName, 'general');
  assert.equal(inline.text, 'hello over AIRC');
  assert.equal(inline.media.length, 1);
  assert.equal(inline.media[0].blobHash, 'sha256:abc');
  assert.equal('base64' in inline.media[0], false);
  assert.equal(inline.metadata?.source, 'user');
  assert.equal('deliveryReceipts' in (inline.metadata ?? {}), false);

  const serialized = serializeAircRealtimeEnvelope(envelope);
  const parsed = JSON.parse(serialized) as { createdAtMs: string };
  assert.equal(parsed.createdAtMs, '1779644700000');
  assert.equal(serialized.includes('must-not-cross-airc'), false);

  console.log('AircChatEnvelope checks passed');
}

run();
