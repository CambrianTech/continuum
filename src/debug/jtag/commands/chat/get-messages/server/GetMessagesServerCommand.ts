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
        filter: JSON.stringify({ roomId: params.roomId }),
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
      const messages: MessageData[] = typedResult.items.map((item: Record<string, unknown>) => ({
        id: item.messageId || item.id || `msg_${Date.now()}`,
        roomId: item.roomId || params.roomId,
        senderId: item.senderId || item.userId || 'unknown',
        senderName: item.senderName || item.userName || 'Unknown User',
        content: {
          text: item.content || item.message || '',
          attachments: item.attachments || [],
          formatting: item.formatting || { markdown: false }
        },
        timestamp: item.timestamp || new Date().toISOString(),
        replyToId: item.replyToId,
        mentions: item.mentions || [],
        reactions: item.reactions || [],
        status: item.status || 'sent',
        metadata: item.metadata || {}
      }));

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