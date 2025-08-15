/**
 * Location-Transparent Room Command System
 * 
 * BREAKTHROUGH: No custom event system needed! JTAG commands already provide:
 * - Location transparency (local vs /remote/{nodeId}/command)
 * - Cross-context transport (browser ↔ server via router)
 * - Type-safe message routing (command handlers)
 * - Session targeting (specific participants only)
 * 
 * Architecture:
 * - Commands work identically for local and distributed rooms
 * - Router handles all transport complexity (WebSocket, P2P, etc.)
 * - Same interfaces for browser widgets and AI adapters
 * - Room updates = targeted commands to room participants
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory, createPayload } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import type { 
  SessionParticipant,
  DistributedParticipant,
  ChatMessage,
  ChatRoom,
  DistributedChatRoom,
  ChatRoomUpdateParams,
  RoomUpdateType,
  RoomUpdateData
} from './ChatTypes';

/**
 * Room Command Coordinator - Location Transparent
 * 
 * Manages room state and coordinates updates across local/remote participants
 * using JTAG's built-in location-transparent command routing.
 */
export class RoomCommandCoordinator {
  // Room participant registry - tracks which node each participant is on
  private roomParticipants = new Map<string, DistributedParticipant[]>();
  
  constructor(
    private context: JTAGContext,
    private router: JTAGRouter,
    private currentNodeId: string
  ) {}

  /**
   * Add participant to room - tracks their node location
   */
  addParticipantToRoom(
    roomId: string, 
    participant: SessionParticipant, 
    nodeId: string = this.currentNodeId
  ): void {
    if (!this.roomParticipants.has(roomId)) {
      this.roomParticipants.set(roomId, []);
    }
    
    const distributedParticipant: DistributedParticipant = {
      ...participant,
      nodeId
    };
    
    this.roomParticipants.get(roomId)!.push(distributedParticipant);
    console.log(`🏠 Added participant ${participant.displayName} to room ${roomId} on node ${nodeId}`);
  }

  /**
   * Remove participant from room
   */
  removeParticipantFromRoom(roomId: string, participantId: UUID): void {
    const participants = this.roomParticipants.get(roomId);
    if (!participants) return;
    
    const index = participants.findIndex(p => p.participantId === participantId);
    if (index >= 0) {
      const participant = participants[index];
      participants.splice(index, 1);
      console.log(`🚪 Removed participant ${participant.displayName} from room ${roomId}`);
    }
  }

  /**
   * Get all participants in a room (distributed)
   */
  getRoomParticipants(roomId: string): DistributedParticipant[] {
    return this.roomParticipants.get(roomId) || [];
  }

  /**
   * Get participants grouped by node
   */
  getParticipantsByNode(roomId: string): Record<string, DistributedParticipant[]> {
    const participants = this.getRoomParticipants(roomId);
    const byNode: Record<string, DistributedParticipant[]> = {};
    
    for (const participant of participants) {
      if (!byNode[participant.nodeId]) {
        byNode[participant.nodeId] = [];
      }
      byNode[participant.nodeId].push(participant);
    }
    
    return byNode;
  }

  /**
   * Notify all room participants - Location Transparent
   * 
   * Uses JTAG's built-in routing to deliver updates to participants
   * regardless of which node they're on.
   */
  async notifyRoomParticipants(
    roomId: string,
    updateType: RoomUpdateType,
    data: RoomUpdateData
  ): Promise<void> {
    const participants = this.getRoomParticipants(roomId);
    
    if (participants.length === 0) {
      console.log(`⚠️ No participants to notify in room ${roomId}`);
      return;
    }
    
    console.log(`📢 Notifying ${participants.length} participants in room ${roomId} of ${updateType}`);
    
    // Send room update command to each participant
    // JTAG router automatically handles local vs remote routing
    const notifications = participants.map(participant => 
      this.sendRoomUpdateCommand(participant, roomId, updateType, data)
    );
    
    // Send all notifications in parallel (location transparent)
    const results = await Promise.allSettled(notifications);
    
    // Log any delivery failures
    results.forEach((result, index) => {
      if (result.status === 'rejected') {
        const participant = participants[index];
        console.error(`❌ Failed to notify ${participant.displayName} on ${participant.nodeId}:`, result.reason);
      }
    });
  }

  /**
   * Send room update command to specific participant
   * 
   * LOCATION TRANSPARENT: Automatically routes to correct node
   */
  private async sendRoomUpdateCommand(
    participant: DistributedParticipant,
    roomId: string,
    updateType: RoomUpdateType,
    data: RoomUpdateData
  ): Promise<void> {
    // Build location-transparent endpoint
    const endpoint = this.buildEndpoint(participant.nodeId, 'chat/room-update');
    
    const updateParams: ChatRoomUpdateParams = {
      context: this.context,
      sessionId: this.context.uuid,
      roomId,
      updateType,
      data,
      targetSessionId: participant.sessionId, // Target specific session
      originNodeId: this.currentNodeId
    };
    
    try {
      // JTAG router handles all routing complexity
      const payload = createPayload(this.context, this.context.uuid, updateParams);
      const message = JTAGMessageFactory.createRequest(
        this.context,
        `${this.context.environment}/chat-daemon`,
        endpoint,
        payload,
        generateUUID()
      );
      await this.router.postMessage(message);
      
      console.log(`✅ Sent ${updateType} to ${participant.displayName} on ${participant.nodeId}`);
      
    } catch (error) {
      console.error(`❌ Failed to send ${updateType} to ${participant.displayName}:`, error);
      throw error;
    }
  }

