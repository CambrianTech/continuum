#!/usr/bin/env tsx

import { strict as assert } from 'node:assert';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { ChatMessageEntity } from '@system/data/entities/ChatMessageEntity';
import { buildAircChatEnvelope } from '../../shared/AircChatEnvelope';
import { AircToORMMirrorWriter } from '../../server/AircToORMMirrorWriter';
import type {
  AircChatEventSource,
  AircChatMirrorCursor,
  AircChatMirrorEvent,
  AircChatMirrorInsertResult,
  AircChatMirrorStore,
} from '../../server/AircChatMirrorTypes';

const ROOM_ID = '22222222-2222-4222-8222-222222222222' as UUID;

class FixtureSource implements AircChatEventSource {
  constructor(private readonly events: readonly AircChatMirrorEvent[]) {}

  async fetchAfter(
    roomId: UUID,
    cursor: AircChatMirrorCursor | undefined,
    limit: number,
  ): Promise<readonly AircChatMirrorEvent[]> {
    const start = cursor
      ? this.events.findIndex((event) => event.eventId === cursor.eventId) + 1
      : 0;
    return this.events
      .filter((event) => event.envelope.roomId === roomId)
      .slice(Math.max(start, 0), Math.max(start, 0) + limit);
  }
}

class FixtureStore implements AircChatMirrorStore {
  readonly messages = new Map<UUID, ChatMessageEntity>();
  cursor: AircChatMirrorCursor | undefined;

  async loadCursor(): Promise<AircChatMirrorCursor | undefined> {
    return this.cursor;
  }

  async saveCursor(cursor: AircChatMirrorCursor): Promise<void> {
    this.cursor = cursor;
  }

  async hasMessage(messageId: UUID): Promise<boolean> {
    return this.messages.has(messageId);
  }

  async insertMessage(message: ChatMessageEntity): Promise<AircChatMirrorInsertResult> {
    if (this.messages.has(message.id)) return 'duplicate';
    this.messages.set(message.id, message);
    return 'inserted';
  }
}

function makeEvent(index: number, text: string): AircChatMirrorEvent {
  const legacyOrmId = `11111111-1111-4111-8111-${String(index).padStart(12, '1')}` as UUID;
  const storedMessage = new ChatMessageEntity();
  storedMessage.id = legacyOrmId;
  storedMessage.roomId = ROOM_ID;
  storedMessage.senderId = '33333333-3333-4333-8333-333333333333' as UUID;
  storedMessage.senderName = 'Joel';
  storedMessage.senderType = 'human';
  storedMessage.timestamp = new Date(1779645600000 + index);
  storedMessage.content = { text, media: [] };
  storedMessage.metadata = { source: 'user' };

  const envelope = buildAircChatEnvelope({
    roomName: 'general',
    storedMessage,
  });
  const eventId = `aaaaaaaa-aaaa-4aaa-8aaa-${String(index).padStart(12, 'a')}` as UUID;

  return {
    eventId,
    lamport: 100 + index,
    occurredAtMs: 1779645601000 + index,
    envelope,
  };
}

async function mirrorsChatTranscriptEventsIntoCanonicalAircIds(): Promise<void> {
  const store = new FixtureStore();
  const events = [makeEvent(1, 'hello'), makeEvent(2, 'second')];
  const writer = new AircToORMMirrorWriter({
    source: new FixtureSource(events),
    store,
  });

  const result = await writer.runOnce(ROOM_ID);

  assert.equal(result.scanned, 2);
  assert.equal(result.inserted, 2);
  assert.equal(result.duplicates, 0);
  assert.equal(result.skipped, 0);
  assert.equal(store.messages.size, 2);
  assert.equal(store.cursor?.eventId, events[1].eventId);

  const mirrored = store.messages.get(events[0].eventId);
  assert.ok(mirrored);
  assert.equal(mirrored.id, events[0].eventId);
  assert.equal(mirrored.content.text, 'hello');
  assert.equal(mirrored.metadata?.source, 'user');
  assert.equal((mirrored.metadata as Record<string, unknown>).aircEventId, events[0].eventId);
  assert.equal((mirrored.metadata as Record<string, unknown>).legacyOrmId, events[0].envelope.traceId);
}

async function resumesFromCursorAndDoesNotDuplicateRows(): Promise<void> {
  const events = [makeEvent(1, 'hello'), makeEvent(2, 'second')];
  const store = new FixtureStore();
  const writer = new AircToORMMirrorWriter({
    source: new FixtureSource(events),
    store,
    batchLimit: 1,
  });

  const first = await writer.runOnce(ROOM_ID);
  const second = await writer.runOnce(ROOM_ID);
  const replay = await writer.runOnce(ROOM_ID);

  assert.equal(first.inserted, 1);
  assert.equal(second.inserted, 1);
  assert.equal(replay.scanned, 0);
  assert.equal(store.messages.size, 2);
  assert.equal(store.cursor?.eventId, events[1].eventId);
}

async function skipsNonChatEventsButStillAdvancesCursor(): Promise<void> {
  const chat = makeEvent(1, 'hello');
  const nonChat: AircChatMirrorEvent = {
    ...makeEvent(2, 'presence'),
    envelope: {
      ...makeEvent(2, 'presence').envelope,
      payload: {
        kind: 'presence',
        event: {
          roomId: ROOM_ID,
          subjectId: '33333333-3333-4333-8333-333333333333',
          state: 'typing',
          startedAtMs: 1779645602000n,
        },
      },
    },
  };
  const store = new FixtureStore();
  const writer = new AircToORMMirrorWriter({
    source: new FixtureSource([chat, nonChat]),
    store,
  });

  const result = await writer.runOnce(ROOM_ID);

  assert.equal(result.inserted, 1);
  assert.equal(result.skipped, 1);
  assert.equal(store.messages.size, 1);
  assert.equal(store.cursor?.eventId, nonChat.eventId);
}

async function run(): Promise<void> {
  await mirrorsChatTranscriptEventsIntoCanonicalAircIds();
  await resumesFromCursorAndDoesNotDuplicateRows();
  await skipsNonChatEventsButStillAdvancesCursor();
  console.log('AircToORMMirrorWriter checks passed');
}

void run();
