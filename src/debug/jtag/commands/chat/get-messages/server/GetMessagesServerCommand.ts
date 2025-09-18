/**
 * Get Messages Command - Server Implementation
 */

import { GetMessagesCommand } from '../shared/GetMessagesCommand';
import { type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { GetMessagesParams, MessageData } from '../shared/GetMessagesTypes';
import type { ChatMessage } from '../../../../api/commands/chat/ChatCommands';

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
      const result = await this.remoteExecute({
        collection: 'chat_messages',
        limit: limit * 3, // Get more to account for filtering
        context: this.context,
        sessionId: params.sessionId
      } as any, 'data/list');

      const typedResult = result as any;
      if (!typedResult.success || !typedResult.items) {
        console.warn(`📚 Server: No messages found in chat_messages collection`);
        return [];
      }

      console.log(`🔧 CLAUDE-FIX-${Date.now()}: Retrieved ${typedResult.items.length} total messages, filtering for room ${params.roomId}`);

      // Transform and filter database records to MessageData format
      const allMessages: MessageData[] = typedResult.items
        .map((item: any) => {
          // Extract data from the database record structure
          const data = item.data || item;

          return {
            id: data.messageId || data.id || item.id || `msg_${Date.now()}`,
            roomId: data.roomId || params.roomId,
            senderId: data.senderId || data.userId || 'unknown',
            senderName: data.senderName || data.userName || 'Unknown User',
            content: {
              text: data.content || data.message || data.text || '[MISSING CONTENT]',
              attachments: data.attachments || [],
              formatting: data.formatting || { markdown: false }
            },
            timestamp: data.timestamp || new Date().toISOString(),
            replyToId: data.replyToId,
            mentions: data.mentions || [],
            reactions: data.reactions || [],
            status: data.status || 'sent',
            metadata: data.metadata || item.metadata || {}
          };
        })
        .filter((message: MessageData) => {
          // CRITICAL FIX: Filter for the specific room
          const matches = message.roomId === params.roomId;
          if (matches) {
            console.log(`🔧 CLAUDE-FIX-${Date.now()}: Found message ${message.id} for room ${params.roomId}`);
          }
          return matches;
        })
        .slice(0, limit); // Apply final limit

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