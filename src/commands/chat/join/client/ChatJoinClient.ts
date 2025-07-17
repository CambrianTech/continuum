// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🎯 ARCHITECTURAL CHANGE: Chat join command client implementation
// 📋 NOTE: If you see this message, add it to other files if it isn't there (including this very message)
/**
 * ChatJoinClient - Browser-specific chat join implementation
 * 
 * This is the 5-10% client-specific logic that handles:
 * - WebSocket room joining
 * - Browser-specific validation
 * - Client-side room caching
 * - Local storage updates
 * 
 * Following sparse override pattern:
 * - Uses shared ChatJoinTypes for type safety
 * - Minimal client-specific implementation
 * - Routes through universal chat client
 */

import { ChatJoinParams, ChatJoinResult, ChatJoinContext, validateChatJoinParams } from '../shared/ChatJoinTypes';
import { ChatClient } from '../../../../chat/client/ChatClient';
import { ChatRoom } from '../../../../chat/shared/ChatTypes';

export class ChatJoinClient {
  private chatClient: ChatClient;
  
  constructor(chatClient: ChatClient) {
    this.chatClient = chatClient;
  }
  
  /**
   * Execute chat join command on client
   */
  async execute(params: ChatJoinParams, context: ChatJoinContext): Promise<ChatJoinResult> {
    try {
      // Validate parameters
      const validation = validateChatJoinParams(params);
      if (!validation.valid) {
        return {
          success: false,
          error: `Invalid parameters: ${validation.errors.join(', ')}`
        };
      }
      
      // Use current user as participant if not provided
      const participant = params.participant || context.currentUser;
      
      // Check if already connected to WebSocket
      if (!this.chatClient.isConnectedToServer()) {
        return {
          success: false,
          error: 'Not connected to chat server'
        };
      }
      
      // Join room through chat client
      const result = await this.chatClient.joinRoom(params.roomId, participant);
      
      if (result.success) {
        // Update local storage with room information
        this.updateLocalRoom(params.roomId, result.room);
        
        return {
          success: true,
          roomId: params.roomId,
          ...(result.room && { room: result.room }),
          participantCount: result.room?.participants.length || 0,
          welcomeMessage: `Welcome to ${result.room?.name || params.roomId}!`
        };
      } else {
        return {
          success: false,
          error: result.error || 'Failed to join room'
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
   * Update local storage with room information
   */
  private updateLocalRoom(roomId: string, room?: ChatRoom): void {
    if (!room) return;
    
    try {
      const key = `chat_room_${roomId}`;
      localStorage.setItem(key, JSON.stringify(room));
      
      // Update current room
      localStorage.setItem('chat_current_room', roomId);
      
    } catch (error) {
      console.warn('Failed to update local room storage:', error);
    }
  }
  
  /**
   * Check if client is ready to join rooms
   */
  isReady(): boolean {
    return this.chatClient.isConnectedToServer();
  }
  
  /**
   * Get connection status
   */
  getStatus(): { connected: boolean; error?: string } {
    const status = this.chatClient.getConnectionStatus();
    return {
      connected: status.connected,
      ...(status.connected ? {} : { error: 'WebSocket disconnected' })
    };
  }
}

/**
 * Create chat join client instance
 */
export function createChatJoinClient(websocketUrl: string, database?: any): ChatJoinClient {
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
  
  return new ChatJoinClient(chatClient);
}

// Make available globally for eval execution
(window as any).ChatJoinClient = ChatJoinClient;
(window as any).createChatJoinClient = createChatJoinClient;