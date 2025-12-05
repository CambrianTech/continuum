/**
 * Chat Export Command - Server Implementation
 * Exports chat messages to markdown format
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { ChatExportCommand } from '../shared/ChatExportCommand';
import type { ChatExportParams, ChatExportResult } from '../shared/ChatExportTypes';
import { RoomEntity } from '../../../../system/data/entities/RoomEntity';
import { ChatMessageEntity } from '../../../../system/data/entities/ChatMessageEntity';
import { Commands } from '../../../../system/core/shared/Commands';
import type { DataListParams, DataListResult } from '../../../data/list/shared/DataListTypes';
import * as fs from 'fs';
import * as path from 'path';

export class ChatExportServerCommand extends ChatExportCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeChatExport(params: ChatExportParams): Promise<ChatExportResult> {
    const collection = params.collection || ChatMessageEntity.collection;
    const includeThreading = params.includeThreading ?? true;

    // 1. Fetch messages with filters
    let messages = await this.fetchMessages(params, collection);

    // 2. Apply post-filters (system/test messages, timestamps)
    messages = this.applyPostFilters(messages, params);

    // 3. Reverse to show oldest first in export
    messages = Array.from(messages).reverse();

    // 4. Generate markdown
    const markdown = this.generateMarkdown(messages, includeThreading, params.room);

    // Write to file or return as string
    if (params.output) {
      const filepath = path.resolve(params.output);
      const dir = path.dirname(filepath);

      // Ensure directory exists
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }

      fs.writeFileSync(filepath, markdown, 'utf-8');

      console.log(`✅ Exported ${messages.length} messages to ${filepath}`);

      return transformPayload(params, {
        success: true,
        message: `Exported ${messages.length} messages to ${filepath}`,
        messageCount: messages.length,
        filepath: filepath,
        collection: collection
      });
    } else {
      // Return markdown as string
      console.log(markdown);

      return transformPayload(params, {
        success: true,
        message: `Exported ${messages.length} messages`,
        messageCount: messages.length,
        markdown: markdown,
        collection: collection
      });
    }
  }

  /**
   * Fetch messages from database with initial filters
   * Returns messages with IDs from DataRecord (entity.id may not be populated)
   */
  private async fetchMessages(params: ChatExportParams, collection: string): Promise<ChatMessageEntity[]> {
    const limit = params.limit || 50;
    const filter: Record<string, unknown> = { ...params.filter };

    // Resolve room if provided
    if (params.room) {
      const room = await this.findRoom(params.room, params);
      filter.roomId = room.id;
    }

    // Query messages using data/list command
    const result = await Commands.execute<DataListParams<ChatMessageEntity>, DataListResult<ChatMessageEntity>>(
      'data/list',
      {
        collection: collection,
        filter: filter,
        orderBy: [{ field: 'timestamp', direction: 'desc' }],
        limit: limit,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    console.log('🔧 fetchMessages QUERY RESULT', {
      success: result.success,
      hasItems: !!result.items,
      recordCount: result.items?.length || 0
    });

    if (!result.success || !result.items) {
      return [];
    }

    // data/list returns entities directly (not wrapped in DataRecord)
    const messagesWithIds = result.items.map(entity => {
      console.log('🔧 fetchMessages ENTITY', {
        entityId: entity.id,
        hasId: !!entity.id,
        senderName: entity.senderName
      });
      return entity;
    });

    console.log('🔧 fetchMessages FINAL', {
      count: messagesWithIds.length,
      firstId: messagesWithIds[0]?.id,
      firstSender: messagesWithIds[0]?.senderName
    });

    return messagesWithIds;
  }

  /**
   * Apply post-filters (system/test messages, timestamps)
   */
  private applyPostFilters(messages: ChatMessageEntity[], params: ChatExportParams): ChatMessageEntity[] {
    let filtered = messages;

    // Filter system messages
    if (!params.includeSystem) {
      filtered = filtered.filter(m => m.metadata?.source !== 'system');
    }

    // Filter test messages
    if (!params.includeTests) {
      filtered = filtered.filter(m => !m.metadata?.isSystemTest);
    }

    // Filter by afterMessageId
    if (params.afterMessageId) {
      const afterIndex = filtered.findIndex(m => m.id === params.afterMessageId);
      if (afterIndex !== -1) {
        filtered = filtered.slice(0, afterIndex);
      }
    }

    // Filter by afterTimestamp
    if (params.afterTimestamp) {
      const timestamp = typeof params.afterTimestamp === 'string'
        ? new Date(params.afterTimestamp)
        : params.afterTimestamp;
      filtered = filtered.filter(m => m.timestamp > timestamp);
    }

    return filtered;
  }

  /**
   * Find room by ID or name
   * Returns entity.id since data/list returns entities directly
   */
  private async findRoom(roomIdOrName: string, params: ChatExportParams): Promise<{ id: import('../../../../system/core/types/CrossPlatformUUID').UUID; entity: RoomEntity }> {
    // Query all rooms using data/list command
    const result = await Commands.execute<DataListParams<RoomEntity>, DataListResult<RoomEntity>>(
      'data/list',
      {
        collection: RoomEntity.collection,
        filter: {},
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (!result.success || !result.items) {
      throw new Error('Failed to query rooms');
    }

    // Find by ID or name
    const room = result.items.find((r: RoomEntity) =>
      r.id === roomIdOrName || r.name === roomIdOrName
    );

    if (!room) {
      const roomNames = result.items.map((r: RoomEntity) => r.name).join(', ');
      throw new Error(`Room not found: ${roomIdOrName}. Available: ${roomNames}`);
    }

    return { id: room.id, entity: room };
  }

  /**
   * Generate markdown from messages
   */
  private generateMarkdown(messages: ChatMessageEntity[], includeThreading: boolean, roomName?: string): string {
    const lines: string[] = [];

    // Header
    lines.push(`# Chat Export${roomName ? ` - ${roomName}` : ''}`);
    lines.push('');
    lines.push(`Exported: ${new Date().toISOString()}`);
    lines.push(`Messages: ${messages.length}`);
    lines.push('');

    // Message ID bookmarks (for pagination)
    if (messages.length > 0) {
      const startMessageId = messages[0].id?.slice(-6) || 'unknown';
      const stopMessageId = messages[messages.length - 1].id?.slice(-6) || 'unknown';
      lines.push(`startMessageId: #${startMessageId}`);
      lines.push(`stopMessageId: #${stopMessageId}`);
      lines.push('');
    }

    lines.push('---');
    lines.push('');

    // Messages
    for (const msg of messages) {
      const shortId = msg.id?.slice(-6) || 'no-id';
      const timestamp = new Date(msg.timestamp).toLocaleString();

      // Message header with short ID
      if (includeThreading) {
        const threadInfo = msg.replyToId ? ` (reply to #${msg.replyToId?.slice(-6) || 'unknown'})` : '';
        lines.push(`## #${shortId} - ${msg.senderName}${threadInfo}`);
      } else {
        lines.push(`## ${msg.senderName}`);
      }

      lines.push(`*${timestamp}*`);
      lines.push('');

      // Message content
      lines.push(msg.content.text);
      lines.push('');

      // Media attachments
      if (msg.content.media && msg.content.media.length > 0) {
        lines.push('**Media:**');
        for (const mediaItem of msg.content.media) {
          const label = mediaItem.filename ?? mediaItem.alt ?? mediaItem.type;
          lines.push(`- ${label}`);
        }
        lines.push('');
      }

      // Reactions
      if (msg.reactions && msg.reactions.length > 0) {
        const reactionStr = msg.reactions.map(r => `${r.emoji} (${r.userId.slice(-6)})`).join(', ');
        lines.push(`*Reactions: ${reactionStr}*`);
        lines.push('');
      }

      lines.push('---');
      lines.push('');
    }

    return lines.join('\n');
  }
}
