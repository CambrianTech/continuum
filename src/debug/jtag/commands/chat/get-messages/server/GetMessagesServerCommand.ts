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
      console.log(`📚 Server: Reading messages via data/list for room ${params.roomId}`);
      
      // Simple like other commands - use existing data/list 
      const result = await this.remoteExecute({
        collection: 'chat_messages',
        filter: { roomId: params.roomId },
        limit: limit,
        context: this.context,
        sessionId: params.sessionId
      } as any, 'data/list');

      const typedResult = result as any;
      if (!typedResult.success || !typedResult.items) {
        console.warn(`📚 Server: No messages found for room ${params.roomId}`);
        return [];
      }

      // Transform database records to MessageData format
      const messages: MessageData[] = typedResult.items.map((item: any) => {
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
      });

      console.log(`📚 Server: Retrieved ${messages.length} messages for room ${params.roomId}`);
      return messages;

    } catch (error) {
      console.error(`❌ Server: Failed to retrieve messages:`, error);
      throw error;
    }
  }

  protected getEnvironmentLabel(): string {
    return 'SERVER';
  }
}