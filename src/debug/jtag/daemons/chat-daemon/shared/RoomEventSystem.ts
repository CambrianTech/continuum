/**
 * Room-Scoped Event System - Selective Subscription Architecture
 * 
 * BREAKTHROUGH: Events are room-scoped, not global. Participants only receive
 * events for rooms they've explicitly joined. This prevents event spam and
 * ensures chat widgets/AIs only update for relevant conversations.
 * 
 * Key Architecture:
 * - Room subscription management
 * - Event routing via existing router/transport system
 * - Shared storage integration for chat history
 * - Universal participant event handling
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { JTAGMessage } from '../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory } from '../../../system/core/types/JTAGTypes';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import type { 
  SessionParticipant,
  ChatMessage,
  ChatRoom,
  ParticipantJoinedEventData,
  ParticipantLeftEventData,
  ChatMessageEventData,
  MessageResponseEventData,
  ParticipantUpdateEventData
} from './ChatTypes';

/**
 * Room Event Manager - Handles room-scoped event distribution
 */
export class RoomEventSystem {
  private roomSubscriptions = new Map<string, Set<string>>(); // roomId -> Set<participantSessionId>
  private participantRooms = new Map<string, Set<string>>();  // sessionId -> Set<roomId>
  
  constructor(
    private router: JTAGRouter,
    private storageAdapter: StorageAdapter
  ) {}

  /**
   * Subscribe participant to room events
   * Only participants in a room receive its events
   */
  public subscribeToRoom(sessionId: string, roomId: string): void {
    // Add to room subscription list
    if (!this.roomSubscriptions.has(roomId)) {
      this.roomSubscriptions.set(roomId, new Set());
    }
    this.roomSubscriptions.get(roomId)!.add(sessionId);
    
    // Add to participant's room list
    if (!this.participantRooms.has(sessionId)) {
      this.participantRooms.set(sessionId, new Set());
    }
    this.participantRooms.get(sessionId)!.add(roomId);
    
    console.log(`🔗 Room subscription: ${sessionId} → room ${roomId.slice(0, 8)}`);
  }

  /**
   * Unsubscribe participant from room events
   */
  public unsubscribeFromRoom(sessionId: string, roomId: string): void {
    // Remove from room subscription list
    this.roomSubscriptions.get(roomId)?.delete(sessionId);
    if (this.roomSubscriptions.get(roomId)?.size === 0) {
      this.roomSubscriptions.delete(roomId);
    }
    
    // Remove from participant's room list
    this.participantRooms.get(sessionId)?.delete(roomId);
    if (this.participantRooms.get(sessionId)?.size === 0) {
      this.participantRooms.delete(sessionId);
    }
    
    console.log(`🔌 Room unsubscription: ${sessionId} ← room ${roomId.slice(0, 8)}`);
  }

  /**
   * Get all participants subscribed to a room
   */
  public getRoomParticipants(roomId: string): string[] {
    return Array.from(this.roomSubscriptions.get(roomId) || []);
  }

  /**
   * Get all rooms a participant is subscribed to
   */
  public getParticipantRooms(sessionId: string): string[] {
    return Array.from(this.participantRooms.get(sessionId) || []);
  }

  // ============================================================================
  // ROOM EVENT DISTRIBUTION - Only to subscribed participants
  // ============================================================================

  /**
   * Distribute message event to room participants only
   */
  public async distributeMessageEvent(
    roomId: string,
    message: ChatMessage
  ): Promise<void> {
    // Store message in shared storage for history
    await this.storageAdapter.storeMessage(roomId, message);
    
    // Get room participants (only those subscribed)
    const participants = this.getRoomParticipants(roomId);
    
    console.log(`📨 Distributing message to ${participants.length} participants in room ${roomId.slice(0, 8)}`);
    
    // Send event to each subscribed participant via router
    const eventData: ChatMessageEventData = {
      message,
      roomId
    };
    
    await this.distributeRoomEvent(roomId, 'chat:message-received', eventData);
  }

  /**
   * Distribute participant joined event to room participants
   */
  public async distributeParticipantJoined(
    roomId: string,
    participant: SessionParticipant
  ): Promise<void> {
    const eventData: ParticipantJoinedEventData = {
      participant,
      roomId,
      welcomeMessage: `${participant.displayName} joined the room`
    };
    
    await this.distributeRoomEvent(roomId, 'chat:participant-joined', eventData);
  }

  /**
   * Distribute participant left event to room participants
   */
  public async distributeParticipantLeft(
    roomId: string,
    participantId: string,
    participantName: string,
    reason?: string
  ): Promise<void> {
    const eventData: ParticipantLeftEventData = {
      participantId,
      participantName,
      roomId,
      reason: reason as any || 'manual'
    };
    
    await this.distributeRoomEvent(roomId, 'chat:participant-left', eventData);
  }

  /**
   * Distribute AI/auto-responder response event
   */
  public async distributeResponseEvent(
    roomId: string,
    responseMessage: ChatMessage,
    responseContext?: any
  ): Promise<void> {
    // Store the response message
    await this.storageAdapter.storeMessage(roomId, responseMessage);
    
    const eventData: MessageResponseEventData = {
      responseMessage: responseMessage,
      originalMessage: responseMessage, // TODO: Pass original message parameter
      roomId,
      responseContext
    };
    
    await this.distributeRoomEvent(roomId, 'chat:message-response', eventData);
  }

  /**
   * Distribute room participant list update
   */
  public async distributeParticipantUpdate(
    roomId: string,
    participants: SessionParticipant[],
    changeType: 'joined' | 'left' | 'updated' | 'bulk-update'
  ): Promise<void> {
    const eventData: ParticipantUpdateEventData = {
      roomId,
      participants,
      totalCount: participants.length,
      changeType
    };
    
    await this.distributeRoomEvent(roomId, 'chat:participant-update', eventData);
  }

