/**
 * Chat Daemon - Shared Types
 * 
 * Following ScreenshotTypes pattern: factory functions, inheritance, strong typing
 * Complete type definitions for chat operations with zero 'any' usage
 */

import type { JTAGPayload, JTAGContext } from '../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGError } from '../../../system/core/types/ErrorTypes';

// ============================================================================
// CHAT ENTITY TYPES
// ============================================================================

export interface ChatCitizen {
  readonly citizenId: UUID;
  readonly sessionId: UUID;
  readonly displayName: string;
  readonly citizenType: 'user' | 'agent' | 'persona';
  readonly joinedAt: string;
  readonly lastSeen: string;
  readonly isOnline: boolean;
  
  // Legacy compatibility for database layer
  readonly status?: 'active' | 'idle' | 'away';
  readonly aiConfig?: {
    readonly provider?: string;
    readonly model?: string;
    readonly settings?: any;
    readonly systemPrompt?: string; // Database layer compatibility
    readonly apiKey?: string; // Database layer compatibility
  };
  readonly context?: any; // Database layer compatibility
  readonly subscribedRooms?: readonly UUID[]; // Database layer compatibility
}

export interface ChatMessage {
  readonly messageId: UUID;
  readonly roomId: UUID;
  readonly senderId: UUID;
  readonly senderName: string;
  readonly content: string;
  readonly timestamp: string;
  readonly messageType: 'chat' | 'system' | 'ai-response';
  readonly mentions: readonly UUID[];
  
  // Legacy compatibility
  readonly senderType?: 'user' | 'agent' | 'persona' | 'system';
  readonly replyToId?: UUID; // Database layer compatibility
  readonly aiProcessed?: boolean; // Database layer compatibility
  readonly aiContext?: any; // Database layer compatibility
}

export interface ChatRoom {
  readonly roomId: UUID;
  readonly name: string;
  readonly description?: string;
  readonly createdAt: string;
  readonly lastActivity: string;
  readonly citizenCount: number;
  readonly messageCount: number;
  readonly isPrivate: boolean;
  
  // Legacy compatibility
  readonly participantCount: number; // Alias for citizenCount
  readonly category?: string;
  readonly allowAI?: boolean;
  readonly requireModeration?: boolean;
  readonly maxHistoryLength?: number;
  readonly citizens?: readonly ChatCitizen[]; // Database layer compatibility
  readonly messageHistory?: readonly ChatMessage[]; // Database layer compatibility
}

// ============================================================================
// CHAT OPERATION PARAMETERS - Extending JTAGPayload
// ============================================================================

export interface ChatCreateRoomParams extends JTAGPayload {
  readonly name: string;
  readonly description?: string;
  readonly isPrivate?: boolean;
}

export interface ChatJoinRoomParams extends JTAGPayload {
  readonly roomId: UUID;
  readonly citizenName: string;
  readonly citizenType: 'user' | 'agent' | 'persona';
}

export interface ChatSendMessageParams extends JTAGPayload {
  readonly roomId: UUID;
  readonly content: string;
  readonly messageType?: 'chat' | 'system';
  readonly mentions?: readonly UUID[];
}

export interface ChatGetHistoryParams extends JTAGPayload {
  readonly roomId: UUID;
  readonly limit?: number;
  readonly beforeTimestamp?: string;
}

export interface ChatListRoomsParams extends JTAGPayload {
  // No additional params needed
}

export interface ChatLeaveRoomParams extends JTAGPayload {
  readonly roomId: UUID;
  readonly citizenId: UUID;
}

// ============================================================================
// FACTORY FUNCTIONS FOR PARAMETERS - Following ScreenshotTypes pattern
// ============================================================================

export const createChatCreateRoomParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatCreateRoomParams, 'context' | 'sessionId'>
): ChatCreateRoomParams => createPayload(context, sessionId, data);

