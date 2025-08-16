/**
 * Chat Daemon - Multi-Participant Chat Service Only
 * 
 * Microarchitecture approach following SessionDaemon patterns:
 * - Single responsibility: Chat room and message coordination
 * - Strong typing: Factory functions, zero 'any' usage
 * - Sparse override: Minimal shared base, environment-specific overrides
 * - Size: ~100-150 lines maximum per microarchitecture principles
 * 
 * What it does NOT do:
 * - Database persistence → DatabaseDaemon
 * - File operations → FileDaemon  
 * - AI API calls → AIDaemon
 * - WebSocket handling → Router
 * - Browser processes → BrowserDaemon
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGContext, JTAGMessage, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import { createPayload } from '../../../system/core/types/JTAGTypes';
import type { 
  ChatResult,
  ChatParams,
  ChatCreateRoomParams,
  ChatJoinRoomParams,
  ChatSendMessageParams,
  ChatGetHistoryParams,
  ChatListRoomsParams,
  ChatLeaveRoomParams
} from './ChatTypes';
import { 
  createChatCreateRoomResult,
  createChatJoinRoomResult,
  createChatSendMessageResult,
  createChatGetHistoryResult,
  createChatListRoomsResult,
  createChatLeaveRoomResult
} from './ChatTypes';

// Re-export core types for legacy compatibility
export type { ChatCitizen, ChatMessage, ChatRoom } from './ChatTypes';

/**
 * Chat Operation Types - Mirror SessionOperation pattern
 */
export type ChatOperation = 'create' | 'join' | 'leave' | 'message' | 'history' | 'list';

/**
 * Chat Events - Legacy compatibility for browser daemon
 */
export const CHAT_EVENTS = {
  MESSAGE_RECEIVED: 'chat:message-received',
  MESSAGE_SENT: 'chat:message-sent',
  CITIZEN_JOINED: 'chat:citizen-joined', 
  CITIZEN_LEFT: 'chat:citizen-left',
  AI_RESPONSE: 'chat:ai-response',
  ROOM_UPDATED: 'chat:room-updated',
  PARTICIPANT_UPDATE: 'chat:participant-update'
} as const;

/**
 * Chat Error Response Factory - Following SessionDaemon pattern
 */
export const createChatErrorResponse = (
  error: string,
  context: JTAGContext,
  sessionId: UUID,
  operation?: ChatOperation
): ChatResult => {
  // Return a basic error result that matches one of the ChatResult union types
  return createPayload(context, sessionId, {
    success: false,
    timestamp: new Date().toISOString(),
    error: error,
    rooms: [],
    totalCount: 0
  }) as ChatResult;
};

/**
 * Chat Daemon - Chat Coordination Service Only
 * 
 * Core responsibility: Coordinate chat rooms and message flow
 * Does NOT handle persistence, files, WebSockets, AI, etc.
 */
export abstract class ChatDaemon extends DaemonBase {
  public readonly subpath: string = 'chat';
  
  // Optional eventManager for legacy compatibility
  protected eventManager?: any;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('chat-daemon', context, router);
  }

  /**
   * Initialize chat daemon
   */
  protected async initialize(): Promise<void> {
    console.log(`💬 ${this.toString()}: Chat daemon initialized - coordination service ready`);
  }

  /**
   * Handle incoming chat messages - base implementation forwards to server
   * ChatDaemonServer overrides this to provide actual chat coordination
   */
  async handleMessage(message: JTAGMessage): Promise<ChatResult> {
    console.log(`📨 ${this.toString()}: Forwarding chat message to server`);
    
    // Base implementation creates forwarding response
    // Server implementation overrides with actual chat logic  
    return createChatErrorResponse(
      'Chat daemon base implementation - server override required',
      message.payload.context,
      message.payload.sessionId
    );
  }

  /**
   * Extract chat operation from message (following extractSessionOperation pattern)
   */
  protected extractChatOperation(message: JTAGMessage): ChatOperation | null {
    // Extract from endpoint: 'chat/create', 'chat/join', etc.
    const endpoint = message.endpoint;
    const parts = endpoint.split('/');
    
    if (parts.length >= 2 && parts[0] === 'chat') {
      const operation = parts[1] as ChatOperation;
      if (['create', 'join', 'leave', 'message', 'history', 'list'].includes(operation)) {
        return operation;
      }
    }
    
    return null;
  }

  /**
   * Route to specific operation handler - following SessionDaemon pattern
   */
  protected async routeChatOperation(operation: ChatOperation, params: JTAGPayload): Promise<ChatResult> {
    switch (operation) {
      case 'create':
        return await this.handleCreateRoom(params as ChatCreateRoomParams);
      case 'join':  
        return await this.handleJoinRoom(params as ChatJoinRoomParams);
      case 'leave':
        return await this.handleLeaveRoom(params as ChatLeaveRoomParams);
      case 'message':
        return await this.handleSendMessage(params as ChatSendMessageParams);
      case 'history':
        return await this.handleGetHistory(params as ChatGetHistoryParams);
      case 'list':
        return await this.handleListRooms(params as ChatListRoomsParams);
      default:
        return createChatErrorResponse(
          `Unknown chat operation: ${operation}`,
          params.context,
          params.sessionId,
          operation
        );
    }
  }

  // ========================================================================
  // OPERATION HANDLERS - Environment-specific implementation (optional override)
  // ========================================================================

  protected async handleCreateRoom(params: ChatCreateRoomParams): Promise<ChatResult> {
    return createChatErrorResponse('Not implemented', params.context, params.sessionId, 'create');
  }
  
  protected async handleJoinRoom(params: ChatJoinRoomParams): Promise<ChatResult> {
    return createChatErrorResponse('Not implemented', params.context, params.sessionId, 'join');
  }
  
  protected async handleLeaveRoom(params: ChatLeaveRoomParams): Promise<ChatResult> {
    return createChatErrorResponse('Not implemented', params.context, params.sessionId, 'leave');
  }
  
  protected async handleSendMessage(params: ChatSendMessageParams): Promise<ChatResult> {
    return createChatErrorResponse('Not implemented', params.context, params.sessionId, 'message');
  }
  
  protected async handleGetHistory(params: ChatGetHistoryParams): Promise<ChatResult> {
    return createChatErrorResponse('Not implemented', params.context, params.sessionId, 'history');
  }
  
  protected async handleListRooms(params: ChatListRoomsParams): Promise<ChatResult> {
    return createChatErrorResponse('Not implemented', params.context, params.sessionId, 'list');
  }

  // ========================================================================
  // UTILITY METHODS - ID generation and string representation
  // ========================================================================

  /**
   * Generate unique IDs - utility methods
   */
  protected generateRoomId(): UUID {
    return generateUUID();
  }

  protected generateCitizenId(): UUID {
    return generateUUID();
  }

  protected generateMessageId(): UUID {
    return generateUUID();
  }

  /**
   * Daemon identification string
   */
  toString(): string {
    return `ChatDaemon[${this.context.environment}:${this.context.uuid.slice(0, 8)}]`;
  }
}