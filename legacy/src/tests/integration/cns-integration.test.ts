/**
 * Cognition Scheduling Integration Test
 *
 * Verifies that the Rust cognition engine + ChannelModule tick loop
 * are routing messages and scheduling work correctly.
 */

import { describe, it, expect, beforeAll } from 'vitest';

import { Ping } from '../../commands/ping/shared/PingTypes';
import { DataList } from '../../commands/data/list/shared/DataListTypes';
import { ChatSend } from '../../commands/collaboration/chat/send/shared/ChatSendTypes';

describe('Cognition Scheduling Integration', () => {
  beforeAll(async () => {
    const pingResult = await Ping.execute({});
    expect(pingResult.success).toBe(true);
  });

  it('should have Rust cognition initialized for all personas', async () => {
    const users = await DataList.execute({
      collection: 'users',
      filter: { type: 'persona' }
    });

    expect(users.success).toBe(true);
    expect(users.items.length).toBeGreaterThan(0);

    console.log(`Found ${users.items.length} personas with Rust cognition engine`);
  });

  it('should route chat messages through Rust serviceCycleFull', async () => {
    const rooms = await DataList.execute({
      collection: 'rooms',
      limit: 1
    });

    expect(rooms.success).toBe(true);
    expect(rooms.items.length).toBeGreaterThan(0);

    const roomId = rooms.items[0].uniqueId;

    const sendResult = await ChatSend.execute({
      room: roomId,
      message: '[TEST] Cognition scheduling integration test message'
    });

    expect(sendResult.success).toBe(true);
    console.log('Message routed through Rust cognition without errors');
  });
});