  /**
   * Build location-transparent endpoint
   * 
   * Returns:
   * - Local: "chat/room-update"  
   * - Remote: "/remote/{nodeId}/chat/room-update"
   */
  private buildEndpoint(nodeId: string, command: string): string {
    return nodeId === this.currentNodeId 
      ? command 
      : `/remote/${nodeId}/${command}`;
  }

  /**
   * Process message and trigger room notifications
   * 
   * This is called after a message is stored to notify all room participants
   */
  async processMessageSent(
    roomId: string, 
    message: ChatMessage,
    room?: ChatRoom
  ): Promise<void> {
    // Notify all room participants of new message
    await this.notifyRoomParticipants(roomId, 'message-sent', {
      message,
      metadata: {
        timestamp: message.timestamp,
        senderName: message.senderName
      }
    });
    
    // Trigger auto-responses (if any participants have autoResponds capability)
    await this.processAutoResponses(roomId, message, room);
  }

  /**
   * Process auto-responses using Universal Response Engine
   * 
   * Participants with autoResponds capability will evaluate the message
   * and potentially generate responses.
   */
  private async processAutoResponses(
    roomId: string,
    message: ChatMessage,
    room?: ChatRoom
  ): Promise<void> {
    const participants = this.getRoomParticipants(roomId);
    const autoResponders = participants.filter(p => p.capabilities?.autoResponds);
    
    if (autoResponders.length === 0) {
      return; // No auto-responders in this room
    }
    
    console.log(`🤖 Processing auto-responses for ${autoResponders.length} responders in room ${roomId}`);
    
    // Each auto-responder will receive the message-sent notification
    // and decide whether to respond based on their own logic.
    // No centralized AI logic needed - each responder handles their own triggers!
  }

  /**
   * Get room statistics - useful for debugging and monitoring
   */
  getRoomStats(roomId: string) {
    const participants = this.getRoomParticipants(roomId);
    const byNode = this.getParticipantsByNode(roomId);
    
    return {
      totalParticipants: participants.length,
      nodeCount: Object.keys(byNode).length,
      participantsByNode: Object.entries(byNode).map(([nodeId, nodeParticipants]) => ({
        nodeId,
        count: nodeParticipants.length,
        participants: nodeParticipants.map(p => ({
          name: p.displayName,
          autoResponds: p.capabilities?.autoResponds || false
        }))
      })),
      isDistributed: Object.keys(byNode).length > 1
    };
  }

  /**
   * Clean up disconnected participants
   * 
   * Should be called periodically to remove stale participant entries
   */
  cleanupDisconnectedParticipants(activeSessionIds: Set<string>): void {
    for (const [roomId, participants] of this.roomParticipants.entries()) {
      const activeBefore = participants.length;
      
      // Filter out participants whose sessions are no longer active
      const activeParticipants = participants.filter(p => 
        activeSessionIds.has(p.sessionId)
      );
      
      if (activeParticipants.length !== activeBefore) {
        this.roomParticipants.set(roomId, activeParticipants);
        console.log(`🧹 Cleaned up ${activeBefore - activeParticipants.length} disconnected participants from room ${roomId}`);
      }
    }
  }

  /**
   * Get system-wide statistics
   */
  getSystemStats() {
    const allRooms = Array.from(this.roomParticipants.keys());
    const totalParticipants = Array.from(this.roomParticipants.values())
      .reduce((sum, participants) => sum + participants.length, 0);
    
    const nodeDistribution: Record<string, number> = {};
    for (const participants of this.roomParticipants.values()) {
      for (const participant of participants) {
        nodeDistribution[participant.nodeId] = (nodeDistribution[participant.nodeId] || 0) + 1;
      }
    }
    
    return {
      totalRooms: allRooms.length,
      totalParticipants,
      nodesInvolved: Object.keys(nodeDistribution).length,
      nodeDistribution,
      currentNode: this.currentNodeId,
      distributedRooms: allRooms.filter(roomId => {
        const byNode = this.getParticipantsByNode(roomId);
        return Object.keys(byNode).length > 1;
      }).length
    };
  }
}

/**
 * Chat Room Update Command Handler
 * 
 * This handler receives room update commands and processes them.
 * It works identically whether called locally or from a remote node.
 */
export interface RoomUpdateHandler {
  handleRoomUpdate(params: ChatRoomUpdateParams): Promise<void>;
}

/**
 * Room Update Command Builder - Helper functions
 */
export class RoomUpdateCommands {
  static messagesSent(message: ChatMessage): RoomUpdateData {
    return {
      message,
      metadata: {
        type: 'message',
        senderId: message.senderId,
        timestamp: message.timestamp
      }
    };
  }
  
  static participantJoined(participant: SessionParticipant): RoomUpdateData {
    return {
      participant,
      metadata: {
        type: 'join',
        participantName: participant.displayName,
        timestamp: new Date().toISOString()
      }
    };
  }
  
  static participantLeft(participant: SessionParticipant, reason?: string): RoomUpdateData {
    return {
      participant,
      reason,
      metadata: {
        type: 'leave',
        participantName: participant.displayName,
        timestamp: new Date().toISOString()
      }
    };
  }
  
  static participantResponse(responseMessage: ChatMessage, originalMessage: ChatMessage): RoomUpdateData {
    return {
      message: responseMessage,
      metadata: {
        type: 'response',
        originalMessageId: originalMessage.messageId,
        responderId: responseMessage.senderId,
        timestamp: responseMessage.timestamp
      }
    };
  }
}