/**
 * ChatMessageCache - Per-room message caching for instant tab switches
 *
 * Problem: chat_messages table is 1.1GB, queries take ~5s even with limit:30
 * Solution: Cache recent messages per room in localStorage with TTL
 *
 * Storage format:
 *   Key: 'chat-messages-{roomId}'
 *   Value: { messages: [...], lastFetch: timestamp, totalCount: number }
 *
 * Invalidation:
 *   - TTL expiry (5 minutes)
 *   - New message arrives (via event subscription)
 *   - Manual clear on room data change
 *
 * NOTE: Uses asyncStorage for non-blocking writes. Reads are synchronous
 * but check pending writes first for read-your-writes consistency.
 */

import type { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { asyncStorage } from '../../../system/core/browser/AsyncStorage';

const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

interface CachedRoomMessages {
  messages: ChatMessageEntity[];
  lastFetch: number;
  totalCount: number;
}

export class ChatMessageCache {
  private static readonly PREFIX = 'chat-messages-';
  private static readonly MAX_MESSAGES_PER_ROOM = 50;
  private static readonly TTL_MS = 5 * 60 * 1000; // 5 minutes

  /**
   * Get cached messages for a room
   * Returns null if cache miss or expired
   */
  static get(roomId: UUID): CachedRoomMessages | null {
    if (typeof window === 'undefined') return null;

    try {
      const key = this.PREFIX + roomId;
      const cached = asyncStorage.getItem(key);
      if (!cached) return null;

      const data: CachedRoomMessages = JSON.parse(cached);

      // Check TTL
      if (Date.now() - data.lastFetch > this.TTL_MS) {
        verbose() && console.log(`⏰ ChatMessageCache: Cache expired for room ${roomId}`);
        asyncStorage.removeItem(key);
        return null;
      }

      verbose() && console.log(`⚡ ChatMessageCache: HIT for room ${roomId} (${data.messages.length} messages)`);
      return data;
    } catch (error) {
      console.warn('ChatMessageCache.get error:', error);
      return null;
    }
  }

  /**
   * Cache messages for a room (non-blocking write)
   */
  static set(roomId: UUID, messages: ChatMessageEntity[], totalCount: number): void {
    if (typeof window === 'undefined') return;

    try {
      const key = this.PREFIX + roomId;
      const data: CachedRoomMessages = {
        messages: messages.slice(0, this.MAX_MESSAGES_PER_ROOM),
        lastFetch: Date.now(),
        totalCount
      };
      asyncStorage.setItem(key, JSON.stringify(data));
      verbose() && console.log(`💾 ChatMessageCache: Cached ${data.messages.length} messages for room ${roomId}`);
    } catch (error) {
      // localStorage might be full - ignore
      console.warn('ChatMessageCache.set error:', error);
    }
  }

  /**
   * Add a new message to the cache (for real-time updates)
   * Keeps cache in sync without requiring full refresh (non-blocking write)
   */
  static addMessage(roomId: UUID, message: ChatMessageEntity): void {
    if (typeof window === 'undefined') return;

    try {
      const cached = this.get(roomId);
      if (!cached) return; // No cache to update

      // Add message to the beginning (newest first)
      cached.messages.unshift(message);

      // Trim to max size
      if (cached.messages.length > this.MAX_MESSAGES_PER_ROOM) {
        cached.messages = cached.messages.slice(0, this.MAX_MESSAGES_PER_ROOM);
      }

      // Increment total count
      cached.totalCount++;

      // Save updated cache (non-blocking)
      const key = this.PREFIX + roomId;
      const data: CachedRoomMessages = {
        messages: cached.messages,
        lastFetch: Date.now(), // Refresh TTL
        totalCount: cached.totalCount
      };
      asyncStorage.setItem(key, JSON.stringify(data));

      verbose() && console.log(`📥 ChatMessageCache: Added message to room ${roomId}, now ${cached.messages.length} cached`);
    } catch (error) {
      console.warn('ChatMessageCache.addMessage error:', error);
    }
  }

  /**
   * Invalidate cache for a room (force refresh on next load)
   */
  static invalidate(roomId: UUID): void {
    if (typeof window === 'undefined') return;

    const key = this.PREFIX + roomId;
    asyncStorage.removeItem(key);
    verbose() && console.log(`🗑️ ChatMessageCache: Invalidated room ${roomId}`);
  }

  /**
   * Clear all cached messages (useful for logout/data reset)
   */
  static clearAll(): void {
    if (typeof window === 'undefined') return;

    const keysToRemove = asyncStorage.keys().filter(key => key.startsWith(this.PREFIX));
    keysToRemove.forEach(key => asyncStorage.removeItem(key));
    verbose() && console.log(`🗑️ ChatMessageCache: Cleared ${keysToRemove.length} cached rooms`);
  }

  /**
   * Get cache stats for debugging
   */
  static getStats(): { roomCount: number; totalMessages: number; totalBytes: number } {
    if (typeof window === 'undefined') {
      return { roomCount: 0, totalMessages: 0, totalBytes: 0 };
    }

    let roomCount = 0;
    let totalMessages = 0;
    let totalBytes = 0;

    const cacheKeys = asyncStorage.keys().filter(key => key.startsWith(this.PREFIX));
    for (const key of cacheKeys) {
      roomCount++;
      const value = asyncStorage.getItem(key);
      if (value) {
        totalBytes += value.length * 2; // UTF-16
        try {
          const data: CachedRoomMessages = JSON.parse(value);
          totalMessages += data.messages.length;
        } catch {
          // Ignore parse errors
        }
      }
    }

    return { roomCount, totalMessages, totalBytes };
  }
}