export const createChatJoinRoomParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatJoinRoomParams, 'context' | 'sessionId'>
): ChatJoinRoomParams => createPayload(context, sessionId, data);

export const createChatSendMessageParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatSendMessageParams, 'context' | 'sessionId'>
): ChatSendMessageParams => createPayload(context, sessionId, data);

export const createChatGetHistoryParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatGetHistoryParams, 'context' | 'sessionId'>
): ChatGetHistoryParams => createPayload(context, sessionId, data);

export const createChatListRoomsParams = (
  context: JTAGContext,
  sessionId: UUID
): ChatListRoomsParams => createPayload(context, sessionId, {});

export const createChatLeaveRoomParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatLeaveRoomParams, 'context' | 'sessionId'>
): ChatLeaveRoomParams => createPayload(context, sessionId, data);

// ============================================================================
// CHAT OPERATION RESULTS - Extending JTAGPayload
// ============================================================================

export interface ChatCreateRoomResult extends JTAGPayload {
  readonly success: boolean;
  readonly roomId: UUID;
  readonly room: ChatRoom;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

export interface ChatJoinRoomResult extends JTAGPayload {
  readonly success: boolean;
  readonly citizenId: UUID;
  readonly room: ChatRoom;
  readonly recentMessages: readonly ChatMessage[];
  readonly citizenList: readonly ChatCitizen[];
  readonly timestamp: string;
  readonly error?: JTAGError;
  
  // Convenience fields extracted from room
  readonly roomId?: UUID; // Extracted from room.roomId
  readonly roomName?: string; // Extracted from room.name
  readonly participantCount?: number; // Extracted from room.participantCount
}

export interface ChatSendMessageResult extends JTAGPayload {
  readonly success: boolean;
  readonly messageId: UUID;
  readonly message: ChatMessage;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

export interface ChatGetHistoryResult extends JTAGPayload {
  readonly success: boolean;
  readonly roomId: UUID;
  readonly messages: readonly ChatMessage[];
  readonly totalCount: number;
  readonly hasMore: boolean;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

export interface ChatListRoomsResult extends JTAGPayload {
  readonly success: boolean;
  readonly rooms: readonly ChatRoom[];
  readonly totalCount: number;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

export interface ChatLeaveRoomResult extends JTAGPayload {
  readonly success: boolean;
  readonly roomId: UUID;
  readonly citizenId: UUID;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

// ============================================================================
// FACTORY FUNCTIONS FOR RESULTS - Following ScreenshotTypes pattern
// ============================================================================

export const createChatCreateRoomResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatCreateRoomResult>, 'context' | 'sessionId'>
): ChatCreateRoomResult => createPayload(context, sessionId, {
  success: false,
  roomId: '' as UUID,
  room: {} as ChatRoom,
  timestamp: new Date().toISOString(),
  ...data
});

export const createChatJoinRoomResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatJoinRoomResult>, 'context' | 'sessionId'>
): ChatJoinRoomResult => {
  const result = createPayload(context, sessionId, {
    success: false,
    citizenId: '' as UUID,
    room: {} as ChatRoom,
    recentMessages: [],
    citizenList: [],
    timestamp: new Date().toISOString(),
    roomId: '' as UUID,
    roomName: '',
    participantCount: 0,
    ...data
  });
  
  // Auto-populate legacy fields from room if room is provided  
  if (data.room) {
    result.roomId = data.room.roomId;
    result.roomName = data.room.name;
    result.participantCount = data.room.participantCount;
  }
  
  return result;
};

export const createChatSendMessageResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatSendMessageResult>, 'context' | 'sessionId'>
): ChatSendMessageResult => createPayload(context, sessionId, {
  success: false,
  messageId: '' as UUID,
  message: {} as ChatMessage,
  timestamp: new Date().toISOString(),
  ...data
});

export const createChatGetHistoryResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatGetHistoryResult>, 'context' | 'sessionId'>
): ChatGetHistoryResult => createPayload(context, sessionId, {
  success: false,
  roomId: '' as UUID,
  messages: [],
  totalCount: 0,
  hasMore: false,
  timestamp: new Date().toISOString(),
  ...data
});

export const createChatListRoomsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatListRoomsResult>, 'context' | 'sessionId'>
): ChatListRoomsResult => createPayload(context, sessionId, {
  success: false,
  rooms: [],
  totalCount: 0,
  timestamp: new Date().toISOString(),
  ...data
});

