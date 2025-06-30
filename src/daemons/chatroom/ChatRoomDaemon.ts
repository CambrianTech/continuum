/**
 * ChatRoomDaemon - Universal Chat and Room Management System
 * ==========================================================
 * 
 * Manages chat rooms, message routing, and real-time communication across
 * all Continuum sessions including human-AI collaboration, persona interactions,
 * and multi-participant sessions.
 * 
 * CRITICAL TESTING REQUIREMENTS:
 * ===============================
 * INTEGRATION TEST COVERAGE NEEDED:
 * - Multi-room message routing: Verify isolation between different chat rooms
 * - Real-time message delivery: Test WebSocket message broadcasting to all participants
 * - Room lifecycle management: Create, join, leave, destroy room operations
 * - Persistence layer: Messages saved/retrieved from DatabaseDaemon correctly
 * - Session integration: Rooms properly tied to SessionManagerDaemon sessions
 * - User presence tracking: Online/offline status management across rooms
 * 
 * LOGGING STRATEGY FOR FAILURE DETECTION:
 * - Message delivery timing and success rates per room
 * - Database write/read operations with error tracking
 * - WebSocket connection health monitoring per participant
 * - Room capacity and resource usage monitoring
 * - Message history retrieval performance metrics
 * 
 * Architecture Integration:
 * - Delegates to DatabaseDaemon for message persistence
 * - Integrates with SessionManagerDaemon for user session tracking
 * - Uses WebSocket daemon for real-time message broadcasting
 * - Coordinates with CommandProcessorDaemon for chat command routing
 * 
 * CRITICAL TODO LIST:
 * ===================
 * MODULARITY ISSUES:
 * - Message storage should delegate to DatabaseDaemon, not implement directly
 * - WebSocket management should use WebSocketDaemon, not duplicate logic
 * - User presence should integrate with SessionManagerDaemon
 * 
 * MISSING FUNCTIONALITY:
 * - Unit tests for room lifecycle operations
 * - Integration tests for message persistence
 * - Real-time delivery performance testing
 * - Room capacity limits and scaling
 * 
 * PERFORMANCE CONCERNS:
 * - Message history loading should be paginated
 * - Room participant lists need efficient lookup
 * - WebSocket connection pooling for large rooms
 */

import { RequestResponseDaemon, RequestHandler, RequestHandlerMap } from '../base/RequestResponseDaemon.js';
import { BaseDaemon } from '../base/BaseDaemon.js';

// ChatRoom types and interfaces
interface ChatRoom {
  id: string;
  name: string;
  type: 'chat' | 'collaboration' | 'system';
  participants: Set<string>;
  created_at: Date;
  created_by: string;
  metadata: Record<string, any>;
}

interface ChatMessage {
  id: string;
  room_id: string;
  sender_id: string;
  content: string;
  timestamp: Date;
  message_type: 'text' | 'system' | 'command';
  metadata: Record<string, any>;
}

interface RoomParticipant {
  user_id: string;
  session_id: string;
  joined_at: Date;
  role: 'participant' | 'moderator' | 'owner';
  status: 'online' | 'away' | 'offline';
}

export class ChatRoomDaemon extends RequestResponseDaemon {
  private rooms: Map<string, ChatRoom> = new Map();
  private participants: Map<string, Set<RoomParticipant>> = new Map();
  private messageHistory: Map<string, ChatMessage[]> = new Map();

  constructor() {
    super('chatroom', '1.0.0');
  }

  protected defineRequestHandlers(): RequestHandlerMap {
    return {
      'create_room': this.handleCreateRoom.bind(this),
      'join_room': this.handleJoinRoom.bind(this),
      'leave_room': this.handleLeaveRoom.bind(this),
      'send_message': this.handleSendMessage.bind(this),
      'get_messages': this.handleGetMessages.bind(this),
      'list_rooms': this.handleListRooms.bind(this),
      'get_room_info': this.handleGetRoomInfo.bind(this),
      'delete_room': this.handleDeleteRoom.bind(this)
    };
  }

