/**
 * RateLimiter Module
 *
 * Extracted from PersonaUser.ts (Phase 1, Commit 1.2)
 *
 * Manages voice transcription deduplication (string-keyed, not UUID).
 * Chat message dedup and rate limiting decisions are now in Rust
 * (PersonaCognitionEngine + full_evaluate gate).
 *
 * Retained functionality:
 * - Configuration holding (synced to Rust on init)
 * - Voice transcription dedup (composite string keys, not UUIDs)
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';

export interface RateLimitConfig {
  minSecondsBetweenResponses: number;
  maxResponsesPerSession: number;
}

export class RateLimiter {
  private evaluatedMessages: Set<string> = new Set();
  private readonly config: RateLimitConfig;

  constructor(config?: Partial<RateLimitConfig>) {
    this.config = {
      minSecondsBetweenResponses: config?.minSecondsBetweenResponses ?? 10,
      maxResponsesPerSession: config?.maxResponsesPerSession ?? 50
    };
  }

  /**
   * Check if a message/transcription key has already been evaluated.
   * Used for voice transcription dedup (composite string keys).
   */
  hasEvaluatedMessage(messageId: UUID): boolean {
    return this.evaluatedMessages.has(messageId);
  }

  /**
   * Mark a message/transcription key as evaluated (deduplication).
   * Used for voice transcription dedup (composite string keys).
   */
  markMessageEvaluated(messageId: UUID): void {
    this.evaluatedMessages.add(messageId);
  }

  /**
   * Clear all evaluated messages (called on chat truncation events).
   */
  clearEvaluatedMessages(): void {
    this.evaluatedMessages.clear();
  }

  /**
   * Get configuration (immutable copy). Synced to Rust on init.
   */
  getConfig(): Readonly<RateLimitConfig> {
    return { ...this.config };
  }
}
