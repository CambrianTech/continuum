/**
 * Chat System Strong Type Definitions
 * 
 * Comprehensive type definitions for the chat system to eliminate any usage.
 * Provides type safety for all chat operations, payloads, and event data.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGPayload } from '../../../system/core/types/JTAGTypes';
import type { ChatCitizen, ChatMessage, ChatRoom } from './ChatDaemon';

/**
 * Chat Command Payloads - Strongly typed request data
 */
export interface ChatJoinRoomPayload extends JTAGPayload {
  readonly roomId: UUID;
  readonly citizenName: string;
  readonly citizenType: 'user' | 'agent' | 'persona';
  readonly aiConfig?: {
    provider: 'openai' | 'anthropic' | 'local';
    model?: string;
    apiKey?: string;
    systemPrompt?: string;
  };
}

export interface ChatLeaveRoomPayload extends JTAGPayload {
  readonly roomId: UUID;
  readonly citizenId: UUID;
}

export interface ChatSendMessagePayload extends JTAGPayload {
  readonly roomId: UUID;
  readonly citizenId: UUID;
  readonly content: string;
  readonly messageType?: 'chat' | 'command' | 'system';
}

export interface ChatGetHistoryPayload extends JTAGPayload {
  readonly roomId: UUID;
  readonly limit?: number;
  readonly before?: UUID;
}

export interface ChatListRoomsPayload extends JTAGPayload {
  // No additional fields needed - just the base JTAGPayload
}

export interface ChatCreateRoomPayload extends JTAGPayload {
  readonly name: string;
  readonly description?: string;
  readonly category?: 'general' | 'support' | 'ai-training' | 'collaboration';
  readonly allowAI?: boolean;
}

/**
 * Chat Event Data - Strongly typed event payloads
 */
export interface ChatMessageEventData {
  readonly message: ChatMessage;
}

export interface ChatCitizenJoinedEventData {
  readonly roomId: UUID;
  readonly citizen: {
    readonly citizenId: UUID;
    readonly displayName: string;
    readonly citizenType: 'user' | 'agent' | 'persona';
  };
}

export interface ChatCitizenLeftEventData {
  readonly roomId: UUID;
  readonly citizenId: UUID;
  readonly displayName: string;
  readonly reason?: 'manual' | 'inactive' | 'error';
}

export interface ChatRoomCreatedEventData {
  readonly roomId: UUID;
  readonly name: string;
  readonly category: ChatRoom['category'];
}

export interface ChatHistoryUpdatedEventData {
  readonly roomId: UUID;
  readonly messageCount: number;
  readonly lastActivity: string;
}

export interface ChatAIResponseEventData {
  readonly message: ChatMessage;
  readonly triggerMessageId: UUID;
}

/**
 * Chat Response Data - Strongly typed response payloads
 */
export interface ChatJoinRoomResponse {
  readonly success: true;
  readonly citizenId: UUID;
  readonly roomId: UUID;
  readonly roomName: string;
  readonly participantCount: number;
  readonly recentMessages: readonly ChatMessage[];
}

export interface ChatLeaveRoomResponse {
  readonly success: true;
  readonly roomId: UUID;
  readonly citizenId: UUID;
}

export interface ChatSendMessageResponse {
  readonly success: true;
  readonly messageId: UUID;
  readonly messageTimestamp: string;
}

export interface ChatGetHistoryResponse {
  readonly success: true;
  readonly roomId: UUID;
  readonly messages: readonly ChatMessage[];
  readonly hasMore: boolean;
}

export interface ChatListRoomsResponse {
  readonly success: true;
  readonly rooms: readonly {
    readonly roomId: UUID;
    readonly name: string;
    readonly description?: string;
    readonly category: ChatRoom['category'];
    readonly participantCount: number;
    readonly lastActivity: string;
    readonly allowAI: boolean;
  }[];
}

export interface ChatCreateRoomResponse {
  readonly success: true;
  readonly roomId: UUID;
  readonly name: string;
}

/**
 * Error Response Types
 */
export interface ChatErrorResponse {
  readonly success: false;
  readonly error: string;
}

/**
 * Union types for all possible responses
 */
export type ChatResponse = 
  | ChatJoinRoomResponse
  | ChatLeaveRoomResponse  
  | ChatSendMessageResponse
  | ChatGetHistoryResponse
  | ChatListRoomsResponse
  | ChatCreateRoomResponse
  | ChatErrorResponse;

/**
 * Type guard functions for response types
 */
export const ChatResponseTypes = {
  isSuccess: <T extends ChatResponse>(response: T): response is Extract<T, { success: true }> => {
    return response.success === true;
  },
  
  isError: (response: ChatResponse): response is ChatErrorResponse => {
    return response.success === false;
  },
  
  isJoinRoomResponse: (response: ChatResponse): response is ChatJoinRoomResponse => {
    return response.success === true && 'citizenId' in response && 'roomName' in response;
  },
  
  isListRoomsResponse: (response: ChatResponse): response is ChatListRoomsResponse => {
    return response.success === true && 'rooms' in response;
  }
};

/**
 * Event data union type with discriminated union
 */
export type ChatEventData = 
  | { type: 'MESSAGE_SENT'; data: ChatMessageEventData }
  | { type: 'CITIZEN_JOINED'; data: ChatCitizenJoinedEventData }
  | { type: 'CITIZEN_LEFT'; data: ChatCitizenLeftEventData }
  | { type: 'ROOM_CREATED'; data: ChatRoomCreatedEventData }
  | { type: 'HISTORY_UPDATED'; data: ChatHistoryUpdatedEventData }
  | { type: 'AI_RESPONSE'; data: ChatAIResponseEventData };

/**
 * Participant list update data for browser UI
 */
export interface ParticipantUpdateData {
  readonly roomId: UUID;
  readonly participants: readonly {
    readonly citizenId: UUID;
    readonly displayName: string;
    readonly citizenType: 'user' | 'agent' | 'persona';
    readonly status: 'active' | 'idle' | 'offline';
  }[];
}