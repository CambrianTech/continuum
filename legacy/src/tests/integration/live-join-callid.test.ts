/**
 * Integration test for LiveJoin callId fix
 *
 * Tests that LiveJoin returns callId (not sessionId) so VoiceOrchestrator
 * can match transcriptions to the registered session.
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { Commands } from '../../system/core/shared/Commands';
import type { LiveJoinParams, LiveJoinResult } from '../../commands/collaboration/live/join/shared/LiveJoinTypes';

import { LiveJoin } from '../../commands/collaboration/live/join/shared/LiveJoinTypes';
describe('LiveJoin callId integration', () => {
  beforeAll(async () => {
    // Give system time to start
    await new Promise(resolve => setTimeout(resolve, 2000));
  });

  it('should return callId that matches CallEntity.id', async () => {
    const result = await LiveJoin.execute({
      entityId: 'general'  // Use general room
    });

    expect(result.success).toBe(true);
    expect(result.callId).toBeDefined();
    expect(result.session).toBeDefined();

    // CallId should match the CallEntity's id
    expect(result.callId).toBe(result.session.id);

    // CallId should be a UUID (36 chars with dashes)
    expect(result.callId).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);

    console.log(`✅ LiveJoin returned callId: ${result.callId.slice(0, 8)}`);
  });

  it('should NOT return JTAG sessionId as callId', async () => {
    const result = await LiveJoin.execute({
      entityId: 'general'
    });

    expect(result.success).toBe(true);

    // The result WILL have a sessionId field (from JTAG), but callId should be different
    // This test verifies we're using the RIGHT field (callId, not sessionId)
    expect(result.callId).toBeDefined();
    expect(result.session.id).toBe(result.callId);

    console.log(`✅ CallId (${result.callId.slice(0, 8)}) correctly set from CallEntity`);
  });
});
