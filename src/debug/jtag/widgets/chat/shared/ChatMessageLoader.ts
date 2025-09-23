/**
 * Chat Message Loading Utility
 *
 * Extracted from ChatWidget to reduce its complexity.
 * Handles all message loading and pagination logic.
 */

import type { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import type { DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { LoadResult } from '../../shared/InfiniteScrollTypes';

// Constants
const COLLECTIONS = {
  CHAT_MESSAGES: 'chat_messages'
} as const;

/**
 * Handles loading chat messages with cursor-based pagination
 */
export class ChatMessageLoader {
  constructor(
    private readonly executeCommand: (command: string, params: any) => Promise<any>
  ) {}

  /**
   * Load messages for a specific room with cursor pagination
   */
  async loadMessages(
    roomId: string,
    cursor?: string,
    pageSize = 20
  ): Promise<LoadResult<ChatMessageEntity>> {
    console.log('📚 ChatMessageLoader: Loading messages', { roomId, cursor, pageSize });

    const result = await this.executeCommand('data/list', {
      collection: COLLECTIONS.CHAT_MESSAGES,
      filter: { roomId },
      orderBy: [{ field: 'timestamp', direction: 'desc' }],
      limit: pageSize,
      ...(cursor && { cursor: { timestamp: cursor } })
    });

    if (!result?.success || !result.items) {
      throw new Error('Failed to load chat messages');
    }

    return {
      items: result.items,
      hasMore: result.items.length >= pageSize,
      cursor: result.items.length > 0 ? result.items[result.items.length - 1].timestamp : undefined
    };
  }

  /**
   * Load initial messages for a room
   */
  async loadInitialMessages(roomId: string, limit = 20): Promise<ChatMessageEntity[]> {
    const result = await this.loadMessages(roomId, undefined, limit);
    return result.items.slice();
  }
}