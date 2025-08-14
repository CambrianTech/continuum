/**
 * Chat Daemon - Multi-Participant Event-Driven Chat System
 * 
 * Handles chat rooms with multiple citizens (users/agents/personas).
 * Each citizen subscribes to room events and gets targeted notifications
 * for their subscribed rooms only. Integrates with AI APIs for intelligent
 * responses and maintains chat history for context.
 * 
 * ARCHITECTURE:
 * - Room-based chat with participant management
 * - Event-driven real-time updates (citizen gets notified only of their rooms)
 * - Message history and context for AI citizens
 * - Integration with OpenAI/Anthropic APIs
 * - Cross-context support (browser ↔ server citizens)
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import { JTAGMessageTypes } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { generateUUID, type UUID } from '../../../system/core/types/CrossPlatformUUID';
import { EventManager } from '../../../system/events/shared/JTAGEventSystem';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { createBaseResponse } from '../../../system/core/types/ResponseTypes';
import type {
  ChatJoinRoomPayload,
  ChatLeaveRoomPayload,
  ChatSendMessagePayload,
  ChatGetHistoryPayload,
  ChatListRoomsPayload,
  ChatCreateRoomPayload,
  ChatEventData,
  ChatCitizenJoinedEventData,
  ChatCitizenLeftEventData,
  ChatMessageEventData,
  ChatAIResponseEventData
} from './ChatTypes';

/**
 * Chat participant - can be user, agent, or persona
 */
export interface ChatCitizen {
  readonly citizenId: UUID;
  readonly sessionId: UUID;
  readonly displayName: string;
  readonly citizenType: 'user' | 'agent' | 'persona';
  readonly context: JTAGContext;
  
  // Subscription management
  subscribedRooms: Set<UUID>;
  
  // AI Integration
  aiConfig?: {
    provider: 'openai' | 'anthropic' | 'local';
    model?: string;
    apiKey?: string;
    systemPrompt?: string;
  };
  
  // Status
  status: 'active' | 'idle' | 'offline';
  lastSeen: string;
}

/**
 * Chat room with participants and message history
 */
export interface ChatRoom {
  readonly roomId: UUID;
  readonly name: string;
  readonly description?: string;
  readonly category: 'general' | 'support' | 'ai-training' | 'collaboration';
  
  // Participants
  citizens: Map<UUID, ChatCitizen>;
  
  // Message history (for AI context)
  messageHistory: ChatMessage[];
  maxHistoryLength: number;
  
  // Room settings
  allowAI: boolean;
  requireModeration: boolean;
  isPrivate: boolean;
  
  // Timestamps
  createdAt: string;
  lastActivity: string;
}

/**
 * Chat message with context for AI processing
 */
export interface ChatMessage {
  readonly messageId: UUID;
  readonly roomId: UUID;
  readonly senderId: UUID;
  readonly senderName: string;
  readonly senderType: 'user' | 'agent' | 'persona';
  
  readonly content: string;
  readonly timestamp: string;
  
  // Context for AI
  readonly messageType: 'chat' | 'command' | 'system' | 'ai-response';
  readonly replyToId?: UUID;
  readonly mentions: UUID[]; // Citizens mentioned in message
  
  // AI processing
  aiProcessed?: boolean;
  aiContext?: Record<string, unknown>;
}

/**
 * Chat events that citizens subscribe to
 */
export const CHAT_EVENTS = {
  MESSAGE_SENT: 'chat.message.sent',
  CITIZEN_JOINED: 'chat.citizen.joined',
  CITIZEN_LEFT: 'chat.citizen.left',
  ROOM_CREATED: 'chat.room.created',
  HISTORY_UPDATED: 'chat.history.updated',
  AI_RESPONSE: 'chat.ai.response',
} as const;

/**
 * Chat Daemon - handles multi-participant chat with AI integration
 */
export abstract class ChatDaemon extends DaemonBase {
  public readonly subpath: string = 'chat';
  