  // ============================================================================
  // CORE EVENT DISTRIBUTION - Via Router/Transport System
  // ============================================================================

  /**
   * Universal room event distribution
   * Uses existing router/transport system for delivery
   */
  private async distributeRoomEvent(
    roomId: string,
    eventType: string,
    eventData: any
  ): Promise<void> {
    // Get subscribed participants for this room only
    const subscribedSessions = this.getRoomParticipants(roomId);
    
    if (subscribedSessions.length === 0) {
      console.log(`⚠️ No participants subscribed to room ${roomId.slice(0, 8)}`);
      return;
    }
    
    console.log(`🎯 Distributing ${eventType} to ${subscribedSessions.length} participants`);
    
    // Send event to each subscribed session via router
    const deliveryPromises = subscribedSessions.map(async (sessionId) => {
      try {
        // Create proper event payload
        // Import secure context creation
        const { createServerContext } = require('../../../system/core/context/SecureJTAGContext');
        const payloadContext = createServerContext();
        const eventContext = createServerContext();
        
        const eventPayload = {
          eventType,
          eventData,
          roomId,
          targetSessionId: sessionId,
          context: payloadContext,
          sessionId: generateUUID() // System message session
        };
        
        const eventMessage = JTAGMessageFactory.createEvent(
          eventContext,
          'room-event-system',
          'chat/room-event',
          eventPayload
        );
        
        // Route via existing transport system
        await this.router.postMessage(eventMessage);
        
      } catch (error) {
        console.error(`❌ Failed to deliver ${eventType} to session ${sessionId}:`, error);
      }
    });
    
    // Wait for all deliveries (with error tolerance)
    await Promise.allSettled(deliveryPromises);
  }

  // ============================================================================
  // ROOM HISTORY AND STORAGE INTEGRATION
  // ============================================================================

  /**
   * Get shared chat history for room participants
   * All participants in a room see the same history
   */
  public async getRoomHistory(
    roomId: string,
    limit: number = 50,
    beforeTimestamp?: string
  ): Promise<ChatMessage[]> {
    return await this.storageAdapter.getRoomHistory(roomId, limit, beforeTimestamp);
  }

  /**
   * Get room participant list from storage
   */
  public async getRoomParticipantList(roomId: string): Promise<SessionParticipant[]> {
    const sessionIds = this.getRoomParticipants(roomId);
    return await this.storageAdapter.getParticipants(sessionIds);
  }

  // ============================================================================
  // CLEANUP AND MANAGEMENT
  // ============================================================================

  /**
   * Clean up subscriptions for disconnected sessions
   */
  public cleanupDisconnectedSessions(activeSessions: Set<string>): void {
    // Remove subscriptions for sessions that are no longer active
    for (const [sessionId, roomIds] of this.participantRooms.entries()) {
      if (!activeSessions.has(sessionId)) {
        console.log(`🧹 Cleaning up subscriptions for disconnected session ${sessionId}`);
        
        // Remove from all room subscriptions
        for (const roomId of roomIds) {
          this.unsubscribeFromRoom(sessionId, roomId);
        }
      }
    }
  }

  /**
   * Get system statistics
   */
  public getStats() {
    const totalRooms = this.roomSubscriptions.size;
    const totalSubscriptions = Array.from(this.roomSubscriptions.values())
      .reduce((sum, subs) => sum + subs.size, 0);
    const activeSessions = this.participantRooms.size;
    
    return {
      totalRooms,
      totalSubscriptions,
      activeSessions,
      averageParticipantsPerRoom: totalRooms > 0 ? totalSubscriptions / totalRooms : 0
    };
  }
}

// ============================================================================
// STORAGE ADAPTER INTERFACE - For chat history and participant data
// ============================================================================

/**
 * Storage Adapter - Abstract interface for chat persistence
 * Can be implemented with databases, files, or in-memory storage
 */
export interface StorageAdapter {
  // Message storage
  storeMessage(roomId: string, message: ChatMessage): Promise<void>;
  getRoomHistory(roomId: string, limit: number, beforeTimestamp?: string): Promise<ChatMessage[]>;
  
  // Participant storage
  storeParticipant(participant: SessionParticipant): Promise<void>;
  getParticipants(sessionIds: string[]): Promise<SessionParticipant[]>;
  
  // Room management
  createRoom(room: ChatRoom): Promise<void>;
  getRoom(roomId: string): Promise<ChatRoom | null>;
  updateRoom(roomId: string, updates: Partial<ChatRoom>): Promise<void>;
}

// ============================================================================
// ROOM EVENT TYPES - Strongly typed event system
// ============================================================================

export const ROOM_EVENT_TYPES = {
  MESSAGE_RECEIVED: 'chat:message-received',
  MESSAGE_RESPONSE: 'chat:message-response',
  PARTICIPANT_JOINED: 'chat:participant-joined',
  PARTICIPANT_LEFT: 'chat:participant-left',
  PARTICIPANT_UPDATE: 'chat:participant-update',
  ROOM_UPDATED: 'chat:room-updated',
  HISTORY_LOADED: 'chat:history-loaded'
} as const;

export type RoomEventType = typeof ROOM_EVENT_TYPES[keyof typeof ROOM_EVENT_TYPES];

/**
 * Room Event Handler - Interface for handling room events
 * Implemented by chat widgets, AI adapters, etc.
 */
export interface RoomEventHandler {
  handleRoomEvent(eventType: RoomEventType, eventData: any, roomId: string): Promise<void>;
}