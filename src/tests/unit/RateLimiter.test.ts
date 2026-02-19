#!/usr/bin/env npx tsx
/**
 * RateLimiter Unit Tests
 *
 * Tests for the RateLimiter module — now focused on voice transcription dedup
 * and config holding. Rate limiting decisions are in Rust (full_evaluate gate).
 *
 * Verifies:
 * - Message/transcription deduplication
 * - Config holding and immutability
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { RateLimiter } from '../../system/user/server/modules/RateLimiter';

describe('RateLimiter', () => {
  let rateLimiter: RateLimiter;
  const testMessageId = 'test-message-456';

  beforeEach(() => {
    rateLimiter = new RateLimiter({
      minSecondsBetweenResponses: 2,
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

    it('should handle composite transcription keys', () => {
      const key1 = 'speaker-uuid-123-1707000000';
      const key2 = 'speaker-uuid-123-1707000001';

      rateLimiter.markMessageEvaluated(key1);

      expect(rateLimiter.hasEvaluatedMessage(key1)).toBe(true);
      expect(rateLimiter.hasEvaluatedMessage(key2)).toBe(false);
    });
  });
});