  // Core state
  protected rooms: Map<UUID, ChatRoom> = new Map();
  protected citizens: Map<UUID, ChatCitizen> = new Map();
  protected eventManager: EventManager = new EventManager();
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super('chat-daemon', context, router);
  }

  /**
   * Handle incoming JTAG messages
   */
  async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    try {
      const endpoint = message.endpoint;
      const parts = endpoint.split('/');
      const operation = parts[parts.length - 1];

      switch (operation) {
        case 'join-room':
          return await this.handleJoinRoom(message);
        case 'leave-room':
          return await this.handleLeaveRoom(message);
        case 'send-message':
          return await this.handleSendMessage(message);
        case 'get-history':
          return await this.handleGetHistory(message);
        case 'list-rooms':
          return await this.handleListRooms(message);
        case 'create-room':
          return await this.handleCreateRoom(message);
        default:
          const correlationId = JTAGMessageTypes.isRequest(message) ? message.correlationId : this.context.uuid;
          return createBaseResponse(
            false,
            this.context,
            correlationId,
            { error: `Unknown chat operation: ${operation}` }
          );
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ ${this.toString()}: Error handling message:`, errorMessage);
      
      const correlationId = JTAGMessageTypes.isRequest(message) ? message.correlationId : this.context.uuid;
      return createBaseResponse(
        false,
        this.context,
        correlationId,
        { error: errorMessage }
      );
    }
  }

  /**
   * Citizen joins a room - gets subscribed to room events
   */
  protected async handleJoinRoom(message: JTAGMessage): Promise<BaseResponsePayload> {
    const payload = message.payload as ChatJoinRoomPayload;
    const { roomId, citizenName, citizenType = 'user', aiConfig } = payload;
    
    // Get or create room
    let room = this.rooms.get(roomId);
    if (!room) {
      room = await this.createDefaultRoom(roomId, `Room ${roomId.substring(0, 8)}`);
    }
    
    // Create or update citizen
    const citizenId = generateUUID();
    const citizen: ChatCitizen = {
      citizenId,
      sessionId: payload.sessionId || generateUUID(),
      displayName: citizenName,
      citizenType,
      context: this.context,
      subscribedRooms: new Set([roomId]),
      aiConfig,
      status: 'active',
      lastSeen: new Date().toISOString()
    };
    
    // Add to room and global citizens
    room.citizens.set(citizenId, citizen);
    this.citizens.set(citizenId, citizen);
    room.lastActivity = new Date().toISOString();
    
    // Notify OTHER citizens in this room (not the joiner)
    await this.notifyCitizensInRoom(roomId, CHAT_EVENTS.CITIZEN_JOINED, {
      roomId,
      citizen: {
        citizenId: citizen.citizenId,
        displayName: citizen.displayName,
        citizenType: citizen.citizenType
      }
    }, new Set([citizenId])); // Exclude the joiner from notification
    
    console.log(`👥 ${this.toString()}: Citizen ${citizenName} joined room ${roomId}`);
    
    const correlationId = JTAGMessageTypes.isRequest(message) ? message.correlationId : this.context.uuid;
    return createBaseResponse(
      true,
      this.context,
      correlationId,
      {
        citizenId,
        roomId,
        roomName: room.name,
        participantCount: room.citizens.size,
        // Send recent history for context
        recentMessages: room.messageHistory.slice(-10)
      }
    );
  }

  /**
   * Citizen leaves room - unsubscribed from room events
   */
  protected async handleLeaveRoom(message: JTAGMessage): Promise<BaseResponsePayload> {
    const payload = message.payload as ChatLeaveRoomPayload;
    const { roomId, citizenId } = payload;
    
    const room = this.rooms.get(roomId);
    const citizen = this.citizens.get(citizenId);
    
    if (!room || !citizen) {
      const correlationId = JTAGMessageTypes.isRequest(message) ? message.correlationId : this.context.uuid;
      return createBaseResponse(
        false,
        this.context,
        correlationId,
        { error: 'Room or citizen not found' }
      );
    }
    
    // Remove from room
    room.citizens.delete(citizenId);
    citizen.subscribedRooms.delete(roomId);
    
    // If citizen not in any rooms, remove globally
    if (citizen.subscribedRooms.size === 0) {
      this.citizens.delete(citizenId);
    }
    
    // Notify other citizens in room
    await this.notifyCitizensInRoom(roomId, CHAT_EVENTS.CITIZEN_LEFT, {
      roomId,
      citizenId,
      displayName: citizen.displayName
    });
    
    console.log(`👋 ${this.toString()}: Citizen ${citizen.displayName} left room ${roomId}`);
    
    const correlationId = JTAGMessageTypes.isRequest(message) ? message.correlationId : this.context.uuid;
    return createBaseResponse(
      true,
      this.context,
      correlationId,
      { roomId, citizenId }
    );
  }

  /**
   * Send message to room - triggers AI responses if enabled
   */
  protected async handleSendMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    const payload = message.payload as ChatSendMessagePayload;
    const { roomId, citizenId, content, messageType = 'chat' } = payload;
    
    const room = this.rooms.get(roomId);
    const citizen = this.citizens.get(citizenId);
    
    if (!room || !citizen) {
      const correlationId = JTAGMessageTypes.isRequest(message) ? message.correlationId : this.context.uuid;
      return createBaseResponse(
        false,
        this.context,
        correlationId,
        { error: 'Room or citizen not found' }
      );
    }
    
    // Create chat message
    const chatMessage: ChatMessage = {
      messageId: generateUUID(),
      roomId,
      senderId: citizenId,
      senderName: citizen.displayName,
      senderType: citizen.citizenType,
      content,
      timestamp: new Date().toISOString(),
      messageType,
      mentions: this.extractMentions(content),
    };
    
    // Add to room history
    room.messageHistory.push(chatMessage);
    if (room.messageHistory.length > room.maxHistoryLength) {
      room.messageHistory.shift(); // Remove oldest message
    }
    room.lastActivity = chatMessage.timestamp;
    
    // Notify ALL citizens in room about new message
    await this.notifyCitizensInRoom(roomId, CHAT_EVENTS.MESSAGE_SENT, {
      message: chatMessage
    });
    
    // Trigger AI responses if room allows AI and there are AI citizens
    if (room.allowAI) {
      await this.triggerAIResponses(room, chatMessage);
    }
    
    console.log(`💬 ${this.toString()}: Message from ${citizen.displayName} in room ${roomId}`);
    
    const correlationId = JTAGMessageTypes.isRequest(message) ? message.correlationId : this.context.uuid;
    return createBaseResponse(
      true,
      this.context,
      correlationId,
      {
        messageId: chatMessage.messageId,
        messageTimestamp: chatMessage.timestamp
      }
    );
  }

  /**
   * Get chat history for a room
   */
  protected async handleGetHistory(message: JTAGMessage): Promise<BaseResponsePayload> {
    const payload = message.payload as ChatGetHistoryPayload;
    const { roomId, limit = 50, before } = payload;
    
    const room = this.rooms.get(roomId);
    if (!room) {
      const correlationId = JTAGMessageTypes.isRequest(message) ? message.correlationId : this.context.uuid;
      return createBaseResponse(
        false,
        this.context,
        correlationId,
        { error: 'Room not found' }
      );
    }
    
    let messages = room.messageHistory;
    
    // Apply pagination
    if (before) {
      const beforeIndex = messages.findIndex(m => m.messageId === before);
      if (beforeIndex > -1) {
        messages = messages.slice(0, beforeIndex);
      }
    }
    
    const limitedMessages = messages.slice(-limit);
    
    const correlationId = JTAGMessageTypes.isRequest(message) ? message.correlationId : this.context.uuid;
    return createBaseResponse(
      true,
      this.context,
      correlationId,
      {
        roomId,
        messages: limitedMessages,
        hasMore: room.messageHistory.length > limit
      }
    );
  }

  /**
   * List available rooms
   */
  protected async handleListRooms(message: JTAGMessage): Promise<BaseResponsePayload> {
    const rooms = Array.from(this.rooms.values()).map(room => ({
      roomId: room.roomId,
      name: room.name,
      description: room.description,
      category: room.category,
      participantCount: room.citizens.size,
      lastActivity: room.lastActivity,
      allowAI: room.allowAI
    }));
    
    const correlationId = JTAGMessageTypes.isRequest(message) ? message.correlationId : this.context.uuid;
    return createBaseResponse(
      true,
      this.context,
      correlationId,
      { rooms }
    );
  }

  /**
   * Create new room
   */
  protected async handleCreateRoom(message: JTAGMessage): Promise<BaseResponsePayload> {
    const payload = message.payload as ChatCreateRoomPayload;
    const { name, description, category = 'general', allowAI = true } = payload;
    
    const room = await this.createDefaultRoom(generateUUID(), name, description, category, allowAI);
    
    // Emit room created event
    this.eventManager.events.emit(CHAT_EVENTS.ROOM_CREATED, {
      roomId: room.roomId,
      name: room.name,
      category: room.category
    });
    
    console.log(`🏠 ${this.toString()}: Created room '${name}'`);
    
    const correlationId = JTAGMessageTypes.isRequest(message) ? message.correlationId : this.context.uuid;
    return createBaseResponse(
      true,
      this.context,
      correlationId,
      {
        roomId: room.roomId,
        name: room.name
      }
    );
  }

  /**
   * Notify all citizens subscribed to a room (with exclusions)
   */
  protected async notifyCitizensInRoom(
    roomId: UUID, 
    eventType: string, 
    eventData: ChatCitizenJoinedEventData | ChatCitizenLeftEventData | ChatMessageEventData | ChatAIResponseEventData, 
    excludeCitizens: Set<UUID> = new Set()
  ): Promise<void> {
    const room = this.rooms.get(roomId);
    if (!room) return;
    
    for (const citizen of room.citizens.values()) {
      if (excludeCitizens.has(citizen.citizenId)) continue;
      
      // Each citizen gets targeted notification for their subscribed room
      await this.notifyCitizen(citizen, eventType, eventData);
    }
  }

  /**
   * Notify individual citizen (context-aware routing)
   */
  protected async notifyCitizen(citizen: ChatCitizen, eventType: string, eventData: ChatCitizenJoinedEventData | ChatCitizenLeftEventData | ChatMessageEventData | ChatAIResponseEventData): Promise<void> {
    try {
      // Route event to citizen's context (browser/server)
      const targetEndpoint = `${citizen.context.environment}/chat-events/${eventType}`;
      
      this.eventManager.events.emit(eventType, {
        targetCitizen: citizen.citizenId,
        ...eventData
      });
      
      console.log(`📢 ${this.toString()}: Notified ${citizen.displayName} of ${eventType}`);
    } catch (error) {
      console.error(`❌ ${this.toString()}: Failed to notify citizen ${citizen.displayName}:`, error);
    }
  }

  /**
   * Trigger AI responses from AI citizens in room
   */
  protected async triggerAIResponses(room: ChatRoom, triggerMessage: ChatMessage): Promise<void> {
    for (const citizen of room.citizens.values()) {
      if (citizen.citizenType === 'agent' || citizen.citizenType === 'persona') {
        if (citizen.aiConfig && citizen.status === 'active') {
          // Don't respond to own messages
          if (citizen.citizenId === triggerMessage.senderId) continue;
          
          // Check if mentioned or should respond
          const shouldRespond = this.shouldAIRespond(citizen, triggerMessage, room);
          if (shouldRespond) {
            await this.generateAIResponse(citizen, room, triggerMessage);
          }
        }
      }
    }
  }

  /**
   * Determine if AI citizen should respond
   */
  protected shouldAIRespond(citizen: ChatCitizen, message: ChatMessage, room: ChatRoom): boolean {
    // Always respond if mentioned
    if (message.mentions.includes(citizen.citizenId)) return true;
    
    // Respond to questions
    if (message.content.includes('?')) return true;
    
    // Respond based on AI config or room activity
    // (This could be more sophisticated based on AI personality, etc.)
    return Math.random() < 0.3; // 30% chance for now
  }

  /**
   * Generate AI response (placeholder - implement with actual AI APIs)
   */
  protected async generateAIResponse(citizen: ChatCitizen, room: ChatRoom, triggerMessage: ChatMessage): Promise<void> {
    try {
      // Get context from recent messages
      const context = room.messageHistory.slice(-5);
      
      // This would integrate with OpenAI/Anthropic/etc APIs
      let response = `I'm ${citizen.displayName}, responding to: "${triggerMessage.content}"`;
      
      if (citizen.aiConfig) {
        response = await this.callAIAPI(citizen.aiConfig, context, triggerMessage);
      }
      
      // Send AI response as new message
      const aiMessage: ChatMessage = {
        messageId: generateUUID(),
        roomId: room.roomId,
        senderId: citizen.citizenId,
        senderName: citizen.displayName,
        senderType: citizen.citizenType,
        content: response,
        timestamp: new Date().toISOString(),
        messageType: 'ai-response',
        replyToId: triggerMessage.messageId,
        mentions: [triggerMessage.senderId], // Reply to original sender
        aiProcessed: true
      };
      
      // Add to room history
      room.messageHistory.push(aiMessage);
      if (room.messageHistory.length > room.maxHistoryLength) {
        room.messageHistory.shift();
      }
      
      // Notify all citizens of AI response
      await this.notifyCitizensInRoom(room.roomId, CHAT_EVENTS.AI_RESPONSE, {
        message: aiMessage,
        triggerMessageId: triggerMessage.messageId
      });
      
      console.log(`🤖 ${this.toString()}: AI response from ${citizen.displayName}`);
      
    } catch (error) {
      console.error(`❌ ${this.toString()}: AI response failed for ${citizen.displayName}:`, error);
    }
  }

  /**
   * Call AI API (OpenAI/Anthropic/etc) - placeholder implementation
   */
  protected async callAIAPI(aiConfig: NonNullable<ChatCitizen['aiConfig']>, context: ChatMessage[], triggerMessage: ChatMessage): Promise<string> {
    // This is where we'd integrate with actual AI APIs
    // For now, return a placeholder response
    
    const contextText = context.map(m => `${m.senderName}: ${m.content}`).join('\n');
    const prompt = `Context:\n${contextText}\n\nRespond to: ${triggerMessage.senderName}: ${triggerMessage.content}`;
    
    // TODO: Implement actual API calls
    switch (aiConfig.provider) {
      case 'openai':
        return `OpenAI response to: "${triggerMessage.content}"`;
      case 'anthropic':
        return `Anthropic response to: "${triggerMessage.content}"`;
      default:
        return `AI response to: "${triggerMessage.content}"`;
    }
  }

  /**
   * Extract @mentions from message content
   */
  protected extractMentions(content: string): UUID[] {
    // Simple mention extraction - could be more sophisticated
    const mentions: UUID[] = [];
    const mentionPattern = /@(\w+)/g;
    let match;
    
    while ((match = mentionPattern.exec(content)) !== null) {
      const mentionName = match[1];
      // Find citizen by display name
      for (const citizen of this.citizens.values()) {
        if (citizen.displayName.toLowerCase() === mentionName.toLowerCase()) {
          mentions.push(citizen.citizenId);
          break;
        }
      }
    }
    
    return mentions;
  }

  /**
   * Create default room
   */
  protected async createDefaultRoom(
    roomId: UUID, 
    name: string, 
    description?: string, 
    category: ChatRoom['category'] = 'general',
    allowAI: boolean = true
  ): Promise<ChatRoom> {
    const room: ChatRoom = {
      roomId,
      name,
      description,
      category,
      citizens: new Map(),
      messageHistory: [],
      maxHistoryLength: 1000,
      allowAI,
      requireModeration: false,
      isPrivate: false,
      createdAt: new Date().toISOString(),
      lastActivity: new Date().toISOString()
    };
    
    this.rooms.set(roomId, room);
    return room;
  }

  /**
   * Get daemon statistics
   */
  public getStats() {
    return {
      roomCount: this.rooms.size,
      citizenCount: this.citizens.size,
      totalMessages: Array.from(this.rooms.values())
        .reduce((sum, room) => sum + room.messageHistory.length, 0),
      activeRooms: Array.from(this.rooms.values())
        .filter(room => room.citizens.size > 0).length,
      aiCitizens: Array.from(this.citizens.values())
        .filter(c => c.citizenType === 'agent' || c.citizenType === 'persona').length
    };
  }
}