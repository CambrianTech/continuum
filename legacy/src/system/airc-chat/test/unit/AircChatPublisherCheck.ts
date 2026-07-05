#!/usr/bin/env tsx

import { strict as assert } from 'node:assert';
import type { AircRealtimeEnvelope } from '@shared/generated/airc/AircRealtimeEnvelope';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import {
  AircCliChatPublisher,
  buildPublishArgs,
  parsePublishReceipt,
  type AircCommandRunner,
} from '../../server/AircChatPublisher';

function makeEnvelope(): AircRealtimeEnvelope {
  return {
    eventId: '11111111-1111-4111-8111-111111111111' as UUID,
    roomId: '22222222-2222-4222-8222-222222222222' as UUID,
    sourceId: '33333333-3333-4333-8333-333333333333' as UUID,
    createdAtMs: 1779645600000n,
    delivery: 'durable',
    traceId: '44444444-4444-4444-8444-444444444444' as UUID,
    payload: {
      kind: 'existing_schema',
      payload: {
        schema: 'chat_transcript',
        schemaVersion: 'continuum.chat.v1',
        inline: { text: 'hello' },
      },
    },
  };
}

async function run(): Promise<void> {
  const envelope = makeEnvelope();
  const args = buildPublishArgs({ roomName: 'general', envelope });
  assert.deepEqual(args.slice(0, 7), [
    'publish',
    '--room',
    'general',
    '--kind',
    'message',
    '--body-json',
    '-',
  ]);
  assert.ok(args.includes('forge.body_hint=continuum.chat_transcript'));
  assert.ok(args.includes('continuum.schema=chat_transcript'));
  assert.ok(args.includes('continuum.trace_id=44444444-4444-4444-8444-444444444444'));
  assert.ok(args.includes('continuum.room_id=22222222-2222-4222-8222-222222222222'));

  const parsed = parsePublishReceipt(JSON.stringify({
    event_id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',
    lamport: 42,
    occurred_at_ms: 1779645600001,
    channel_id: 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb',
    channel_name: 'general',
  }));
  assert.equal(parsed.ok, true);
  if (parsed.ok) {
    assert.equal(parsed.value.event_id, 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa');
  }
  assert.equal(parsePublishReceipt('not json').ok, false);
  assert.equal(parsePublishReceipt('{}').ok, false);

  let capturedArgs: string[] = [];
  let capturedStdin = '';
  const runner: AircCommandRunner = async (argv, options) => {
    capturedArgs = argv;
    capturedStdin = options.stdin ?? '';
    return {
      exitCode: 0,
      stdout: JSON.stringify({
        event_id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',
        lamport: 42,
        occurred_at_ms: 1779645600001,
        channel_id: 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb',
        channel_name: 'general',
      }),
      stderr: '',
      timedOut: false,
    };
  };
  const publisher = new AircCliChatPublisher({
    repoRoot: process.cwd(),
    runner,
  });
  const result = await publisher.publish({ roomName: 'general', envelope });
  assert.equal(result.ok, true);
  assert.equal(capturedArgs[0], 'publish');
  assert.ok(capturedStdin.includes('"traceId":"44444444-4444-4444-8444-444444444444"'));
  if (result.ok) {
    assert.equal(result.eventId, 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa');
    assert.equal(result.roomId, 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb');
    assert.equal(result.lamport, 42);
  }

  console.log('AircChatPublisher checks passed');
}

void run();
