/**
 * Get Messages Command - Server Implementation
 */

import { GetMessagesCommand } from '../shared/GetMessagesCommand';
import { type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { GetMessagesParams, MessageData } from '../shared/GetMessagesTypes';
import type { ChatMessage } from '../../../../api/commands/chat/ChatCommands';
import type { DataListParams, DataListResult } from '../../../../commands/data/list/shared/DataListTypes';

export class GetMessagesServerCommand extends GetMessagesCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async retrieveMessages(
    params: GetMessagesParams,
    limit: number
  ): Promise<MessageData[]> {
    try {
      console.log(`🔧 CLAUDE-FIX-${Date.now()}: Server: Reading messages via data/list for room ${params.roomId}`);

      // CRITICAL FIX: Database filter not working for nested data, get all and filter programmatically
      // Use very large buffer to ensure we get the newest messages in chronological sorting
      const dataListParams: DataListParams = {
        collection: 'chat_messages',
        limit: 1000, // Large buffer to ensure newest messages aren't filtered out
        context: this.context,
        sessionId: params.sessionId
      };

      const result = await this.remoteExecute(dataListParams, 'data/list');
      const typedResult = result as DataListResult<unknown>;
      if (!typedResult.success || !typedResult.items) {
        console.warn(`📚 Server: No messages found in chat_messages collection`);
        return [];
      }

      console.log(`🔧 CLAUDE-FIX-${Date.now()}: Retrieved ${typedResult.items.length} total messages, filtering for room ${params.roomId}`);

      // CRITICAL FIX: Transform ALL records first, then filter by room, THEN sort chronologically
      const allTransformedMessages: MessageData[] = typedResult.items.map((item: unknown) => {
        // Type guard to ensure item is a record-like object
        if (!item || typeof item !== 'object') {
          console.warn(`🔧 CLAUDE-FIX-${Date.now()}: Invalid item structure:`, item);
          return null;
        }

        const itemRecord = item as Record<string, unknown>;
        // Extract data from the database record structure - handle JSON string parsing
        let data: Record<string, unknown>;
        if (typeof itemRecord.data === 'string') {
          try {
            data = JSON.parse(itemRecord.data) as Record<string, unknown>;
          } catch (error) {
            console.warn(`🔧 CLAUDE-FIX-${Date.now()}: Failed to parse JSON data for item ${itemRecord.id}:`, error);
            data = itemRecord; // Fallback to item itself
          }
        } else {
          data = (itemRecord.data as Record<string, unknown>) || itemRecord;
        }

        return {
          id: (data.messageId || data.id || itemRecord.id || `msg_${Date.now()}`) as string,
          roomId: (data.roomId || params.roomId) as string,
          senderId: (data.senderId || data.userId || 'unknown') as string,
          senderName: (data.senderName || data.userName || 'Unknown User') as string,
          content: {
            text: (data.content || data.message || data.text || '[MISSING CONTENT]') as string,
            attachments: (data.attachments || []) as unknown[],
            formatting: (data.formatting || { markdown: false }) as Record<string, unknown>
          },
          timestamp: (data.timestamp || new Date().toISOString()) as string,
          replyToId: data.replyToId as string | undefined,
          mentions: (data.mentions || []) as unknown[],
          reactions: (data.reactions || []) as unknown[],
          status: (data.status || 'sent') as string,
          metadata: (data.metadata || itemRecord.metadata || {}) as Record<string, unknown>
        };
      }).filter((message): message is MessageData => message !== null);

      // STEP 1: Filter by room FIRST (before chronological sorting)
      const roomMessages = allTransformedMessages.filter((message: MessageData) => {
        const matches = message.roomId === params.roomId;
        if (matches) {
          console.log(`🔧 CLAUDE-FIX-${Date.now()}: Found message ${message.id} for room ${params.roomId}: "${message.content.text}"`);
        }
        return matches;
      });

      // STEP 2: Sort room messages DESC (newest first) for selection
      roomMessages.sort((a, b) => {
        const aTime = new Date(a.timestamp).getTime();
        const bTime = new Date(b.timestamp).getTime();
        return bTime - aTime; // DESC - newest first
      });

      // STEP 3: Take the most recent N room messages
      const recentRoomMessages = roomMessages.slice(0, limit);

      // STEP 4: Sort those N messages ASC (oldest first) for display
      recentRoomMessages.sort((a, b) => {
        const aTime = new Date(a.timestamp).getTime();
        const bTime = new Date(b.timestamp).getTime();
        return aTime - bTime; // ASC - oldest first, recent at bottom
      });

      const allMessages = recentRoomMessages;

      console.log(`📚 Server: Retrieved ${allMessages.length} messages for room ${params.roomId} (filtered from ${typedResult.items.length} total)`);
      return allMessages;

    } catch (error) {
      console.error(`❌ Server: Failed to retrieve messages:`, error);
      throw error;
    }
  }

  protected getEnvironmentLabel(): string {
    return 'SERVER';
  }
}