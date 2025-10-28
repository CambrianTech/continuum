#!/usr/bin/env npx tsx
/**
 * RateLimiter Unit Tests
 *
 * Tests for the extracted RateLimiter module (Phase 1, Commit 1.2)
 *
 * Verifies:
 * - Time-based rate limiting per room
 * - Response count caps per room
 * - Message deduplication
 * - Rate limit info retrieval
 * - Room-specific tracking
 */

import { RateLimiter } from '../../system/user/server/modules/RateLimiter';

describe('RateLimiter', () => {
  let rateLimiter: RateLimiter;
  const testRoomId = 'test-room-123';
  const testMessageId = 'test-message-456';

  beforeEach(() => {
    rateLimiter = new RateLimiter({
      minSecondsBetweenResponses: 2, // 2 seconds for faster tests
      maxResponsesPerSession: 5
    });
  });

  describe('Configuration', () => {
    it('should use default config when none provided', () => {
      const limiter = new RateLimiter();
      const config = limiter.getConfig();

      expect(config.minSecondsBetweenResponses).toBe(10);
      expect(config.maxResponsesPerSession).toBe(50);
    });

    it('should use custom config when provided', () => {
      const limiter = new RateLimiter({
        minSecondsBetweenResponses: 15,
        maxResponsesPerSession: 100
      });
      const config = limiter.getConfig();

      expect(config.minSecondsBetweenResponses).toBe(15);
      expect(config.maxResponsesPerSession).toBe(100);
    });

    it('should return immutable config copy', () => {
      const config = rateLimiter.getConfig();
      const originalMin = config.minSecondsBetweenResponses;

      // Attempt to modify (should not affect internal state)
      (config as any).minSecondsBetweenResponses = 999;

      const newConfig = rateLimiter.getConfig();
      expect(newConfig.minSecondsBetweenResponses).toBe(originalMin);
    });
  });

  describe('Time-Based Rate Limiting', () => {
    it('should not be rate limited when never responded', () => {
      expect(rateLimiter.isRateLimited(testRoomId)).toBe(false);
    });

    it('should be rate limited immediately after response', () => {
      rateLimiter.trackResponse(testRoomId);
      expect(rateLimiter.isRateLimited(testRoomId)).toBe(true);
    });

    it('should remain rate limited within time window', async () => {
      rateLimiter.trackResponse(testRoomId);

      // Wait 1 second (less than 2 second limit)
      await new Promise(resolve => setTimeout(resolve, 1000));

      expect(rateLimiter.isRateLimited(testRoomId)).toBe(true);
    });

    it('should not be rate limited after time window expires', async () => {
      rateLimiter.trackResponse(testRoomId);

      // Wait 2.1 seconds (exceeds 2 second limit)
      await new Promise(resolve => setTimeout(resolve, 2100));

      expect(rateLimiter.isRateLimited(testRoomId)).toBe(false);
    });

    it('should be room-specific (different rooms independent)', () => {
      const room1 = 'room-1';
      const room2 = 'room-2';

      rateLimiter.trackResponse(room1);

      expect(rateLimiter.isRateLimited(room1)).toBe(true);
      expect(rateLimiter.isRateLimited(room2)).toBe(false);
    });
  });

  describe('Rate Limit Info', () => {
    it('should return null info when never responded', () => {
      const info = rateLimiter.getRateLimitInfo(testRoomId);

      expect(info.isLimited).toBe(false);
      expect(info.lastResponseTime).toBeNull();
      expect(info.responseCount).toBe(0);
      expect(info.secondsSinceLastResponse).toBeNull();
      expect(info.waitTimeSeconds).toBeNull();
    });

    it('should return accurate info immediately after response', () => {
      rateLimiter.trackResponse(testRoomId);
      const info = rateLimiter.getRateLimitInfo(testRoomId);

      expect(info.isLimited).toBe(true);
      expect(info.lastResponseTime).toBeInstanceOf(Date);
      expect(info.responseCount).toBe(1);
      expect(info.secondsSinceLastResponse).toBeLessThan(1);
      expect(info.waitTimeSeconds).toBeGreaterThan(1);
      expect(info.waitTimeSeconds).toBeLessThanOrEqual(2);
    });

    it('should update wait time as time passes', async () => {
      rateLimiter.trackResponse(testRoomId);
      const info1 = rateLimiter.getRateLimitInfo(testRoomId);

      // Wait 1 second
      await new Promise(resolve => setTimeout(resolve, 1000));

      const info2 = rateLimiter.getRateLimitInfo(testRoomId);

      expect(info2.secondsSinceLastResponse).toBeGreaterThan(info1.secondsSinceLastResponse!);
      expect(info2.waitTimeSeconds).toBeLessThan(info1.waitTimeSeconds!);
    });

    it('should show not limited after time window expires', async () => {
      rateLimiter.trackResponse(testRoomId);

      // Wait for window to expire
      await new Promise(resolve => setTimeout(resolve, 2100));

      const info = rateLimiter.getRateLimitInfo(testRoomId);

      expect(info.isLimited).toBe(false);
      expect(info.waitTimeSeconds).toBeNull();
      expect(info.secondsSinceLastResponse).toBeGreaterThan(2);
    });
  });

  describe('Response Count Tracking', () => {
    it('should start at zero responses', () => {
      expect(rateLimiter.getResponseCount(testRoomId)).toBe(0);
    });

    it('should increment response count on each track', () => {
      rateLimiter.trackResponse(testRoomId);
      expect(rateLimiter.getResponseCount(testRoomId)).toBe(1);

      rateLimiter.trackResponse(testRoomId);
      expect(rateLimiter.getResponseCount(testRoomId)).toBe(2);

      rateLimiter.trackResponse(testRoomId);
      expect(rateLimiter.getResponseCount(testRoomId)).toBe(3);
    });

    it('should be room-specific (different rooms independent)', () => {
      const room1 = 'room-1';
      const room2 = 'room-2';

      rateLimiter.trackResponse(room1);
      rateLimiter.trackResponse(room1);
      rateLimiter.trackResponse(room2);

      expect(rateLimiter.getResponseCount(room1)).toBe(2);
      expect(rateLimiter.getResponseCount(room2)).toBe(1);
    });

    it('should detect when response cap reached', () => {
      // Track 5 responses (cap is 5)
      for (let i = 0; i < 5; i++) {
        rateLimiter.trackResponse(testRoomId);
      }

      expect(rateLimiter.hasReachedResponseCap(testRoomId)).toBe(true);
    });

    it('should not reach cap before limit', () => {
      // Track 4 responses (cap is 5)
      for (let i = 0; i < 4; i++) {
        rateLimiter.trackResponse(testRoomId);
      }

      expect(rateLimiter.hasReachedResponseCap(testRoomId)).toBe(false);
    });

    it('should detect cap exceeded', () => {
      // Track 6 responses (cap is 5)
      for (let i = 0; i < 6; i++) {
        rateLimiter.trackResponse(testRoomId);
      }

      expect(rateLimiter.hasReachedResponseCap(testRoomId)).toBe(true);
      expect(rateLimiter.getResponseCount(testRoomId)).toBe(6);
    });
  });

  describe('Message Deduplication', () => {
    it('should not have evaluated new message', () => {
      expect(rateLimiter.hasEvaluatedMessage(testMessageId)).toBe(false);
    });

    it('should mark message as evaluated', () => {
      rateLimiter.markMessageEvaluated(testMessageId);
      expect(rateLimiter.hasEvaluatedMessage(testMessageId)).toBe(true);
    });

    it('should track multiple messages independently', () => {
      const msg1 = 'message-1';
      const msg2 = 'message-2';
      const msg3 = 'message-3';

      rateLimiter.markMessageEvaluated(msg1);
      rateLimiter.markMessageEvaluated(msg3);

      expect(rateLimiter.hasEvaluatedMessage(msg1)).toBe(true);
      expect(rateLimiter.hasEvaluatedMessage(msg2)).toBe(false);
      expect(rateLimiter.hasEvaluatedMessage(msg3)).toBe(true);
    });

    it('should handle marking same message multiple times', () => {
      rateLimiter.markMessageEvaluated(testMessageId);
      rateLimiter.markMessageEvaluated(testMessageId);
      rateLimiter.markMessageEvaluated(testMessageId);

      expect(rateLimiter.hasEvaluatedMessage(testMessageId)).toBe(true);
    });

    it('should clear evaluated messages', () => {
      rateLimiter.markMessageEvaluated('msg-1');
      rateLimiter.markMessageEvaluated('msg-2');
      rateLimiter.markMessageEvaluated('msg-3');

      rateLimiter.clearEvaluatedMessages();

      expect(rateLimiter.hasEvaluatedMessage('msg-1')).toBe(false);
      expect(rateLimiter.hasEvaluatedMessage('msg-2')).toBe(false);
      expect(rateLimiter.hasEvaluatedMessage('msg-3')).toBe(false);
    });
  });

  describe('Room Reset', () => {
    it('should reset rate limit state for room', async () => {
      rateLimiter.trackResponse(testRoomId);
      expect(rateLimiter.isRateLimited(testRoomId)).toBe(true);
      expect(rateLimiter.getResponseCount(testRoomId)).toBe(1);

      rateLimiter.resetRoom(testRoomId);

      expect(rateLimiter.isRateLimited(testRoomId)).toBe(false);
      expect(rateLimiter.getResponseCount(testRoomId)).toBe(0);

      const info = rateLimiter.getRateLimitInfo(testRoomId);
      expect(info.lastResponseTime).toBeNull();
    });

    it('should not affect other rooms when resetting', () => {
      const room1 = 'room-1';
      const room2 = 'room-2';

      rateLimiter.trackResponse(room1);
      rateLimiter.trackResponse(room2);

      rateLimiter.resetRoom(room1);

      expect(rateLimiter.isRateLimited(room1)).toBe(false);
      expect(rateLimiter.isRateLimited(room2)).toBe(true);
      expect(rateLimiter.getResponseCount(room1)).toBe(0);
      expect(rateLimiter.getResponseCount(room2)).toBe(1);
    });
  });

  describe('Integration Scenarios', () => {
    it('should handle rapid responses with time-based limiting', async () => {
      // First response - allowed
      rateLimiter.trackResponse(testRoomId);
      expect(rateLimiter.isRateLimited(testRoomId)).toBe(true);

      // Wait for window to expire
      await new Promise(resolve => setTimeout(resolve, 2100));

      // Second response - allowed
      rateLimiter.trackResponse(testRoomId);
      expect(rateLimiter.isRateLimited(testRoomId)).toBe(true);
      expect(rateLimiter.getResponseCount(testRoomId)).toBe(2);
    });

    it('should enforce both time and count limits', async () => {
      // Send 5 responses (reach cap)
      for (let i = 0; i < 5; i++) {
        rateLimiter.trackResponse(testRoomId);
        await new Promise(resolve => setTimeout(resolve, 2100)); // Wait for time window
      }

      expect(rateLimiter.hasReachedResponseCap(testRoomId)).toBe(true);
      expect(rateLimiter.isRateLimited(testRoomId)).toBe(true);

      // Even after time window, cap is reached
      await new Promise(resolve => setTimeout(resolve, 2100));
      expect(rateLimiter.hasReachedResponseCap(testRoomId)).toBe(true);
    });

    it('should handle multiple rooms with different states', () => {
      const room1 = 'room-1';
      const room2 = 'room-2';
      const room3 = 'room-3';

      // Room 1: Just responded
      rateLimiter.trackResponse(room1);

      // Room 2: Responded 3 times
      rateLimiter.trackResponse(room2);
      rateLimiter.trackResponse(room2);
      rateLimiter.trackResponse(room2);

      // Room 3: Never responded

      expect(rateLimiter.isRateLimited(room1)).toBe(true);
      expect(rateLimiter.isRateLimited(room2)).toBe(true);
      expect(rateLimiter.isRateLimited(room3)).toBe(false);

      expect(rateLimiter.getResponseCount(room1)).toBe(1);
      expect(rateLimiter.getResponseCount(room2)).toBe(3);
      expect(rateLimiter.getResponseCount(room3)).toBe(0);
    });
  });
});
