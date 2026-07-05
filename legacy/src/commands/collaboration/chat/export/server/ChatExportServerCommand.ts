/**
 * Chat Export Command - Server Implementation
 * Exports chat messages to markdown format
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { ChatExportCommand } from '../shared/ChatExportCommand';
import type { ChatExportParams, ChatExportResult } from '../shared/ChatExportTypes';
import { ChatMessageEntity } from '@system/data/entities/ChatMessageEntity';
import { Commands } from '@system/core/shared/Commands';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import { resolveRoomIdentifier } from '@system/routing/RoutingService';
import * as fs from 'fs';
import * as path from 'path';
import { SystemPaths } from '@system/core/config/SystemPaths';

import { DataList } from '../../../../data/list/shared/DataListTypes';
export class ChatExportServerCommand extends ChatExportCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeChatExport(params: ChatExportParams): Promise<ChatExportResult> {
    const collection = params.collection || ChatMessageEntity.collection;
    const includeThreading = params.includeThreading ?? true;

    // Resolve room ONCE up front through the canonical resolver — used both
    // for the data/list filter (needs UUID) and the markdown header (wants
    // displayName). Pre-fix this command had its own findRoom() that only
    // matched RoomEntity.id and RoomEntity.name, so chat/send accepting
    // 'general' (uniqueId) but chat/export rejecting it as "Room not
    // found" was a real input asymmetry — Carl-UX QA #94 from airc-8a5e
    // 2026-05-03. resolveRoomIdentifier handles uniqueId/UUID/name and
    // is documented as "THE SINGLE SOURCE OF TRUTH for room resolution"
    // in RoutingService.ts.
    let resolvedRoomId: string | undefined;
    let resolvedRoomDisplayName: string | undefined;
    if (params.room) {
      const resolved = await resolveRoomIdentifier(params.room);
      if (!resolved) {
        throw new Error(`Room not found: ${params.room}`);
      }
      resolvedRoomId = resolved.id;
      resolvedRoomDisplayName = resolved.displayName;
    }

    // 1. Fetch messages with filters
    let messages = await this.fetchMessages(params, collection, resolvedRoomId);

    // 2. Apply post-filters (system/test messages, timestamps)
    messages = this.applyPostFilters(messages, params);

    // 3. Reverse to show oldest first in export
    messages = Array.from(messages).reverse();

    // 4. Generate markdown — prefer canonical displayName from the resolver
    // so the export header reads "Chat Export - General" regardless of
    // whether the user typed --room=general or --room=General.
    const markdown = this.generateMarkdown(messages, includeThreading, resolvedRoomDisplayName ?? params.room);

    // Write to file or return as string
    if (params.output) {
      // Resolve relative paths against safe exports directory, not cwd (which is src/)
      // This prevents bare filenames like "export.md" from polluting the repo
      const filepath = path.isAbsolute(params.output)
        ? params.output
        : path.join(SystemPaths.shared.exports, params.output);
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
  private async fetchMessages(params: ChatExportParams, collection: string, resolvedRoomId?: string): Promise<ChatMessageEntity[]> {
    const limit = params.limit || 50;
    const filter: Record<string, unknown> = { ...params.filter };

    if (resolvedRoomId) {
      filter.roomId = resolvedRoomId;
    }

    // Query messages using data/list command
    const result = await DataList.execute<ChatMessageEntity>({
        dbHandle: 'default',
        collection: collection,
        filter: filter,
        orderBy: [{ field: 'timestamp', direction: 'desc' }],
        limit: limit,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (!result.success || !result.items) {
      return [];
    }

    // data/list returns entities directly (not wrapped in DataRecord)
    return [...result.items];
  }

  /**
   * Safely access metadata as an object, handling both parsed objects and JSON strings.
   * The Rust ORM may return JSON fields as strings depending on the storage backend.
   */
  private parseMeta(m: ChatMessageEntity): Partial<import('@system/data/entities/ChatMessageEntity').MessageMetadata> | undefined {
    if (!m.metadata) return undefined;
    if (typeof m.metadata === 'string') {
      try { return JSON.parse(m.metadata); } catch { return undefined; }
    }
    return m.metadata;
  }

  /**
   * Apply post-filters (system/test messages, timestamps)
   */
  private applyPostFilters(messages: ChatMessageEntity[], params: ChatExportParams): ChatMessageEntity[] {
    let filtered = messages;

    // Filter system messages
    if (!params.includeSystem) {
      filtered = filtered.filter(m => this.parseMeta(m)?.source !== 'system');
    }

    // Filter tool result messages (stored by PersonaToolExecutor for RAG, not for display)
    if (!params.includeSystem) {
      filtered = filtered.filter(m => !this.parseMeta(m)?.toolResult);
    }

    // Filter test messages
    if (!params.includeTests) {
      filtered = filtered.filter(m => !this.parseMeta(m)?.isSystemTest);
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
