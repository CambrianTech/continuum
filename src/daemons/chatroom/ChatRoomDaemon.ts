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

import { RequestResponseDaemon, RequestHandlerMap } from '../base/RequestResponseDaemon';
import { DaemonType } from '../base/DaemonTypes';
import {
  ChatRoom,
  ChatMessage,
  ChatParticipant,
  ChatRoomType,
  MessageType,
  ParticipantRole,
  ParticipantStatus
} from '../../types/shared/chat/ChatTypes';
import { CommandOperation, getChatRoomOperations } from '../../types/shared/CommandOperationTypes';
import { loadDefaultRoomsConfig } from './DefaultRoomsConfig';

export class ChatRoomDaemon extends RequestResponseDaemon {
  private rooms: Map<string, ChatRoom> = new Map();
  private participants: Map<string, Set<ChatParticipant>> = new Map();
  private messageHistory: Map<string, ChatMessage[]> = new Map();

  public readonly name = 'chatroom';
  public readonly version = '1.0.0';
  public readonly daemonType = DaemonType.CHATROOM;

  constructor() {
    super();
  }

  getRequestHandlers(): RequestHandlerMap {
    return this.defineRequestHandlers();
  }

  protected defineRequestHandlers(): RequestHandlerMap {
    // Create handler map by iterating over ChatRoom operations from the single CommandOperation enum
    const handlers: RequestHandlerMap = {};
    
    // Map each ChatRoom operation to its corresponding handler method
    const methodMap: Record<string, any> = {
      [CommandOperation.CREATE_ROOM]: this.handleCreateRoom.bind(this),
      [CommandOperation.JOIN_ROOM]: this.handleJoinRoom.bind(this),
      [CommandOperation.LEAVE_ROOM]: this.handleLeaveRoom.bind(this),
      [CommandOperation.SEND_MESSAGE]: this.handleSendMessage.bind(this),
      [CommandOperation.GET_MESSAGES]: this.handleGetMessages.bind(this),
      [CommandOperation.LIST_ROOMS]: this.handleListRooms.bind(this),
      [CommandOperation.GET_ROOM_INFO]: this.handleGetRoomInfo.bind(this),
      [CommandOperation.DELETE_ROOM]: this.handleDeleteRoom.bind(this)
    };
    
    // Dynamically register all ChatRoom operations using the helper function
    for (const operation of getChatRoomOperations()) {
      const handler = methodMap[operation];
      if (handler) {
        handlers[operation] = handler;
      } else {
        this.log(`⚠️ No handler found for ChatRoom operation: ${operation}`, 'warn');
      }
    }
    
    this.log(`🔧 Registered ${Object.keys(handlers).length} ChatRoom operation handlers from single CommandOperation enum`);
    return handlers;
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
      role: ParticipantRole.OWNER,
      status: ParticipantStatus.ONLINE,
      last_seen: new Date(),
      metadata: {}
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
      role: ParticipantRole.MEMBER,
      status: ParticipantStatus.ONLINE,
      last_seen: new Date(),
      metadata: {}
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
    const { room_id, sender_id, content, message_type = MessageType.TEXT, metadata = {} } = params;
    
    const room = this.rooms.get(room_id);
    if (!room) {
      throw new Error(`Room ${room_id} not found`);
    }

    // Auto-join user to room if not already a participant (smart daemon logic)
    if (!room.participants.has(sender_id)) {
      // Check if this is a default/public room that allows auto-joining
      const isAutoJoinRoom = room.metadata?.default === true || 
                            room.metadata?.autoCreated === true ||
                            room.metadata?.category === 'public';
      
      if (isAutoJoinRoom) {
        this.log(`🔧 Auto-joining user ${sender_id} to ${room.name} (${room_id}) for message sending`);
        
        try {
          // Auto-join the user to the room
          await this.handleJoinRoom({ 
            room_id, 
            user_id: sender_id,
            session_id: params.session_id || 'auto-join'
          });
          this.log(`✅ Auto-joined user ${sender_id} to room ${room.name}`);
        } catch (joinError) {
          this.log(`❌ Failed to auto-join user ${sender_id} to room ${room.name}: ${joinError}`);
          throw new Error(`User ${sender_id} is not a participant in room ${room_id} and auto-join failed`);
        }
      } else {
        // Private/restricted room - require explicit join
        throw new Error(`User ${sender_id} is not a participant in room ${room_id}. Private rooms require explicit joining.`);
      }
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
    
    // Register to listen for chatroom requests
    const { DAEMON_EVENT_BUS } = await import('../base/DaemonEventBus');
    DAEMON_EVENT_BUS.on('chatroom_request', this.handleChatRoomRequest.bind(this));
    this.log('🔌 Registered for chatroom_request events');
    
    // Create default rooms for immediate use
    await this.createDefaultRooms();
    
    // TODO: Load existing rooms from DatabaseDaemon
    // const existingRooms = await this.delegateToDatabaseDaemon('load_rooms');
    
    this.log('✅ ChatRoom Daemon ready for chat and room management');
  }

  /**
   * Create default rooms that are always available
   * Loads room specifications from JSON configuration for easy editing
   */
  private async createDefaultRooms(): Promise<void> {
    try {
      const defaultRoomSpecs = await loadDefaultRoomsConfig();
      this.log(`🏠 Loading ${defaultRoomSpecs.length} default rooms from configuration`);

      for (const roomSpec of defaultRoomSpecs) {
        try {
          const room: ChatRoom = {
            id: roomSpec.id,
            name: roomSpec.name,
            type: roomSpec.type as ChatRoomType,
            participants: new Set(['system']),
            created_at: new Date(),
            created_by: 'system',
            metadata: { 
              default: true, 
              autoCreated: roomSpec.autoCreated || true,
              description: roomSpec.description,
              ...roomSpec.metadata 
            }
          };

          this.rooms.set(roomSpec.id, room);
          this.participants.set(roomSpec.id, new Set([{
            user_id: 'system',
            session_id: 'system',
            joined_at: new Date(),
            role: ParticipantRole.OWNER,
            status: ParticipantStatus.ONLINE,
            last_seen: new Date(),
            metadata: {}
          }]));
          this.messageHistory.set(roomSpec.id, []);

          this.log(`🏠 Created default room: ${roomSpec.name} (${roomSpec.id})`);
        } catch (error) {
          const errorMessage = error instanceof Error ? error.message : String(error);
          this.log(`❌ Failed to create default room ${roomSpec.id}: ${errorMessage}`, 'error');
        }
      }

      this.log(`🏠 Created ${defaultRoomSpecs.length} default chat rooms from JSON configuration`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to load default rooms configuration: ${errorMessage}`, 'error');
      
      // Create minimal fallback room if configuration loading fails
      const fallbackRoom: ChatRoom = {
        id: 'general',
        name: 'General Chat',
        type: ChatRoomType.CHAT,
        participants: new Set(['system']),
        created_at: new Date(),
        created_by: 'system',
        metadata: { default: true, autoCreated: true, fallback: true }
      };
      
      this.rooms.set('general', fallbackRoom);
      this.participants.set('general', new Set([{
        user_id: 'system',
        session_id: 'system',
        joined_at: new Date(),
        role: ParticipantRole.OWNER,
        status: ParticipantStatus.ONLINE,
        last_seen: new Date(),
        metadata: {}
      }]));
      this.messageHistory.set('general', []);
      
      this.log('🏠 Created fallback general room due to configuration error');
    }
  }

  /**
   * Handle chatroom requests from the event bus
   */
  private async handleChatRoomRequest(request: any): Promise<void> {
    try {
      this.log(`📩 Received chatroom request: ${request.operation} (${request.correlationId})`);
      
      const handlers = this.getRequestHandlers();
      const handler = handlers[request.operation];
      
      if (!handler) {
        const availableOps = Object.keys(handlers).join(', ');
        const errorResponse = {
          correlationId: request.correlationId,
          success: false,
          error: `Unknown operation: ${request.operation}. Available: ${availableOps}`,
          timestamp: Date.now()
        };
        
        const { DAEMON_EVENT_BUS } = await import('../base/DaemonEventBus');
        DAEMON_EVENT_BUS.emit('chatroom_response', errorResponse);
        return;
      }

      // Execute the handler with the request data
      const result = await handler(request.data);
      
      // Format response with correlation ID and timestamp
      const response = {
        correlationId: request.correlationId,
        success: result.success || true,
        data: result.success === false ? undefined : result,
        error: result.success === false ? result.error : undefined,
        timestamp: Date.now()
      };
      
      this.log(`📤 Sending chatroom response: ${request.operation} (${request.correlationId})`);
      
      // Emit response back to the event bus
      const { DAEMON_EVENT_BUS } = await import('../base/DaemonEventBus');
      DAEMON_EVENT_BUS.emit('chatroom_response', response);
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Error handling chatroom request: ${errorMessage}`, 'error');
      
      // Send error response
      const errorResponse = {
        correlationId: request.correlationId,
        success: false,
        error: errorMessage,
        timestamp: Date.now()
      };
      
      const { DAEMON_EVENT_BUS } = await import('../base/DaemonEventBus');
      DAEMON_EVENT_BUS.emit('chatroom_response', errorResponse);
    }
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping ChatRoom Daemon...');
    
    // Unregister from event bus
    const { DAEMON_EVENT_BUS } = await import('../base/DaemonEventBus');
    DAEMON_EVENT_BUS.off('chatroom_request', this.handleChatRoomRequest.bind(this));
    this.log('🔌 Unregistered from chatroom_request events');
    
    // TODO: Save current state to DatabaseDaemon
    // await this.delegateToDatabaseDaemon('save_all_rooms', Array.from(this.rooms.values()));
    
    this.log('✅ ChatRoom Daemon stopped');
  }
}

// Main entry point for standalone execution
if (import.meta.url === `file://${process.argv[1]}`) {
  const daemon = new ChatRoomDaemon();
  
  // Handle graceful shutdown
  process.on('SIGTERM', async () => {
    console.log('Received SIGTERM, shutting down gracefully...');
    await daemon.stop();
    process.exit(0);
  });
  
  process.on('SIGINT', async () => {
    console.log('Received SIGINT, shutting down gracefully...');
    await daemon.stop();
    process.exit(0);
  });
  
  // Start the daemon
  daemon.start().catch(error => {
    console.error('Failed to start ChatRoom Daemon:', error);
    process.exit(1);
  });
}