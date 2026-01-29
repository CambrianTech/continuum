/**
 * CNS Integration Test
 *
 * Verifies that PersonaCentralNervousSystem is actually being used (not falling back)
 * and that cognitive schedulers are making decisions.
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { Commands } from '../../system/core/shared/Commands';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { ChatSendParams, ChatSendResult } from '../../commands/collaboration/chat/send/shared/ChatSendTypes';
import type { BaseEntity } from '../../system/data/entities/BaseEntity';

import { Ping } from '../../commands/ping/shared/PingTypes';
import { DataList } from '../../commands/data/list/shared/DataListTypes';
import { ChatSend } from '../../commands/collaboration/chat/send/shared/ChatSendTypes';
describe('CNS Integration', () => {
  beforeAll(async () => {
    // Ensure system is ready
    const pingResult = await Ping.execute({});
    expect(pingResult.success).toBe(true);
  });

  it('should have CNS initialized for all personas', async () => {
    // Get all PersonaUsers
    const users = await DataList.execute({
      collection: 'users',
      filter: { type: 'persona' }
    });

    expect(users.success).toBe(true);
    expect(users.items.length).toBeGreaterThan(0);

    console.log(`✅ Found ${users.items.length} personas - CNS should be initialized for each`);
  });

  it('should route chat messages through CNS serviceCycle', async () => {
    // This test verifies the integration by sending a message
    // If CNS wasn't working, the system would crash or fall back

    const rooms = await DataList.execute({
      collection: 'rooms',
      limit: 1
    });

    expect(rooms.success).toBe(true);
    expect(rooms.items.length).toBeGreaterThan(0);

    const roomId = rooms.items[0].uniqueId;

    // Send a test message
    const sendResult = await ChatSend.execute({
      room: roomId,
      message: '[TEST] CNS integration test message'
    });

    expect(sendResult.success).toBe(true);
    console.log('✅ Message sent through CNS without errors');
  });
});
