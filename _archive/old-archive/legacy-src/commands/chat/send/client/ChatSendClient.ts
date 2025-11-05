// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🎯 ARCHITECTURAL CHANGE: Chat send command client implementation
// 📋 NOTE: If you see this message, add it to other files if it isn't there (including this very message)
/**
 * ChatSendClient - Browser-specific chat send implementation
 * 
 * This is the 5-10% client-specific logic that handles:
 * - WebSocket message sending
 * - Browser-specific validation
 * - Client-side error handling
 * - Local message caching
 * 
 * Following sparse override pattern:
 * - Uses shared ChatSendTypes for type safety
 * - Minimal client-specific implementation
 * - Routes through universal chat client
 */

import { ChatSendParams, ChatSendResult, ChatSendContext, validateChatSendParams } from '../shared/ChatSendTypes';
import { ChatClient } from '../../../../chat/client/ChatClient';

export class ChatSendClient {
  private chatClient: ChatClient;
  
  constructor(chatClient: ChatClient) {
    this.chatClient = chatClient;
  }
  
  /**
   * Execute chat send command on client
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
      
      // Send message through chat client
      const result = await this.chatClient.sendChatMessage(
        params.content,
        sender,
        params.roomId || context.currentRoom,
        params.messageType || 'text'
      );
      
      if (result.success) {
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
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }
  
  /**
   * Check if client is ready to send messages
   */
  isReady(): boolean {
    return this.chatClient.isConnectedToServer();
  }
  
  /**
   * Get connection status
   */
  getStatus(): { connected: boolean; error?: string } {
    const status = this.chatClient.getConnectionStatus();
    const result: { connected: boolean; error?: string } = {
      connected: status.connected
    };
    
    if (!status.connected) {
      result.error = 'WebSocket disconnected';
    }
    
    return result;
  }
}

/**
 * Create chat send client instance
 */
export function createChatSendClient(websocketUrl: string, database?: any): ChatSendClient {
  const chatClient = new ChatClient({
    websocketUrl,
    reconnectInterval: 3000,
    maxReconnectAttempts: 5,
    enableOfflineStorage: true,
    maxConnections: 1,
    allowCommands: true,
    allowAttachments: true,
    allowReactions: true
  }, database);
  
  return new ChatSendClient(chatClient);
}

// Make available globally for eval execution
(window as any).ChatSendClient = ChatSendClient;
(window as any).createChatSendClient = createChatSendClient;