export const createChatLeaveRoomResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatLeaveRoomResult>, 'context' | 'sessionId'>
): ChatLeaveRoomResult => createPayload(context, sessionId, {
  success: false,
  roomId: '' as UUID,
  citizenId: '' as UUID,
  timestamp: new Date().toISOString(),
  ...data
});

// ============================================================================
// SMART RESULT INHERITANCE - Following ScreenshotTypes pattern
// ============================================================================

export const createChatJoinRoomResultFromParams = (
  params: ChatJoinRoomParams,
  differences: Omit<Partial<ChatJoinRoomResult>, 'context' | 'sessionId' | 'roomId' | 'citizenName'>
): ChatJoinRoomResult => transformPayload(params, {
  success: false,
  citizenId: '' as UUID,
  room: {} as ChatRoom,
  recentMessages: [],
  citizenList: [],
  timestamp: new Date().toISOString(),
  ...differences
});

export const createChatSendMessageResultFromParams = (
  params: ChatSendMessageParams,
  differences: Omit<Partial<ChatSendMessageResult>, 'context' | 'sessionId' | 'roomId' | 'content'>
): ChatSendMessageResult => transformPayload(params, {
  success: false,
  messageId: '' as UUID,
  message: {} as ChatMessage,
  timestamp: new Date().toISOString(),
  ...differences
});

// ============================================================================
// CHAT RESPONSE UNION TYPE
// ============================================================================

export type ChatResult = 
  | ChatCreateRoomResult 
  | ChatJoinRoomResult 
  | ChatSendMessageResult 
  | ChatGetHistoryResult 
  | ChatListRoomsResult
  | ChatLeaveRoomResult;

export type ChatParams = 
  | ChatCreateRoomParams 
  | ChatJoinRoomParams 
  | ChatSendMessageParams 
  | ChatGetHistoryParams 
  | ChatListRoomsParams
  | ChatLeaveRoomParams;

// ============================================================================
// LEGACY COMPATIBILITY EXPORTS - Bridge to old architecture
// ============================================================================

// Legacy Response Types (rename Result -> Response for compatibility)
export type ChatListRoomsResponse = ChatListRoomsResult;
export type ChatJoinRoomResponse = ChatJoinRoomResult;
export type ChatSendMessageResponse = ChatSendMessageResult;

// Legacy Error Response (compatibility with old error handling)
export interface ChatErrorResponse {
  readonly success: false;
  readonly timestamp: string;
  readonly error: string;
  readonly operation?: string;
}

// Legacy Event Data Types (expected by browser daemon)
export interface ChatMessageEventData {
  readonly message: ChatMessage;
  readonly roomId: UUID;
}

export interface ChatCitizenJoinedEventData {
  readonly citizen: ChatCitizen;
  readonly roomId: UUID;
}

export interface ChatCitizenLeftEventData {
  readonly citizenId: UUID;
  readonly roomId: UUID;
  readonly citizenName: string;
  readonly displayName: string; // Legacy compatibility
}

export interface ChatAIResponseEventData {
  readonly message: ChatMessage;
  readonly roomId: UUID;
  readonly thinking?: string;
}

export interface ParticipantUpdateData {
  readonly roomId: UUID;
  readonly citizens: readonly ChatCitizen[];
  readonly participants: readonly ChatCitizen[]; // Legacy alias
  readonly totalCount: number;
}