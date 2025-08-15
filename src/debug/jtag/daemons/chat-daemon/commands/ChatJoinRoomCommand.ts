/**
 * Chat Join Room Command - Universal Room Joining
 * 
 * BREAKTHROUGH: Universal room joining for ANY participant type:
 * - Human users joining via browser chat widget
 * - AI agents joining via API for auto-response  
 * - Persona systems joining via LoRA adapter
 * - External systems joining via webhook integration
 * 
 * Key Insight: Room joining is just adding a sessionId to room participant list.
 * The system doesn't need to know WHAT the participant is, only their capabilities.
 */

import { CommandBase } from '../../command-daemon/shared/CommandBase';
import type { JTAGContext, CommandParams, CommandResult } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import type { 
  ChatJoinRoomParams,
  ChatJoinRoomResult,
  SessionParticipant,
  ChatRoom,
  ChatMessage,
  ChatCitizen
} from '../shared/ChatTypes';
import { 
  createChatJoinRoomResultFromParams,
  createChatJoinRoomResult
} from '../shared/ChatTypes';
import { RoomCommandCoordinator, RoomUpdateCommands } from '../shared/RoomCommandSystem';
import { AdapterPropertyExtractor } from '../shared/AdapterTypeGuards';

/**
 * Universal Room Join Handler
 * 
 * Manages room membership for all participant types using same interface
 */
export class ChatJoinRoomCommand extends CommandBase<ChatJoinRoomParams, ChatJoinRoomResult> {
  public readonly subpath = 'join-room';
  
  private roomCoordinator: RoomCommandCoordinator;

  constructor(
    context: JTAGContext, 
    router: JTAGRouter,
    roomCoordinator?: RoomCommandCoordinator
  ) {
    // TODO: Need proper commander interface - using router as placeholder
    super('chat-join-room', context, 'join-room', router as any);
    
    this.roomCoordinator = roomCoordinator || new RoomCommandCoordinator(
      context,
      router,
'local' // TODO: Get actual node ID from router or system
    );
  }

  /**
   * Execute room joining - Universal for all participant types
   */
  async execute(params: ChatJoinRoomParams): Promise<ChatJoinRoomResult> {
    console.log(`🚪 ChatJoinRoom: ${params.participantName} joining room ${params.roomId}`);

    try {
      // 1. Create universal participant record
      const participant = this.createParticipant(params);
      
      // 2. Verify room exists (or create if allowed)
      const room = await this.getOrCreateRoom(params.roomId, participant);
      
      // 3. Add participant to room state
      await this.addParticipantToRoom(room.roomId, participant);
      
      // 4. Get recent message history for participant
      const recentMessages = await this.getRecentMessages(room.roomId);
      
      // 5. Get current participant list
      const participantList = await this.getRoomParticipants(room.roomId);
      
      // 6. Notify other room participants of new join
      await this.notifyRoomJoin(room.roomId, participant);
      
      console.log(`✅ ChatJoinRoom: ${participant.displayName} successfully joined room ${room.roomId}`);
      
      return createChatJoinRoomResultFromParams(params, {
        success: true,
        participantId: participant.participantId,
        room,
        recentMessages,
        participantList,
        timestamp: new Date().toISOString(),
        
        // Legacy compatibility
        citizenId: participant.participantId,
        citizenList: participantList.map(this.mapParticipantToCitizen),
        roomName: room.name,
        participantCount: participantList.length
      });
      
    } catch (error) {
      console.error(`❌ ChatJoinRoom: Failed to join room ${params.roomId}:`, error);
      
      return createChatJoinRoomResultFromParams(params, {
        success: false,
        error: {
          message: error instanceof Error ? error.message : 'Unknown error',
          type: 'processing' as const,
          name: 'RoomJoinError',
          timestamp: new Date().toISOString(),
          toJSON: () => ({ 
            message: error instanceof Error ? error.message : 'Unknown error', 
            code: 'ROOM_JOIN_ERROR' 
          })
        }
      });
    }
  }