  private async handleCreateRoom(params: any): Promise<any> {
    const { name, type = 'chat', created_by, metadata = {} } = params;
    
    if (!name || !created_by) {
      throw new Error('Room name and creator are required');
    }

    const roomId = `room_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    
    const room: ChatRoom = {
      id: roomId,
      name,
      type,
      participants: new Set([created_by]),
      created_at: new Date(),
      created_by,
      metadata
    };

    this.rooms.set(roomId, room);
    this.participants.set(roomId, new Set([{
      user_id: created_by,
      session_id: params.session_id || 'unknown',
      joined_at: new Date(),
      role: 'owner',
      status: 'online'
    }]));
    this.messageHistory.set(roomId, []);

    this.log(`✅ Room created: ${name} (${roomId}) by ${created_by}`);

    // TODO: Delegate to DatabaseDaemon for persistence
    // await this.delegateToDatabaseDaemon('save_room', room);

    return {
      success: true,
      room_id: roomId,
      room: room
    };
  }

  private async handleJoinRoom(params: any): Promise<any> {
    const { room_id, user_id, session_id } = params;
    
    const room = this.rooms.get(room_id);
    if (!room) {
      throw new Error(`Room ${room_id} not found`);
    }

    room.participants.add(user_id);
    
    const roomParticipants = this.participants.get(room_id) || new Set();
    roomParticipants.add({
      user_id,
      session_id: session_id || 'unknown',
      joined_at: new Date(),
      role: 'participant',
      status: 'online'
    });
    this.participants.set(room_id, roomParticipants);

    this.log(`✅ User ${user_id} joined room ${room.name} (${room_id})`);

    // TODO: Broadcast join message to other participants
    // await this.broadcastToRoom(room_id, { type: 'user_joined', user_id });

    return {
      success: true,
      room: room,
      participant_count: room.participants.size
    };
  }

  private async handleLeaveRoom(params: any): Promise<any> {
    const { room_id, user_id } = params;
    
    const room = this.rooms.get(room_id);
    if (!room) {
      throw new Error(`Room ${room_id} not found`);
    }

    room.participants.delete(user_id);
    
    const roomParticipants = this.participants.get(room_id);
    if (roomParticipants) {
      const updatedParticipants = new Set(
        Array.from(roomParticipants).filter(p => p.user_id !== user_id)
      );
      this.participants.set(room_id, updatedParticipants);
    }

    this.log(`✅ User ${user_id} left room ${room.name} (${room_id})`);

    return {
      success: true,
      participant_count: room.participants.size
    };
  }

  private async handleSendMessage(params: any): Promise<any> {
    const { room_id, sender_id, content, message_type = 'text', metadata = {} } = params;
    
    const room = this.rooms.get(room_id);
    if (!room) {
      throw new Error(`Room ${room_id} not found`);
    }

    if (!room.participants.has(sender_id)) {
      throw new Error(`User ${sender_id} is not a participant in room ${room_id}`);
    }

    const messageId = `msg_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    
    const message: ChatMessage = {
      id: messageId,
      room_id,
      sender_id,
      content,
      timestamp: new Date(),
      message_type,
      metadata
    };

    // Store message in history
    const history = this.messageHistory.get(room_id) || [];
    history.push(message);
    this.messageHistory.set(room_id, history);

    this.log(`📩 Message sent in ${room.name}: ${content.substring(0, 50)}...`);

    // TODO: Delegate to DatabaseDaemon for persistence
    // await this.delegateToDatabaseDaemon('save_message', message);

    // TODO: Broadcast to all participants via WebSocketDaemon
    // await this.broadcastToRoom(room_id, message);

    return {
      success: true,
      message_id: messageId,
      message: message
    };
  }

  private async handleGetMessages(params: any): Promise<any> {
    const { room_id, limit = 50, offset = 0 } = params;
    
    const room = this.rooms.get(room_id);
    if (!room) {
      throw new Error(`Room ${room_id} not found`);
    }

    const history = this.messageHistory.get(room_id) || [];
    const messages = history.slice(offset, offset + limit);

    return {
      success: true,
      messages: messages,
      total_count: history.length,
      room_id: room_id
    };
  }

  private async handleListRooms(params: any): Promise<any> {
    const { user_id } = params;
    
    const userRooms = Array.from(this.rooms.values()).filter(room => 
      !user_id || room.participants.has(user_id)
    );

    return {
      success: true,
      rooms: userRooms.map(room => ({
        id: room.id,
        name: room.name,
        type: room.type,
        participant_count: room.participants.size,
        created_at: room.created_at
      }))
    };
  }

  private async handleGetRoomInfo(params: any): Promise<any> {
    const { room_id } = params;
    
    const room = this.rooms.get(room_id);
    if (!room) {
      throw new Error(`Room ${room_id} not found`);
    }

    const participants = this.participants.get(room_id) || new Set();

    return {
      success: true,
      room: room,
      participants: Array.from(participants),
      message_count: (this.messageHistory.get(room_id) || []).length
    };
  }

  private async handleDeleteRoom(params: any): Promise<any> {
    const { room_id, user_id } = params;
    
    const room = this.rooms.get(room_id);
    if (!room) {
      throw new Error(`Room ${room_id} not found`);
    }

    if (room.created_by !== user_id) {
      throw new Error('Only room creator can delete the room');
    }

    this.rooms.delete(room_id);
    this.participants.delete(room_id);
    this.messageHistory.delete(room_id);

    this.log(`🗑️ Room deleted: ${room.name} (${room_id}) by ${user_id}`);

    return {
      success: true,
      deleted_room_id: room_id
    };
  }

  protected async onStart(): Promise<void> {
    this.log('📋 Starting ChatRoom Daemon...');
    
    // TODO: Load existing rooms from DatabaseDaemon
    // const existingRooms = await this.delegateToDatabaseDaemon('load_rooms');
    
    this.log('✅ ChatRoom Daemon ready for chat and room management');
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping ChatRoom Daemon...');
    
    // TODO: Save current state to DatabaseDaemon
    // await this.delegateToDatabaseDaemon('save_all_rooms', Array.from(this.rooms.values()));
    
    this.log('✅ ChatRoom Daemon stopped');
  }
}