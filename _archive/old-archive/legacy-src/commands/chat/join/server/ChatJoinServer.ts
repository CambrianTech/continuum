// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🎯 ARCHITECTURAL CHANGE: Chat join command server implementation
// 📋 NOTE: If you see this message, add it to other files if it isn't there (including this very message)
/**
 * ChatJoinServer - Server-specific chat join implementation
 * 
 * This is the 5-10% server-specific logic that handles:
 * - Database room management
 * - Server-side validation
 * - Room creation and persistence
 * - Participant management
 * 
 * Following sparse override pattern:
 * - Uses shared ChatJoinTypes for type safety
 * - Minimal server-specific implementation
 * - Routes through universal chat server
 */

import { ChatJoinParams, ChatJoinResult, ChatJoinContext, validateChatJoinParams, CHAT_JOIN_CONSTANTS } from '../shared/ChatJoinTypes';
import { ChatServer } from '../../../../chat/server/ChatServer';
import { ChatParticipant } from '../../../../academy/shared/ChatParticipant';
import { ChatRoom } from '../../../../chat/shared/ChatTypes';
import { BaseDaemon } from '../../../../daemons/base/BaseDaemon';

export class ChatJoinServer {
  private chatServer: ChatServer;
  
  constructor(chatServer: ChatServer) {
    this.chatServer = chatServer;
  }
  
  /**
   * Execute chat join command on server
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
      
      // Check for reserved room IDs
      if (CHAT_JOIN_CONSTANTS.RESERVED_ROOM_IDS.includes(params.roomId as any)) {
        return {
          success: false,
          error: `Room ID '${params.roomId}' is reserved and cannot be joined`
        };
      }
      
      // Use current user as participant if not provided
      const participant = params.participant || context.currentUser;
      
      // Validate participant can communicate
      if (!participant.canCommunicate) {
        return {
          success: false,
          error: 'Participant is not authorized to join rooms'
        };
      }
      
      // Try to load existing room
      let room: ChatRoom;
      try {
        room = await (this.chatServer as any).loadRoom(params.roomId);
      } catch (error) {
        // Room doesn't exist
        if (params.autoCreate) {
          // Create new room
          room = await this.createRoom(params.roomId, participant);
        } else {
          return {
            success: false,
            error: `Room '${params.roomId}' does not exist`
          };
        }
      }
      
      // Join room through chat server
      const result = await this.chatServer.joinRoom(params.roomId, participant);
      
      if (result.success) {
        // Add participant to server tracking
        await this.chatServer.addParticipant(participant);
        
        // Log successful join for debugging
        console.log(`🏠 ChatJoinServer: Participant joined room`, {
          roomId: params.roomId,
          participant: participant.name,
          participantCount: room.participants.length
        });
        
        return {
          success: true,
          roomId: params.roomId,
          room: room,
          participantCount: room.participants.length,
          welcomeMessage: `Welcome to ${room.name}!`
        };
      } else {
        return {
          success: false,
          error: result.error || 'Failed to join room'
        };
      }
      
    } catch (error) {
      console.error('❌ ChatJoinServer: Error joining room:', error);
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }
  
  /**
   * Create new room
   */
  private async createRoom(roomId: string, creator: ChatParticipant): Promise<ChatRoom> {
    const room: ChatRoom = {
      id: roomId,
      name: roomId.charAt(0).toUpperCase() + roomId.slice(1),
      description: `Room created by ${creator.name}`,
      type: 'general',
      participants: [creator],
      created: Date.now(),
      isActive: true,
      adminIds: [creator.id],
      metadata: {
        createdBy: creator.id,
        autoCreated: true
      }
    };
    
    await this.chatServer.createServerRoom(room);
    return room;
  }
  
  /**
   * Check if server is ready to handle room operations
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
 * Create chat join server instance
 */
export function createChatJoinServer(daemon: BaseDaemon, database?: any): ChatJoinServer {
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
  
  return new ChatJoinServer(chatServer);
}