  /**
   * Create universal participant record - No type-specific logic
   */
  private createParticipant(params: ChatJoinRoomParams): SessionParticipant {
    const participantId = generateUUID();
    const timestamp = new Date().toISOString();
    
    return {
      participantId,
      sessionId: params.sessionId,
      displayName: params.participantName || params.citizenName || `Session-${params.sessionId.substring(0, 8)}`,
      joinedAt: timestamp,
      lastSeen: timestamp,
      isOnline: true,
      
      // Universal capabilities (provided by participant or defaults)
      capabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: params.capabilities?.autoResponds || false,
        providesContext: params.capabilities?.providesContext || false,
        ...params.capabilities
      },
      
      // Adapter info (how this participant connects)
      adapter: params.adapter,
      
      // Start with current room subscribed
      subscribedRooms: [params.roomId]
    };
  }

  /**
   * Get existing room or create new one if allowed
   */
  private async getOrCreateRoom(roomId: string, participant: SessionParticipant): Promise<ChatRoom> {
    // TODO: Integrate with DataDaemon for room storage
    console.log(`🏠 Getting/creating room ${roomId} for ${participant.displayName}`);
    
    // For now, create a minimal room object
    // In real implementation, this would query DataDaemon
    const room: ChatRoom = {
      roomId,
      name: `Room ${roomId.substring(0, 8)}`,
      description: 'Universal chat room',
      createdAt: new Date().toISOString(),
      lastActivity: new Date().toISOString(),
      participantCount: 0, // Will be updated after adding participant
      messageCount: 0,
      isPrivate: false,
      
      // Universal room settings
      category: 'general',
      moderationRules: {
        autoModerationEnabled: false,
        allowAutoResponders: true,
        requireApproval: false
      },
      participantLimits: {
        maxParticipants: 100,
        requireInvite: false,
        allowGuests: true
      },
      messageRetention: {
        maxMessages: 1000,
        maxAgeMs: 30 * 24 * 60 * 60 * 1000, // 30 days
        archiveOldMessages: true
      },
      
      // Legacy compatibility
      citizenCount: 0,
      allowAI: true,
      requireModeration: false,
      maxHistoryLength: 1000
    };
    
    return room;
  }

  /**
   * Add participant to room using location-transparent coordinator
   */
  private async addParticipantToRoom(roomId: string, participant: SessionParticipant): Promise<void> {
    console.log(`➕ Adding ${participant.displayName} to room ${roomId} participant list`);
    
    // Add to room coordinator (handles distributed participant tracking)
    this.roomCoordinator.addParticipantToRoom(
      roomId, 
      participant, 
      'local' // TODO: Get actual node ID
    );
  }

  /**
   * Get recent message history for new participant
   */
  private async getRecentMessages(roomId: string): Promise<readonly ChatMessage[]> {
    // TODO: Integrate with DataDaemon for message history
    console.log(`📜 Getting recent messages for room ${roomId}`);
    
    // For now, return empty array
    // In real implementation: await this.dataDaemon.getRecentMessages(roomId, 20)
    return [];
  }

  /**
   * Get current room participants
   */
  private async getRoomParticipants(roomId: string): Promise<readonly SessionParticipant[]> {
    console.log(`👥 Getting participant list for room ${roomId}`);
    
    // Get from room coordinator
    const distributedParticipants = this.roomCoordinator.getRoomParticipants(roomId);
    
    // Convert back to SessionParticipant (removing nodeId)
    return distributedParticipants.map(({ nodeId, nodeEndpoint, ...participant }) => participant);
  }

  /**
   * Notify existing room participants of new join
   */
  private async notifyRoomJoin(roomId: string, participant: SessionParticipant): Promise<void> {
    console.log(`📢 Notifying room ${roomId} of ${participant.displayName} joining`);
    
    try {
      // Use room coordinator for location-transparent notifications
      const updateData = RoomUpdateCommands.participantJoined(participant);
      await this.roomCoordinator.notifyRoomParticipants(roomId, 'participant-joined', updateData);
      
      console.log(`✅ Join notifications sent for ${participant.displayName}`);
      
    } catch (error) {
      console.error(`❌ Failed to notify room of join:`, error);
      // Don't fail the join if notifications fail
    }
  }

  /**
   * Map SessionParticipant to legacy ChatCitizen format
   * Uses type-safe adapter property extraction
   */
  private mapParticipantToCitizen(participant: SessionParticipant): ChatCitizen {
    // Extract AI properties safely
    const aiProperties = participant.adapter 
      ? AdapterPropertyExtractor.extractAIProperties(participant.adapter)
      : {};

    // Extract generic config safely  
    const genericConfig = participant.adapter
      ? AdapterPropertyExtractor.extractGenericConfig(participant.adapter)
      : {};

    return {
      ...participant,
      citizenId: participant.participantId,
      citizenType: (participant.capabilities?.autoResponds ? 'agent' : 'user') as 'agent' | 'user',
      status: participant.isOnline ? 'active' : 'idle' as const,
      aiConfig: aiProperties.provider ? {
        provider: aiProperties.provider,
        model: aiProperties.model,
        settings: aiProperties.settings
      } : undefined,
      context: genericConfig
    };
  }

  /**
   * Set room coordinator (for dependency injection)
   */
  public setRoomCoordinator(coordinator: RoomCommandCoordinator): void {
    this.roomCoordinator = coordinator;
  }

  /**
   * Get command statistics
   */
  public getStats() {
    return {
      commandType: 'chat-join-room',
      context: {
        environment: this.context.environment,
        sessionId: this.context.uuid,
        nodeId: 'local' // TODO: Get actual node ID
      },
      roomCoordinator: this.roomCoordinator.getSystemStats()
    };
  }
}