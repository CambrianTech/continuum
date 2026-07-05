/**
 * Priority Calculation - Baseline Validation Test
 *
 * PURPOSE: Test the algorithm that calculates message priority.
 * This is a FAST test (no system initialization required).
 *
 * Validates that priority calculation heuristics work correctly:
 * - @mentions get high priority (+0.4)
 * - Recent messages get higher priority (+0.2 for <1min, +0.1 for <5min)
 * - Active rooms get priority boost (+0.1 if in recentRooms)
 * - Relevant expertise gets priority boost (+0.1 if content matches)
 */

import { describe, it, expect } from 'vitest';
import { calculateMessagePriority } from '../../modules/PersonaInbox';
import type { UUID } from '../../../../core/types/CrossPlatformUUID';

describe('Priority Calculation (Baseline Validation)', () => {
  const persona = {
    id: 'test-persona-id' as UUID,
    displayName: 'Test Persona',
    recentRooms: ['room-active' as UUID],
    expertise: ['typescript', 'testing']
  };
  const now = Date.now();

  const createTestMessage = (overrides: Partial<{content: string; timestamp: number; roomId: UUID}>) => ({
    content: 'Test message',
    timestamp: now,
    roomId: 'room-1' as UUID,
    ...overrides
  });

  it('should give high priority (+0.4) to @mentions with spaces', () => {
    const message = createTestMessage({
      content: '@test persona hello'
    });

    const priority = calculateMessagePriority(message, persona);
    // Base 0.2 + mention 0.4 + recency 0.2 (< 1min) = 0.8
    expect(priority).toBeGreaterThanOrEqual(0.6);
  });

  it('should give high priority (+0.4) to @mentions with hyphens', () => {
    const message = createTestMessage({
      content: '@test-persona urgent question'
    });

    const priority = calculateMessagePriority(message, persona);
    // Base 0.2 + mention 0.4 + recency 0.2 (< 1min) = 0.8
    expect(priority).toBeGreaterThanOrEqual(0.6);
  });

  it('should give higher priority to recent messages (<1 min: +0.2)', () => {
    const recentMessage = createTestMessage({
      timestamp: now,
      content: 'just now'
    });
    const priority = calculateMessagePriority(recentMessage, persona);

    // Base 0.2 + recency 0.2 = 0.4
    expect(priority).toBeGreaterThanOrEqual(0.4);
  });

  it('should give medium priority to messages <5 minutes old (+0.1)', () => {
    const message = createTestMessage({
      timestamp: now - 180000, // 3 minutes ago
      content: 'a few minutes ago'
    });
    const priority = calculateMessagePriority(message, persona);

    // Base 0.2 + recency 0.1 = 0.3
    expect(priority).toBeGreaterThanOrEqual(0.3);
    expect(priority).toBeLessThan(0.4);
  });

  it('should give base priority to old messages (no recency bonus)', () => {
    const oldMessage = createTestMessage({
      timestamp: now - 600000, // 10 minutes ago
      content: 'old message'
    });
    const priority = calculateMessagePriority(oldMessage, persona);

    // Base 0.2 only
    expect(priority).toBe(0.2);
  });

  it('should boost priority for messages in active rooms (+0.1)', () => {
    const activeRoomMessage = createTestMessage({
      roomId: 'room-active' as UUID,
      content: 'message in active room'
    });
    const priority = calculateMessagePriority(activeRoomMessage, persona);

    // Base 0.2 + recency 0.2 + active room 0.1 = 0.5
    expect(priority).toBeGreaterThanOrEqual(0.5);
  });

  it('should boost priority for messages matching expertise (+0.1)', () => {
    const relevantMessage = createTestMessage({
      content: 'Help with typescript testing issue'
    });
    const priority = calculateMessagePriority(relevantMessage, persona);

    // Base 0.2 + recency 0.2 + expertise 0.1 = 0.5
    expect(priority).toBeGreaterThanOrEqual(0.5);
  });

  it('should return priority in valid range [0.0, 1.0]', () => {
    const message = createTestMessage({
      content: 'basic message'
    });
    const priority = calculateMessagePriority(message, persona);

    expect(priority).toBeGreaterThanOrEqual(0.0);
    expect(priority).toBeLessThanOrEqual(1.0);
  });

  it('should cap priority at 1.0 even when factors exceed it', () => {
    const vipMessage = createTestMessage({
      roomId: 'room-active' as UUID,
      content: '@test-persona urgent typescript testing help',
      timestamp: now
    });

    const priority = calculateMessagePriority(vipMessage, persona);
    // Base 0.2 + mention 0.4 + recency 0.2 + active 0.1 + expertise 0.1 = 1.0
    expect(priority).toBe(1.0);
  });

  it('should combine all factors correctly', () => {
    const maxMessage = createTestMessage({
      roomId: 'room-active' as UUID,
      content: '@test-persona need help with typescript',
      timestamp: now
    });

    const priority = calculateMessagePriority(maxMessage, persona);
    // Base 0.2 + mention 0.4 + recency 0.2 + active 0.1 + expertise 0.1 = 1.0
    expect(priority).toBe(1.0);
  });
});
