// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🎯 ARCHITECTURAL CHANGE: Chat send command server implementation
// 📋 NOTE: If you see this message, add it to other files if it isn't there (including this very message)
/**
 * ChatSendServer - Server-specific chat send implementation
 * 
 * This is the 5-10% server-specific logic that handles:
 * - Database persistence
 * - Server-side validation
 * - Message broadcasting
 * - Daemon communication
 * 
 * Following sparse override pattern:
 * - Uses shared ChatSendTypes for type safety
 * - Minimal server-specific implementation
 * - Routes through universal chat server
 */

import { ChatSendParams, ChatSendResult, ChatSendContext, validateChatSendParams } from '../shared/ChatSendTypes';
import { ChatServer } from '../../../../chat/server/ChatServer';
import { BaseDaemon } from '../../../../daemons/base/BaseDaemon';

export class ChatSendServer {
  private chatServer: ChatServer;
  
  constructor(chatServer: ChatServer) {
    this.chatServer = chatServer;
  }
  
  /**
   * Execute chat send command on server
   */
  async execute(params: ChatSendParams, context: ChatSendContext): Promise<ChatSendResult> {
    try {
      // Validate parameters
      const validation = validateChatSendParams(params);
      if (!validation.valid) {
        return {
          success: false,
          error: `Invalid parameters: ${validation.errors.join(', ')}`
        };
      }
      
      // Use current user as sender if not provided
      const sender = params.sender || context.currentUser;
      
      // Validate sender can communicate
      if (!sender.canCommunicate) {
        return {
          success: false,
          error: 'Sender is not authorized to send messages'
        };
      }
      
      // Send message through chat server
      const result = await this.chatServer.sendChatMessage(
        params.content,
        sender,
        params.roomId || context.currentRoom,
        params.messageType || 'text'
      );
      
      if (result.success) {
        // Log successful message for debugging
        console.log(`💬 ChatSendServer: Message sent successfully`, {
          messageId: result.messageId,
          sender: sender.name,
          roomId: params.roomId || context.currentRoom,
          content: params.content.substring(0, 100) + (params.content.length > 100 ? '...' : '')
        });
        
        const sendResult: ChatSendResult = {
          success: true,
          messageId: result.messageId || '',
          timestamp: Date.now()
        };
        
        const roomId = params.roomId || context.currentRoom;
        if (roomId) {
          sendResult.roomId = roomId;
        }
        
        return sendResult;
      } else {
        return {
          success: false,
          error: result.error || 'Failed to send message'
        };
      }
      
    } catch (error) {
      console.error('❌ ChatSendServer: Error sending message:', error);
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }
  
  /**
   * Check if server is ready to handle messages
   */
  isReady(): boolean {
    return this.chatServer !== undefined;
  }
  
  /**
   * Get server statistics
   */
  getStats(): {
    totalRooms: number;
    totalParticipants: number;
    connectedClients: number;
    totalMessages: number;
    uptime: number;
  } {
    return this.chatServer.getServerStats();
  }
}

/**
 * Create chat send server instance
 */
export function createChatSendServer(daemon: BaseDaemon, database?: any): ChatSendServer {
  const chatServer = new ChatServer({
    databasePath: '.continuum/chat/messages.db',
    enableRealTimeSync: true,
    maxRoomsPerServer: 100,
    enableMessageBroadcast: true,
    maxConnections: 1000,
    allowCommands: true,
    allowAttachments: true,
    allowReactions: true
  }, daemon, database);
  
  return new ChatSendServer(chatServer);
}