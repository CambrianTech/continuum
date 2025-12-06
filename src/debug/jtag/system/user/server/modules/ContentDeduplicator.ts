/**
 * ContentDeduplicator - Prevents duplicate content from being posted
 *
 * Problem: When AI generates identical content twice (due to retries, race conditions,
 * or deterministic responses to similar prompts), we end up with duplicate messages.
 *
 * Solution: Track recent response content hashes and reject duplicates within time window.
 * This is a simple heuristic approach (as recommended in PersonaResponseGenerator TODO).
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';

/**
 * Content deduplication entry - tracks recent responses to prevent duplicates
 */
interface RecentResponseEntry {
  contentHash: string;
  timestamp: number;
  roomId: UUID;
}

/**
 * Content deduplication configuration
 */
export interface ContentDeduplicatorConfig {
  /** Time window in ms - don't post same content within this window (default: 60s) */
  windowMs?: number;
  /** Max entries to track per persona (default: 50) */
  maxEntries?: number;
  /** Logger function for debug output */
  log?: (message: string) => void;
}

const DEFAULT_WINDOW_MS = 60000;  // 60 seconds
const DEFAULT_MAX_ENTRIES = 50;

/**
 * ContentDeduplicator - Per-persona content deduplication
 *
 * Usage:
 * ```typescript
 * const deduplicator = new ContentDeduplicator({ log: this.log.bind(this) });
 *
 * // Before posting
 * if (deduplicator.isDuplicate(responseText, roomId)) {
 *   return { success: true, wasRedundant: true };
 * }
 *
 * // After successful post
 * deduplicator.recordResponse(responseText, roomId);
 * ```
 */
export class ContentDeduplicator {
  private recentResponses: RecentResponseEntry[] = [];
  private windowMs: number;
  private maxEntries: number;
  private log: (message: string) => void;

  constructor(config: ContentDeduplicatorConfig = {}) {
    this.windowMs = config.windowMs ?? DEFAULT_WINDOW_MS;
    this.maxEntries = config.maxEntries ?? DEFAULT_MAX_ENTRIES;
    this.log = config.log ?? (() => {});
  }

  /**
   * Check if content was recently posted (should be skipped)
   *
   * @param content - The response content to check
   * @param roomId - The room where the response would be posted
   * @returns true if this content is a duplicate (should be skipped)
   */
  isDuplicate(content: string, roomId: UUID): boolean {
    const now = Date.now();
    const contentHash = this.hashContent(content);

    // Clean up expired entries first
    this.recentResponses = this.recentResponses.filter(
      entry => now - entry.timestamp < this.windowMs
    );

    // Check for duplicate in same room
    const isDuplicate = this.recentResponses.some(
      entry => entry.contentHash === contentHash && entry.roomId === roomId
    );

    if (isDuplicate) {
      this.log(`🔄 Content duplicate detected (hash: ${contentHash.slice(0, 8)}), skipping post`);
    }

    return isDuplicate;
  }

  /**
   * Record a response as recently posted
   * Called after successfully posting to prevent immediate duplicates
   *
   * @param content - The posted content
   * @param roomId - The room it was posted to
   */
  recordResponse(content: string, roomId: UUID): void {
    const now = Date.now();
    const contentHash = this.hashContent(content);

    // Add new entry
    this.recentResponses.push({
      contentHash,
      timestamp: now,
      roomId
    });

    // Limit cache size (FIFO eviction)
    while (this.recentResponses.length > this.maxEntries) {
      this.recentResponses.shift();
    }
  }

  /**
   * Get current cache statistics
   */
  getStats(): { entries: number; maxEntries: number; windowMs: number } {
    return {
      entries: this.recentResponses.length,
      maxEntries: this.maxEntries,
      windowMs: this.windowMs
    };
  }

  /**
   * Clear all cached entries (useful for testing)
   */
  clear(): void {
    this.recentResponses = [];
  }

  /**
   * Simple djb2 hash function - fast, non-cryptographic
   * Good enough for duplicate detection within short time windows
   */
  private hashContent(str: string): string {
    // Normalize: trim and lowercase for case-insensitive matching
    const normalized = str.trim().toLowerCase();

    let hash = 5381;
    for (let i = 0; i < normalized.length; i++) {
      hash = ((hash << 5) + hash) + normalized.charCodeAt(i);
      hash = hash & hash; // Convert to 32-bit integer
    }
    return hash.toString(16);
  }
}
