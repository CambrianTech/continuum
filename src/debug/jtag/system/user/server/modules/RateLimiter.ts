/**
 * RateLimiter Module
 *
 * Extracted from PersonaUser.ts (Phase 1, Commit 1.2)
 *
 * Manages per-room rate limiting for AI responses:
 * - Time-based limiting (min seconds between responses per room)
 * - Response count caps (max responses per room per session)
 * - Message deduplication (prevent evaluating same message multiple times)
 *
 * This module is stateful and maintains in-memory tracking.
 * Future: Move to SQLite for persistence across restarts.
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';

export interface RateLimitConfig {
  minSecondsBetweenResponses: number;
  maxResponsesPerSession: number;
}

export interface RateLimitInfo {
  isLimited: boolean;
  lastResponseTime: Date | null;
  responseCount: number;
  secondsSinceLastResponse: number | null;
  waitTimeSeconds: number | null;
}

export class RateLimiter {
  // Rate limiting state (in-memory for now, will move to SQLite later)
  private lastResponseTime: Map<UUID, Date> = new Map();
  private responseCount: Map<UUID, number> = new Map(); // room -> count
  private evaluatedMessages: Set<string> = new Set(); // messageId -> already evaluated

  private readonly config: RateLimitConfig;

  constructor(config?: Partial<RateLimitConfig>) {
    this.config = {
      minSecondsBetweenResponses: config?.minSecondsBetweenResponses ?? 10,
      maxResponsesPerSession: config?.maxResponsesPerSession ?? 50
    };
  }

  /**
   * Check if this persona is rate limited for a room
   */
  isRateLimited(roomId: UUID): boolean {
    const lastTime = this.lastResponseTime.get(roomId);
    if (!lastTime) {
      return false; // Never responded in this room
    }

    const secondsSince = (Date.now() - lastTime.getTime()) / 1000;
    return secondsSince < this.config.minSecondsBetweenResponses;
  }

  /**
   * Get detailed rate limit information for a room
   */
  getRateLimitInfo(roomId: UUID): RateLimitInfo {
    const lastTime = this.lastResponseTime.get(roomId);
    const count = this.responseCount.get(roomId) || 0;

    if (!lastTime) {
      return {
        isLimited: false,
        lastResponseTime: null,
        responseCount: count,
        secondsSinceLastResponse: null,
        waitTimeSeconds: null
      };
    }

    const secondsSince = (Date.now() - lastTime.getTime()) / 1000;
    const isLimited = secondsSince < this.config.minSecondsBetweenResponses;
    const waitTime = isLimited
      ? this.config.minSecondsBetweenResponses - secondsSince
      : null;

    return {
      isLimited,
      lastResponseTime: lastTime,
      responseCount: count,
      secondsSinceLastResponse: secondsSince,
      waitTimeSeconds: waitTime
    };
  }

  /**
   * Track that a response was sent in a room
   * Updates both response time and count
   */
  trackResponse(roomId: UUID): void {
    // Increment response count
    const newCount = (this.responseCount.get(roomId) || 0) + 1;
    this.responseCount.set(roomId, newCount);

    // Track response time for rate limiting
    this.lastResponseTime.set(roomId, new Date());
  }

  /**
   * Check if a message has already been evaluated
   */
  hasEvaluatedMessage(messageId: UUID): boolean {
    return this.evaluatedMessages.has(messageId);
  }

  /**
   * Mark a message as evaluated (deduplication)
   */
  markMessageEvaluated(messageId: UUID): void {
    this.evaluatedMessages.add(messageId);
  }

  /**
   * Get current response count for a room
   */
  getResponseCount(roomId: UUID): number {
    return this.responseCount.get(roomId) || 0;
  }

  /**
   * Check if response count cap has been reached for a room
   */
  hasReachedResponseCap(roomId: UUID): boolean {
    const count = this.responseCount.get(roomId) || 0;
    return count >= this.config.maxResponsesPerSession;
  }

  /**
   * Reset rate limit state for a room (useful for testing)
   */
  resetRoom(roomId: UUID): void {
    this.lastResponseTime.delete(roomId);
    this.responseCount.delete(roomId);
  }

  /**
   * Clear all evaluated messages (useful for testing or memory management)
   */
  clearEvaluatedMessages(): void {
    this.evaluatedMessages.clear();
  }

  /**
   * Get configuration (immutable copy)
   */
  getConfig(): Readonly<RateLimitConfig> {
    return { ...this.config };
  }
}
